//! [`CartDetail`].

use serde::Serialize;

use super::CartLineDetail;
use crate::identity::IdentityUser;
use crate::model::Cart;

/// Enriched cart on the wire (`{ cart, user, lines }`). Borrows everything it
/// renders so serving a cart never clones the cart, its lines, or its SKUs.
#[derive(Serialize)]
pub(crate) struct CartDetail<'a> {
    pub(crate) cart: &'a Cart,
    pub(crate) user: Option<&'a IdentityUser>,
    pub(crate) lines: Vec<CartLineDetail<'a>>,
}
