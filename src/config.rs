//! Environment-driven configuration (service URLs, optional integrations).
//!
//! Required values are declared in the [`sigma_config::service!`] block and
//! checked by [`validate_with`] at startup; optional integrations return
//! `None` when they are not configured for this environment.

sigma_config::service! {
    prefix = "CART";
    role = "cart";
    urls {
        /// Canonical public URL of this cart service.
        public_base_url = "PUBLIC_BASE_URL" => "http://127.0.0.1:8084/";
        /// Public base URL of the identity BFF.
        identity_public_base_url = "IDENTITY_PUBLIC_URL" => "http://127.0.0.1:3000/";
        /// Public base URL of the contact service for the cart navbar link.
        contact_public_base_url = "CONTACT_PUBLIC_URL" => "http://127.0.0.1:8083/";
        /// Public base URL of the store for product links and continue-shopping navigation.
        store_public_base_url = "STORE_PUBLIC_URL" => "http://127.0.0.1:8082/";
        /// Public addresses URL for “add address” links on checkout.
        addresses_public_base_url = "ADDRESSES_PUBLIC_URL" => "http://127.0.0.1:8089/";
        /// Public payments URL for “add payment method” links on checkout.
        payments_public_base_url = "PAYMENTS_PUBLIC_URL" => "http://127.0.0.1:8090/";
        /// Public info site URL for Terms and Conditions (`/doc/terms`).
        info_public_base_url = "INFO_PUBLIC_URL" => "http://127.0.0.1:8085/";
    }
}

/// Browser origin of the identity BFF for CSP `connect-src` (no trailing slash).
#[must_use]
pub fn identity_public_origin() -> String {
    sigma_config::origin_of(&identity_public_base_url())
}

/// Base URL of the catalog service (e.g. `http://127.0.0.1:8081/`).
#[must_use]
pub fn catalog_base_url() -> Option<String> {
    SERVICE.opt_url("CATALOG_BASE_URL")
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
    SERVICE.opt_url("STORE_BASE_URL")
}

/// Base URL for server-to-server calls to the identity BFF (e.g. session
/// status checks during reserve). Must be reachable from this pod, unlike
/// `identity_public_base_url`, which is the browser-facing ingress host and
/// does not resolve back to identity from inside the cluster network.
/// Falls back to the public URL for non-cluster local dev.
#[must_use]
pub fn identity_internal_base_url() -> String {
    SERVICE
        .opt_url("IDENTITY_INTERNAL_URL")
        .unwrap_or_else(identity_public_base_url)
}

/// Base URL of the orders service (e.g. `http://127.0.0.1:8085/`).
#[must_use]
pub fn orders_base_url() -> Option<String> {
    SERVICE.opt_url("ORDERS_BASE_URL")
}

/// Cluster-internal addresses service URL for checkout address lists.
#[must_use]
pub fn addresses_internal_base_url() -> Option<String> {
    SERVICE.opt_url("ADDRESSES_INTERNAL_URL")
}

/// Cluster-internal accounting service URL for recording checkout deposit
/// receipts. Unset skips the receipt push entirely.
#[must_use]
pub fn accounting_internal_base_url() -> Option<String> {
    SERVICE.opt_url("ACCOUNTING_INTERNAL_URL")
}

/// Cluster-internal payments service URL for methods + charges.
#[must_use]
pub fn payments_internal_base_url() -> Option<String> {
    SERVICE.opt_url("PAYMENTS_INTERNAL_URL")
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
    SERVICE.opt_str("COOKIE_DOMAIN")
}

/// OIDC issuer URL for the identity provider (Keycloak realm URL).
#[must_use]
pub fn identity_issuer_url() -> Option<String> {
    SERVICE.opt_str("IDENTITY_ISSUER_URL")
}

/// Service-account client id for Keycloak Admin API access.
#[must_use]
pub fn identity_client_id() -> Option<String> {
    SERVICE.opt_str("IDENTITY_CLIENT_ID")
}

/// Service-account client secret for Keycloak Admin API access.
#[must_use]
pub fn identity_client_secret() -> Option<String> {
    SERVICE.opt_str("IDENTITY_CLIENT_SECRET")
}

/// Whether identity user lookup is configured.
#[must_use]
pub fn identity_configured() -> bool {
    identity_issuer_url().is_some()
        && identity_client_id().is_some()
        && identity_client_secret().is_some()
}
