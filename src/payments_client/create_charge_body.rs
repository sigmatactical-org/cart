//! [`CreateChargeBody`].

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct CreateChargeBody<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) payment_method_id: &'a str,
    pub(crate) amount_cents: u64,
    pub(crate) currency: &'a str,
    pub(crate) reference: &'a str,
}
