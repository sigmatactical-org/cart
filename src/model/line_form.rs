//! [`LineForm`].

use serde::Deserialize;

use super::{CreateLine, parse_quantity};

#[derive(Debug, Clone, Deserialize)]
pub struct LineForm {
    pub sku_id: String,
    pub quantity: String,
}
impl LineForm {
    /// Validate the form into a create request.
    pub fn into_create(self) -> Result<CreateLine, String> {
        let quantity = parse_quantity(&self.quantity)?;
        Ok(CreateLine {
            sku_id: self.sku_id,
            quantity,
        })
    }
}
