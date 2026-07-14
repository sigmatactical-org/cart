//! [`CartForm`].

#[allow(unused_imports)]
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CartForm {
    pub user_id: String,
    pub status: String,
    pub note: String,
}
impl CartForm {
    /// Validate the form into a create request.
    pub fn into_create(self) -> Result<CreateCart, String> {
        Ok(CreateCart {
            user_id: empty_to_none(self.user_id),
            note: empty_to_none(self.note),
        })
    }

    /// Validate the form into an update request.
    pub fn into_update(self) -> Result<UpdateCart, String> {
        Ok(UpdateCart {
            user_id: empty_to_none(self.user_id),
            status: parse_status(&self.status)?,
            note: empty_to_none(self.note),
        })
    }
}
