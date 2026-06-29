use std::path::{Path, PathBuf};

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
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Database {
    carts: Vec<Cart>,
}

#[derive(Debug, Clone)]
pub struct CartStore {
    path: PathBuf,
    db: Database,
}

impl CartStore {
    /// Load or initialize the cart database at `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let db = if path.exists() {
            let bytes = std::fs::read(&path)?;
            serde_json::from_slice(&bytes)?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Database::default()
        };
        Ok(Self { path, db })
    }

    fn save(&self) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.db)?;
        std::fs::write(&self.path, bytes)?;
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

    pub fn create(&mut self, input: CreateCart) -> Result<Cart, StoreError> {
        let cart = Cart::new(input);
        self.db.carts.push(cart.clone());
        self.save()?;
        Ok(cart)
    }

    pub fn update(&mut self, id: &str, input: UpdateCart) -> Result<Cart, StoreError> {
        let cart = self
            .db
            .carts
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or(StoreError::CartNotFound)?;
        cart.apply_update(input);
        let updated = cart.clone();
        self.save()?;
        Ok(updated)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), StoreError> {
        let index = self
            .db
            .carts
            .iter()
            .position(|c| c.id == id)
            .ok_or(StoreError::CartNotFound)?;
        self.db.carts.remove(index);
        self.save()
    }

    pub fn add_line(&mut self, cart_id: &str, input: CreateLine) -> Result<CartLine, StoreError> {
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
        self.save()?;
        Ok(line)
    }

    pub fn update_line(
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
        self.save()?;
        Ok(updated)
    }

    pub fn delete_line(&mut self, cart_id: &str, line_id: &str) -> Result<(), StoreError> {
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
        self.save()
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
    use tempfile::TempDir;

    fn test_store() -> (CartStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = CartStore::load(dir.path().join("carts.json")).unwrap();
        (store, dir)
    }

    #[test]
    fn create_cart_and_add_line() {
        let (mut store, _dir) = test_store();
        let cart = store
            .create(CreateCart {
                user_id: Some("user-1".to_string()),
                note: None,
            })
            .unwrap();
        let line = store
            .add_line(
                &cart.id,
                CreateLine {
                    sku_id: "sku-abc".to_string(),
                    quantity: 2,
                },
            )
            .unwrap();
        assert_eq!(line.quantity, 2);
        let updated = store.get(&cart.id).unwrap();
        assert_eq!(updated.lines.len(), 1);
    }

    #[test]
    fn reject_line_on_submitted_cart() {
        let (mut store, _dir) = test_store();
        let cart = store.create(CreateCart::default()).unwrap();
        store
            .update(
                &cart.id,
                UpdateCart {
                    user_id: None,
                    status: CartStatus::Submitted,
                    note: None,
                },
            )
            .unwrap();
        let err = store
            .add_line(
                &cart.id,
                CreateLine {
                    sku_id: "sku-abc".to_string(),
                    quantity: 1,
                },
            )
            .unwrap_err();
        assert!(matches!(err, StoreError::CartNotOpen));
    }
}
