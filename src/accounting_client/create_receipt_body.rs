//! [`CreateReceiptBody`].

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct CreateReceiptBody<'a> {
    pub(crate) charge_id: &'a str,
    pub(crate) order_id: &'a str,
    pub(crate) user_id: &'a str,
    pub(crate) kind: &'a str,
    pub(crate) amount_cents: u64,
    pub(crate) currency: &'a str,
}
