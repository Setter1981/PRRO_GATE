//! CS-3 S7-1 — `submit_authorized`: the SOLE production wire function (design §0/§2 Layer 3,
//! spec4b §6).
//!
//! It consumes an [`Authorization`] **by value**, verifies the envelope-hash rebind guard + the
//! full 5-binding echo (AO-2) BEFORE any wire I/O, performs EXACTLY ONE `send_chk_observed`, and
//! returns a post-wire [`AttemptObservation`]. Because the token is consumed, no wire-capable value
//! survives the call — the same authorization can never reach the wire twice (P2 Layer 3). A refusal
//! (rebind / binding mismatch) performs ZERO wire I/O.
//!
//! **INACTIVE (S7-1 foundation).** No production caller wires it yet; at the whole-fence cutover
//! `stage_send::run` becomes its sole caller and the wire relocates here from `stage_send.rs:1568`.

use crate::db::repositories::delivery_reservation::{AttemptObservation, Authorization};
use crate::services::write_path::shadow_map::map_send_reply;
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::CheckEnvelope;
use prro_domain::delivery::{DpsProtocolBinding, EnvelopeHash, SubmissionEvidence};
use prro_domain::enums::DocType;
use sha2::{Digest, Sha256};

/// Why [`submit_authorized`] refused BEFORE performing any wire I/O. Both variants guarantee ZERO
/// `send_chk_observed` calls — the token is dropped without ever reaching the channel.
#[derive(Debug)]
pub enum SubmitRefused {
    /// The envelope bytes do not hash to the token's `envelope_hash`: the bound bytes were re-bound
    /// after authorization (rebind guard, design §2 Layer 3).
    EnvelopeRebind,
    /// The port's live protocol binding differs from the token's authorized binding (AO-2 echo,
    /// spec4b §4). Checking only protocol id/version is insufficient — the FULL 5-binding is echoed.
    BindingMismatch,
}

/// Perform the sole authorized wire submission (design §0/§2 Layer 3).
///
/// - `channel` — the bound DPS channel; `send_chk_observed` is invoked EXACTLY once.
/// - `port_binding` — the channel's live immutable binding snapshot (the #4B `binding()` value);
///   AO-2 requires it to equal the token's authorized binding.
/// - `auth` — the sealed [`Authorization`], consumed by value.
/// - `envelope` — the already-bound signed wire envelope; `SHA256(check_sign)` must equal the
///   token's `envelope_hash`.
/// - `doc_type` — consumed only by the evidence map for the close/non-close (`-2/-15`) split.
///
/// On success returns an [`AttemptObservation`] carrying the token's immutable identity/binding/hash
/// plus a `Started` evidence over the observed wire response. It carries NO wire capability.
pub async fn submit_authorized(
    channel: &dyn DpsChannel,
    port_binding: &DpsProtocolBinding,
    auth: Authorization,
    envelope: CheckEnvelope,
    doc_type: DocType,
) -> Result<AttemptObservation, SubmitRefused> {
    // 1. Rebind guard — the wire bytes MUST hash to the authorized envelope hash (design §2 Layer 3).
    //    SHA-256 of the CMS-signed blob is the bound-envelope identity (`EnvelopeHash`, 032:85).
    let computed: [u8; 32] = <Sha256 as Digest>::digest(&envelope.check_sign).into();
    if &computed != auth.envelope_hash() {
        return Err(SubmitRefused::EnvelopeRebind);
    }

    // 2. AO-2 binding echo — the FULL 5-binding tuple (protocol id + contract + capability +
    //    endpoint), not just id/version. A mismatch returns before any wire I/O.
    let binding_ok = auth.dps_protocol_id() == port_binding.protocol_id.as_str()
        && auth.protocol_contract_version() == i64::from(port_binding.contract_version.0)
        && auth.capability_profile_version()
            == port_binding
                .capability_profile_version
                .map(|v| i64::from(v.0))
        && auth.endpoint_config_revision()
            == port_binding
                .endpoint_config_revision
                .map(|v| i64::from(v.0));
    if !binding_ok {
        return Err(SubmitRefused::BindingMismatch);
    }

    // Snapshot the authorized hash before the token is consumed (step 5) / the envelope is moved.
    let envelope_hash = EnvelopeHash(*auth.envelope_hash());

    // 3. The SOLE wire call. Post-`CALL_STARTED`, evidence is ALWAYS `Started` (SE-1) — even a crash
    //    surfaces as `Started{NoResponse(CrashedBeforeObservation)}`. The `_legacy` arm is the
    //    reconciliation-degraded projection; the authoritative evidence is `observation.evidence()`.
    let (_legacy, observation) = channel.send_chk_observed(envelope).await;

    // 4. Map the transport-minted evidence into the typed `SendResponse` and wrap it as `Started`
    //    over the authorized binding + hash (AO-2 holds by construction: the binding IS auth's).
    let response = map_send_reply(observation.evidence(), doc_type);
    let evidence = SubmissionEvidence::Started {
        response,
        binding: port_binding.clone(),
        envelope_hash,
    };

    // 5. Mint the post-wire observation, CONSUMING the token by value (P2 Layer 3).
    Ok(AttemptObservation::from_authorization(auth, evidence))
}
