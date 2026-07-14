//! [`UpdateCart`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCart {
    #[serde(default)]
    pub user_id: Option<String>,
    pub status: CartStatus,
    #[serde(default)]
    pub note: Option<String>,
}
