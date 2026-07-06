use askama::Template;

use crate::catalog::CatalogSku;
use crate::config;
use crate::identity::IdentityUser;
use crate::model::{
    Cart, CartStatus, Reservation, deposit_cents_for_price, format_price_cents, status_label,
};
use crate::storefront::PriceBook;
use sigma_identity_nav::{auth_links, render_app_site_nav};
use sigma_theme::copyright_years;

fn keep_shopping_link(store_url: &str) -> String {
    format!(
        r#"<a class="btn btn-outline-light btn-sm" href="{store_url}">Keep shopping</a>"#
    )
}

fn site_nav(
    return_path: &str,
    cart_count: u32,
    show_contact_us: bool,
    leading_html: &str,
) -> Result<String, askama::Error> {
    render_app_site_nav(
        &config::identity_public_base_url(),
        &config::public_base_url(),
        &config::contact_public_base_url(),
        "/",
        cart_count,
        return_path,
        show_contact_us,
        leading_html,
    )
}

fn storefront_site_nav(cart_count: u32) -> Result<String, askama::Error> {
    site_nav(
        "/",
        cart_count,
        true,
        &keep_shopping_link(&config::store_public_base_url()),
    )
}

fn admin_site_nav(return_path: &str) -> Result<String, askama::Error> {
    site_nav(return_path, 0, true, "")
}

/// Public shopping cart view: line items, quantity steppers, totals, and the
/// "pay deposit to reserve" action.
#[derive(Template)]
#[template(path = "storefront_cart.html")]
struct StorefrontCartTemplate {
    lines: Vec<PublicLineRow>,
    has_items: bool,
    has_priced_items: bool,
    subtotal_display: String,
    deposit_display: String,
    site_nav: String,
    sign_in_url: String,
    identity_base_url: String,
    store_url: String,
    copyright_years: String,
}

/// Confirmation page shown after a shopper reserves by paying the deposit.
#[derive(Template)]
#[template(path = "reserved.html")]
struct ReservedTemplate {
    reservation_id: String,
    username: String,
    lines: Vec<ReservedLineRow>,
    subtotal_display: String,
    deposit_display: String,
    site_nav: String,
    store_url: String,
    copyright_years: String,
}

/// A resolved, priced line for the public cart view.
pub struct PublicLineRow {
    pub line_id: String,
    pub sku_code: String,
    pub name: String,
    pub product_url: String,
    pub quantity: u32,
    pub unit_price_display: String,
    pub line_total_display: String,
    pub priced: bool,
}

pub struct ReservedLineRow {
    pub sku_code: String,
    pub name: String,
    pub product_url: String,
    pub quantity: u32,
    pub line_total_display: String,
}

/// A cart line joined with its catalog SKU and store price.
pub struct PricedLine {
    pub line_id: String,
    pub sku_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub unit_price_cents: u64,
    pub in_catalog: bool,
}

/// Join cart lines with catalog SKUs and store prices. Lines without a known
/// price are still returned (so shoppers can see/remove them) but flagged.
#[must_use]
pub fn priced_lines(cart: &Cart, skus: &[CatalogSku], prices: &PriceBook) -> Vec<PricedLine> {
    cart.lines
        .iter()
        .map(|line| {
            let sku = skus.iter().find(|s| s.id == line.sku_id);
            let (sku_code, name, in_catalog) = match sku {
                Some(s) => (s.sku_code.clone(), s.name.clone(), true),
                None => (line.sku_id.clone(), "—".to_string(), false),
            };
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
                line_id: l.line_id,
                sku_code: l.sku_code.clone(),
                name: l.name,
                product_url: l
                    .in_catalog
                    .then(|| config::store_product_url(&l.sku_code))
                    .unwrap_or_default(),
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
        &config::contact_public_base_url(),
        "/",
    );
    StorefrontCartTemplate {
        has_items: !lines.is_empty(),
        has_priced_items,
        lines,
        subtotal_display: format_price_cents(subtotal_cents),
        deposit_display: format_price_cents(deposit_cents_for_price(subtotal_cents)),
        site_nav: storefront_site_nav(cart_count)?,
        sign_in_url: links.sign_in_url,
        identity_base_url: links.identity_base_url,
        store_url: config::store_public_base_url(),
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_reserved_html(reservation: &Reservation) -> Result<String, askama::Error> {
    let lines: Vec<ReservedLineRow> = reservation
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
    ReservedTemplate {
        reservation_id: reservation.id.clone(),
        username: reservation.username.clone(),
        lines,
        subtotal_display: format_price_cents(reservation.subtotal_cents),
        deposit_display: format_price_cents(reservation.deposit_cents),
        site_nav: storefront_site_nav(0)?,
        store_url: config::store_public_base_url(),
        copyright_years: copyright_years(),
    }
    .render()
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    rows: Vec<CartRow>,
    catalog_configured: bool,
    identity_configured: bool,
    catalog_error: Option<String>,
    identity_error: Option<String>,
    message: Option<String>,
    site_nav: String,
    copyright_years: String,
}

#[derive(Template)]
#[template(path = "form.html")]
struct FormTemplate {
    cart: Option<Cart>,
    user_id: String,
    status: String,
    note: String,
    identity_users: Vec<UserRef>,
    error: Option<String>,
    site_nav: String,
    copyright_years: String,
}

#[derive(Template)]
#[template(path = "detail.html")]
struct DetailTemplate {
    cart: Cart,
    user_id: String,
    status: String,
    note: String,
    status_label: String,
    user_display: String,
    line_rows: Vec<LineRow>,
    identity_users: Vec<UserRef>,
    catalog_skus: Vec<CatalogSkuRef>,
    line_sku_id: String,
    line_quantity: String,
    cart_open: bool,
    error: Option<String>,
    site_nav: String,
    copyright_years: String,
}

pub struct CartRow {
    pub cart: Cart,
    pub user_display: String,
    pub status_label: String,
    pub line_count: usize,
    pub missing_user: bool,
}

pub struct LineRow {
    pub line_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub missing_catalog: bool,
}

pub struct UserRef {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
}

pub struct CatalogSkuRef {
    pub id: String,
    pub sku_code: String,
    pub name: String,
}

pub struct CartFormValues {
    pub user_id: String,
    pub status: String,
    pub note: String,
}

#[derive(Default)]
pub struct LineFormValues {
    pub sku_id: String,
    pub quantity: String,
}

pub struct IndexContext<'a> {
    pub catalog_skus: &'a [CatalogSku],
    pub identity_users: &'a [IdentityUser],
    pub catalog_configured: bool,
    pub identity_configured: bool,
    pub catalog_error: Option<String>,
    pub identity_error: Option<String>,
    pub message: Option<String>,
}

impl CartFormValues {
    pub fn from_cart(cart: &Cart) -> Self {
        Self {
            user_id: cart.user_id.clone().unwrap_or_default(),
            status: status_to_form(cart.status),
            note: cart.note.clone().unwrap_or_default(),
        }
    }
}

fn status_to_form(status: CartStatus) -> String {
    match status {
        CartStatus::Open => "open".to_string(),
        CartStatus::Submitted => "submitted".to_string(),
        CartStatus::Cancelled => "cancelled".to_string(),
    }
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
                cart: cart.clone(),
                user_display,
                status_label: status_label(cart.status).to_string(),
                line_count: cart.lines.len(),
                missing_user,
            }
        })
        .collect()
}

fn line_rows(cart: &Cart, skus: &[CatalogSku]) -> Vec<LineRow> {
    cart.lines
        .iter()
        .map(|line| {
            let sku = skus.iter().find(|s| s.id == line.sku_id);
            let (sku_code, name, missing_catalog) = match sku {
                Some(s) => (s.sku_code.clone(), s.name.clone(), false),
                None => (line.sku_id.clone(), "—".to_string(), !skus.is_empty()),
            };
            LineRow {
                line_id: line.id.clone(),
                sku_code,
                name,
                quantity: line.quantity,
                missing_catalog,
            }
        })
        .collect()
}

fn render_form(
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
        site_nav: admin_site_nav(&return_path)?,
        copyright_years: copyright_years(),
    }
    .render()
}

fn render_detail(
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
        status_label: status_label(cart.status).to_string(),
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
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_index_html(carts: Vec<Cart>, ctx: IndexContext<'_>) -> Result<String, askama::Error> {
    let _ = ctx.catalog_skus;
    IndexTemplate {
        rows: cart_rows(&carts, ctx.identity_users),
        catalog_configured: ctx.catalog_configured,
        identity_configured: ctx.identity_configured,
        catalog_error: ctx.catalog_error,
        identity_error: ctx.identity_error,
        message: ctx.message,
        site_nav: admin_site_nav("/admin")?,
        copyright_years: copyright_years(),
    }
    .render()
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_cart_form_html(
    _carts: Vec<Cart>,
    identity_users: &[IdentityUser],
    cart: Option<Cart>,
    error: Option<String>,
) -> Result<String, askama::Error> {
    let values = cart
        .as_ref()
        .map(CartFormValues::from_cart)
        .unwrap_or(CartFormValues {
            user_id: String::new(),
            status: "open".to_string(),
            note: String::new(),
        });
    render_form(identity_users, cart, error, values)
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_cart_form_html_with_values(
    _carts: Vec<Cart>,
    identity_users: &[IdentityUser],
    cart: Option<Cart>,
    error: Option<String>,
    values: CartFormValues,
) -> Result<String, askama::Error> {
    render_form(identity_users, cart, error, values)
}

/// # Errors
///
/// Returns [`askama::Error`] when template rendering fails.
pub fn render_detail_html(
    cart: Cart,
    catalog_skus: &[CatalogSku],
    identity_users: &[IdentityUser],
    error: Option<String>,
    line_error: Option<String>,
) -> Result<String, askama::Error> {
    let _ = line_error;
    render_detail(
        cart.clone(),
        catalog_skus,
        identity_users,
        error,
        CartFormValues::from_cart(&cart),
        LineFormValues::default(),
    )
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
    render_detail(
        cart,
        catalog_skus,
        identity_users,
        error,
        cart_values,
        line_values,
    )
}
