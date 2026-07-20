//! [`LineRow`].

/// One rendered table row.
pub struct LineRow {
    pub line_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub missing_catalog: bool,
}
