//! [`RefundChargeBody`].

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct RefundChargeBody<'a> {
    pub(crate) reason: &'a str,
}
