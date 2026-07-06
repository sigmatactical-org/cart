use std::convert::Infallible;

use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::catalog::{self, CatalogSku};
use crate::identity::{self, IdentityUser};
use crate::model::{Cart, CartLine, CreateCart, CreateLine, UpdateCart, UpdateLine};
use crate::store::StoreError;

#[derive(serde::Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(serde::Serialize)]
struct CartLineDetail {
    line: CartLine,
    sku: Option<CatalogSku>,
}

#[derive(serde::Serialize)]
struct CartDetail {
    cart: Cart,
    user: Option<IdentityUser>,
    lines: Vec<CartLineDetail>,
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    warp::reply::with_status(
        warp::reply::json(&ErrorBody {
            error: message.into(),
        }),
        status,
    )
    .into_response()
}

fn store_error_status(err: &StoreError) -> StatusCode {
    match err {
        StoreError::CartNotFound | StoreError::LineNotFound => StatusCode::NOT_FOUND,
        StoreError::SkuIdRequired
        | StoreError::InvalidQuantity
        | StoreError::CartNotOpen
        | StoreError::UserNotFound(_)
        | StoreError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        StoreError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn enrich_cart(cart: Cart) -> CartDetail {
    let skus = catalog::fetch_skus().await.ok();
    let users = identity::fetch_users().await.ok();
    let user = cart.user_id.as_deref().and_then(|id| {
        users
            .as_ref()
            .and_then(|all| identity::user_by_id(all, id))
            .cloned()
    });
    let lines = cart
        .lines
        .iter()
        .cloned()
        .map(|line| {
            let sku = skus
                .as_ref()
                .and_then(|all| catalog::sku_by_id(all, &line.sku_id).cloned());
            CartLineDetail { line, sku }
        })
        .collect();
    CartDetail { cart, user, lines }
}

async fn validate_user(user_id: Option<&str>) -> Result<(), Response> {
    let Some(user_id) = user_id.filter(|s| !s.trim().is_empty()) else {
        return Ok(());
    };
    if !crate::config::identity_configured() {
        return Ok(());
    }
    let users = match identity::fetch_users().await {
        Ok(users) => users,
        Err(e) => {
            return Err(json_error(
                StatusCode::BAD_GATEWAY,
                format!("identity lookup failed: {e}"),
            ));
        }
    };
    if identity::user_by_id(&users, user_id.trim()).is_none() {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            format!("identity user not found: {}", user_id.trim()),
        ));
    }
    Ok(())
}

pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    list_users(store.clone())
        .or(list_carts(store.clone()))
        .or(get_cart(store.clone()))
        .or(create_cart(store.clone()))
        .or(update_cart(store.clone()))
        .or(delete_cart(store.clone()))
        .or(add_line(store.clone()))
        .or(update_line(store.clone()))
        .or(delete_line(store))
}

fn list_users(
    _store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("users")
        .and(warp::path::end())
        .and(warp::get())
        .and_then(|| async move {
            let response = match identity::fetch_users().await {
                Ok(users) => warp::reply::json(&users).into_response(),
                Err(e) => json_error(StatusCode::BAD_GATEWAY, e.to_string()),
            };
            Ok::<_, Rejection>(response)
        })
}

fn list_carts(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("carts")
        .and(warp::path::end())
        .and(warp::get())
        .and(store)
        .and_then(|store: SharedStore| async move {
            let store = store.lock().await;
            let carts = store.list().await.map_err(|_| warp::reject::not_found())?;
            let mut details = Vec::with_capacity(carts.len());
            for cart in carts {
                details.push(enrich_cart(cart).await);
            }
            Ok::<_, Rejection>(warp::reply::json(&details))
        })
}

fn get_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String)
        .and(warp::path::end())
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
            Ok(warp::reply::json(&enrich_cart(cart).await))
        })
}

fn create_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("carts")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(store)
        .and_then(|input: CreateCart, store: SharedStore| async move {
            if let Err(resp) = validate_user(input.user_id.as_deref()).await {
                return Ok(resp);
            }
            let mut store = store.lock().await;
            let response = match store.create(input).await {
                Ok(cart) => {
                    let detail = enrich_cart(cart).await;
                    warp::reply::with_status(warp::reply::json(&detail), StatusCode::CREATED)
                        .into_response()
                }
                Err(e) => json_error(store_error_status(&e), e.to_string()),
            };
            Ok::<_, Rejection>(response)
        })
}

fn update_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String)
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(store)
        .and_then(
            |id: String, input: UpdateCart, store: SharedStore| async move {
                if let Err(resp) = validate_user(input.user_id.as_deref()).await {
                    return Ok(resp);
                }
                let mut store = store.lock().await;
                let response = match store.update(&id, input).await {
                    Ok(cart) => warp::reply::json(&enrich_cart(cart).await).into_response(),
                    Err(StoreError::CartNotFound) => return Err(warp::reject::not_found()),
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok(response)
            },
        )
}

fn delete_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String)
        .and(warp::path::end())
        .and(warp::delete())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            let mut store = store.lock().await;
            let response = match store.delete(&id).await {
                Ok(()) => {
                    warp::reply::with_status(warp::reply(), StatusCode::NO_CONTENT).into_response()
                }
                Err(StoreError::CartNotFound) => return Err(warp::reject::not_found()),
                Err(e) => json_error(store_error_status(&e), e.to_string()),
            };
            Ok(response)
        })
}

fn add_line(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String / "lines")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(store)
        .and_then(
            |cart_id: String, input: CreateLine, store: SharedStore| async move {
                if let Ok(skus) = catalog::fetch_skus().await
                    && catalog::validate_sku_id(&skus, input.sku_id.trim()).is_err()
                {
                    return Ok(json_error(
                        StatusCode::BAD_REQUEST,
                        format!("catalog sku not found: {}", input.sku_id.trim()),
                    ));
                }
                let mut store = store.lock().await;
                let response = match store.add_line(&cart_id, input).await {
                    Ok(line) => {
                        warp::reply::with_status(warp::reply::json(&line), StatusCode::CREATED)
                            .into_response()
                    }
                    Err(StoreError::CartNotFound) => return Err(warp::reject::not_found()),
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok::<_, Rejection>(response)
            },
        )
}

fn update_line(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String / "lines" / String)
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(store)
        .and_then(
            |cart_id: String, line_id: String, input: UpdateLine, store: SharedStore| async move {
                let mut store = store.lock().await;
                let response = match store.update_line(&cart_id, &line_id, input).await {
                    Ok(line) => warp::reply::json(&line).into_response(),
                    Err(StoreError::CartNotFound | StoreError::LineNotFound) => {
                        return Err(warp::reject::not_found());
                    }
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok(response)
            },
        )
}

fn delete_line(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String / "lines" / String)
        .and(warp::path::end())
        .and(warp::delete())
        .and(store)
        .and_then(
            |cart_id: String, line_id: String, store: SharedStore| async move {
                let mut store = store.lock().await;
                let response = match store.delete_line(&cart_id, &line_id).await {
                    Ok(()) => warp::reply::with_status(warp::reply(), StatusCode::NO_CONTENT)
                        .into_response(),
                    Err(StoreError::CartNotFound | StoreError::LineNotFound) => {
                        return Err(warp::reject::not_found());
                    }
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok(response)
            },
        )
}
