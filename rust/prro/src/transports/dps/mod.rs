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
//! Generated `gen` stays `pub mod` for now so the typed wrappers in
//! `dto` can construct and read the prost types crate-internally; the
//! public trait surface (`DpsChannel`, the DTOs, `DpsError`) does NOT
//! leak any prost / tonic shape across crate boundaries.

pub mod channel;
pub mod dto;
pub mod error;
pub mod gen;
pub mod grpc;

pub use channel::DpsChannel;
pub use dto::{
    CheckAck, CheckEnvelope, CheckSignBlob, DpsCheckType, DpsOperator, RroInfo, StatusSnapshot,
};
pub use error::DpsError;
pub use grpc::GrpcDpsChannel;
