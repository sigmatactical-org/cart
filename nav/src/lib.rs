//! Reusable Sigma cart navbar widget: a cart icon with an item-count badge,
//! linking to a cart URL. Shared across Sigma web services (store, cart, ...)
//! so the cart affordance looks identical everywhere.

use askama::Template;

#[derive(Template)]
#[template(path = "cart_nav.html")]
struct CartNavTemplate<'a> {
    cart_url: &'a str,
    cart_count: u32,
}

/// Render the cart icon and item-count badge linking to `cart_url`.
///
/// When `cart_count` is zero the badge is omitted.
///
/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_cart_nav(cart_url: &str, cart_count: u32) -> Result<String, askama::Error> {
    CartNavTemplate {
        cart_url,
        cart_count,
    }
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_link_without_badge_when_empty() {
        let html = render_cart_nav("http://cart.example/", 0).expect("render");
        assert!(html.contains("href=\"http://cart.example/\""));
        assert!(html.contains("aria-label=\"Cart\""));
        assert!(!html.contains("badge"));
    }

    #[test]
    fn renders_badge_with_count() {
        let html = render_cart_nav("/", 3).expect("render");
        assert!(html.contains("href=\"/\""));
        assert!(html.contains("aria-label=\"Cart (3 items)\""));
        assert!(html.contains(">3</span>"));
    }
}
