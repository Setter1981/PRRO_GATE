//! W4-Z0 piece 3 — repository for `payment_methods` table (per-FN payment forms).
//!
//! Per `docs/superpowers/specs/2026-05-26-w4-z0-config-storage-spec.md`
//! §1.2.  Mirror of W4-Z0 piece 2 (`tax_groups`) — typed input struct,
//! typed conflict + not-found errors, plain `sqlx::query()` runtime-
//! bound (secure pool dir not in offline cache).
//!
//! ## Wire-shape mapping
//!
//! The XML `<M T="...">` attribute is derived as `(pay_index - 1)` per
//! WebCheck convention (see `feedback_operator_ua_fiscal_authority` +
//! `2026-05-26-w4-z1-dps-xml-wire-shape.md` §M element).  Pay_index = 1
//! (Готівка default) → T="0".  This mapping is the builder's concern
//! (W4-Z1); the repository just stores the raw `pay_index`.

use sqlx::SqlitePool;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct PaymentMethod {
    pub fn_id: String,
    pub pay_index: i64,
    pub name: String,
    pub iscash: bool,
    pub is_active: bool,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewPaymentMethod {
    pub fn_id: String,
    pub pay_index: i64,
    pub name: String,
    pub iscash: bool,
}

#[derive(Debug, Error)]
pub enum PaymentMethodsRepoError {
    /// PRIMARY KEY (fn, pay_index) violation.
    #[error("duplicate pay_index: fn={fn_id} pay_index={pay_index}")]
    DuplicatePayIndex { fn_id: String, pay_index: i64 },

    /// Partial unique index `idx_payment_methods_fn_name` violation —
    /// another active row with this name already exists for this FN.
    #[error("duplicate active name: fn={fn_id} name={name}")]
    DuplicateActiveName { fn_id: String, name: String },

    #[error("payment method not found: fn={fn_id} pay_index={pay_index}")]
    NotFound { fn_id: String, pay_index: i64 },

    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

pub async fn insert(
    pool: &SqlitePool,
    new: &NewPaymentMethod,
) -> Result<(), PaymentMethodsRepoError> {
    let result = sqlx::query(
        "INSERT INTO payment_methods (fn, pay_index, name, iscash) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(&new.fn_id)
    .bind(new.pay_index)
    .bind(&new.name)
    .bind(if new.iscash { 1_i64 } else { 0 })
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(classify_insert_error(
            e,
            &new.fn_id,
            new.pay_index,
            &new.name,
        )),
    }
}

fn classify_insert_error(
    e: sqlx::Error,
    fn_id: &str,
    pay_index: i64,
    name: &str,
) -> PaymentMethodsRepoError {
    if let sqlx::Error::Database(ref db) = e {
        if db.is_unique_violation() {
            let msg = db.message();
            // SQLite STRICT-mode UNIQUE failure message contains the
            // colliding column names:
            //   PK: "UNIQUE constraint failed: payment_methods.fn,
            //         payment_methods.pay_index"
            //   name-idx: "UNIQUE constraint failed: payment_methods.fn,
            //         payment_methods.name"
            if msg.contains("pay_index") {
                return PaymentMethodsRepoError::DuplicatePayIndex {
                    fn_id: fn_id.to_string(),
                    pay_index,
                };
            }
            if msg.contains("name") {
                return PaymentMethodsRepoError::DuplicateActiveName {
                    fn_id: fn_id.to_string(),
                    name: name.to_string(),
                };
            }
        }
    }
    PaymentMethodsRepoError::Db(e)
}

/// Update both mutable attributes (name + iscash).  Caller passes
/// canonical values for both — partial updates are not supported
/// (admin CLI surface is per-PR, simpler).
pub async fn update(
    pool: &SqlitePool,
    fn_id: &str,
    pay_index: i64,
    name: &str,
    iscash: bool,
) -> Result<(), PaymentMethodsRepoError> {
    let result = sqlx::query(
        "UPDATE payment_methods SET name = ?, iscash = ? \
          WHERE fn = ? AND pay_index = ?",
    )
    .bind(name)
    .bind(if iscash { 1_i64 } else { 0 })
    .bind(fn_id)
    .bind(pay_index)
    .execute(pool)
    .await
    .map_err(|e| classify_insert_error(e, fn_id, pay_index, name))?;

    if result.rows_affected() == 0 {
        return Err(PaymentMethodsRepoError::NotFound {
            fn_id: fn_id.to_string(),
            pay_index,
        });
    }
    Ok(())
}

pub async fn soft_delete(
    pool: &SqlitePool,
    fn_id: &str,
    pay_index: i64,
) -> Result<(), PaymentMethodsRepoError> {
    let result = sqlx::query(
        "UPDATE payment_methods SET is_active = 0 \
          WHERE fn = ? AND pay_index = ?",
    )
    .bind(fn_id)
    .bind(pay_index)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(PaymentMethodsRepoError::NotFound {
            fn_id: fn_id.to_string(),
            pay_index,
        });
    }
    Ok(())
}

pub async fn find(
    pool: &SqlitePool,
    fn_id: &str,
    pay_index: i64,
) -> Result<Option<PaymentMethod>, PaymentMethodsRepoError> {
    let row = sqlx::query_as::<_, PaymentMethodRowRaw>(
        "SELECT fn, pay_index, name, iscash, is_active, created_at \
         FROM payment_methods \
         WHERE fn = ? AND pay_index = ? LIMIT 1",
    )
    .bind(fn_id)
    .bind(pay_index)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn find_by_name(
    pool: &SqlitePool,
    fn_id: &str,
    name: &str,
) -> Result<Option<PaymentMethod>, PaymentMethodsRepoError> {
    let row = sqlx::query_as::<_, PaymentMethodRowRaw>(
        "SELECT fn, pay_index, name, iscash, is_active, created_at \
         FROM payment_methods \
         WHERE fn = ? AND name = ? AND is_active = 1 LIMIT 1",
    )
    .bind(fn_id)
    .bind(name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn list_active_for_fn(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<PaymentMethod>, PaymentMethodsRepoError> {
    let rows = sqlx::query_as::<_, PaymentMethodRowRaw>(
        "SELECT fn, pay_index, name, iscash, is_active, created_at \
         FROM payment_methods \
         WHERE fn = ? AND is_active = 1 \
         ORDER BY pay_index ASC",
    )
    .bind(fn_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// All payment methods for a FN (active + inactive).  Used by the cash-ledger
/// reconcile seam (`cash_ledger::reconcile_opening_anchor`): historical receipts
/// may reference a form that was later de-activated, so active-only would miss it.
pub async fn list_all_for_fn(
    pool: &SqlitePool,
    fn_id: &str,
) -> Result<Vec<PaymentMethod>, PaymentMethodsRepoError> {
    let rows = sqlx::query_as::<_, PaymentMethodRowRaw>(
        "SELECT fn, pay_index, name, iscash, is_active, created_at \
         FROM payment_methods \
         WHERE fn = ? \
         ORDER BY pay_index ASC",
    )
    .bind(fn_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[derive(sqlx::FromRow)]
struct PaymentMethodRowRaw {
    #[sqlx(rename = "fn")]
    fn_id: String,
    pay_index: i64,
    name: String,
    iscash: i64,
    is_active: i64,
    created_at: String,
}

impl From<PaymentMethodRowRaw> for PaymentMethod {
    fn from(r: PaymentMethodRowRaw) -> Self {
        Self {
            fn_id: r.fn_id,
            pay_index: r.pay_index,
            name: r.name,
            iscash: r.iscash != 0,
            is_active: r.is_active != 0,
            created_at: r.created_at,
        }
    }
}
