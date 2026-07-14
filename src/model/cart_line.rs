//! [`CartLine`].

#[allow(unused_imports)]
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartLine {
    pub id: String,
    pub sku_id: String,
    pub quantity: u32,
    pub updated_at: String,
}
impl CartLine {
    /// New Line from a create request.
    pub fn new(input: CreateLine) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            sku_id: input.sku_id.trim().to_string(),
            quantity: input.quantity,
            updated_at: now,
        }
    }

    /// Apply a partial update in place.
    pub fn apply_update(&mut self, quantity: u32) {
        self.quantity = quantity;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}
