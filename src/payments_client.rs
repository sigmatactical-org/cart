//! Client for the payments service internal JSON API.
//!
//! Every call takes the service's base URL rather than reading it from the
//! environment, matching the shared clients in `sigma_pg::clients` and letting
//! checkout tests point these calls at a stub.

mod charge;
mod create_charge_body;
mod payment_method_summary;
mod payments_client_error;
mod refund_charge_body;
pub use charge::Charge;
pub(crate) use create_charge_body::CreateChargeBody;
pub use payment_method_summary::PaymentMethodSummary;
pub use payments_client_error::PaymentsClientError;
pub(crate) use refund_charge_body::RefundChargeBody;

use sigma_pg::clients::http;

fn payments_url(base_url: Option<&str>, path: &str) -> Result<String, PaymentsClientError> {
    let base = base_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            PaymentsClientError::Request("payments service not configured".to_string())
        })?;
    Ok(format!(
        "{}{}",
        http::normalize_base_url(base),
        path.trim_start_matches('/')
    ))
}

pub async fn list_payment_methods(
    base_url: Option<&str>,
    user_id: &str,
) -> Result<Vec<PaymentMethodSummary>, PaymentsClientError> {
    let url = payments_url(base_url, &format!("api/users/{user_id}/payment-methods"))?;
    let response = http::with_internal_auth(http::client().get(url))
        .send()
        .await?;
    let response = http::ensure_success(response)
        .await
        .map_err(PaymentsClientError::Request)?;
    Ok(response.json().await?)
}

/// Reverse a charge in full: the compensating action when a deposit was taken
/// for a checkout that could not be completed.
///
/// Idempotent on the payments side, so repeating this after a timeout returns
/// the original refund rather than issuing a second credit.
pub async fn refund_charge(
    base_url: Option<&str>,
    charge_id: &str,
    reason: &str,
) -> Result<(), PaymentsClientError> {
    let url = payments_url(base_url, &format!("api/charges/{charge_id}/refund"))?;
    let response =
        http::with_internal_auth(http::client().post(url).json(&RefundChargeBody { reason }))
            .send()
            .await?;
    http::ensure_success(response)
        .await
        .map_err(PaymentsClientError::Request)?;
    Ok(())
}

/// Charge a saved payment method.
///
/// `reference` is the caller's idempotency key: payments collapses a repeat
/// charge for the same reference onto the original rather than taking the money
/// twice.
pub async fn create_charge(
    base_url: Option<&str>,
    user_id: &str,
    payment_method_id: &str,
    amount_cents: u64,
    reference: &str,
) -> Result<Charge, PaymentsClientError> {
    let url = payments_url(base_url, "api/charges")?;
    let body = CreateChargeBody {
        user_id,
        payment_method_id,
        amount_cents,
        currency: "usd",
        reference,
    };
    let response = http::with_internal_auth(http::client().post(url).json(&body))
        .send()
        .await?;
    // A declined card is a 402 carrying the charge, not a transport failure.
    if response.status().as_u16() == 402 {
        let charge: Charge = response.json().await?;
        return Err(PaymentsClientError::Declined(
            charge
                .failure_reason
                .unwrap_or_else(|| "payment declined".to_string()),
        ));
    }
    let response = http::ensure_success(response)
        .await
        .map_err(PaymentsClientError::Request)?;
    Ok(response.json().await?)
}
