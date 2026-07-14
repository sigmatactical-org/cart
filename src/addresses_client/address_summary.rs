//! [`AddressSummary`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

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
