//! [`CartLine`].

use serde::{Deserialize, Serialize};

use super::CreateLine;

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
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            sku_id: input.sku_id.trim().to_string(),
            quantity: input.quantity,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}
