//! Must NOT compile: every construction below tries to bypass the sealed delivery
//! algebra. Each targets a distinct seal, so removing any single seal still leaves the
//! file failing — the canary only turns GREEN→RED when ALL seals are intact→broken.
//!
//! The sole legitimate constructor is `SendOutcome::from_dps_status`.

use prro_domain::delivery::{SendIndeterminate, SendOutcome, UnknownStatusCode};

fn main() {
    // 1. `SendOutcome` wraps a PRIVATE inner enum — external tuple construction is a
    //    privacy error (field `0` is private). The `!`-typed arg is deliberate: if the
    //    field were unsealed to `pub`, `!` would coerce to the inner type and this WOULD
    //    compile — that is exactly the teeth (unseal → canary RED).
    let _bypass_outcome = SendOutcome(unimplemented!());

    // 2. `SendIndeterminate` likewise wraps a PRIVATE inner enum.
    let _bypass_indeterminate = SendIndeterminate(unimplemented!());

    // 3. `UnknownStatusCode`'s field is PRIVATE — only `from_dps_status` may mint a code
    //    proven not to be a named `DpsReject` (the checkpoint's `UnknownStatus("-1")`).
    let _bypass_code = UnknownStatusCode(-1);
}
