//! [`ReservedLineRow`].

#[allow(unused_imports)]
use super::*;

/// One rendered table row.
pub struct ReservedLineRow {
    pub sku_code: String,
    pub name: String,
    pub product_url: String,
    pub quantity: u32,
    pub line_total_display: String,
}
