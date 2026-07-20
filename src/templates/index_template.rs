//! [`IndexTemplate`].

use super::CartRow;
use askama::Template;
use sigma_theme::nav::SiteHeader;

#[derive(Template)]
#[template(path = "index.html")]
pub(crate) struct IndexTemplate {
    pub(crate) rows: Vec<CartRow>,
    pub(crate) catalog_configured: bool,
    pub(crate) identity_configured: bool,
    pub(crate) catalog_error: Option<String>,
    pub(crate) identity_error: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) copyright_years: String,
}
