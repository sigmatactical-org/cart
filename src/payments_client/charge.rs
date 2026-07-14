//! [`Charge`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Charge {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub failure_reason: Option<String>,
    pub amount_cents: u64,
}
