use std::convert::Infallible;

use warp::http::StatusCode;
use warp::http::header::{LOCATION, SET_COOKIE};
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::catalog;
use crate::identity;
use crate::model::{CartForm, CartStatus, CreateLine, LineForm, UpdateLine};
use crate::order::{self, CreateOrderLine, CreateOrderRequest};
use crate::store::StoreError;
use crate::storefront;
use crate::templates::{self, CartFormValues, IndexContext, LineFormValues};

/// Cookie tying a browser to its guest cart. Shared with the storefront so it
/// can show a live item count (same host in dev, shared parent domain in prod).
const CART_COOKIE: &str = "sigma_cart";
/// Guest cart cookie lifetime (30 days).
const CART_COOKIE_MAX_AGE: i64 = 60 * 60 * 24 * 30;

pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    // Public shopping-cart UI.
    cart_view(store.clone())
        .or(add_to_cart(store.clone()))
        .or(change_line(store.clone()))
        .or(reserve(store.clone()))
        // Internal admin UI (reached through the identity proxy in production).
        .or(admin_index(store.clone()))
        .or(admin_new_cart(store.clone()))
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
    cookie_header?.split(';').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name.trim() == CART_COOKIE)
            .then(|| value.trim().to_string())
            .filter(|v| !v.is_empty())
    })
}

fn set_cart_cookie(cart_id: &str) -> String {
    let mut cookie =
        format!("{CART_COOKIE}={cart_id}; Path=/; Max-Age={CART_COOKIE_MAX_AGE}; SameSite=Lax");
    if let Some(domain) = crate::config::cookie_domain() {
        cookie.push_str(&format!("; Domain={domain}"));
    }
    cookie
}

fn clear_cart_cookie() -> String {
    let mut cookie = format!("{CART_COOKIE}=; Path=/; Max-Age=0; SameSite=Lax");
    if let Some(domain) = crate::config::cookie_domain() {
        cookie.push_str(&format!("; Domain={domain}"));
    }
    cookie
}

/// 303 redirect, optionally attaching a `Set-Cookie` header.
fn redirect_to(location: &'static str, set_cookie: Option<String>) -> warp::reply::Response {
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
                Some(id) => store.lock().await.get(&id).await.ok().flatten(),
                None => None,
            };
            let cart = cart.filter(|c| c.status == CartStatus::Open);
            let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
            let prices = storefront::fetch_prices().await.unwrap_or_default();
            templates::render_storefront_cart_html(cart.as_ref(), &catalog_skus, &prices)
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
                // Only real, active catalog SKUs can be added.
                if let Ok(skus) = catalog::fetch_skus().await
                    && catalog::validate_sku_id(&skus, &sku_id).is_err()
                {
                    return Err(warp::reject::not_found());
                }

                let mut store = store.lock().await;
                let mut set_cookie: Option<String> = None;
                let cart_id = match cart_id_from_cookie(cookie.as_deref()) {
                    Some(id)
                        if store
                            .get(&id)
                            .await
                            .ok()
                            .flatten()
                            .is_some_and(|c| c.status == CartStatus::Open) =>
                    {
                        id
                    }
                    _ => {
                        let cart = store
                            .create(Default::default())
                            .await
                            .map_err(|_| warp::reject::not_found())?;
                        set_cookie = Some(set_cart_cookie(&cart.id));
                        cart.id
                    }
                };

                // Merge with an existing line for the same SKU.
                let existing = store
                    .get(&cart_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|c| c.lines.into_iter().find(|l| l.sku_id == sku_id));
                let result = match existing {
                    Some(line) => store
                        .update_line(
                            &cart_id,
                            &line.id,
                            UpdateLine {
                                quantity: line.quantity + 1,
                            },
                        )
                        .await
                        .map(|_| ()),
                    None => store
                        .add_line(
                            &cart_id,
                            CreateLine {
                                sku_id: sku_id.clone(),
                                quantity: 1,
                            },
                        )
                        .await
                        .map(|_| ()),
                };
                result.map_err(|_| warp::reject::not_found())?;

                Ok(redirect_to("/", set_cookie))
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
                let mut store = store.lock().await;
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

/// Reserve the cart by paying the deposit: `POST /reserve` (form: `username`).
fn reserve(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    #[derive(serde::Deserialize)]
    struct ReserveForm {
        username: String,
    }

    warp::path("reserve")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::header::optional::<String>("cookie"))
        .and(warp::body::form())
        .and(store)
        .and_then(
            |cookie: Option<String>, form: ReserveForm, store: SharedStore| async move {
                let username = form.username.trim().to_string();
                let Some(cart_id) = cart_id_from_cookie(cookie.as_deref()) else {
                    return Ok::<_, Rejection>(redirect_to("/", None));
                };
                let cart = store
                    .lock()
                    .await
                    .get(&cart_id)
                    .await
                    .ok()
                    .flatten()
                    .filter(|c| c.status == CartStatus::Open);
                let Some(cart) = cart else {
                    return Ok(redirect_to("/", None));
                };
                let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
                let prices = storefront::fetch_prices().await.unwrap_or_default();

                let order_lines: Vec<CreateOrderLine> =
                    templates::priced_lines(&cart, &catalog_skus, &prices)
                        .into_iter()
                        .filter(|l| l.unit_price_cents > 0)
                        .map(|l| CreateOrderLine {
                            sku_id: l.sku_id,
                            sku_code: l.sku_code,
                            name: l.name,
                            quantity: l.quantity,
                            unit_price_cents: l.unit_price_cents,
                            line_total_cents: None,
                            deposit_cents: None,
                        })
                        .collect();

                if username.is_empty() || order_lines.is_empty() {
                    return Ok(redirect_to("/", None));
                }

                let order = match order::create_order(CreateOrderRequest {
                    cart_id: cart_id.clone(),
                    username,
                    user_id: cart.user_id.clone(),
                    lines: order_lines,
                    id: None,
                    status: None,
                    subtotal_cents: None,
                    deposit_cents: None,
                    created_at: None,
                })
                .await
                {
                    Ok(order) => order,
                    Err(_) => return Ok(redirect_to("/", None)),
                };
                let mut store = store.lock().await;
                let _ = store.set_status(&cart_id, CartStatus::Submitted).await;
                drop(store);

                let html = templates::render_reserved_html(&order)
                    .map_err(|_| warp::reject::not_found())?;
                let reply = warp::reply::html(html);
                Ok(
                    warp::reply::with_header(reply, SET_COOKIE, clear_cart_cookie())
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
            let carts = store
                .lock()
                .await
                .list()
                .await
                .map_err(|_| warp::reject::not_found())?;
            let catalog_result = catalog::fetch_skus().await;
            let identity_result = identity::fetch_users().await;
            let (catalog_skus, catalog_error) = match catalog_result {
                Ok(skus) => (Some(skus), None),
                Err(e) => (None, Some(e.to_string())),
            };
            let (identity_users, identity_error) = match identity_result {
                Ok(users) => (Some(users), None),
                Err(e) if crate::config::identity_configured() => (None, Some(e.to_string())),
                Err(_) => (None, None),
            };
            templates::render_index_html(
                carts,
                IndexContext {
                    catalog_skus: catalog_skus.as_deref().unwrap_or(&[]),
                    identity_users: identity_users.as_deref().unwrap_or(&[]),
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

fn admin_new_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("admin" / "carts" / "new")
        .and(warp::get())
        .and(store)
        .and_then(|store: SharedStore| async move {
            let carts = store
                .lock()
                .await
                .list()
                .await
                .map_err(|_| warp::reject::not_found())?;
            let identity_users = identity::fetch_users().await.unwrap_or_default();
            templates::render_cart_form_html(carts, &identity_users, None, None)
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
            let mut store = store.lock().await;
            let carts = store.list().await.map_err(|_| warp::reject::not_found())?;
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
                            carts,
                            &identity_users,
                            None,
                            values,
                            invalid_input("identity user not found".to_string()),
                        )
                    } else {
                        match store.create(input).await {
                            Ok(cart) => redirect(format!("/admin/carts/{}", cart.id)),
                            Err(e) => {
                                render_cart_form_error(carts, &identity_users, None, values, e)
                            }
                        }
                    }
                }
                Err(e) => {
                    render_cart_form_error(carts, &identity_users, None, values, invalid_input(e))
                }
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
            let store = store.lock().await;
            let Some(cart) = store
                .get(&id)
                .await
                .map_err(|_| warp::reject::not_found())?
            else {
                return Err(warp::reject::not_found());
            };
            let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
            let identity_users = identity::fetch_users().await.unwrap_or_default();
            templates::render_detail_html(cart, &catalog_skus, &identity_users, None, None)
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
                let mut store = store.lock().await;
                let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
                let identity_users = identity::fetch_users().await.unwrap_or_default();
                let values = cart_form_to_values(&form);
                let response = match form.into_update() {
                    Ok(input) => {
                        if crate::config::identity_configured()
                            && input.user_id.is_some()
                            && identity::user_by_id(
                                &identity_users,
                                input.user_id.as_deref().unwrap_or_default(),
                            )
                            .is_none()
                        {
                            let cart = store.get(&id).await.ok().flatten();
                            render_detail_error(
                                cart,
                                &catalog_skus,
                                &identity_users,
                                values,
                                invalid_input("identity user not found".to_string()),
                            )
                        } else {
                            match store.update(&id, input).await {
                                Ok(cart) => redirect(format!("/admin/carts/{}", cart.id)),
                                Err(e) => {
                                    let cart = store.get(&id).await.ok().flatten();
                                    render_detail_error(
                                        cart,
                                        &catalog_skus,
                                        &identity_users,
                                        values,
                                        e,
                                    )
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let cart = store.get(&id).await.ok().flatten();
                        render_detail_error(
                            cart,
                            &catalog_skus,
                            &identity_users,
                            values,
                            invalid_input(e),
                        )
                    }
                };
                Ok::<_, Rejection>(response)
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
                let mut store = store.lock().await;
                let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
                let identity_users = identity::fetch_users().await.unwrap_or_default();
                let line_values = line_form_to_values(&form);
                let response = match form.into_create() {
                    Ok(input) => {
                        if !catalog_skus.is_empty()
                            && catalog::validate_sku_id(&catalog_skus, input.sku_id.trim()).is_err()
                        {
                            let cart = store.get(&cart_id).await.ok().flatten();
                            render_detail_line_error(
                                cart,
                                &catalog_skus,
                                &identity_users,
                                line_values,
                                invalid_input(format!(
                                    "catalog sku not found: {}",
                                    input.sku_id.trim()
                                )),
                            )
                        } else {
                            match store.add_line(&cart_id, input).await {
                                Ok(_) => redirect(format!("/admin/carts/{cart_id}")),
                                Err(StoreError::CartNotFound) => {
                                    return Err(warp::reject::not_found());
                                }
                                Err(e) => {
                                    let cart = store.get(&cart_id).await.ok().flatten();
                                    render_detail_line_error(
                                        cart,
                                        &catalog_skus,
                                        &identity_users,
                                        line_values,
                                        e,
                                    )
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let cart = store.get(&cart_id).await.ok().flatten();
                        render_detail_line_error(
                            cart,
                            &catalog_skus,
                            &identity_users,
                            line_values,
                            invalid_input(e),
                        )
                    }
                };
                Ok(response)
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
                let mut store = store.lock().await;
                let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
                let identity_users = identity::fetch_users().await.unwrap_or_default();
                match store.delete_line(&cart_id, &line_id).await {
                    Ok(()) => Ok(redirect(format!("/admin/carts/{cart_id}"))),
                    Err(StoreError::CartNotFound | StoreError::LineNotFound) => {
                        Err(warp::reject::not_found())
                    }
                    Err(e) => {
                        let cart = store.get(&cart_id).await.ok().flatten();
                        Ok(render_detail_line_error(
                            cart,
                            &catalog_skus,
                            &identity_users,
                            LineFormValues::default(),
                            e,
                        ))
                    }
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
            let mut store = store.lock().await;
            let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
            let identity_users = identity::fetch_users().await.unwrap_or_default();
            match store.delete(&id).await {
                Ok(()) => Ok(redirect("/admin".to_string())),
                Err(StoreError::CartNotFound) => Err(warp::reject::not_found()),
                Err(e) => {
                    let carts = store.list().await.map_err(|_| warp::reject::not_found())?;
                    Ok(templates::render_index_html(
                        carts,
                        IndexContext {
                            catalog_skus: &catalog_skus,
                            identity_users: &identity_users,
                            catalog_configured: crate::config::catalog_configured(),
                            identity_configured: crate::config::identity_configured(),
                            catalog_error: None,
                            identity_error: None,
                            message: Some(format!("Delete failed: {e}")),
                        },
                    )
                    .map(|html| warp::reply::html(html).into_response())
                    .map_err(|_| warp::reject::not_found())?)
                }
            }
        })
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn redirect(location: String) -> warp::reply::Response {
    warp::redirect::redirect(warp::http::Uri::from_maybe_shared(location).unwrap()).into_response()
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

fn render_cart_form_error(
    carts: Vec<crate::model::Cart>,
    identity_users: &[identity::IdentityUser],
    cart: Option<crate::model::Cart>,
    values: CartFormValues,
    err: StoreError,
) -> warp::reply::Response {
    let message = err.to_string();
    match templates::render_cart_form_html_with_values(
        carts,
        identity_users,
        cart,
        Some(message),
        values,
    ) {
        Ok(html) => warp::reply::with_status(warp::reply::html(html), StatusCode::BAD_REQUEST)
            .into_response(),
        Err(_) => warp::reply::with_status(warp::reply(), StatusCode::INTERNAL_SERVER_ERROR)
            .into_response(),
    }
}

fn render_detail_error(
    cart: Option<crate::model::Cart>,
    catalog_skus: &[catalog::CatalogSku],
    identity_users: &[identity::IdentityUser],
    values: CartFormValues,
    err: StoreError,
) -> warp::reply::Response {
    let message = err.to_string();
    match cart {
        Some(cart) => match templates::render_detail_html_with_values(
            cart,
            catalog_skus,
            identity_users,
            Some(message),
            values,
            LineFormValues::default(),
        ) {
            Ok(html) => warp::reply::with_status(warp::reply::html(html), StatusCode::BAD_REQUEST)
                .into_response(),
            Err(_) => warp::reply::with_status(warp::reply(), StatusCode::INTERNAL_SERVER_ERROR)
                .into_response(),
        },
        None => warp::reply::with_status(warp::reply(), StatusCode::NOT_FOUND).into_response(),
    }
}

fn render_detail_line_error(
    cart: Option<crate::model::Cart>,
    catalog_skus: &[catalog::CatalogSku],
    identity_users: &[identity::IdentityUser],
    line_values: LineFormValues,
    err: StoreError,
) -> warp::reply::Response {
    let message = err.to_string();
    match cart {
        Some(cart) => {
            let cart_values = CartFormValues::from_cart(&cart);
            match templates::render_detail_html_with_values(
                cart,
                catalog_skus,
                identity_users,
                Some(message),
                cart_values,
                line_values,
            ) {
                Ok(html) => {
                    warp::reply::with_status(warp::reply::html(html), StatusCode::BAD_REQUEST)
                        .into_response()
                }
                Err(_) => {
                    warp::reply::with_status(warp::reply(), StatusCode::INTERNAL_SERVER_ERROR)
                        .into_response()
                }
            }
        }
        None => warp::reply::with_status(warp::reply(), StatusCode::NOT_FOUND).into_response(),
    }
}
