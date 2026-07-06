pub use sigma_pg::clients::catalog::{CatalogError, CatalogSku};

pub async fn fetch_skus() -> Result<Vec<CatalogSku>, CatalogError> {
    sigma_pg::clients::catalog::fetch_skus(crate::config::catalog_base_url().as_deref()).await
}

pub use sigma_pg::clients::catalog::{sku_by_id, validate_sku_id};

/// Fail-closed SKU validation for mutations.
pub async fn require_active_sku(sku_id: &str) -> Result<(), CatalogError> {
    let skus = fetch_skus().await?;
    validate_sku_id(&skus, sku_id.trim())
}
