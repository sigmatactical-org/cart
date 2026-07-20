//! Client for the accounting service internal JSON API.
//!
//! After a successful checkout the cart records the deposit as an accounting
//! receipt, so accounting sees money coming in and not just vendor spend.
//! The cart is the only place that knows the charge id and the order id at
//! the same moment — the charge is created before the order exists, and its
//! `reference` is the cart id — so it is the only caller that can record a
//! fully linked receipt.
//!
//! Deliberately best-effort: a paid checkout must never fail because
//! accounting is unavailable. Anything missed here is backfilled by
//! accounting's reconcile against the payments charge log.

mod accounting_client_error;
mod create_receipt_body;
pub use accounting_client_error::AccountingClientError;
pub(crate) use create_receipt_body::CreateReceiptBody;

use sigma_pg::clients::http;

/// Record the checkout deposit as an accounting receipt.
///
/// Returns `false` without sending anything when
/// `CART_ACCOUNTING_INTERNAL_URL` is unset. Accounting keys receipts on
/// `charge_id`, so retrying a request that already landed is harmless.
pub async fn record_deposit_receipt(
    user_id: &str,
    charge_id: &str,
    order_id: &str,
    amount_cents: u64,
) -> Result<bool, AccountingClientError> {
    let Some(base) = crate::config::accounting_internal_base_url() else {
        return Ok(false);
    };
    let body = CreateReceiptBody {
        charge_id,
        order_id,
        user_id,
        kind: "deposit",
        amount_cents,
        currency: "usd",
    };
    let response =
        http::with_internal_auth(http::client().post(format!("{base}receipts")).json(&body))
            .send()
            .await?;
    http::ensure_success(response)
        .await
        .map_err(AccountingClientError::Request)?;
    Ok(true)
}
