//! [`ReservedTemplate`].

use super::ReservedLineRow;
use askama::Template;
use sigma_theme::nav::SiteHeader;

/// Confirmation page shown after a shopper reserves by paying the deposit.
#[derive(Template)]
#[template(path = "reserved.html")]
pub(crate) struct ReservedTemplate {
    pub(crate) order_id: String,
    pub(crate) username: String,
    pub(crate) lines: Vec<ReservedLineRow>,
    pub(crate) subtotal_display: String,
    pub(crate) deposit_display: String,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) copyright_years: String,
}
