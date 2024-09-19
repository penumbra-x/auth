use rcgen::Error as RcgenError;
use std::io;
use thiserror::Error;
use tokio_tungstenite::tungstenite::error::ProtocolError;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Tls(#[from] RcgenError),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

    #[error(transparent)]
    BodyErrpr(#[from] http::Error),

    #[error(transparent)]
    RequestConnectError(#[from] rquest::Error),

    #[error(transparent)]
    IO(#[from] io::Error),

    #[error(transparent)]
    WebSocketError(#[from] ProtocolError),
}
