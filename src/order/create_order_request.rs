//! [`CreateOrderRequest`].

#[allow(unused_imports)]
use super::*;

/// Request body for `POST /orders`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CreateOrderRequest {
    pub cart_id: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub lines: Vec<CreateOrderLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtotal_cents: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_cents: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub billing_address_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shipping_address_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_method_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charge_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms_accepted_at: Option<String>,
}
