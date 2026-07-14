//! [`CartLineDetail`].

#[allow(unused_imports)]
use super::*;
use crate::catalog::CatalogSku;
use crate::model::CartLine;

#[derive(serde::Serialize)]
pub(crate) struct CartLineDetail {
    pub(crate) line: CartLine,
    pub(crate) sku: Option<CatalogSku>,
}
