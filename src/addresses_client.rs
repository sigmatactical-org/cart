//! Client for the addresses service internal JSON API.

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AddressSummary {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub line1: String,
    pub city: String,
    #[serde(default)]
    pub region: Option<String>,
    pub postal_code: String,
    pub country: String,
    pub category: String,
    #[serde(default)]
    pub is_default: bool,
}

impl AddressSummary {
    #[must_use]
    pub fn short_summary(&self) -> String {
        format!("{}, {}", self.line1, self.city)
    }
}

#[derive(Debug, Error)]
pub enum AddressesClientError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("addresses request failed: {0}")]
    Request(String),
}

fn addresses_url(path: &str) -> Result<String, AddressesClientError> {
    let base = crate::config::addresses_internal_base_url().ok_or_else(|| {
        AddressesClientError::Request("addresses service not configured".to_string())
    })?;
    Ok(format!("{base}{}", path.trim_start_matches('/')))
}

pub async fn list_addresses(
    user_id: &str,
    category: &str,
) -> Result<Vec<AddressSummary>, AddressesClientError> {
    let url = addresses_url(&format!(
        "api/users/{user_id}/addresses?category={category}"
    ))?;
    let response =
        sigma_pg::clients::http::with_internal_auth(sigma_pg::clients::http::client().get(url))
            .send()
            .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AddressesClientError::Request(format!("{status}: {body}")));
    }
    Ok(response.json().await?)
}
