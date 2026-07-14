//! [`CartDetail`].

#[allow(unused_imports)]
use super::*;
use crate::identity::IdentityUser;
use crate::model::Cart;

#[derive(serde::Serialize)]
pub(crate) struct CartDetail {
    pub(crate) cart: Cart,
    pub(crate) user: Option<IdentityUser>,
    pub(crate) lines: Vec<CartLineDetail>,
}
