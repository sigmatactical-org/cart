use sqlx::PgPool;
use thiserror::Error;

use crate::model::{
    Cart, CartLine, CartStatus, CreateCart, CreateLine, Reservation, UpdateCart, UpdateLine,
};

const SCHEMA: &str = "cart";

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
    #[error("user not found: {0}")]
    UserNotFound(String),
    #[error("database error: {0}")]
    Database(#[from] anyhow::Error),
    #[error("{0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Database {
    carts: Vec<Cart>,
    #[serde(default)]
    reservations: Vec<Reservation>,
}

#[derive(Debug, Clone)]
pub struct CartStore {
    pool: PgPool,
    db: Database,
}

impl CartStore {
    /// Connect to PostgreSQL and load the cart snapshot.
    pub async fn connect() -> Result<Self, StoreError> {
        let pool = sigma_pg::connect().await?;
        let db: Database = sigma_pg::load_snapshot(&pool, SCHEMA).await?;
        Ok(Self { pool, db })
    }

    /// Reset the cart snapshot (tests only).
    #[cfg(test)]
    pub async fn connect_empty() -> Result<Self, StoreError> {
        let pool = sigma_pg::connect().await?;
        let db = Database::default();
        sigma_pg::save_snapshot(&pool, SCHEMA, &db).await?;
        Ok(Self { pool, db })
    }

    async fn persist(&self) -> Result<(), StoreError> {
        sigma_pg::save_snapshot(&self.pool, SCHEMA, &self.db).await?;
        Ok(())
    }

    #[must_use]
    pub fn list(&self) -> Vec<Cart> {
        let mut carts = self.db.carts.clone();
        carts.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        carts
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<Cart> {
        self.db.carts.iter().find(|c| c.id == id).cloned()
    }

    pub async fn create(&mut self, input: CreateCart) -> Result<Cart, StoreError> {
        let cart = Cart::new(input);
        self.db.carts.push(cart.clone());
        self.persist().await?;
        Ok(cart)
    }

    pub async fn update(&mut self, id: &str, input: UpdateCart) -> Result<Cart, StoreError> {
        let cart = self
            .db
            .carts
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(StoreError::CartNotFound)?;
        cart.apply_update(input);
        let updated = cart.clone();
        self.persist().await?;
        Ok(updated)
    }

    pub async fn delete(&mut self, id: &str) -> Result<(), StoreError> {
        let index = self
            .db
            .carts
            .iter()
            .position(|c| c.id == id)
            .ok_or(StoreError::CartNotFound)?;
        self.db.carts.remove(index);
        self.persist().await
    }

    pub async fn add_line(
        &mut self,
        cart_id: &str,
        input: CreateLine,
    ) -> Result<CartLine, StoreError> {
        self.validate_line_input(&input)?;
        let cart = self
            .db
            .carts
            .iter_mut()
            .find(|c| c.id == cart_id)
            .ok_or(StoreError::CartNotFound)?;
        if cart.status != CartStatus::Open {
            return Err(StoreError::CartNotOpen);
        }
        let line = CartLine::new(input);
        cart.lines.push(line.clone());
        cart.updated_at = chrono::Utc::now().to_rfc3339();
        self.persist().await?;
        Ok(line)
    }

    pub async fn update_line(
        &mut self,
        cart_id: &str,
        line_id: &str,
        input: UpdateLine,
    ) -> Result<CartLine, StoreError> {
        if input.quantity == 0 {
            return Err(StoreError::InvalidQuantity);
        }
        let cart = self
            .db
            .carts
            .iter_mut()
            .find(|c| c.id == cart_id)
            .ok_or(StoreError::CartNotFound)?;
        if cart.status != CartStatus::Open {
            return Err(StoreError::CartNotOpen);
        }
        let line = cart
            .lines
            .iter_mut()
            .find(|l| l.id == line_id)
            .ok_or(StoreError::LineNotFound)?;
        line.apply_update(input.quantity);
        let updated = line.clone();
        cart.updated_at = chrono::Utc::now().to_rfc3339();
        self.persist().await?;
        Ok(updated)
    }

    pub async fn delete_line(&mut self, cart_id: &str, line_id: &str) -> Result<(), StoreError> {
        let cart = self
            .db
            .carts
            .iter_mut()
            .find(|c| c.id == cart_id)
            .ok_or(StoreError::CartNotFound)?;
        if cart.status != CartStatus::Open {
            return Err(StoreError::CartNotOpen);
        }
        let index = cart
            .lines
            .iter()
            .position(|l| l.id == line_id)
            .ok_or(StoreError::LineNotFound)?;
        cart.lines.remove(index);
        cart.updated_at = chrono::Utc::now().to_rfc3339();
        self.persist().await
    }

    /// Set a cart's status (used to mark a cart reserved/submitted at checkout).
    pub async fn set_status(
        &mut self,
        cart_id: &str,
        status: CartStatus,
    ) -> Result<(), StoreError> {
        let cart = self
            .db
            .carts
            .iter_mut()
            .find(|c| c.id == cart_id)
            .ok_or(StoreError::CartNotFound)?;
        cart.status = status;
        cart.updated_at = chrono::Utc::now().to_rfc3339();
        self.persist().await
    }

    /// Record a reservation created when a shopper pays a deposit.
    pub async fn create_reservation(
        &mut self,
        reservation: Reservation,
    ) -> Result<Reservation, StoreError> {
        self.db.reservations.push(reservation.clone());
        self.persist().await?;
        Ok(reservation)
    }

    #[must_use]
    pub fn get_reservation(&self, id: &str) -> Option<Reservation> {
        self.db.reservations.iter().find(|r| r.id == id).cloned()
    }

    fn validate_line_input(&self, input: &CreateLine) -> Result<(), StoreError> {
        if input.sku_id.trim().is_empty() {
            return Err(StoreError::SkuIdRequired);
        }
        if input.quantity == 0 {
            return Err(StoreError::InvalidQuantity);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_store() -> CartStore {
        CartStore::connect_empty()
            .await
            .expect("PostgreSQL required for tests")
    }

    #[tokio::test]
    async fn create_cart_and_add_line() {
        let mut store = test_store().await;
        let cart = store
            .create(CreateCart {
                user_id: Some("user-1".to_string()),
                note: None,
            })
            .await
            .unwrap();
        let line = store
            .add_line(
                &cart.id,
                CreateLine {
                    sku_id: "sku-abc".to_string(),
                    quantity: 2,
                },
            )
            .await
            .unwrap();
        assert_eq!(line.quantity, 2);
        let updated = store.get(&cart.id).unwrap();
        assert_eq!(updated.lines.len(), 1);
    }

    #[tokio::test]
    async fn reject_line_on_submitted_cart() {
        let mut store = test_store().await;
        let cart = store.create(CreateCart::default()).await.unwrap();
        store
            .update(
                &cart.id,
                UpdateCart {
                    user_id: None,
                    status: CartStatus::Submitted,
                    note: None,
                },
            )
            .await
            .unwrap();
        let err = store
            .add_line(
                &cart.id,
                CreateLine {
                    sku_id: "sku-abc".to_string(),
                    quantity: 1,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::CartNotOpen));
    }
}
