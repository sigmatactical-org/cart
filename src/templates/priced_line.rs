//! [`PricedLine`].

#[allow(unused_imports)]
use super::*;

/// A cart line joined with its catalog SKU and store price.
pub struct PricedLine {
    pub line_id: String,
    pub sku_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub unit_price_cents: u64,
    pub in_catalog: bool,
}
