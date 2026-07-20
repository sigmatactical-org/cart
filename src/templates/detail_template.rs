//! [`DetailTemplate`].

use super::{CatalogSkuRef, LineRow, UserRef};
use crate::model::Cart;
use askama::Template;
use sigma_theme::nav::SiteHeader;

#[derive(Template)]
#[template(path = "detail.html")]
pub(crate) struct DetailTemplate {
    pub(crate) cart: Cart,
    pub(crate) user_id: String,
    pub(crate) status: String,
    pub(crate) note: String,
    pub(crate) status_label: &'static str,
    pub(crate) user_display: String,
    pub(crate) line_rows: Vec<LineRow>,
    pub(crate) identity_users: Vec<UserRef>,
    pub(crate) catalog_skus: Vec<CatalogSkuRef>,
    pub(crate) line_sku_id: String,
    pub(crate) line_quantity: String,
    pub(crate) cart_open: bool,
    pub(crate) error: Option<String>,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) copyright_years: String,
}
