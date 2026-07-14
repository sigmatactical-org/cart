mod create_order_line;
mod create_order_request;
mod order;
mod order_error;
mod order_line;
pub use create_order_line::CreateOrderLine;
pub use create_order_request::CreateOrderRequest;
pub use order::Order;
pub use order_error::OrderError;
pub use order_line::OrderLine;

/// Create a committed order in the orders service.
pub async fn create_order(input: CreateOrderRequest) -> Result<Order, OrderError> {
    let base = crate::config::orders_base_url().ok_or(OrderError::NotConfigured)?;
    let url = format!("{}orders", base);
    let mut request = sigma_pg::clients::http::client().post(url).json(&input);
    if let Some(token) = sigma_pg::clients::internal::internal_token() {
        request = request.bearer_auth(token);
    }
    let response = request
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
