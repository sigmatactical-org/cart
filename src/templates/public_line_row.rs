//! [`PublicLineRow`].

#[allow(unused_imports)]
use super::*;

/// A resolved, priced line for the public cart view.
pub struct PublicLineRow {
    pub line_id: String,
    pub sku_code: String,
    pub name: String,
    pub product_url: String,
    pub quantity: u32,
    pub unit_price_display: String,
    pub line_total_display: String,
    pub priced: bool,
}
