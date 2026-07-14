mod cart;
mod cart_form;
mod cart_line;
mod cart_status;
mod create_cart;
mod create_line;
mod line_form;
mod update_cart;
mod update_line;
pub use cart::Cart;
pub use cart_form::CartForm;
pub use cart_line::CartLine;
pub use cart_status::CartStatus;
pub use create_cart::CreateCart;
pub use create_line::CreateLine;
pub use line_form::LineForm;
pub use update_cart::UpdateCart;
pub use update_line::UpdateLine;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quantity_rejects_zero() {
        assert!(parse_quantity("0").is_err());
        assert_eq!(parse_quantity("3").unwrap(), 3);
    }
}
