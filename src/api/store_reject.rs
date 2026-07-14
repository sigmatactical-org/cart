//! [`StoreReject`].

#[allow(unused_imports)]
use super::*;
use crate::store::StoreError;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct StoreReject(pub(crate) StoreError);
impl warp::reject::Reject for StoreReject {}
