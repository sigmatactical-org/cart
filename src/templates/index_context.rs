//! [`IndexContext`].

#[allow(unused_imports)]
use super::*;
use crate::catalog::CatalogSku;
use crate::identity::IdentityUser;

/// Everything the index page template renders.
pub struct IndexContext<'a> {
    pub catalog_skus: &'a [CatalogSku],
    pub identity_users: &'a [IdentityUser],
    pub catalog_configured: bool,
    pub identity_configured: bool,
    pub catalog_error: Option<String>,
    pub identity_error: Option<String>,
    pub message: Option<String>,
}
