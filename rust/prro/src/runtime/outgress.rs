//! W4-Z0 piece 10 — outgress dispatch trait quartet (architectural scaffolding).
//!
//! Per spec §5 + `project_m4_outgress_architecture` memory.
//! Defines the **4 traits** that abstract per-outgress concerns so
//! pilot ships FSCO/ZZD only and post-pilot adds EVPZ_DPS via new
//! impl blocks without refactoring the canonical layer.
//!
//! Traits:
//!   * [`DpsXmlBuilder`]      — serialise `CanonicalFiscalCommand`
//!     into outgress-specific bytes
//!   * [`SignEnvelope`]       — CMS / signed-file wrap
//!   * [`DpsTransport`]       — submit signed bytes to DPS endpoint
//!   * [`DpsResponseParser`]  — decode raw response into [`DpsOutcome`]
//!
//! Pilot impls in this file are **skeletons** returning typed
//! `Err(BuildError::Unimplemented(...))` and equivalents (per Round-2
//! audit: `todo!()` would panic the supervisor on accidental
//! dispatch).  W4-Z1 (builder), W4-Z2 (canonical_doc_digest), and
//! W4-Z3 (live DPS smoke) fill them in.  EVPZ impls are POST-PILOT
//! placeholders returning the same typed Unimplemented; runtime
//! `dispatch_to_outgress` surfaces both via the trait chain as
//! `OutgressError::{Build,Sign,Transport,Parse}` (Round-3 audit
//! removed the hardcoded EVPZ early-exit + dead
//! `ProfileNotImplemented` variant).
//!
//! This piece's primary value: locking the public API surface so
//! downstream worklets can be parallel-implemented without scope
//! drift.

use crate::services::write_path::types::CanonicalFiscalCommand;
use async_trait::async_trait;
use std::sync::Arc;
use thiserror::Error;

// Re-export from the repository module so a single import sources
// the enum + the persistence layer.
pub use crate::db::repositories::fn_outgress_profile::OutgressProfile;

// ─── Builder/transport/sign-envelope context types ────────────────

/// Placeholder builder context — W4-Z1 will populate per-FN config
/// snapshots (tax_groups + payment_methods + driver_tax_mapping +
/// integration_flags + previous_hash + z_number + local_number)
/// looked up from `secure.db` at supervisor dispatch time.
///
/// Carried by reference into the builder so the trait stays object-
/// safe (no generics).  Pilot fields are intentionally minimal
/// — extension is purely additive in W4-Z1.
#[derive(Debug, Clone, Default)]
pub struct BuilderContext {
    pub fiscal_number: String,
    pub tax_number: String,
    pub previous_hash: Option<String>,
    pub z_number: u32,
    pub local_number: u32,
    pub ts_str: String,
}

/// DPS endpoint target — host / port / TLS settings.  Outgress-
/// agnostic so the trait can be implemented by both gRPC and REST.
#[derive(Debug, Clone)]
pub struct TargetEndpoint {
    /// e.g. "cabinet.tax.gov.ua" (test) or "prro.tax.gov.ua" (prod).
    pub host: String,
    /// e.g. 9443 (gRPC test) or 443 (REST prod).
    pub port: u16,
    /// e.g. "/api/v2" for REST; gRPC ignores.
    pub path: String,
}

/// Signing context — placeholder for W4-Z2.  Will carry the cashier
/// cert + private key handle from the jkurwa sidecar.
#[derive(Debug, Clone, Default)]
pub struct SignContext {
    pub cashier_id_hint: Option<String>,
}

// ─── Outcomes + errors ────────────────────────────────────────────

/// Canonical DPS outcome, normalised across outgress variants.
/// Supervisor dispatch consumes this enum outgress-agnostically;
/// per-outgress parsers map raw protobuf / JSON / XML responses
/// onto this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpsOutcome {
    /// Receipt accepted; fiscal_number assigned + payload hash echoed.
    /// FSCO: KVT1 with fiscal_number; KVT2 not yet arrived.
    /// EVPZ: single-phase ACK with assigned fiscal_number.
    Acked {
        fiscal_number: String,
        local_number: u32,
        kvt1_payload_hash: Option<[u8; 32]>,
    },

    /// FSCO-only: KVT2 confirmation arrived (delayed-final ack).
    /// EVPZ: never emitted from EvpzResponseParser.
    Kvt2Confirmed {
        fiscal_number: String,
        kvt2_payload_hash: [u8; 32],
    },

    /// Receipt rejected by DPS with typed error code.  Distinct from
    /// `RetryablePending` (which is transport-level failure).
    Rejected {
        code: String,
        message: Option<String>,
    },

    /// Transport-level failure surfaced as retryable.
    RetryablePending { reason: String },
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("builder unimplemented for {0:?}")]
    Unimplemented(String),
    #[error("build error: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum SignError {
    #[error("sign unimplemented for {0:?}")]
    Unimplemented(String),
    #[error("sign error: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport unimplemented for {0:?}")]
    Unimplemented(String),
    #[error("transport error: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("parse unimplemented for {0:?}")]
    Unimplemented(String),
    #[error("parse error: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum OutgressError {
    // Audit Round-3 (2026-05-27): `ProfileNotImplemented` variant
    // removed — Round-2 unwound the hardcoded EvpzDps early-exit, so
    // the variant became dead code.  EVPZ profile's `Unimplemented`
    // surfaces via the trait chain as `OutgressError::Build`
    // (or `Sign`/`Transport`/`Parse` for the corresponding layer)
    // with a per-error-class message body identifying which trait
    // has no shipped impl yet.
    #[error("build: {0}")]
    Build(#[from] BuildError),
    #[error("sign: {0}")]
    Sign(#[from] SignError),
    #[error("transport: {0}")]
    Transport(#[from] TransportError),
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
}

// ─── Traits ───────────────────────────────────────────────────────

pub trait DpsXmlBuilder: Send + Sync {
    /// Build the wire-bytes representation of a fiscal command per the
    /// outgress's expected XML/JSON shape.  Output is the *unsigned*
    /// payload — sign envelope wraps separately.
    fn build_check(
        &self,
        cmd: &CanonicalFiscalCommand,
        ctx: &BuilderContext,
    ) -> Result<Vec<u8>, BuildError>;
}

pub trait SignEnvelope: Send + Sync {
    /// Wrap unsigned payload bytes in the outgress's signed envelope
    /// (CMS for FSCO, CMS-over-CHECK for EVPZ).  Output is the
    /// transport-ready signed blob.
    fn wrap(&self, content: &[u8], sign_ctx: &SignContext) -> Result<Vec<u8>, SignError>;
}

#[async_trait]
pub trait DpsTransport: Send + Sync {
    /// Submit signed payload to the DPS endpoint and return the raw
    /// response bytes.  Parser maps to canonical [`DpsOutcome`].
    async fn submit(
        &self,
        payload: &[u8],
        target: &TargetEndpoint,
    ) -> Result<Vec<u8>, TransportError>;
}

pub trait DpsResponseParser: Send + Sync {
    /// Decode raw DPS response into a canonical [`DpsOutcome`].
    /// Handles outgress-specific shape (FSCO KVT1/KVT2 split, EVPZ
    /// single-ACK) and normalises to the shared enum.
    fn parse_response(
        &self,
        raw: &[u8],
        cmd: &CanonicalFiscalCommand,
    ) -> Result<DpsOutcome, ParseError>;
}

// ─── FSCO/ZZD pilot impls (skeleton; bodies land in W4-Z1..Z3) ────

// Audit Round-2 (2026-05-27): pilot FSCO_ZZD is the DEFAULT outgress
// profile.  If a supervisor task accidentally dispatches before
// W4-Z1..Z3 fill in the bodies, `todo!()` would PANIC the tokio
// worker thread — crashing the whole supervisor process.  Return
// typed `Unimplemented` errors instead so accidental invocations
// safely surface as `OutgressError::Build/Sign/Transport/Parse`
// and the worker can re-route to offline-backlog or refuse the
// command without aborting the runtime.

pub struct FscoXmlBuilder;
impl DpsXmlBuilder for FscoXmlBuilder {
    fn build_check(
        &self,
        _cmd: &CanonicalFiscalCommand,
        _ctx: &BuilderContext,
    ) -> Result<Vec<u8>, BuildError> {
        // W4-Z1 will land the real FSCO `<RQ><DAT><C>...` builder
        // mirroring `src/prro_gateway/serializers/dps_xml.py`.
        Err(BuildError::Unimplemented(
            "FSCO XML builder body lands in W4-Z1".to_string(),
        ))
    }
}

pub struct CmsOverDatEnvelope;
impl SignEnvelope for CmsOverDatEnvelope {
    fn wrap(&self, _content: &[u8], _sign_ctx: &SignContext) -> Result<Vec<u8>, SignError> {
        // W4-Z2 — CMS sign over the `<DAT>` content via jkurwa sidecar.
        Err(SignError::Unimplemented(
            "CMS-over-DAT envelope body lands in W4-Z2".to_string(),
        ))
    }
}

pub struct GrpcSendChkV2Transport;
#[async_trait]
impl DpsTransport for GrpcSendChkV2Transport {
    async fn submit(
        &self,
        _payload: &[u8],
        _target: &TargetEndpoint,
    ) -> Result<Vec<u8>, TransportError> {
        // W4-Z3 — gRPC `sendChkV2` to cabinet.tax.gov.ua:9443 / prro.tax.gov.ua:443.
        Err(TransportError::Unimplemented(
            "gRPC sendChkV2 transport body lands in W4-Z3".to_string(),
        ))
    }
}

pub struct FscoResponseParser;
impl DpsResponseParser for FscoResponseParser {
    fn parse_response(
        &self,
        _raw: &[u8],
        _cmd: &CanonicalFiscalCommand,
    ) -> Result<DpsOutcome, ParseError> {
        // W4-Z3 — decode KVT1/KVT2 protobuf per `sendChkV2.proto`.
        Err(ParseError::Unimplemented(
            "FSCO response parser (KVT1/KVT2) body lands in W4-Z3".to_string(),
        ))
    }
}

// ─── EVPZ/DPS post-pilot placeholders ─────────────────────────────

pub struct EvpzXmlBuilder;
impl DpsXmlBuilder for EvpzXmlBuilder {
    fn build_check(
        &self,
        _cmd: &CanonicalFiscalCommand,
        _ctx: &BuilderContext,
    ) -> Result<Vec<u8>, BuildError> {
        Err(BuildError::Unimplemented(
            "EVPZ XML builder is post-pilot (W4-Y1)".to_string(),
        ))
    }
}

pub struct CmsOverCheckSignedFileEnvelope;
impl SignEnvelope for CmsOverCheckSignedFileEnvelope {
    fn wrap(&self, _content: &[u8], _sign_ctx: &SignContext) -> Result<Vec<u8>, SignError> {
        Err(SignError::Unimplemented(
            "EVPZ CMS-over-CHECK signed-file envelope is post-pilot (W4-Y2)".to_string(),
        ))
    }
}

pub struct HttpsRestTransport;
#[async_trait]
impl DpsTransport for HttpsRestTransport {
    async fn submit(
        &self,
        _payload: &[u8],
        _target: &TargetEndpoint,
    ) -> Result<Vec<u8>, TransportError> {
        Err(TransportError::Unimplemented(
            "EVPZ HTTPS REST transport is post-pilot (W4-Y3)".to_string(),
        ))
    }
}

pub struct EvpzResponseParser;
impl DpsResponseParser for EvpzResponseParser {
    fn parse_response(
        &self,
        _raw: &[u8],
        _cmd: &CanonicalFiscalCommand,
    ) -> Result<DpsOutcome, ParseError> {
        Err(ParseError::Unimplemented(
            "EVPZ response parser is post-pilot (W4-Y2)".to_string(),
        ))
    }
}

// ─── Quartet accessor + router ────────────────────────────────────

pub type Quartet = (
    Arc<dyn DpsXmlBuilder>,
    Arc<dyn SignEnvelope>,
    Arc<dyn DpsTransport>,
    Arc<dyn DpsResponseParser>,
);

/// Per-outgress quartet accessor.  Returns the trio + parser for
/// the chosen profile.  EVPZ_DPS in pilot returns the placeholder
/// quartet — every method on it returns `Unimplemented`, which the
/// supervisor surfaces as `OutgressError::ProfileNotImplemented`.
///
/// FSCO_ZZD returns pilot skeletons (`todo!()`) — supervisor SHOULD
/// NOT actually hit `submit` / etc. until W4-Z3 lands real bodies.
/// Calling stage_send before that triggers `todo!()` panic — caught
/// in CI as a "haven't built FSCO transport yet" reminder.
pub fn quartet_for(profile: OutgressProfile) -> Quartet {
    match profile {
        OutgressProfile::FscoZzd => (
            Arc::new(FscoXmlBuilder),
            Arc::new(CmsOverDatEnvelope),
            Arc::new(GrpcSendChkV2Transport),
            Arc::new(FscoResponseParser),
        ),
        OutgressProfile::EvpzDps => (
            Arc::new(EvpzXmlBuilder),
            Arc::new(CmsOverCheckSignedFileEnvelope),
            Arc::new(HttpsRestTransport),
            Arc::new(EvpzResponseParser),
        ),
    }
}

/// Supervisor-side dispatch — per-FN outgress profile drives the
/// quartet selection.  Pilot only ever sees FSCO_ZZD return Ok
/// (when bodies land); EVPZ_DPS returns
/// `OutgressError::ProfileNotImplemented` typed.
pub async fn dispatch_to_outgress(
    profile: OutgressProfile,
    cmd: &CanonicalFiscalCommand,
    ctx: &BuilderContext,
    target: &TargetEndpoint,
    sign_ctx: &SignContext,
) -> Result<DpsOutcome, OutgressError> {
    // Audit Round-2 (2026-05-27): no hardcoded `if profile ==
    // EvpzDps` early-exit.  Open-closed principle — adding a third
    // profile (e.g. Tax-1 / ОФД-aggregator) must not require
    // modifying this router function.  Each impl's native
    // `Err(...::Unimplemented)` flows through the `?` chain and
    // converts to `OutgressError::{Build,Sign,Transport,Parse}`
    // via the `#[from]` From impls.  Per-profile detail is in
    // the error message body.
    let (builder, envelope, transport, parser) = quartet_for(profile);

    let payload = builder.build_check(cmd, ctx)?;
    let signed = envelope.wrap(&payload, sign_ctx)?;
    let raw_response = transport.submit(&signed, target).await?;
    parser
        .parse_response(&raw_response, cmd)
        .map_err(OutgressError::from)
}
