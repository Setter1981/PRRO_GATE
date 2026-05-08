//! Case 5a (W0-2 §9.2): caller calls `WriteTxConn::new_for_test`
//! from outside `db::tx`.  Must NOT compile — `new_for_test` is
//! `#[cfg(test)] pub(super) fn`, so:
//!   - In dependency builds (the trybuild fixture compiles `prro`
//!     without `--cfg test`) the symbol does not exist at all.
//!   - Even with `--cfg test`, `pub(super)` keeps it visible only
//!     inside `db::tx`.
//! Either path produces a compile failure.

use prro::db::tx::WriteTxConn;

fn boom(conn: &mut sqlx::SqliteConnection) {
    let _ = WriteTxConn::new_for_test(conn);
}

fn main() {}
