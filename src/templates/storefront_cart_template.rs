//! [`StorefrontCartTemplate`].

#[allow(unused_imports)]
use super::*;
use askama::Template;
use sigma_theme::nav::SiteHeader;

/// Public shopping cart view: line items, quantity steppers, totals, and the
/// "pay deposit to reserve" action.
#[derive(Template)]
#[template(path = "storefront_cart.html")]
pub(crate) struct StorefrontCartTemplate {
    pub(crate) lines: Vec<PublicLineRow>,
    pub(crate) has_items: bool,
    pub(crate) has_priced_items: bool,
    pub(crate) subtotal_display: String,
    pub(crate) deposit_display: String,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) sign_in_url: String,
    pub(crate) identity_base_url: String,
    pub(crate) store_url: String,
    pub(crate) copyright_years: String,
}
