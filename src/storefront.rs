//! Client for the store service's public `/items` feed. Prices live on store
//! listings (not the catalog), so the cart resolves the authoritative unit
//! price for each catalog SKU here, keyed by the catalog SKU id.

use std::collections::HashMap;

use serde::Deserialize;
use thiserror::Error;

use crate::config;

#[derive(Debug, Error)]
pub enum StorefrontError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("store request failed: {0}")]
    Request(String),
}

#[derive(Debug, Clone, Deserialize)]
struct Listing {
    sku_id: String,
    #[serde(default)]
    price_cents: Option<u64>,
    #[serde(default)]
    visible: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct StorefrontItem {
    listing: Listing,
}

/// Map of catalog SKU id -> unit price in cents, for visible, priced listings.
#[derive(Debug, Clone, Default)]
pub struct PriceBook {
    prices: HashMap<String, u64>,
}

impl PriceBook {
    /// Unit price in cents for a catalog SKU id, when it is a visible, priced listing.
    #[must_use]
    pub fn unit_price_cents(&self, sku_id: &str) -> Option<u64> {
        self.prices.get(sku_id).copied()
    }
}

/// Fetch the store's visible storefront items and build a price book. Returns an
/// empty price book (rather than erroring) when the store is not configured, so
/// the cart still renders — just without prices.
pub async fn fetch_prices() -> Result<PriceBook, StorefrontError> {
    let Some(base) = config::store_base_url() else {
        return Ok(PriceBook::default());
    };
    let url = format!("{base}items");
    let response = reqwest::Client::new().get(url).send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(StorefrontError::Request(format!("{status}: {body}")));
    }
    let items: Vec<StorefrontItem> = response.json().await?;
    let prices = items
        .into_iter()
        .filter(|item| item.listing.visible)
        .filter_map(|item| {
            item.listing
                .price_cents
                .filter(|cents| *cents > 0)
                .map(|cents| (item.listing.sku_id, cents))
        })
        .collect();
    Ok(PriceBook { prices })
}
