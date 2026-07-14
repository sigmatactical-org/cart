//! [`CheckoutForm`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct CheckoutForm {
    pub(crate) billing_address_id: String,
    pub(crate) shipping_address_id: String,
    pub(crate) payment_method_id: String,
    #[serde(default)]
    pub(crate) accept_terms: Option<String>,
}
