//! [`CartFormValues`].

use crate::model::{Cart, CartStatus};

/// Prefilled field values for the edit/create form.
pub struct CartFormValues {
    pub user_id: String,
    pub status: String,
    pub note: String,
}
impl CartFormValues {
    /// Prefill from an existing cart.
    #[must_use]
    pub fn from_cart(cart: &Cart) -> Self {
        Self {
            user_id: cart.user_id.clone().unwrap_or_default(),
            status: cart.status.as_str().to_string(),
            note: cart.note.clone().unwrap_or_default(),
        }
    }

    /// Prefill from an existing cart, or blank defaults for a new one.
    #[must_use]
    pub fn for_cart(cart: Option<&Cart>) -> Self {
        match cart {
            Some(cart) => Self::from_cart(cart),
            None => Self {
                user_id: String::new(),
                status: CartStatus::Open.as_str().to_string(),
                note: String::new(),
            },
        }
    }
}
