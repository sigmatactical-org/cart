//! [`CartRow`].

#[allow(unused_imports)]
use super::*;
use crate::model::Cart;

/// One rendered table row.
pub struct CartRow {
    pub cart: Cart,
    pub user_display: String,
    pub status_label: String,
    pub line_count: usize,
    pub missing_user: bool,
}
