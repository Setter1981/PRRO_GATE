//! S7-P2-2: the sealed `Authorization` token must NOT be `Clone` (P2 Layer 3). A clonable token
//! could be duplicated and submitted twice → two wires for one document. Must NOT compile.
//! Expected: E0599 (no method `clone`).

use prro::db::repositories::delivery_reservation::Authorization;

fn boom(auth: Authorization) -> (Authorization, Authorization) {
    let dup = auth.clone();
    (auth, dup)
}

fn main() {}
