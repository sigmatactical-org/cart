pub use sigma_pg::clients::identity::{IdentityError, IdentityUser};

pub async fn fetch_users() -> Result<Vec<IdentityUser>, IdentityError> {
    sigma_pg::clients::identity::fetch_users(
        crate::config::identity_issuer_url().as_deref(),
        crate::config::identity_client_id().as_deref(),
        crate::config::identity_client_secret().as_deref(),
    )
    .await
}

pub use sigma_pg::clients::identity::user_by_id;
