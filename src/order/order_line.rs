//! [`OrderLine`].

#[allow(unused_imports)]
use super::*;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OrderLine {
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub line_total_cents: u64,
}
