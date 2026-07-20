//! Test-only helpers shared by the store and route tests.

use tokio::sync::{Mutex, MutexGuard};

/// Serialize tests that touch PostgreSQL.
///
/// Every fixture truncates the shared `cart` tables, so two tests running at
/// once would wipe each other's rows. Bind the returned guard for the whole
/// test body (`let _db = db_guard().await;`).
pub(crate) async fn db_guard() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::const_new(());
    LOCK.lock().await
}
