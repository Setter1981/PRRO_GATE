//! RS-2 piece-5b-ii — D1 frozen-slot startup preflight.
//!
//! Validates the per-FN `payment_methods` rows against the D1 frozen-slot
//! policy at supervisor startup (BEFORE any `RestHttp` listener binds), so a
//! mis-seeded slot fails the boot LOUDLY instead of failing the first cashless
//! receipt at convert-time.
//!
//! Policy (operator-locked, RS-2 §0.4 D1 / CF3):
//!   - **slot 1 (CASH)** MUST exist + be active + `iscash = true` → else
//!     boot-fail (the cash baseline every RestHttp FN needs);
//!   - any **PRESENT + active** slot in `2..=4` MUST have `iscash = false` →
//!     else boot-fail (kind drift — a cashless slot mislabelled as cash);
//!   - a **MISSING / inactive** slot `2..=4` is NOT a boot failure — `convert`
//!     is already fail-closed (`MissingPaymentMethod` / `InactivePaymentMethod`)
//!     at the actual cashless receipt, so a cash-only pilot is not blocked.

use sqlx::SqlitePool;
use thiserror::Error;

use crate::db::repositories::payment_methods;

/// D1 frozen pay-slot indices (CASH=1, CASHLESS_1=2, CASHLESS_2=3, CASHLESS_3=4).
const CASH_SLOT: i64 = 1;
const CASHLESS_SLOTS: [i64; 3] = [2, 3, 4];

#[derive(Debug, Error)]
pub enum D1PreflightError {
    #[error(
        "fn {fiscal_number}: D1 cash slot (pay_index=1) is missing or inactive — every RestHttp \
         FN needs an active CASH baseline"
    )]
    CashSlotMissing { fiscal_number: String },

    #[error(
        "fn {fiscal_number}: D1 cash slot (pay_index=1) has iscash=false — slot 1 must be cash"
    )]
    CashSlotNotCash { fiscal_number: String },

    #[error(
        "fn {fiscal_number}: D1 cashless slot pay_index={pay_index} has iscash=true — slots 2..4 \
         must be cashless (kind drift)"
    )]
    CashlessSlotIsCash {
        fiscal_number: String,
        pay_index: i64,
    },

    #[error("payment_methods read failed for fn {fiscal_number}: {source}")]
    Repo {
        fiscal_number: String,
        #[source]
        source: payment_methods::PaymentMethodsRepoError,
    },
}

/// Run the D1 preflight for ONE fiscal number.  Reads ACTIVE `payment_methods`
/// only (`list_active_for_fn` filters `is_active = 1`), so a "missing" slot 1
/// covers both absent and soft-deleted rows.
pub async fn preflight_d1_slots(
    secure_pool: &SqlitePool,
    fiscal_number: &str,
) -> Result<(), D1PreflightError> {
    let active = payment_methods::list_active_for_fn(secure_pool, fiscal_number)
        .await
        .map_err(|source| D1PreflightError::Repo {
            fiscal_number: fiscal_number.to_string(),
            source,
        })?;

    // Slot 1 (CASH): present (active) + iscash = true.
    match active.iter().find(|m| m.pay_index == CASH_SLOT) {
        None => {
            return Err(D1PreflightError::CashSlotMissing {
                fiscal_number: fiscal_number.to_string(),
            })
        }
        Some(m) if !m.iscash => {
            return Err(D1PreflightError::CashSlotNotCash {
                fiscal_number: fiscal_number.to_string(),
            })
        }
        Some(_) => {}
    }

    // Slots 2..=4: if present + active, must be iscash = false.
    for m in &active {
        if CASHLESS_SLOTS.contains(&m.pay_index) && m.iscash {
            return Err(D1PreflightError::CashlessSlotIsCash {
                fiscal_number: fiscal_number.to_string(),
                pay_index: m.pay_index,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_secure_pool;
    use crate::db::repositories::payment_methods::{insert, soft_delete, NewPaymentMethod};

    const FN: &str = "4000000001";

    async fn pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let p = open_secure_pool(&dir.path().join("secure.db"))
            .await
            .unwrap();
        (dir, p)
    }

    fn pm(pay_index: i64, name: &str, iscash: bool) -> NewPaymentMethod {
        NewPaymentMethod {
            fn_id: FN.to_string(),
            pay_index,
            name: name.to_string(),
            iscash,
        }
    }

    /// Cash-only (just slot 1) is valid — missing cashless slots do NOT
    /// boot-fail.
    #[tokio::test]
    async fn cash_only_baseline_ok() {
        let (_d, p) = pool().await;
        insert(&p, &pm(1, "Готівка", true)).await.unwrap();
        preflight_d1_slots(&p, FN)
            .await
            .expect("cash-only must pass");
    }

    /// Full cash + cashless layout (correct iscash) is valid.
    #[tokio::test]
    async fn full_correct_layout_ok() {
        let (_d, p) = pool().await;
        insert(&p, &pm(1, "Готівка", true)).await.unwrap();
        insert(&p, &pm(2, "Картка", false)).await.unwrap();
        insert(&p, &pm(3, "Картка2", false)).await.unwrap();
        preflight_d1_slots(&p, FN)
            .await
            .expect("correct layout must pass");
    }

    /// No slot 1 at all → boot-fail (cash baseline missing).
    #[tokio::test]
    async fn missing_cash_slot_fails() {
        let (_d, p) = pool().await;
        insert(&p, &pm(2, "Картка", false)).await.unwrap();
        match preflight_d1_slots(&p, FN).await {
            Err(D1PreflightError::CashSlotMissing { .. }) => {}
            other => panic!("expected CashSlotMissing, got {other:?}"),
        }
    }

    /// Slot 1 mislabelled cashless → boot-fail.
    #[tokio::test]
    async fn cash_slot_not_cash_fails() {
        let (_d, p) = pool().await;
        insert(&p, &pm(1, "Готівка", false)).await.unwrap();
        match preflight_d1_slots(&p, FN).await {
            Err(D1PreflightError::CashSlotNotCash { .. }) => {}
            other => panic!("expected CashSlotNotCash, got {other:?}"),
        }
    }

    /// A cashless slot (2..4) mislabelled cash → boot-fail (kind drift).
    #[tokio::test]
    async fn cashless_slot_marked_cash_fails() {
        let (_d, p) = pool().await;
        insert(&p, &pm(1, "Готівка", true)).await.unwrap();
        insert(&p, &pm(2, "Картка", true)).await.unwrap();
        match preflight_d1_slots(&p, FN).await {
            Err(D1PreflightError::CashlessSlotIsCash { pay_index: 2, .. }) => {}
            other => panic!("expected CashlessSlotIsCash(2), got {other:?}"),
        }
    }

    /// An INACTIVE (soft-deleted) cashless-marked-cash slot does NOT boot-fail
    /// — the preflight reads active rows only.
    #[tokio::test]
    async fn inactive_drifted_slot_is_ignored() {
        let (_d, p) = pool().await;
        insert(&p, &pm(1, "Готівка", true)).await.unwrap();
        insert(&p, &pm(2, "BadCard", true)).await.unwrap();
        soft_delete(&p, FN, 2).await.unwrap();
        preflight_d1_slots(&p, FN)
            .await
            .expect("an inactive drifted slot must not boot-fail");
    }
}
