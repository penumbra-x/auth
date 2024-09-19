pub mod ws;

use crate::error::Error;
use http::{
    header::{self},
    response::Builder,
    Request, Response,
};
use hyper::Body;
use rquest::{tls::Impersonate, Client, Url, WebSocket};
use tokio_tungstenite::tungstenite::error::ProtocolError;

#[derive(Clone)]
pub struct HttpClient {
    http: Client,
    ws: Client,
}

impl HttpClient {
    pub fn new(proxy: Option<Url>) -> Result<Self, Error> {
        Ok(Self {
            http: build_client(proxy.clone(), false)?,
            ws: build_client(proxy, true)?,
        })
    }

    pub async fn http(&self, req: Request<Body>) -> Result<Response<Body>, Error> {
        let (parts, body) = req.into_parts();

        // Send request
        let mut resp = self
            .http
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(rquest::Body::wrap_stream(body))
            .send()
            .await?;

        // Create response builder
        let mut builder = Builder::new()
            .status(resp.status())
            .version(resp.version())
            .extension(parts.extensions);

        // Move headers
        builder
            .headers_mut()
            .map(|headers| headers.extend(std::mem::take(resp.headers_mut())));

        // Build response
        builder
            .body(Body::wrap_stream(resp.bytes_stream()))
            .map_err(Into::into)
    }

    pub async fn websocket(
        &self,
        req: &mut Request<Body>,
    ) -> Result<(Response<Body>, WebSocket), Error> {
        // Extract the request sec-websocket-key
        let key = req
            .headers()
            .get(header::SEC_WEBSOCKET_KEY)
            .ok_or(ProtocolError::MissingSecWebSocketKey)?;
        let key = key.to_str().map(ToOwned::to_owned).unwrap_or_default();

        // Extract the request sec-websocket-protocol
        let protocol = req.headers().get(header::SEC_WEBSOCKET_PROTOCOL).cloned();

        // Send the request to the server
        let mut client_builder = self
            .ws
            .request(req.method().clone(), req.uri().to_string())
            .headers(req.headers().clone())
            .upgrade_with_key(key);

        // Set the sec-websocket-protocol header if it exists
        if let Some(protocol) = protocol {
            let protocols = protocol
                .to_str()
                .map(ToOwned::to_owned)
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_owned())
                .collect::<Vec<String>>();

            client_builder = client_builder.protocols(protocols);
        }

        // Send the request to the server
        let resp = client_builder.send().await?;

        // Create a new response with the same status and version as the response from the server
        let mut builder = Builder::new().status(resp.status()).version(resp.version());

        // Copy the headers from the response
        builder
            .headers_mut()
            .map(|h| h.extend(resp.headers().clone()));

        // Return an empty body
        let response = builder.body(Body::empty())?;

        // Into_websocket() will return an error if the response is not a websocket
        let websocket = resp.into_websocket().await?;

        Ok((response, websocket))
    }
}

fn build_client(proxy: Option<Url>, ws: bool) -> Result<Client, Error> {
    let mut builder = Client::builder();

    if let Some(proxy) = proxy {
        let proxy = rquest::Proxy::all(proxy)?;
        builder = builder.proxy(proxy);
    }

    if ws {
        builder = builder.http1_only();
    }

    builder
        .impersonate_with_headers(Impersonate::SafariIos17_4_1, false)
        .build()
        .map_err(Into::into)
}
