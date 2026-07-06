/// PostgreSQL connection URL (shared Sigma database).
#[must_use]
pub fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| sigma_pg::DEFAULT_DATABASE_URL.to_string())
}

/// Base URL of the catalog service (e.g. `http://127.0.0.1:8081/`).
#[must_use]
pub fn catalog_base_url() -> Option<String> {
    std::env::var("CART_CATALOG_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            let mut url = s.trim().to_string();
            if !url.ends_with('/') {
                url.push('/');
            }
            url
        })
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
    std::env::var("CART_STORE_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
}

/// Whether store price integration is configured.
#[must_use]
pub fn store_configured() -> bool {
    store_base_url().is_some()
}

/// Canonical public URL of this cart service (e.g. `http://127.0.0.1:8084/`).
#[must_use]
pub fn public_base_url() -> String {
    std::env::var("CART_PUBLIC_BASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:8084/".to_string())
}

/// Public base URL of the identity BFF (e.g. `http://127.0.0.1:3000/`).
#[must_use]
pub fn identity_public_base_url() -> String {
    std::env::var("CART_IDENTITY_PUBLIC_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:3000/".to_string())
}

/// Browser origin of the identity BFF for CSP `connect-src` (no trailing slash).
#[must_use]
pub fn identity_public_origin() -> String {
    identity_public_base_url().trim_end_matches('/').to_string()
}

/// Public base URL of the contact service for the cart navbar link.
#[must_use]
pub fn contact_public_base_url() -> String {
    std::env::var("CART_CONTACT_PUBLIC_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:8083/".to_string())
}

/// Public base URL of the store, for the "keep shopping" navbar link.
#[must_use]
pub fn store_public_base_url() -> String {
    std::env::var("CART_STORE_PUBLIC_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| normalize_base_url(&s))
        .unwrap_or_else(|| "http://127.0.0.1:8082/".to_string())
}

/// Public store URL for a product detail page (`/products/{sku_code}`).
#[must_use]
pub fn store_product_url(sku_code: &str) -> String {
    format!(
        "{}products/{sku_code}",
        store_public_base_url().trim_end_matches('/')
    )
}

/// Optional cookie `Domain` for the guest-cart cookie so it is shared with the
/// storefront across sibling subdomains (e.g. `.sigmatacticalgroup.com`). Unset
/// in local development, where all apps share `localhost`.
#[must_use]
pub fn cookie_domain() -> Option<String> {
    std::env::var("CART_COOKIE_DOMAIN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn normalize_base_url(url: &str) -> String {
    let mut url = url.trim().to_string();
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// OIDC issuer URL for the identity provider (Keycloak realm URL).
#[must_use]
pub fn identity_issuer_url() -> Option<String> {
    std::env::var("CART_IDENTITY_ISSUER_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Service-account client id for Keycloak Admin API access.
#[must_use]
pub fn identity_client_id() -> Option<String> {
    std::env::var("CART_IDENTITY_CLIENT_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Service-account client secret for Keycloak Admin API access.
#[must_use]
pub fn identity_client_secret() -> Option<String> {
    std::env::var("CART_IDENTITY_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Whether identity user lookup is configured.
#[must_use]
pub fn identity_configured() -> bool {
    identity_issuer_url().is_some()
        && identity_client_id().is_some()
        && identity_client_secret().is_some()
}
