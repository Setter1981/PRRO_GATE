//! DPS gRPC transport.
//!
//! C1 landed the tonic-generated proto seam (`gen`).  C2 (this commit)
//! lands the typed `DpsChannel` trait, the typed DTO wrappers, the
//! `DpsError` surface, and the `GrpcDpsChannel` substrate.  C3 wires
//! the real RPC method bodies; C4 lands the native tonic mock server
//! + integration tests.
//!
//! ADR-M2-6: NO public function or trait method here takes a
//! `SqlitePool` / `SqliteConnection` / `Pool<Sqlite>` / `Transaction<…>`.
//! W5 will static-assert this at build time.
//!
//! Generated `gen` is `#[doc(hidden)] pub mod` — technically reachable
//! from outside the crate (so C4's integration-test mock server can
//! call `tonic::include_proto!`-derived server-side stubs without
//! re-running the proto codegen) but flagged as "test/unstable
//! support, NOT public contract".  Production callers consume the
//! typed surface (`DpsChannel`, the DTOs, `DpsError`); raw prost /
//! tonic shapes do NOT belong in any production code path.

pub mod channel;
pub(crate) mod digest_framing;
pub mod dto;
pub mod error;
#[doc(hidden)]
pub mod gen;
pub mod grpc;
pub mod raw_reply;
pub mod t112;

pub use channel::DpsChannel;
pub use dto::{
    CheckAck, CheckEnvelope, CheckSignBlob, DpsCheckType, DpsOperator, RroInfo, StatusSnapshot,
};
pub use error::{AuthorizationKind, DpsError};
pub use grpc::GrpcDpsChannel;
// CS-3 3.2 PR2 pin3: the shadow-path READ surface (spec §4.2). The construction
// seal is the private `RawSendReplyInner` field + `pub(in crate::transports::dps)`
// builders — making the module/read-types `pub` lets the engine (and tests) NAME and
// MATCH the observation without gaining the ability to mint one.
pub use raw_reply::{RawSendObservation, RawSendReply, RawSendReplyKind, WireDiagnostics};
