//! [`CartStatus`].

#[allow(unused_imports)]
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CartStatus {
    Open,
    Submitted,
    Cancelled,
}
