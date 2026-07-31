//! Must NOT compile: `classify` takes ONLY the sealed evidence.
//!
//! The `-2/-15` close/non-close split is consumed exactly ONCE, at `SendOutcome::from_server_code`
//! construction (the single source of doc_type). A sealed outcome therefore already
//! encodes its doc-type-relevant distinction in its variant (`Rejected(Close)` vs
//! `Indeterminate(CloseAmbiguous)`), and `classify` has no doc_type-dependent logic left.
//! Passing a second, independent `doc_type` to `classify` — the re-binding vector where a
//! `Sell`-built outcome is re-classified as `ZReport` — is an arity error, so the illegal
//! cross-context classification is structurally unexpressible.
//!
//! TEETH: re-add a `doc_type` parameter to `classify` → this compiles → the canary RED.

use prro_domain::delivery::classify;
use prro_domain::enums::DocType;

fn main() {
    // Re-bind attempt: a second `doc_type` argument. `classify` takes exactly one.
    let _ = classify(unimplemented!(), DocType::ZReport);
}
