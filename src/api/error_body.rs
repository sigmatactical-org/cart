//! [`ErrorBody`].

#[allow(unused_imports)]
use super::*;

#[derive(serde::Serialize)]
pub(crate) struct ErrorBody {
    pub(crate) error: String,
}
