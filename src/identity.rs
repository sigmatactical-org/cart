use thiserror::Error;

use crate::config;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IdentityUser {
    pub id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error(
        "identity integration is not configured (set CART_IDENTITY_ISSUER_URL, CART_IDENTITY_CLIENT_ID, CART_IDENTITY_CLIENT_SECRET)"
    )]
    NotConfigured,
    #[error("invalid issuer URL: {0}")]
    InvalidIssuer(String),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Keycloak token request failed: {0}")]
    Token(String),
    #[error("Keycloak user listing failed: {0}")]
    Users(String),
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(serde::Deserialize)]
struct KeycloakUser {
    id: String,
    username: Option<String>,
    email: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    enabled: Option<bool>,
}

struct IssuerParts {
    admin_base: String,
    realm: String,
}

fn parse_issuer(issuer: &str) -> Result<IssuerParts, IdentityError> {
    let issuer = issuer.trim().trim_end_matches('/');
    let Some((base, realm)) = issuer.rsplit_once("/realms/") else {
        return Err(IdentityError::InvalidIssuer(
            "expected URL ending with /realms/{realm}".to_string(),
        ));
    };
    if realm.is_empty() {
        return Err(IdentityError::InvalidIssuer(
            "missing realm name".to_string(),
        ));
    }
    Ok(IssuerParts {
        admin_base: base.to_string(),
        realm: realm.to_string(),
    })
}

async fn fetch_access_token(
    client: &reqwest::Client,
    issuer: &IssuerParts,
    client_id: &str,
    client_secret: &str,
) -> Result<String, IdentityError> {
    let token_url = format!(
        "{}/realms/{}/protocol/openid-connect/token",
        issuer.admin_base, issuer.realm
    );
    let response = client
        .post(token_url)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(IdentityError::Token(format!("{status}: {body}")));
    }

    let token: TokenResponse = response.json().await?;
    Ok(token.access_token)
}

fn display_name_for_user(user: &KeycloakUser) -> String {
    let first = user.first_name.as_deref().unwrap_or("").trim();
    let last = user.last_name.as_deref().unwrap_or("").trim();
    let full = format!("{first} {last}").trim().to_string();
    if !full.is_empty() {
        return full;
    }
    user.email
        .clone()
        .or_else(|| user.username.clone())
        .unwrap_or_else(|| user.id.clone())
}

/// Pull enabled realm users from the identity provider (Keycloak Admin API).
pub async fn fetch_users() -> Result<Vec<IdentityUser>, IdentityError> {
    let issuer_url = config::identity_issuer_url().ok_or(IdentityError::NotConfigured)?;
    let client_id = config::identity_client_id().ok_or(IdentityError::NotConfigured)?;
    let client_secret = config::identity_client_secret().ok_or(IdentityError::NotConfigured)?;
    let issuer = parse_issuer(&issuer_url)?;

    let http = reqwest::Client::new();
    let token = fetch_access_token(&http, &issuer, &client_id, &client_secret).await?;

    let users_url = format!(
        "{}/admin/realms/{}/users?max=1000",
        issuer.admin_base, issuer.realm
    );
    let response = http.get(users_url).bearer_auth(token).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(IdentityError::Users(format!("{status}: {body}")));
    }

    let users: Vec<KeycloakUser> = response.json().await?;
    Ok(users
        .into_iter()
        .filter(|u| u.enabled.unwrap_or(true))
        .filter(|u| {
            !u.username
                .as_deref()
                .is_some_and(|n| n.starts_with("service-account-"))
        })
        .map(|u| IdentityUser {
            id: u.id.clone(),
            display_name: display_name_for_user(&u),
            email: u.email.filter(|e| !e.is_empty()),
        })
        .collect())
}

#[must_use]
pub fn user_by_id<'a>(users: &'a [IdentityUser], id: &str) -> Option<&'a IdentityUser> {
    users.iter().find(|u| u.id == id)
}
