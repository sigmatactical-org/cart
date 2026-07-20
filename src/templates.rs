//! Askama page rendering for the storefront cart, checkout, and admin UI.

mod cart_form_values;
mod cart_row;
mod catalog_sku_ref;
mod checkout_line_row;
mod checkout_option;
mod checkout_template;
mod detail_template;
mod form_template;
mod index_context;
mod index_template;
mod line_form_values;
mod line_row;
mod priced_line;
mod public_line_row;
mod reserved_line_row;
mod reserved_template;
mod storefront_cart_template;
mod user_ref;
pub use cart_form_values::CartFormValues;
pub use cart_row::CartRow;
pub use catalog_sku_ref::CatalogSkuRef;
pub use checkout_line_row::CheckoutLineRow;
pub use checkout_option::CheckoutOption;
pub(crate) use checkout_template::CheckoutTemplate;
pub(crate) use detail_template::DetailTemplate;
pub(crate) use form_template::FormTemplate;
pub use index_context::IndexContext;
pub(crate) use index_template::IndexTemplate;
pub use line_form_values::LineFormValues;
pub use line_row::LineRow;
pub use priced_line::PricedLine;
pub use public_line_row::PublicLineRow;
pub use reserved_line_row::ReservedLineRow;
pub(crate) use reserved_template::ReservedTemplate;
pub(crate) use storefront_cart_template::StorefrontCartTemplate;
pub use user_ref::UserRef;

use std::collections::HashMap;

use askama::Template;
use sigma_identity_nav::auth_links;
use sigma_pg::clients::orders::Order;
use sigma_pg::money::{deposit_cents_for_price, format_price_cents};
use sigma_theme::copyright_years;
use sigma_theme::nav::{Breadcrumb, SiteHeader, site_menu};
use sigma_theme::site_nav::{AppSiteNav, render_app_site_nav};

use crate::catalog::CatalogSku;
use crate::config;
use crate::identity::IdentityUser;
use crate::model::{Cart, CartStatus};
use crate::storefront::PriceBook;

fn page_header() -> SiteHeader {
    SiteHeader::new("Cart").with_menu(site_menu(None))
}

fn storefront_page_header(store_url: &str) -> SiteHeader {
    page_header()
        .with_breadcrumb(Breadcrumb::link(store_url, "Store"))
        .with_breadcrumb(Breadcrumb::current("Cart"))
}

fn checkout_page_header(store_url: &str) -> SiteHeader {
    page_header()
        .with_breadcrumb(Breadcrumb::link(store_url, "Store"))
        .with_breadcrumb(Breadcrumb::link("/", "Cart"))
        .with_breadcrumb(Breadcrumb::current("Checkout"))
}

fn site_nav(
    return_path: &str,
    cart_count: u32,
    show_contact_us: bool,
) -> Result<String, askama::Error> {
    render_app_site_nav(&AppSiteNav {
        identity_base: &config::identity_public_base_url(),
        app_base: &config::public_base_url(),
        contact_base: &config::contact_public_base_url(),
        cart_url: &config::public_base_url(),
        cart_count,
        return_path,
        show_cart: true,
        show_contact_us,
        leading_html: "",
    })
}

fn storefront_site_nav(cart_count: u32) -> Result<String, askama::Error> {
    site_nav("/", cart_count, false)
}

fn admin_site_nav(return_path: &str) -> Result<String, askama::Error> {
    site_nav(return_path, 0, false)
}

/// Index catalog SKUs by id so line joins are O(n + m), not O(n·m).
fn sku_index(skus: &[CatalogSku]) -> HashMap<&str, &CatalogSku> {
    skus.iter().map(|sku| (sku.id.as_str(), sku)).collect()
}

/// Display code and name for a cart line, falling back to the raw SKU id when
/// the catalog does not know it. The `bool` is whether the SKU was resolved.
fn sku_display(sku: Option<&&CatalogSku>, sku_id: &str) -> (String, String, bool) {
    match sku {
        Some(sku) => (sku.sku_code.clone(), sku.name.clone(), true),
        None => (sku_id.to_string(), "—".to_string(), false),
    }
}

/// Join cart lines with catalog SKUs and store prices. Lines without a known
/// price are still returned (so shoppers can see/remove them) but flagged.
#[must_use]
pub fn priced_lines(cart: &Cart, skus: &[CatalogSku], prices: &PriceBook) -> Vec<PricedLine> {
    let index = sku_index(skus);
    cart.lines
        .iter()
        .map(|line| {
            let (sku_code, name, in_catalog) =
                sku_display(index.get(line.sku_id.as_str()), &line.sku_id);
            PricedLine {
                line_id: line.id.clone(),
                sku_id: line.sku_id.clone(),
                sku_code,
                name,
                quantity: line.quantity,
                unit_price_cents: prices.unit_price_cents(&line.sku_id).unwrap_or(0),
                in_catalog,
            }
        })
        .collect()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_storefront_cart_html(
    cart: Option<&Cart>,
    skus: &[CatalogSku],
    prices: &PriceBook,
) -> Result<String, askama::Error> {
    let priced: Vec<PricedLine> = cart
        .map(|c| priced_lines(c, skus, prices))
        .unwrap_or_default();
    let cart_count: u32 = priced.iter().map(|l| l.quantity).sum();
    let subtotal_cents: u64 = priced
        .iter()
        .map(|l| l.unit_price_cents * u64::from(l.quantity))
        .sum();
    let has_priced_items = priced.iter().any(|l| l.unit_price_cents > 0);
    let lines: Vec<PublicLineRow> = priced
        .into_iter()
        .map(|l| {
            let line_total_cents = l.unit_price_cents * u64::from(l.quantity);
            let priced = l.unit_price_cents > 0;
            PublicLineRow {
                product_url: if l.in_catalog {
                    config::store_product_url(&l.sku_code)
                } else {
                    String::new()
                },
                line_id: l.line_id,
                sku_code: l.sku_code,
                name: l.name,
                quantity: l.quantity,
                unit_price_display: if priced {
                    format_price_cents(l.unit_price_cents)
                } else {
                    "—".to_string()
                },
                line_total_display: if priced {
                    format_price_cents(line_total_cents)
                } else {
                    "—".to_string()
                },
                priced,
            }
        })
        .collect();
    let links = auth_links(
        &config::identity_public_base_url(),
        &config::public_base_url(),
        "/",
    );
    let store_url = config::store_public_base_url();
    StorefrontCartTemplate {
        has_items: !lines.is_empty(),
        has_priced_items,
        lines,
        subtotal_display: format_price_cents(subtotal_cents),
        deposit_display: format_price_cents(deposit_cents_for_price(subtotal_cents)),
        site_header: storefront_page_header(&store_url),
        site_nav: storefront_site_nav(cart_count)?,
        sign_in_url: links.sign_in_url,
        identity_base_url: links.identity_base_url,
        store_url,
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_reserved_html(order: &Order) -> Result<String, askama::Error> {
    let lines: Vec<ReservedLineRow> = order
        .lines
        .iter()
        .map(|l| ReservedLineRow {
            sku_code: l.sku_code.clone(),
            name: l.name.clone(),
            product_url: config::store_product_url(&l.sku_code),
            quantity: l.quantity,
            line_total_display: format_price_cents(l.line_total_cents),
        })
        .collect();
    let store_url = config::store_public_base_url();
    ReservedTemplate {
        order_id: order.id.clone(),
        username: order.username.clone(),
        lines,
        subtotal_display: format_price_cents(order.subtotal_cents),
        deposit_display: format_price_cents(order.deposit_cents),
        site_header: storefront_page_header(&store_url),
        site_nav: storefront_site_nav(0)?,
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_checkout_html(
    lines: &[PricedLine],
    billing_addresses: Vec<CheckoutOption>,
    shipping_addresses: Vec<CheckoutOption>,
    payment_methods: Vec<CheckoutOption>,
    error: &str,
) -> Result<String, askama::Error> {
    let priced = lines.iter().filter(|l| l.unit_price_cents > 0);
    let checkout_lines: Vec<CheckoutLineRow> = priced
        .clone()
        .map(|l| CheckoutLineRow {
            name: l.name.clone(),
            quantity: l.quantity,
            line_total_display: format_price_cents(
                l.unit_price_cents.saturating_mul(u64::from(l.quantity)),
            ),
        })
        .collect();
    let subtotal: u64 = priced
        .map(|l| l.unit_price_cents.saturating_mul(u64::from(l.quantity)))
        .sum();
    let has_billing = !billing_addresses.is_empty();
    let has_shipping = !shipping_addresses.is_empty();
    let has_payment_methods = !payment_methods.is_empty();
    let store_url = config::store_public_base_url();
    CheckoutTemplate {
        lines: checkout_lines,
        subtotal_display: format_price_cents(subtotal),
        deposit_display: format_price_cents(deposit_cents_for_price(subtotal)),
        billing_addresses,
        shipping_addresses,
        payment_methods,
        has_billing,
        has_shipping,
        has_payment_methods,
        ready: has_billing && has_shipping && has_payment_methods,
        error: error.to_string(),
        addresses_url: config::addresses_public_base_url(),
        payments_url: config::payments_public_base_url(),
        terms_url: config::terms_url(),
        site_header: checkout_page_header(&store_url),
        site_nav: storefront_site_nav(u32::try_from(lines.len()).unwrap_or(0))?,
        copyright_years: copyright_years(),
    }
    .render()
}

fn user_refs(users: &[IdentityUser]) -> Vec<UserRef> {
    users
        .iter()
        .map(|u| UserRef {
            id: u.id.clone(),
            display_name: u.display_name.clone(),
            email: u.email.clone(),
        })
        .collect()
}

fn catalog_sku_refs(skus: &[CatalogSku]) -> Vec<CatalogSkuRef> {
    skus.iter()
        .filter(|s| s.active)
        .map(|s| CatalogSkuRef {
            id: s.id.clone(),
            sku_code: s.sku_code.clone(),
            name: s.name.clone(),
        })
        .collect()
}

fn resolve_user_display(cart: &Cart, users: &[IdentityUser]) -> (String, bool) {
    let Some(user_id) = cart.user_id.as_deref() else {
        return ("—".to_string(), false);
    };
    match users.iter().find(|u| u.id == user_id) {
        Some(user) => (user.display_name.clone(), false),
        None if users.is_empty() => (user_id.to_string(), false),
        None => (user_id.to_string(), true),
    }
}

fn cart_rows(carts: &[Cart], users: &[IdentityUser]) -> Vec<CartRow> {
    carts
        .iter()
        .map(|cart| {
            let (user_display, missing_user) = resolve_user_display(cart, users);
            CartRow {
                id: cart.id.clone(),
                updated_at: cart.updated_at.clone(),
                user_display,
                status_label: cart.status.label(),
                line_count: cart.lines.len(),
                missing_user,
            }
        })
        .collect()
}

fn line_rows(cart: &Cart, skus: &[CatalogSku]) -> Vec<LineRow> {
    let index = sku_index(skus);
    cart.lines
        .iter()
        .map(|line| {
            let (sku_code, name, resolved) =
                sku_display(index.get(line.sku_id.as_str()), &line.sku_id);
            LineRow {
                line_id: line.id.clone(),
                sku_code,
                name,
                quantity: line.quantity,
                missing_catalog: !resolved && !skus.is_empty(),
            }
        })
        .collect()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_index_html(carts: Vec<Cart>, ctx: IndexContext<'_>) -> Result<String, askama::Error> {
    IndexTemplate {
        rows: cart_rows(&carts, ctx.identity_users),
        catalog_configured: ctx.catalog_configured,
        identity_configured: ctx.identity_configured,
        catalog_error: ctx.catalog_error,
        identity_error: ctx.identity_error,
        message: ctx.message,
        site_header: page_header(),
        site_nav: admin_site_nav("/admin")?,
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_cart_form_html_with_values(
    identity_users: &[IdentityUser],
    cart: Option<Cart>,
    error: Option<String>,
    values: CartFormValues,
) -> Result<String, askama::Error> {
    let return_path = cart
        .as_ref()
        .map(|entry| format!("/admin/carts/{}/edit", entry.id))
        .unwrap_or_else(|| "/admin/carts/new".to_string());
    FormTemplate {
        cart,
        user_id: values.user_id,
        status: values.status,
        note: values.note,
        identity_users: user_refs(identity_users),
        error,
        site_header: page_header(),
        site_nav: admin_site_nav(&return_path)?,
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_detail_html_with_values(
    cart: Cart,
    catalog_skus: &[CatalogSku],
    identity_users: &[IdentityUser],
    error: Option<String>,
    cart_values: CartFormValues,
    line_values: LineFormValues,
) -> Result<String, askama::Error> {
    let (user_display, _) = resolve_user_display(&cart, identity_users);
    let site_nav = admin_site_nav(&format!("/admin/carts/{}", cart.id))?;
    DetailTemplate {
        cart_open: cart.status == CartStatus::Open,
        status_label: cart.status.label(),
        user_display,
        line_rows: line_rows(&cart, catalog_skus),
        identity_users: user_refs(identity_users),
        catalog_skus: catalog_sku_refs(catalog_skus),
        user_id: cart_values.user_id,
        status: cart_values.status,
        note: cart_values.note,
        line_sku_id: line_values.sku_id,
        line_quantity: line_values.quantity,
        cart,
        error,
        site_nav,
        site_header: page_header(),
        copyright_years: copyright_years(),
    }
    .render()
}
