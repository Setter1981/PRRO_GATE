//! RS-1 Piece 3b — `build_fn_sign` produces a well-formed ATTACHED CMS.
//!
//! Closes the crypto-seam review LOW: in the pieces-2/3/3b commit
//! `build_fn_sign` had no caller and no test (its only consumer is the
//! not-yet-landed Piece-5 supervisor).  This locks the structural
//! contract (attached CAdES-BES over the FN string, no panic, non-trivial
//! DER) against a future regression (e.g. an edit flipping `attached`).
//!
//! Uses a real DER DSTU-4145 cert fixture so the CMS `SignerInfo`
//! issuer/serial parse exercises the real path.  The private scalar is
//! arbitrary — this proves WELL-FORMEDNESS, not on-wire DPS acceptance
//! (that is the live-DPS smoke's job, owed to WL-2).

use prro::crypto::session::SigningSession;
use prro::runtime::key_loader::build_fn_sign;

const FIXTURE_CERT_DER: &[u8] = include_bytes!("fixtures/SELF_SIGNED_ENC_6929.cer");

#[test]
fn build_fn_sign_produces_wellformed_attached_cms() {
    let session =
        SigningSession::new_for_test("OP-1".to_string(), [7u8; 32], FIXTURE_CERT_DER.to_vec());

    let blob = build_fn_sign(&session, "4000000001")
        .expect("build_fn_sign must produce a CMS over a valid cert");

    // Attached CAdES-BES CMS is a top-level ASN.1 SEQUENCE (DER tag 0x30).
    assert_eq!(
        blob.0.first(),
        Some(&0x30),
        "CMS DER must start with a SEQUENCE tag"
    );
    // `attached: true` embeds the FN eContent + the 525-byte cert + signed
    // attrs + signature, so the blob must exceed the embedded cert alone.
    assert!(
        blob.0.len() > FIXTURE_CERT_DER.len(),
        "attached CMS must exceed the embedded cert ({} bytes), got {}",
        FIXTURE_CERT_DER.len(),
        blob.0.len(),
    );
}
