//! `prro::crypto` — in-process wrapper over `prro_crypto`.
//!
//! Architectural boundaries:
//! - `errors` carries `CryptoError` + the per-method reason enums
//!   (`SealKind` / `SignKind` / `DecryptKind` / `FetchKind` / `VerifyKind`).
//! - `session` carries `SealedMaterial` + `SigningSession` + the
//!   `unseal_jks` boundary helper.  The plaintext private scalar
//!   never leaves the `session` module's `pub(crate)` accessor.
//! - `provider` carries the `CryptoProvider` async trait + request/
//!   response types.
//! - `in_process` is the default implementation backed by `prro_crypto`,
//!   wrapping blocking C-FFI calls in `tokio::task::spawn_blocking`.
//!
//! ADR-M2-6: NO public function or trait method here takes a
//! `SqlitePool` / `SqliteConnection` / `Pool<Sqlite>` /
//! `Transaction<…>`.  W5's static check enforces this at build time.

pub mod errors;
pub mod in_process;
pub mod provider;
pub mod session;

pub use errors::{CryptoError, DecryptKind, FetchKind, SealKind, SignKind, VerifyKind};
pub use in_process::InProcessProvider;
pub use provider::{CertDer, CryptoProvider, DstuVerifyResult, SignCmsRequest, SignedCmsBytes};
pub use session::{unseal_jks, SealedMaterial, SigningSession};
