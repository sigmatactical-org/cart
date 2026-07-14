mod cart_detail;
mod cart_line_detail;
mod error_body;
mod store_reject;
pub(crate) use cart_detail::CartDetail;
pub(crate) use cart_line_detail::CartLineDetail;
pub(crate) use error_body::ErrorBody;
pub(crate) use store_reject::StoreReject;

use std::convert::Infallible;

use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::catalog::{self, CatalogSku};
use crate::identity::{self, IdentityUser};
use crate::model::{Cart, CreateCart, CreateLine, UpdateCart, UpdateLine};
use crate::store::StoreError;

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

fn internal_auth()
-> impl Filter<Extract = (Option<String>, Option<String>), Error = Rejection> + Clone {
    warp::header::optional::<String>("authorization")
        .and(warp::header::optional::<String>("x-sigma-internal-token"))
}

fn ensure_internal(
    authorization: Option<String>,
    internal_token: Option<String>,
) -> Result<(), Rejection> {
    if sigma_pg::clients::internal::authorize_internal(
        authorization.as_deref(),
        internal_token.as_deref(),
    ) {
        Ok(())
    } else {
        Err(warp::reject::not_found())
    }
}

fn enrich_cart(
    cart: Cart,
    skus: Option<&[CatalogSku]>,
    users: Option<&[IdentityUser]>,
) -> CartDetail {
    let user = cart
        .user_id
        .as_deref()
        .and_then(|id| users.and_then(|all| identity::user_by_id(all, id).cloned()));
    let lines = cart
        .lines
        .iter()
        .cloned()
        .map(|line| {
            let sku = skus.and_then(|all| catalog::sku_by_id(all, &line.sku_id).cloned());
            CartLineDetail { line, sku }
        })
        .collect();
    CartDetail { cart, user, lines }
}

async fn enrich_carts(carts: Vec<Cart>) -> Result<Vec<CartDetail>, Response> {
    let skus = catalog::fetch_skus().await.ok();
    let users = if crate::config::identity_configured() {
        Some(identity::fetch_users().await.map_err(|e| {
            json_error(
                StatusCode::BAD_GATEWAY,
                format!("identity lookup failed: {e}"),
            )
        })?)
    } else {
        None
    };
    Ok(carts
        .into_iter()
        .map(|cart| enrich_cart(cart, skus.as_deref(), users.as_deref()))
        .collect())
}

async fn validate_user(user_id: Option<&str>) -> Result<(), Response> {
    let Some(user_id) = user_id.filter(|s| !s.trim().is_empty()) else {
        return Ok(());
    };
    if !crate::config::identity_configured() {
        return Ok(());
    }
    let users = identity::fetch_users().await.map_err(|e| {
        json_error(
            StatusCode::BAD_GATEWAY,
            format!("identity lookup failed: {e}"),
        )
    })?;
    if identity::user_by_id(&users, user_id.trim()).is_none() {
        return Err(json_error(
            StatusCode::BAD_REQUEST,
            format!("identity user not found: {}", user_id.trim()),
        ));
    }
    Ok(())
}

async fn require_catalog_sku(sku_id: &str) -> Result<(), Response> {
    catalog::require_active_sku(sku_id).await.map_err(|e| {
        json_error(
            StatusCode::BAD_REQUEST,
            format!("catalog validation failed: {e}"),
        )
    })
}

/// Build this module's routes.
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
        .and(internal_auth())
        .and_then(|authorization, internal_token| async move {
            ensure_internal(authorization, internal_token)?;
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
        .and(internal_auth())
        .and(store)
        .and_then(
            |authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                let carts = store.list().await.map_err(|e| {
                    tracing::error!("list carts failed: {e}");
                    warp::reject::custom(StoreReject(e))
                })?;
                let details = match enrich_carts(carts).await {
                    Ok(details) => details,
                    Err(resp) => return Ok(resp),
                };
                Ok::<_, Rejection>(warp::reply::json(&details).into_response())
            },
        )
}

fn get_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String)
        .and(warp::path::end())
        .and(warp::get())
        .and(internal_auth())
        .and(store)
        .and_then(
            |id: String, authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                let Some(cart) = store.get(&id).await.map_err(|e| {
                    tracing::error!("get cart failed: {e}");
                    warp::reject::custom(StoreReject(e))
                })?
                else {
                    return Err(warp::reject::not_found());
                };
                let skus = catalog::fetch_skus().await.ok();
                let users = identity::fetch_users().await.ok();
                Ok(warp::reply::json(&enrich_cart(
                    cart,
                    skus.as_deref(),
                    users.as_deref(),
                )))
            },
        )
}

fn create_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path("carts")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(internal_auth())
        .and(store)
        .and_then(
            |input: CreateCart, authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                if let Err(resp) = validate_user(input.user_id.as_deref()).await {
                    return Ok(resp);
                }
                let response = match store.create(input).await {
                    Ok(cart) => {
                        let skus = catalog::fetch_skus().await.ok();
                        let users = identity::fetch_users().await.ok();
                        let detail = enrich_cart(cart, skus.as_deref(), users.as_deref());
                        warp::reply::with_status(warp::reply::json(&detail), StatusCode::CREATED)
                            .into_response()
                    }
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok::<_, Rejection>(response)
            },
        )
}

fn update_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String)
        .and(warp::path::end())
        .and(warp::put())
        .and(warp::body::json())
        .and(internal_auth())
        .and(store)
        .and_then(
            |id: String,
             input: UpdateCart,
             authorization,
             internal_token,
             store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                if let Err(resp) = validate_user(input.user_id.as_deref()).await {
                    return Ok(resp);
                }
                let response = match store.update(&id, input).await {
                    Ok(cart) => {
                        let skus = catalog::fetch_skus().await.ok();
                        let users = identity::fetch_users().await.ok();
                        warp::reply::json(&enrich_cart(cart, skus.as_deref(), users.as_deref()))
                            .into_response()
                    }
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
        .and(internal_auth())
        .and(store)
        .and_then(
            |id: String, authorization, internal_token, store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                let response = match store.delete(&id).await {
                    Ok(()) => warp::reply::with_status(warp::reply(), StatusCode::NO_CONTENT)
                        .into_response(),
                    Err(StoreError::CartNotFound) => return Err(warp::reject::not_found()),
                    Err(e) => json_error(store_error_status(&e), e.to_string()),
                };
                Ok(response)
            },
        )
}

fn add_line(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String / "lines")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(internal_auth())
        .and(store)
        .and_then(
            |cart_id: String,
             input: CreateLine,
             authorization,
             internal_token,
             store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
                if let Err(resp) = require_catalog_sku(input.sku_id.trim()).await {
                    return Ok(resp);
                }
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
        .and(internal_auth())
        .and(store)
        .and_then(
            |cart_id: String,
             line_id: String,
             input: UpdateLine,
             authorization,
             internal_token,
             store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
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
        .and(internal_auth())
        .and(store)
        .and_then(
            |cart_id: String,
             line_id: String,
             authorization,
             internal_token,
             store: SharedStore| async move {
                ensure_internal(authorization, internal_token)?;
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
