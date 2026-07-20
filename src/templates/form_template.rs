//! [`FormTemplate`].

use super::UserRef;
use crate::model::Cart;
use askama::Template;
use sigma_theme::nav::SiteHeader;

#[derive(Template)]
#[template(path = "form.html")]
pub(crate) struct FormTemplate {
    pub(crate) cart: Option<Cart>,
    pub(crate) user_id: String,
    pub(crate) status: String,
    pub(crate) note: String,
    pub(crate) identity_users: Vec<UserRef>,
    pub(crate) error: Option<String>,
    pub(crate) site_header: SiteHeader,
    pub(crate) site_nav: String,
    pub(crate) copyright_years: String,
}
