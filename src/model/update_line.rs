//! [`UpdateLine`].

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLine {
    pub quantity: u32,
}
