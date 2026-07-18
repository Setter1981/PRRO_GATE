//! Must NOT compile: the CS-3 3.2 provenance types are sealed (private field). Only the transport
//! decoder mints them via `from_transport`; no external literal can fabricate a fiscal id or a
//! status code — so the engine can never inject provenance it did not observe on the wire.

use prro_domain::delivery::{NonEmptyFiscalNumber, NonOkStatusCode};

fn main() {
    // Private field → "cannot initialize a tuple struct which contains private fields" (E0423).
    let _id = NonEmptyFiscalNumber("DPS-forged".to_string());
    let _code = NonOkStatusCode(-1);
}
