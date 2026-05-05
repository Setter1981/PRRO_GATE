//! DPS gRPC transport.
//!
//! C1 intentionally exposes only the generated proto module seam. The typed
//! `DpsChannel` facade and `GrpcDpsChannel` implementation land in the next
//! W3 commits after the generated contract is reviewable and compiling.

pub mod gen;
