//! Catalog service lookups. `sigma_pg` caches the SKU list per process, so
//! callers can fetch freely; the `Arc` keeps repeat reads allocation-free.

use std::sync::Arc;

pub use sigma_pg::clients::catalog::{CatalogError, CatalogSku, sku_by_id, validate_sku_id};

pub async fn fetch_skus() -> Result<Arc<Vec<CatalogSku>>, CatalogError> {
    sigma_pg::clients::catalog::fetch_skus(crate::config::catalog_base_url().as_deref()).await
}

/// Fail-closed SKU validation for mutations when catalog integration is configured.
pub async fn require_active_sku(sku_id: &str) -> Result<(), CatalogError> {
    if !crate::config::catalog_configured() {
        return Ok(());
    }
    let skus = fetch_skus().await?;
    validate_sku_id(&skus, sku_id.trim())
}
