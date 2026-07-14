//! [`CheckoutOption`].

#[allow(unused_imports)]
use super::*;

#[derive(Clone)]
pub struct CheckoutOption {
    pub id: String,
    pub summary: String,
    pub selected: bool,
}
