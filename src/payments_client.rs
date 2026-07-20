//! Client for the payments service internal JSON API.

mod charge;
mod create_charge_body;
mod payment_method_summary;
mod payments_client_error;
pub use charge::Charge;
pub(crate) use create_charge_body::CreateChargeBody;
pub use payment_method_summary::PaymentMethodSummary;
pub use payments_client_error::PaymentsClientError;

use sigma_pg::clients::http;

fn payments_url(path: &str) -> Result<String, PaymentsClientError> {
    let base = crate::config::payments_internal_base_url().ok_or_else(|| {
        PaymentsClientError::Request("payments service not configured".to_string())
    })?;
    Ok(format!("{base}{}", path.trim_start_matches('/')))
}

pub async fn list_payment_methods(
    user_id: &str,
) -> Result<Vec<PaymentMethodSummary>, PaymentsClientError> {
    let url = payments_url(&format!("api/users/{user_id}/payment-methods"))?;
    let response = http::with_internal_auth(http::client().get(url))
        .send()
        .await?;
    let response = http::ensure_success(response)
        .await
        .map_err(PaymentsClientError::Request)?;
    Ok(response.json().await?)
}

pub async fn create_charge(
    user_id: &str,
    payment_method_id: &str,
    amount_cents: u64,
    reference: &str,
) -> Result<Charge, PaymentsClientError> {
    let url = payments_url("api/charges")?;
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
