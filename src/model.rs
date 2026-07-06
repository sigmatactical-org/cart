use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CartStatus {
    Open,
    Submitted,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartLine {
    pub id: String,
    pub sku_id: String,
    pub quantity: u32,
    pub updated_at: String,
}

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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateCart {
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCart {
    #[serde(default)]
    pub user_id: Option<String>,
    pub status: CartStatus,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateLine {
    pub sku_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateLine {
    pub quantity: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CartForm {
    pub user_id: String,
    pub status: String,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LineForm {
    pub sku_id: String,
    pub quantity: String,
}

impl CartForm {
    pub fn into_create(self) -> Result<CreateCart, String> {
        Ok(CreateCart {
            user_id: empty_to_none(self.user_id),
            note: empty_to_none(self.note),
        })
    }

    pub fn into_update(self) -> Result<UpdateCart, String> {
        Ok(UpdateCart {
            user_id: empty_to_none(self.user_id),
            status: parse_status(&self.status)?,
            note: empty_to_none(self.note),
        })
    }
}

impl LineForm {
    pub fn into_create(self) -> Result<CreateLine, String> {
        let quantity = parse_quantity(&self.quantity)?;
        Ok(CreateLine {
            sku_id: self.sku_id,
            quantity,
        })
    }
}

fn empty_to_none(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_status(value: &str) -> Result<CartStatus, String> {
    match value.trim().to_lowercase().as_str() {
        "open" => Ok(CartStatus::Open),
        "submitted" => Ok(CartStatus::Submitted),
        "cancelled" => Ok(CartStatus::Cancelled),
        _ => Err("status must be open, submitted, or cancelled".to_string()),
    }
}

fn parse_quantity(value: &str) -> Result<u32, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("quantity is required".to_string());
    }
    let quantity: u32 = trimmed
        .parse()
        .map_err(|_| "quantity must be a positive integer".to_string())?;
    if quantity == 0 {
        return Err("quantity must be at least 1".to_string());
    }
    Ok(quantity)
}

impl Cart {
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

    pub fn apply_update(&mut self, input: UpdateCart) {
        self.user_id = input.user_id.map(|s| s.trim().to_string());
        self.status = input.status;
        self.note = input.note;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}

impl CartLine {
    pub fn new(input: CreateLine) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            sku_id: input.sku_id.trim().to_string(),
            quantity: input.quantity,
            updated_at: now,
        }
    }

    pub fn apply_update(&mut self, quantity: u32) {
        self.quantity = quantity;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}

#[must_use]
pub fn status_label(status: CartStatus) -> &'static str {
    match status {
        CartStatus::Open => "Open",
        CartStatus::Submitted => "Submitted",
        CartStatus::Cancelled => "Cancelled",
    }
}

/// Deposit required to reserve a build (50% of the list price).
#[must_use]
pub fn deposit_cents_for_price(price_cents: u64) -> u64 {
    price_cents / 2
}

/// Render a cents amount as a US dollar string, e.g. `175000` -> `$1,750.00`.
#[must_use]
pub fn format_price_cents(cents: u64) -> String {
    format!("${}.{:02}", group_thousands(cents / 100), cents % 100)
}

fn group_thousands(dollars: u64) -> String {
    let digits = dollars.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    grouped.chars().rev().collect()
}

/// A single line captured on a reservation at reserve time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReservationLine {
    pub sku_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub unit_price_cents: u64,
    pub line_total_cents: u64,
    pub deposit_cents: u64,
}

/// A reservation created when a shopper pays the deposit to reserve their build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reservation {
    pub id: String,
    pub cart_id: String,
    pub username: String,
    pub lines: Vec<ReservationLine>,
    pub subtotal_cents: u64,
    pub deposit_cents: u64,
    pub created_at: String,
}

/// A priced line to persist onto a reservation.
#[derive(Debug, Clone)]
pub struct CreateReservationLine {
    pub sku_id: String,
    pub sku_code: String,
    pub name: String,
    pub quantity: u32,
    pub unit_price_cents: u64,
}

impl Reservation {
    #[must_use]
    pub fn new(cart_id: &str, username: &str, lines: Vec<CreateReservationLine>) -> Self {
        let lines: Vec<ReservationLine> = lines
            .into_iter()
            .map(|line| {
                let line_total_cents = line.unit_price_cents * u64::from(line.quantity);
                ReservationLine {
                    sku_id: line.sku_id,
                    sku_code: line.sku_code,
                    name: line.name,
                    quantity: line.quantity,
                    unit_price_cents: line.unit_price_cents,
                    line_total_cents,
                    deposit_cents: deposit_cents_for_price(line_total_cents),
                }
            })
            .collect();
        let subtotal_cents = lines.iter().map(|l| l.line_total_cents).sum();
        let deposit_cents = deposit_cents_for_price(subtotal_cents);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            cart_id: cart_id.to_string(),
            username: username.trim().to_string(),
            lines,
            subtotal_cents,
            deposit_cents,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quantity_rejects_zero() {
        assert!(parse_quantity("0").is_err());
        assert_eq!(parse_quantity("3").unwrap(), 3);
    }
}
