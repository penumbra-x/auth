use hyper::Request;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::{Role, WebSocketConfig};
use tungstenite::{error::ProtocolError, Error};

pin_project_lite::pin_project! {
    /// A future that resolves to a websocket stream when the associated HTTP upgrade completes.
    #[derive(Debug)]
    pub struct HyperWebsocket {
        #[pin]
        inner: hyper::upgrade::OnUpgrade,
        config: Option<WebSocketConfig>,
    }
}

/// Try to upgrade a received `hyper::Request` to a websocket connection.
///
/// The function returns a HTTP response and a future that resolves to the websocket stream.
/// The response body *MUST* be sent to the client before the future can be resolved.
///
/// This functions checks `Sec-WebSocket-Key` and `Sec-WebSocket-Version` headers.
/// It does not inspect the `Origin`, `Sec-WebSocket-Protocol` or `Sec-WebSocket-Extensions` headers.
/// You can inspect the headers manually before calling this function,
/// and modify the response headers appropriately.
///
/// This function also does not look at the `Connection` or `Upgrade` headers.
/// To check if a request is a websocket upgrade request, you can use [`is_upgrade_request`].
/// Alternatively you can inspect the `Connection` and `Upgrade` headers manually.
///
pub fn upgrade<B>(
    mut request: impl std::borrow::BorrowMut<Request<B>>,
    config: Option<WebSocketConfig>,
) -> HyperWebsocket {
    let request = request.borrow_mut();

    HyperWebsocket {
        inner: hyper::upgrade::on(request),
        config,
    }
}

/// Check if a request is a websocket upgrade request.
///
/// If the `Upgrade` header lists multiple protocols,
/// this function returns true if of them are `"websocket"`,
/// If the server supports multiple upgrade protocols,
/// it would be more appropriate to try each listed protocol in order.
pub fn is_upgrade_request<B>(request: &hyper::Request<B>) -> bool {
    header_contains_value(request.headers(), hyper::header::CONNECTION, "Upgrade")
        && header_contains_value(request.headers(), hyper::header::UPGRADE, "websocket")
}

/// Check if there is a header of the given name containing the wanted value.
fn header_contains_value(
    headers: &hyper::HeaderMap,
    header: impl hyper::header::AsHeaderName,
    value: impl AsRef<[u8]>,
) -> bool {
    let value = value.as_ref();
    for header in headers.get_all(header) {
        if header
            .as_bytes()
            .split(|&c| c == b',')
            .any(|x| trim(x).eq_ignore_ascii_case(value))
        {
            return true;
        }
    }
    false
}

fn trim(data: &[u8]) -> &[u8] {
    trim_end(trim_start(data))
}

fn trim_start(data: &[u8]) -> &[u8] {
    if let Some(start) = data.iter().position(|x| !x.is_ascii_whitespace()) {
        &data[start..]
    } else {
        b""
    }
}

fn trim_end(data: &[u8]) -> &[u8] {
    if let Some(last) = data.iter().rposition(|x| !x.is_ascii_whitespace()) {
        &data[..last + 1]
    } else {
        b""
    }
}

impl std::future::Future for HyperWebsocket {
    type Output = Result<WebSocketStream<hyper::upgrade::Upgraded>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = self.project();
        let upgraded = match this.inner.poll(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(x) => x,
        };

        let upgraded = upgraded.map_err(|_| Error::Protocol(ProtocolError::HandshakeIncomplete))?;

        let stream = WebSocketStream::from_raw_socket(upgraded, Role::Server, this.config.take());
        tokio::pin!(stream);

        // The future returned by `from_raw_socket` is always ready.
        // Not sure why it is a future in the first place.
        match stream.as_mut().poll(cx) {
            Poll::Pending => unreachable!("from_raw_socket should always be created ready"),
            Poll::Ready(x) => Poll::Ready(Ok(x)),
        }
    }
}
