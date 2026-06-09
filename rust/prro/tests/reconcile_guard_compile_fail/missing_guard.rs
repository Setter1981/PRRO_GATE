//! W2 module-level enforcement (M3b).  Caller invokes
//! `boot_phase::run_boot_reconciliation` without the
//! `&ReconcileGuard<'_>` first parameter.  Must NOT compile — the
//! W2 signature change makes the token mandatory.  Expected: E0061
//! "this function takes 4 arguments but 3 arguments were supplied".
//!
//! This proves the W2 contract structurally: any code path that
//! invokes `run_boot_reconciliation` without going through
//! `App::reconcile_pending_inner` (which mints the production token
//! from the App mutex) OR through the explicit test seam
//! `ReconcileGuard::for_integration_test_only` (which is
//! `#[doc(hidden)]` + named to discourage prod use) fails to compile.

use prro::services::reconciliation::boot_phase;
use sqlx::SqlitePool;

async fn boom(pool: &SqlitePool) {
    // Missing the &ReconcileGuard<'_> first arg.
    let _ = boot_phase::run_boot_reconciliation(pool, "1234567890", None).await;
}

fn main() {}
