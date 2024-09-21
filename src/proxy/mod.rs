mod ca;
mod client;
mod handler;
mod mitm;
mod rewind;

use crate::error::Error;
use handler::HttpHandler;
use http::Response;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Server,
};
use mitm::MitmProxy;
use reqwest::Url;
use serde_json::Value;
use std::{convert::Infallible, future::Future, net::SocketAddr, sync::Arc};
use typed_builder::TypedBuilder;

use self::client::HttpClient;
pub use ca::CertificateAuthority;
pub use hyper;

#[derive(TypedBuilder)]
pub struct Proxy {
    /// The address to listen on.
    pub listen_addr: SocketAddr,

    /// Upstream proxy
    pub proxy: Option<Url>,

    /// The certificate authority to use.
    pub ca: Arc<CertificateAuthority>,
}

impl Proxy {
    pub async fn start<F: Future<Output = ()>>(self, shutdown_signal: F) -> Result<(), Error> {
        let client = HttpClient::new(self.proxy)?;
        let http_handler = PreAuthHandler;
        let make_service = make_service_fn(move |_conn: &AddrStream| {
            let ca = Arc::clone(&self.ca);
            let client = client.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let mitm_proxy = MitmProxy {
                        ca: Arc::clone(&ca),
                        client: client.clone(),
                        handler: http_handler,
                    };
                    mitm_proxy.proxy(req)
                }))
            }
        });

        Server::bind(&self.listen_addr)
            .http1_preserve_header_case(true)
            .http1_title_case_headers(true)
            .serve(make_service)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(Into::into)
    }
}

#[derive(Clone, Copy)]
struct PreAuthHandler;

impl HttpHandler for PreAuthHandler {
    async fn handle_request(&self, req: http::Request<hyper::Body>) -> mitm::RequestOrResponse {
        if req.uri().path().eq("/backend-api/preauth_devicecheck") {
            tracing::info!("{req:?}");
            match hyper::body::to_bytes(req.into_body())
                .await
                .map(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            {
                Ok(Some(body)) => {
                    let body = serde_json::to_string_pretty(&body).unwrap_or_default();
                    tracing::info!("preauth_devicecheck request body: {body}")
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::error!("invalid preauth_devicecheck request: {}", err)
                }
            }
            // Hook return invalid request
            return mitm::RequestOrResponse::Response(Response::new(Body::empty()));
        }

        // Pass request
        mitm::RequestOrResponse::Request(req)
    }
}
