use crate::error::Error;
use http::{response::Builder, Request, Response};
use hyper::Body;
use reqwest::{redirect::Policy, Client, Url};

#[derive(Clone)]
pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    pub fn new(proxy: Option<Url>) -> Result<Self, Error> {
        let mut builder = Client::builder();

        if let Some(proxy) = proxy {
            let proxy = reqwest::Proxy::all(proxy)?;
            builder = builder.proxy(proxy);
        }

        builder
            .redirect(Policy::none())
            .build()
            .map(|inner| Self { inner })
            .map_err(Into::into)
    }

    pub async fn http(&self, req: Request<Body>) -> Result<Response<Body>, Error> {
        let (parts, body) = req.into_parts();

        // Send request
        let mut resp = self
            .inner
            .request(parts.method, parts.uri.to_string())
            .headers(parts.headers)
            .body(reqwest::Body::wrap_stream(body))
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
}
