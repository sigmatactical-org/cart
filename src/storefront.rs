pub use sigma_pg::clients::storefront::{PriceBook, StorefrontError};

pub async fn fetch_prices() -> Result<PriceBook, StorefrontError> {
    sigma_pg::clients::storefront::fetch_prices(crate::config::store_base_url().as_deref()).await
}
