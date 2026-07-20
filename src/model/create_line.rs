//! [`CreateLine`].

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLine {
    pub sku_id: String,
    pub quantity: u32,
}
