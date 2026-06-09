//! Case 3 (W0-2 §9.2): caller fabricates a `WriteTxConn` via
//! struct-literal syntax, naming the private `_seal: ()` field.
//! Must NOT compile — both `inner` and `_seal` are private fields,
//! so struct-literal construction from outside `db::tx` is rejected.
//! Expected: E0451 field is private.

use prro::db::tx::WriteTxConn;

fn boom<'a>(conn: &'a mut sqlx::SqliteConnection) -> WriteTxConn<'a> {
    WriteTxConn {
        inner: conn,
        _seal: (),
    }
}

fn main() {}
