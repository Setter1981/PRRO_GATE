//! CS-3 3.2 PR1 — byte-exact, versioned digest framing (spec §4.1).
//!
//! The *decoded-content* digest of a DPS reply is a **collision-resistant fingerprint of the
//! KNOWN decoded fields** — NOT a raw-wire proof (a re-encode drops unknown fields; that is a
//! future forensic slice, [[project_digest_decoded_content_decision]]). The framing is fixed and
//! length-prefixed so distinct decoded content yields a distinct digest:
//!
//! ```text
//! digest = SHA-256( DOMAIN_TAG ‖ FRAMING_VERSION:u8 ‖ msg_type:u8 ‖ Σ len(field):u32be ‖ enc(field) )
//! enc(bool)      = 1 byte (0/1)
//! enc(i32|i64)   = i64 big-endian (8 bytes, sign-extended)   — incl. the canonical gRPC numeric code
//! enc(string|bytes) = raw bytes
//! enc(repeated)  = count:u32be ‖ for each elem: len(block(elem)):u32be ‖ block(elem)
//! enc(nested)    = block(nested.fields)                       — recursive
//! ```
//!
//! **Auditor condition (b):** the golden-vector test recomputes this framing INDEPENDENTLY (see
//! `tests`), it does not call this production helper — so a bug here cannot hide behind a shared
//! oracle. Field order is proto field-number ascending; the per-message field lists are pinned by
//! those golden vectors (`prro/proto/fiscal_server.proto:36-112`).

// Wired into the digest mint (`response_digest`/`status_digest`) by PR1 pin 4; until then the
// helpers are exercised only by the golden tests, so the non-test build sees them as unused.
#![allow(dead_code)]

use sha2::{Digest, Sha256};

/// 15-byte domain-separation tag (spec §4.1). Distinguishes this digest from any other SHA-256 use.
pub(crate) const DOMAIN_TAG: &[u8; 15] = b"PRRO-DPS-DIGEST";

/// Fixed framing version — NOT the reservation `contract_version` (unavailable to the transport).
pub(crate) const FRAMING_VERSION: u8 = 1;

/// Message-type discriminant byte (spec §4.1). Part of the domain separation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MsgType {
    CheckResponse = 0x01,
    StatusResponse = 0x02,
    RroInfoResponse = 0x03,
    GrpcStatus = 0x10,
}

/// Length-prefixed field-block accumulator over a SHA-256 state.
///
/// Every field is written as `len:u32be ‖ bytes`, so no two distinct field sequences can collide by
/// concatenation. `nested`/`repeated` recurse by building a child block and writing it as one field.
pub(crate) struct DigestFramer {
    hasher: Sha256,
}

impl DigestFramer {
    /// Start a framed digest: `DOMAIN_TAG ‖ FRAMING_VERSION ‖ msg_type`.
    pub(crate) fn new(msg_type: MsgType) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(DOMAIN_TAG);
        hasher.update([FRAMING_VERSION]);
        hasher.update([msg_type as u8]);
        Self { hasher }
    }

    /// Write one already-encoded field as `len:u32be ‖ bytes`.
    fn raw_field(&mut self, bytes: &[u8]) {
        // u32 length prefix — a reply field longer than u32::MAX bytes is impossible over gRPC.
        self.hasher.update((bytes.len() as u32).to_be_bytes());
        self.hasher.update(bytes);
    }

    /// `enc(bool)` = 1 byte.
    pub(crate) fn field_bool(&mut self, v: bool) -> &mut Self {
        self.raw_field(&[v as u8]);
        self
    }

    /// `enc(i32|i64)` = `i64` big-endian (8 bytes). The gRPC numeric code uses this too.
    pub(crate) fn field_int(&mut self, v: i64) -> &mut Self {
        self.raw_field(&v.to_be_bytes());
        self
    }

    /// `enc(string|bytes)` = raw bytes, length-prefixed.
    pub(crate) fn field_bytes(&mut self, v: &[u8]) -> &mut Self {
        self.raw_field(v);
        self
    }

    /// `enc(string)` — convenience for UTF-8 strings.
    pub(crate) fn field_str(&mut self, v: &str) -> &mut Self {
        self.raw_field(v.as_bytes());
        self
    }

    /// `enc(repeated<T>)` = `count:u32be ‖ for each elem: len(block):u32be ‖ block`.
    ///
    /// `elem` builds each element's field block into a fresh sub-hasher-independent buffer; the
    /// buffer is then written as one length-prefixed field, so the element boundary is unambiguous.
    pub(crate) fn field_repeated<T>(
        &mut self,
        items: &[T],
        mut elem: impl FnMut(&mut BlockBuf, &T),
    ) -> &mut Self {
        let mut outer = BlockBuf::new();
        outer.raw(&(items.len() as u32).to_be_bytes());
        for it in items {
            let mut inner = BlockBuf::new();
            elem(&mut inner, it);
            outer.field(inner.as_bytes());
        }
        self.raw_field(outer.as_bytes());
        self
    }

    /// Finalize into the 32-byte digest.
    pub(crate) fn finalize(self) -> [u8; 32] {
        self.hasher.finalize().into()
    }
}

/// A byte buffer that mirrors [`DigestFramer`]'s length-prefixed field encoding, used for nested /
/// repeated element blocks (which must be materialized before their length is known).
pub(crate) struct BlockBuf {
    buf: Vec<u8>,
}

impl BlockBuf {
    pub(crate) fn new() -> Self {
        Self { buf: Vec::new() }
    }
    fn raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }
    /// `len:u32be ‖ bytes` — identical to [`DigestFramer::raw_field`].
    pub(crate) fn field(&mut self, bytes: &[u8]) -> &mut Self {
        self.buf
            .extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        self.buf.extend_from_slice(bytes);
        self
    }
    pub(crate) fn field_bool(&mut self, v: bool) -> &mut Self {
        self.field(&[v as u8])
    }
    pub(crate) fn field_int(&mut self, v: i64) -> &mut Self {
        self.field(&v.to_be_bytes())
    }
    pub(crate) fn field_str(&mut self, v: &str) -> &mut Self {
        self.field(v.as_bytes())
    }
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.buf
    }
}

#[cfg(test)]
mod tests {
    //! Golden-preimage tests (auditor condition b): these recompute the framing **independently**
    //! from raw bytes — they do NOT call `DigestFramer` — and assert the production framer matches.
    //! A bug in `DigestFramer` (or a silent framing change) therefore cannot hide behind a shared
    //! oracle: the two implementations must agree byte-for-byte.

    use super::*;

    /// Independent length-prefix: `len:u32be ‖ bytes` (mirror of the spec, hand-written).
    fn lp(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(bytes);
    }

    /// Independent preimage prefix `DOMAIN_TAG ‖ FRAMING_VERSION ‖ msg_type`, hand-written.
    fn preimage_prefix(msg_type: u8) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(b"PRRO-DPS-DIGEST"); // literal, NOT the DOMAIN_TAG const, to catch a tag change
        p.push(1); // literal FRAMING_VERSION
        p.push(msg_type);
        p
    }

    fn sha(bytes: &[u8]) -> [u8; 32] {
        Sha256::digest(bytes).into()
    }

    #[test]
    fn check_response_shape_matches_independent_preimage() {
        // A CheckResponse-shaped field set: id(str), status(i32), id_sign(bytes), data_sign(bytes),
        // error_message(str) — proto field-number order.
        let id = "DPS-42";
        let status: i64 = 1;
        let id_sign = &[0xAA_u8, 0xBB];
        let data_sign = &[0x01_u8, 0x02, 0x03];
        let err_msg = "";

        let produced = {
            let mut f = DigestFramer::new(MsgType::CheckResponse);
            f.field_str(id)
                .field_int(status)
                .field_bytes(id_sign)
                .field_bytes(data_sign)
                .field_str(err_msg);
            f.finalize()
        };

        let expected = {
            let mut p = preimage_prefix(0x01);
            lp(&mut p, id.as_bytes());
            lp(&mut p, &status.to_be_bytes());
            lp(&mut p, id_sign);
            lp(&mut p, data_sign);
            lp(&mut p, err_msg.as_bytes());
            sha(&p)
        };

        assert_eq!(
            produced, expected,
            "framing must equal the independent preimage"
        );
    }

    #[test]
    fn grpc_status_uses_numeric_code_not_debug() {
        // GrpcStatus{code,message,details}. `code` is the canonical numeric (e.g. Unauthenticated=16),
        // encoded as i64-be — NOT a Debug string.
        let code: i64 = 16;
        let message = "invalid token";
        let details = &[0x09_u8, 0x09];

        let produced = {
            let mut f = DigestFramer::new(MsgType::GrpcStatus);
            f.field_int(code).field_str(message).field_bytes(details);
            f.finalize()
        };
        let expected = {
            let mut p = preimage_prefix(0x10);
            lp(&mut p, &code.to_be_bytes());
            lp(&mut p, message.as_bytes());
            lp(&mut p, details);
            sha(&p)
        };
        assert_eq!(produced, expected);

        // A Debug-string encoding ("Unauthenticated") would differ — pin that we do NOT do that.
        let debug_framed = {
            let mut f = DigestFramer::new(MsgType::GrpcStatus);
            f.field_str("Unauthenticated")
                .field_str(message)
                .field_bytes(details);
            f.finalize()
        };
        assert_ne!(
            produced, debug_framed,
            "numeric code must not equal a Debug-string framing"
        );
    }

    #[test]
    fn repeated_nested_operators_are_recursive_and_counted() {
        // RroInfoResponse.operators (repeated Operator{serial,status,senior,isname}). Two elements.
        let ops = [
            ("SER-1", 1_i64, true, "Ann"),
            ("SER-2", 2_i64, false, "Bob"),
        ];

        let produced = {
            let mut f = DigestFramer::new(MsgType::RroInfoResponse);
            f.field_repeated(&ops, |b, (serial, status, senior, isname)| {
                b.field_str(serial)
                    .field_int(*status)
                    .field_bool(*senior)
                    .field_str(isname);
            });
            f.finalize()
        };

        let expected = {
            let mut p = preimage_prefix(0x03);
            // repeated field = one length-prefixed block: [count:u32be] then per-elem [len(block):u32be ‖ block]
            let mut rep = Vec::new();
            rep.extend_from_slice(&(ops.len() as u32).to_be_bytes());
            for (serial, status, senior, isname) in ops {
                let mut blk = Vec::new();
                lp(&mut blk, serial.as_bytes());
                lp(&mut blk, &status.to_be_bytes());
                lp(&mut blk, &[senior as u8]);
                lp(&mut blk, isname.as_bytes());
                lp(&mut rep, &blk);
            }
            lp(&mut p, &rep);
            sha(&p)
        };

        assert_eq!(produced, expected);
    }

    #[test]
    fn distinct_content_yields_distinct_digest() {
        let base = {
            let mut f = DigestFramer::new(MsgType::CheckResponse);
            f.field_str("DPS-42").field_int(1);
            f.finalize()
        };
        // change one field → different digest (collision-resistant fingerprint of decoded content)
        let changed = {
            let mut f = DigestFramer::new(MsgType::CheckResponse);
            f.field_str("DPS-43").field_int(1);
            f.finalize()
        };
        // same content, different msg_type → different digest (domain separation)
        let other_type = {
            let mut f = DigestFramer::new(MsgType::StatusResponse);
            f.field_str("DPS-42").field_int(1);
            f.finalize()
        };
        // length-prefix prevents the classic "ab|c" == "a|bc" concatenation collision
        let split_a = {
            let mut f = DigestFramer::new(MsgType::CheckResponse);
            f.field_str("ab").field_str("c");
            f.finalize()
        };
        let split_b = {
            let mut f = DigestFramer::new(MsgType::CheckResponse);
            f.field_str("a").field_str("bc");
            f.finalize()
        };
        assert_ne!(base, changed);
        assert_ne!(base, other_type);
        assert_ne!(split_a, split_b);
    }
}
