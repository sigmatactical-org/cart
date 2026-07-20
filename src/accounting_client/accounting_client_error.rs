//! [`AccountingClientError`].

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccountingClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("accounting request failed: {0}")]
    Request(String),
}
