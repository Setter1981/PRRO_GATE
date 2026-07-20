//! PR-C4 — `OfflineCodeReplenishService`: manual-trigger T=112 replenish.
//!
//! ## Flow (order is load-bearing per architect ruling)
//!
//! 1. **Acquire** the per-FN single-writer lease via `App::acquire_fn_gate` —
//!    the SAME gate the write-path holds during every `fiscalize` call.
//!    Replenish advances the chain seed and MUST serialise against concurrent
//!    SELLs (a seed race = chain fork).
//! 2. Read `node_state.last_known_unsigned_xml_sha256` → `mac_hex` (empty
//!    string if None / genesis).
//! 3. Build the T=112 request XML via `t112::build_t112_request`.
//! 4. **Sign** the XML bytes (ATTACHED CAdES-BES) via `SigningContext` — the
//!    same provider seam the write-path's `stage_sign` uses; injected in the
//!    service constructor, NOT constructed inline.
//! 5. **Outside any DB transaction**: call `channel.ask_offline_codes`.
//! 6. On DPS success, in **ONE `with_immediate` envelope**:
//!    `insert_dps_codes_tx` + `update_last_known_xml_sha_tx` to sha256 of the
//!    request XML bytes.
//!    (C-i live proof 2026-07-07: DPS chain tip econtent == our request XML
//!    verbatim; sha256 confirmed against the cabinet.)
//! 7. On DPS server reject: NO persist, NO seed advance, typed error surfaced.
//!    On ambiguous transport error/timeout: NO retry, NO persist — T=112 is
//!    non-idempotent server-side (each call issues fresh codes AND advances
//!    DPS's chain); a lost response = harmless server-side range leak; chain
//!    self-heals via mac-recovery on the next doc.
//!
//! ## Invariant preservation
//!
//! - **#1** (no wire/crypto inside tx): gate acquired, MAC read, XML built,
//!   sign called, DPS called — all BEFORE `with_immediate`; only the persist
//!   + seed advance land inside the envelope.
//! - **#2** (single-writer per FN): `acquire_fn_gate` serialises against the
//!   write-path and the online-convergence tick for the same FN.
//! - **#4** (idempotency): `insert_dps_codes_tx` uses `INSERT OR IGNORE`
//!   against the partial unique index; duplicates are silently deduped.
//!   No-retry on transport error is intentional (non-idempotent DPS-side).
//! - **#5 / INV-11** (SIZE bounds): enforced by `t112::build_t112_request`
//!   (0 → error; >2000 → clamped to 2000 per WebCheck cap).
//! - **#8** (seed advance via existing primitive only): uses the same
//!   `node_state::update_last_known_xml_sha_tx` that `stage_send` and
//!   `stage_offline_ack` use; no new state-machine transitions.

use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::app::App;
use crate::crypto::errors::CryptoError;
use crate::crypto::provider::SignCmsRequest;
use crate::db::repositories::{delivery_reservation, node_state, offline_sessions};
use crate::db::tx::with_immediate;
use crate::services::write_path::stage_sign::SigningContext;
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::{CheckEnvelope, DpsCheckType};
use crate::transports::dps::error::DpsError;
use crate::transports::dps::t112;
use crate::transports::dps::t112::T112Error;

// ── Public types ────────────────────────────────────────────────────────────

/// Outcome of a successful [`OfflineCodeReplenishService::replenish`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplenishSummary {
    /// Number of codes returned by DPS in this call.
    pub codes_received: usize,
    /// Number of codes actually written to `offline_codes` (new rows).
    pub inserted: u64,
    /// Number of codes silently skipped because `(fiscal_number, dps_code)`
    /// already existed (partial UNIQUE `ux_offline_codes_fn_dps_code`).
    pub deduped: u64,
    /// Lowercase hex of `sha256(request_xml_bytes)` — the new chain seed
    /// stored in `node_state.last_known_unsigned_xml_sha256`.
    pub new_seed_hex: String,
    /// The exact T=112 request XML that was signed and sent to DPS. Exposed
    /// so callers/tests can verify the chain-seed advance equals
    /// `sha256(request_xml)` (the C-i invariant) and audit the wire payload.
    /// Contains no secrets — FN/TN are public fiscal identifiers.
    pub request_xml: String,
}

/// Typed error from [`OfflineCodeReplenishService::replenish`].
#[derive(Debug)]
pub enum ReplenishError {
    /// T=112 request builder rejected the inputs (SIZE == 0 is the only
    /// current trigger; SIZE > 2000 is silently clamped by the builder).
    T112Build(T112Error),
    /// CAdES-BES signing of the XML bytes failed.
    Sign(CryptoError),
    /// DPS returned a server-level error (`status < 0`).  NO codes were
    /// persisted; seed UNCHANGED.
    DpsServer { code: i32, message: String },
    /// Ambiguous wire/transport error (timeout, connection refused, etc.).
    /// NO codes persisted; seed UNCHANGED; NO retry (T=112 is
    /// non-idempotent server-side — see module docs).
    DpsTransport(String),
    /// `node_state` row missing for the FN — structural breach (App boot
    /// must upsert the row before replenish is called).
    NodeStateMissing,
    /// `update_last_known_xml_sha_tx` returned false (row disappeared
    /// between the read and the persist — should not happen under the gate).
    SeedUpdateMissing,
    /// SQLite error from a repository call.
    Db(sqlx::Error),
    /// Any other non-recoverable internal error (e.g., `with_immediate`
    /// envelope failure).
    Internal(anyhow::Error),
}

impl std::fmt::Display for ReplenishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplenishError::T112Build(e) => write!(f, "T=112 build error: {e}"),
            ReplenishError::Sign(e) => write!(f, "CMS sign error: {e}"),
            ReplenishError::DpsServer { code, message } => {
                write!(f, "DPS server error {code}: {message}")
            }
            ReplenishError::DpsTransport(msg) => write!(f, "DPS transport error: {msg}"),
            ReplenishError::NodeStateMissing => write!(f, "node_state row missing for FN"),
            ReplenishError::SeedUpdateMissing => {
                write!(
                    f,
                    "seed update returned false (node_state row missing during persist)"
                )
            }
            ReplenishError::Db(e) => write!(f, "DB error: {e}"),
            ReplenishError::Internal(e) => write!(f, "internal error: {e}"),
        }
    }
}

impl std::error::Error for ReplenishError {}

// ── Service ─────────────────────────────────────────────────────────────────

/// Manual-trigger T=112 offline code replenish service.
///
/// No production callers in this slice (C4); the CLI trigger lands in C5.
/// Construct once per `App` lifetime; `replenish` is called on-demand.
pub struct OfflineCodeReplenishService {
    /// Gate source + DB pool.  `acquire_fn_gate` serialises against the
    /// write-path for the same FN (frozen invariant #2).
    app: App,
    /// DPS transport channel.  Must be `Arc` so the service is `Clone`able
    /// and the caller can share one channel across multiple FNs.
    dps: Arc<dyn DpsChannel>,
    /// Crypto provider + session for ATTACHED CAdES-BES signing of the T=112
    /// XML bytes.  Same seam as `stage_sign::run` (injected, not constructed
    /// inline).
    sign_ctx: Arc<SigningContext>,
}

impl OfflineCodeReplenishService {
    /// Construct the service.
    ///
    /// `app` provides the per-FN write gate AND the DB pool.
    /// `dps` is the production (or test-scripted) `DpsChannel`.
    /// `sign_ctx` is the signing context — the same one wired into the
    /// write-path's `stage_sign`.
    pub fn new(app: App, dps: Arc<dyn DpsChannel>, sign_ctx: Arc<SigningContext>) -> Self {
        Self { app, dps, sign_ctx }
    }

    /// Execute one T=112 replenish cycle for `fiscal_number`.
    ///
    /// See module docs for the full 7-step flow.
    ///
    /// # Arguments
    /// * `fiscal_number` — FN string (e.g. `"4000162280"`)
    /// * `tn` — tax number string (e.g. `"13667753"`)
    /// * `di` — document index (1-based; usually 1 for the first batch)
    /// * `size` — codes to request (0 → `T112Build` error; >2000 → clamped)
    pub async fn replenish(
        &self,
        fiscal_number: &str,
        tn: &str,
        di: u32,
        size: u32,
    ) -> Result<ReplenishSummary, ReplenishError> {
        // ── Step 1: acquire per-FN single-writer lease ───────────────────
        // The gate is an in-process `tokio::sync::Mutex` held OUTSIDE every
        // `with_immediate` envelope (invariant #1 / fn_gate module contract).
        let _gate = self.app.acquire_fn_gate(fiscal_number).await;

        // ── Step 2: read chain tip ────────────────────────────────────────
        let pool = self.app.db();
        let ns = node_state::get(pool, fiscal_number)
            .await
            .map_err(ReplenishError::Db)?
            .ok_or(ReplenishError::NodeStateMissing)?;
        let mac_hex: String = match ns.last_known_unsigned_xml_sha256 {
            None => String::new(),
            Some(arr) => arr.iter().map(|b| format!("{b:02x}")).collect(),
        };

        // ── Step 3: build T=112 request XML ──────────────────────────────
        let request = t112::build_t112_request(
            fiscal_number,
            tn,
            di,
            size,
            t112::kyiv_comp_date_now(),
            &mac_hex,
        )
        .map_err(ReplenishError::T112Build)?;

        // ── Step 4: sign XML bytes OUTSIDE any DB transaction ────────────
        // ATTACHED CAdES-BES, same profile as receipt signing in stage_sign.
        let signed = self
            .sign_ctx
            .provider
            .sign_cms_detached(SignCmsRequest {
                session: &self.sign_ctx.session,
                canonical_xml: request.xml.as_bytes(),
                profile: self.sign_ctx.profile,
            })
            .await
            .map_err(ReplenishError::Sign)?;

        // ── Step 5: call DPS OUTSIDE any DB transaction ───────────────────
        // GrpcDpsChannel::ask_offline_codes asserts !in_with_immediate at
        // entry (C1 invariant guard); test stubs do not assert but the
        // call-log pin (test g) verifies ordering.
        let envelope = CheckEnvelope {
            rro_fn: fiscal_number.to_string(),
            date_time: request.comp_date,
            check_sign: signed.0,
            local_number: di as i32,
            check_type: DpsCheckType::ServiceChk,
            id_offline: String::new(),
            id_cancel: String::new(),
        };

        let codes_resp = match self.dps.ask_offline_codes(envelope).await {
            Ok(r) => r,
            Err(DpsError::Server { code, message }) => {
                // Step 7 (server reject): NO persist, NO seed advance.
                return Err(ReplenishError::DpsServer { code, message });
            }
            Err(e) => {
                // Step 7 (transport error/timeout): ambiguous.
                // T=112 is non-idempotent server-side: each call issues fresh
                // codes AND advances DPS's chain. A lost response = harmless
                // server-side range leak; chain self-heals via mac-recovery.
                // NO retry, NO persist.
                return Err(ReplenishError::DpsTransport(e.to_string()));
            }
        };

        // ── Step 6: compute new chain seed ────────────────────────────────
        // C-i live proof (2026-07-07): DPS chain tip econtent == our request
        // XML verbatim; sha256 confirmed against the DPS cabinet.
        let new_seed: [u8; 32] = Sha256::digest(request.xml.as_bytes()).into();
        let new_seed_hex: String = new_seed.iter().map(|b| format!("{b:02x}")).collect();
        let codes = codes_resp.codes;
        let codes_received = codes.len();

        // ── Step 6 (cont.): persist codes + advance seed in ONE envelope ──
        let fn_id = fiscal_number.to_string();
        let inserted_summary = with_immediate(pool, move |tx| {
            Box::pin(async move {
                // CS-3 S7-2 — fail-closed FN fence: refuse the chain-seed advance while a
                // delivery reservation is in-flight (INACTIVE today; guards the tip against
                // a fork at cutover). This surface has NO equality gate → highest fork risk.
                if delivery_reservation::fn_fence_active_tx(tx, &fn_id)
                    .await
                    .map_err(anyhow::Error::from)?
                {
                    return Err(anyhow::anyhow!(
                        "replenish refused: FN {fn_id} has an active delivery reservation (S7-2 fence)"
                    ));
                }
                // insert_dps_codes_tx: INSERT OR IGNORE (dedupe-safe / INV-4).
                let ins = offline_sessions::insert_dps_codes_tx(tx, &fn_id, &codes)
                    .await
                    .map_err(anyhow::Error::from)?;

                // Advance the chain seed to sha256(request_xml).
                let updated = node_state::update_last_known_xml_sha_tx(tx, &fn_id, &new_seed)
                    .await
                    .map_err(anyhow::Error::from)?;

                if !updated {
                    // FN row disappeared between the read (step 2) and now —
                    // impossible under the gate + upsert contract, but fail-closed.
                    return Err(anyhow::anyhow!(
                        "replenish seed update returned false: node_state row missing for {}",
                        fn_id
                    ));
                }

                Ok(ins)
            })
        })
        .await
        .map_err(ReplenishError::Internal)?;

        Ok(ReplenishSummary {
            codes_received,
            inserted: inserted_summary.inserted,
            deduped: inserted_summary.deduped,
            new_seed_hex,
            request_xml: request.xml,
        })
    }
}
