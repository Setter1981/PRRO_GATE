//! Case 2 (W0-2 §9.2): caller calls `WriteTxConn::new` from outside
//! `db::tx`.  Must NOT compile — `new` is module-private (NOT
//! `pub(crate)`), so it is invisible even to other modules in the
//! `prro` crate, let alone external crates.  Expected: E0603 function
//! `new` is private.

use prro::db::tx::WriteTxConn;

fn boom(conn: &mut sqlx::SqliteConnection) {
    let _ = WriteTxConn::new(conn);
}

fn main() {}
