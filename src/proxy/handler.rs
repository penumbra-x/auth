use super::mitm::RequestOrResponse;
use http::{header, HeaderMap, Method, Request, Response, StatusCode, Uri};
pub use hyper;
use hyper::{body, Body};
use moka::sync::Cache;
use rand::seq::IteratorRandom;
use reqwest::{Client, Error, Url};
use serde::{Deserialize, Serialize};
use std::{future::Future, time::Duration};
use tokio::task::JoinHandle;
use tracing::{Instrument, Span};

#[derive(Clone)]
pub struct DeviceCheckHandler {
    client: Client,
    cache: Cache<String, String>,
}

impl DeviceCheckHandler {
    pub fn new(proxy: Option<Url>) -> Result<Self, Error> {
        Ok(DeviceCheckHandler {
            client: Client::builder()
                .proxy(reqwest::Proxy::custom(move |_| {
                    proxy.as_ref().cloned().map_or(None, Some)
                }))
                .build()?,
            cache: Cache::builder()
                .max_capacity(u64::MAX)
                .time_to_live(Duration::from_secs(3600 * 24 * 7))
                .build(),
        })
    }

    pub fn get_cookie_res(&self) -> Result<Response<Body>, crate::error::Error> {
        let preauth_cookie = PreAuthCookie {
            preauth_cookie: self
                .cache
                .iter()
                .choose(&mut rand::thread_rng())
                .map(|(_, cookie)| cookie),
        };

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_string_pretty(&preauth_cookie)?))
            .map_err(Into::into)
    }

    async fn hook_request(&self, req: Request<Body>) -> RequestOrResponse {
        let (parts, body) = req.into_parts();
        match body::to_bytes(body)
            .await
            .map(|bytes| serde_json::from_slice::<DeviceCheckBody>(&bytes).ok())
        {
            Ok(None) => {
                tracing::error!("parse preauth_devicecheck request error")
            }
            Err(err) => {
                tracing::error!("invalid preauth_devicecheck request: {}", err)
            }
            Ok(Some(body)) => {
                // Build request
                let req = DeviceCheckRequest {
                    uri: parts.uri,
                    method: parts.method,
                    headers: parts.headers,
                    body,
                };

                // Spwan background fetch task
                spawn_with_trace(
                    self.clone().fetch_preauth_cookie(req),
                    tracing::info_span!("preauth_devicecheck"),
                );
            }
        }

        // Hook return invalid request
        RequestOrResponse::Response(Response::new(Body::empty()))
    }

    async fn fetch_preauth_cookie(self, mut req: DeviceCheckRequest) {
        tracing::info!("preauth_devicecheck request: {req:#?}");

        let device_id = req.body.device_id.clone();

        let resp = async {
            tracing::info!("send preauth_devicecheck request..");

            req.headers.remove(header::CONTENT_LENGTH);
            req.headers.remove(header::ACCEPT_ENCODING);

            let resp = self
                .client
                .request(req.method, req.uri.to_string())
                .headers(req.headers)
                .json(&req.body)
                .send()
                .await?;

            Ok::<_, reqwest::Error>(resp)
        };

        match resp.await {
            Ok(resp) => {
                if let Some(cookie) = resp
                    .cookies()
                    .find(|c| c.name().eq("_preauth_devicecheck"))
                    .map(|c| c.value().to_owned())
                {
                    tracing::info!("preauth_devicecheck: {cookie}");
                    self.cache.insert(device_id, cookie);
                }
            }
            Err(err) => {
                tracing::error!("invalid preauth_devicecheck request: {}", err)
            }
        }
    }
}

impl DeviceCheckHandler {
    pub async fn handle_request(&self, req: http::Request<Body>) -> RequestOrResponse {
        if req.uri().path().eq("/backend-api/preauth_devicecheck") {
            // Hook request
            return self.hook_request(req).await;
        }

        // Pass request
        RequestOrResponse::Request(req)
    }
}

#[derive(Debug)]
struct DeviceCheckRequest {
    uri: Uri,
    method: Method,
    headers: HeaderMap,
    body: DeviceCheckBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceCheckBody {
    pub bundle_id: String,
    pub device_id: String,
    pub device_token: String,
    pub request_flag: bool,
}

#[derive(Serialize)]
struct PreAuthCookie {
    preauth_cookie: Option<String>,
}

fn spawn_with_trace<T: Send + Sync + 'static>(
    fut: impl Future<Output = T> + Send + 'static,
    span: Span,
) -> JoinHandle<T> {
    tokio::spawn(fut.instrument(span))
}
