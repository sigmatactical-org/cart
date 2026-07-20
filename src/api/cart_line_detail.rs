//! [`CartLineDetail`].

use serde::Serialize;

use crate::catalog::CatalogSku;
use crate::model::CartLine;

#[derive(Serialize)]
pub(crate) struct CartLineDetail<'a> {
    pub(crate) line: &'a CartLine,
    pub(crate) sku: Option<&'a CatalogSku>,
}
