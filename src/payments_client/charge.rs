//! [`Charge`].

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Charge {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub failure_reason: Option<String>,
}
