//! L0 — cash-on-hand ledger.
//!
//! ## Formula (§1.2 full form, L3 wired)
//! ```text
//! cash_on_hand = opening_cash
//!              + Σcash(SELL)
//!              − Σcash(RETURN)
//!              + Σ(service-in)    ← L3 wired
//!              − Σ(service-out)   ← L3 wired
//!              − Σ(EPZ-out)       ← 0 until L4/EPZ wired (stays fail-closed)
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
//! ## Service-in/out (L3): `amount_kop` field in stored service doc payloads.
//! SERVICE_IN docs carry `{amount_kop, name, schema_version}` as their payload.
//! SERVICE_OUT docs carry the same shape. The cash formula adds service_in and
//! subtracts service_out (symmetric to how SELL adds and RETURN subtracts cash).
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
/// Formula (§1.2 full, EPZ wired):
///   cash_in_drawer = opening + cash_sell − cash_return + service_in
///                    − service_out − epz_out
///
/// `cash_sell_kop`    = Σ sum_kop for type_code="0" payments on SELL receipts.
/// `cash_return_kop`  = Σ sum_kop for type_code="0" payments on RETURN receipts.
/// `service_in_kop`   = Σ amount_kop for SERVICE_IN docs (L3, issued ACK).
/// `service_out_kop`  = Σ amount_kop for SERVICE_OUT docs (L3, issued ACK).
/// `epz_out_kop`      = Σ sum for EPZ (cash-advance) docs (issued ACK / OLA).
///
/// EPZ (видача готівки за ЕПЗ) drives cash OUT of the drawer against a card
/// charge — WebCheck `Nal()` (`All.cs:431-462`) carries the `− num5` EPZ term.
pub fn derive_cash_on_hand(
    opening_cash_kop: i64,
    cash_sell_kop: i64,
    cash_return_kop: i64,
    service_in_kop: i64,
    service_out_kop: i64,
    epz_out_kop: i64,
) -> i64 {
    opening_cash_kop + cash_sell_kop - cash_return_kop + service_in_kop
        - service_out_kop
        - epz_out_kop
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

/// Serde-minimal view of a stored ServiceIo payload — `{amount_kop}`.
/// Does NOT use `deny_unknown_fields` so it remains forward-compatible with
/// the `name` and `schema_version` fields the signer also stores.
#[derive(serde::Deserialize)]
struct StoredServiceIoPayload {
    amount_kop: i64,
}

/// Serde-minimal view of a stored EPZ payload — `{sum_kop}`.
/// Forward-compatible (no `deny_unknown_fields`) with the card requisites the
/// signer also stores (paymentid / pa..rrn / name).
#[derive(serde::Deserialize)]
struct StoredEpzPayload {
    sum_kop: i64,
}

/// Aggregate SELL/RETURN issued receipts + SERVICE_IN/OUT + EPZ docs for one
/// shift into `(cash_sell_kop, cash_return_kop, service_in_kop, service_out_kop,
/// epz_out_kop)`.
///
/// SELL/RETURN: type_code="0" legs only (D1 frozen invariant).
/// SERVICE_IN/OUT: `amount_kop` from the stored service-io payload (L3).
/// EPZ: `sum_kop` from the stored EPZ payload (drives cash out).
///
/// Pure main-pool SELECT; no network/crypto (invariant #1).
pub async fn aggregate_shift_cash(
    pool: &SqlitePool,
    fiscal_number: &str,
    shift_id: ShiftId,
) -> sqlx::Result<(i64, i64, i64, i64, i64)> {
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

    // SERVICE_IN / SERVICE_OUT: read amount_kop from service docs.
    let (service_in, service_out) =
        aggregate_shift_service_io(pool, fiscal_number, shift_id).await?;

    // EPZ: read sum_kop from EPZ docs (drives cash OUT of the drawer).
    let epz_out = aggregate_shift_epz(pool, fiscal_number, shift_id).await?;

    Ok((cash_sell, cash_return, service_in, service_out, epz_out))
}

/// Aggregate EPZ (видача готівки за ЕПЗ) docs for a shift into `epz_out_kop`.
///
/// Reads `fiscal_documents` with `doc_type = 'CASH_ADVANCE_EPZ'` and
/// `state IN ('ACK','OFFLINE_LOCAL_ACK')` (same issued-set as SELL/RETURN /
/// service-io).  `sum_kop` from the stored EPZ payload.  Mirrors
/// [`aggregate_shift_service_io`].
///
/// Pure main-pool SELECT; no network/crypto (invariant #1).
pub async fn aggregate_shift_epz(
    pool: &SqlitePool,
    fiscal_number: &str,
    shift_id: ShiftId,
) -> sqlx::Result<i64> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT payload_json FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? \
           AND doc_type = 'CASH_ADVANCE_EPZ' \
           AND state IN ('ACK','OFFLINE_LOCAL_ACK') \
         ORDER BY lnd",
    )
    .bind(fiscal_number)
    .bind(shift_id)
    .fetch_all(pool)
    .await?;

    let mut epz_out: i64 = 0;
    for (payload_json,) in &rows {
        let Ok(parsed) = serde_json::from_str::<StoredEpzPayload>(payload_json) else {
            continue; // malformed: skip
        };
        if parsed.sum_kop <= 0 {
            continue;
        }
        epz_out = epz_out.saturating_add(parsed.sum_kop);
    }
    Ok(epz_out)
}

/// Aggregate SERVICE_IN and SERVICE_OUT docs for a shift.
///
/// Returns `(service_in_kop, service_out_kop)`.
/// Reads from `fiscal_documents` with `doc_type IN ('SERVICE_IN','SERVICE_OUT')`
/// and `state IN ('ACK','OFFLINE_LOCAL_ACK')` (same issued-set as SELL/RETURN).
///
/// Pure main-pool SELECT; no network/crypto (invariant #1).
pub async fn aggregate_shift_service_io(
    pool: &SqlitePool,
    fiscal_number: &str,
    shift_id: ShiftId,
) -> sqlx::Result<(i64, i64)> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT doc_type, payload_json FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? \
           AND doc_type IN ('SERVICE_IN','SERVICE_OUT') \
           AND state IN ('ACK','OFFLINE_LOCAL_ACK') \
         ORDER BY lnd",
    )
    .bind(fiscal_number)
    .bind(shift_id)
    .fetch_all(pool)
    .await?;

    let mut service_in: i64 = 0;
    let mut service_out: i64 = 0;

    for (doc_type_str, payload_json) in &rows {
        let Ok(parsed) = serde_json::from_str::<StoredServiceIoPayload>(payload_json) else {
            continue; // malformed: skip
        };
        if parsed.amount_kop <= 0 {
            continue;
        }
        match doc_type_str.as_str() {
            "SERVICE_IN" => service_in = service_in.saturating_add(parsed.amount_kop),
            "SERVICE_OUT" => service_out = service_out.saturating_add(parsed.amount_kop),
            _ => {}
        }
    }

    Ok((service_in, service_out))
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
) -> sqlx::Result<(i64, i64, i64, i64, i64)> {
    use crate::db::models::enums::DocType;

    // SELL / RETURN cash legs.
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT doc_type, payload_json FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? \
           AND doc_type IN ('SELL','RETURN') \
           AND state IN ('ACK','OFFLINE_LOCAL_ACK') \
         ORDER BY lnd",
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

    // SERVICE_IN / SERVICE_OUT — in the same tx snapshot.
    let svc_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT doc_type, payload_json FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? \
           AND doc_type IN ('SERVICE_IN','SERVICE_OUT') \
           AND state IN ('ACK','OFFLINE_LOCAL_ACK') \
         ORDER BY lnd",
    )
    .bind(fiscal_number)
    .bind(shift_id)
    .fetch_all(&mut **tx)
    .await?;

    let mut service_in: i64 = 0;
    let mut service_out: i64 = 0;
    for (doc_type_str, payload_json) in &svc_rows {
        let Ok(parsed) = serde_json::from_str::<StoredServiceIoPayload>(payload_json) else {
            continue;
        };
        if parsed.amount_kop <= 0 {
            continue;
        }
        match doc_type_str.as_str() {
            "SERVICE_IN" => service_in = service_in.saturating_add(parsed.amount_kop),
            "SERVICE_OUT" => service_out = service_out.saturating_add(parsed.amount_kop),
            _ => {}
        }
    }

    // EPZ (CASH_ADVANCE_EPZ) — in the same tx snapshot.  Drives cash OUT.
    let epz_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT payload_json FROM fiscal_documents \
         WHERE fiscal_number = ? AND shift_id = ? \
           AND doc_type = 'CASH_ADVANCE_EPZ' \
           AND state IN ('ACK','OFFLINE_LOCAL_ACK') \
         ORDER BY lnd",
    )
    .bind(fiscal_number)
    .bind(shift_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut epz_out: i64 = 0;
    for (payload_json,) in &epz_rows {
        let Ok(parsed) = serde_json::from_str::<StoredEpzPayload>(payload_json) else {
            continue;
        };
        if parsed.sum_kop <= 0 {
            continue;
        }
        epz_out = epz_out.saturating_add(parsed.sum_kop);
    }

    Ok((cash_sell, cash_return, service_in, service_out, epz_out))
}

/// Derive the closing cash-on-hand for a shift from its DB receipts.
/// `opening_kop` = the shift's stored `cash_balance_kop` (opening anchor).
pub async fn derive_closing_cash(
    pool: &SqlitePool,
    fiscal_number: &str,
    shift_id: ShiftId,
    opening_kop: i64,
) -> sqlx::Result<i64> {
    let (sell, ret, svc_in, svc_out, epz_out) =
        aggregate_shift_cash(pool, fiscal_number, shift_id).await?;
    Ok(derive_cash_on_hand(
        opening_kop,
        sell,
        ret,
        svc_in,
        svc_out,
        epz_out,
    ))
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
        let (sell, ret, svc_in, svc_out, epz_out) =
            aggregate_shift_cash(pool, fiscal_number, shift_id).await?;
        // carry-in = the prior carry (opening of this shift); carry-out = closing.
        carry = derive_cash_on_hand(carry, sell, ret, svc_in, svc_out, epz_out);
    }

    Ok(Some((carry, stored_opening)))
}
