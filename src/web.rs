use std::convert::Infallible;

use warp::http::StatusCode;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::catalog;
use crate::identity;
use crate::model::{CartForm, LineForm};
use crate::store::StoreError;
use crate::templates::{self, CartFormValues, IndexContext, LineFormValues};

pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    index_page(store.clone())
        .or(new_cart_page(store.clone()))
        .or(create_cart_form(store.clone()))
        .or(cart_detail_page(store.clone()))
        .or(update_cart_form(store.clone()))
        .or(add_line_form(store.clone()))
        .or(delete_line_form(store.clone()))
        .or(delete_cart_form(store))
}

fn index_page(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path::end()
        .and(warp::get())
        .and(store)
        .and_then(|store: SharedStore| async move {
            let carts = store.lock().await.list();
            let catalog_result = catalog::fetch_skus().await;
            let identity_result = identity::fetch_users().await;
            let (catalog_skus, catalog_error) = match catalog_result {
                Ok(skus) => (Some(skus), None),
                Err(e) => (None, Some(e.to_string())),
            };
            let (identity_users, identity_error) = match identity_result {
                Ok(users) => (Some(users), None),
                Err(e) => (None, Some(e.to_string())),
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

fn new_cart_page(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("carts")
        .and(warp::path("new"))
        .and(warp::path::end())
        .and(warp::get())
        .and(store)
        .and_then(|store: SharedStore| async move {
            let carts = store.lock().await.list();
            let identity_users = identity::fetch_users().await.unwrap_or_default();
            templates::render_cart_form_html(carts, &identity_users, None, None)
                .map(warp::reply::html)
                .map_err(|_| warp::reject::not_found())
        })
}

fn create_cart_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("carts")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::form())
        .and(store)
        .and_then(|form: CartForm, store: SharedStore| async move {
            let mut store = store.lock().await;
            let carts = store.list();
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
                        match store.create(input) {
                            Ok(cart) => warp::redirect::redirect(
                                warp::http::Uri::from_maybe_shared(format!("/carts/{}", cart.id))
                                    .unwrap(),
                            )
                            .into_response(),
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

fn cart_detail_page(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String)
        .and(warp::path::end())
        .and(warp::get())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            let store = store.lock().await;
            let Some(cart) = store.get(&id) else {
                return Err(warp::reject::not_found());
            };
            let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
            let identity_users = identity::fetch_users().await.unwrap_or_default();
            templates::render_detail_html(cart, &catalog_skus, &identity_users, None, None)
                .map(warp::reply::html)
                .map_err(|_| warp::reject::not_found())
        })
}

fn update_cart_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String / "edit")
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
                            let cart = store.get(&id);
                            render_detail_error(
                                cart,
                                &catalog_skus,
                                &identity_users,
                                values,
                                invalid_input("identity user not found".to_string()),
                            )
                        } else {
                            match store.update(&id, input) {
                                Ok(cart) => warp::redirect::redirect(
                                    warp::http::Uri::from_maybe_shared(format!(
                                        "/carts/{}",
                                        cart.id
                                    ))
                                    .unwrap(),
                                )
                                .into_response(),
                                Err(e) => {
                                    let cart = store.get(&id);
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
                        let cart = store.get(&id);
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

fn add_line_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String / "lines")
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
                            let cart = store.get(&cart_id);
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
                            match store.add_line(&cart_id, input) {
                                Ok(_) => warp::redirect::redirect(
                                    warp::http::Uri::from_maybe_shared(format!("/carts/{cart_id}"))
                                        .unwrap(),
                                )
                                .into_response(),
                                Err(StoreError::CartNotFound) => {
                                    return Err(warp::reject::not_found());
                                }
                                Err(e) => {
                                    let cart = store.get(&cart_id);
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
                        let cart = store.get(&cart_id);
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

fn delete_line_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String / "lines" / String / "delete")
        .and(warp::post())
        .and(store)
        .and_then(
            |cart_id: String, line_id: String, store: SharedStore| async move {
                let mut store = store.lock().await;
                let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
                let identity_users = identity::fetch_users().await.unwrap_or_default();
                match store.delete_line(&cart_id, &line_id) {
                    Ok(()) => Ok(warp::redirect::redirect(
                        warp::http::Uri::from_maybe_shared(format!("/carts/{cart_id}")).unwrap(),
                    )
                    .into_response()),
                    Err(StoreError::CartNotFound | StoreError::LineNotFound) => {
                        Err(warp::reject::not_found())
                    }
                    Err(e) => {
                        let cart = store.get(&cart_id);
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

fn delete_cart_form(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String / "delete")
        .and(warp::post())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            let mut store = store.lock().await;
            let catalog_skus = catalog::fetch_skus().await.unwrap_or_default();
            let identity_users = identity::fetch_users().await.unwrap_or_default();
            match store.delete(&id) {
                Ok(()) => {
                    Ok(warp::redirect::redirect(warp::http::Uri::from_static("/")).into_response())
                }
                Err(StoreError::CartNotFound) => Err(warp::reject::not_found()),
                Err(e) => templates::render_index_html(
                    store.list(),
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
                .map_err(|_| warp::reject::not_found()),
            }
        })
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
    StoreError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
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
