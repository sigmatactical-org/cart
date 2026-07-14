//! [`Cart`].

#[allow(unused_imports)]
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cart {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    pub status: CartStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<CartLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub updated_at: String,
}
impl Cart {
    /// New Cart from a create request.
    pub fn new(input: CreateCart) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: input.user_id.map(|s| s.trim().to_string()),
            status: CartStatus::Open,
            lines: Vec::new(),
            note: input.note,
            updated_at: now,
        }
    }

    /// Apply a partial update in place.
    pub fn apply_update(&mut self, input: UpdateCart) {
        self.user_id = input.user_id.map(|s| s.trim().to_string());
        self.status = input.status;
        self.note = input.note;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}
