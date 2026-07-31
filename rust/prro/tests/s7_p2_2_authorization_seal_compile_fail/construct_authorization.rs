//! S7-P2-2: the sealed `Authorization` token cannot be constructed outside its module — the only
//! mint path is `authorize_submission`. A struct-literal from an external crate names private
//! fields. Must NOT compile. Expected: E0451 (private field).

use prro::db::repositories::delivery_reservation::Authorization;

#[allow(unreachable_code)]
fn boom() -> Authorization {
    Authorization {
        reservation_id: todo!(),
    }
}

fn main() {}
