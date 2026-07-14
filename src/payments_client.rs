//! Client for the payments service internal JSON API.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct PaymentMethodSummary {
    pub id: String,
    pub method_type: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub brand: Option<String>,
    pub last4: String,
    #[serde(default)]
    pub is_default: bool,
}

impl PaymentMethodSummary {
    #[must_use]
    pub fn short_summary(&self) -> String {
        let brand = self
            .brand
            .as_deref()
            .or(self.label.as_deref())
            .unwrap_or(self.method_type.as_str());
        format!("{brand} ···· {}", self.last4)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Charge {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub failure_reason: Option<String>,
    pub amount_cents: u64,
}

#[derive(Debug, Serialize)]
struct CreateChargeBody<'a> {
    user_id: &'a str,
    payment_method_id: &'a str,
    amount_cents: u64,
    currency: &'a str,
    reference: &'a str,
}

#[derive(Debug, Error)]
pub enum PaymentsClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("payments request failed: {0}")]
    Request(String),
    #[error("payment declined: {0}")]
    Declined(String),
}

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
    let response =
        sigma_pg::clients::http::with_internal_auth(sigma_pg::clients::http::client().get(url))
            .send()
            .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(PaymentsClientError::Request(format!("{status}: {body}")));
    }
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
    let response = sigma_pg::clients::http::with_internal_auth(
        sigma_pg::clients::http::client().post(url).json(&body),
    )
    .send()
    .await?;
    let status = response.status();
    if status.as_u16() == 402 {
        let charge: Charge = response.json().await?;
        return Err(PaymentsClientError::Declined(
            charge
                .failure_reason
                .unwrap_or_else(|| "payment declined".to_string()),
        ));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(PaymentsClientError::Request(format!("{status}: {body}")));
    }
    Ok(response.json().await?)
}
