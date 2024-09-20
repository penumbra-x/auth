mod ca;
mod client;
mod handler;
mod mitm;
mod rewind;

use crate::error::Error;
use handler::HttpHandler;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Server,
};
use mitm::MitmProxy;
use rquest::Url;
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
    fn handle_request(&self, req: http::Request<hyper::Body>) -> mitm::RequestOrResponse {
        if req.uri().host().unwrap().eq("ios.chat.openai.com") {
            tracing::info!("{req:?}");
        }
        mitm::RequestOrResponse::Request(req)
    }

    fn handle_response(&self, res: http::Response<hyper::Body>) -> http::Response<hyper::Body> {
        tracing::info!("{res:?}");
        res
    }
}
