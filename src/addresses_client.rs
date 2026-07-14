//! Client for the addresses service internal JSON API.

mod address_summary;
mod addresses_client_error;
pub use address_summary::AddressSummary;
pub use addresses_client_error::AddressesClientError;

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
