//! Public shopping-cart UI, checkout, and the internal admin pages.

mod checkout_choice;
mod checkout_form;
mod checkout_session;
pub(crate) use checkout_choice::CheckoutChoice;
pub(crate) use checkout_form::CheckoutForm;
pub(crate) use checkout_session::CheckoutSession;

use std::convert::Infallible;

use sigma_pg::clients::addresses::{self, AddressSummary};
use sigma_pg::clients::orders::{self, CreateOrder, CreateOrderLine, OrderStatus};
use sigma_pg::money::deposit_cents_for_price;
use warp::http::StatusCode;
use warp::http::header::{LOCATION, SET_COOKIE};
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::accounting_client;
use crate::catalog;
use crate::identity;
use crate::model::{CartForm, CartStatus, CreateLine, LineForm, UpdateLine};
use crate::payments_client::{self, PaymentMethodSummary};
use crate::store::StoreError;
use crate::storefront;
use crate::templates::{
    self, CartFormValues, CheckoutOption, IndexContext, LineFormValues, PricedLine,
};

/// Cookie tying a browser to its guest cart. Shared with the storefront so it
/// can show a live item count (same host in dev, shared parent domain in prod).
const CART_COOKIE: &str = "sigma_cart";
/// Guest cart cookie lifetime (30 days).
const CART_COOKIE_MAX_AGE: i64 = 60 * 60 * 24 * 30;

/// Build this module's routes.
pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    // Public shopping-cart UI.
    cart_view(store.clone())
        .or(add_to_cart(store.clone()))
        .or(change_line(store.clone()))
        .or(checkout_get(store.clone()))
        .or(checkout_post(store.clone()))
        .or(reserve_redirect())
        // Internal admin UI (reached through the identity proxy in production).
        .or(admin_index(store.clone()))
        .or(admin_new_cart())
        .or(admin_create_cart(store.clone()))
        .or(admin_cart_detail(store.clone()))
        .or(admin_update_cart(store.clone()))
        .or(admin_add_line(store.clone()))
        .or(admin_delete_line(store.clone()))
        .or(admin_delete_cart(store))
}

// ---------------------------------------------------------------------------
// Cookie + redirect helpers
// ---------------------------------------------------------------------------

fn cart_id_from_cookie(cookie_header: Option<&str>) -> Option<String> {
    sigma_pg::clients::cart::cart_id_from_cookie(cookie_header)
}

/// `Set-Cookie` value for the guest cart. `max_age` of 0 clears it.
fn cart_cookie(cart_id: &str, max_age: i64) -> String {
    let mut cookie =
        format!("{CART_COOKIE}={cart_id}; Path=/; HttpOnly; Max-Age={max_age}; SameSite=Lax");
    if crate::config::public_base_url().starts_with("https://") {
        cookie.push_str("; Secure");
    }
    if let Some(domain) = crate::config::cookie_domain() {
        cookie.push_str(&format!("; Domain={domain}"));
    }
    cookie
}

/// 303 redirect, optionally attaching a `Set-Cookie` header.
fn redirect_to(location: &'static str, set_cookie: Option<String>) -> Response {
    let redirect = warp::reply::with_header(warp::reply(), LOCATION, location);
    let redirect = warp::reply::with_status(redirect, StatusCode::SEE_OTHER);
    match set_cookie {
        Some(cookie) => warp::reply::with_header(redirect, SET_COOKIE, cookie).into_response(),
        None => redirect.into_response(),
    }
}

// ---------------------------------------------------------------------------
// Public shopping-cart UI
// ---------------------------------------------------------------------------

/// Public cart view: `GET /`.
fn cart_view(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::end()
        .and(warp::get())
        .and(warp::header::optional::<String>("cookie"))
        .and(store)
        .and_then(|cookie: Option<String>, store: SharedStore| async move {
            let cart = match cart_id_from_cookie(cookie.as_deref()) {
                Some(id) => store.get(&id).await.ok().flatten(),
                None => None,
            };
            let cart = cart.filter(|c| c.status == CartStatus::Open);
            let (catalog_skus, prices) =
                tokio::join!(catalog::fetch_skus(), storefront::fetch_prices());
            templates::render_storefront_cart_html(
                cart.as_ref(),
                &catalog_skus.unwrap_or_default(),
                &prices.unwrap_or_default(),
            )
            .map(warp::reply::html)
            .map_err(|_| warp::reject::not_found())
        })
}

/// Add an item to the cart: `POST /add` (form: `sku_id`). Called cross-site by
/// storefronts; creates a guest cart on first add and sets the cart cookie.
fn add_to_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    #[derive(serde::Deserialize)]
    struct AddForm {
        sku_id: String,
    }

    warp::path("add")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::header::optional::<String>("cookie"))
        .and(warp::body::form())
        .and(store)
        .and_then(
            |cookie: Option<String>, form: AddForm, store: SharedStore| async move {
                let sku_id = form.sku_id.trim().to_string();
                if sku_id.is_empty() {
                    return Ok::<_, Rejection>(redirect_to("/", None));
                }
                if let Err(error) = catalog::require_active_sku(&sku_id).await {
                    tracing::error!("add_to_cart: require_active_sku({sku_id}) failed: {error:?}");
                    return Err(warp::reject::not_found());
                }

                // `add_line` upserts against an open cart in one statement, so
                // the common case is a single write with no pre-read. Only a
                // missing or closed cart falls through to creating a new one.
                let line = CreateLine {
                    sku_id: sku_id.clone(),
                    quantity: 1,
                };
                if let Some(cart_id) = cart_id_from_cookie(cookie.as_deref()) {
                    match store.add_line(&cart_id, line.clone()).await {
                        Ok(_) => return Ok(redirect_to("/", None)),
                        Err(StoreError::CartNotFound | StoreError::CartNotOpen) => {}
                        Err(error) => {
                            tracing::error!(
                                "add_to_cart: line write for cart {cart_id} failed: {error:?}"
                            );
                            return Err(warp::reject::not_found());
                        }
                    }
                }

                let cart = store.create(Default::default()).await.map_err(|error| {
                    tracing::error!("add_to_cart: store.create failed: {error:?}");
                    warp::reject::not_found()
                })?;
                store.add_line(&cart.id, line).await.map_err(|error| {
                    tracing::error!(
                        "add_to_cart: line write for cart {} failed: {error:?}",
                        cart.id
                    );
                    warp::reject::not_found()
                })?;
                Ok(redirect_to(
                    "/",
                    Some(cart_cookie(&cart.id, CART_COOKIE_MAX_AGE)),
                ))
            },
        )
}

/// Adjust a line: `POST /lines/{line_id}/{increment|decrement|remove}`.
fn change_line(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("lines" / String / String)
        .and(warp::post())
        .and(warp::header::optional::<String>("cookie"))
        .and(store)
        .and_then(
            |line_id: String, action: String, cookie: Option<String>, store: SharedStore| async move {
                let Some(cart_id) = cart_id_from_cookie(cookie.as_deref()) else {
                    return Ok::<_, Rejection>(redirect_to("/", None));
                };

                let current = store
                    .get(&cart_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|c| c.lines.into_iter().find(|l| l.id == line_id));
                let Some(line) = current else {
                    return Ok(redirect_to("/", None));
                };
                let _ = match action.as_str() {
                    "increment" => store
                        .update_line(
                            &cart_id,
                            &line_id,
                            UpdateLine {
                                quantity: line.quantity + 1,
                            },
                        )
                        .await
                        .map(|_| ()),
                    "decrement" if line.quantity > 1 => store
                        .update_line(
                            &cart_id,
                            &line_id,
                            UpdateLine {
                                quantity: line.quantity - 1,
                            },
                        )
                        .await
                        .map(|_| ()),
                    "decrement" | "remove" => store.delete_line(&cart_id, &line_id).await,
                    _ => Ok(()),
                };
                Ok(redirect_to("/", None))
            },
        )
}

fn sign_in_redirect(return_path: &str) -> Response {
    let links = sigma_identity_nav::auth_links(
        &crate::config::identity_public_base_url(),
        &crate::config::public_base_url(),
        return_path,
    );
    match warp::http::Uri::from_maybe_shared(links.sign_in_url) {
        Ok(uri) => warp::redirect::see_other(uri).into_response(),
        Err(_) => internal_server_error(),
    }
}

async fn require_checkout_session(cookie: Option<&str>) -> Result<CheckoutSession, Response> {
    let status = sigma_pg::clients::session::fetch_identity_status(
        &crate::config::identity_internal_base_url(),
        cookie,
    )
    .await;
    let session = match status {
        Ok(Some(session)) => session,
        Ok(None) => return Err(sign_in_redirect("/checkout")),
        Err(error) => {
            tracing::error!("checkout: fetch_identity_status failed: {error:?}");
            return Err(sign_in_redirect("/checkout"));
        }
    };
    let user_id = session
        .user_id
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| sign_in_redirect("/checkout"))?;
    let username = session
        .username
        .or(session.email)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "customer".to_string());
    Ok(CheckoutSession { user_id, username })
}

/// Build the `<select>` options, preselecting the submitted choice, else the
/// caller's default, else the first entry.
fn checkout_options<T: CheckoutChoice>(items: &[T], selected: Option<&str>) -> Vec<CheckoutOption> {
    let selected = selected
        .or_else(|| {
            items
                .iter()
                .find(|item| item.is_choice_default())
                .map(CheckoutChoice::choice_id)
        })
        .or_else(|| items.first().map(CheckoutChoice::choice_id));
    items
        .iter()
        .map(|item| CheckoutOption {
            id: item.choice_id().to_string(),
            summary: item.choice_summary(),
            selected: selected == Some(item.choice_id()),
        })
        .collect()
}

/// The shopper's saved billing/shipping addresses and payment methods, fetched
/// concurrently. Each list degrades to empty (the page then explains why).
async fn checkout_choices(
    user_id: &str,
) -> (
    Vec<AddressSummary>,
    Vec<AddressSummary>,
    Vec<PaymentMethodSummary>,
) {
    let addresses_base = crate::config::addresses_internal_base_url();
    let (billing, shipping, methods) = tokio::join!(
        addresses::list_addresses(addresses_base.as_deref(), user_id, "billing"),
        addresses::list_addresses(addresses_base.as_deref(), user_id, "shipping"),
        payments_client::list_payment_methods(user_id),
    );
    (
        unwrap_or_warn(billing, "billing addresses"),
        unwrap_or_warn(shipping, "shipping addresses"),
        unwrap_or_warn(methods, "payment methods"),
    )
}

fn unwrap_or_warn<T, E: std::fmt::Display>(result: Result<Vec<T>, E>, what: &str) -> Vec<T> {
    result.unwrap_or_else(|e| {
        tracing::warn!("checkout: list {what} failed: {e}");
        Vec::new()
    })
}

async fn load_checkout_priced_lines(
    store: &SharedStore,
    cookie: Option<&str>,
) -> Option<(String, Vec<PricedLine>)> {
    let cart_id = cart_id_from_cookie(cookie)?;
    let cart = store
        .get(&cart_id)
        .await
        .ok()
        .flatten()
        .filter(|c| c.status == CartStatus::Open)?;
    // Both lists are process-cached, so a checkout POST reuses what the GET
    // that rendered the form already fetched.
    let (catalog_skus, prices) = tokio::join!(catalog::fetch_skus(), storefront::fetch_prices());
    let (catalog_skus, prices) = (catalog_skus.ok()?, prices.ok()?);
    let lines = templates::priced_lines(&cart, &catalog_skus, &prices);
    if !lines.iter().any(|l| l.unit_price_cents > 0) {
        return None;
    }
    Some((cart_id, lines))
}

fn checkout_html_reply(
    lines: &[PricedLine],
    billing: Vec<CheckoutOption>,
    shipping: Vec<CheckoutOption>,
    payment_methods: Vec<CheckoutOption>,
    error: &str,
) -> Result<Response, Rejection> {
    let html = templates::render_checkout_html(lines, billing, shipping, payment_methods, error)
        .map_err(|_| warp::reject::not_found())?;
    Ok(warp::reply::html(html).into_response())
}

/// The first reason a submitted checkout form can't proceed to payment, or
/// `None` when the terms are accepted and each selection names a real saved
/// address or payment method.
fn checkout_rejection(
    form: &CheckoutForm,
    billing: &[AddressSummary],
    shipping: &[AddressSummary],
    methods: &[PaymentMethodSummary],
) -> Option<&'static str> {
    if form.accept_terms.as_deref().is_none_or(|v| v.trim().is_empty()) {
        return Some("Please accept the Terms and Conditions.");
    }
    if billing.is_empty() || shipping.is_empty() || methods.is_empty() {
        return Some(
            "Add a billing address, shipping address, and payment method before paying.",
        );
    }
    if !billing.iter().any(|a| a.id == form.billing_address_id) {
        return Some("Select a valid billing address.");
    }
    if !shipping.iter().any(|a| a.id == form.shipping_address_id) {
        return Some("Select a valid shipping address.");
    }
    if !methods.iter().any(|m| m.id == form.payment_method_id) {
        return Some("Select a valid payment method.");
    }
    None
}

/// Legacy path: `POST /reserve` → checkout.
fn reserve_redirect()
-> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("reserve")
        .and(warp::path::end())
        .and(warp::post().or(warp::get()).unify())
        .map(|| redirect_to("/checkout", None))
}

/// Checkout page: `GET /checkout` (requires identity session).
fn checkout_get(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("checkout")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::header::optional::<String>("cookie"))
        .and(store)
        .and_then(|cookie: Option<String>, store: SharedStore| async move {
            let session = match require_checkout_session(cookie.as_deref()).await {
                Ok(session) => session,
                Err(response) => return Ok::<_, Rejection>(response),
            };
            let Some((_cart_id, lines)) =
                load_checkout_priced_lines(&store, cookie.as_deref()).await
            else {
                return Ok(redirect_to("/", None));
            };

            let (billing, shipping, methods) = checkout_choices(&session.user_id).await;
            checkout_html_reply(
                &lines,
                checkout_options(&billing, None),
                checkout_options(&shipping, None),
                checkout_options(&methods, None),
                "",
            )
        })
}

/// Pay deposit and create order: `POST /checkout`.
fn checkout_post(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("checkout")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::header::optional::<String>("cookie"))
        .and(warp::body::form())
        .and(store)
        .and_then(
            |cookie: Option<String>, form: CheckoutForm, store: SharedStore| async move {
                let session = match require_checkout_session(cookie.as_deref()).await {
                    Ok(session) => session,
                    Err(response) => return Ok::<_, Rejection>(response),
                };
                let Some((cart_id, lines)) =
                    load_checkout_priced_lines(&store, cookie.as_deref()).await
                else {
                    return Ok(redirect_to("/", None));
                };

                let (billing, shipping, methods) = checkout_choices(&session.user_id).await;
                let redisplay = |error: &str| {
                    checkout_html_reply(
                        &lines,
                        checkout_options(&billing, Some(form.billing_address_id.as_str())),
                        checkout_options(&shipping, Some(form.shipping_address_id.as_str())),
                        checkout_options(&methods, Some(form.payment_method_id.as_str())),
                        error,
                    )
                };

                if let Some(message) = checkout_rejection(&form, &billing, &shipping, &methods) {
                    return redisplay(message);
                }

                let subtotal: u64 = lines
                    .iter()
                    .filter(|l| l.unit_price_cents > 0)
                    .map(|l| l.unit_price_cents.saturating_mul(u64::from(l.quantity)))
                    .sum();
                let deposit = deposit_cents_for_price(subtotal);
                if deposit == 0 {
                    return Ok(redirect_to("/", None));
                }

                let charge = match payments_client::create_charge(
                    &session.user_id,
                    &form.payment_method_id,
                    deposit,
                    &cart_id,
                )
                .await
                {
                    Ok(charge) if charge.status == "succeeded" => charge,
                    Ok(_) => return redisplay("Payment was declined. Try another method."),
                    Err(payments_client::PaymentsClientError::Declined(reason)) => {
                        return redisplay(&format!("Payment declined: {reason}"));
                    }
                    Err(e) => {
                        tracing::warn!("checkout: charge failed: {e}");
                        return redisplay("Payment failed. Please try again.");
                    }
                };

                let order_lines: Vec<CreateOrderLine> = lines
                    .iter()
                    .filter(|l| l.unit_price_cents > 0)
                    .map(|l| CreateOrderLine {
                        sku_id: l.sku_id.clone(),
                        sku_code: l.sku_code.clone(),
                        name: l.name.clone(),
                        quantity: l.quantity,
                        unit_price_cents: l.unit_price_cents,
                        line_total_cents: None,
                        deposit_cents: None,
                    })
                    .collect();

                let orders_base = crate::config::orders_base_url();
                let order = match orders::create_order(
                    orders_base.as_deref(),
                    &CreateOrder {
                        cart_id: cart_id.clone(),
                        username: session.username.clone(),
                        user_id: Some(session.user_id.clone()),
                        lines: order_lines,
                        id: None,
                        status: Some(OrderStatus::DepositPaid),
                        subtotal_cents: Some(subtotal),
                        deposit_cents: Some(deposit),
                        created_at: None,
                        billing_address_id: Some(form.billing_address_id.clone()),
                        shipping_address_id: Some(form.shipping_address_id.clone()),
                        payment_method_id: Some(form.payment_method_id.clone()),
                        charge_id: Some(charge.id.clone()),
                        terms_accepted_at: Some(chrono::Utc::now().to_rfc3339()),
                    },
                )
                .await
                {
                    Ok(order) => order,
                    Err(e) => {
                        tracing::error!(
                            "checkout: order create failed after charge {}: {e}",
                            charge.id
                        );
                        return redisplay(
                            "Payment succeeded but order creation failed. Contact support with your cart id.",
                        );
                    }
                };

                // Best-effort: a paid checkout must never fail because
                // accounting is down. Anything missed here is backfilled by
                // accounting's reconcile against the payments charge log.
                if let Err(e) = accounting_client::record_deposit_receipt(
                    &session.user_id,
                    &charge.id,
                    &order.id,
                    deposit,
                )
                .await
                {
                    tracing::warn!(
                        "checkout: accounting receipt for charge {} failed: {e}",
                        charge.id
                    );
                }

                if let Err(e) = store.set_status(&cart_id, CartStatus::Submitted).await {
                    tracing::warn!("cart submit after order {} failed: {e}", order.id);
                }

                let html = templates::render_reserved_html(&order)
                    .map_err(|_| warp::reject::not_found())?;
                Ok(
                    warp::reply::with_header(warp::reply::html(html), SET_COOKIE, cart_cookie("", 0))
                        .into_response(),
                )
            },
        )
}

// ---------------------------------------------------------------------------
// Internal admin UI (mounted under /admin)
// ---------------------------------------------------------------------------

fn admin_index(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("admin")
        .and(warp::path::end())
        .and(warp::get())
        .and(store)
        .and_then(|store: SharedStore| async move {
            let (carts, catalog_result, identity_result) =
                tokio::join!(store.list(), catalog::fetch_skus(), identity::fetch_users());
            let carts = carts.map_err(|_| warp::reject::not_found())?;
            let catalog_error = catalog_result.err().map(|e| e.to_string());
            let (identity_users, identity_error) = match identity_result {
                Ok(users) => (users, None),
                Err(e) if crate::config::identity_configured() => {
                    (Default::default(), Some(e.to_string()))
                }
                Err(_) => (Default::default(), None),
            };
            templates::render_index_html(
                carts,
                IndexContext {
                    identity_users: &identity_users,
                    catalog_configured: crate::config::catalog_configured(),
                    identity_configured: crate::config::identity_configured(),
                    catalog_error,
                    identity_error,
                    message: None,
                },
            )
            .map(warp::reply::html)
            .map_err(|_| warp::reject::not_found())
        })
}

fn admin_new_cart()
-> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("admin" / "carts" / "new")
        .and(warp::get())
        .and_then(|| async move {
            let identity_users = identity::fetch_users().await.unwrap_or_default();
            templates::render_cart_form_html_with_values(
                &identity_users,
                None,
                None,
                CartFormValues::for_cart(None),
            )
            .map(warp::reply::html)
            .map_err(|_| warp::reject::not_found())
        })
}

fn admin_create_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("admin" / "carts")
        .and(warp::post())
        .and(warp::body::form())
        .and(store)
        .and_then(|form: CartForm, store: SharedStore| async move {
            let identity_users = identity::fetch_users().await.unwrap_or_default();
            let values = cart_form_to_values(&form);
            let response = match form.into_create() {
                Ok(input) => {
                    if crate::config::identity_configured()
                        && input.user_id.is_some()
                        && identity::user_by_id(
                            &identity_users,
                            input.user_id.as_deref().unwrap_or_default(),
                        )
                        .is_none()
                    {
                        render_cart_form_error(
                            &identity_users,
                            values,
                            invalid_input("identity user not found".to_string()),
                        )
                    } else {
                        match store.create(input).await {
                            Ok(cart) => redirect(format!("/admin/carts/{}", cart.id)),
                            Err(e) => render_cart_form_error(&identity_users, values, e),
                        }
                    }
                }
                Err(e) => render_cart_form_error(&identity_users, values, invalid_input(e)),
            };
            Ok::<_, Rejection>(response)
        })
}

fn admin_cart_detail(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("admin" / "carts" / String)
        .and(warp::get())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            let Some(cart) = store
                .get(&id)
                .await
                .map_err(|_| warp::reject::not_found())?
            else {
                return Err(warp::reject::not_found());
            };
            let (catalog_skus, identity_users) =
                tokio::join!(catalog::fetch_skus(), identity::fetch_users());
            let values = CartFormValues::from_cart(&cart);
            templates::render_detail_html_with_values(
                cart,
                &catalog_skus.unwrap_or_default(),
                &identity_users.unwrap_or_default(),
                None,
                values,
                LineFormValues::default(),
            )
            .map(warp::reply::html)
            .map_err(|_| warp::reject::not_found())
        })
}

fn admin_update_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("admin" / "carts" / String / "edit")
        .and(warp::post())
        .and(warp::body::form())
        .and(store)
        .and_then(
            |id: String, form: CartForm, store: SharedStore| async move {
                let values = cart_form_to_values(&form);
                let error = match form.into_update() {
                    Ok(input) => {
                        let identity_users = identity::fetch_users().await.unwrap_or_default();
                        if crate::config::identity_configured()
                            && input.user_id.is_some()
                            && identity::user_by_id(
                                &identity_users,
                                input.user_id.as_deref().unwrap_or_default(),
                            )
                            .is_none()
                        {
                            invalid_input("identity user not found".to_string())
                        } else {
                            match store.update(&id, input).await {
                                Ok(cart) => {
                                    return Ok::<_, Rejection>(redirect(format!(
                                        "/admin/carts/{}",
                                        cart.id
                                    )));
                                }
                                Err(e) => e,
                            }
                        }
                    }
                    Err(e) => invalid_input(e),
                };
                Ok(
                    render_detail_error(&store, &id, values, LineFormValues::default(), error)
                        .await,
                )
            },
        )
}

fn admin_add_line(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("admin" / "carts" / String / "lines")
        .and(warp::post())
        .and(warp::body::form())
        .and(store)
        .and_then(
            |cart_id: String, form: LineForm, store: SharedStore| async move {
                let line_values = line_form_to_values(&form);
                let error = match form.into_create() {
                    Ok(input) => {
                        let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
                        if !catalog_skus.is_empty()
                            && catalog::validate_sku_id(&catalog_skus, input.sku_id.trim()).is_err()
                        {
                            invalid_input(format!("catalog sku not found: {}", input.sku_id.trim()))
                        } else {
                            match store.add_line(&cart_id, input).await {
                                Ok(_) => {
                                    return Ok::<_, Rejection>(redirect(format!(
                                        "/admin/carts/{cart_id}"
                                    )));
                                }
                                Err(StoreError::CartNotFound) => {
                                    return Err(warp::reject::not_found());
                                }
                                Err(e) => e,
                            }
                        }
                    }
                    Err(e) => invalid_input(e),
                };
                Ok(render_detail_line_error(&store, &cart_id, line_values, error).await)
            },
        )
}

fn admin_delete_line(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("admin" / "carts" / String / "lines" / String / "delete")
        .and(warp::post())
        .and(store)
        .and_then(
            |cart_id: String, line_id: String, store: SharedStore| async move {
                match store.delete_line(&cart_id, &line_id).await {
                    Ok(()) => Ok(redirect(format!("/admin/carts/{cart_id}"))),
                    Err(StoreError::CartNotFound | StoreError::LineNotFound) => {
                        Err(warp::reject::not_found())
                    }
                    // Only the failure path needs the SKU and user lists.
                    Err(e) => Ok(render_detail_line_error(
                        &store,
                        &cart_id,
                        LineFormValues::default(),
                        e,
                    )
                    .await),
                }
            },
        )
}

fn admin_delete_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("admin" / "carts" / String / "delete")
        .and(warp::post())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            match store.delete(&id).await {
                Ok(()) => Ok(redirect("/admin".to_string())),
                Err(StoreError::CartNotFound) => Err(warp::reject::not_found()),
                // Only the failure path needs the cart and user lists.
                Err(e) => {
                    let (carts, identity_users) =
                        tokio::join!(store.list(), identity::fetch_users());
                    let carts = carts.map_err(|_| warp::reject::not_found())?;
                    templates::render_index_html(
                        carts,
                        IndexContext {
                            identity_users: &identity_users.unwrap_or_default(),
                            catalog_configured: crate::config::catalog_configured(),
                            identity_configured: crate::config::identity_configured(),
                            catalog_error: None,
                            identity_error: None,
                            message: Some(format!("Delete failed: {e}")),
                        },
                    )
                    .map(|html| warp::reply::html(html).into_response())
                    .map_err(|_| warp::reject::not_found())
                }
            }
        })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn redirect(location: String) -> Response {
    match warp::http::Uri::from_maybe_shared(location) {
        Ok(uri) => warp::redirect::redirect(uri).into_response(),
        Err(_) => internal_server_error(),
    }
}

fn internal_server_error() -> Response {
    warp::reply::with_status(warp::reply(), StatusCode::INTERNAL_SERVER_ERROR).into_response()
}

/// A rendered admin page is a 400 (the form is redisplayed with its error); a
/// render failure is a 500.
fn html_or_500(html: Result<String, askama::Error>) -> Response {
    match html {
        Ok(html) => warp::reply::with_status(warp::reply::html(html), StatusCode::BAD_REQUEST)
            .into_response(),
        Err(_) => internal_server_error(),
    }
}

fn cart_form_to_values(form: &CartForm) -> CartFormValues {
    CartFormValues {
        user_id: form.user_id.clone(),
        status: form.status.clone(),
        note: form.note.clone(),
    }
}

fn line_form_to_values(form: &LineForm) -> LineFormValues {
    LineFormValues {
        sku_id: form.sku_id.clone(),
        quantity: form.quantity.clone(),
    }
}

fn invalid_input(message: String) -> StoreError {
    StoreError::InvalidInput(message)
}

/// Redisplay the new-cart form with `err`. Only the create flow reaches this;
/// editing an existing cart redisplays its detail page instead.
fn render_cart_form_error(
    identity_users: &[identity::IdentityUser],
    values: CartFormValues,
    err: StoreError,
) -> Response {
    html_or_500(templates::render_cart_form_html_with_values(
        identity_users,
        None,
        Some(err.to_string()),
        values,
    ))
}

/// Redisplay the cart detail page with `err`, loading the SKU and user lists
/// only now that they are needed.
async fn render_detail_error(
    store: &SharedStore,
    cart_id: &str,
    cart_values: CartFormValues,
    line_values: LineFormValues,
    err: StoreError,
) -> Response {
    let (cart, catalog_skus, identity_users) = tokio::join!(
        store.get(cart_id),
        catalog::fetch_skus(),
        identity::fetch_users()
    );
    let Some(cart) = cart.ok().flatten() else {
        return warp::reply::with_status(warp::reply(), StatusCode::NOT_FOUND).into_response();
    };
    html_or_500(templates::render_detail_html_with_values(
        cart,
        &catalog_skus.unwrap_or_default(),
        &identity_users.unwrap_or_default(),
        Some(err.to_string()),
        cart_values,
        line_values,
    ))
}

/// Same as [`render_detail_error`], but the cart fields keep their stored
/// values because only the add-line form was rejected.
async fn render_detail_line_error(
    store: &SharedStore,
    cart_id: &str,
    line_values: LineFormValues,
    err: StoreError,
) -> Response {
    let (cart, catalog_skus, identity_users) = tokio::join!(
        store.get(cart_id),
        catalog::fetch_skus(),
        identity::fetch_users()
    );
    let Some(cart) = cart.ok().flatten() else {
        return warp::reply::with_status(warp::reply(), StatusCode::NOT_FOUND).into_response();
    };
    let cart_values = CartFormValues::from_cart(&cart);
    html_or_500(templates::render_detail_html_with_values(
        cart,
        &catalog_skus.unwrap_or_default(),
        &identity_users.unwrap_or_default(),
        Some(err.to_string()),
        cart_values,
        line_values,
    ))
}
