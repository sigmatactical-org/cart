//! [`CheckoutTemplate`].

#[allow(unused_imports)]
use super::*;
use askama::Template;
use sigma_theme::nav::SiteHeader;

/// Full-page checkout: address/payment selection and terms acceptance.
#[derive(Template)]
#[template(path = "checkout.html")]
pub(crate) struct CheckoutTemplate {
    pub(crate) lines: Vec<CheckoutLineRow>,
    pub(crate) subtotal_display: String,
    pub(crate) deposit_display: String,
    pub(crate) billing_addresses: Vec<CheckoutOption>,
    pub(crate) shipping_addresses: Vec<CheckoutOption>,
    pub(crate) payment_methods: Vec<CheckoutOption>,
    pub(crate) has_billing: bool,
    pub(crate) has_shipping: bool,
    pub(crate) has_payment_methods: bool,
    pub(crate) ready: bool,
    pub(crate) error: String,
    pub(crate) addresses_url: String,
    pub(crate) payments_url: String,
    pub(crate) terms_url: String,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) copyright_years: String,
}
