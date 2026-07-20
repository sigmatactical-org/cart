//! Environment-driven configuration, read once per process.

use std::sync::OnceLock;

use sigma_pg::clients::http::{env_url, normalize_base_url};

fn cached(cell: &'static OnceLock<String>, init: impl FnOnce() -> String) -> String {
    cell.get_or_init(init).clone()
}

fn cached_opt(
    cell: &'static OnceLock<Option<String>>,
    init: impl FnOnce() -> Option<String>,
) -> Option<String> {
    cell.get_or_init(init).clone()
}

fn env_opt(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn env_opt_url(var: &str) -> Option<String> {
    env_opt(var).map(|s| normalize_base_url(&s))
}

/// Base URL of the catalog service (e.g. `http://127.0.0.1:8081/`).
#[must_use]
pub fn catalog_base_url() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt_url("CART_CATALOG_BASE_URL"))
}

/// Whether catalog integration is configured.
#[must_use]
pub fn catalog_configured() -> bool {
    catalog_base_url().is_some()
}

/// Base URL of the store service used to resolve authoritative listing prices
/// (e.g. `http://127.0.0.1:8082/`). Prices live on store listings, not the
/// catalog, so the cart reads them from the store's `/items` endpoint.
#[must_use]
pub fn store_base_url() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt_url("CART_STORE_BASE_URL"))
}

/// Canonical public URL of this cart service (e.g. `http://127.0.0.1:8084/`).
#[must_use]
pub fn public_base_url() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    cached(&CELL, || {
        env_url("CART_PUBLIC_BASE_URL", "http://127.0.0.1:8084/")
    })
}

/// Public base URL of the identity BFF (e.g. `http://127.0.0.1:3000/`).
#[must_use]
pub fn identity_public_base_url() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    cached(&CELL, || {
        env_url("CART_IDENTITY_PUBLIC_URL", "http://127.0.0.1:3000/")
    })
}

/// Browser origin of the identity BFF for CSP `connect-src` (no trailing slash).
#[must_use]
pub fn identity_public_origin() -> String {
    identity_public_base_url().trim_end_matches('/').to_string()
}

/// Base URL for server-to-server calls to the identity BFF (e.g. session
/// status checks during reserve). Must be reachable from this pod, unlike
/// `identity_public_base_url`, which is the browser-facing ingress host and
/// does not resolve back to identity from inside the cluster network.
/// Falls back to the public URL for non-cluster local dev.
#[must_use]
pub fn identity_internal_base_url() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    cached(&CELL, || {
        env_opt_url("CART_IDENTITY_INTERNAL_URL").unwrap_or_else(identity_public_base_url)
    })
}

/// Public base URL of the contact service for the cart navbar link.
#[must_use]
pub fn contact_public_base_url() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    cached(&CELL, || {
        env_url("CART_CONTACT_PUBLIC_URL", "http://127.0.0.1:8083/")
    })
}

/// Public base URL of the store for product links and continue-shopping navigation.
#[must_use]
pub fn store_public_base_url() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    cached(&CELL, || {
        env_url("CART_STORE_PUBLIC_URL", "http://127.0.0.1:8082/")
    })
}

/// Base URL of the orders service (e.g. `http://127.0.0.1:8085/`).
#[must_use]
pub fn orders_base_url() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt_url("CART_ORDERS_BASE_URL"))
}

/// Cluster-internal addresses service URL for checkout address lists.
#[must_use]
pub fn addresses_internal_base_url() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt_url("CART_ADDRESSES_INTERNAL_URL"))
}

/// Public addresses URL for “add address” links on checkout.
#[must_use]
pub fn addresses_public_base_url() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    cached(&CELL, || {
        env_url("CART_ADDRESSES_PUBLIC_URL", "http://127.0.0.1:8089/")
    })
}

/// Cluster-internal accounting service URL for recording checkout deposit
/// receipts. Unset skips the receipt push entirely.
#[must_use]
pub fn accounting_internal_base_url() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt_url("CART_ACCOUNTING_INTERNAL_URL"))
}

/// Cluster-internal payments service URL for methods + charges.
#[must_use]
pub fn payments_internal_base_url() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt_url("CART_PAYMENTS_INTERNAL_URL"))
}

/// Public payments URL for “add payment method” links on checkout.
#[must_use]
pub fn payments_public_base_url() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    cached(&CELL, || {
        env_url("CART_PAYMENTS_PUBLIC_URL", "http://127.0.0.1:8090/")
    })
}

/// Public info site URL for Terms and Conditions (`/doc/terms`).
#[must_use]
pub fn info_public_base_url() -> String {
    static CELL: OnceLock<String> = OnceLock::new();
    cached(&CELL, || {
        env_url("CART_INFO_PUBLIC_URL", "http://127.0.0.1:8085/")
    })
}

#[must_use]
pub fn terms_url() -> String {
    format!("{}doc/terms", info_public_base_url())
}

/// Public store URL for a product detail page (`/products/{sku_code}`).
#[must_use]
pub fn store_product_url(sku_code: &str) -> String {
    format!(
        "{}/products/{}",
        store_public_base_url().trim_end_matches('/'),
        sku_code.to_lowercase()
    )
}

/// Optional cookie `Domain` for the guest-cart cookie so it is shared with the
/// storefront across sibling subdomains (e.g. `.sigmatacticalgroup.com`). Unset
/// in local development, where all apps share `localhost`.
#[must_use]
pub fn cookie_domain() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt("CART_COOKIE_DOMAIN"))
}

/// OIDC issuer URL for the identity provider (Keycloak realm URL).
#[must_use]
pub fn identity_issuer_url() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt("CART_IDENTITY_ISSUER_URL"))
}

/// Service-account client id for Keycloak Admin API access.
#[must_use]
pub fn identity_client_id() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt("CART_IDENTITY_CLIENT_ID"))
}

/// Service-account client secret for Keycloak Admin API access.
#[must_use]
pub fn identity_client_secret() -> Option<String> {
    static CELL: OnceLock<Option<String>> = OnceLock::new();
    cached_opt(&CELL, || env_opt("CART_IDENTITY_CLIENT_SECRET"))
}

/// Whether identity user lookup is configured.
#[must_use]
pub fn identity_configured() -> bool {
    identity_issuer_url().is_some()
        && identity_client_id().is_some()
        && identity_client_secret().is_some()
}
