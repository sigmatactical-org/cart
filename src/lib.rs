//! Sigma Cart: shopping carts backed by catalog SKUs and identity users.

#![forbid(unsafe_code)]

mod accounting_client;
mod api;
mod catalog;
pub mod config;
mod identity;
mod model;
mod payments_client;
pub mod store;
mod storefront;
mod templates;
#[cfg(test)]
mod test_support;
mod web;

use std::convert::Infallible;
use std::sync::{Arc, OnceLock};

use warp::Filter;
use warp::Reply;

pub use model::{Cart, CartLine, CartStatus, CreateCart, CreateLine, UpdateCart, UpdateLine};

/// Shared cart store handle (`PgPool` is internally concurrent).
pub type SharedStore = Arc<store::CartStore>;

fn with_store(
    store: SharedStore,
) -> impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone {
    warp::any().map(move || store.clone())
}

/// Site routes: web UI, JSON API, `/up`, theme static assets, error recovery,
/// and the shared security-header set.
pub fn routes(
    store: store::CartStore,
) -> impl Filter<Extract = (impl Reply,), Error = Infallible> + Clone + Send + 'static {
    let health_pool = Arc::new(store.pool().clone());
    let store = Arc::new(store);

    let site = sigma_theme::warp::site_routes(
        web::routes(with_store(store.clone())).or(api::routes(with_store(store))),
        sigma_pg::health::warp::health_routes("cart", Some(health_pool)),
    );
    // The header set is built once and shared by every reply, so the CSP's
    // `connect-src` origin has to outlive the filter.
    static IDENTITY_ORIGIN: OnceLock<String> = OnceLock::new();
    let identity_origin = IDENTITY_ORIGIN.get_or_init(config::identity_public_origin);
    sigma_theme::warp::security_headers(site, identity_origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use warp::http::StatusCode;

    async fn test_store() -> store::CartStore {
        sigma_pg::clients::internal::ensure_test_internal_token();
        store::CartStore::connect_empty()
            .await
            .expect("PostgreSQL required for tests")
    }

    #[tokio::test]
    async fn up_returns_ok() {
        let _db = crate::test_support::db_guard().await;
        let res = warp::test::request()
            .method("GET")
            .path("/up")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn index_lists_carts() {
        let _db = crate::test_support::db_guard().await;
        let res = warp::test::request()
            .method("GET")
            .path("/")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = std::str::from_utf8(res.body()).unwrap();
        assert!(body.contains("Cart"));
    }

    #[tokio::test]
    async fn api_lists_empty_carts() {
        let _db = crate::test_support::db_guard().await;
        let res = warp::test::request()
            .method("GET")
            .path("/carts")
            .header("accept", "application/json")
            .header(
                "x-sigma-internal-token",
                sigma_pg::clients::internal::TEST_INTERNAL_TOKEN,
            )
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
        let body: Vec<serde_json::Value> = serde_json::from_slice(res.body()).unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn api_create_cart() {
        let _db = crate::test_support::db_guard().await;
        let res = warp::test::request()
            .method("POST")
            .path("/carts")
            .header("content-type", "application/json")
            .header(
                "x-sigma-internal-token",
                sigma_pg::clients::internal::TEST_INTERNAL_TOKEN,
            )
            .body(r#"{"note":"test cart"}"#)
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let body: serde_json::Value = serde_json::from_slice(res.body()).unwrap();
        assert_eq!(body["cart"]["status"], "open");
    }

    #[tokio::test]
    async fn api_add_line() {
        let _db = crate::test_support::db_guard().await;
        let store = test_store().await;
        let app = routes(store);

        let cart_res = warp::test::request()
            .method("POST")
            .path("/carts")
            .header("content-type", "application/json")
            .header(
                "x-sigma-internal-token",
                sigma_pg::clients::internal::TEST_INTERNAL_TOKEN,
            )
            .body(r#"{}"#)
            .reply(&app)
            .await;
        let cart: serde_json::Value = serde_json::from_slice(cart_res.body()).unwrap();
        let cart_id = cart["cart"]["id"].as_str().unwrap();

        let res = warp::test::request()
            .method("POST")
            .path(&format!("/carts/{cart_id}/lines"))
            .header("content-type", "application/json")
            .header(
                "x-sigma-internal-token",
                sigma_pg::clients::internal::TEST_INTERNAL_TOKEN,
            )
            .body(r#"{"sku_id":"sku-abc","quantity":2}"#)
            .reply(&app)
            .await;
        assert_eq!(res.status(), StatusCode::CREATED);
        let line: CartLine = serde_json::from_slice(res.body()).unwrap();
        assert_eq!(line.quantity, 2);
    }
}
