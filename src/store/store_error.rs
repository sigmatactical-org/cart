//! [`StoreError`].

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("cart not found")]
    CartNotFound,
    #[error("line not found")]
    LineNotFound,
    #[error("sku_id is required")]
    SkuIdRequired,
    #[error("quantity must be at least 1")]
    InvalidQuantity,
    #[error("cart is not open")]
    CartNotOpen,
    #[error("database error: {0}")]
    Database(#[from] anyhow::Error),
    #[error("{0}")]
    InvalidInput(String),
}
impl From<sqlx::Error> for StoreError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err.into())
    }
}
