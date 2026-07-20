//! Authoritative listing prices from the store service, cached per process.
//!
//! Every cart and checkout render needs the whole price book, so it is served
//! from a short TTL cache (with stale fallback) instead of re-fetching the
//! store's `/items` list on each request.

use std::sync::Arc;
use std::time::Duration;

use sigma_theme::cache::TtlCache;

pub use sigma_pg::clients::storefront::{PriceBook, StorefrontError};

const PRICES_TTL: Duration = Duration::from_secs(30);

static PRICES: TtlCache<PriceBook> = TtlCache::new();

pub async fn fetch_prices() -> Result<Arc<PriceBook>, StorefrontError> {
    PRICES
        .get_or_fetch(PRICES_TTL, || async {
            let base = crate::config::store_base_url();
            sigma_pg::clients::storefront::fetch_prices(base.as_deref()).await
        })
        .await
}
