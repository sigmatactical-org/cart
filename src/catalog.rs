use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog integration is not configured (set CART_CATALOG_BASE_URL)")]
    NotConfigured,
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("catalog request failed: {0}")]
    Request(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogSkuKind {
    Simple,
    Composite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSkuComponent {
    pub sku_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogSku {
    pub id: String,
    pub sku_code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub kind: CatalogSkuKind,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<CatalogSkuComponent>,
    pub updated_at: String,
}

/// Pull all SKUs from the catalog service.
pub async fn fetch_skus() -> Result<Vec<CatalogSku>, CatalogError> {
    let base = config::catalog_base_url().ok_or(CatalogError::NotConfigured)?;
    let url = format!("{base}skus");
    let response = reqwest::Client::new().get(url).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(CatalogError::Request(format!("{status}: {body}")));
    }

    let skus: Vec<CatalogSku> = response.json().await?;
    Ok(skus)
}

/// Validate that a catalog SKU id exists and is active.
pub fn validate_sku_id(skus: &[CatalogSku], sku_id: &str) -> Result<(), CatalogError> {
    skus.iter()
        .find(|s| s.id == sku_id && s.active)
        .map(|_| ())
        .ok_or_else(|| CatalogError::Request(format!("catalog sku not found: {sku_id}")))
}

#[must_use]
pub fn sku_by_id<'a>(skus: &'a [CatalogSku], id: &str) -> Option<&'a CatalogSku> {
    skus.iter().find(|s| s.id == id)
}
