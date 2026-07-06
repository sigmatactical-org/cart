use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use thiserror::Error;

use crate::model::{Cart, CartLine, CartStatus, CreateCart, CreateLine, UpdateCart, UpdateLine};

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

impl From<sqlx::Error> for StoreError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err.into())
    }
}

#[derive(Debug, Clone)]
pub struct CartStore {
    pool: PgPool,
}

impl CartStore {
    pub async fn connect() -> Result<Self, StoreError> {
        let pool = sigma_pg::connect_as("cart").await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub async fn connect_empty() -> Result<Self, StoreError> {
        let store = Self::connect().await?;
        sqlx::query("TRUNCATE cart.cart_lines, cart.carts")
            .execute(&store.pool)
            .await?;
        Ok(store)
    }

    pub async fn list(&self) -> Result<Vec<Cart>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, user_id, status, note, updated_at FROM cart.carts ORDER BY updated_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        self.rows_to_carts(rows).await
    }

    pub async fn get(&self, id: &str) -> Result<Option<Cart>, StoreError> {
        let row = sqlx::query(
            "SELECT id, user_id, status, note, updated_at FROM cart.carts WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => {
                let carts = self.rows_to_carts(vec![row]).await?;
                Ok(carts.into_iter().next())
            }
            None => Ok(None),
        }
    }

    pub async fn create(&self, input: CreateCart) -> Result<Cart, StoreError> {
        let cart = Cart::new(input);
        sqlx::query(
            "INSERT INTO cart.carts (id, user_id, status, note, updated_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&cart.id)
        .bind(&cart.user_id)
        .bind(status_str(cart.status))
        .bind(&cart.note)
        .bind(parse_ts(&cart.updated_at)?)
        .execute(&self.pool)
        .await?;
        Ok(cart)
    }

    pub async fn update(&self, id: &str, input: UpdateCart) -> Result<Cart, StoreError> {
        let mut cart = self.get(id).await?.ok_or(StoreError::CartNotFound)?;
        cart.apply_update(input);
        sqlx::query(
            "UPDATE cart.carts SET user_id = $2, status = $3, note = $4, updated_at = $5 \
             WHERE id = $1",
        )
        .bind(&cart.id)
        .bind(&cart.user_id)
        .bind(status_str(cart.status))
        .bind(&cart.note)
        .bind(parse_ts(&cart.updated_at)?)
        .execute(&self.pool)
        .await?;
        Ok(cart)
    }

    pub async fn delete(&self, id: &str) -> Result<(), StoreError> {
        let result = sqlx::query("DELETE FROM cart.carts WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::CartNotFound);
        }
        Ok(())
    }

    pub async fn add_line(&self, cart_id: &str, input: CreateLine) -> Result<CartLine, StoreError> {
        self.validate_line_input(&input)?;
        let cart = self.get(cart_id).await?.ok_or(StoreError::CartNotFound)?;
        if cart.status != CartStatus::Open {
            return Err(StoreError::CartNotOpen);
        }
        let line = CartLine::new(input);
        let now = parse_ts(&line.updated_at)?;
        sqlx::query(
            "INSERT INTO cart.cart_lines (id, cart_id, sku_id, quantity, updated_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&line.id)
        .bind(cart_id)
        .bind(&line.sku_id)
        .bind(line.quantity as i32)
        .bind(now)
        .execute(&self.pool)
        .await?;
        touch_cart(cart_id, &self.pool).await?;
        Ok(line)
    }

    pub async fn update_line(
        &self,
        cart_id: &str,
        line_id: &str,
        input: UpdateLine,
    ) -> Result<CartLine, StoreError> {
        if input.quantity == 0 {
            return Err(StoreError::InvalidQuantity);
        }
        let cart = self.get(cart_id).await?.ok_or(StoreError::CartNotFound)?;
        if cart.status != CartStatus::Open {
            return Err(StoreError::CartNotOpen);
        }
        let mut line = cart
            .lines
            .into_iter()
            .find(|l| l.id == line_id)
            .ok_or(StoreError::LineNotFound)?;
        line.apply_update(input.quantity);
        let now = parse_ts(&line.updated_at)?;
        let result = sqlx::query(
            "UPDATE cart.cart_lines SET quantity = $3, updated_at = $4 \
             WHERE id = $1 AND cart_id = $2",
        )
        .bind(line_id)
        .bind(cart_id)
        .bind(line.quantity as i32)
        .bind(now)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::LineNotFound);
        }
        touch_cart(cart_id, &self.pool).await?;
        Ok(line)
    }

    pub async fn delete_line(&self, cart_id: &str, line_id: &str) -> Result<(), StoreError> {
        let cart = self.get(cart_id).await?.ok_or(StoreError::CartNotFound)?;
        if cart.status != CartStatus::Open {
            return Err(StoreError::CartNotOpen);
        }
        let result = sqlx::query("DELETE FROM cart.cart_lines WHERE id = $1 AND cart_id = $2")
            .bind(line_id)
            .bind(cart_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::LineNotFound);
        }
        touch_cart(cart_id, &self.pool).await?;
        Ok(())
    }

    pub async fn set_status(&self, cart_id: &str, status: CartStatus) -> Result<(), StoreError> {
        let now = Utc::now();
        let result =
            sqlx::query("UPDATE cart.carts SET status = $2, updated_at = $3 WHERE id = $1")
                .bind(cart_id)
                .bind(status_str(status))
                .bind(now)
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::CartNotFound);
        }
        Ok(())
    }

    async fn rows_to_carts(
        &self,
        rows: Vec<sqlx::postgres::PgRow>,
    ) -> Result<Vec<Cart>, StoreError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = rows.iter().map(|r| r.get("id")).collect();
        let line_rows = sqlx::query(
            "SELECT id, cart_id, sku_id, quantity, updated_at FROM cart.cart_lines \
             WHERE cart_id = ANY($1) ORDER BY cart_id, id",
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await?;
        let mut lines: HashMap<String, Vec<CartLine>> = HashMap::new();
        for row in line_rows {
            let cart_id: String = row.get("cart_id");
            lines.entry(cart_id).or_default().push(row_to_line(row)?);
        }
        rows.into_iter()
            .map(|row| {
                let id: String = row.get("id");
                row_to_cart(row, lines.remove(&id).unwrap_or_default())
            })
            .collect()
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

async fn touch_cart(cart_id: &str, pool: &PgPool) -> Result<(), StoreError> {
    sqlx::query("UPDATE cart.carts SET updated_at = now() WHERE id = $1")
        .bind(cart_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn row_to_cart(row: sqlx::postgres::PgRow, lines: Vec<CartLine>) -> Result<Cart, StoreError> {
    let status_str: String = row.get("status");
    Ok(Cart {
        id: row.get("id"),
        user_id: row.get("user_id"),
        status: parse_status(&status_str),
        lines,
        note: row.get("note"),
        updated_at: row.get::<DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })
}

fn row_to_line(row: sqlx::postgres::PgRow) -> Result<CartLine, StoreError> {
    Ok(CartLine {
        id: row.get("id"),
        sku_id: row.get("sku_id"),
        quantity: row.get::<i32, _>("quantity") as u32,
        updated_at: row.get::<DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })
}

fn status_str(status: CartStatus) -> &'static str {
    match status {
        CartStatus::Open => "open",
        CartStatus::Submitted => "submitted",
        CartStatus::Cancelled => "cancelled",
    }
}

fn parse_status(value: &str) -> CartStatus {
    match value {
        "submitted" => CartStatus::Submitted,
        "cancelled" => CartStatus::Cancelled,
        _ => CartStatus::Open,
    }
}

fn parse_ts(value: &str) -> Result<DateTime<Utc>, StoreError> {
    value
        .parse::<DateTime<Utc>>()
        .map_err(|e| StoreError::InvalidInput(format!("invalid timestamp: {e}")))
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
        let updated = store.get(&cart.id).await.unwrap().unwrap();
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
