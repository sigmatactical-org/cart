//! Identity (Keycloak) user lookups, cached per process.
//!
//! The admin pages resolve user display names on nearly every request, and the
//! Keycloak Admin API needs a client-credentials token round trip per call, so
//! the list is served from a short TTL cache with stale fallback.

use std::sync::Arc;
use std::time::Duration;

use sigma_theme::cache::TtlCache;

pub use sigma_pg::clients::identity::{IdentityError, IdentityUser, user_by_id};

const USERS_TTL: Duration = Duration::from_secs(60);

static USERS: TtlCache<Vec<IdentityUser>> = TtlCache::new();

pub async fn fetch_users() -> Result<Arc<Vec<IdentityUser>>, IdentityError> {
    USERS
        .get_or_fetch(USERS_TTL, || async {
            let issuer_url = crate::config::identity_issuer_url();
            let client_id = crate::config::identity_client_id();
            let client_secret = crate::config::identity_client_secret();
            sigma_pg::clients::identity::fetch_users(
                issuer_url.as_deref(),
                client_id.as_deref(),
                client_secret.as_deref(),
            )
            .await
        })
        .await
}
