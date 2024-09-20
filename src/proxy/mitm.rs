use super::handler::HttpHandler;
use super::{ca::CertificateAuthority, client::HttpClient, rewind::Rewind};
use futures_util::Future;
use http::uri::Authority;
use http::StatusCode;
use http::{header, uri::Scheme, Uri};
use hyper::{server::conn::Http, service::service_fn, Body, Method, Request, Response};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tracing::{info_span, instrument, Instrument, Span};

/// Enum representing either an HTTP request or response.
#[allow(dead_code)]
#[derive(Debug)]
pub enum RequestOrResponse {
    Request(Request<Body>),
    Response(Response<Body>),
}

#[derive(Clone)]
pub(crate) struct MitmProxy<H: HttpHandler> {
    pub handler: H,
    pub ca: Arc<CertificateAuthority>,
    pub client: HttpClient,
}

impl<H: HttpHandler> MitmProxy<H> {
    pub(crate) async fn proxy(self, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
        tracing::debug!("{req:?}");
        if req.method() == Method::CONNECT {
            Ok(self.process_connect(req))
        } else {
            self.process_request(normalize_request(req), Scheme::HTTP)
                .await
        }
    }

    async fn process_request(
        self,
        mut req: Request<Body>,
        scheme: Scheme,
    ) -> Result<Response<Body>, hyper::Error> {
        if req.uri().path().starts_with("/mitm/cert") {
            return Ok(self.get_cert_res());
        }

        if req.version() == http::Version::HTTP_10 || req.version() == http::Version::HTTP_11 {
            let (mut parts, body) = req.into_parts();

            if let Some(Ok(authority)) = parts
                .headers
                .get(http::header::HOST)
                .map(|host| host.to_str())
            {
                let mut uri = parts.uri.into_parts();
                uri.scheme = Some(scheme.clone());
                uri.authority = authority.try_into().ok();
                parts.uri = Uri::from_parts(uri).expect("build uri");
            }

            req = Request::from_parts(parts, body);
        };

        // Fix VPN signature recognition
        {
            let headers = req.headers_mut();
            headers.remove(http::header::HOST);
            headers.remove(http::header::CONNECTION);
        }

        // Http request Handler
        let req = match self.handler.handle_request(req) {
            RequestOrResponse::Request(request) => request,
            RequestOrResponse::Response(response) => {
                return Ok(response);
            }
        };

        // Send Http request
        let res = match self.client.http(req).await {
            Ok(res) => res,
            Err(err) => {
                tracing::warn!("Http proxy request failed: {err:?}");
                bad_request()
            }
        };

        // Http response handler
        let mut res = self.handler.handle_response(res);
        {
            let header_mut = res.headers_mut();
            // Remove `Strict-Transport-Security` to avoid HSTS
            // See: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Strict-Transport-Security
            header_mut.remove(header::STRICT_TRANSPORT_SECURITY);
        }

        Ok(res)
    }

    fn process_connect(self, mut req: Request<Body>) -> Response<Body> {
        match req.uri().authority().cloned() {
            Some(authority) => {
                let span = info_span!("process_connect");
                let fut = async move {
                    match hyper::upgrade::on(&mut req).await {
                        Ok(mut upgraded) => {
                            let mut buffer = [0; 4];
                            let bytes_read = match upgraded.read(&mut buffer).await {
                                Ok(bytes_read) => bytes_read,
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to read from upgraded connection: {}",
                                        e
                                    );
                                    return;
                                }
                            };

                            let mut upgraded = Rewind::new_buffered(
                                upgraded,
                                bytes::Bytes::copy_from_slice(buffer[..bytes_read].as_ref()),
                            );

                            if buffer[..2] == *b"\x16\x03" {
                                let server_config = self.ca.clone().gen_server_config();

                                let stream =
                                    match TlsAcceptor::from(server_config).accept(upgraded).await {
                                        Ok(stream) => stream,
                                        Err(e) => {
                                            tracing::error!(
                                                "Failed to establish TLS connection: {}",
                                                e
                                            );
                                            return;
                                        }
                                    };

                                if let Err(e) =
                                    self.serve_stream(stream, Scheme::HTTPS, authority).await
                                {
                                    if !e.to_string().starts_with("error shutting down connection")
                                    {
                                        tracing::error!("HTTPS connect error: {}", e);
                                    }
                                }

                                return;
                            } else {
                                tracing::warn!(
                                    "Unknown protocol, read '{:02X?}' from upgraded connection",
                                    &buffer[..bytes_read]
                                );
                            }

                            let mut server = match TcpStream::connect(authority.as_ref()).await {
                                Ok(server) => server,
                                Err(e) => {
                                    tracing::error!("Failed to connect to {}: {}", authority, e);
                                    return;
                                }
                            };

                            if let Err(e) =
                                tokio::io::copy_bidirectional(&mut upgraded, &mut server).await
                            {
                                tracing::error!("Failed to tunnel to {}: {}", authority, e);
                            }
                        }
                        Err(e) => tracing::error!("Upgrade error: {}", e),
                    };
                };

                spawn_with_trace(fut, span);
                Response::new(Body::empty())
            }
            None => bad_request(),
        }
    }

    async fn serve_stream<I>(
        self,
        stream: I,
        scheme: Scheme,
        authority: Authority,
    ) -> Result<(), hyper::Error>
    where
        I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let service = service_fn(|mut req| {
            if req.version() == hyper::Version::HTTP_10 || req.version() == hyper::Version::HTTP_11
            {
                let (mut parts, body) = req.into_parts();

                parts.uri = {
                    let mut parts = parts.uri.into_parts();
                    parts.scheme = Some(scheme.clone());
                    parts.authority = Some(authority.clone());
                    Uri::from_parts(parts).expect("Failed to build URI")
                };

                req = Request::from_parts(parts, body);
            };

            self.clone().proxy(req)
        });

        Http::new()
            .serve_connection(stream, service)
            .with_upgrades()
            .await
    }

    fn get_cert_res(&self) -> hyper::Response<Body> {
        Response::builder()
            .header(
                http::header::CONTENT_DISPOSITION,
                "attachment; filename=auth-mitm.crt",
            )
            .header(http::header::CONTENT_TYPE, "application/octet-stream")
            .status(http::StatusCode::OK)
            .body(Body::from(self.ca.clone().get_cert()))
            .expect("Failed build response")
    }
}

fn bad_request() -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Body::empty())
        .expect("Failed to build response")
}

fn spawn_with_trace<T: Send + Sync + 'static>(
    fut: impl Future<Output = T> + Send + 'static,
    span: Span,
) -> JoinHandle<T> {
    tokio::spawn(fut.instrument(span))
}

#[instrument(skip_all)]
fn normalize_request<T>(mut req: Request<T>) -> Request<T> {
    // Hyper will automatically add a Host header if needed.
    req.headers_mut().remove(hyper::header::HOST);
    *req.version_mut() = hyper::Version::HTTP_11;
    req
}
