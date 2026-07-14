//! [`CheckoutLineRow`].

#[allow(unused_imports)]
use super::*;

#[derive(Clone)]
pub struct CheckoutLineRow {
    pub name: String,
    pub quantity: u32,
    pub line_total_display: String,
}
