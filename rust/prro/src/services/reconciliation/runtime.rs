//! W11 runtime composition surface for boot reconciliation.
//!
//! [`ReconciliationRuntime`] carries the dependencies that ctx-needy
//! dispatch branches (PREPARED / SIGNED / SENT / ERROR_RETRYABLE)
//! consume on boot.  Plumbed in W11 PR-1a; consumed by PR-2.
//!
//! ## M3a hardening pass 1 (per-FN resolver)
//!
//! M3a-original shape (`ReconciliationRuntime { dps, signing_ctx,
//! fn_sign }`) was a singleton bundle: one `(dps, signing_ctx,
//! fn_sign)` triple shared across every FN visited by
//! `App::reconcile_pending_with`.  Risk: in any deployment hosting
//! more than one operator-bound FN, recovery would issue
//! `DpsChannel::last_chk` and `stage_sign::run` for FN-A under
//! FN-B's operator identity — either failing authentication or,
//! worse, authenticating as the wrong operator.
//!
//! Hardening reshape: [`ReconciliationRuntime`] becomes a resolver
//! around per-FN [`RuntimeView`]s.  The caller decides whether the
//! same view applies to every FN ([`single_fn`]) OR varies per FN
//! ([`with_resolver`]).  `App::reconcile_pending_with` resolves the
//! view inside the per-FN loop body, BEFORE dispatching to
//! `run_boot_reconciliation`.
//!
//! **Behaviour on `None`**: if the resolver returns `None` for an
//! FN that has ctx-needy pending docs, `run_boot_reconciliation`
//! falls through to the legacy ctx-free path (emits
//! `BOOT_DISPATCH_DEFERRED` per doc).  Recovery NEVER substitutes
//! foreign identity — `None` resolves to "no recovery deps available
//! for this FN this tick", not to "borrow whatever's around".
//!
//! Per ADR-M3-A10 (`docs/superpowers/specs/2026-05-12-adr-m3-a10-global-single-writer.md`):
//! under M3a's global-single-writer invariant, the runtime is held
//! for the duration of one `App::reconcile_pending_with` call; the
//! invariant remains "at most one writer mutates state for a given
//! `fiscal_number` at any moment", enforced by single tokio worker +
//! `with_immediate` BEGIN IMMEDIATE + per-row CAS.  Concurrent
//! dispatchers are not supported in M3a.
//!
//! [`single_fn`]: ReconciliationRuntime::single_fn
//! [`with_resolver`]: ReconciliationRuntime::with_resolver

use crate::services::write_path::stage_sign::SigningContext;
use crate::transports::dps::dto::CheckSignBlob;
use crate::transports::dps::DpsChannel;

/// Per-FN snapshot of runtime dependencies.  Plain references (which
/// are `Copy`) so the resolver can safely return a [`RuntimeView`]
/// by value and the caller can thread `Option<&RuntimeView>`
/// through the dispatch tree.
///
/// All three fields are operator-bound by construction:
///   - `dps` — the wire channel typically wraps a per-operator
///     transport profile (TLS client cert, keystore).
///   - `signing_ctx` — the operator's `SigningSession` + crypto
///     provider; carries the operator's keypair / cert chain.
///   - `fn_sign` — the operator DPS identity blob (opaque bytes
///     the live worker holds for `DpsChannel::last_chk` /
///     `status_rro` / `info_rro` calls).
///
/// In a single-operator-many-FNs pilot deployment the same view is
/// returned by the resolver for every `fn_id`; in multi-operator
/// deployments the resolver looks up the operator binding per FN.
pub struct RuntimeView<'a> {
    pub dps: &'a dyn DpsChannel,
    pub signing_ctx: &'a SigningContext,
    pub fn_sign: &'a CheckSignBlob,
}

// `RuntimeView<'a>` is a trio of shared references — all `Copy` by
// definition (`&T` is `Copy` regardless of `T: ?Sized`).  Manual
// impls because `derive(Clone)` on `&dyn Trait` fields can be
// flaky depending on the trait's auto-trait bounds.  Copy lets the
// resolver return the view by value cheaply.
impl<'a> Clone for RuntimeView<'a> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'a> Copy for RuntimeView<'a> {}

/// Boxed `Fn(fn_id) -> Option<RuntimeView>` resolver.  Type alias
/// keeps the [`ReconciliationRuntime::PerFn`] variant readable
/// (clippy::type_complexity threshold).
type RuntimeResolver<'a> = Box<dyn Fn(&str) -> Option<RuntimeView<'a>> + 'a>;

/// Reconciliation runtime composition surface.  The caller
/// constructs one and passes it to
/// [`crate::App::reconcile_pending_with`]; internally
/// `App::reconcile_pending_inner` invokes [`resolve`] per FN inside
/// the iteration loop.
///
/// [`resolve`]: ReconciliationRuntime::resolve
pub enum ReconciliationRuntime<'a> {
    /// Single-FN ergonomics: the same view is returned for every
    /// `fn_id`.  Used by the test fixtures and by single-operator
    /// pilot deployments.  Construct via
    /// [`ReconciliationRuntime::single_fn`].
    SingleFn(RuntimeView<'a>),
    /// Per-FN resolver: caller-supplied closure looks up the
    /// operator identity bundle for a given `fn_id`.  Returns
    /// `None` when no recovery deps are available for that FN
    /// (ctx-needy dispatch will then emit `BOOT_DISPATCH_DEFERRED`
    /// for that FN's pending docs — recovery NEVER falls back to
    /// foreign identity).  Construct via
    /// [`ReconciliationRuntime::with_resolver`].
    PerFn(RuntimeResolver<'a>),
}

impl<'a> ReconciliationRuntime<'a> {
    /// Build a single-FN-ergonomic runtime that returns `view` for
    /// every FN resolved.  Use in single-operator-many-FN pilot
    /// deployments OR in test fixtures that exercise only one FN.
    pub fn single_fn(view: RuntimeView<'a>) -> Self {
        Self::SingleFn(view)
    }

    /// Build a per-FN runtime with a closure that maps `fn_id` to
    /// `Option<RuntimeView>`.  Use in multi-operator deployments
    /// where each FN may have a distinct operator identity binding.
    pub fn with_resolver(resolver: impl Fn(&str) -> Option<RuntimeView<'a>> + 'a) -> Self {
        Self::PerFn(Box::new(resolver))
    }

    /// Resolve the runtime view for `fn_id`.  Returns `Some(view)`
    /// for the [`SingleFn`] variant verbatim; delegates to the
    /// closure for the [`PerFn`] variant.
    ///
    /// [`SingleFn`]: ReconciliationRuntime::SingleFn
    /// [`PerFn`]: ReconciliationRuntime::PerFn
    pub fn resolve(&self, fn_id: &str) -> Option<RuntimeView<'a>> {
        match self {
            Self::SingleFn(view) => Some(*view),
            Self::PerFn(resolver) => resolver(fn_id),
        }
    }
}
