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
use sigma_theme::warp::{internal_error, internal_rejection, see_other};
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
            .map_err(|e| internal_rejection("render storefront cart page", e))
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

                let cart = match store.get(&cart_id).await {
                    Ok(cart) => cart,
                    Err(e) => {
                        tracing::error!("change_line: reading cart {cart_id} failed: {e}");
                        return Ok(internal_error());
                    }
                };
                // A cookie pointing at a cart that is gone, or a line that is
                // already gone, is a stale page rather than an error: re-render
                // and the shopper sees the current cart.
                let Some(line) = cart
                    .and_then(|c| c.lines.into_iter().find(|l| l.id == line_id))
                else {
                    return Ok(redirect_to("/", None));
                };

                let outcome = match action.as_str() {
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
                    // Only this service's own pages post here, so an unknown
                    // action is a bad link rather than something to explain.
                    _ => return Ok(redirect_to("/", None)),
                };
                // A write that failed must not look like it worked: the cart
                // page would redisplay the old quantity with no explanation.
                if let Err(e) = outcome {
                    tracing::error!("change_line: {action} on line {line_id} failed: {e}");
                    return Ok(internal_error());
                }
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
    see_other(&links.sign_in_url)
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

/// What the shopper can pick from on the checkout form.
struct CheckoutChoices {
    billing: Vec<AddressSummary>,
    shipping: Vec<AddressSummary>,
    methods: Vec<PaymentMethodSummary>,
    /// Whether any list is empty only because its service could not be reached.
    ///
    /// Worth tracking separately: an unreachable addresses service looks exactly
    /// like a shopper with no saved addresses, and telling them to add one they
    /// already have sends them off to fix nothing.
    degraded: bool,
}

impl CheckoutChoices {
    /// The shopper's saved billing/shipping addresses and payment methods,
    /// fetched concurrently. A list whose service fails comes back empty and
    /// marks the whole set degraded.
    async fn load(user_id: &str) -> Self {
        let addresses_base = crate::config::addresses_internal_base_url();
        let payments_base = crate::config::payments_internal_base_url();
        let (billing, shipping, methods) = tokio::join!(
            addresses::list_addresses(addresses_base.as_deref(), user_id, "billing"),
            addresses::list_addresses(addresses_base.as_deref(), user_id, "shipping"),
            payments_client::list_payment_methods(payments_base.as_deref(), user_id),
        );
        let mut degraded = false;
        let (billing, shipping, methods) = (
            unwrap_or_warn(billing, "billing addresses", &mut degraded),
            unwrap_or_warn(shipping, "shipping addresses", &mut degraded),
            unwrap_or_warn(methods, "payment methods", &mut degraded),
        );
        Self {
            billing,
            shipping,
            methods,
            degraded,
        }
    }

    /// The notice to show above the form: empty unless a service is down, in
    /// which case the shopper is told to come back rather than to add details
    /// they may already have saved.
    fn notice(&self) -> &'static str {
        if self.degraded {
            UNAVAILABLE_NOTICE
        } else {
            ""
        }
    }
}

/// Shown whenever checkout cannot see the shopper's saved addresses or payment
/// methods, on both the form and a rejected submission.
const UNAVAILABLE_NOTICE: &str =
    "Checkout is temporarily unavailable. Please try again in a moment.";

fn unwrap_or_warn<T, E: std::fmt::Display>(
    result: Result<Vec<T>, E>,
    what: &str,
    degraded: &mut bool,
) -> Vec<T> {
    result.unwrap_or_else(|e| {
        tracing::warn!("checkout: list {what} failed: {e}");
        *degraded = true;
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
        .map_err(|e| internal_rejection("render checkout page", e))?;
    Ok(warp::reply::html(html).into_response())
}

/// The first reason a submitted checkout form can't proceed to payment, or
/// `None` when the terms are accepted and each selection names a real saved
/// address or payment method.
fn checkout_rejection(form: &CheckoutForm, choices: &CheckoutChoices) -> Option<&'static str> {
    if form
        .accept_terms
        .as_deref()
        .is_none_or(|v| v.trim().is_empty())
    {
        return Some("Please accept the Terms and Conditions.");
    }
    // Checked before the emptiness rules below: with a service down we cannot
    // tell a shopper who has saved nothing from one whose details we simply
    // cannot see.
    if choices.degraded {
        return Some(UNAVAILABLE_NOTICE);
    }
    if choices.billing.is_empty() || choices.shipping.is_empty() || choices.methods.is_empty() {
        return Some("Add a billing address, shipping address, and payment method before paying.");
    }
    if !choices
        .billing
        .iter()
        .any(|a| a.id == form.billing_address_id)
    {
        return Some("Select a valid billing address.");
    }
    if !choices
        .shipping
        .iter()
        .any(|a| a.id == form.shipping_address_id)
    {
        return Some("Select a valid shipping address.");
    }
    if !choices
        .methods
        .iter()
        .any(|m| m.id == form.payment_method_id)
    {
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

            let choices = CheckoutChoices::load(&session.user_id).await;
            checkout_html_reply(
                &lines,
                checkout_options(&choices.billing, None),
                checkout_options(&choices.shipping, None),
                checkout_options(&choices.methods, None),
                choices.notice(),
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

                let choices = CheckoutChoices::load(&session.user_id).await;
                let redisplay = |error: &str| {
                    checkout_html_reply(
                        &lines,
                        checkout_options(&choices.billing, Some(form.billing_address_id.as_str())),
                        checkout_options(
                            &choices.shipping,
                            Some(form.shipping_address_id.as_str()),
                        ),
                        checkout_options(&choices.methods, Some(form.payment_method_id.as_str())),
                        error,
                    )
                };

                if let Some(message) = checkout_rejection(&form, &choices) {
                    return redisplay(message);
                }

                let Some(totals) = CheckoutTotals::from_lines(&lines) else {
                    return Ok(redirect_to("/", None));
                };

                let services = CheckoutServices::from_config();
                let order =
                    match place_deposit_order(&services, &session, &cart_id, &lines, &form, totals)
                        .await
                    {
                        Ok(order) => order,
                        Err(message) => return redisplay(&message),
                    };

                if let Err(e) = store.set_status(&cart_id, CartStatus::Submitted).await {
                    tracing::warn!("cart submit after order {} failed: {e}", order.id);
                }

                let html = templates::render_reserved_html(&order)
                    .map_err(|e| internal_rejection("render reserved page", e))?;
                Ok(warp::reply::with_header(
                    warp::reply::html(html),
                    SET_COOKIE,
                    cart_cookie("", 0),
                )
                .into_response())
            },
        )
}

// ---------------------------------------------------------------------------
// Checkout saga
// ---------------------------------------------------------------------------

/// Where the checkout saga's collaborators live, resolved from configuration
/// once at the request boundary.
///
/// Passing these in rather than reading the environment inside each step keeps
/// the saga testable: the tests below point it at stub services.
struct CheckoutServices {
    orders: Option<String>,
    payments: Option<String>,
    accounting: Option<String>,
}

impl CheckoutServices {
    fn from_config() -> Self {
        Self {
            orders: crate::config::orders_base_url(),
            payments: crate::config::payments_internal_base_url(),
            accounting: crate::config::accounting_internal_base_url(),
        }
    }
}

/// What a checkout is worth: the priced subtotal and the deposit due now.
struct CheckoutTotals {
    subtotal_cents: u64,
    deposit_cents: u64,
}

impl CheckoutTotals {
    /// `None` when the cart carries no deposit -- nothing priced, or a subtotal
    /// too small to deposit against -- so there is nothing to check out.
    fn from_lines(lines: &[PricedLine]) -> Option<Self> {
        let subtotal_cents: u64 = lines
            .iter()
            .filter(|l| l.unit_price_cents > 0)
            .map(|l| l.unit_price_cents.saturating_mul(u64::from(l.quantity)))
            .sum();
        let deposit_cents = deposit_cents_for_price(subtotal_cents);
        (deposit_cents > 0).then_some(Self {
            subtotal_cents,
            deposit_cents,
        })
    }
}

/// Turn a validated checkout form into a paid order: reserve the order, take
/// the deposit, then commit the order.
///
/// The ordering is the point. The order is recorded `pending_deposit` *before*
/// any money moves, so a deposit can never exist without a durable order to
/// attach it to -- the failure that used to leave a shopper charged with
/// nothing to show for it. Each step is keyed so a retry replays rather than
/// duplicates: the order by cart id, the charge by order id.
///
/// # Errors
///
/// The message to show the shopper. Failures before the charge leave a
/// `pending_deposit` order that the next attempt reuses; a charge that cannot
/// be committed is refunded.
async fn place_deposit_order(
    services: &CheckoutServices,
    session: &CheckoutSession,
    cart_id: &str,
    lines: &[PricedLine],
    form: &CheckoutForm,
    totals: CheckoutTotals,
) -> Result<orders::Order, String> {
    let orders_base = services.orders.as_deref();
    let order = reserve_pending_order(orders_base, session, cart_id, lines, form, &totals).await?;

    match order.status {
        OrderStatus::PendingDeposit => {}
        // The deposit already landed: a double submit, or a browser retrying a
        // response it never received. Show the order rather than charging again.
        OrderStatus::DepositPaid | OrderStatus::InBuild | OrderStatus::Shipped => return Ok(order),
        OrderStatus::Cancelled => {
            return Err("This order was cancelled. Please start a new cart.".to_string());
        }
    }

    let charge = charge_deposit(
        services.payments.as_deref(),
        session,
        form,
        &order.id,
        totals.deposit_cents,
    )
    .await?;

    // Committing the order is the first step that happens after money moves, so
    // it is the one that must compensate: a deposit we cannot attach to an
    // order is refunded rather than left stranded.
    match orders::mark_deposit_paid(orders_base, &order.id, &charge.id).await {
        Ok(paid) => {
            record_deposit_receipt(
                services.accounting.as_deref(),
                session,
                &charge.id,
                &paid.id,
                totals.deposit_cents,
            )
            .await;
            Ok(paid)
        }
        Err(e) => {
            tracing::error!(
                "checkout: committing order {} after charge {} failed: {e}",
                order.id,
                charge.id
            );
            Err(refund_stranded_deposit(services.payments.as_deref(), &charge.id).await)
        }
    }
}

/// Record the order awaiting its deposit. Idempotent on the orders side per
/// cart, so a shopper who resubmits gets the same order back -- which is what
/// makes the charge below idempotent too, since it keys on the order id.
async fn reserve_pending_order(
    orders_base: Option<&str>,
    session: &CheckoutSession,
    cart_id: &str,
    lines: &[PricedLine],
    form: &CheckoutForm,
    totals: &CheckoutTotals,
) -> Result<orders::Order, String> {
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

    orders::create_order(
        orders_base,
        &CreateOrder {
            cart_id: cart_id.to_string(),
            username: session.username.clone(),
            user_id: Some(session.user_id.clone()),
            lines: order_lines,
            id: None,
            status: Some(OrderStatus::PendingDeposit),
            subtotal_cents: Some(totals.subtotal_cents),
            deposit_cents: Some(totals.deposit_cents),
            created_at: None,
            billing_address_id: Some(form.billing_address_id.clone()),
            shipping_address_id: Some(form.shipping_address_id.clone()),
            payment_method_id: Some(form.payment_method_id.clone()),
            // Attached by `mark_deposit_paid` once a charge succeeds.
            charge_id: None,
            terms_accepted_at: Some(chrono::Utc::now().to_rfc3339()),
        },
    )
    .await
    .map_err(|e| {
        tracing::error!("checkout: reserving order for cart {cart_id} failed: {e}");
        "We could not start your order. Please try again.".to_string()
    })
}

/// Charge the deposit against the shopper's saved payment method.
///
/// `order_id` is the payment reference: payments collapses a repeat charge for
/// the same reference onto the original, so a retried checkout cannot take a
/// second deposit.
async fn charge_deposit(
    payments_base: Option<&str>,
    session: &CheckoutSession,
    form: &CheckoutForm,
    order_id: &str,
    deposit_cents: u64,
) -> Result<payments_client::Charge, String> {
    match payments_client::create_charge(
        payments_base,
        &session.user_id,
        &form.payment_method_id,
        deposit_cents,
        order_id,
    )
    .await
    {
        Ok(charge) if charge.status == "succeeded" => Ok(charge),
        Ok(_) => Err("Payment was declined. Try another method.".to_string()),
        Err(payments_client::PaymentsClientError::Declined(reason)) => {
            Err(format!("Payment declined: {reason}"))
        }
        Err(e) => {
            tracing::warn!("checkout: charging order {order_id} failed: {e}");
            Err("Payment failed. Please try again.".to_string())
        }
    }
}

/// Reverse a deposit that could not be attached to its order, and return the
/// message to show the shopper.
///
/// If the reversal itself fails there is money held against an order that will
/// never ship, so the message carries the charge id: that is the one case where
/// support has to intervene.
async fn refund_stranded_deposit(payments_base: Option<&str>, charge_id: &str) -> String {
    match payments_client::refund_charge(
        payments_base,
        charge_id,
        "checkout could not be completed",
    )
    .await
    {
        Ok(()) => {
            "We could not complete your order, so your deposit has been returned. Please try again."
                .to_string()
        }
        Err(e) => {
            tracing::error!("checkout: refunding stranded charge {charge_id} failed: {e}");
            format!(
                "We could not complete your order. Please contact support quoting payment {charge_id}."
            )
        }
    }
}

/// Best-effort: a paid checkout must never fail because accounting is down.
/// Anything missed here is backfilled by accounting's reconcile against the
/// payments charge log.
async fn record_deposit_receipt(
    accounting_base: Option<&str>,
    session: &CheckoutSession,
    charge_id: &str,
    order_id: &str,
    deposit_cents: u64,
) {
    if let Err(e) = accounting_client::record_deposit_receipt(
        accounting_base,
        &session.user_id,
        charge_id,
        order_id,
        deposit_cents,
    )
    .await
    {
        tracing::warn!("checkout: accounting receipt for charge {charge_id} failed: {e}");
    }
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
            let carts = carts.map_err(|e| internal_rejection("list carts", e))?;
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
            .map_err(|e| internal_rejection("render admin index", e))
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
            .map_err(|e| internal_rejection("render cart form", e))
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
                            Ok(cart) => see_other(&format!("/admin/carts/{}", cart.id)),
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
                .map_err(|e| internal_rejection("read cart", e))?
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
            .map_err(|e| internal_rejection("render cart detail", e))
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
                                    return Ok::<_, Rejection>(see_other(&format!(
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
                                    return Ok::<_, Rejection>(see_other(&format!(
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
                    Ok(()) => Ok(see_other(&format!("/admin/carts/{cart_id}"))),
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
                Ok(()) => Ok(see_other("/admin")),
                Err(StoreError::CartNotFound) => Err(warp::reject::not_found()),
                // Only the failure path needs the cart and user lists.
                Err(e) => {
                    let (carts, identity_users) =
                        tokio::join!(store.list(), identity::fetch_users());
                    let carts = carts.map_err(|e| internal_rejection("list carts", e))?;
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
                    .map_err(|e| internal_rejection("render admin index", e))
                }
            }
        })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// A rendered admin page is a 400 (the form is redisplayed with its error); a
/// render failure is a 500.
fn html_or_500(html: Result<String, askama::Error>) -> Response {
    match html {
        Ok(html) => warp::reply::with_status(warp::reply::html(html), StatusCode::BAD_REQUEST)
            .into_response(),
        Err(_) => internal_error(),
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};

    use super::*;

    /// How the stub orders and payments services behave for one attempt.
    #[derive(Clone, Copy)]
    struct StubBehavior {
        /// Status the reserved order comes back with. Anything other than
        /// `PendingDeposit` stands in for a checkout that already ran once.
        reserved_status: OrderStatus,
        reserve_fails: bool,
        charge_declined: bool,
        commit_fails: bool,
        refund_fails: bool,
    }

    impl Default for StubBehavior {
        fn default() -> Self {
            Self {
                reserved_status: OrderStatus::PendingDeposit,
                reserve_fails: false,
                charge_declined: false,
                commit_fails: false,
                refund_fails: false,
            }
        }
    }

    /// What the stub services were actually asked to do.
    #[derive(Default)]
    struct StubCalls {
        reserved: usize,
        /// Payment reference of each charge attempt.
        charged: Vec<String>,
        /// Charge id passed to each order commit.
        committed: Vec<String>,
        /// Charge id of each refund.
        refunded: Vec<String>,
        receipts: usize,
    }

    struct Stub {
        behavior: StubBehavior,
        calls: Mutex<StubCalls>,
    }

    const ORDER_ID: &str = "order-1";
    const CHARGE_ID: &str = "charge-1";

    fn stub_order(status: OrderStatus, charge_id: Option<&str>) -> orders::Order {
        orders::Order {
            id: ORDER_ID.to_string(),
            cart_id: "cart-1".to_string(),
            username: "shopper".to_string(),
            user_id: Some("user-1".to_string()),
            lines: vec![orders::OrderLine {
                sku_id: "sku-1".to_string(),
                sku_code: "SIGMA-RACER".to_string(),
                name: "Sigma Racer".to_string(),
                quantity: 1,
                unit_price_cents: 350_000,
                line_total_cents: 350_000,
                deposit_cents: 35_000,
            }],
            subtotal_cents: 350_000,
            deposit_cents: 35_000,
            status,
            billing_address_id: Some("addr-b".to_string()),
            shipping_address_id: Some("addr-s".to_string()),
            payment_method_id: Some("pm-1".to_string()),
            charge_id: charge_id.map(str::to_string),
            terms_accepted_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn json_status(status: StatusCode, body: &Value) -> Response {
        warp::reply::with_status(warp::reply::json(body), status).into_response()
    }

    /// The orders, payments, and accounting endpoints the saga calls, standing
    /// in for the three services.
    fn stub_routes(
        stub: Arc<Stub>,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + Sync + 'static
    {
        let reserve_stub = stub.clone();
        let reserve = warp::path!("orders")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |_body: Value| {
                reserve_stub.calls.lock().unwrap().reserved += 1;
                if reserve_stub.behavior.reserve_fails {
                    return json_status(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &json!({ "error": "orders unavailable" }),
                    );
                }
                warp::reply::json(&stub_order(reserve_stub.behavior.reserved_status, None))
                    .into_response()
            });

        let commit_stub = stub.clone();
        let commit = warp::path!("orders" / String / "deposit-paid")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |_id: String, body: Value| {
                let charge_id = body["charge_id"].as_str().unwrap_or_default().to_string();
                commit_stub.calls.lock().unwrap().committed.push(charge_id);
                if commit_stub.behavior.commit_fails {
                    return json_status(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &json!({ "error": "orders unavailable" }),
                    );
                }
                warp::reply::json(&stub_order(OrderStatus::DepositPaid, Some(CHARGE_ID)))
                    .into_response()
            });

        let charge_stub = stub.clone();
        let charge = warp::path!("api" / "charges")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |body: Value| {
                let reference = body["reference"].as_str().unwrap_or_default().to_string();
                charge_stub.calls.lock().unwrap().charged.push(reference);
                if charge_stub.behavior.charge_declined {
                    return json_status(
                        StatusCode::PAYMENT_REQUIRED,
                        &json!({
                            "id": CHARGE_ID,
                            "status": "failed",
                            "failure_reason": "insufficient funds",
                        }),
                    );
                }
                json_status(
                    StatusCode::CREATED,
                    &json!({ "id": CHARGE_ID, "status": "succeeded" }),
                )
            });

        let refund_stub = stub.clone();
        let refund = warp::path!("api" / "charges" / String / "refund")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |charge_id: String, _body: Value| {
                refund_stub.calls.lock().unwrap().refunded.push(charge_id);
                if refund_stub.behavior.refund_fails {
                    return json_status(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &json!({ "error": "payments unavailable" }),
                    );
                }
                json_status(StatusCode::CREATED, &json!({ "id": "refund-1" }))
            });

        let receipt = warp::path!("receipts")
            .and(warp::post())
            .and(warp::body::json())
            .map(move |_body: Value| {
                stub.calls.lock().unwrap().receipts += 1;
                json_status(StatusCode::CREATED, &json!({ "id": "receipt-1" }))
            });

        reserve.or(commit).or(charge).or(refund).or(receipt)
    }

    fn checkout_line() -> PricedLine {
        PricedLine {
            line_id: "line-1".to_string(),
            sku_id: "sku-1".to_string(),
            sku_code: "SIGMA-RACER".to_string(),
            name: "Sigma Racer".to_string(),
            quantity: 1,
            unit_price_cents: 350_000,
            in_catalog: true,
        }
    }

    /// Run one checkout against stub services behaving as described, returning
    /// its outcome alongside every call the stubs saw.
    async fn checkout_against_stubs(
        behavior: StubBehavior,
    ) -> (Result<orders::Order, String>, StubCalls) {
        let stub = Arc::new(Stub {
            behavior,
            calls: Mutex::new(StubCalls::default()),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stub services");
        let base = format!("http://{}/", listener.local_addr().expect("stub address"));
        tokio::spawn(
            warp::serve(stub_routes(Arc::clone(&stub)))
                .incoming(listener)
                .run(),
        );

        let services = CheckoutServices {
            orders: Some(base.clone()),
            payments: Some(base.clone()),
            accounting: Some(base),
        };
        let session = CheckoutSession {
            user_id: "user-1".to_string(),
            username: "shopper".to_string(),
        };
        let form = CheckoutForm {
            billing_address_id: "addr-b".to_string(),
            shipping_address_id: "addr-s".to_string(),
            payment_method_id: "pm-1".to_string(),
            accept_terms: Some("on".to_string()),
        };
        let lines = vec![checkout_line()];
        let totals = CheckoutTotals::from_lines(&lines).expect("priced cart");

        let outcome =
            place_deposit_order(&services, &session, "cart-1", &lines, &form, totals).await;
        let calls = std::mem::take(&mut *stub.calls.lock().unwrap());
        (outcome, calls)
    }

    #[tokio::test]
    async fn a_checkout_reserves_the_order_before_taking_the_deposit() {
        let (outcome, calls) = checkout_against_stubs(StubBehavior::default()).await;

        let order = outcome.expect("checkout completes");
        assert_eq!(order.status, OrderStatus::DepositPaid);
        assert_eq!(order.charge_id.as_deref(), Some(CHARGE_ID));
        assert_eq!(calls.reserved, 1);
        // The charge is keyed on the order, which is what makes a retry a replay
        // rather than a second deposit.
        assert_eq!(calls.charged, [ORDER_ID]);
        assert_eq!(calls.committed, [CHARGE_ID]);
        assert!(calls.refunded.is_empty());
        assert_eq!(calls.receipts, 1);
    }

    #[tokio::test]
    async fn no_money_moves_when_the_order_cannot_be_reserved() {
        let (outcome, calls) = checkout_against_stubs(StubBehavior {
            reserve_fails: true,
            ..StubBehavior::default()
        })
        .await;

        assert!(outcome.is_err());
        assert!(
            calls.charged.is_empty(),
            "a shopper must never be charged before the order exists"
        );
    }

    #[tokio::test]
    async fn a_resubmitted_checkout_is_not_charged_twice() {
        let (outcome, calls) = checkout_against_stubs(StubBehavior {
            reserved_status: OrderStatus::DepositPaid,
            ..StubBehavior::default()
        })
        .await;

        let order = outcome.expect("the paid order is shown again");
        assert_eq!(order.status, OrderStatus::DepositPaid);
        assert!(calls.charged.is_empty());
        assert!(calls.committed.is_empty());
    }

    #[tokio::test]
    async fn a_declined_card_leaves_the_order_awaiting_its_deposit() {
        let (outcome, calls) = checkout_against_stubs(StubBehavior {
            charge_declined: true,
            ..StubBehavior::default()
        })
        .await;

        let message = outcome.expect_err("a declined card cannot complete checkout");
        assert!(message.contains("declined"), "got {message:?}");
        // Nothing to compensate: the reserved order simply stays unpaid and the
        // next attempt reuses it.
        assert!(calls.committed.is_empty());
        assert!(calls.refunded.is_empty());
    }

    #[tokio::test]
    async fn a_deposit_that_cannot_be_committed_is_refunded() {
        let (outcome, calls) = checkout_against_stubs(StubBehavior {
            commit_fails: true,
            ..StubBehavior::default()
        })
        .await;

        let message = outcome.expect_err("an uncommittable order cannot complete checkout");
        assert!(message.contains("returned"), "got {message:?}");
        assert_eq!(calls.refunded, [CHARGE_ID]);
        assert_eq!(
            calls.receipts, 0,
            "a refunded deposit must not be booked as revenue"
        );
    }

    #[tokio::test]
    async fn a_deposit_that_cannot_be_refunded_names_the_payment() {
        let (outcome, _) = checkout_against_stubs(StubBehavior {
            commit_fails: true,
            refund_fails: true,
            ..StubBehavior::default()
        })
        .await;

        let message = outcome.expect_err("checkout cannot complete");
        assert!(
            message.contains(CHARGE_ID),
            "support needs the payment id: {message:?}"
        );
    }

    #[tokio::test]
    async fn a_cancelled_order_is_not_charged() {
        let (outcome, calls) = checkout_against_stubs(StubBehavior {
            reserved_status: OrderStatus::Cancelled,
            ..StubBehavior::default()
        })
        .await;

        assert!(outcome.is_err());
        assert!(calls.charged.is_empty());
    }

    #[test]
    fn totals_ignore_unpriced_lines_and_reject_an_empty_deposit() {
        let mut unpriced = checkout_line();
        unpriced.unit_price_cents = 0;
        assert!(CheckoutTotals::from_lines(&[unpriced]).is_none());

        let mut two = checkout_line();
        two.quantity = 2;
        let totals = CheckoutTotals::from_lines(&[two]).expect("priced cart");
        assert_eq!(totals.subtotal_cents, 700_000);
        assert_eq!(
            totals.deposit_cents,
            deposit_cents_for_price(totals.subtotal_cents)
        );
    }
}
