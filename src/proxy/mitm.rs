use super::client;
use super::{ca::CertificateAuthority, client::HttpClient, rewind::Rewind};
use futures_util::{Future, SinkExt, StreamExt};
use http::uri::Authority;
use http::StatusCode;
use http::{header, uri::Scheme, Uri};
use hyper::upgrade::Upgraded;
use hyper::{server::conn::Http, service::service_fn, Body, Method, Request, Response};
use rquest::WebSocket;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{info_span, instrument, Instrument, Span};

/// Enum representing either an HTTP request or response.
#[allow(dead_code)]
#[derive(Debug)]
pub enum RequestOrResponse {
    Request(Request<Body>),
    Response(Response<Body>),
}

#[derive(Clone)]
pub(crate) struct MitmProxy {
    pub ca: Arc<CertificateAuthority>,
    pub client: HttpClient,
}

impl MitmProxy {
    pub(crate) async fn proxy(self, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
        tracing::info!("{req:?}");
        if req.method() == Method::CONNECT {
            Ok(self.process_connect(req))
        } else if client::ws::is_upgrade_request(&req) {
            Ok(self.upgrade_websocket(req).await)
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

        let mut res = match self.client.http(req).await {
            Ok(res) => res,
            Err(err) => {
                tracing::warn!("Http proxy request failed: {err:?}");
                bad_request()
            }
        };

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

                            if buffer == *b"GET " {
                                if let Err(e) =
                                    self.serve_stream(upgraded, Scheme::HTTP, authority).await
                                {
                                    tracing::error!("WebSocket connect error: {}", e);
                                }

                                return;
                            } else if buffer[..2] == *b"\x16\x03" {
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

    async fn upgrade_websocket(self, req: Request<Body>) -> Response<Body> {
        let mut req = {
            let (mut parts, body) = req.into_parts();

            parts.uri = {
                let mut parts = parts.uri.into_parts();

                parts.scheme = if parts.scheme.unwrap_or(Scheme::HTTP) == Scheme::HTTP {
                    Some("ws".try_into().expect("Failed to convert scheme"))
                } else {
                    Some("wss".try_into().expect("Failed to convert scheme"))
                };

                match Uri::from_parts(parts) {
                    Ok(uri) => uri,
                    Err(_) => {
                        return bad_request();
                    }
                }
            };

            Request::from_parts(parts, body)
        };

        let span = info_span!("upgrade_websocket");
        match self.client.websocket(&mut req).await {
            Ok((resp, server_socket)) => {
                let fut = async move {
                    match client::ws::upgrade(&mut req, None).await {
                        Ok(client_socket) => handle_websocket(client_socket, server_socket).await,
                        Err(err) => {
                            tracing::error!("Failed to upgrade websocket: {err}")
                        }
                    }
                };

                spawn_with_trace(fut, span);
                resp
            }
            Err(err) => {
                tracing::warn!("Websocket proxy request failed: {err:?}");
                bad_request()
            }
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

pub async fn handle_websocket(client_socket: WebSocketStream<Upgraded>, server_socket: WebSocket) {
    let (mut client_sender, mut client_receiver) = client_socket.split();
    let (mut server_sender, mut server_receiver) = server_socket.split();

    loop {
        tokio::select! {
            Some(Ok(msg)) = server_receiver.next() => {
                // If the server sends a message, we send it to the client
                if let Err(err) = client_sender.send(r2m(msg)).await {
                    tracing::debug!("Error sending message to client: {err}");
                    break;
                }
            }
            Some(Ok(msg)) = client_receiver.next() => {
                // If the client sends a message, we send it to the server
                if let Err(err) = server_sender.send(m2r(msg)).await {
                    tracing::debug!("Error sending message to server: {err}");
                    break;
                }
            }
            else => {
                break;
            },
        }
    }

    // If either the client or server socket is closed, we close the other
    let _ = client_sender.close().await;
    let _ = server_sender.close().await;

    // Drop the client sockets
    drop(client_sender);
    drop(client_receiver);

    // Drop the server sockets
    drop(server_sender);
    drop(server_receiver);
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

fn r2m(msg: rquest::Message) -> Message {
    match msg {
        rquest::Message::Text(text) => Message::Text(text),
        rquest::Message::Binary(binary) => Message::Binary(binary),
        rquest::Message::Ping(ping) => Message::Ping(ping),
        rquest::Message::Pong(pong) => Message::Pong(pong),
        rquest::Message::Close { code, .. } => Message::Close(Some(CloseFrame {
            code: CloseCode::from(u16::from(code)),
            reason: std::borrow::Cow::Borrowed("Close"),
        })),
    }
}

fn m2r(msg: Message) -> rquest::Message {
    match msg {
        Message::Text(text) => rquest::Message::Text(text),
        Message::Binary(binary) => rquest::Message::Binary(binary),
        Message::Ping(ping) => rquest::Message::Ping(ping),
        Message::Pong(pong) => rquest::Message::Pong(pong),
        Message::Close(Some(CloseFrame { code, reason })) => rquest::Message::Close {
            code: rquest::CloseCode::from(u16::from(code)),
            reason: Some(reason.into_owned()),
        },
        Message::Close(None) => rquest::Message::Close {
            code: rquest::CloseCode::default(),
            reason: None,
        },
        Message::Frame(_) => unimplemented!("Unsupport websocket frame"),
    }
}
