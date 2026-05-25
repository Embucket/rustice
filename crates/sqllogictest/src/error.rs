use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("Arrow: {0}")]
    Arrow(#[from] datafusion::arrow::error::ArrowError),
    #[error("{0}")]
    Other(String),
}
