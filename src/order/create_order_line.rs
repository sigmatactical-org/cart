//! [`CreateOrderLine`].

#[allow(unused_imports)]
use super::*;

/// Line payload for `POST /orders`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CreateOrderLine {
    pub sku_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub unit_price_cents: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_total_cents: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_cents: Option<u64>,
}
