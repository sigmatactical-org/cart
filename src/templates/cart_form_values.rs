//! [`CartFormValues`].

#[allow(unused_imports)]
use super::*;
use crate::model::Cart;

/// Prefilled field values for the edit/create form.
pub struct CartFormValues {
    pub user_id: String,
    pub status: String,
    pub note: String,
}
impl CartFormValues {
    /// Prefill from an existing cart.
    pub fn from_cart(cart: &Cart) -> Self {
        Self {
            user_id: cart.user_id.clone().unwrap_or_default(),
            status: status_to_form(cart.status),
            note: cart.note.clone().unwrap_or_default(),
        }
    }
}
