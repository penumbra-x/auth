use rcgen::Error as RcgenError;
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Tls(#[from] RcgenError),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

    #[error(transparent)]
    BodyErrpr(#[from] http::Error),

    #[error(transparent)]
    RequestConnectError(#[from] reqwest::Error),

    #[error(transparent)]
    IO(#[from] io::Error),

    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
}
