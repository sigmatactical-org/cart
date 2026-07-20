//! [`StoreReject`].

/// Marker rejection for a store failure that has already been logged; recovery
/// renders the themed 500 page.
#[derive(Debug)]
pub(crate) struct StoreReject;
impl warp::reject::Reject for StoreReject {}
