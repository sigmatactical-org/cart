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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quantity_rejects_zero() {
        assert!(parse_quantity("0").is_err());
        assert_eq!(parse_quantity("3").unwrap(), 3);
    }
}
