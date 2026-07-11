//! L0 — cash-on-hand ledger.
//!
//! ## Formula (§1.2 full form, forward-compatible)
//! ```text
//! cash_on_hand = opening_cash
//!              + Σcash(SELL)
//!              − Σcash(RETURN)
//!              + Σ(service-in)    ← 0 until L3 wired
//!              − Σ(service-out)   ← 0 until L3 wired
//!              − Σ(EPZ-out)       ← 0 until L3 wired
//! ```
//!
//! ## Carry semantics (default, operator decision 2026-07-10)
//! At SHIFT_OPEN: `opening_cash = prior shift's cash_balance_kop` (carry-over),
//! or 0 for the FN's first shift.
//! At SHIFT_CLOSE: `closing_cash = cash_on_hand` is persisted in
//! `shifts.cash_balance_kop` (overwrites the row so it holds the CLOSING value,
//! which becomes the NEXT shift's opening carry).
//!
//! ## Cash identification — D1 frozen-invariant (type_code == "0")
//! Cash = stored `type_code == "0"` in `fiscal_documents.payload_json` payments.
//! This is reliable because:
//!   - D1 frozen: CASH_SLOT = pay_index 1 → type_code "0" (convert.rs:701).
//!   - Admin-guard refuses to move the cash slot (admin_w4_z0.rs:108-164).
//!   - Receipt conversion validates `iscash == kind_is_cash` at convert-time
//!     (convert.rs:730), so a stored type_code "0" payment IS a cash payment by
//!     construction — the D1 invariant makes positional == semantic for cash.
//!   - Boot preflight verifies D1.
//!     Pure main-pool SQL — no secure_pool cross-dependency on the close/derive path.
//!
//! ## Fidelity: mirrors aggregate_zreport grouping
//! `aggregate_zreport` groups by `(type_code, name)`; this module extracts
//! the type_code="0" row, so cash-on-hand and the Z `<M>` never diverge.
//!
//! ## Pure core — unit-testable without a DB
//! `derive_cash_on_hand` is a pure fn; callers supply pre-aggregated sums.
//! Mirrors `tax_summary.rs` architecture.
//!
//! ## Invariant #1
//! No network/crypto in any derive fn; all DB reads are pool-bound outside
//! write-tx.

use crate::db::models::ids::ShiftId;
use crate::db::tx::WriteTxConn;
use sqlx::SqlitePool;

/// Derive cash-on-hand from the opening anchor and per-shift cash totals.
///
/// Pure — no I/O, no `async`.  All values in kopecks.
///
/// `cash_sell_kop`   = Σ sum_kop for type_code="0" payments on SELL receipts.
/// `cash_return_kop` = Σ sum_kop for type_code="0" payments on RETURN receipts.
///
/// TODO(L3): caller extends with service_in_kop, service_out_kop, epz_out_kop
/// and applies: balance += service_in − service_out − epz_out.
pub fn derive_cash_on_hand(opening_cash_kop: i64, cash_sell_kop: i64, cash_return_kop: i64) -> i64 {
    opening_cash_kop + cash_sell_kop - cash_return_kop
    // TODO(L3): + service_in_kop − service_out_kop − epz_out_kop
}

/// Serde-minimal view of a stored CheckJson payment.
#[derive(serde::Deserialize)]
struct StoredPayment {
    type_code: String,
    sum_kop: i64,
}

#[derive(serde::Deserialize)]
struct StoredCheckPayments {
    payments: Vec<StoredPayment>,
}

/// Aggregate SELL/RETURN issued receipts for one shift into
/// `(cash_sell_kop, cash_return_kop)` — type_code="0" legs only.
///
/// D1 frozen invariant: type_code "0" ↔ cash payment by construction.
/// Pure main-pool SELECT; no network/crypto (invariant #1).
pub async fn aggregate_shift_cash(
    pool: &SqlitePool,
    fiscal_number: &str,
    shift_id: ShiftId,
) -> sqlx::Result<(i64, i64)> {
    use crate::db::models::enums::DocType;

    let receipts = crate::db::repositories::fiscal_documents::list_shift_issued_receipts(
        pool,
        fiscal_number,
        shift_id,
    )
    .await?;

    let mut cash_sell: i64 = 0;
    let mut cash_return: i64 = 0;

    for (doc_type, payload_json, _snap_id) in &receipts {
        let Ok(parsed) = serde_json::from_str::<StoredCheckPayments>(payload_json) else {
            continue; // malformed payload: skip; invariant_scan will catch it
        };
        for p in parsed.payments {
            // type_code "0" = cash slot (D1 frozen; CASH_SLOT=1 → type_code=(pay_index-1)="0")
            if p.type_code == "0" && p.sum_kop > 0 {
                match doc_type {
                    DocType::Sell => cash_sell = cash_sell.saturating_add(p.sum_kop),
                    DocType::Return => cash_return = cash_return.saturating_add(p.sum_kop),
                    _ => {}
                }
            }
        }
    }

    Ok((cash_sell, cash_return))
}

/// Tx-based variant of [`aggregate_shift_cash`] — runs INSIDE a `with_immediate`
/// envelope, reading from the same serialized snapshot.
///
/// Used by the in-lease INV-21 re-check in `stage_acquire` (Step 6b‴‴).  All
/// other callers (Z aggregation, reconcile) use the pool-based variant.
///
/// **Invariant #1**: this is a pure `SELECT` — no network/crypto — so running
/// it inside a BEGIN IMMEDIATE transaction does NOT violate INV-1.  The
/// serialize point is the FN write-lease (held by `with_immediate` at this
/// point), which is EXACTLY what closes the TOCTOU between two concurrent
/// cash RETURNs for the same FN.
pub async fn aggregate_shift_cash_tx(
    tx: &mut WriteTxConn<'_>,
    fiscal_number: &str,
    shift_id: ShiftId,
) -> sqlx::Result<(i64, i64)> {
    use crate::db::models::enums::DocType;

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT doc_type, payload_json FROM fiscal_documents          WHERE fiscal_number = ? AND shift_id = ?            AND doc_type IN ('SELL','RETURN')            AND state IN ('ACK','OFFLINE_LOCAL_ACK')          ORDER BY lnd",
    )
    .bind(fiscal_number)
    .bind(shift_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut cash_sell: i64 = 0;
    let mut cash_return: i64 = 0;

    for (doc_type_str, payload_json) in &rows {
        let Ok(parsed) = serde_json::from_str::<StoredCheckPayments>(payload_json) else {
            continue;
        };
        let doc_type: DocType = match doc_type_str.as_str() {
            "SELL" => DocType::Sell,
            "RETURN" => DocType::Return,
            _ => continue,
        };
        for p in parsed.payments {
            if p.type_code == "0" && p.sum_kop > 0 {
                match doc_type {
                    DocType::Sell => cash_sell = cash_sell.saturating_add(p.sum_kop),
                    DocType::Return => cash_return = cash_return.saturating_add(p.sum_kop),
                    _ => {}
                }
            }
        }
    }

    Ok((cash_sell, cash_return))
}


/// Derive the closing cash-on-hand for a shift from its DB receipts.
/// `opening_kop` = the shift's stored `cash_balance_kop` (opening anchor).
pub async fn derive_closing_cash(
    pool: &SqlitePool,
    fiscal_number: &str,
    shift_id: ShiftId,
    opening_kop: i64,
) -> sqlx::Result<i64> {
    let (sell, ret) = aggregate_shift_cash(pool, fiscal_number, shift_id).await?;
    Ok(derive_cash_on_hand(opening_kop, sell, ret))
}

/// Look up the opening carry for a NEW shift being opened on `fiscal_number`:
/// the most-recently-closed shift's `cash_balance_kop` (its closing balance).
/// Returns 0 if no closed shift exists (first shift ever for this FN).
///
/// Pure main-pool read, called BEFORE the write-tx (invariant #1).
pub async fn opening_carry_for_fn(pool: &SqlitePool, fiscal_number: &str) -> sqlx::Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT cash_balance_kop FROM shifts \
             WHERE fiscal_number = ? AND state = 'CLOSED' \
             ORDER BY serial DESC LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await?
    .unwrap_or(0))
}

/// Compute current cash-on-hand for `fiscal_number`'s open shift.
///
/// Used by the L1 guard in `convert_to_signer_payload` (pre-inbox, row-less).
/// Returns 0 if there is no open shift (no shift → RETURN is already
/// rejected upstream by the shift-guard; 0 is safe here because the
/// `return_cash_kop > 0` gate will not trigger on a 0-amount RETURN).
///
/// Pure main-pool reads, outside write-tx (invariant #1).
pub async fn cash_on_hand_for_fn(pool: &SqlitePool, fiscal_number: &str) -> sqlx::Result<i64> {
    // Find the current open shift and its opening anchor via node_state join.
    let row: Option<(Vec<u8>, i64)> = sqlx::query_as(
        "SELECT s.shift_id, s.cash_balance_kop \
         FROM shifts s \
         JOIN node_state ns ON ns.current_shift_id = s.shift_id \
         WHERE ns.fiscal_number = ? \
           AND s.state NOT IN ('CLOSED','REQUIRES_MANUAL_RECONCILIATION','ERROR','CREATED') \
         LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await?;

    let Some((shift_id_bytes, opening_kop)) = row else {
        return Ok(0); // no open shift; upstream guard owns that refusal
    };

    let arr: [u8; 16] = shift_id_bytes
        .try_into()
        .map_err(|_| sqlx::Error::Decode("shift_id not 16 bytes".into()))?;
    let shift_id = ShiftId::from_bytes(arr);

    derive_closing_cash(pool, fiscal_number, shift_id, opening_kop).await
}

/// Re-derive the opening anchor of the CURRENT open shift from the journal.
///
/// Algorithm:
///  1. Walk closed shifts for `fiscal_number` in serial order.
///  2. Re-derive each shift's closing cash from its docs (using the stored
///     `cash_balance_kop` as that shift's opening anchor).
///  3. The carry after all closed shifts = what the open shift's opening
///     SHOULD be.
///  4. Return `Some((re_derived, stored))` so the caller can detect drift.
///
/// Returns `None` if there is no current open shift.
///
/// Called by `invariant_scan` (existing ledger-reconcile seam; NOT a new
/// startup pass — opt #4).  Pool reads outside write-tx (invariant #1).
pub async fn reconcile_opening_anchor(
    pool: &SqlitePool,
    fiscal_number: &str,
) -> sqlx::Result<Option<(i64, i64)>> {
    // Find the current open shift and its stored opening.
    let open_shift: Option<(Vec<u8>, i64)> = sqlx::query_as(
        "SELECT shift_id, cash_balance_kop \
         FROM shifts \
         WHERE fiscal_number = ? \
           AND state NOT IN ('CLOSED','REQUIRES_MANUAL_RECONCILIATION','ERROR','CREATED') \
         ORDER BY serial DESC LIMIT 1",
    )
    .bind(fiscal_number)
    .fetch_optional(pool)
    .await?;

    let Some((_, stored_opening)) = open_shift else {
        return Ok(None);
    };

    // Walk closed shifts in serial order to accumulate carry.
    //
    // Schema note: for CLOSED shifts, `cash_balance_kop` holds the CLOSING
    // balance (updated atomically by stage_send at the Closing→Closed CAS,
    // L0 §1.3).  For OPEN shifts it holds the OPENING carry (the value set
    // at shift-create time from the prior closed shift's closing balance).
    //
    // Therefore we must NOT use `cash_balance_kop` of a closed shift as its
    // opening anchor — that would double-count (opening=15000 + SELL=15000 →
    // re_derived=30000 when the true opening was 0).
    //
    // Algorithm: use the accumulated `carry` from the PREVIOUS iteration as
    // the opening anchor for the CURRENT shift.  For the very first shift
    // the carry starts at 0 (no prior shift).  `cash_balance_kop` on closed
    // shifts is ignored during the walk — it equals the re_derived closing
    // cash which we compute here independently, and comparing them would be
    // a per-shift check (not done here; the key invariant is that the OPEN
    // shift's opening matches what the chain re-derives as its carry).
    let closed: Vec<(Vec<u8>,)> = sqlx::query_as(
        "SELECT shift_id FROM shifts \
         WHERE fiscal_number = ? AND state = 'CLOSED' \
         ORDER BY serial ASC",
    )
    .bind(fiscal_number)
    .fetch_all(pool)
    .await?;

    let mut carry: i64 = 0;
    for (shift_id_bytes,) in closed {
        let arr: [u8; 16] = shift_id_bytes
            .try_into()
            .map_err(|_| sqlx::Error::Decode("shift_id not 16 bytes".into()))?;
        let shift_id = ShiftId::from_bytes(arr);
        let (sell, ret) = aggregate_shift_cash(pool, fiscal_number, shift_id).await?;
        // carry-in = the prior carry (opening of this shift); carry-out = closing.
        carry = derive_cash_on_hand(carry, sell, ret);
    }

    Ok(Some((carry, stored_opening)))
}
