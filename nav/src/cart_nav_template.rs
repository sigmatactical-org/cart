//! [`CartNavTemplate`].

use askama::Template;

#[derive(Template)]
#[template(path = "cart_nav.html")]
pub(crate) struct CartNavTemplate<'a> {
    pub(crate) cart_url: &'a str,
    pub(crate) cart_count: u32,
}
