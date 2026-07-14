//! [`CheckoutSession`].

#[allow(unused_imports)]
use super::*;

/// State carried across the multi-step checkout flow.
pub(crate) struct CheckoutSession {
    pub(crate) user_id: String,
    pub(crate) username: String,
}
