use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrderError {
    #[error("order service not configured")]
    NotConfigured,
    #[error("order service request failed: {0}")]
    Request(String),
}

/// Line payload for `POST /orders`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CreateOrderLine {
    pub sku_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub unit_price_cents: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_total_cents: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_cents: Option<u64>,
}

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
}

/// Order returned by the order service (confirmation page).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Order {
    pub id: String,
    pub username: String,
    pub lines: Vec<OrderLine>,
    pub subtotal_cents: u64,
    pub deposit_cents: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OrderLine {
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub line_total_cents: u64,
}

/// Legacy reservation row stored in `cart.snapshot` before the order service existed.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LegacyReservation {
    pub id: String,
    pub cart_id: String,
    pub username: String,
    pub lines: Vec<LegacyReservationLine>,
    pub subtotal_cents: u64,
    pub deposit_cents: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LegacyReservationLine {
    pub sku_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub unit_price_cents: u64,
    pub line_total_cents: u64,
    pub deposit_cents: u64,
}

/// Create a committed order in the order service.
pub async fn create_order(input: CreateOrderRequest) -> Result<Order, OrderError> {
    let base = crate::config::order_base_url().ok_or(OrderError::NotConfigured)?;
    let url = format!("{}orders", base);
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .json(&input)
        .send()
        .await
        .map_err(|e| OrderError::Request(e.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(OrderError::Request(format!("HTTP {status}: {body}")));
    }
    response
        .json()
        .await
        .map_err(|e| OrderError::Request(e.to_string()))
}

/// Import legacy cart reservations into the order service (one-time migration).
pub async fn migrate_reservations(reservations: &[LegacyReservation]) -> Result<(), OrderError> {
    for reservation in reservations {
        create_order(CreateOrderRequest {
            cart_id: reservation.cart_id.clone(),
            username: reservation.username.clone(),
            user_id: None,
            lines: reservation
                .lines
                .iter()
                .map(|line| CreateOrderLine {
                    sku_id: line.sku_id.clone(),
                    sku_code: line.sku_code.clone(),
                    name: line.name.clone(),
                    quantity: line.quantity,
                    unit_price_cents: line.unit_price_cents,
                    line_total_cents: Some(line.line_total_cents),
                    deposit_cents: Some(line.deposit_cents),
                })
                .collect(),
            id: Some(reservation.id.clone()),
            status: Some("pending_deposit".to_string()),
            subtotal_cents: Some(reservation.subtotal_cents),
            deposit_cents: Some(reservation.deposit_cents),
            created_at: Some(reservation.created_at.clone()),
        })
        .await?;
    }
    Ok(())
}
