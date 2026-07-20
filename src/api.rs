//! Internal JSON API (`/users`, `/carts`, ...), gated on service-to-service auth.

mod cart_detail;
mod cart_line_detail;
mod store_reject;
pub(crate) use cart_detail::CartDetail;
pub(crate) use cart_line_detail::CartLineDetail;
pub(crate) use store_reject::StoreReject;

use std::convert::Infallible;
use std::sync::Arc;

use sigma_pg::api::{internal_auth, json_error};
use warp::http::StatusCode;
use warp::reply::Response;
use warp::{Filter, Rejection, Reply};

use crate::SharedStore;
use crate::catalog::{self, CatalogSku};
use crate::identity::{self, IdentityUser};
use crate::model::{Cart, CreateCart, CreateLine, UpdateCart, UpdateLine};
use crate::store::StoreError;

fn store_error_status(err: &StoreError) -> StatusCode {
    match err {
        StoreError::CartNotFound | StoreError::LineNotFound => StatusCode::NOT_FOUND,
        StoreError::SkuIdRequired
        | StoreError::InvalidQuantity
        | StoreError::CartNotOpen
        | StoreError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        StoreError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn store_error_reply(err: &StoreError) -> Response {
    json_error(store_error_status(err), err.to_string())
}

fn enrich_cart<'a>(
    cart: &'a Cart,
    skus: &'a [CatalogSku],
    users: &'a [IdentityUser],
) -> CartDetail<'a> {
    let user = cart
        .user_id
        .as_deref()
        .and_then(|id| identity::user_by_id(users, id));
    let lines = cart
        .lines
        .iter()
        .map(|line| CartLineDetail {
            line,
            sku: catalog::sku_by_id(skus, &line.sku_id),
        })
        .collect();
    CartDetail { cart, user, lines }
}

/// Catalog SKUs and identity users for enrichment, fetched concurrently.
/// Both are optional: enrichment degrades to ids when an upstream is down.
async fn enrichment_sources() -> (Arc<Vec<CatalogSku>>, Arc<Vec<IdentityUser>>) {
    let (skus, users) = tokio::join!(catalog::fetch_skus(), identity::fetch_users());
    (skus.unwrap_or_default(), users.unwrap_or_default())
}

/// Same as [`enrichment_sources`], but a configured-yet-failing identity is a
/// hard error: list responses must not silently drop user attribution.
async fn required_enrichment_sources()
-> Result<(Arc<Vec<CatalogSku>>, Arc<Vec<IdentityUser>>), Response> {
    let (skus, users) = tokio::join!(catalog::fetch_skus(), identity::fetch_users());
    let users = match users {
        Ok(users) => users,
        Err(e) if crate::config::identity_configured() => {
            return Err(json_error(
                StatusCode::BAD_GATEWAY,
                format!("identity lookup failed: {e}"),
            ));
        }
        Err(_) => Arc::default(),
    };
    Ok((skus.unwrap_or_default(), users))
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

/// Serialize a cart enriched with catalog and identity data.
async fn enriched_reply(cart: &Cart, status: StatusCode) -> Response {
    let (skus, users) = enrichment_sources().await;
    warp::reply::with_status(warp::reply::json(&enrich_cart(cart, &skus, &users)), status)
        .into_response()
}

fn store_rejection(context: &str, err: StoreError) -> Rejection {
    tracing::error!("{context} failed: {err}");
    warp::reject::custom(StoreReject)
}

/// Build this module's routes.
pub fn routes(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    list_users()
        .or(list_carts(store.clone()))
        .or(get_cart(store.clone()))
        .or(create_cart(store.clone()))
        .or(update_cart(store.clone()))
        .or(delete_cart(store.clone()))
        .or(add_line(store.clone()))
        .or(update_line(store.clone()))
        .or(delete_line(store))
}

fn list_users() -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static
{
    warp::path("users")
        .and(warp::path::end())
        .and(warp::get())
        .and(internal_auth())
        .and_then(|| async move {
            let response = match identity::fetch_users().await {
                Ok(users) => warp::reply::json(&*users).into_response(),
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
        .and_then(|store: SharedStore| async move {
            let carts = store
                .list()
                .await
                .map_err(|e| store_rejection("list carts", e))?;
            let (skus, users) = match required_enrichment_sources().await {
                Ok(sources) => sources,
                Err(resp) => return Ok(resp),
            };
            let details: Vec<_> = carts
                .iter()
                .map(|cart| enrich_cart(cart, &skus, &users))
                .collect();
            Ok::<_, Rejection>(warp::reply::json(&details).into_response())
        })
}

fn get_cart(
    store: impl Filter<Extract = (SharedStore,), Error = Infallible> + Clone + Send + 'static,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone + Send + 'static {
    warp::path!("carts" / String)
        .and(warp::path::end())
        .and(warp::get())
        .and(internal_auth())
        .and(store)
        .and_then(|id: String, store: SharedStore| async move {
            let Some(cart) = store
                .get(&id)
                .await
                .map_err(|e| store_rejection("get cart", e))?
            else {
                return Err(warp::reject::not_found());
            };
            Ok(enriched_reply(&cart, StatusCode::OK).await)
        })
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
        .and_then(|input: CreateCart, store: SharedStore| async move {
            if let Err(resp) = validate_user(input.user_id.as_deref()).await {
                return Ok(resp);
            }
            let response = match store.create(input).await {
                Ok(cart) => enriched_reply(&cart, StatusCode::CREATED).await,
                Err(e) => store_error_reply(&e),
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
        .and(internal_auth())
        .and(store)
        .and_then(
            |id: String, input: UpdateCart, store: SharedStore| async move {
                if let Err(resp) = validate_user(input.user_id.as_deref()).await {
                    return Ok(resp);
                }
                let response = match store.update(&id, input).await {
                    Ok(cart) => enriched_reply(&cart, StatusCode::OK).await,
                    Err(StoreError::CartNotFound) => return Err(warp::reject::not_found()),
                    Err(e) => store_error_reply(&e),
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
        .and_then(|id: String, store: SharedStore| async move {
            let response = match store.delete(&id).await {
                Ok(()) => no_content(),
                Err(StoreError::CartNotFound) => return Err(warp::reject::not_found()),
                Err(e) => store_error_reply(&e),
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
        .and(internal_auth())
        .and(store)
        .and_then(
            |cart_id: String, input: CreateLine, store: SharedStore| async move {
                if let Err(resp) = require_catalog_sku(input.sku_id.trim()).await {
                    return Ok(resp);
                }
                let response = match store.add_line(&cart_id, input).await {
                    Ok(line) => {
                        warp::reply::with_status(warp::reply::json(&line), StatusCode::CREATED)
                            .into_response()
                    }
                    Err(StoreError::CartNotFound) => return Err(warp::reject::not_found()),
                    Err(e) => store_error_reply(&e),
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
            |cart_id: String, line_id: String, input: UpdateLine, store: SharedStore| async move {
                let response = match store.update_line(&cart_id, &line_id, input).await {
                    Ok(line) => warp::reply::json(&line).into_response(),
                    Err(StoreError::CartNotFound | StoreError::LineNotFound) => {
                        return Err(warp::reject::not_found());
                    }
                    Err(e) => store_error_reply(&e),
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
            |cart_id: String, line_id: String, store: SharedStore| async move {
                let response = match store.delete_line(&cart_id, &line_id).await {
                    Ok(()) => no_content(),
                    Err(StoreError::CartNotFound | StoreError::LineNotFound) => {
                        return Err(warp::reject::not_found());
                    }
                    Err(e) => store_error_reply(&e),
                };
                Ok(response)
            },
        )
}

fn no_content() -> Response {
    warp::reply::with_status(warp::reply(), StatusCode::NO_CONTENT).into_response()
}
