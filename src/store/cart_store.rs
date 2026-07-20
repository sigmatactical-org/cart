//! [`CartStore`].

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use super::StoreError;
use crate::model::{Cart, CartLine, CartStatus, CreateCart, CreateLine, UpdateCart, UpdateLine};

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
        sigma_pg::assert_disposable_test_db(&store.pool).await;
        sqlx::query("TRUNCATE cart.cart_lines, cart.carts")
            .execute(&store.pool)
            .await?;
        Ok(store)
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
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
            Some(row) => Ok(self.rows_to_carts(vec![row]).await?.into_iter().next()),
            None => Ok(None),
        }
    }

    pub async fn create(&self, input: CreateCart) -> Result<Cart, StoreError> {
        let now = Utc::now();
        let mut cart = Cart::new(input);
        cart.updated_at = now.to_rfc3339();
        sqlx::query(
            "INSERT INTO cart.carts (id, user_id, status, note, updated_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&cart.id)
        .bind(&cart.user_id)
        .bind(cart.status.as_str())
        .bind(&cart.note)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(cart)
    }

    /// Apply an update in one guarded statement, then load the cart's lines.
    pub async fn update(&self, id: &str, input: UpdateCart) -> Result<Cart, StoreError> {
        let row = sqlx::query(
            "UPDATE cart.carts SET user_id = $2, status = $3, note = $4, updated_at = $5 \
             WHERE id = $1 RETURNING id, user_id, status, note, updated_at",
        )
        .bind(id)
        .bind(input.user_id.map(|s| s.trim().to_string()))
        .bind(input.status.as_str())
        .bind(&input.note)
        .bind(Utc::now())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(StoreError::CartNotFound)?;
        let mut lines = self.lines_by_cart(&[id.to_string()]).await?;
        row_to_cart(row, lines.remove(id).unwrap_or_default())
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

    /// Upsert a line on an **open** cart and touch the cart, in one statement.
    /// A same-SKU line has its quantity incremented rather than duplicated.
    pub async fn add_line(&self, cart_id: &str, input: CreateLine) -> Result<CartLine, StoreError> {
        validate_line_input(&input)?;
        let now = Utc::now();
        let mut line = CartLine::new(input);
        line.updated_at = now.to_rfc3339();
        let row = sqlx::query(
            "WITH upserted AS ( \
               INSERT INTO cart.cart_lines (id, cart_id, sku_id, quantity, updated_at) \
               SELECT $1, c.id, $3, $4, $5 FROM cart.carts c \
                 WHERE c.id = $2 AND c.status = 'open' \
               ON CONFLICT (cart_id, sku_id) DO UPDATE SET \
                 quantity = cart.cart_lines.quantity + EXCLUDED.quantity, \
                 updated_at = EXCLUDED.updated_at \
               RETURNING id, cart_id, sku_id, quantity, updated_at \
             ), touched AS ( \
               UPDATE cart.carts SET updated_at = $5 \
                 WHERE id = $2 AND EXISTS (SELECT 1 FROM upserted) \
             ) \
             SELECT id, cart_id, sku_id, quantity, updated_at FROM upserted",
        )
        .bind(&line.id)
        .bind(cart_id)
        .bind(&line.sku_id)
        .bind(line.quantity as i32)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_line(row),
            None => Err(self.unwritable_cart_error(cart_id).await?),
        }
    }

    /// Set a line's quantity on an **open** cart and touch the cart, in one
    /// statement.
    pub async fn update_line(
        &self,
        cart_id: &str,
        line_id: &str,
        input: UpdateLine,
    ) -> Result<CartLine, StoreError> {
        if input.quantity == 0 {
            return Err(StoreError::InvalidQuantity);
        }
        let now = Utc::now();
        let row = sqlx::query(
            "WITH updated AS ( \
               UPDATE cart.cart_lines l SET quantity = $3, updated_at = $4 \
                 FROM cart.carts c \
                 WHERE l.id = $2 AND l.cart_id = $1 AND c.id = l.cart_id \
                   AND c.status = 'open' \
               RETURNING l.id, l.cart_id, l.sku_id, l.quantity, l.updated_at \
             ), touched AS ( \
               UPDATE cart.carts SET updated_at = $4 \
                 WHERE id = $1 AND EXISTS (SELECT 1 FROM updated) \
             ) \
             SELECT id, cart_id, sku_id, quantity, updated_at FROM updated",
        )
        .bind(cart_id)
        .bind(line_id)
        .bind(input.quantity as i32)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => row_to_line(row),
            None => Err(self.missing_line_error(cart_id).await?),
        }
    }

    /// Remove a line from an **open** cart and touch the cart, in one statement.
    pub async fn delete_line(&self, cart_id: &str, line_id: &str) -> Result<(), StoreError> {
        let row = sqlx::query(
            "WITH deleted AS ( \
               DELETE FROM cart.cart_lines l USING cart.carts c \
                 WHERE l.id = $2 AND l.cart_id = $1 AND c.id = l.cart_id \
                   AND c.status = 'open' \
               RETURNING l.id \
             ), touched AS ( \
               UPDATE cart.carts SET updated_at = $3 \
                 WHERE id = $1 AND EXISTS (SELECT 1 FROM deleted) \
             ) \
             SELECT id FROM deleted",
        )
        .bind(cart_id)
        .bind(line_id)
        .bind(Utc::now())
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(_) => Ok(()),
            None => Err(self.missing_line_error(cart_id).await?),
        }
    }

    pub async fn set_status(&self, cart_id: &str, status: CartStatus) -> Result<(), StoreError> {
        let result =
            sqlx::query("UPDATE cart.carts SET status = $2, updated_at = $3 WHERE id = $1")
                .bind(cart_id)
                .bind(status.as_str())
                .bind(Utc::now())
                .execute(&self.pool)
                .await?;
        if result.rows_affected() == 0 {
            return Err(StoreError::CartNotFound);
        }
        Ok(())
    }

    /// Why a guarded line write matched nothing: the cart is missing, or it is
    /// no longer open. Only runs on the error path.
    async fn unwritable_cart_error(&self, cart_id: &str) -> Result<StoreError, StoreError> {
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM cart.carts WHERE id = $1")
                .bind(cart_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(match status {
            Some(_) => StoreError::CartNotOpen,
            None => StoreError::CartNotFound,
        })
    }

    /// Same as [`Self::unwritable_cart_error`], but an open cart means the line
    /// itself was missing.
    async fn missing_line_error(&self, cart_id: &str) -> Result<StoreError, StoreError> {
        Ok(match self.unwritable_cart_error(cart_id).await? {
            StoreError::CartNotOpen => StoreError::LineNotFound,
            other => other,
        })
    }

    async fn lines_by_cart(
        &self,
        ids: &[String],
    ) -> Result<HashMap<String, Vec<CartLine>>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, cart_id, sku_id, quantity, updated_at FROM cart.cart_lines \
             WHERE cart_id = ANY($1) ORDER BY cart_id, id",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        let mut lines: HashMap<String, Vec<CartLine>> = HashMap::new();
        for row in rows {
            let cart_id: String = row.get("cart_id");
            lines.entry(cart_id).or_default().push(row_to_line(row)?);
        }
        Ok(lines)
    }

    async fn rows_to_carts(&self, rows: Vec<PgRow>) -> Result<Vec<Cart>, StoreError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = rows.iter().map(|r| r.get("id")).collect();
        let mut lines = self.lines_by_cart(&ids).await?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.get("id");
                row_to_cart(row, lines.remove(&id).unwrap_or_default())
            })
            .collect()
    }
}

fn validate_line_input(input: &CreateLine) -> Result<(), StoreError> {
    if input.sku_id.trim().is_empty() {
        return Err(StoreError::SkuIdRequired);
    }
    if input.quantity == 0 {
        return Err(StoreError::InvalidQuantity);
    }
    Ok(())
}

fn row_to_cart(row: PgRow, lines: Vec<CartLine>) -> Result<Cart, StoreError> {
    let status: String = row.get("status");
    Ok(Cart {
        id: row.get("id"),
        user_id: row.get("user_id"),
        status: status.parse().unwrap_or(CartStatus::Open),
        lines,
        note: row.get("note"),
        updated_at: row.get::<DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })
}

fn row_to_line(row: PgRow) -> Result<CartLine, StoreError> {
    Ok(CartLine {
        id: row.get("id"),
        sku_id: row.get("sku_id"),
        quantity: row.get::<i32, _>("quantity") as u32,
        updated_at: row.get::<DateTime<Utc>, _>("updated_at").to_rfc3339(),
    })
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
        let _db = crate::test_support::db_guard().await;
        let store = test_store().await;
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
    async fn add_line_merges_same_sku() {
        let _db = crate::test_support::db_guard().await;
        let store = test_store().await;
        let cart = store.create(CreateCart::default()).await.unwrap();
        for _ in 0..2 {
            store
                .add_line(
                    &cart.id,
                    CreateLine {
                        sku_id: "sku-abc".to_string(),
                        quantity: 1,
                    },
                )
                .await
                .unwrap();
        }
        let updated = store.get(&cart.id).await.unwrap().unwrap();
        assert_eq!(updated.lines.len(), 1);
        assert_eq!(updated.lines[0].quantity, 2);
    }

    #[tokio::test]
    async fn reject_line_on_submitted_cart() {
        let _db = crate::test_support::db_guard().await;
        let store = test_store().await;
        let cart = store.create(CreateCart::default()).await.unwrap();
        store
            .set_status(&cart.id, CartStatus::Submitted)
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

    #[tokio::test]
    async fn add_line_on_missing_cart_reports_cart_not_found() {
        let _db = crate::test_support::db_guard().await;
        let store = test_store().await;
        let err = store
            .add_line(
                "nope",
                CreateLine {
                    sku_id: "sku-abc".to_string(),
                    quantity: 1,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::CartNotFound));
    }

    #[tokio::test]
    async fn update_and_delete_line() {
        let _db = crate::test_support::db_guard().await;
        let store = test_store().await;
        let cart = store.create(CreateCart::default()).await.unwrap();
        let line = store
            .add_line(
                &cart.id,
                CreateLine {
                    sku_id: "sku-abc".to_string(),
                    quantity: 1,
                },
            )
            .await
            .unwrap();
        let updated = store
            .update_line(&cart.id, &line.id, UpdateLine { quantity: 5 })
            .await
            .unwrap();
        assert_eq!(updated.quantity, 5);

        store.delete_line(&cart.id, &line.id).await.unwrap();
        let err = store.delete_line(&cart.id, &line.id).await.unwrap_err();
        assert!(matches!(err, StoreError::LineNotFound));
    }
}
