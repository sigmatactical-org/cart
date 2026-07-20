//! [`UserRef`].

/// Lightweight reference for pickers/links.
pub struct UserRef {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
}
