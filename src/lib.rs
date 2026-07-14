//! Sigma Cart: shopping carts backed by catalog SKUs and identity users.

mod addresses_client;
mod api;
mod catalog;
pub mod config;
mod identity;
mod model;
mod order;
mod payments_client;
pub mod store;
mod storefront;
mod templates;
mod web;

use std::convert::Infallible;
use std::sync::Arc;

use warp::Filter;
use warp::Reply;

pub use model::{Cart, CartLine, CartStatus, CreateCart, CreateLine, UpdateCart, UpdateLine};

/// Shared cart store handle (`PgPool` is internally concurrent).
pub type SharedStore = Arc<store::CartStore>;

/// Resolve listen address from **`PORT`** (default **8080**).
#[must_use]
pub fn listen_socket_addr_from_env() -> std::net::SocketAddr {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)
}

fn with_store(
    store: SharedStore,
) -> impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone {
    warp::any().map(move || store.clone())
}

fn content_security_policy() -> String {
    let identity_origin = config::identity_public_origin();
    format!(
        "default-src 'self'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; \
         img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; \
         font-src 'self'; connect-src 'self' {identity_origin}; form-action 'self'"
    )
}

/// Site routes: web UI, JSON API, `/up`, theme static assets, error recovery.
pub fn routes(
    store: store::CartStore,
) -> impl Filter<Extract = (impl Reply,), Error = Infallible> + Clone + Send + 'static {
    use warp::reply::with::header;

    let health_pool = Arc::new(store.pool().clone());
    let store = Arc::new(store);

    warp::path("up")
        .and(warp::get())
        .map(|| warp::reply::with_status("up", warp::http::StatusCode::OK))
        .or(sigma_pg::health::warp::health_routes(
            "cart",
            Some(health_pool),
        ))
        .or(web::routes(with_store(store.clone())))
        .or(api::routes(with_store(store)))
        .or(sigma_theme::warp::static_files())
        .or(sigma_theme::warp::favicon())
        .recover(sigma_theme::warp::handle_rejection)
        .with(header("content-security-policy", content_security_policy()))
        .with(header("x-content-type-options", "nosniff"))
        .with(header("x-frame-options", "DENY"))
        .with(header("referrer-policy", "strict-origin-when-cross-origin"))
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
        let res = warp::test::request()
            .method("GET")
            .path("/up")
            .reply(&routes(test_store().await))
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn index_lists_carts() {
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
