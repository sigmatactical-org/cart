//! [`CartRow`].

/// One rendered table row. Carries only the fields the index template shows,
/// so listing carts never clones a whole [`crate::model::Cart`].
pub struct CartRow {
    pub id: String,
    pub updated_at: String,
    pub user_display: String,
    pub status_label: &'static str,
    pub line_count: usize,
    pub missing_user: bool,
}
