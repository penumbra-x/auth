mod ca;
mod client;
pub mod handler;
mod mitm;
mod rewind;

use self::client::HttpClient;
use crate::error::Error;
pub use ca::CertificateAuthority;
use handler::DeviceCheckHandler;
pub use hyper;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Server,
};
use mitm::MitmProxy;
use reqwest::Url;
use std::{convert::Infallible, future::Future, net::SocketAddr, sync::Arc};
use typed_builder::TypedBuilder;

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
        let proxy = self.proxy;
        let client = HttpClient::new(proxy.clone())?;
        let handler = DeviceCheckHandler::new(proxy)?;
        let make_service = make_service_fn(move |_conn: &AddrStream| {
            let ca = Arc::clone(&self.ca);
            let client = client.clone();
            let handler = handler.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let mitm_proxy = MitmProxy {
                        ca: Arc::clone(&ca),
                        client: client.clone(),
                        handler: handler.clone(),
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
