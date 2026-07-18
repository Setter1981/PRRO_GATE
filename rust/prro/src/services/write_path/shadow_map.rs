//! CS-3 3.2 PR4 — the engine-owned pure mapping (spec §4.3): `RawSendReply` + the store-owned
//! `doc_type` → the typed `SendResponse` delivery contract.
//!
//! This is THE ONE §4.4-gated engine-mapper file: the only place under `prro/src` that constructs a
//! `SendResponse` (via the authority ctors `parsed`/`no_response`/`remote_status`) or calls the
//! domain's `SendOutcome` authority ctors (`accepted`/`ok_but_no_fiscal_number`/`missing_status`/
//! `from_server_code`). The source-gate (`digest_mint_source_gate.rs`) enforces that placement.
//!
//! The mapper only FORWARDS transport-minted evidence — the digest / fiscal id / status code it
//! passes on are read out of the opaque, transport-minted `RawSendReply` (its `kind()` borrowed
//! view); it never mints a digest/id/code of its own. The `-2/-15` close/Z split is NOT here — it
//! lives in the domain's `from_server_code`, the sole owner of the code×doc_type table (so `Close`
//! vs `CloseAmbiguous` can never be mismatched at the mapper).
//!
//! READ-ONLY in 3.2: the mapped `SendResponse` drives nothing (no record, no state, no second
//! wire); the §4.6 drift-pin proves it agrees with the live `route_send_result` outcome.

use prro_domain::delivery::{RemoteStatusEvidence, SendOutcome, SendResponse};
use prro_domain::enums::DocType;

use crate::transports::dps::{RawSendReply, RawSendReplyKind};

/// Total map over the 6 `RawSendReplyKind` arms (spec §4.3). `doc_type` is consumed ONLY in the
/// `ServerCode` arm (the domain resolves verdict vs indeterminate by code×doc_type).
pub(crate) fn map_send_reply(reply: &RawSendReply, doc_type: DocType) -> SendResponse {
    match reply.kind() {
        // Row 1 — OK + non-empty id: forward the transport-proven fiscal number (no digest, D-4).
        RawSendReplyKind::Accepted { fiscal_id } => {
            SendResponse::parsed(SendOutcome::accepted(fiscal_id.clone()))
        }
        // Row 2 — OK + empty id → the indeterminate that captures what a bare OK erases.
        RawSendReplyKind::OkNoFiscalId { digest } => {
            SendResponse::parsed(SendOutcome::ok_but_no_fiscal_number(*digest))
        }
        // Rows 3/4/5/6 — a non-OK DPS status code: the domain resolves verdict / CloseAmbiguous /
        // SaveError / UnknownStatus by code×doc_type. (Unknown non-zero arrives here too — §4.3
        // row-6 — and maps to UnknownStatus.)
        RawSendReplyKind::ServerCode { code, digest } => {
            SendResponse::parsed(SendOutcome::from_server_code(code, doc_type, *digest))
        }
        // Row 7 — proto status==0 → decode-indeterminate missing status (D-2).
        RawSendReplyKind::MissingStatus { digest } => {
            SendResponse::parsed(SendOutcome::missing_status(*digest))
        }
        // Row 8 — TLS-proven authenticated peer, non-DPS body → remote status (NOT a parsed
        // envelope). Maps DIRECTLY, never through a parsed `SendOutcome`.
        RawSendReplyKind::RemoteAuthStatus { grpc } => {
            SendResponse::remote_status(RemoteStatusEvidence::RemoteAuthStatus(*grpc))
        }
        // Rows 9/10 — no trusted DPS reply (catch-all non-DPS status, or genuine absence). DIRECT.
        RawSendReplyKind::NoResponse { cause } => SendResponse::no_response(cause),
    }
}
