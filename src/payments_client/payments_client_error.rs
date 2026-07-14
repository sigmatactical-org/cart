//! [`PaymentsClientError`].

#[allow(unused_imports)]
use super::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PaymentsClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("payments request failed: {0}")]
    Request(String),
    #[error("payment declined: {0}")]
    Declined(String),
}
