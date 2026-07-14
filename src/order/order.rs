//! [`Order`].

#[allow(unused_imports)]
use super::*;

/// Order returned by the order service (confirmation page).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Order {
    pub id: String,
    pub username: String,
    pub lines: Vec<OrderLine>,
    pub subtotal_cents: u64,
    pub deposit_cents: u64,
}
