//! [`CreateCart`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateCart {
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}
