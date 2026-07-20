//! [`CartStatus`].

use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CartStatus {
    Open,
    Submitted,
    Cancelled,
}
impl CartStatus {
    /// Wire/storage spelling, also the form `<option>` value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Submitted => "submitted",
            Self::Cancelled => "cancelled",
        }
    }

    /// Human-readable label for admin pages.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Submitted => "Submitted",
            Self::Cancelled => "Cancelled",
        }
    }
}
impl FromStr for CartStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "submitted" => Ok(Self::Submitted),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err("status must be open, submitted, or cancelled".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_as_str() {
        for status in [
            CartStatus::Open,
            CartStatus::Submitted,
            CartStatus::Cancelled,
        ] {
            assert_eq!(status.as_str().parse::<CartStatus>().unwrap(), status);
        }
    }

    #[test]
    fn rejects_unknown_status() {
        assert!("nope".parse::<CartStatus>().is_err());
    }
}
