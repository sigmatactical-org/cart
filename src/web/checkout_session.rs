//! [`CheckoutSession`].

/// The signed-in shopper resolved from the identity session cookie.
pub(crate) struct CheckoutSession {
    pub(crate) user_id: String,
    pub(crate) username: String,
}
