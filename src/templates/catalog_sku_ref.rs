//! [`CatalogSkuRef`].

#[allow(unused_imports)]
use super::*;

/// Lightweight reference for pickers/links.
pub struct CatalogSkuRef {
    pub id: String,
    pub sku_code: String,
    pub name: String,
}
