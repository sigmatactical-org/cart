//! [`OrderError`].

#[allow(unused_imports)]
use super::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrderError {
    #[error("order service not configured")]
    NotConfigured,
    #[error("order service request failed: {0}")]
    Request(String),
}
