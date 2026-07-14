//! [`PaymentMethodSummary`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

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
