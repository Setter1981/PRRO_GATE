#![cfg(feature = "live-dps")]
//! **W4-Z3 — Live DPS extended-XML fiscal-cycle smoke (2026-05-28)**.
//!
//! These tests drive the REAL Rust write-path against the REAL DPS test
//! server (`cabinet.tax.gov.ua:9443`) with a REAL signing key and the
//! NATIVE `prro_crypto` in-process signer (NO jkurwa sidecar — that
//! architecture is dead).  They exist to prove the native fiscal cycle
//! (SHIFT_OPEN → extended SELL → Z_REPORT) is ACCEPTED by live DPS, not
//! just by our mock + byte-goldens.
//!
//! ## Triple gate (this file never runs by accident)
//!
//! 1. **Cargo feature** — the whole file is `#![cfg(feature = "live-dps")]`,
//!    so it does not even COMPILE without `--features live-dps`.
//! 2. **`#[ignore]`** — every test is ignored; opt-in needs `-- --ignored`.
//! 3. **`PRRO_LIVE_DPS=1` env kill-switch** — every test self-skips (prints
//!    a SKIP line and returns OK) unless this is set, so a stray
//!    `--ignored` run still cannot touch live DPS.
//!
//! ```bash
//! PRRO_LIVE_DPS=1 \
//! PRRO_LIVE_DPS_JKS_PATH="/abs/path/key_13667753_13667753 (2).jks" \
//! PRRO_LIVE_DPS_JKS_PASS=... \
//!   cargo test -p prro --features live-dps \
//!     --test live_dps_extended_smoke -- --ignored --nocapture
//! ```
//!
//! ## Env contract
//!
//! | var | required | default | meaning |
//! |-----|----------|---------|---------|
//! | `PRRO_LIVE_DPS`          | yes (gate) | —                               | must equal `1` or every test self-skips |
//! | `PRRO_LIVE_DPS_HOST`     | no         | `https://cabinet.tax.gov.ua:9443` | DPS test endpoint (gRPC over TLS) |
//! | `PRRO_LIVE_DPS_FN`       | no         | `4000162280`                    | test fiscal number (`rro_fn`) |
//! | `PRRO_LIVE_DPS_JKS_PATH` | signing    | —                               | path to the JKS key container |
//! | `PRRO_LIVE_DPS_JKS_PASS` | signing    | —                               | JKS password (NEVER logged) |
//!
//! Signing key for FN `4000162280` is the JKS `key_13667753_…(2).jks`
//! (TN `13667753`, signer «ГАЛЬЧУН МИКОЛА ДМИТРОВИЧ»).  The key files are
//! gitignored — the operator mounts them locally and points
//! `PRRO_LIVE_DPS_JKS_PATH` at one.
//!
//! ## Caveats (per operator memory)
//!
//! 1. **DPS rate limit** (`project_dps_rate_limit`): the test server returns
//!    `status=-4` after too many errors, with a 5+ minute per-FN cooldown.
//!    Run sparsely and manually — NEVER in a loop and NEVER in CI.
//! 2. **Test-host allowlist (default-deny)**: the resolved endpoint's HOST is
//!    parsed and must be `cabinet.tax.gov.ua` (or a `*-cabinet`/`*.cabinet`
//!    test/dev cabinet); ANY other host — every production endpoint
//!    (`prro.tax.gov.ua`, legacy `prro2.tax.gov.ua`, `fs.tax.gov.ua`) and any
//!    lookalike — is refused, so the smoke can never fiscalize against prod.
//! 3. **Native signing** (`prro_crypto::cms`, DSTU 4145-2002 + GOST 34.311,
//!    CAdES-BES, ATTACHED) — no external sidecar.

use prro::db::models::enums::{FiscalMode, NodeMode, ShiftState};
use prro::db::open_pool;
use prro::db::repositories::{fiscal_number_config as fn_cfg, node_state};
use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckSignBlob, DpsCheckType};
use prro::transports::dps::error::DpsError;
use prro::transports::dps::grpc::GrpcDpsChannel;
use prro_crypto::cms::builder::{CmsBuildOptions, CmsSigner};
use prro_crypto::cms::profile::CmsProfile;
use prro_crypto::cms::signed_data::extract_econtent;
use prro_crypto::cms::signer::DstuInProcessSigner;
use prro_crypto::core::curve::Curve;
use prro_crypto::core::field::FieldEl;
use prro_crypto::interop::prro::containers::{extract_private_key, ExtractedKey};
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ── Piece 4 — production stage_sign drive of an extended SELL (native sign) ──
use async_trait::async_trait;
use prro::config::AppConfig;
use prro::crypto::in_process::InProcessProvider;
use prro::crypto::provider::CryptoProvider;
use prro::crypto::session::SigningSession;
use prro::db::models::ids::DocumentId;
use prro::db::repositories::signing_config_snapshots;
use prro::db::tx::with_immediate;
use prro::services::reconciliation::{ReconciliationRuntime, RuntimeView};
use prro::services::write_path::stage_sign::SigningContext;
use prro::services::write_path::tax_summary::{
    DriverNumberMapping, ResolvedTaxGroupBps, TaxResolutionSnapshot,
};
use prro::transports::dps::dto::{CheckAck, CheckEnvelope, RroInfo, StatusSnapshot};
use prro::App;
use sqlx::SqlitePool;
use std::sync::Arc;

// ── Smoke 9 — production write-path drive of the offline Pattern-C sequence ──
// Smoke 9 no longer HAND-SEEDS a PREPARED offline SELL (which bypassed
// `inline::run`'s `ensure_offline_session_begin` hoist → no DocType=9 BEGIN was
// minted → the drain backlog was just the bare SELL and never exercised the B10
// handshake).  Instead it drives the offline docs through the REAL production
// ingress → `production_write_path` (`InlineWritePath::fiscalize` → `inline::run`
// → `run_staged` → `ensure_offline_session_begin`), so the lazy BEGIN mints as
// the FIRST offline doc.  These imports mirror `tests/b10_offline_session_
// handshake.rs`'s `drive(...)` harness, but wire the LIVE operator key (via a
// custom `OperatorKeyLoader`) instead of the deterministic fixture signer.
use prro::db::models::enums::{DocState, DocType, Protocol};
use prro::db::models::ids::RequestId;
use prro::db::repositories::ingress_inbox::{
    self as inbox, InboxInsertOutcome, InboxRow, NewInboxEntry,
};
use prro::db::repositories::operators as ops_repo;
use prro::runtime::bindings::{BindingsRegistry, KeyLoadFailure, OperatorKeyLoader};
use prro::runtime::coding::Coding;
use prro::runtime::ingress::inline_binding::production_write_path;
use prro::runtime::ingress::seam::{FiscalOutcome, WritePathEntry};
use std::path::Path;

// ─── Env contract ──────────────────────────────────────────────────────

/// Hard env kill-switch: every test self-skips unless this equals `"1"`.
const ENV_GATE: &str = "PRRO_LIVE_DPS";
/// DPS endpoint override; default = test cabinet (gRPC over TLS).
const ENV_HOST: &str = "PRRO_LIVE_DPS_HOST";
/// Test fiscal number override.
const ENV_FN: &str = "PRRO_LIVE_DPS_FN";
/// Path to the JKS key container (required for any signed RPC — pieces 2+).
const ENV_JKS_PATH: &str = "PRRO_LIVE_DPS_JKS_PATH";
/// JKS password (NEVER logged).
const ENV_JKS_PASS: &str = "PRRO_LIVE_DPS_JKS_PASS";
/// Smoke-9 only: back-date the offline documents (DocType=9 BEGIN + the offline
/// SHIFT_OPEN + the offline SELL) by this many SECONDS, simulating a REAL
/// offline period.  Default `0` = docs dated "now" (unchanged behavior).  The
/// smoke drains near-instantly, so with `0` every offline doc carries the
/// current second, which DPS intermittently rejects as at/ahead of its clock
/// (`-8` on the drained BEGIN).  A real offline period dates docs minutes/hours
/// in the past — set e.g. `600` to date them 10 minutes back and see whether the
/// `-8` clears.  All three offline docs move together (their frozen `<TS>` and
/// the drain envelope `date_time` still agree — both derive from the same
/// back-dated `business_ts`).
const ENV_OFFLINE_BACKDATE_SEC: &str = "PRRO_LIVE_DPS_OFFLINE_BACKDATE_SEC";

const DEFAULT_HOST: &str = "https://cabinet.tax.gov.ua:9443";
const DEFAULT_FN: &str = "4000162280";

/// Allowlist marker: the live smoke is permitted ONLY against a DPS *test*
/// cabinet host (the default and any dev cabinet both contain this).  Any
/// other host — including EVERY production endpoint (`prro.tax.gov.ua`,
/// the legacy `prro2.tax.gov.ua`, `fs.tax.gov.ua`) — is refused, so the
/// smoke can never accidentally fiscalize against production.  Default-deny
/// is deliberate: a prod-host blocklist would miss variants like `prro2`.
const TEST_HOST_MARKER: &str = "cabinet.tax.gov.ua";

/// Per-call deadline.  15s is generous (typical DPS response ~1-3s);
/// covers slow TLS handshake + intermittent network during manual runs.
const SMOKE_TIMEOUT_SECS: u64 = 15;

fn resolve_host() -> String {
    std::env::var(ENV_HOST).unwrap_or_else(|_| DEFAULT_HOST.to_string())
}

fn resolve_fn() -> String {
    std::env::var(ENV_FN).unwrap_or_else(|_| DEFAULT_FN.to_string())
}

/// Returns `true` when the live env gate is armed (`PRRO_LIVE_DPS=1`).
fn live_enabled() -> bool {
    std::env::var(ENV_GATE).as_deref() == Ok("1")
}

/// Self-skip guard for every live test body.  Prints a SKIP line and
/// returns `false` unless the env gate is armed; also enforces the
/// production-endpoint refusal.  Usage: `if !live_armed("name") { return; }`.
fn live_armed(test_name: &str) -> bool {
    if !live_enabled() {
        println!(
            "=== {test_name} SKIP: set {ENV_GATE}=1 to run live DPS smoke \
             (feature `live-dps` + `--ignored` + {ENV_GATE}=1 all required) ==="
        );
        return false;
    }
    let endpoint = resolve_host();
    let host = host_of(&endpoint);
    // Default-deny allowlist on the PARSED host (not a substring of the raw
    // URL): exact `cabinet.tax.gov.ua`, a `.cabinet…` subdomain, or a
    // `*-cabinet…` dev cabinet.  Rejects every prod endpoint (prro/prro2/fs)
    // AND lookalikes like `cabinet.tax.gov.ua.evil.com`.
    let allowed = host == TEST_HOST_MARKER
        || host.ends_with(&format!(".{TEST_HOST_MARKER}"))
        || host.ends_with(&format!("-{TEST_HOST_MARKER}"));
    if !allowed {
        panic!(
            "{test_name} REFUSED: {ENV_HOST}={endpoint} resolves to host `{host}`, \
             which is not a DPS TEST cabinet (allowlist: `{TEST_HOST_MARKER}` / \
             `*-{TEST_HOST_MARKER}` / `*.{TEST_HOST_MARKER}`).  The live smoke is \
             test-server only (default {DEFAULT_HOST}); refusing to risk \
             fiscalizing against a production endpoint (prro/prro2/fs.tax.gov.ua)."
        );
    }
    true
}

/// Extract the bare hostname from an endpoint URL — strips scheme, optional
/// userinfo, port, and any path — so the allowlist matches the real host and
/// not a substring of the URL (e.g. a path or a lookalike domain).
fn host_of(endpoint: &str) -> &str {
    let after_scheme = endpoint.split("://").nth(1).unwrap_or(endpoint);
    // Isolate the AUTHORITY first: it ends at the first '/', '?', or '#'.  This
    // MUST happen before userinfo stripping — otherwise a query/fragment like
    // `prro.tax.gov.ua:9443?x=@cabinet.tax.gov.ua` would let the `@cabinet…` in
    // the query masquerade as the host and bypass the prod-refusal allowlist
    // (the real URI host there is prro.tax.gov.ua).
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // Within the authority, strip userinfo (`user@host`) then the port.
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    hostport.split(':').next().unwrap_or(hostport)
}

#[test]
fn host_of_isolates_authority_and_blocks_prod_tricks() {
    // Real test cabinet (default).
    assert_eq!(
        host_of("https://cabinet.tax.gov.ua:9443"),
        "cabinet.tax.gov.ua"
    );
    assert_eq!(
        host_of("https://cabinet.tax.gov.ua:9443/path"),
        "cabinet.tax.gov.ua"
    );
    // Valid test host WITH a query must still resolve to the cabinet (no false reject).
    assert_eq!(
        host_of("https://cabinet.tax.gov.ua?param=1"),
        "cabinet.tax.gov.ua"
    );
    // Round-4 bypass: query/fragment `@cabinet…` must NOT be read as the host —
    // the real authority host is the prod endpoint, which must be rejected.
    assert_eq!(
        host_of("https://prro.tax.gov.ua:9443?x=@cabinet.tax.gov.ua"),
        "prro.tax.gov.ua"
    );
    assert_eq!(
        host_of("https://prro.tax.gov.ua:9443#@cabinet.tax.gov.ua"),
        "prro.tax.gov.ua"
    );
    // Userinfo trick: real host is AFTER the '@' inside the authority.
    assert_eq!(host_of("https://cabinet.tax.gov.ua@evil.com"), "evil.com");
}

// ─── Piece 1 — connectivity probe ───────────────────────────────────────

/// **W4-Z3 Smoke 1 — DPS connect + wire round-trip (connectivity)**.
///
/// Cheapest reachability check, mirroring `live_smoke_w12_hardening` Smoke A:
/// `GrpcDpsChannel::connect` (eager TLS handshake + HTTP/2 SETTINGS) then a
/// `last_chk` with a dummy `CheckSignBlob`.  DPS rejects the dummy sign with
/// a typed application-level error — which itself proves TLS + HTTP/2 + gRPC
/// + response-parse all work.  Only `DpsError::Transport` is a FAIL (wire
/// brokenness).  No real signing, no fiscal mutation, zero rate-limit cost
/// beyond one read RPC.
///
/// PASS: connect ok AND `last_chk` returns `Ok(_)` or any non-`Transport`
/// `Err`.  FAIL: connect error, or `DpsError::Transport`.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_1_connect_probe() {
    if !live_armed("W4-Z3 Smoke 1") {
        return;
    }
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    println!("\n=== W4-Z3 Smoke 1: DPS connectivity ===");
    println!("Endpoint: {host}");
    println!("FN:       {fiscal_number}");
    println!("Timeout:  {SMOKE_TIMEOUT_SECS}s\n");

    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "Smoke 1 FAIL: GrpcDpsChannel::connect — wire-level connectivity \
                 broken (TLS / DNS / network / handshake timeout).  Endpoint: \
                 {host}.  Error: {e:?}"
            )
        });

    let dummy_sign = CheckSignBlob(vec![0u8; 32]);
    let result = channel.last_chk(&dummy_sign).await;
    println!("last_chk(dummy) response: {result:?}\n");

    match result {
        Err(DpsError::Transport(msg)) => {
            panic!(
                "Smoke 1 FAIL: wire-level Transport error on last_chk — connection \
                 established but mid-call failure (server reset / deadline / proxy).  \
                 Error: {msg}"
            );
        }
        Ok(_) | Err(_) => {
            println!(
                "Smoke 1 PASS: wire path operational (TLS + HTTP/2 + gRPC + \
                 response parse).  Ready for native-signed RPCs (pieces 2+)."
            );
        }
    }
}

// ─── Piece 2 — native-signed lastChk (read the DPS chain tip) ───────────

/// Load the signing key from `PRRO_LIVE_DPS_JKS_PATH` (+ `..._JKS_PASS`).
/// Returns `None` (with a SKIP line) when the signing env is not provided,
/// so a connectivity-only run (piece 1) still works without a key mounted.
fn load_signing_key(test_name: &str) -> Option<ExtractedKey> {
    let (Some(path), Some(pass)) = (
        std::env::var(ENV_JKS_PATH).ok(),
        std::env::var(ENV_JKS_PASS).ok(),
    ) else {
        println!(
            "=== {test_name} SKIP: set {ENV_JKS_PATH} + {ENV_JKS_PASS} to run \
             signed live RPCs (piece 2+) ==="
        );
        return None;
    };
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{test_name}: cannot read JKS at {path}: {e}"));
    let ek = extract_private_key(&bytes, &pass).unwrap_or_else(|e| {
        panic!("{test_name}: extract_private_key failed (wrong pass / not a JKS?): {e:?}")
    });
    Some(ek)
}

/// Build the `rro_fn_sign` blob for the read RPCs (`lastChk` / `statusRro`).
///
/// Per the DPS protocol (field `rro_fn_sign`: «Фіскальній номер пРРО
/// підписаний електронним підписом з позначкою часу»), the signed content
/// is the fiscal-number string and the signature carries a `signingTime` —
/// i.e. exactly the ATTACHED CAdES-BES profile `sendChkV2` requires.  We
/// reproduce the production signer path (`crypto::in_process`): native
/// DSTU-4145 (PB-257) + GOST-34.311 digest, ATTACHED eContent, signingTime.
fn sign_fn_blob(ek: &ExtractedKey, fiscal_number: &str) -> CheckSignBlob {
    // Embed the SIGNING cert (KeyUsage=digitalSignature), NOT certs[0] — a UA
    // EDS keystore holds both a signing AND a key-agreement (encryption) cert,
    // and embedding the encryption cert makes DPS reject the signature
    // (CryptBadSign).  `signing_cert()` (prro_crypto, PR #107) does the select.
    let cert_der: &[u8] = ek
        .signing_cert()
        .expect("JKS must carry a signing certificate (KeyUsage=digitalSignature)");
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&ek.param_d[..], curve.mod_words);
    let signer = DstuInProcessSigner::new(d);
    let cms = CmsSigner {
        cert_der,
        signer: &signer,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    };
    let der = cms
        .sign_with(
            fiscal_number.as_bytes(),
            CmsBuildOptions {
                attached: true,
                signing_time: Some(SystemTime::now()),
            },
        )
        .expect("native attached CAdES-BES sign of FN must succeed")
        .cms_der;
    CheckSignBlob(der)
}

/// **W4-Z3 Smoke 2 — native-signed `lastChk` (read DPS chain tip)**.
///
/// Builds a REAL `rro_fn_sign` (FN signed with the operator EDS, ATTACHED
/// CAdES-BES + signingTime — the same profile `sendChkV2` requires) and reads
/// the FN's chain state from live DPS.  First RPC that exercises the native
/// attached signer end-to-end against the real server; read-only (no fiscal
/// mutation, no chain advance).
///
/// PASS: `last_chk` returns `Ok(CheckAck)` — DPS accepted our signature and
/// returned its view of the chain (`data_sign` = the previous check's CMS;
/// empty body for a virgin/genesis FN).  Rate-limit (`status=-4`) self-skips.
/// `Transport` is the only hard FAIL; a signature rejection (`-1
/// ERROR_VEREFY`) fails loudly because it means the attached-CMS profile does
/// not match what DPS expects for `rro_fn_sign`.  Piece 3 will CMS-strip
/// `data_sign` + sha256 it into `node_state` as the MAC-chain seed.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_2_last_chk_real() {
    if !live_armed("W4-Z3 Smoke 2") {
        return;
    }
    let Some(ek) = load_signing_key("W4-Z3 Smoke 2") else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    println!("\n=== W4-Z3 Smoke 2: native-signed lastChk ===");
    println!("Endpoint: {host}");
    println!("FN:       {fiscal_number}\n");

    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke 2 FAIL: GrpcDpsChannel::connect: {e:?}"));

    let fn_sign = sign_fn_blob(&ek, &fiscal_number);
    println!(
        "rro_fn_sign: {} bytes (ATTACHED CAdES-BES over the FN string)",
        fn_sign.0.len()
    );

    match channel.last_chk(&fn_sign).await {
        Ok(ack) => {
            println!(
                "Smoke 2 PASS: lastChk ACCEPTED our native signature.\n  \
                 id        = {:?}\n  id_sign   = {} bytes\n  \
                 data_sign = {} bytes (previous check CMS; 0 = virgin/genesis FN)",
                ack.id,
                ack.id_sign.len(),
                ack.data_sign.len()
            );
        }
        Err(DpsError::Server { code: -4, message }) => {
            println!(
                "Smoke 2 SKIP: DPS rate-limit (status=-4): {message}. \
                 Cool down 5+ minutes before re-running."
            );
        }
        Err(DpsError::Transport(msg)) => {
            panic!("Smoke 2 FAIL: wire-level Transport error on lastChk: {msg}");
        }
        Err(e) => {
            panic!(
                "Smoke 2 FAIL: lastChk REJECTED our native signature: {e:?}.  If this is \
                 `-1 ERROR_VEREFY`, the ATTACHED-CMS profile (eContent / signingTime / \
                 cert / signed content = FN bytes) does not match what DPS expects for \
                 `rro_fn_sign` — re-check the signed-content encoding."
            );
        }
    }
}

// ─── Piece 3 — MAC-chain seed bootstrap from live lastChk ────────────────

/// **W4-Z3 Smoke 3 — MAC-chain seed bootstrap**.
///
/// Closes the fresh-DB-vs-DPS-history gap: a fresh gateway DB has no MAC
/// chain state, so its first online SELL would carry the wrong `<MAC>` and be
/// rejected `-12 ERROR_BAD_HASH_PREV`. The fix (per the WebCheck-proven model)
/// is to seed `node_state.last_known_unsigned_xml_sha256` from the DPS view:
///
///   1. `lastChk` → `CheckAck.data_sign` = the FN's PREVIOUS check (ATTACHED
///      CMS). Empty body ⇒ genesis (virgin FN) — nothing to seed.
///   2. `extract_econtent(data_sign)` → the CMS-stripped inner check bytes.
///   3. `SHA-256` of those bytes = the MAC the NEXT check must carry.
///   4. `node_state::seed_prevhash` persists it; read it back to confirm.
///
/// Read-only against DPS (one `lastChk`); all mutation is to a throwaway temp
/// DB. The byte-exactness of this seed vs what DPS expects on the next SELL is
/// the piece-3 mid-review focus and is finally proven by piece 5's `sendChkV2`.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_3_mac_seed() {
    if !live_armed("W4-Z3 Smoke 3") {
        return;
    }
    let Some(ek) = load_signing_key("W4-Z3 Smoke 3") else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    println!("\n=== W4-Z3 Smoke 3: MAC-seed bootstrap from live lastChk ===");
    println!("FN: {fiscal_number}\n");

    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke 3 FAIL: connect: {e:?}"));

    let fn_sign = sign_fn_blob(&ek, &fiscal_number);
    let ack = match channel.last_chk(&fn_sign).await {
        Ok(a) => a,
        Err(DpsError::Server { code: -4, message }) => {
            println!("Smoke 3 SKIP: DPS rate-limit (-4): {message}. Cool down 5+ min.");
            return;
        }
        Err(e) => panic!("Smoke 3 FAIL: lastChk: {e:?}"),
    };

    if ack.data_sign.is_empty() {
        println!(
            "Smoke 3: FN is genesis (lastChk.data_sign empty) — the MAC chain has no \
             previous check; the first SELL carries an empty <MAC> and there is nothing \
             to seed. PASS (genesis path)."
        );
        return;
    }

    // CMS-strip the previous check + hash it = the next check's MAC.
    let inner = extract_econtent(&ack.data_sign)
        .unwrap_or_else(|e| panic!("Smoke 3 FAIL: cannot CMS-strip data_sign: {e}"));
    let mac_seed: [u8; 32] = Sha256::digest(&inner).into();
    let hex: String = mac_seed.iter().map(|b| format!("{b:02x}")).collect();
    println!("  data_sign = {} bytes", ack.data_sign.len());
    println!(
        "  inner     = {} bytes (CMS-stripped previous check)",
        inner.len()
    );
    println!("  MAC seed  = {hex}");

    // Seed a throwaway node_state row + read it back.
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = open_pool(&dir.path().join("prro.db"))
        .await
        .expect("open migrated temp pool");
    // node_state.fiscal_number FK → fiscal_number_config: seed the parent row.
    fn_cfg::insert(
        &pool,
        &fn_cfg::NewFnConfig {
            fiscal_number: fiscal_number.clone(),
            tax_number: "TN-test".to_string(),
            vat_payer_inn: None,
            fiscal_mode: FiscalMode::Test,
            org_name: None,
            point_name: None,
            org_address: None,
            tsp_enabled: false,
            offline_enabled: true,
            national_check_enabled: false,
            min_offline_codes: 50,
            max_offline_codes: 1000,
        },
    )
    .await
    .expect("seed fiscal_number_config (FK parent)");
    node_state::upsert_initial(
        &pool,
        &fiscal_number,
        NodeMode::Online,
        ShiftState::Closed,
        1,
    )
    .await
    .expect("seed node_state row");

    let updated = node_state::seed_prevhash(&pool, &fiscal_number, &mac_seed)
        .await
        .expect("seed_prevhash");
    assert!(
        updated,
        "seed_prevhash must update the existing node_state row"
    );

    let row = node_state::get(&pool, &fiscal_number)
        .await
        .expect("node_state::get")
        .expect("node_state row present");
    assert_eq!(
        row.last_known_unsigned_xml_sha256,
        Some(mac_seed),
        "the seeded MAC must round-trip through node_state.last_known_unsigned_xml_sha256"
    );
    println!(
        "Smoke 3 PASS: MAC seed persisted + read back from node_state \
         (next online SELL will chain off this hash)."
    );
}

// ─── Piece 4 — extended SELL build + NATIVE sign via production stage_sign ──
//
// **Offline piece (NO live DPS).**  Unlike pieces 1-3, piece 4 never
// touches the wire: it drives a PREPARED extended-SELL document through
// the REAL production write-path (`App::reconcile_pending_with` →
// `dispatch_prepared_via_chain` → `stage_sign::run` → `stage_send::run`)
// with a STUB DpsChannel, so it proves the build+sign half of the cycle
// without fiscalizing.  What it adds over pieces 1-3 and over the mock-
// only goldens: the canonical XML is produced by the production
// driver→canonical translation (`check_payload_from`, W4-Z2a) AND signed
// by the REAL native in-process signer (`InProcessProvider` +
// `prro_crypto`) using the REAL operator key — the exact combination
// piece 5 then sends to live DPS.
//
// Gate: feature `live-dps` (file-level) + `#[ignore]` + a real JKS key
// (`PRRO_LIVE_DPS_JKS_PATH` / `_PASS`).  It does NOT require
// `PRRO_LIVE_DPS=1` because there is no DPS contact — the stub channel
// answers `send_chk` locally and panics on every other method.
//
// **Assertions:**
//   (1) PREPARED → SENT (full sign+send chain ran, stub ack).
//   (2) PAYLOAD_XML carries CANONICAL tax groups `TX="1"` / `TX="2"`
//       (driver 5/7 were translated) and NOT the raw driver numbers —
//       this is the live proof of the W4-Z2a silent-fiscal-divergence
//       fix on the production path.
//   (3) extended attrs present: excise `<CA CA="…">` children + UKTZED
//       `CZD="…"` + `<TX>` summaries for both canonical groups.
//   (4) SIGNED_XML is a valid ATTACHED CMS whose eContent is byte-
//       identical to PAYLOAD_XML — proves the native signer embedded the
//       exact canonical XML (no re-encode / detached drift).

/// Synthetic FN for the offline piece-4 drive.  NOT a real registered
/// PRRO — piece 4 never contacts DPS, so any well-formed FN works; using
/// a synthetic one keeps the offline test decoupled from live-cabinet
/// registration state.
const PIECE4_FN: &str = "4000000077";

/// Extended SELL payload in the adapter wire-JSON shape consumed by
/// `stage_sign`'s `CheckJson`.  Two items exercise the full extended
/// surface via POS driver-numbers (translated by the pinned snapshot):
///   - item 1: `tax_group_1=5` (→ canonical TX=1, 20% VAT) + UKTZED
///     (`CZD`) + two excise stamps (`<CA>` children).
///   - item 2: `tax_group_1=7` (→ canonical TX=2, 7% VAT), plain line.
const EXTENDED_SELL_PAYLOAD_JSON: &str = r#"{
  "items": [
    {
      "code": "ALC-001",
      "name": "VODKA 0.5L",
      "price_kop": 6000,
      "quantity_thousandths": 1000,
      "sum_kop": 6000,
      "uktzed": "22042100",
      "tax_group_1": 5,
      "excise_stamps": ["UA1234567890", "UA0987654321"]
    },
    {
      "code": "ART-002",
      "name": "JUICE 1L",
      "price_kop": 3000,
      "quantity_thousandths": 1000,
      "sum_kop": 3000,
      "tax_group_1": 7
    }
  ],
  "payments": [
    { "name": "CASH", "sum_kop": 9000, "type_code": "0" }
  ]
}"#;

/// Sum of the two item lines (6000 + 3000) — `command.total_sum_kop`,
/// required by `parse_payload` for a Check artifact.
const EXTENDED_SELL_TOTAL_SUM_KOP: i64 = 9000;

/// Build a `SigningContext` over the REAL operator key + the REAL
/// production native signer (`InProcessProvider` → `prro_crypto`).
/// Mirrors `tests/common::det_signing_ctx` but swaps the deterministic
/// stub provider/session for the live key — so `stage_sign` exercises
/// the exact crypto path piece 5 sends to DPS.  Embeds the SIGNING cert
/// (KeyUsage=digitalSignature) via `signing_cert()`, NOT `certs[0]`
/// (the encryption cert — see `sign_fn_blob`).
fn live_signing_ctx(ek: &ExtractedKey) -> SigningContext {
    let cert_der = ek
        .signing_cert()
        .expect("JKS must carry a signing certificate (KeyUsage=digitalSignature)")
        .to_vec();
    let param_d: [u8; 32] = *ek.param_d;
    SigningContext {
        provider: Arc::new(InProcessProvider::new()) as Arc<dyn CryptoProvider>,
        session: SigningSession::new_for_test(
            "GALCHUN MYKOLA DMYTROVYCH".into(),
            param_d,
            cert_der,
        ),
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    }
}

/// Stub `DpsChannel` for the OFFLINE piece-4 drive: `send_chk` returns a
/// fixed ack (so the PREPARED→SENT chain completes), every other method
/// panics — proving the chain consults ONLY `send_chk`.
struct StubAckDps;

#[async_trait]
impl DpsChannel for StubAckDps {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        Ok(CheckAck {
            id: "stub-fiscal-piece4".into(),
            id_sign: vec![],
            data_sign: vec![],
        })
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        unreachable!(
            "piece 4 stub: last_chk must not be invoked (PREPARED→SENT uses send_chk only)"
        )
    }
    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("piece 4 stub: ping must not be invoked")
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("piece 4 stub: status_rro must not be invoked")
    }
    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("piece 4 stub: info_rro must not be invoked")
    }

    async fn ask_offline_codes(
        &self,
        _: prro::transports::dps::dto::CheckEnvelope,
    ) -> Result<
        prro::transports::dps::dto::OfflineCodesResponse,
        prro::transports::dps::error::DpsError,
    > {
        unreachable!("stub: ask_offline_codes not exercised");
    }
}

/// Boot a real `App` over a throwaway temp DB (migrated, recovery run on
/// an empty DB).  Mirrors `write_path_deterministic_replay::fresh_app`.
async fn boot_offline_app() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("w4z3-piece4.db");
    let toml_text = format!(
        r#"
app_name = "prro"
version  = "0.1.0"

[database]
db_path = "{0}"
secure_db_path = "{0}_secure"

[admin_ui]
enabled = false
listen  = "127.0.0.1:8443"
"#,
        db_path.display().to_string().replace('\\', "/")
    );
    let cfg = AppConfig::from_toml(&toml_text).expect("config parse");
    let app = App::boot(cfg).await.expect("App::boot");
    (dir, app)
}

async fn seed_fn_config_piece4(pool: &SqlitePool, fn_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, '12345678', 'test')",
    )
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed an OPENED shift + ONLINE node_state (next_lnd=1, b1/t1 profiles).
/// Mirrors `write_path_deterministic_replay::seed_open_shift_and_node`.
async fn seed_open_shift_and_node_piece4(pool: &SqlitePool, fn_id: &str) {
    use prro::db::models::ids::ShiftId;
    let shift_id = ShiftId::new();
    sqlx::query(
        "INSERT INTO shifts (shift_id, fiscal_number, serial, state, open_mode, cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(shift_id)
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO node_state (fiscal_number, mode, shift_state, current_shift_id, \
            next_lnd, backend_profile_id, transport_profile_id) \
         VALUES (?, 'ONLINE', 'OPENED', ?, 1, 'b1', 't1')",
    )
    .bind(fn_id)
    .bind(shift_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a PREPARED extended-SELL `fiscal_documents` row with the
/// snapshot FK set (so the PREPARED reconcile arm loads the snapshot and
/// `derive_check_tax_summaries` can emit `<TX>`).  The doc's own shift
/// (doc_byte^0x80) carries `opened_by_cashier_id='test-cashier'` matching
/// `signed_by_cashier_id`, so stage_send 4-pre signer_guard passes.
/// Mirrors `write_path_deterministic_replay::seed_doc_prepared_full` +
/// the `signing_config_snapshot_id` FK.
async fn seed_extended_sell_prepared(
    pool: &SqlitePool,
    fn_id: &str,
    doc_byte: u8,
    snapshot_id: i64,
    business_ts: &str,
) -> (DocumentId, [u8; 16]) {
    let shift_byte = doc_byte ^ 0x80;
    let shift_bytes = vec![shift_byte; 16];
    sqlx::query(
        "INSERT OR IGNORE INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(&shift_bytes)
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();

    // `INSERT OR IGNORE` is silently swallowed by `ux_shifts_one_open_per_fn`
    // (partial UNIQUE on fiscal_number WHERE state IN non-terminal states) when
    // a caller such as `seed_open_shift_and_node_piece4` has already seeded a
    // different OPENED shift_id for this FN.  In that case `shift_bytes` never
    // lands, and the fiscal_documents FK trips with code 787.  Resolve the
    // actual row present in the DB so the FK is always satisfied.
    let actual_shift_id: Vec<u8> = sqlx::query_scalar(
        "SELECT shift_id FROM shifts WHERE fiscal_number = ? AND state = 'OPENED' LIMIT 1",
    )
    .bind(fn_id)
    .fetch_one(pool)
    .await
    .expect("seed_extended_sell_prepared: no OPENED shift found after INSERT OR IGNORE");

    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            total_sum_kop, payload_json, payload_sha256_canonical, signed_by_cashier_id, \
            signing_config_snapshot_id) \
         VALUES (?, ?, ?, ?, ?, 'SELL', 'PREPARED', 'b1', 't1', 'ONLINE', \
            ?, ?, ?, ?, 'test-cashier', ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(&actual_shift_id)
    .bind(lnd)
    .bind(business_ts)
    .bind(EXTENDED_SELL_TOTAL_SUM_KOP)
    .bind(EXTENDED_SELL_PAYLOAD_JSON)
    .bind(&sha)
    .bind(snapshot_id)
    .execute(pool)
    .await
    .unwrap();
    let doc_id = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let req_arr: [u8; 16] = <[u8; 16]>::try_from(req_bytes.as_slice()).unwrap();
    (doc_id, req_arr)
}

/// Seed the matching `ingress_inbox` PROCESSING row (payload identical to
/// the doc's so a payload-hash invariant guard wouldn't trip).
async fn seed_inbox_processing_for_sell_piece4(pool: &SqlitePool, fn_id: &str, req_id: &[u8; 16]) {
    let sha = vec![0u8; 32];
    let req_slice: &[u8] = req_id;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', 'SELL', ?, ?, ?, 'PROCESSING')",
    )
    .bind(req_slice)
    .bind(fn_id)
    .bind(format!("idem-{:02x}", req_id[0]))
    .bind(EXTENDED_SELL_PAYLOAD_JSON)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
}

async fn doc_state_piece4(pool: &SqlitePool, doc: DocumentId) -> String {
    sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
        .bind(doc)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn read_document_file_piece4(
    pool: &SqlitePool,
    doc: DocumentId,
    kind: &str,
) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT content FROM document_files WHERE document_id = ? AND kind = ?")
        .bind(doc)
        .bind(kind)
        .fetch_optional(pool)
        .await
        .expect("read document_files row")
}

/// **W4-Z3 Smoke 4 — extended SELL build + native sign (offline)**.
#[tokio::test]
#[ignore = "real JKS key required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS_JKS_PATH/_PASS (no DPS contact)"]
async fn live_smoke_4_extended_sell_native_sign() {
    let Some(ek) = load_signing_key("W4-Z3 Smoke 4") else {
        return;
    };
    println!("\n=== W4-Z3 Smoke 4: extended SELL build + native sign (offline, stub DPS) ===");

    let (_dir, app) = boot_offline_app().await;
    let fn_id = PIECE4_FN;
    seed_fn_config_piece4(app.db(), fn_id).await;
    seed_open_shift_and_node_piece4(app.db(), fn_id).await;

    // Pin a snapshot with the non-identity driver mapping the pilot uses:
    // driver 5 → canonical TX=1 (20% VAT), driver 7 → canonical TX=2 (7% VAT).
    let snapshot = TaxResolutionSnapshot::with_driver_mapping(
        vec![
            ResolvedTaxGroupBps {
                tx: 1,
                txpr_bps: 2000,
                dtpr_bps: 0,
                txal: 0,
                txty: 0,
            },
            ResolvedTaxGroupBps {
                tx: 2,
                txpr_bps: 700,
                dtpr_bps: 0,
                txal: 0,
                txty: 0,
            },
        ],
        vec![
            DriverNumberMapping {
                driver_number: 5,
                canonical_tx_num: 1,
            },
            DriverNumberMapping {
                driver_number: 7,
                canonical_tx_num: 2,
            },
        ],
    );
    let snapshot_id = with_immediate(app.db(), move |tx| {
        Box::pin(async move {
            let id = signing_config_snapshots::insert_or_get_id_tx(
                tx,
                PIECE4_FN,
                "driver-piece4",
                &snapshot,
            )
            .await?;
            Ok::<i64, anyhow::Error>(id)
        })
    })
    .await
    .expect("insert signing_config_snapshot");

    let (doc, req_id) =
        seed_extended_sell_prepared(app.db(), fn_id, 0x44, snapshot_id, "2026-04-22T12:00:00Z")
            .await;
    seed_inbox_processing_for_sell_piece4(app.db(), fn_id, &req_id).await;

    let stub = StubAckDps;
    let signing_ctx = live_signing_ctx(&ek);
    // fn_sign is unused by the stub (last_chk panics) — a dummy blob.
    let fn_sign = CheckSignBlob(vec![0xDE, 0xAD, 0xBE, 0xEF]);
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &stub,
        signing_ctx: &signing_ctx,
        fn_sign: &fn_sign,
    });

    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    // (1) PREPARED → SENT (sign + send chain ran).
    assert_eq!(
        doc_state_piece4(app.db(), doc).await,
        "SENT",
        "dispatch_prepared_via_chain must drive PREPARED → SIGNED → SENT (native sign + stub ack)"
    );

    // (2)+(3) PAYLOAD_XML carries CANONICAL groups + extended attrs.
    let payload_xml = read_document_file_piece4(app.db(), doc, "PAYLOAD_XML")
        .await
        .expect("stage_sign 3-PERSIST must INSERT PAYLOAD_XML");
    let signed_xml = read_document_file_piece4(app.db(), doc, "SIGNED_XML")
        .await
        .expect("stage_sign 3-PERSIST must INSERT SIGNED_XML");
    let xml = String::from_utf8_lossy(&payload_xml);

    // Driver→canonical translation reached the wire (W4-Z2a fix proof):
    // emit canonical TX=1/TX=2, NEVER the raw driver numbers 5/7.
    assert!(
        xml.contains(r#"TX="1""#),
        "driver 5 must emit canonical TX=\"1\" on the wire; xml=\n{xml}"
    );
    assert!(
        xml.contains(r#"TX="2""#),
        "driver 7 must emit canonical TX=\"2\" on the wire; xml=\n{xml}"
    );
    assert!(
        !xml.contains(r#"TX="5""#) && !xml.contains(r#"TX="7""#),
        "raw POS driver numbers (5/7) must NOT reach the wire — translation must happen; xml=\n{xml}"
    );
    // Extended attrs: excise stamps as <CA> children + UKTZED as CZD.
    assert!(
        xml.contains(r#"<CA CA="UA1234567890""#),
        "excise stamp 1 must emit a <CA> child; xml=\n{xml}"
    );
    assert!(
        xml.contains(r#"<CA CA="UA0987654321""#),
        "excise stamp 2 must emit a <CA> child; xml=\n{xml}"
    );
    assert!(
        xml.contains(r#"CZD="22042100""#),
        "UKTZED must emit the CZD attr; xml=\n{xml}"
    );
    // <TX> summaries for both canonical groups (inside <E>; attrs are
    // emitted in alphabetical order so TX is NOT the first attr).  Assert
    // the snapshot rates flowed through (2000bps→20.00, 700bps→7.00) AND
    // the VAT-inclusive amounts aggregated correctly per group:
    //   group 1: 6000 kop @ 20% incl = 1000;  group 2: 3000 @ 7% incl = 196.
    assert!(
        xml.contains(r#"TXPR="20.00""#),
        "group 1 (driver 5→canonical 1) <TX> summary must carry the 20.00% rate; xml=\n{xml}"
    );
    assert!(
        xml.contains(r#"TXPR="7.00""#),
        "group 2 (driver 7→canonical 2) <TX> summary must carry the 7.00% rate; xml=\n{xml}"
    );
    assert!(
        xml.contains(r#"TX="1" TXAL="0" TXPR="20.00" TXSM="1000""#),
        "group 1 <TX> must aggregate 6000 kop @ 20% VAT-incl = TXSM 1000; xml=\n{xml}"
    );
    assert!(
        xml.contains(r#"TX="2" TXAL="0" TXPR="7.00" TXSM="196""#),
        "group 2 <TX> must aggregate 3000 kop @ 7% VAT-incl = TXSM 196; xml=\n{xml}"
    );

    // (4) SIGNED_XML is a valid ATTACHED CMS whose eContent == PAYLOAD_XML.
    let inner = extract_econtent(&signed_xml).expect("SIGNED_XML must be a parseable ATTACHED CMS");
    assert_eq!(
        inner, payload_xml,
        "native signer's ATTACHED CMS eContent must be byte-identical to PAYLOAD_XML \
         (no detached drift / no re-encode)"
    );

    println!(
        "Smoke 4 PASS: extended SELL built via production stage_sign (driver 5→1, 7→2 translated \
         to canonical TX), native ATTACHED CMS over {} bytes XML; eContent round-trips.",
        payload_xml.len()
    );
}

// ─── Piece 5 — LIVE SHIFT_OPEN to DPS (first live fiscalization) ────────────
//
// **LIVE piece (mutates the FN chain).**  Unlike piece 4, these touch the
// real test cabinet via `GrpcDpsChannel::send_chk` (= `sendChkV2`).  A
// SHIFT_OPEN rides as `ServiceChk(3)` with `local_number=0` (per the wire
// map) and OPENS the shift on the DPS side.
//
// Drive model (WebCheck-faithful): the gateway's LOCAL online shift-state
// lifecycle is NOT wired (no `shifts`/`node_state.shift_state` flip on
// online SHIFT_OPEN→ACK — an M4 gap), so the smoke drives the WIRE only: a
// seeded PREPARED SHIFT_OPEN doc + `reconcile_pending_with` against the live
// channel.  The MAC is seeded from the live `lastChk` chain tip per send
// (trust DPS, not local state) — sidestepping the internal-chain
// byte-exactness GAP.  Local shift tracking after ACK is intentionally out
// of scope here (it is the M4 shift-state-wiring gap, reported separately).
//
// Gate: feature `live-dps` + `#[ignore]` + `PRRO_LIVE_DPS=1` + real JKS
// (these CONTACT live DPS, so the full triple gate applies).

/// Real taxpayer code (EDRPOU) for the test FN `4000162280` — the `TN`
/// attribute in the canonical XML.  MUST match the FN's registered
/// taxpayer (operator company ЄДРПОУ 13667753; the signer's individual ІПН
/// is 2790008754 but the receipt TN is the company code) or DPS rejects.
const LIVE_TN: &str = "13667753";

/// Minimal SHIFT_OPEN payload (`ShiftOpenJson { opening_sum_kop }`).
const SHIFT_OPEN_PAYLOAD_JSON: &str = r#"{"opening_sum_kop":0}"#;

/// Current UTC instant as an RFC-3339 string for `business_ts`.  Live docs
/// need a fresh timestamp — `stage_sign` converts it to the Kyiv-local
/// `<TS>` / wire `date_time`, and DPS rejects a stale receipt time.
fn iso_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Smoke-9 knob: how many SECONDS to back-date the offline documents by, read
/// from `PRRO_LIVE_DPS_OFFLINE_BACKDATE_SEC`.  `0` (the default, and the value
/// when unset or unparseable) means "date the offline docs now" — unchanged
/// behavior.  A positive `N` simulates a real offline period of `N` seconds.
fn offline_backdate_secs() -> i64 {
    std::env::var(ENV_OFFLINE_BACKDATE_SEC)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(0)
}

/// `business_ts` for a smoke-9 OFFLINE document: `iso_now()` back-dated by
/// `PRRO_LIVE_DPS_OFFLINE_BACKDATE_SEC` seconds (0 ⇒ exactly `iso_now()`).  The
/// SHIFT_OPEN + SELL offline docs derive their `business_ts` from this so all
/// three offline docs (including the BEGIN, whose `business_ts` = the session
/// `opened_at` we back-date in lockstep) land ~N seconds in the past — safely in
/// DPS's clock past while their `<TS>` and wire `date_time` still agree (both
/// derive from this same back-dated instant).
fn offline_business_ts() -> String {
    let back = offline_backdate_secs();
    if back == 0 {
        return iso_now();
    }
    (chrono::Utc::now() - chrono::Duration::seconds(back)).to_rfc3339()
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Extract the `<TS>...</TS>` inner digits from a check's XML bytes — the EXACT
/// wall-clock string (`yyyyMMddHHmmss`) DPS reads and validates against the wire
/// `Check.date`.  The `<TS>` region is ASCII even in a cp1251-encoded payload, so
/// a lossy UTF-8 scan of the raw bytes is byte-faithful for this substring.
/// Returns `None` if the blob carries no `<TS>` element (e.g. a genesis
/// `data_sign`).  Used by the smoke-9 `-8` date diagnostics to surface what DPS
/// actually receives in the signed BEGIN vs the accepted SELL.
fn extract_ts_digits(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let start = text.find("<TS>")? + "<TS>".len();
    let rest = &text[start..];
    let end = rest.find("</TS>")?;
    Some(rest[..end].to_string())
}

/// Decode a DPS "Kyiv-local-as-epoch" wire `date_time` (the value
/// `stage_send::kyiv_local_epoch` emits) into a human-readable
/// `yyyyMMddHHmmss`-shaped string, both as the RAW UTC decode of the epoch AND as
/// the Kyiv-local wall clock the operator sees.  Because the wire epoch is Kyiv
/// wall-clock digits re-interpreted as UTC, the UTC decode of that epoch is what
/// SHOULD equal the signed `<TS>` byte-for-byte — a mismatch here vs `<TS>` is
/// exactly what draws DPS `-8`.  Print-only.
fn decode_epoch_utc_and_kyiv(epoch: i64) -> (String, String) {
    use chrono::{DateTime, Utc};
    use chrono_tz::Europe::Kiev;
    let utc: DateTime<Utc> = DateTime::from_timestamp(epoch, 0)
        .unwrap_or_else(|| panic!("epoch {epoch} out of range for a DateTime decode"));
    let as_utc = utc.format("%Y%m%d%H%M%S").to_string();
    let as_kyiv = utc.with_timezone(&Kiev).format("%Y%m%d%H%M%S").to_string();
    (as_utc, as_kyiv)
}

/// Seed `fiscal_number_config` for the LIVE FN with the REAL taxpayer code.
async fn seed_fn_config_live(pool: &SqlitePool, fn_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO fiscal_number_config(fiscal_number, tax_number, fiscal_mode) \
         VALUES (?, ?, 'test')",
    )
    .bind(fn_id)
    .bind(LIVE_TN)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a PREPARED SHIFT_OPEN doc (doc_type SHIFT_OPEN, shift_id NULL — it
/// bypasses signer_guard, no snapshot FK).  `business_ts` is a fresh UTC
/// instant for the live wire time.
async fn seed_shift_open_prepared(
    pool: &SqlitePool,
    fn_id: &str,
    doc_byte: u8,
    business_ts: &str,
) -> (DocumentId, [u8; 16]) {
    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            total_sum_kop, payload_json, payload_sha256_canonical) \
         VALUES (?, ?, ?, ?, 'SHIFT_OPEN', 'PREPARED', 'b1', 't1', 'ONLINE', \
            ?, 0, ?, ?)",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(lnd)
    .bind(business_ts)
    .bind(SHIFT_OPEN_PAYLOAD_JSON)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
    let doc_id = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let req_arr: [u8; 16] = <[u8; 16]>::try_from(req_bytes.as_slice()).unwrap();
    (doc_id, req_arr)
}

/// Seed a matching `ingress_inbox` PROCESSING row.  `operation_type` MUST
/// equal the doc_type string and `payload_json` must be byte-identical to
/// the doc's, else the PREPARED-replay drift cross-check holds the doc.
async fn seed_inbox_processing_generic(
    pool: &SqlitePool,
    fn_id: &str,
    req_id: &[u8; 16],
    op_type: &str,
    payload_json: &str,
) {
    let sha = vec![0u8; 32];
    let req_slice: &[u8] = req_id;
    sqlx::query(
        "INSERT INTO ingress_inbox(request_id, fiscal_number, protocol, operation_type, \
            idempotency_key, payload_json, payload_sha256_canonical, status) \
         VALUES (?, ?, 'REST', ?, ?, ?, ?, 'PROCESSING')",
    )
    .bind(req_slice)
    .bind(fn_id)
    .bind(op_type)
    .bind(format!("idem-{:02x}", req_id[0]))
    .bind(payload_json)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed the MAC chain from the live `lastChk` chain tip (WebCheck model:
/// the next check's `<MAC>` = sha256 of the CMS-stripped previous check as
/// DPS returns it).  No-op for a genesis FN (empty data_sign).  Returns the
/// hex seed for logging, or None for genesis.
async fn seed_mac_from_lastchk(pool: &SqlitePool, fn_id: &str, ack: &CheckAck) -> Option<String> {
    if ack.data_sign.is_empty() {
        return None;
    }
    let inner = extract_econtent(&ack.data_sign)
        .unwrap_or_else(|e| panic!("CMS-strip lastChk.data_sign: {e}"));
    let mac_seed: [u8; 32] = Sha256::digest(&inner).into();
    node_state::seed_prevhash(pool, fn_id, &mac_seed)
        .await
        .expect("seed_prevhash");
    Some(hex_lower(&mac_seed))
}

/// Read the latest transport_trace outcome + the doc's server_fiscal_no —
/// used to diagnose a non-ACK send (which DPS code rejected it).
// The diagnostic row is a 5-column projection read once on a failed live
// send; a named struct would add ceremony for a print-only helper.
#[allow(clippy::type_complexity)]
async fn print_live_diagnostics(pool: &SqlitePool, doc: DocumentId) {
    // transport_trace carries the DPS code in `server_status_code` (the
    // -1..-16 reject code) plus error_kind / error_message.
    let trace: Option<(
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT outcome_kind, server_status_code, error_kind, error_message, server_fiscal_no \
         FROM transport_trace WHERE document_id = ? ORDER BY attempt_no DESC LIMIT 1",
    )
    .bind(doc)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    println!(
        "  transport_trace(latest) [outcome, dps_code, error_kind, error_message, sfn]: {trace:?}"
    );
    // audit_log's payload column is `event_payload_json`.
    let audits: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT event_type, event_payload_json FROM audit_log ORDER BY rowid DESC LIMIT 8",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (et, pj) in &audits {
        println!("  audit: {et}  {}", pj.as_deref().unwrap_or(""));
    }
}

/// **W4-Z3 Smoke 5a — DPS shift-state probe (read-only)**.
///
/// `statusRro` + `lastChk` against the live cabinet — reports whether a
/// shift is already OPEN on the FN and the current chain tip, so the
/// operator can decide whether a SHIFT_OPEN (5b) is the right next move.
/// Read-only: no fiscal mutation.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_5a_status_probe() {
    if !live_armed("W4-Z3 Smoke 5a") {
        return;
    }
    let Some(ek) = load_signing_key("W4-Z3 Smoke 5a") else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    println!("\n=== W4-Z3 Smoke 5a: DPS shift-state probe (read-only) ===");
    println!("FN: {fiscal_number}\n");

    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke 5a FAIL: connect: {e:?}"));
    let fn_sign = sign_fn_blob(&ek, &fiscal_number);

    match channel.status_rro(&fn_sign).await {
        Ok(s) => println!(
            "  statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        ),
        Err(DpsError::Server { code: -4, message }) => {
            println!("  Smoke 5a SKIP: DPS rate-limit (-4): {message}");
            return;
        }
        Err(DpsError::Transport(msg)) => panic!("Smoke 5a FAIL: Transport on statusRro: {msg}"),
        Err(e) => println!("  statusRro non-fatal error: {e:?}"),
    }

    match channel.last_chk(&fn_sign).await {
        Ok(a) => println!(
            "  lastChk: id={:?} data_sign={} bytes (0 = genesis)",
            a.id,
            a.data_sign.len()
        ),
        Err(DpsError::Server { code: -4, message }) => {
            println!("  Smoke 5a SKIP: rate-limit (-4): {message}");
            return;
        }
        Err(e) => println!("  lastChk non-fatal error: {e:?}"),
    }
    println!("Smoke 5a PASS: DPS shift-state + chain tip read.");
}

/// **W4-Z3 Smoke 5b — LIVE SHIFT_OPEN (fiscalizes; opens the DPS shift)**.
///
/// Drives a seeded PREPARED SHIFT_OPEN through the production write-path
/// against the live cabinet: lastChk → seed MAC from the chain tip →
/// reconcile → `sendChkV2` (ServiceChk, local_number=0).  ACK (status OK +
/// non-empty id) advances the doc to SENT with `server_fiscal_no`.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_5b_shift_open() {
    if !live_armed("W4-Z3 Smoke 5b") {
        return;
    }
    let Some(ek) = load_signing_key("W4-Z3 Smoke 5b") else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    println!("\n=== W4-Z3 Smoke 5b: LIVE SHIFT_OPEN → DPS ===");
    println!("FN: {fiscal_number}  TN: {LIVE_TN}\n");

    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke 5b FAIL: connect: {e:?}"));

    let fn_sign = sign_fn_blob(&ek, &fiscal_number);
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        println!(
            "  pre statusRro: open_shift={} online={}",
            s.open_shift, s.online
        );
    }
    let ack = match channel.last_chk(&fn_sign).await {
        Ok(a) => a,
        Err(DpsError::Server { code: -4, message }) => {
            println!("Smoke 5b SKIP: rate-limit (-4): {message}");
            return;
        }
        Err(e) => panic!("Smoke 5b FAIL: lastChk: {e:?}"),
    };

    let (_dir, app) = boot_offline_app().await;
    seed_fn_config_live(app.db(), &fiscal_number).await;
    node_state::upsert_initial(
        app.db(),
        &fiscal_number,
        NodeMode::Online,
        ShiftState::Closed,
        1,
    )
    .await
    .expect("seed node_state");

    match seed_mac_from_lastchk(app.db(), &fiscal_number, &ack).await {
        Some(hex) => println!(
            "  MAC seed from lastChk: {hex} ({} B data_sign)",
            ack.data_sign.len()
        ),
        None => println!("  FN genesis (empty data_sign) — SHIFT_OPEN carries empty <MAC>"),
    }

    let business_ts = iso_now();
    let (doc, req_id) =
        seed_shift_open_prepared(app.db(), &fiscal_number, 0x51, &business_ts).await;
    seed_inbox_processing_generic(
        app.db(),
        &fiscal_number,
        &req_id,
        "SHIFT_OPEN",
        SHIFT_OPEN_PAYLOAD_JSON,
    )
    .await;

    let signing_ctx = live_signing_ctx(&ek);
    let drive_fn_sign = sign_fn_blob(&ek, &fiscal_number);
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &channel,
        signing_ctx: &signing_ctx,
        fn_sign: &drive_fn_sign,
    });
    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    let state = doc_state_piece4(app.db(), doc).await;
    let server_fiscal_no: Option<String> =
        sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(app.db())
            .await
            .unwrap();
    println!("  post-reconcile: state={state} server_fiscal_no={server_fiscal_no:?}");
    print_live_diagnostics(app.db(), doc).await;

    assert_eq!(
        state, "SENT",
        "SHIFT_OPEN must reach SENT (DPS ACK). If not, the diagnostics above carry the DPS code \
         (e.g. -12 MAC, -8 date, -14 signer, -15 no-shift)."
    );
    assert!(
        server_fiscal_no.is_some(),
        "ACK must populate server_fiscal_no (the DPS-assigned receipt id)"
    );
    println!("Smoke 5b PASS: LIVE SHIFT_OPEN fiscalized — server_fiscal_no={server_fiscal_no:?}");
}

// ─── Piece 6/7 — LIVE extended SELL + Z_REPORT (complete the shift cycle) ───
//
// Run order (each its own temp DB; DPS state is server-side): 5b SHIFT_OPEN
// (opens the DPS shift) → 6 SELL (into the open shift) → 7 Z_REPORT (closes
// it).  Each doc re-seeds its `<MAC>` from the live `lastChk` chain tip
// (WebCheck model), so the chain advances SHIFT_OPEN→SELL→Z automatically.
// local_number is sequential (SHIFT_OPEN wire=0; SELL=1; Z=2).

/// Pinned snapshot with the non-identity driver mapping the pilot uses:
/// driver 5 → canonical TX=1 (20% VAT), driver 7 → canonical TX=2 (7% VAT).
fn dual_mapping_snapshot() -> TaxResolutionSnapshot {
    TaxResolutionSnapshot::with_driver_mapping(
        vec![
            ResolvedTaxGroupBps {
                tx: 1,
                txpr_bps: 2000,
                dtpr_bps: 0,
                txal: 0,
                txty: 0,
            },
            ResolvedTaxGroupBps {
                tx: 2,
                txpr_bps: 700,
                dtpr_bps: 0,
                txal: 0,
                txty: 0,
            },
        ],
        vec![
            DriverNumberMapping {
                driver_number: 5,
                canonical_tx_num: 1,
            },
            DriverNumberMapping {
                driver_number: 7,
                canonical_tx_num: 2,
            },
        ],
    )
}

/// Minimal Z payload reflecting the shift's one SELL (9000 kop cash).
/// `ZReportJson { payments, sell_count, return_count }`.
const Z_REPORT_PAYLOAD_JSON: &str = r#"{"payments":[{"name":"CASH","sum_in_kop":9000,"sum_out_kop":0,"type_code":"0"}],"sell_count":1,"return_count":0}"#;

/// Seed a PREPARED Z_REPORT doc bound to a local OPENED shift + signer
/// (mirrors the SELL seed; no snapshot FK — Z does not translate tax).
async fn seed_z_report_prepared(
    pool: &SqlitePool,
    fn_id: &str,
    doc_byte: u8,
    business_ts: &str,
) -> (DocumentId, [u8; 16]) {
    let shift_byte = doc_byte ^ 0x80;
    let shift_bytes = vec![shift_byte; 16];
    sqlx::query(
        "INSERT OR IGNORE INTO shifts(shift_id, fiscal_number, serial, state, open_mode, \
            cash_balance_kop, opened_by_cashier_id) \
         VALUES (?, ?, 1, 'OPENED', 'ONLINE', 0, 'test-cashier')",
    )
    .bind(&shift_bytes)
    .bind(fn_id)
    .execute(pool)
    .await
    .unwrap();

    // Same fix as in seed_extended_sell_prepared: resolve the actual shift_id
    // that landed (or was already present due to a prior seed call) so the
    // fiscal_documents FK is always satisfied even when OR IGNORE was a no-op.
    let actual_shift_id: Vec<u8> = sqlx::query_scalar(
        "SELECT shift_id FROM shifts WHERE fiscal_number = ? AND state = 'OPENED' LIMIT 1",
    )
    .bind(fn_id)
    .fetch_one(pool)
    .await
    .expect("seed_z_report_prepared: no OPENED shift found after INSERT OR IGNORE");

    let doc_bytes = vec![doc_byte; 16];
    let req_bytes = vec![doc_byte ^ 0xFF; 16];
    let sha = vec![0u8; 32];
    let lnd = doc_byte as i64;
    sqlx::query(
        "INSERT INTO fiscal_documents(document_id, request_id, fiscal_number, shift_id, lnd, \
            doc_type, state, backend_profile_id, transport_profile_id, fs_mode, business_ts, \
            total_sum_kop, payload_json, payload_sha256_canonical, signed_by_cashier_id) \
         VALUES (?, ?, ?, ?, ?, 'Z_REPORT', 'PREPARED', 'b1', 't1', 'ONLINE', \
            ?, 0, ?, ?, 'test-cashier')",
    )
    .bind(&doc_bytes)
    .bind(&req_bytes)
    .bind(fn_id)
    .bind(&actual_shift_id)
    .bind(lnd)
    .bind(business_ts)
    .bind(Z_REPORT_PAYLOAD_JSON)
    .bind(&sha)
    .execute(pool)
    .await
    .unwrap();
    let doc_id = DocumentId::from_bytes(<[u8; 16]>::try_from(doc_bytes.as_slice()).unwrap());
    let req_arr: [u8; 16] = <[u8; 16]>::try_from(req_bytes.as_slice()).unwrap();
    (doc_id, req_arr)
}

/// **W4-Z3 Smoke 6 — LIVE extended SELL (W4-Z3 core: extended XML to DPS)**.
///
/// Requires the DPS shift to be OPEN (run 5b first).  Drives the piece-4
/// extended SELL (driver 5→1, 7→2 + excise + UKTZED) against the live
/// cabinet: lastChk → MAC seed → reconcile → sendChkV2 (Chk, local_number=1).
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_6_extended_sell() {
    if !live_armed("W4-Z3 Smoke 6") {
        return;
    }
    let Some(ek) = load_signing_key("W4-Z3 Smoke 6") else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    println!("\n=== W4-Z3 Smoke 6: LIVE extended SELL → DPS ===");
    println!("FN: {fiscal_number}  TN: {LIVE_TN}\n");

    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke 6 FAIL: connect: {e:?}"));

    let fn_sign = sign_fn_blob(&ek, &fiscal_number);
    match channel.status_rro(&fn_sign).await {
        Ok(s) => {
            println!(
                "  pre statusRro: open_shift={} online={}",
                s.open_shift, s.online
            );
            assert!(
                s.open_shift,
                "Smoke 6 requires an OPEN shift on DPS — run live_smoke_5b_shift_open first"
            );
        }
        Err(e) => println!("  statusRro non-fatal: {e:?}"),
    }
    let ack = match channel.last_chk(&fn_sign).await {
        Ok(a) => a,
        Err(DpsError::Server { code: -4, message }) => {
            println!("Smoke 6 SKIP: rate-limit (-4): {message}");
            return;
        }
        Err(e) => panic!("Smoke 6 FAIL: lastChk: {e:?}"),
    };

    let (_dir, app) = boot_offline_app().await;
    seed_fn_config_live(app.db(), &fiscal_number).await;
    seed_open_shift_and_node_piece4(app.db(), &fiscal_number).await;
    match seed_mac_from_lastchk(app.db(), &fiscal_number, &ack).await {
        Some(hex) => println!("  MAC seed from lastChk: {hex}"),
        None => println!("  genesis (empty data_sign)"),
    }

    let snapshot = dual_mapping_snapshot();
    let fn_snap = fiscal_number.clone();
    let snapshot_id = with_immediate(app.db(), move |tx| {
        Box::pin(async move {
            let id = signing_config_snapshots::insert_or_get_id_tx(
                tx,
                &fn_snap,
                "driver-piece6",
                &snapshot,
            )
            .await?;
            Ok::<i64, anyhow::Error>(id)
        })
    })
    .await
    .expect("insert signing_config_snapshot");

    let business_ts = iso_now();
    let (doc, req_id) =
        seed_extended_sell_prepared(app.db(), &fiscal_number, 0x01, snapshot_id, &business_ts)
            .await;
    seed_inbox_processing_for_sell_piece4(app.db(), &fiscal_number, &req_id).await;

    let signing_ctx = live_signing_ctx(&ek);
    let drive_fn_sign = sign_fn_blob(&ek, &fiscal_number);
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &channel,
        signing_ctx: &signing_ctx,
        fn_sign: &drive_fn_sign,
    });
    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    let state = doc_state_piece4(app.db(), doc).await;
    let server_fiscal_no: Option<String> =
        sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(app.db())
            .await
            .unwrap();
    println!("  post-reconcile: state={state} server_fiscal_no={server_fiscal_no:?}");
    print_live_diagnostics(app.db(), doc).await;

    assert_eq!(
        state, "SENT",
        "extended SELL must reach SENT (DPS ACK). Diagnostics above carry the DPS code on reject."
    );
    assert!(
        server_fiscal_no.is_some(),
        "ACK must populate server_fiscal_no"
    );
    println!(
        "Smoke 6 PASS: LIVE extended SELL fiscalized (driver 5→1/7→2, excise, UKTZED) — \
         server_fiscal_no={server_fiscal_no:?}"
    );
}

/// **W4-Z3 Smoke 7 — LIVE Z_REPORT (closes the DPS shift; bookend)**.
///
/// Requires an OPEN shift with the SELL fiscalized (run 5b → 6 first).
/// Drives a Z_REPORT against the live cabinet: lastChk → MAC seed →
/// reconcile → sendChkV2 (ZReport, local_number=2).  NOTE: the Z report
/// NUMBER is allocated fresh (1) from a fresh temp DB — if DPS enforces a
/// per-RRO Z sequence, this may reject (-6/-10); the diagnostics surface
/// the code for operator follow-up.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_7_z_report() {
    if !live_armed("W4-Z3 Smoke 7") {
        return;
    }
    let Some(ek) = load_signing_key("W4-Z3 Smoke 7") else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    println!("\n=== W4-Z3 Smoke 7: LIVE Z_REPORT (close shift) → DPS ===");
    println!("FN: {fiscal_number}  TN: {LIVE_TN}\n");

    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke 7 FAIL: connect: {e:?}"));

    let fn_sign = sign_fn_blob(&ek, &fiscal_number);
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        println!(
            "  pre statusRro: open_shift={} online={}",
            s.open_shift, s.online
        );
    }
    let ack = match channel.last_chk(&fn_sign).await {
        Ok(a) => a,
        Err(DpsError::Server { code: -4, message }) => {
            println!("Smoke 7 SKIP: rate-limit (-4): {message}");
            return;
        }
        Err(e) => panic!("Smoke 7 FAIL: lastChk: {e:?}"),
    };

    let (_dir, app) = boot_offline_app().await;
    seed_fn_config_live(app.db(), &fiscal_number).await;
    seed_open_shift_and_node_piece4(app.db(), &fiscal_number).await;
    match seed_mac_from_lastchk(app.db(), &fiscal_number, &ack).await {
        Some(hex) => println!("  MAC seed from lastChk: {hex}"),
        None => println!("  genesis (empty data_sign)"),
    }

    let business_ts = iso_now();
    let (doc, req_id) = seed_z_report_prepared(app.db(), &fiscal_number, 0x02, &business_ts).await;
    seed_inbox_processing_generic(
        app.db(),
        &fiscal_number,
        &req_id,
        "Z_REPORT",
        Z_REPORT_PAYLOAD_JSON,
    )
    .await;

    let signing_ctx = live_signing_ctx(&ek);
    let drive_fn_sign = sign_fn_blob(&ek, &fiscal_number);
    let deps = ReconciliationRuntime::single_fn(RuntimeView {
        dps: &channel,
        signing_ctx: &signing_ctx,
        fn_sign: &drive_fn_sign,
    });
    app.reconcile_pending_with(deps)
        .await
        .expect("reconcile_pending_with green");

    let state = doc_state_piece4(app.db(), doc).await;
    let server_fiscal_no: Option<String> =
        sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
            .bind(doc)
            .fetch_one(app.db())
            .await
            .unwrap();
    println!("  post-reconcile: state={state} server_fiscal_no={server_fiscal_no:?}");
    print_live_diagnostics(app.db(), doc).await;

    assert_eq!(
        state, "SENT",
        "Z_REPORT must reach SENT (DPS ACK). If it rejected with a Z-sequence code (-6/-10), the \
         Z report NUMBER needs to match the FN's per-RRO sequence — see diagnostics."
    );
    assert!(
        server_fiscal_no.is_some(),
        "ACK must populate server_fiscal_no"
    );
    println!("Smoke 7 PASS: LIVE Z_REPORT fiscalized — shift closed — server_fiscal_no={server_fiscal_no:?}");
}

// ─── Piece 8 — T=112 ASK_OFFLINE_CODES ground-truth wire-contract probe ─────
//
// **LIVE piece (contacts DPS; capture-only, no local DB write beyond boot).**
// Submits a raw T=112 (ASK_OFFLINE_CODES) SERVICE document directly via
// `send_chk(ServiceChk)` to capture the live wire contract.  This is the
// first live contact for this command; the probe's job is to CAPTURE DPS
// behaviour (response shape, codes, chain impact), NOT to assert business
// outcomes.
//
// Wire format (WebCheck decompile SendingOfflineChecksRobot.cs:693-704,
// cross-confirmed Offlin.cs:224-235):
//   <RQ V = '1'><DAT FN='{fn}' TN='{tn}' ZN='' DI='{di}' V='1'>
//     <C T='112'><H SIZE='{size}'></H></C><TS>{ts}</TS></DAT><MAC>{mac}</MAC></RQ>
//   - T='112' (API v2; API v1 used T='12')
//   - DI: WebCheck uses MaxID("ksef")+1 (local DB counter); default 1 here.
//   - SIZE: how many codes to request; default 1 (minimal on test cabinet).
//   - TS: Kyiv-local-as-epoch (wall-clock local time cast to epoch seconds);
//     we use raw UTC epoch — DPS may reject with -8 (bad date), that is
//     itself ground-truth.
//   - MAC: sha256-hex of CMS-stripped previous check (same derivation as
//     the harness `seed_mac_from_lastchk`); empty string for a genesis FN.
// Submitted as `typCheck=3` (SERVICECHK proto enum) — the same slot that
// WebCheck uses (SubmitPtrRobot.SubmitCheck(text4, num4.ToString(), 3, dd)).
// `DpsCheckType::ServiceChk` maps to `SERVICECHK=3` in fiscal_server.proto.
//
// Bracket: pre-lastChk + pre-statusRro → send → post-lastChk + post-statusRro
// so we learn whether T=112 advances the MAC chain / consumes anything.
//
// Pass conditions:
//   - ANY definitive DPS response (OK or server-level reject) = PASS.
//     A reject code IS valuable ground truth; printed with GROUND-TRUTH: prefix.
//   - FAIL only on Transport/connect errors.
//
// Env overrides (no recompile needed for iteration):
//   PRRO_LIVE_DPS_T112_DI   — document index (integer, default 1)
//   PRRO_LIVE_DPS_T112_SIZE — number of codes to request (integer, default 1)

/// **Smoke 8 — T=112 ASK_OFFLINE_CODES ground-truth wire probe**.
///
/// Sends a raw T=112 SERVICE doc to live DPS and captures the response
/// contract: response code, message, and CMS-stripped inner XML with
/// parsed `<ID>` offline-code elements.  Pre/post `lastChk` + `statusRro`
/// bracket reveals whether T=112 advances the MAC chain.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_8_ask_offline_codes() {
    if !live_armed("Smoke 8 (T=112 ASK_OFFLINE_CODES)") {
        return;
    }
    let Some(ek) = load_signing_key("Smoke 8 (T=112 ASK_OFFLINE_CODES)") else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    let tn = LIVE_TN;

    // DI and SIZE can be overridden without recompiling.
    let di: i64 = std::env::var("PRRO_LIVE_DPS_T112_DI")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let size: u32 = std::env::var("PRRO_LIVE_DPS_T112_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);

    println!("\n=== Smoke 8: T=112 ASK_OFFLINE_CODES ground-truth wire probe ===");
    println!("FN: {fiscal_number}  TN: {tn}  DI: {di}  SIZE: {size}");
    println!("Endpoint: {host}\n");

    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke 8 FAIL: connect: {e:?}"));
    let fn_sign = sign_fn_blob(&ek, &fiscal_number);

    // ── PRE-BRACKET: lastChk + statusRro ────────────────────────────────
    println!("--- PRE-BRACKET ---");
    let pre_ack = match channel.last_chk(&fn_sign).await {
        Ok(a) => {
            println!(
                "  pre lastChk: id={:?} id_sign={} bytes data_sign={} bytes",
                a.id,
                a.id_sign.len(),
                a.data_sign.len()
            );
            a
        }
        Err(DpsError::Server { code: -4, message }) => {
            println!(
                "Smoke 8 SKIP: DPS rate-limit (-4) on pre-lastChk: {message}. Cool down 5+ min."
            );
            return;
        }
        Err(DpsError::Transport(msg)) => {
            panic!("Smoke 8 FAIL: Transport on pre-lastChk: {msg}");
        }
        Err(e) => {
            // Non-transport, non-rate-limit errors on lastChk: log and proceed
            // with genesis MAC (empty), so the T=112 send still happens.
            println!("  pre lastChk non-fatal error (will assume genesis MAC): {e:?}");
            CheckAck {
                id: String::new(),
                id_sign: vec![],
                data_sign: vec![],
            }
        }
    };
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        println!(
            "  pre statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        );
    }

    // ── MAC derivation (same model as harness seed_mac_from_lastchk) ────
    // MAC = sha256-hex of the CMS-stripped previous check bytes.
    // Empty for a genesis FN (no previous check).
    let mac_hex: String = if pre_ack.data_sign.is_empty() {
        println!("  FN genesis (empty data_sign) — T=112 carries empty <MAC>");
        String::new()
    } else {
        let inner = extract_econtent(&pre_ack.data_sign)
            .unwrap_or_else(|e| panic!("Smoke 8: CMS-strip pre data_sign: {e}"));
        let mac: [u8; 32] = Sha256::digest(&inner).into();
        let hex = hex_lower(&mac);
        println!(
            "  MAC derived from pre-lastChk: {hex} ({} B data_sign → {} B inner)",
            pre_ack.data_sign.len(),
            inner.len()
        );
        hex
    };

    // ── T=112 XML construction (WebCheck-faithful) ───────────────────────
    // <TS>: NOT epoch. WebCheck's `All.СurrentCompDate()` (All.cs:531-535)
    // formats local (Kyiv) wall-clock time as the number `yyyyMMddHHmmss`
    // and that long goes verbatim into <TS>.  GROUND-TRUTH (live probes,
    // 2026-07-07): raw-UTC epoch and Kyiv-local epoch both drew -8 (bad
    // date) with chain_changed=false.
    // The gRPC envelope `date_time` stays Kyiv-local-as-EPOCH — the
    // production `kyiv_local_epoch` convention proven live by smokes 5b/6/7.
    let utc_secs: i64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after UNIX_EPOCH")
        .as_secs() as i64;
    let (ts, epoch_kyiv): (i64, i64) = {
        use chrono::{Datelike, Offset, TimeZone, Timelike};
        let now = chrono::Utc
            .timestamp_opt(utc_secs, 0)
            .single()
            .expect("valid epoch");
        let kyiv = now.with_timezone(&chrono_tz::Europe::Kiev);
        let comp_date: i64 = format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}",
            kyiv.year(),
            kyiv.month(),
            kyiv.day(),
            kyiv.hour(),
            kyiv.minute(),
            kyiv.second()
        )
        .parse()
        .expect("yyyyMMddHHmmss fits i64");
        let epoch = utc_secs + i64::from(kyiv.offset().fix().local_minus_utc());
        (comp_date, epoch)
    };

    // Note the space in V = '1' on the RQ tag — exact byte order from decompile.
    // Single-line format string: rustfmt does not break string literals, so the
    // XML content is byte-exact with no injected whitespace.
    #[rustfmt::skip]
    let t112_xml = format!(
        "<RQ V = '1'><DAT FN='{fn}' TN='{tn}' ZN='' DI='{di}' V='1'><C T='112'><H SIZE='{size}'></H></C><TS>{ts}</TS></DAT><MAC>{mac}</MAC></RQ>",
        fn = fiscal_number, tn = tn, di = di, size = size, ts = ts, mac = mac_hex,
    );
    // XML contains no secrets (FN/TN are public fiscal identifiers).
    println!("  T=112 XML:\n    {t112_xml}");

    // ── Sign the XML bytes with ATTACHED CAdES-BES ───────────────────────
    // Same crypto profile as real receipt submission (sign_fn_blob uses
    // signing_cert() + DstuInProcessSigner + CmsProfile::Dstu4145WithGost34311Pb).
    // Here we sign the XML bytes instead of the FN string.
    let cert_der: &[u8] = ek
        .signing_cert()
        .expect("JKS must carry a signing certificate (KeyUsage=digitalSignature)");
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&ek.param_d[..], curve.mod_words);
    let signer_inner = DstuInProcessSigner::new(d);
    let cms_signer = CmsSigner {
        cert_der,
        signer: &signer_inner,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    };
    let signed_der = cms_signer
        .sign_with(
            t112_xml.as_bytes(),
            CmsBuildOptions {
                attached: true,
                signing_time: Some(SystemTime::now()),
            },
        )
        .expect("T=112 XML ATTACHED CAdES-BES sign must succeed")
        .cms_der;
    println!(
        "  Signed T=112 blob: {} bytes (ATTACHED CAdES-BES over {} bytes XML)",
        signed_der.len(),
        t112_xml.len()
    );

    // ── Send via send_chk (ServiceChk = SERVICECHK = typCheck=3) ────────
    // DpsCheckType::ServiceChk → gen::check::Type::Servicechk → proto int 3
    // — matches SubmitPtrRobot.SubmitCheck(..., typCheck=3, ...) in WebCheck.
    // Check.date MUST equal the XML <TS> byte-for-byte: WebCheck passes the
    // SAME `dd` long (yyyyMMddHHmmss, СurrentCompDate) both into <TS> and
    // into the gRPC submit (SubmitPtrRobot.cs:74/83, no conversion), and the
    // DPS -8 decode reads "XML дата не відповідає Check.date".
    let _ = epoch_kyiv; // kyiv-epoch retained for diagnostics only
    let envelope = CheckEnvelope {
        rro_fn: fiscal_number.clone(),
        date_time: ts,
        check_sign: signed_der,
        local_number: di as i32,
        check_type: DpsCheckType::ServiceChk,
        id_offline: String::new(),
        id_cancel: String::new(),
    };

    println!("\n--- SENDING T=112 (DI={di} SIZE={size}) ---");
    let send_result = channel.send_chk(envelope).await;
    println!("  send_chk raw result: {send_result:?}");

    // ── Response parsing ─────────────────────────────────────────────────
    // Parse <ID> offline-code elements from CMS-stripped data_sign if present
    // (WebCheck SendingOfflineChecksRobot.cs:659-667).
    let ground_truth_summary = match &send_result {
        Ok(ack) => {
            println!(
                "  T=112 DPS OK: id={:?} id_sign={} bytes data_sign={} bytes",
                ack.id,
                ack.id_sign.len(),
                ack.data_sign.len()
            );
            if !ack.data_sign.is_empty() {
                match extract_econtent(&ack.data_sign) {
                    Ok(inner_bytes) => {
                        let inner_xml = String::from_utf8_lossy(&inner_bytes);
                        println!(
                            "  response inner XML ({} bytes): {inner_xml}",
                            inner_bytes.len()
                        );
                        let id_count = inner_xml.matches("<ID>").count();
                        println!("  <ID> offline-code elements: {id_count}");
                        format!(
                            "DPS ACCEPTED T=112 id={:?} offline_codes={id_count}",
                            ack.id
                        )
                    }
                    Err(e) => {
                        // data_sign present but not valid CMS — log raw bytes (truncated).
                        let raw_preview: String = ack
                            .data_sign
                            .iter()
                            .take(64)
                            .map(|b| format!("{b:02x}"))
                            .collect();
                        println!(
                            "  data_sign is not a CMS blob (raw hex first 64B): {raw_preview}  strip-err: {e}"
                        );
                        format!(
                            "DPS ACCEPTED T=112 id={:?} data_sign-not-CMS err={e}",
                            ack.id
                        )
                    }
                }
            } else {
                println!("  data_sign empty — no offline codes in payload");
                format!("DPS ACCEPTED T=112 id={:?} data_sign=empty", ack.id)
            }
        }
        Err(DpsError::Server { code, message }) => {
            println!("  T=112 DPS server reject: code={code} message={message:?}");
            format!("DPS REJECTED T=112 code={code} msg={message:?}")
        }
        Err(DpsError::Authorization {
            code,
            kind,
            message,
        }) => {
            println!(
                "  T=112 DPS authorization reject: code={code} kind={kind:?} message={message}"
            );
            format!("DPS AUTH-REJECTED T=112 code={code} kind={kind:?}")
        }
        Err(DpsError::Transport(msg)) => {
            panic!("Smoke 8 FAIL: Transport error on T=112 send_chk (wire broken): {msg}");
        }
        Err(e) => {
            panic!("Smoke 8 FAIL: unexpected error kind on T=112 send_chk: {e:?}");
        }
    };

    // ── POST-BRACKET: lastChk + statusRro ────────────────────────────────
    println!("\n--- POST-BRACKET ---");
    if let Ok(a) = channel.last_chk(&fn_sign).await {
        let chain_changed = a.data_sign.len() != pre_ack.data_sign.len() || a.id != pre_ack.id;
        println!(
            "  post lastChk: id={:?} id_sign={} bytes data_sign={} bytes  chain_changed={chain_changed}",
            a.id,
            a.id_sign.len(),
            a.data_sign.len()
        );
    }
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        println!(
            "  post statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        );
    }

    println!("\nGROUND-TRUTH: {ground_truth_summary}");
    println!("Smoke 8 PASS: T=112 wire contract captured — DPS responded definitively.");
}

// ─── Smoke 9 — B9 acceptance: offline drain carries `<MAC ID>` (INV-11) ──────
//
// **LIVE piece (mutates the FN chain).**  The end-to-end acceptance test for
// the B9 fix (`<MAC ID='{offline_dps_code}'>` in the SIGNED offline bytes).
// Pre-B9 the gateway emitted a BARE `<MAC>` on offline receipts and every
// offline drain was rejected by live DPS with `-9 "not ID in MAC"`
// (INV-11 blocker, proven live + against WebCheck).  This smoke proves the
// drain is now ACCEPTED.
//
// It drives the FULL offline path through the REAL production write-path — no
// hand-seeding of `fs_mode`/`offline_dps_code` on the doc (that is exactly what
// B9's `stage_sign` stamp-at-sign is supposed to do itself):
//
//   1. LIVE T=112 ASK_OFFLINE_CODES (smoke-8 model) → extract FOUR REAL
//      DPS-issued opaque `<ID>` offline codes.  The `<MAC ID>` MUST be a real
//      DPS code, so we seed the pool from the wire, not a synthetic value.
//      B10 note: an offline session lazily mints a DocType=9 BEGIN boundary as
//      its FIRST offline doc and a DocType=10 END boundary at drain — so one
//      offline SELL costs THREE pool codes (BEGIN + the SELL + END).  We fetch
//      4 for margin so none of the three legs hits pool-exhaustion.
//   2. `offline_sessions::insert_dps_codes_tx` → the real codes land in the
//      `offline_codes` pool (the T=112 / migration-028 `dps_code` column).
//   3. `admin::go_offline` → node OFFLINE + an OPEN offline session (the
//      production surface; requires node ONLINE first).
//   4. A PREPARED **offline-origin** (`fs_mode='OFFLINE'`) SELL doc is driven
//      through `reconcile_pending_with`.  Node is OFFLINE + session OPEN +
//      pool non-empty, so `stage_sign`'s conditional stamp-at-sign
//      (`resolve_offline_dps_code`) acquires the real code and emits
//      `<MAC ID='{code}'>` in the SIGNED bytes; `stage_offline_ack` drives the
//      doc to `OFFLINE_LOCAL_ACK`.  NO DPS contact on this leg (stub channel).
//   5. `admin::go_online` → the backlog is drainable.
//   6. `App::drain_offline_backlog_scheduled` re-sends the `<MAC ID>`-stamped
//      SIGNED doc to LIVE DPS.
//   7. **ACCEPTANCE ASSERT:** the drained doc reaches an accepted terminal
//      (`SENT`/`KVT1`/`KVT2`/`ACK`) and is NOT rejected with `-9`
//      "not ID in MAC" (the pre-B9 failure).  The DPS response / trace is
//      printed for evidence.
//
// Gate: feature `live-dps` + `#[ignore]` + `PRRO_LIVE_DPS=1` + real JKS (this
// CONTACTS live DPS on the T=112 fetch AND the drain, so the full triple gate
// applies).

/// Stub `DpsChannel` for the OFFLINE reconcile leg (step 4): node is OFFLINE,
/// so `send_or_offline` takes the OFFLINE branch and NEVER touches the wire.
/// Every method panics — proving the offline reconcile consults NO DPS method
/// (the stamp-at-sign + offline-ack are pure-local).  The LIVE drain (step 6)
/// uses the real `GrpcDpsChannel`, not this stub.
struct StubOfflineAckDps;

#[async_trait]
impl DpsChannel for StubOfflineAckDps {
    async fn send_chk(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!(
            "smoke 9 offline reconcile: send_chk must NOT be invoked — node is OFFLINE \
             so stage_send is not reached (drain uses the real channel, not this stub)"
        )
    }
    async fn last_chk(&self, _: &CheckSignBlob) -> Result<CheckAck, DpsError> {
        unreachable!("smoke 9 offline reconcile: last_chk must not be invoked")
    }
    async fn ping(&self, _: CheckEnvelope) -> Result<CheckAck, DpsError> {
        unreachable!("smoke 9 offline reconcile: ping must not be invoked")
    }
    async fn status_rro(&self, _: &CheckSignBlob) -> Result<StatusSnapshot, DpsError> {
        unreachable!("smoke 9 offline reconcile: status_rro must not be invoked")
    }
    async fn info_rro(&self, _: &CheckSignBlob) -> Result<RroInfo, DpsError> {
        unreachable!("smoke 9 offline reconcile: info_rro must not be invoked")
    }
    async fn ask_offline_codes(
        &self,
        _: prro::transports::dps::dto::CheckEnvelope,
    ) -> Result<
        prro::transports::dps::dto::OfflineCodesResponse,
        prro::transports::dps::error::DpsError,
    > {
        unreachable!("smoke 9 offline reconcile: ask_offline_codes must not be invoked")
    }
}

// ── Smoke 9 production-write-path harness (mirror of b10_offline_session_
//    handshake.rs, but wiring the LIVE operator key + a real live drain) ──────
//
// The whole point of smoke 9 is to exercise the B10 Pattern-C wire handshake
// end-to-end: the offline docs are driven through the REAL production ingress
// so `ensure_offline_session_begin` fires and the lazy DocType=9 BEGIN mints as
// the FIRST offline doc.  We therefore build a `BindingsRegistry` whose
// per-FN signing context is the LIVE operator key (`live_signing_ctx`) and
// whose (shared) DPS channel is the OFFLINE stub — because while the node is
// OFFLINE the drives never touch the wire (BEGIN + SHIFT_OPEN + SELL all land
// OFFLINE_LOCAL_ACK).  The LIVE `GrpcDpsChannel` is used ONLY for the T=112
// fetch and the final drain, exactly as before.

/// Live-key `OperatorKeyLoader`: every `load` hands back a `SigningContext`
/// built over the REAL operator EDS key (`live_signing_ctx`), so the production
/// write-path signs the offline docs with the exact native CMS profile the
/// live drain then sends to DPS.  `ExtractedKey` is `Clone`, so the loader owns
/// a clone and can rebuild the context per operator without re-reading the JKS.
struct Smoke9LiveKeyLoader {
    ek: ExtractedKey,
}

#[async_trait]
impl OperatorKeyLoader for Smoke9LiveKeyLoader {
    async fn load(
        &self,
        _operator_id: &str,
        _key_path: &Path,
        _password: &[u8],
    ) -> Result<SigningContext, KeyLoadFailure> {
        Ok(live_signing_ctx(&self.ek))
    }
}

/// FN config for the LIVE FN with the REAL taxpayer code + `offline_enabled`,
/// so `ensure_offline_session_begin` / the offline path are wired.  Mirrors
/// `b10::fn_config` but for the live cabinet FN.
fn smoke9_fn_config(fn_id: &str) -> fn_cfg::NewFnConfig {
    fn_cfg::NewFnConfig {
        fiscal_number: fn_id.to_string(),
        tax_number: LIVE_TN.to_string(),
        vat_payer_inn: None,
        fiscal_mode: FiscalMode::Test,
        org_name: None,
        point_name: None,
        org_address: None,
        tsp_enabled: false,
        offline_enabled: true,
        national_check_enabled: false,
        min_offline_codes: 0,
        max_offline_codes: 0,
    }
}

/// Build the boot-time `BindingsRegistry` for the LIVE FN: seed the FN config +
/// one operator row (secure DB), then `build_from_db` with the live-key loader
/// and the OFFLINE stub DPS channel.  Mirrors `b10::build_registry`.
async fn smoke9_build_registry(app: &App, fn_id: &str, ek: &ExtractedKey) -> BindingsRegistry {
    fn_cfg::insert(app.db(), &smoke9_fn_config(fn_id))
        .await
        .expect("seed FN config for smoke 9 registry");
    ops_repo::insert(
        app.db_secure(),
        &ops_repo::NewOperator {
            operator_id: "OP-SMOKE9".into(),
            fiscal_number: fn_id.into(),
            name: "GALCHUN MYKOLA DMYTROVYCH".into(),
            key_path: "/tmp/smoke9-key.dat".into(),
            key_pass_enc: Coding::encode(b"unused-live-loader-ignores-pass")
                .expect("encode placeholder password"),
        },
    )
    .await
    .expect("seed operator for smoke 9 registry");
    // The offline drives never send (node OFFLINE) — the stub panics on every
    // DPS method, proving the offline reconcile is pure-local; the live drain
    // uses the real `GrpcDpsChannel`, not this stub.
    let offline_dps: Arc<dyn DpsChannel> = Arc::new(StubOfflineAckDps);
    let loader = Smoke9LiveKeyLoader { ek: ek.clone() };
    BindingsRegistry::build_from_db(app.db_secure(), app.db(), offline_dps, &loader)
        .await
        .expect("build_from_db for smoke 9")
}

/// A minimal SELL payload with NO `tax_group_1` (so it drives cleanly through
/// production `stage_acquire` without seeded `tax_groups` — the B10 handshake,
/// not tax translation, is smoke 9's subject; smoke 6 proves extended tax live).
const SMOKE9_SELL_PAYLOAD_JSON: &str = r#"{"items":[{"code":"item-1","name":"Test item","price_kop":15000,"quantity_thousandths":1000,"sum_kop":15000}],"payments":[{"name":"Cash","sum_kop":15000,"type_code":"0"}]}"#;
const SMOKE9_SELL_TOTAL_KOP: i64 = 15000;

/// Build a fresh NEW inbox entry for a production-path drive.  `business_ts` is
/// stamped via `offline_business_ts()` — the CURRENT instant (`iso_now`, the -8
/// fix) by default, or `now - N s` when `PRRO_LIVE_DPS_OFFLINE_BACKDATE_SEC>0`
/// (simulate a real offline period).  Either way the SELL / SHIFT_OPEN docs
/// carry a self-consistent Kyiv wire date (`stage_sign`'s `<TS>` and
/// `stage_send`'s envelope `date_time` both derive from this same `business_ts`,
/// so a stale value would draw a DPS -8 "invalid XML date"; back-dating merely
/// moves the whole triplet consistently into DPS's clock past).
fn smoke9_entry(
    fn_id: &str,
    op: &str,
    payload: &str,
    idem: &str,
    total: Option<i64>,
) -> NewInboxEntry {
    let request_id: [u8; 16] = *RequestId::new().as_bytes();
    let payload_sha256_canonical: [u8; 32] = Sha256::digest(payload.as_bytes()).into();
    NewInboxEntry {
        request_id,
        fiscal_number: fn_id.to_string(),
        protocol: Protocol::Rest,
        operation_type: op.into(),
        idempotency_key: idem.into(),
        payload_json: payload.into(),
        payload_sha256_canonical,
        correlation_id: None,
        signed_by_cashier_id: Some("test-cashier".into()),
        driver_id: Some("drv-smoke9".into()),
        // CURRENT (the -8 fix) unless `PRRO_LIVE_DPS_OFFLINE_BACKDATE_SEC>0`, in
        // which case it is `now - N s` — simulating a real offline period so the
        // offline SHIFT_OPEN + SELL are dated safely in DPS's clock past (the
        // BEGIN's `business_ts` = the session `opened_at`, back-dated in lockstep
        // right after `go_offline`, so all three offline docs agree).
        business_ts: Some(offline_business_ts()),
        total_sum_kop: total,
    }
}

/// Insert a NEW inbox row and run the production write-path over it — the same
/// `drive(...)` mechanism as `b10_offline_session_handshake.rs`.  `wp.fiscalize`
/// is `InlineWritePath::fiscalize` → `inline::run` → `run_staged` →
/// `ensure_offline_session_begin` (lazy DocType=9 BEGIN mint).
async fn smoke9_drive(
    wp: &dyn WritePathEntry,
    pool: &SqlitePool,
    e: NewInboxEntry,
) -> Result<FiscalOutcome, prro::runtime::ingress::seam::FiscalError> {
    let row: InboxRow = match inbox::insert(pool, &e).await.unwrap() {
        InboxInsertOutcome::Created(row) => row,
        other => panic!("smoke 9: expected a fresh Created inbox row, got {other:?}"),
    };
    wp.fiscalize(&row).await
}

/// Read `(lnd, doc_type, state)` for the FN, ordered by lnd — used to assert the
/// pre-drain backlog is `[BEGIN, SHIFT_OPEN, SELL]`.  Mirrors `b10::
/// doc_types_by_lnd`.
async fn smoke9_docs_by_lnd(pool: &SqlitePool, fn_id: &str) -> Vec<(i64, String, String)> {
    sqlx::query_as(
        "SELECT lnd, doc_type, state FROM fiscal_documents WHERE fiscal_number = ? ORDER BY lnd ASC",
    )
    .bind(fn_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Count `fiscal_documents` rows of a given `doc_type` for the FN.  Mirrors
/// `b10::count_doc_type` — used to pin exactly one DocType=9 BEGIN.
async fn count_doc_type(pool: &SqlitePool, dt: DocType) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_documents WHERE fiscal_number = ? AND doc_type = ?",
    )
    .bind(resolve_fn())
    .bind(dt)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// **Smoke 9 — B9 acceptance: live offline drain with `<MAC ID>` (INV-11)**.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_smoke_9_offline_drain_mac_id() {
    if !live_armed("Smoke 9 (B9 offline drain <MAC ID>)") {
        return;
    }
    let Some(ek) = load_signing_key("Smoke 9 (B9 offline drain <MAC ID>)") else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    let tn = LIVE_TN;

    println!("\n=== Smoke 9: B9 acceptance — offline drain carries <MAC ID> → live DPS ===");
    println!("FN: {fiscal_number}  TN: {tn}");
    println!("Endpoint: {host}");
    println!("Goal: DPS ACCEPTS the offline drain (NOT -9 \"not ID in MAC\") — B9 / INV-11");
    // -8 diagnostic knob: how far in the past the offline docs are dated.  0 =
    // "now" (near-instant drain → docs carry the current second, which DPS
    // intermittently rejects -8 as at/ahead of its clock); N>0 simulates a real
    // offline period so the offline docs sit safely in DPS's clock past.
    let offline_backdate = offline_backdate_secs();
    println!(
        "{ENV_OFFLINE_BACKDATE_SEC}={offline_backdate} → offline docs dated {offline_backdate} s in the past\n"
    );

    // ── Step 1: connect + pre-bracket ──────────────────────────────────────
    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke 9 FAIL: GrpcDpsChannel::connect: {e:?}"));
    let fn_sign = sign_fn_blob(&ek, &fiscal_number);

    println!("--- PRE-BRACKET ---");
    let pre_ack = match channel.last_chk(&fn_sign).await {
        Ok(a) => {
            println!(
                "  pre lastChk: id={:?} data_sign={} bytes",
                a.id,
                a.data_sign.len()
            );
            a
        }
        Err(DpsError::Server { code: -4, message }) => {
            println!(
                "Smoke 9 SKIP: DPS rate-limit (-4) on pre-lastChk: {message}. Cool down 5+ min."
            );
            return;
        }
        Err(DpsError::Transport(msg)) => {
            panic!("Smoke 9 FAIL: Transport on pre-lastChk: {msg}");
        }
        Err(e) => {
            println!("  pre lastChk non-fatal error (assume genesis MAC): {e:?}");
            CheckAck {
                id: String::new(),
                id_sign: vec![],
                data_sign: vec![],
            }
        }
    };
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        println!(
            "  pre statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        );
        if !s.open_shift {
            println!(
                "  WARN: no open shift on DPS — the SELL drain may draw a shift-state \
                 reject (run live_smoke_5b_shift_open first)"
            );
        }
    }

    // ── Step 2: boot throwaway App + build the PRODUCTION write-path ────────
    // The offline docs are driven through the REAL production ingress (so
    // `ensure_offline_session_begin` mints the DocType=9 BEGIN), NOT hand-seeded.
    // `smoke9_build_registry` seeds the FN config + one operator and wires the
    // LIVE operator key (signing) + the OFFLINE stub DPS (no wire while OFFLINE).
    println!("\n--- SEEDING APP (production write-path) ---");
    let (_dir, app) = boot_offline_app().await;
    let pool = app.db();
    let registry = smoke9_build_registry(&app, &fiscal_number, &ek).await;
    let write_path = production_write_path(app.clone(), Arc::new(registry));
    // Pattern C: the DPS test cabinet has `open_shift=false`, so we do NOT
    // pre-open a shift — the offline SHIFT_OPEN (driven below) creates it.  Seed
    // node_state ONLINE + Closed with `current_shift_id=NULL` and the
    // `backend_profile_id`/`transport_profile_id` BINDINGS set — mirroring the
    // proven `b10::seed_boot_baseline` raw INSERT.  The profile bindings are
    // REQUIRED: `stage_acquire`'s Step-3b profile-binding guard rejects any doc
    // (including the lazily-minted BEGIN) with `MissingProfileBinding` when
    // either profile column is NULL.  `node_state::upsert_initial` does NOT set
    // these columns (leaves them NULL) — which is why an earlier revision drove
    // the BEGIN's acquire into a reject and the SHIFT_OPEN fail-closed with
    // OFFLINE_SESSION_BEGIN_PENDING.  A raw INSERT (not upsert_initial) is the
    // minimal fix that matches the working unit test byte-for-byte.
    sqlx::query(
        "INSERT INTO node_state \
         (fiscal_number, mode, shift_state, current_shift_id, next_lnd, \
          backend_profile_id, transport_profile_id) \
         VALUES (?, ?, ?, NULL, 1, 'b', 't')",
    )
    .bind(&fiscal_number)
    .bind(NodeMode::Online)
    .bind(ShiftState::Closed)
    .execute(pool)
    .await
    .expect("seed node_state ONLINE/Closed + profile bindings (no pre-opened shift — Pattern C)");
    match seed_mac_from_lastchk(pool, &fiscal_number, &pre_ack).await {
        Some(hex) => println!("  pre-T112 MAC seeded into node_state: {hex}"),
        None => println!("  FN genesis (empty data_sign) — pre-T112 MAC is empty"),
    }

    // ── Step 3: fetch FOUR real T=112 opaque offline codes (smoke-8 model) ─
    // B10 costs 3 pool codes per one-SELL session (BEGIN + SELL + END); we ask
    // for 4 so none of the three legs hits pool-exhaustion.
    println!("\n--- T=112 FETCH (4 codes: BEGIN + SELL + END + margin) ---");
    let t112_di: i64 = std::env::var("PRRO_LIVE_DPS_T112_DI")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    // Build the T=112 request with the production builder (byte-exact / WebCheck).
    let pre_mac_hex: String = {
        let ns = node_state::get(pool, &fiscal_number)
            .await
            .expect("node_state row must exist")
            .expect("node_state row present");
        match ns.last_known_unsigned_xml_sha256 {
            None => String::new(),
            Some(arr) => hex_lower(&arr),
        }
    };
    let t112_req = prro::transports::dps::t112::build_t112_request(
        &fiscal_number,
        tn,
        t112_di as u32,
        4, // request 4 codes (B10: BEGIN + SELL + END + 1 margin)
        prro::transports::dps::t112::kyiv_comp_date_now(),
        &pre_mac_hex,
    )
    .expect("T=112 request builder must succeed for size=4");
    println!("  T=112 XML: {}", t112_req.xml);

    // Sign the T=112 XML with the live operator key (ATTACHED CAdES-BES).
    let cert_der: &[u8] = ek
        .signing_cert()
        .expect("JKS must carry a signing certificate");
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&ek.param_d[..], curve.mod_words);
    let signer_inner = DstuInProcessSigner::new(d);
    let cms_signer_t112 = CmsSigner {
        cert_der,
        signer: &signer_inner,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    };
    let t112_signed = cms_signer_t112
        .sign_with(
            t112_req.xml.as_bytes(),
            CmsBuildOptions {
                attached: true,
                signing_time: Some(SystemTime::now()),
            },
        )
        .expect("T=112 sign must succeed")
        .cms_der;

    let t112_envelope = CheckEnvelope {
        rro_fn: fiscal_number.clone(),
        date_time: t112_req.comp_date,
        check_sign: t112_signed,
        local_number: t112_di as i32,
        check_type: DpsCheckType::ServiceChk,
        id_offline: String::new(),
        id_cancel: String::new(),
    };

    // Send T=112 and parse ALL REAL opaque codes from the `<ID>` elements.
    // B10 needs ≥3 codes in the pool (BEGIN + SELL + END), so we harvest every
    // `<ID>` DPS issued for this SIZE=4 request, not just the first.
    let all_opaque_codes: Vec<String> = match channel.send_chk(t112_envelope).await {
        Ok(ack) => {
            println!(
                "  T=112 DPS OK: id={:?} data_sign={} bytes",
                ack.id,
                ack.data_sign.len()
            );
            if ack.data_sign.is_empty() {
                println!(
                    "Smoke 9 SKIP: T=112 returned an empty data_sign — no offline code \
                          issued (cannot seed a REAL <MAC ID>). Re-run when DPS issues codes."
                );
                return;
            }
            let inner_bytes = match extract_econtent(&ack.data_sign) {
                Ok(b) => b,
                Err(e) => {
                    println!(
                        "Smoke 9 SKIP: T=112 data_sign is not a CMS blob ({e}) — cannot \
                              extract a REAL offline code. Re-run when DPS issues codes."
                    );
                    return;
                }
            };
            let inner_xml = String::from_utf8_lossy(&inner_bytes);
            println!("  T=112 response inner XML: {inner_xml}");
            // Harvest every `<ID>…</ID>` opaque code (WebCheck emits one per
            // issued offline code).  `split("<ID>")` yields a leading non-code
            // prefix in element 0, so skip(1); each remaining segment starts
            // with the code text up to `</ID>`.
            let codes: Vec<String> = inner_xml
                .split("<ID>")
                .skip(1)
                .filter_map(|s| s.split("</ID>").next())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if codes.is_empty() {
                println!(
                    "Smoke 9 SKIP: no <ID> offline code in the T=112 response — nothing \
                          to seed as <MAC ID>. Re-run when DPS issues codes."
                );
                return;
            }
            if codes.len() < 3 {
                println!(
                    "Smoke 9 SKIP: T=112 issued only {} offline code(s); B10 needs ≥3 \
                          (BEGIN + SELL + END). Re-run when DPS issues a full batch.",
                    codes.len()
                );
                return;
            }
            println!("  REAL OPAQUE OFFLINE CODES ({}): {codes:?}", codes.len());
            codes
        }
        Err(DpsError::Server { code: -4, message }) => {
            println!("Smoke 9 SKIP: DPS rate-limit (-4) on T=112: {message}. Cool down 5+ min.");
            return;
        }
        Err(DpsError::Server { code, message }) => {
            panic!(
                "Smoke 9 FAIL: T=112 ASK_OFFLINE_CODES rejected (code={code} msg={message:?}) — \
                 cannot obtain a REAL offline code to prove the drain. This smoke needs a live \
                 T=112 to issue at least one code."
            );
        }
        Err(DpsError::Transport(msg)) => {
            panic!("Smoke 9 FAIL: Transport error on T=112 send_chk: {msg}");
        }
        Err(e) => {
            panic!("Smoke 9 FAIL: unexpected error on T=112 send_chk: {e:?}");
        }
    };
    // The SELL is stamped with *some* pool code (BEGIN consumes the first, so
    // the SELL usually draws a later one); assertions below accept any member
    // of `all_opaque_codes`, not specifically the first.
    let expected_insert = all_opaque_codes.len() as u64;

    // ── Step 4: seed the code pool + advance node_state MAC past the T=112 ──
    // The T=112 request advanced the DPS chain; the offline SELL's `<MAC>`
    // previous-hash must chain off sha256(t112_xml) so the drained receipt is
    // MAC-consistent with what DPS expects.  B10: all harvested codes go into
    // the pool so the BEGIN + SELL + END legs each have a code to acquire.
    {
        use prro::db::repositories::offline_sessions;
        let fn_owned = fiscal_number.clone();
        let codes = all_opaque_codes.clone();
        let summary = with_immediate(pool, move |tx| {
            Box::pin(async move {
                offline_sessions::insert_dps_codes_tx(tx, &fn_owned, &codes)
                    .await
                    .map_err(anyhow::Error::from)
            })
        })
        .await
        .expect("insert_dps_codes_tx");
        println!(
            "  offline_codes pool: inserted={} deduped={} (real dps_codes, requested 4)",
            summary.inserted, summary.deduped
        );
        assert_eq!(
            summary.inserted, expected_insert,
            "every REAL T=112 offline code harvested from the wire must be inserted into the pool \
             (need ≥3 for BEGIN + SELL + END)"
        );
    }
    let post_t112_seed: [u8; 32] = Sha256::digest(t112_req.xml.as_bytes()).into();
    node_state::seed_prevhash(pool, &fiscal_number, &post_t112_seed)
        .await
        .expect("seed_prevhash for post-T=112 MAC");
    println!(
        "  post-T112 MAC seeded into node_state: {}",
        hex_lower(&post_t112_seed)
    );

    // ── Step 4b: wait for DPS to settle its chain tip to sha256(t112_xml) ──
    // DPS advances its chain tip after a T=112 ASK_OFFLINE_CODES *lazily*, not
    // synchronously with the request.  A back-to-back offline drain races that
    // advance and is rejected -15 ERROR_BAD_HASH_PREV (DPS's stored tip is still
    // the pre-T112 hash while our drained SELL already chains off
    // `post_t112_seed`).  This is a live-timing artifact, NOT a code bug — the
    // real flow (T=112 online, then offline sells, then a later drain) has a
    // large gap and never races.  Poll `last_chk` until DPS's tip ==
    // `post_t112_seed` before going offline so the drain matches.
    {
        let mut settled = false;
        for attempt in 1..=20u32 {
            if let Ok(ack) = channel.last_chk(&fn_sign).await {
                if !ack.data_sign.is_empty() {
                    if let Ok(inner) = extract_econtent(&ack.data_sign) {
                        let tip: [u8; 32] = Sha256::digest(&inner).into();
                        if tip == post_t112_seed {
                            println!("  DPS tip settled to post-T112 seed after {attempt} poll(s)");
                            settled = true;
                            break;
                        }
                        println!(
                            "  poll {attempt}: DPS tip = {} (want {}) — waiting for T=112 advance",
                            hex_lower(&tip),
                            hex_lower(&post_t112_seed)
                        );
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        if !settled {
            println!(
                "Smoke 9 SKIP: DPS chain tip did not settle to the post-T112 seed within the \
                 poll window (~100s) — T=112 advance latency exceeded budget"
            );
            return;
        }
    }

    // ── Step 5: admin::go_offline → node OFFLINE + OPEN session ────────────
    prro::admin::go_offline(
        pool,
        &fiscal_number,
        "live smoke 9 — B9 offline drain acceptance",
    )
    .await
    .unwrap_or_else(|e| panic!("Smoke 9 FAIL: go_offline: {e:?}"));
    println!("  go_offline: node OFFLINE, offline_session OPEN");

    // ── Step 5b: (optional) back-date the OPEN session's `opened_at` ────────
    // `go_offline` stamps `opened_at = Utc::now()`.  The lazily-minted DocType=9
    // BEGIN reads THAT value (`mint_offline_session_begin`:
    //   SELECT opened_at FROM offline_sessions WHERE state='OPEN'`)
    // and stamps it as its own `business_ts` — so back-dating `opened_at` HERE
    // (after go_offline, BEFORE the first offline doc is driven/signed) makes the
    // BEGIN's frozen `<TS>` reflect the back-dated time.  We write an RFC-3339
    // string (the SAME shape/instant `offline_business_ts()` gives the SHIFT_OPEN
    // + SELL) so all THREE offline docs land ~N s in the past, consistently — a
    // SQLite `datetime('now',…)` value would NOT parse as RFC-3339 in
    // `format_kyiv_local`/`kyiv_local_epoch` and would desync the BEGIN.
    if offline_backdate > 0 {
        let backdated_opened_at = offline_business_ts();
        let updated = sqlx::query(
            "UPDATE offline_sessions SET opened_at = ? \
             WHERE fiscal_number = ? AND state = 'OPEN'",
        )
        .bind(&backdated_opened_at)
        .bind(&fiscal_number)
        .execute(pool)
        .await
        .expect("back-date offline_session opened_at")
        .rows_affected();
        assert_eq!(
            updated, 1,
            "exactly one OPEN offline_session must have its opened_at back-dated \
             (the BEGIN's business_ts is read from this row)"
        );
        println!(
            "  BEGIN back-date: offline_session.opened_at ← {backdated_opened_at} \
             (now - {offline_backdate}s; the DocType=9 BEGIN inherits this business_ts)"
        );
    }

    // ── Step 6: drive an offline SHIFT_OPEN then an offline SELL through the ──
    //            PRODUCTION write-path (Pattern C, full B10 wire sequence) ─────
    // The DPS test cabinet has `open_shift=false`, so we FIRST drive an offline
    // SHIFT_OPEN (it opens the shift locally + is the session's first offline
    // business doc → `ensure_offline_session_begin` lazily mints the DocType=9
    // BEGIN BEFORE it), THEN an offline SELL.  All three land OFFLINE_LOCAL_ACK
    // (node OFFLINE → no wire; the stub DPS panics if any method is touched).
    // The BEGIN takes the LOWEST offline lnd, so the pre-drain backlog ordered
    // by lnd is [BEGIN(9), SHIFT_OPEN, SELL] — the target Pattern-C shape.
    println!("\n--- OFFLINE DRIVES (production write-path) ---");
    let open_outcome = match smoke9_drive(
        &*write_path,
        pool,
        smoke9_entry(
            &fiscal_number,
            "SHIFT_OPEN",
            SHIFT_OPEN_PAYLOAD_JSON,
            "smoke9-idem-OPEN",
            None,
        ),
    )
    .await
    {
        Ok(o) => o,
        Err(e) => {
            // DIAGNOSTIC: the SHIFT_OPEN drive refuses (typically
            // OFFLINE_SESSION_BEGIN_PENDING) only when the lazily-minted BEGIN
            // failed to reach OFFLINE_LOCAL_ACK in one drive.  Surface WHERE the
            // BEGIN stalled so a live failure is actionable (a NULL profile
            // binding rejects the BEGIN at acquire; an empty pool refuses pre-mint;
            // a sign fault leaves it PREPARED/SIGNED).
            let begin_row: Option<(i64, String, Option<String>)> = sqlx::query_as(
                "SELECT lnd, state, offline_dps_code FROM fiscal_documents \
                 WHERE fiscal_number = ? AND doc_type = 'OFFLINE_SESSION_BEGIN' \
                 ORDER BY lnd ASC LIMIT 1",
            )
            .bind(&fiscal_number)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
            println!("  DIAGNOSTIC: SHIFT_OPEN drive refused: {e:?}");
            match &begin_row {
                Some((lnd, state, code)) => println!(
                    "  DIAGNOSTIC: BEGIN row present — lnd={lnd} state={state} offline_dps_code={code:?} \
                     (if not OFFLINE_LOCAL_ACK the BEGIN stalled here)"
                ),
                None => println!(
                    "  DIAGNOSTIC: NO BEGIN row — it was refused pre-mint (empty code pool) or \
                     rejected at acquire before any row minted (e.g. NULL profile bindings)"
                ),
            }
            // Tail the audit_log for the reject reason (e.g. profile_binding_missing).
            let audits: Vec<(String, Option<String>)> = sqlx::query_as(
                "SELECT event_type, event_payload_json FROM audit_log ORDER BY rowid DESC LIMIT 8",
            )
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            for (et, pj) in &audits {
                println!("  DIAGNOSTIC audit: {et}  {}", pj.as_deref().unwrap_or(""));
            }
            panic!(
                "Smoke 9 FAIL: offline SHIFT_OPEN drive refused ({e:?}) — the lazy DocType=9 BEGIN \
                 did not reach OFFLINE_LOCAL_ACK in one drive. See DIAGNOSTIC lines above."
            )
        }
    };
    assert_eq!(
        open_outcome.document_state,
        DocState::OfflineLocalAck,
        "offline SHIFT_OPEN must land OFFLINE_LOCAL_ACK"
    );
    println!("  offline SHIFT_OPEN → OFFLINE_LOCAL_ACK (BEGIN minted first)");

    let sell_outcome = smoke9_drive(
        &*write_path,
        pool,
        smoke9_entry(
            &fiscal_number,
            "SELL",
            SMOKE9_SELL_PAYLOAD_JSON,
            "smoke9-idem-SELL",
            Some(SMOKE9_SELL_TOTAL_KOP),
        ),
    )
    .await
    .expect("offline SELL drive must succeed");
    assert_eq!(
        sell_outcome.document_state,
        DocState::OfflineLocalAck,
        "offline SELL must land OFFLINE_LOCAL_ACK"
    );
    let doc_id = sell_outcome.document_id;
    println!(
        "  offline SELL → OFFLINE_LOCAL_ACK: doc_id={}",
        hex_lower(doc_id.as_bytes())
    );

    // ── Step 7: assert the pre-drain backlog is [BEGIN(9), SHIFT_OPEN, SELL] ──
    let backlog = smoke9_docs_by_lnd(pool, &fiscal_number).await;
    println!("  pre-drain backlog (by lnd): {backlog:?}");
    let backlog_types: Vec<&str> = backlog.iter().map(|(_, dt, _)| dt.as_str()).collect();
    assert_eq!(
        backlog_types,
        vec!["OFFLINE_SESSION_BEGIN", "SHIFT_OPEN", "SELL"],
        "B10 Pattern-C pre-drain backlog must be exactly [BEGIN(9), SHIFT_OPEN, SELL]; got {backlog:?}"
    );
    assert_eq!(
        count_doc_type(pool, DocType::OfflineSessionBegin).await,
        1,
        "exactly one DocType=9 BEGIN must be lazily minted"
    );
    // All three offline docs must rest OFFLINE_LOCAL_ACK before the drain.
    for (lnd, dt, state) in &backlog {
        assert_eq!(
            state, "OFFLINE_LOCAL_ACK",
            "backlog doc (lnd={lnd}, {dt}) must be OFFLINE_LOCAL_ACK before the drain"
        );
    }

    // The SELL's stamped `offline_dps_code` — the code that rides in the SELL's
    // `<MAC ID>` (the B9 fix).  B10: BEGIN + SHIFT_OPEN consume earlier pool
    // codes, so the SELL is stamped with a *later* harvested code — assert it is
    // a REAL member of the T=112 batch (proving stamp-at-sign used a live DPS
    // code, not a synthetic one).
    let stamped_code: Option<String> =
        sqlx::query_scalar("SELECT offline_dps_code FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(pool)
            .await
            .unwrap();
    println!("  SELL offline_dps_code (rides in <MAC ID>): {stamped_code:?}");
    let stamped_code = stamped_code.expect(
        "B9 stage_sign stamp-at-sign must stamp an offline_dps_code on the offline SELL (got NULL)",
    );
    assert!(
        all_opaque_codes.contains(&stamped_code),
        "B9 stage_sign stamp-at-sign must stamp one of the REAL T=112 offline codes \
         (not a synthetic one); stamped={stamped_code:?} harvested={all_opaque_codes:?}"
    );

    // Prove the SIGNED bytes carry `<MAC ID='{code}'>` — the B9 fix, in the
    // exact bytes that will be sent to DPS on the drain.  (Pre-B9 the offline
    // signed XML carried a bare `<MAC>` and DPS rejected the drain with -9.)
    let payload_xml_blob = read_document_file_piece4(pool, doc_id, "PAYLOAD_XML")
        .await
        .expect("stage_sign must have produced a PAYLOAD_XML artifact");
    let signed_xml_blob = read_document_file_piece4(pool, doc_id, "SIGNED_XML")
        .await
        .expect("stage_sign must have produced a SIGNED_XML artifact");
    let payload_xml = String::from_utf8_lossy(&payload_xml_blob);
    let expected_mac_id = format!("<MAC ID='{stamped_code}'>");
    assert!(
        payload_xml.contains(&expected_mac_id),
        "B9: the offline SELL PAYLOAD_XML must carry `<MAC ID='{stamped_code}'>` (not a bare \
         <MAC>) — this is the exact fix DPS validates on the drain; xml=\n{payload_xml}"
    );
    // The SIGNED bytes are an ATTACHED CMS whose eContent == PAYLOAD_XML, so the
    // `<MAC ID>` is inside the signature the drain sends.
    let inner =
        extract_econtent(&signed_xml_blob).expect("SIGNED_XML must be a parseable ATTACHED CMS");
    assert_eq!(
        inner, payload_xml_blob,
        "the SIGNED offline bytes' eContent must be byte-identical to the <MAC ID>-carrying \
         PAYLOAD_XML (the drain sends these exact bytes to DPS)"
    );
    println!(
        "  SIGNED offline XML carries `<MAC ID='{stamped_code}'>` — {} bytes signed",
        signed_xml_blob.len()
    );

    // ── Step 8: admin::go_online → backlog drainable ───────────────────────
    prro::admin::go_online(pool, &fiscal_number, "smoke 9 drain")
        .await
        .unwrap_or_else(|e| panic!("Smoke 9 FAIL: go_online: {e:?}"));
    println!("  go_online: node GOING_ONLINE — ready to drain");

    // ── Step 9: DRAIN the backlog to LIVE DPS ──────────────────────────────
    println!("\n--- DRAIN (live DPS — offline SELL with <MAC ID>) ---");
    let drain_fn_sign = sign_fn_blob(&ek, &fiscal_number);
    let drain_signing_ctx = live_signing_ctx(&ek);
    let drain_view = RuntimeView {
        dps: &channel,
        signing_ctx: &drain_signing_ctx,
        fn_sign: &drain_fn_sign,
    };
    let drain_result = app
        .drain_offline_backlog_scheduled(&fiscal_number, &drain_view)
        .await;

    // ── Step 10: capture verdict + B9 acceptance assert ────────────────────
    println!("\n--- DRAIN RESULT ---");
    let summary = match drain_result {
        Ok(prro::ScheduledDrainOutcome::Ran(summary)) => summary,
        Ok(prro::ScheduledDrainOutcome::SkippedBackoff { .. }) => {
            panic!("Smoke 9 FAIL: drain skipped due to backoff on the first run")
        }
        Err(e) => panic!("Smoke 9 FAIL: drain_offline_backlog_scheduled returned Err: {e:?}"),
    };
    println!(
        "  drain ran: backlog_before={} advanced_to_ack={} advanced_to_kvt1={} \
         held_at_kvt1={} held_at_sent={} failures={} er_queued={}",
        summary.backlog_size_before(),
        summary.advanced_to_ack(),
        summary.advanced_to_kvt1(),
        summary.held_at_kvt1(),
        summary.held_at_sent(),
        summary.per_doc_failures().len(),
        summary.er_redrive_queued(),
    );

    let final_state: String =
        sqlx::query_scalar("SELECT state FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(pool)
            .await
            .unwrap_or_else(|_| "UNKNOWN".into());
    let server_fiscal_no: Option<String> =
        sqlx::query_scalar("SELECT server_fiscal_no FROM fiscal_documents WHERE document_id = ?")
            .bind(doc_id)
            .fetch_one(pool)
            .await
            .unwrap();
    // The DPS reject code (if any) is recorded in transport_trace.server_status_code.
    let trace_code: Option<i64> = sqlx::query_scalar(
        "SELECT server_status_code FROM transport_trace \
         WHERE document_id = ? ORDER BY attempt_no DESC LIMIT 1",
    )
    .bind(doc_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .flatten();
    println!("  doc final state: {final_state}  server_fiscal_no={server_fiscal_no:?}  dps_code={trace_code:?}");
    print_live_diagnostics(pool, doc_id).await;
    println!("  offline_dps_code sent as <MAC ID>: {stamped_code:?}");

    // ── Step 10a: DocType=9 BEGIN vs SELL DATE DIAGNOSTICS (-8 investigation) ─
    //
    // The drain's FIRST doc is the DocType=9 OFFLINE_SESSION_BEGIN.  Live smoke 9
    // saw DPS reject that BEGIN with `wire_status_code=-8` on ~6/7 runs (accepted
    // 1/7).  WebCheck defines `-8` as "the signed XML `<TS>` date does not match
    // the wire `Check.date`" (SubmitPtr.cs:397).  A prior source trace VERIFIED
    // that our BEGIN's signed `<TS>` (`stage_sign::format_kyiv_local(business_ts)`)
    // and its wire `date_time` (`stage_send::kyiv_local_epoch(business_ts)`) BOTH
    // derive from the SAME stored `fiscal_documents.business_ts` and both truncate
    // to the same whole second — so they CANNOT diverge in our source.  This block
    // captures the EXACT wire bytes so the next `-8` run shows what DPS actually
    // rejects: for BOTH the BEGIN (rejected) and the SELL (accepted), side by side,
    // it prints (1) the stored `business_ts`, (2) the `<TS>` digits extracted from
    // the stored SIGNED_XML eContent — the exact wall-clock DPS receives, (3) the
    // envelope `date_time` computed via the SAME production path the drain uses
    // (`recover_kyiv_local_epoch`, a byte-verbatim mirror of the private
    // `stage_send::kyiv_local_epoch`), as raw epoch + UTC/Kyiv decode, (4) the
    // current wall clock + DPS's own chain clock (the `<TS>` inside the latest
    // `lastChk` check DPS returns — the closest thing the wire API exposes to
    // "DPS's clock"), and (5) the BEGIN's chain tip (`previous_hash`) + `lnd`, in
    // case `-8` correlates with chain state rather than date.  Print-only; no wire
    // calls beyond the read-only `lastChk` already made above (`pre_ack`).
    println!("\n--- DocType=9 BEGIN vs SELL DATE DIAGNOSTICS (-8 investigation) ---");

    // A print-only helper: for one doc row, surface (business_ts, signed <TS>,
    // envelope date_time via the production path).  `label` distinguishes BEGIN
    // (rejected) from SELL (accepted) in the side-by-side output.
    async fn print_date_triplet(pool: &SqlitePool, label: &str, doc: DocumentId) {
        // (1) stored business_ts — the single column BOTH the <TS> and the wire
        //     date_time derive from.
        let business_ts: Option<String> =
            sqlx::query_scalar("SELECT business_ts FROM fiscal_documents WHERE document_id = ?")
                .bind(doc)
                .fetch_optional(pool)
                .await
                .unwrap_or(None)
                .flatten();
        println!(
            "  [{label}] document_id={} business_ts(raw column)={business_ts:?}",
            hex_lower(doc.as_bytes())
        );
        // (2) the <TS> DPS actually receives — extracted from the SIGNED_XML's
        //     eContent (the ATTACHED-CMS inner XML that IS the signed check).  Fall
        //     back to PAYLOAD_XML if the signed artifact is missing.
        let signed_ts = match read_document_file_piece4(pool, doc, "SIGNED_XML").await {
            Some(signed) => match extract_econtent(&signed) {
                Ok(inner) => extract_ts_digits(&inner),
                Err(e) => {
                    println!("  [{label}] SIGNED_XML eContent extract failed: {e}");
                    None
                }
            },
            None => None,
        };
        let signed_ts = match signed_ts {
            Some(ts) => Some(ts),
            None => read_document_file_piece4(pool, doc, "PAYLOAD_XML")
                .await
                .and_then(|p| extract_ts_digits(&p)),
        };
        println!("  [{label}] signed <TS> (exact digits DPS reads): {signed_ts:?}");
        // (3) the wire envelope date_time via the SAME production path the drain
        //     uses (`recover_kyiv_local_epoch` ≡ `stage_send::kyiv_local_epoch`),
        //     printed as raw epoch + its UTC/Kyiv decode.  On a clean run the UTC
        //     decode of this epoch equals the signed <TS> above; a divergence here
        //     is exactly what DPS rejects with -8.
        match &business_ts {
            Some(bts) => {
                let epoch = recover_kyiv_local_epoch(bts);
                let (as_utc, as_kyiv) = decode_epoch_utc_and_kyiv(epoch);
                println!(
                    "  [{label}] wire date_time (kyiv_local_epoch): raw_epoch={epoch} \
                     decode_utc={as_utc} decode_kyiv={as_kyiv}"
                );
                println!(
                    "  [{label}] MATCH signed<TS>==decode_utc(date_time): {}",
                    signed_ts.as_deref() == Some(as_utc.as_str())
                );
            }
            None => println!(
                "  [{label}] no business_ts column — cannot compute envelope date_time"
            ),
        }
    }

    // (a) Locate the single DocType=9 OFFLINE_SESSION_BEGIN row for this FN, and
    //     print its date triplet + chain state.  The BEGIN is a SEPARATE
    //     `fiscal_documents` row from the SELL (`doc_id`).
    let begin_row: Option<(DocumentId, i64, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT document_id, lnd, previous_hash \
         FROM fiscal_documents \
         WHERE fiscal_number = ? AND doc_type = 'OFFLINE_SESSION_BEGIN' \
         ORDER BY lnd DESC LIMIT 1",
    )
    .bind(&fiscal_number)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    match &begin_row {
        None => println!(
            "  NO OFFLINE_SESSION_BEGIN row — the drain's first doc was never minted \
             (check the audit lines above)"
        ),
        Some((begin_doc, begin_lnd, begin_prev)) => {
            print_date_triplet(pool, "BEGIN(9)", *begin_doc).await;
            // (5) chain tip + lnd for the BEGIN — in case -8 correlates with chain
            //     state rather than date.
            println!(
                "  [BEGIN(9)] lnd={begin_lnd} previous_hash(chain tip)={}",
                begin_prev
                    .as_deref()
                    .map(hex_lower)
                    .unwrap_or_else(|| "<none/genesis>".into())
            );
            // The BEGIN's latest transport_trace (its DPS reject code, i.e. the -8).
            println!("  [BEGIN(9)] transport_trace + audit tail:");
            print_live_diagnostics(pool, *begin_doc).await;
        }
    }

    // (b) The SELL's date triplet, printed RIGHT NEXT TO the BEGIN's — DPS ACCEPTS
    //     the SELL, so a side-by-side diff shows whether the BEGIN's <TS> /
    //     date_time actually differ from the accepted SELL's.
    print_date_triplet(pool, "SELL(accepted)", doc_id).await;
    let sell_chain: Option<(i64, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT lnd, previous_hash FROM fiscal_documents WHERE document_id = ?",
    )
    .bind(doc_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    if let Some((sell_lnd, sell_prev)) = &sell_chain {
        println!(
            "  [SELL(accepted)] lnd={sell_lnd} previous_hash(chain tip)={}",
            sell_prev
                .as_deref()
                .map(hex_lower)
                .unwrap_or_else(|| "<none/genesis>".into())
        );
    }

    // (4) Reference clocks: current wall clock (UTC + Kyiv) and DPS's own chain
    //     clock.  `CheckAck` carries NO scalar timestamp, so the closest wire proxy
    //     for "DPS's clock" is the `<TS>` inside the previous check DPS returned on
    //     `lastChk` (`pre_ack.data_sign`, an ATTACHED CMS) — how far the BEGIN's
    //     <TS> sits from DPS's last-seen check time.  Genesis (empty data_sign)
    //     has no such check.
    {
        use chrono::Utc;
        use chrono_tz::Europe::Kiev;
        let now = Utc::now();
        println!(
            "  [now] Utc::now() utc={} kyiv={}",
            now.format("%Y%m%d%H%M%S"),
            now.with_timezone(&Kiev).format("%Y%m%d%H%M%S")
        );
        let dps_chain_ts = if pre_ack.data_sign.is_empty() {
            None
        } else {
            match extract_econtent(&pre_ack.data_sign) {
                Ok(inner) => extract_ts_digits(&inner),
                Err(e) => {
                    println!("  [dps-clock] pre_ack.data_sign eContent extract failed: {e}");
                    None
                }
            }
        };
        println!(
            "  [dps-clock] <TS> of DPS's latest check on lastChk (pre-drain tip): {dps_chain_ts:?} \
             (id={:?}, data_sign={} bytes)",
            pre_ack.id,
            pre_ack.data_sign.len()
        );
    }

    // ── Step 10b: DocType=10 END (OFFLINE_SESSION_END) reject diagnostics ────
    //
    // The B10 drain lazily mints a DocType=10 END boundary (`<C T='110'>`) AFTER
    // the backlog [BEGIN, SHIFT_OPEN, SELL] drains, to close the offline session
    // at DPS.  Live smoke 9 saw the backlog reach ACK (`-5`/`-8`/`-9` cleared)
    // but the drain finalize as `OFFLINE_DRAIN_PARTIAL finalized:false` with a
    // `per_doc_failures` entry classed `offline_session_end_hold` and a
    // `STAGE_SEND_REJECTED { retry_class: TerminalReject }` — the END was signed,
    // sent, and TERMINAL-rejected by DPS, but the smoke never surfaced the END's
    // DPS reject code.  This block prints WHY: the END row's state /
    // offline_dps_code / server_fiscal_no, its latest transport_trace
    // (outcome, dps_code, error_kind, error_message), and its SIGNED `<C T='110'>`
    // document — so the next live run reveals the reject reason instead of only
    // the failure CLASS.  The END is a SEPARATE `fiscal_documents` row (NOT
    // `doc_id`, which is the SELL) — we look it up by doc_type + cross-check the
    // `per_doc_failures` document_ids.
    println!("\n--- DocType=10 END (OFFLINE_SESSION_END) REJECT DIAGNOSTICS ---");
    // (i) Dump every per-doc drain failure `(document_id, failure_class)` — the
    //     END shows up here as `offline_session_end_hold`.
    for (fail_doc, fail_class) in summary.per_doc_failures() {
        println!(
            "  per_doc_failure: document_id={} failure_class={fail_class}",
            hex_lower(fail_doc.as_bytes())
        );
    }
    // (ii) Locate the single DocType=10 END row for this FN and print its
    //      row-level fiscal state.  `fiscal_documents` has NO per-row dps_code
    //      column (the DPS reject code lives in transport_trace) — so we print
    //      state / offline_dps_code (the `<MAC ID>` code) / server_fiscal_no here
    //      and the DPS code via `print_live_diagnostics` below.
    let end_row: Option<(DocumentId, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT document_id, state, offline_dps_code, server_fiscal_no \
         FROM fiscal_documents \
         WHERE fiscal_number = ? AND doc_type = 'OFFLINE_SESSION_END' \
         ORDER BY lnd DESC LIMIT 1",
    )
    .bind(&fiscal_number)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    match end_row {
        None => println!(
            "  NO OFFLINE_SESSION_END row — the drain never minted a DocType=10 END \
             (session-end mint refused before any row; check the audit lines above)"
        ),
        Some((end_doc_id, end_state, end_offline_code, end_sfn)) => {
            println!(
                "  END row: document_id={} state={end_state} \
                 offline_dps_code(<MAC ID>)={end_offline_code:?} server_fiscal_no={end_sfn:?}",
                hex_lower(end_doc_id.as_bytes())
            );
            // (iii) Latest transport_trace for the END: outcome_kind, the DPS
            //       reject CODE (server_status_code), error_kind, error_message —
            //       this is the `-N` reason the END was terminal-rejected.
            println!("  END transport_trace + audit tail:");
            print_live_diagnostics(pool, end_doc_id).await;
            // (iv) The END's SIGNED `<C T='110'>` document — mirror how the SELL's
            //      signed XML is surfaced above.  PAYLOAD_XML carries the human-
            //      readable `<C T='110'>` header + `<MAC ID>`; SIGNED_XML is the
            //      ATTACHED CMS actually sent on the drain (print its length +
            //      confirm its eContent == PAYLOAD_XML, as smoke 9 does for the
            //      SELL).  cp1251 payload but the `<C T>`/MAC region is ASCII, so
            //      lossy UTF-8 is fine for the printed substring.
            match read_document_file_piece4(pool, end_doc_id, "PAYLOAD_XML").await {
                Some(end_payload) => {
                    println!(
                        "  END PAYLOAD_XML (<C T='110'> boundary): {}",
                        String::from_utf8_lossy(&end_payload)
                    );
                    match read_document_file_piece4(pool, end_doc_id, "SIGNED_XML").await {
                        Some(end_signed) => {
                            let econtent_matches = extract_econtent(&end_signed)
                                .map(|inner| inner == end_payload)
                                .unwrap_or(false);
                            println!(
                                "  END SIGNED_XML: {} bytes (ATTACHED CMS; eContent==PAYLOAD_XML: {econtent_matches}) \
                                 — these are the exact bytes the drain sent to DPS",
                                end_signed.len()
                            );
                        }
                        None => println!(
                            "  END has NO SIGNED_XML artifact — it was rejected before/at sign \
                             (state={end_state}); nothing was sent to DPS"
                        ),
                    }
                }
                None => println!(
                    "  END has NO PAYLOAD_XML artifact — it was refused before stage_sign \
                     produced canonical bytes (state={end_state})"
                ),
            }
        }
    }

    // ── B9 acceptance assertions ───────────────────────────────────────────
    // (1) The pre-B9 failure was DPS code -9 "not ID in MAC".  It MUST NOT recur.
    assert_ne!(
        trace_code,
        Some(-9),
        "B9 REGRESSION: live DPS rejected the offline drain with -9 \"not ID in MAC\" — the \
         signed `<MAC ID='{stamped_code}'>` was not accepted. This is the exact pre-B9 \
         blocker (INV-11) the fix was supposed to close. Diagnostics above carry the DPS reply."
    );
    // (2) The drained doc must reach an accepted terminal (SENT is the DPS-ACK
    //     CAS moment; KVT1/KVT2/ACK are the confirm ticks).  Anything else means
    //     DPS did not accept the receipt — the DPS code is printed above.
    assert!(
        matches!(final_state.as_str(), "SENT" | "KVT1" | "KVT2" | "ACK"),
        "Smoke 9 FAIL: the offline drain did NOT reach an accepted terminal \
         (doc→{final_state}, dps_code={trace_code:?}). If dps_code=-9 the <MAC ID> was rejected; \
         any other code is a different DPS reject — see diagnostics above."
    );
    assert!(
        server_fiscal_no.is_some(),
        "an accepted offline drain must populate server_fiscal_no"
    );

    // ── Post-bracket ────────────────────────────────────────────────────────
    println!("\n--- POST-BRACKET ---");
    if let Ok(a) = channel.last_chk(&fn_sign).await {
        let chain_changed = a.data_sign.len() != pre_ack.data_sign.len() || a.id != pre_ack.id;
        println!(
            "  post lastChk: id={:?} data_sign={} bytes  chain_changed={chain_changed}",
            a.id,
            a.data_sign.len()
        );
    }
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        println!(
            "  post statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        );
    }

    println!(
        "\nSmoke 9 PASS: B9 offline drain ACCEPTED by live DPS — offline SELL carried \
         `<MAC ID='{stamped_code}'>`, drained to {final_state} \
         (server_fiscal_no={server_fiscal_no:?}), NO -9 \"not ID in MAC\". INV-11 closed live."
    );
}

// ── Live recovery — close a DANGLING offline session (DocType=10 END) ────────
//
// Gate: feature `live-dps` (file-level) + `#[ignore]` + `PRRO_LIVE_DPS=1` + a
// real JKS (this CONTACTS live DPS: reads the chain tip, then SENDS a signed
// DocType=10 END).  The full triple gate applies.
//
// ── Env contract (recovery-specific) ────────────────────────────────────────
/// **RETIRED (kept only so `--nocapture` runs surface a clear note).**  The
/// recovery END no longer carries ANY offline code — see the WebCheck root-cause
/// below.  This var name is documented here purely so an operator who set it in a
/// prior run understands it is now IGNORED (the bare-`<MAC>` END needs no code).
///
/// ROOT CAUSE (why the offline-shaped END drew `-5 "no id offline"`):
/// the drain-to-online close in WebCheck is `SendingOfflineChecks.cs::
/// CloseOfflineDoc()` (line 221) — reached via `Dispatch.OfflineToOnline`
/// (Dispatch.cs:98) — NOT the offline-session `CloseOfflineDocOffline()`
/// (line 143).  `CloseOfflineDoc()` builds the DocType=10 END as an
/// **ONLINE-SHAPED** doc: a **bare `<MAC>` (line 238, NO `ID=` attribute, NO
/// offline code)** submitted via a **4-param `SubmitCheck(pathFile, …, 3, num)`
/// (line 265) with NO `id_offline`** and persisted via `SaveXMLcheck` (online),
/// not `SaveXMLcheckOffline`.  At drain you are going BACK ONLINE, so the END
/// rides as an ordinary ONLINE doc — it needs no code at all.  Our previous
/// offline-shaped END (`<MAC ID='{code}'>` + wire `id_offline`) is exactly what
/// DPS rejects with `-5`; the fix is to REMOVE the code, not to source a fresh
/// one.
const ENV_RECOVER_CODE: &str = "PRRO_LIVE_DPS_RECOVER_CODE";
/// The END's `<DAT DI>` / wire `local_number`.  Operator-overridable so a retry
/// can advance the DI if DPS rejects a duplicate.  Default `9` mirrors the
/// canonical END position in the [BEGIN, SHIFT_OPEN, SELL, END] backlog shape
/// (DI is the boundary doc's local number; smoke 9's END unit-test uses `9`).
const ENV_RECOVER_DI: &str = "PRRO_LIVE_DPS_RECOVER_DI";
const DEFAULT_RECOVER_DI: u32 = 9;

/// Resolve the recovery DI (env override → default 9).
fn resolve_recover_di() -> u32 {
    std::env::var(ENV_RECOVER_DI)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_RECOVER_DI)
}

/// Convert a UTC ISO-8601 `business_ts` to Kyiv-local `YYYYMMDDHHMMSS` — the
/// `<TS>` string the signed END carries.  Reproduces production
/// `stage_sign::format_kyiv_local` verbatim (that fn is private) so the signed
/// `<TS>` matches what the real drain would emit.
fn recover_kyiv_ts_str(business_ts: &str) -> String {
    use chrono::{DateTime, Utc};
    use chrono_tz::Europe::Kiev;
    let dt: DateTime<Utc> = business_ts
        .parse::<DateTime<Utc>>()
        .expect("recovery business_ts must be a valid RFC-3339 timestamp");
    dt.with_timezone(&Kiev).format("%Y%m%d%H%M%S").to_string()
}

/// Convert a UTC ISO-8601 `business_ts` to the DPS "Kyiv-local-as-epoch" wire
/// value — the `CheckEnvelope.date_time` the END is sent with.  Reproduces
/// production `stage_send::kyiv_local_epoch` verbatim (that fn is private) so
/// the wire `date_time` matches what the real drain would send and stays
/// CONSISTENT with the signed `<TS>` (both derive from the SAME instant).
fn recover_kyiv_local_epoch(business_ts: &str) -> i64 {
    use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
    use chrono_tz::Europe::Kiev;
    let dt: DateTime<Utc> = business_ts
        .parse::<DateTime<Utc>>()
        .expect("recovery business_ts must be a valid RFC-3339 timestamp");
    let kyiv = dt.with_timezone(&Kiev);
    // Re-interpret the Kyiv-local digits as if they were UTC → the epoch DPS
    // expects (matches stage_send::kyiv_local_epoch byte-for-byte).
    Utc.with_ymd_and_hms(
        kyiv.year(),
        kyiv.month(),
        kyiv.day(),
        kyiv.hour(),
        kyiv.minute(),
        kyiv.second(),
    )
    .single()
    .expect("Kyiv-local components must be an unambiguous instant")
    .timestamp()
}

/// Build a **WebCheck-EXACT** drain-close DocType=10 OFFLINE_SESSION_END XML,
/// byte-faithful to `SendingOfflineChecks.cs::EndOfflineXML` (line 297) + the
/// `mmmaaaccc` → **bare `<MAC>`** replacement in **`CloseOfflineDoc()`**
/// (line 238) — the ONLINE-shaped drain-close reached via
/// `Dispatch.OfflineToOnline` (Dispatch.cs:98).  This is the CORRECT close form
/// for going back online, and it is DELIBERATELY NOT the offline-session
/// `CloseOfflineDocOffline()` (line 143), which uses `<MAC ID='{OfflineNum}'>` +
/// a 6-param `SubmitCheck(..., id_offline=OfflineNum)`.  The `EndOfflineXML`
/// concat chain emits, verbatim:
///
/// ```text
/// <?xml version='1.0' encoding='windows-1251'?><RQ V ='1'><DAT  FN='{FN}' TN='ПН {TIN}' ZN='' DI='{DI}' V='1'><C T='110'></C><TS>{CCD}</TS></DAT>{MAC}</RQ>
/// ```
///
/// with `{MAC}` = **`<MAC>{prev_tip}</MAC>`** (line 238 — a BARE `<MAC>`, NO
/// `ID=` attribute, NO offline code).  This is the ROOT-CAUSE fix: our previous
/// offline-shaped END carried `<MAC ID='{code}'>` (+ wire `id_offline`), which
/// DPS rejects with `-5 "no id offline"` on a drain-close.
///
/// The `<DAT>` form still DIVERGES from the production
/// `emit_offline_session_boundary` builder in the deltas the byte-diff found,
/// all reproduced here to match WebCheck's PROVEN client:
///   - `TN='ПН {TIN}'`  (production emits bare `TN="{TIN}"` — no `ПН ` prefix)
///   - `ZN=''`          (production emits `ZN="0"`)
///   - NO `NDv=` / `PrV=` attrs on `<RQ>` (production emits both defaults)
///   - single-quoted attrs + `<RQ V ='1'>` spacing + `<DAT  FN=` double-space,
///     matching the decompiled C# string literals exactly.
///
/// The leading `<?xml …?>` prolog is INCLUDED to mirror WebCheck's `text`
/// verbatim (WebCheck signs the whole `text`, including the prolog).  `prev_tip`
/// (the settled tip hash hex) is emitted RAW — a hex string with no XML-special
/// chars, exactly as WebCheck's `.Replace` inserts it (the signed hash must
/// cover the exact bytes DPS reads back).
///
/// Returns the cp1251-encoded wire bytes (the `ПН ` prefix is Cyrillic, so the
/// signed + submitted bytes MUST be cp1251, matching the `encoding='windows-1251'`
/// prolog and WebCheck's `SaveToFileText` codepage).
fn build_webcheck_exact_end_xml(
    fiscal_number: &str,
    tin: &str,
    di: u32,
    ts_str: &str,
    prev_tip_hex: &str,
) -> Vec<u8> {
    // Byte-faithful to EndOfflineXML's string.Concat chain + the ONLINE
    // (`CloseOfflineDoc`) bare-`<MAC>` replace at line 238.  Emit cp1251 bytes
    // directly: the ENTIRE document is ASCII EXCEPT the `ПН ` (Cyrillic) TN
    // prefix.  The production `cp1251::encode` is `pub(super)` (not reachable
    // from this test crate), so we splice the two cp1251 bytes for `П`/`Н`
    // in-place.  cp1251 uppercase Cyrillic: `А`(U+0410)=0xC0 … so `П`(U+041F)=
    // 0xCF and `Н`(U+041D)=0xCD (verified against the standard Windows-1251
    // table used by `xml::cp1251::encode_char`).
    const CP1251_PE: u8 = 0xCF; // 'П'
    const CP1251_EN: u8 = 0xCD; // 'Н'
    let head_ascii = format!(
        "<?xml version='1.0' encoding='windows-1251'?><RQ V ='1'>\
         <DAT  FN='{fiscal_number}' TN='"
    );
    let tail_ascii = format!(
        " {tin}' ZN='' DI='{di}' V='1'>\
         <C T='110'></C><TS>{ts_str}</TS></DAT>\
         <MAC>{prev_tip_hex}</MAC></RQ>"
    );
    // `head_ascii` + cp1251(`ПН`) + `tail_ascii` (the tail leads with the space
    // that follows `ПН` in `TN='ПН {tin}'`).  All FN / TN / DI / TS / hash
    // content is ASCII by construction, so byte-splicing is exact.
    let mut out = Vec::with_capacity(head_ascii.len() + 2 + tail_ascii.len());
    out.extend_from_slice(head_ascii.as_bytes());
    out.push(CP1251_PE);
    out.push(CP1251_EN);
    out.extend_from_slice(tail_ascii.as_bytes());
    out
}

/// **Live recovery — close a DANGLING offline session via DocType=10 END**.
///
/// WHY: live smoke 9 proved `-5` cleared (BEGIN + offline SHIFT_OPEN + SELL all
/// ACK'd on real DPS), but the drain-finalize DocType=10 END was
/// terminal-rejected → the offline session was left OPEN on the DPS cabinet
/// (post `statusRro` showed `open_shift=true online=false`).  An open offline
/// session does NOT expire on DPS — it hangs until a valid DocType=10 END
/// closes it, and it now blocks `T=112` with `-16`.  This routine closes it.
///
/// ROOT CAUSE (byte-diff vs WebCheck): our previous END was OFFLINE-shaped
/// (`<MAC ID='{code}'>` + a wire `id_offline`), mirroring WebCheck's
/// `CloseOfflineDocOffline()`.  But the drain-to-online close is the DIFFERENT
/// `CloseOfflineDoc()` (SendingOfflineChecks.cs:221, reached via
/// `Dispatch.OfflineToOnline`): it builds an ONLINE-shaped END with a **BARE
/// `<MAC>` (line 238, NO `ID=`, NO offline code)** and a **4-param
/// `SubmitCheck(pathFile, …, 3, num)` (line 265, NO `id_offline`)**.  DPS
/// rejects the offline-shaped drain-close with `-5 "no id offline"`.  This
/// routine now sends the CORRECT bare-`<MAC>` ONLINE form — NO offline code at
/// all — off the SETTLED chain tip.  (A stale `previous_hash` was the earlier
/// hypothesis; the tip-settle poll below is retained as belt-and-braces, but the
/// SHAPE was the real bug.)
///
/// FLOW:
///   1. `live_armed` gate + `load_signing_key` (skip/return if not armed).
///   2. Connect; read `status_rro` + `last_chk`; POLL `last_chk` until the tip
///      is SETTLED (stable across 2 reads) → the settled tip hash (hex).
///   3. Build a DocType=10 `<C T='110'>` END via `build_webcheck_exact_end_xml`
///      (WebCheck-EXACT ONLINE `CloseOfflineDoc` form): `TN='ПН {TIN}'`, `ZN=''`,
///      no NDv/PrV, **bare `<MAC>{settled_tip}</MAC>` (NO ID=, NO code)**,
///      `<TS>` = current Kyiv time, `DI` = `PRRO_LIVE_DPS_RECOVER_DI`.  Print the
///      `<C T='110'>` XML.
///   4. Sign it with the LIVE key (ATTACHED CAdES-BES; same CMS block as
///      the SELL path).
///   5. `send_chk` it with an EMPTY wire `id_offline` (ONLINE-shaped); PRINT the
///      FULL DPS response (OK → assigned id; Err → exact `dps_code` + `message`).
///   6. `status_rro` again → print `open_shift`/`online` (session closed?).
///   7. Assert: PASS if DPS ACCEPTS the END; on reject, fail LOUDLY with the
///      code so we learn the real reject reason.
///
/// This is a TARGETED one-shot recovery: it does NOT boot the write-path or
/// mutate any gateway DB — it hand-builds + signs + sends a single END, exactly
/// as WebCheck's `CloseOfflineDoc` would, off the SETTLED tip.  It needs NO
/// offline code (the `PRRO_LIVE_DPS_RECOVER_CODE` var is now IGNORED).
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_recover_close_dangling_offline_session() {
    const NAME: &str = "Live recovery (close dangling offline session — DocType=10 END)";
    if !live_armed(NAME) {
        return;
    }
    let Some(ek) = load_signing_key(NAME) else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    let tn = LIVE_TN;
    let recover_di = resolve_recover_di();

    println!("\n=== RECOVERY: close DANGLING offline session via DocType=10 END ===");
    println!("FN: {fiscal_number}  TN: {tn}");
    println!("Endpoint: {host}");
    println!(
        "END form: WebCheck-EXACT drain-close (CloseOfflineDoc) — ONLINE-shaped, \
         BARE <MAC> (NO ID=, NO offline code), empty wire id_offline.   DI: {recover_di}"
    );
    if std::env::var(ENV_RECOVER_CODE).is_ok() {
        println!(
            "NOTE: {ENV_RECOVER_CODE} is set but IGNORED — the drain-close END carries no \
             offline code (WebCheck CloseOfflineDoc uses a bare <MAC>)."
        );
    }
    println!(
        "Goal: DPS ACCEPTS a settled-chain BARE-<MAC> DocType=10 END → the offline session \
         CLOSES (online should flip to true)\n"
    );

    // ── Step 1: connect + read the CURRENT DPS state ───────────────────────
    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("RECOVERY FAIL: GrpcDpsChannel::connect: {e:?}"));
    let fn_sign = sign_fn_blob(&ek, &fiscal_number);

    println!("--- PRE-STATE ---");
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        println!(
            "  pre statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        );
        if s.online {
            println!(
                "  NOTE: DPS reports online=true already — the session may ALREADY be \
                 closed.  The END below will confirm (a duplicate END typically draws a \
                 shift/session-state reject, which this routine will surface)."
            );
        }
    }

    // ── Step 2: read + SETTLE the current chain tip ────────────────────────
    // Poll `last_chk` until the tip is STABLE across two consecutive reads, so
    // the END's `previous_hash` chains off DPS's REAL current tip.  This is the
    // fix for the original END reject (which chained off a not-yet-settled tip).
    // Each read decodes `data_sign` (ATTACHED CMS) → sha256(eContent) = the hash
    // the NEXT doc must carry in `<MAC>` (mirrors the SELL settle-poll at :2508).
    println!("\n--- SETTLE CHAIN TIP (poll last_chk until stable across 2 reads) ---");
    let settled_tip_hex: String = {
        // Read the tip hash once; returns Some(hex) for a real tip, None for a
        // genesis/empty chain, or on a decode miss (treated as "not yet
        // readable" and retried).
        async fn read_tip_hex(
            channel: &GrpcDpsChannel,
            fn_sign: &CheckSignBlob,
        ) -> Result<Option<String>, DpsError> {
            let ack = channel.last_chk(fn_sign).await?;
            if ack.data_sign.is_empty() {
                // Genesis / empty chain → the END would chain off an empty MAC.
                return Ok(Some(String::new()));
            }
            match extract_econtent(&ack.data_sign) {
                Ok(inner) => {
                    let tip: [u8; 32] = Sha256::digest(&inner).into();
                    Ok(Some(hex_lower(&tip)))
                }
                // data_sign present but not a parseable CMS — transient; retry.
                Err(_) => Ok(None),
            }
        }

        let mut prev: Option<String> = None;
        let mut settled: Option<String> = None;
        for attempt in 1..=20u32 {
            match read_tip_hex(&channel, &fn_sign).await {
                Ok(Some(tip)) => {
                    println!(
                        "  poll {attempt}: DPS tip = {}",
                        if tip.is_empty() {
                            "<genesis/empty>"
                        } else {
                            &tip
                        }
                    );
                    if prev.as_ref() == Some(&tip) {
                        // Stable across two consecutive reads → settled.
                        println!(
                            "  DPS tip SETTLED (stable across 2 reads) after {attempt} poll(s)"
                        );
                        settled = Some(tip);
                        break;
                    }
                    prev = Some(tip);
                }
                Ok(None) => {
                    println!("  poll {attempt}: tip not yet decodable — retry");
                }
                Err(DpsError::Server { code: -4, message }) => {
                    println!(
                        "RECOVERY SKIP: DPS rate-limit (-4) on last_chk: {message}. Cool down 5+ min."
                    );
                    return;
                }
                Err(DpsError::Transport(msg)) => {
                    panic!("RECOVERY FAIL: Transport on last_chk: {msg}");
                }
                Err(e) => {
                    println!("  poll {attempt}: last_chk non-fatal error (retry): {e:?}");
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        match settled {
            Some(tip) => tip,
            None => {
                println!(
                    "RECOVERY SKIP: DPS chain tip did not stabilise within the poll window \
                     (~100s) — cannot safely chain the END off a moving tip.  Re-run later."
                );
                return;
            }
        }
    };
    println!(
        "  settled previous_hash for the END: {}",
        if settled_tip_hex.is_empty() {
            "<genesis/empty>"
        } else {
            &settled_tip_hex
        }
    );

    // ── Step 3: build the WebCheck-EXACT ONLINE (bare-`<MAC>`) DocType=10 END ─
    // WebCheck's drain-to-online close (`CloseOfflineDoc`, reached via
    // `Dispatch.OfflineToOnline`) submits an ONLINE-shaped END: a BARE `<MAC>`
    // (NO `ID=`, NO offline code) via a 4-param `SubmitCheck(..., 3, num)` with
    // NO `id_offline`.  There is NO offline code to fetch — the drain-close needs
    // none.  This is the ROOT-CAUSE fix for the `-5 "no id offline"` reject our
    // OFFLINE-shaped END drew.  One `business_ts` instant drives BOTH the signed
    // `<TS>` (ts_str) and the wire `date_time` (fake-epoch), exactly as
    // production stage_sign + stage_send do; `<MAC>` chains off the SETTLED tip.
    println!("\n--- BUILD DocType=10 END (<C T='110'>, WebCheck-EXACT bare-<MAC> online form) ---");
    let business_ts = iso_now();
    let ts_str = recover_kyiv_ts_str(&business_ts);
    let date_time = recover_kyiv_local_epoch(&business_ts);

    let end_xml_bytes =
        build_webcheck_exact_end_xml(&fiscal_number, tn, recover_di, &ts_str, &settled_tip_hex);
    // The wire bytes are cp1251-encoded; the `ПН ` TN prefix is Cyrillic (the
    // rest — `<C T='110'>` / bare `<MAC>` — is ASCII) so lossy-UTF8 is a faithful
    // human-readable render of the exact bytes signed + sent.
    println!(
        "  END <C T='110'> XML ({} bytes, cp1251): {}",
        end_xml_bytes.len(),
        String::from_utf8_lossy(&end_xml_bytes)
    );
    println!("  END <TS>={ts_str}  wire date_time(fake-epoch)={date_time}");
    println!("  END <MAC>=bare (NO ID=)  wire id_offline=<empty>  typCheck=ServiceChk(3)");

    // ── Step 4: sign with the LIVE key (ATTACHED CAdES-BES) ────────────────
    // Same CmsSigner / Dstu4145WithGost34311Pb block as the T=112 / SELL path.
    println!("\n--- SIGN END (native ATTACHED CAdES-BES, live key) ---");
    let cert_der: &[u8] = ek
        .signing_cert()
        .expect("JKS must carry a signing certificate");
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&ek.param_d[..], curve.mod_words);
    let signer_inner = DstuInProcessSigner::new(d);
    let cms_signer_end = CmsSigner {
        cert_der,
        signer: &signer_inner,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    };
    let end_signed = cms_signer_end
        .sign_with(
            &end_xml_bytes,
            CmsBuildOptions {
                attached: true,
                signing_time: Some(SystemTime::now()),
            },
        )
        .expect("END sign must succeed")
        .cms_der;
    // Confirm the ATTACHED CMS eContent == the canonical bytes we're closing on.
    let econtent_ok = extract_econtent(&end_signed)
        .map(|inner| inner == end_xml_bytes)
        .unwrap_or(false);
    println!(
        "  END SIGNED: {} bytes (ATTACHED CMS; eContent==canonical XML: {econtent_ok})",
        end_signed.len()
    );

    // ── Step 5: send the END + capture the FULL DPS response ───────────────
    // DocType=110 → DpsCheckType::ServiceChk (typ 9/10 → PROTO 3), local_number
    // = DI, id_offline = EMPTY (WebCheck's `CloseOfflineDoc` uses a 4-param
    // `SubmitCheck(..., 3, num)` with NO id_offline — the drain-close is an
    // ONLINE issuance, so DPS interprets empty id_offline as "online"),
    // date_time = fake-epoch.
    println!("\n--- SEND END → live DPS ---");
    let end_envelope = CheckEnvelope {
        rro_fn: fiscal_number.clone(),
        date_time,
        check_sign: end_signed,
        local_number: recover_di as i32,
        check_type: DpsCheckType::ServiceChk,
        // ONLINE-shaped drain-close: NO offline code on the wire.
        id_offline: String::new(),
        id_cancel: String::new(),
    };

    let mut accepted = false;
    match channel.send_chk(end_envelope).await {
        Ok(ack) => {
            accepted = true;
            println!(
                "  END DPS OK — session CLOSED.  assigned id (server_fiscal_no)={:?}  \
                 id_sign={} bytes  data_sign={} bytes",
                ack.id,
                ack.id_sign.len(),
                ack.data_sign.len()
            );
        }
        Err(DpsError::Server { code: -4, message }) => {
            println!(
                "RECOVERY SKIP: DPS rate-limit (-4) on END send_chk: {message}. Cool down 5+ min."
            );
            return;
        }
        Err(DpsError::Server { code, message }) => {
            // THIS is the END reject reason we've been chasing — print it loudly.
            println!(
                "  END DPS REJECT — dps_code={code}  message={message:?}\n  \
                 (this is the exact DocType=10 END reject reason.  This END is the \
                 WebCheck-EXACT ONLINE drain-close form: BARE <MAC> (no ID=), EMPTY wire \
                 id_offline.  Interpretation: if code=-16 the session is still open; if it \
                 names a MAC/hash issue the settled-tip fix needs revisiting; if it is STILL \
                 -5 \"no id offline\" on a BARE-<MAC>/empty-id_offline END, then DPS wants the \
                 OPPOSITE of what the bare form provides and the WebCheck CloseOfflineDoc \
                 model does not fit this session state — escalate to cabinet-side)"
            );
        }
        Err(DpsError::Transport(msg)) => {
            panic!("RECOVERY FAIL: Transport error on END send_chk: {msg}");
        }
        Err(e) => {
            panic!("RECOVERY FAIL: unexpected error on END send_chk: {e:?}");
        }
    }

    // ── Step 6: re-read state — did the session close? ─────────────────────
    println!("\n--- POST-STATE ---");
    let mut post_online: Option<bool> = None;
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        post_online = Some(s.online);
        println!(
            "  post statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        );
        if s.online {
            println!("  → online flipped TRUE: the dangling offline session is CLOSED.");
        } else {
            println!(
                "  → online still FALSE: the session did NOT close (see the END reject code above)."
            );
        }
    }

    // ── Step 7: verdict ────────────────────────────────────────────────────
    // PASS iff DPS ACCEPTED the END.  On reject we FAIL LOUDLY (the dps_code +
    // message are printed above) so the operator learns the real reject reason.
    assert!(
        accepted,
        "RECOVERY FAIL: DPS did NOT accept the DocType=10 END — the dangling offline \
         session is STILL OPEN.  The exact dps_code + message are printed above (Step 5). \
         This END is the WebCheck-EXACT ONLINE drain-close (bare <MAC>, empty id_offline). \
         If a MAC/hash reject, the tip had not truly settled; if STILL -5 \"no id offline\" \
         on this bare-<MAC>/empty-id_offline END, the WebCheck CloseOfflineDoc model does not \
         fit this session state — escalate cabinet-side."
    );
    println!(
        "\nRECOVERY PASS: DPS ACCEPTED the settled-chain WebCheck-EXACT bare-<MAC> DocType=10 \
         END — the dangling offline session is CLOSED (post online={:?}).  T=112 should no \
         longer draw -16.",
        post_online
    );
}

// ─── Live recovery — close a leftover OPEN SHIFT via an ONLINE Z_REPORT ──────
//
// WHY: live smoke 9 (run 3) opened a shift on the DPS cabinet via an OFFLINE
// SHIFT_OPEN + 1 SELL (both ACK'd on real DPS), then the drain-finalize left the
// SHIFT itself OPEN (post `statusRro` showed `open_shift=true`).  A leftover open
// shift makes a FRESH smoke's SHIFT_OPEN draw `-2 "shift is already open"`.  To
// get a clean integrated run the shift must be closed with a Z_REPORT
// (DPS DocType=80 / `Check.Type::ZREPORT=2`).  We are ONLINE now, so this is a
// NORMAL online issuance — a BARE `<MAC>` (no offline code), exactly like the
// B10 END fix's online drain-close form, only carrying the Z body instead of an
// empty END.
//
// SHAPE: the production T=80 wire form is `<Z NO=...>` (NOT `<C T='80'>`) — the
// DPS doctype-80 is signalled by the `<Z>` block + `typCheck=ZReport(2)` on the
// wire.  We drive the REAL production builder (`xml::build_canonical_xml(
// &CanonicalDoc::ZReport(..))`, the A′.2 Z-surface used live by smoke 7) with a
// `DocumentHeader { mac_id: None }` → the online bare-`<MAC>` form.  This is a
// PURE standalone call (no DB / write-path boot), matching the one-shot recovery
// posture of `live_recover_close_dangling_offline_session` above.
//
// Z TOTALS: the shift's only receipt is run-3's SELL (`SMOKE9_SELL_PAYLOAD_JSON`
// = one CASH item, `sum_kop=15000`, NO `tax_group_1`).  The production
// `convert::derive_z_report_tax_summaries` would emit NO `<TXS>` for a
// tax-group-less item, so the DEFAULT Z carries one `<M NM="CASH" SMI={sum} T=0>`
// payment + `<NC NI=1 NO=0>` check-count + NO `<TXS>`.  Both the sale sum and an
// optional tax code are ENV-OVERRIDABLE so the operator can adjust on the live
// run if DPS rejects with a totals mismatch (`-6`/`-10` Z-sequence/XML rejects):
//   PRRO_LIVE_DPS_Z_SUM_KOP — CASH SMI (default 15000 = run-3 SELL total).
//   PRRO_LIVE_DPS_Z_TAX     — canonical tax-group number; when set, inject one
//                             SHORT-FORM `<TXS SMI={sum} SMO=0 TX={tax}>` (the
//                             same shape `derive_z_report_tax_summaries` emits for
//                             an UNRESOLVED group — we have no rate snapshot live
//                             to compute TXI/TXO).  Unset → no `<TXS>`.
//   PRRO_LIVE_DPS_Z_NO      — `<DAT ZN=...>` / `<Z NO=...>` Z counter (default 1).
//                             DPS validates the per-RRO Z sequence; bump on a -6.
//   PRRO_LIVE_DPS_Z_DI      — `<DAT DI=...>` / wire local_number (default 10 —
//                             next after run-3's [BEGIN,SHIFT_OPEN,SELL] backlog).

/// `PRRO_LIVE_DPS_Z_SUM_KOP` — CASH `<M SMI>` (kopecks).  Default = run-3 SELL.
const ENV_Z_SUM_KOP: &str = "PRRO_LIVE_DPS_Z_SUM_KOP";
/// `PRRO_LIVE_DPS_Z_TAX` — canonical tax group for a short-form `<TXS>` (opt).
const ENV_Z_TAX: &str = "PRRO_LIVE_DPS_Z_TAX";
/// `PRRO_LIVE_DPS_Z_NO` — the `<Z NO>` / `<DAT ZN>` Z counter.
const ENV_Z_NO: &str = "PRRO_LIVE_DPS_Z_NO";
/// `PRRO_LIVE_DPS_Z_DI` — the `<DAT DI>` / wire `local_number` for the Z.
const ENV_Z_DI: &str = "PRRO_LIVE_DPS_Z_DI";

/// Run-3 SELL total (`SMOKE9_SELL_PAYLOAD_JSON.payments[0].sum_kop`) = the CASH
/// inflow the closing Z must report by default.
const DEFAULT_Z_SUM_KOP: i64 = SMOKE9_SELL_TOTAL_KOP; // 15000
/// Default Z counter — first Z on this FN's per-RRO sequence.  Bump via env on -6.
const DEFAULT_Z_NO: u32 = 1;
/// Default Z DI — next local number after run-3's [BEGIN, SHIFT_OPEN, SELL].
const DEFAULT_Z_DI: u32 = 10;

fn resolve_env_i64(var: &str, default: i64) -> i64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn resolve_env_u32(var: &str, default: u32) -> u32 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

/// **Live recovery — close a leftover OPEN SHIFT via an ONLINE Z_REPORT**.
///
/// WHY: run-3's offline SHIFT_OPEN + SELL left the SHIFT open on the DPS cabinet
/// (`statusRro.open_shift=true`), so a fresh smoke's SHIFT_OPEN draws `-2 "shift
/// is already open"`.  This routine closes it with an ONLINE Z_REPORT (DPS
/// DocType=80, `typCheck=ZReport(2)`) off the SETTLED chain tip — a normal online
/// issuance with a BARE `<MAC>` (NO offline code), the same online form as the
/// B10 END fix, but carrying the Z body.
///
/// FLOW (mirrors `live_recover_close_dangling_offline_session`):
///   1. `live_armed` gate + `load_signing_key` (skip/return if not armed).
///   2. Connect; read `status_rro` (print pre `open_shift`/`online`).  If
///      `open_shift=false` already, the shift is closed — the Z below will
///      confirm (a Z with no open shift typically draws a shift-state reject,
///      which this routine surfaces).
///   3. POLL `last_chk` until the tip is SETTLED (stable across 2 reads) → the
///      settled tip hash (hex) → the Z's `<MAC>` previous-hash.
///   4. Build the Z via the PRODUCTION builder
///      `xml::build_canonical_xml(&CanonicalDoc::ZReport(..))` with
///      `DocumentHeader { mac_id: None }` (bare-`<MAC>` ONLINE form): one CASH
///      `<M SMI={PRRO_LIVE_DPS_Z_SUM_KOP}>`, `<NC NI=1 NO=0>`, and an OPTIONAL
///      short-form `<TXS>` when `PRRO_LIVE_DPS_Z_TAX` is set.  Print the Z XML.
///   5. Sign with the LIVE key (ATTACHED CAdES-BES, `Dstu4145WithGost34311Pb`).
///   6. `send_chk` with `typCheck=ZReport(2)`, EMPTY wire `id_offline` (online);
///      PRINT the FULL DPS response (OK → shift closed / server_fiscal_no;
///      reject → exact `dps_code` + message).
///   7. `status_rro` again → print `open_shift`/`online` (should flip
///      `open_shift=false`).  PASS iff DPS ACCEPTS the Z; on reject, FAIL loudly
///      with the exact code (esp. a totals mismatch → adjust the env sums).
///
/// This is a TARGETED one-shot recovery: it hand-drives the production Z builder
/// + signs + sends a single Z_REPORT, off the SETTLED tip.  It does NOT boot the
/// write-path or mutate any gateway DB.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via --features live-dps + --ignored + PRRO_LIVE_DPS=1"]
async fn live_recover_close_open_shift() {
    // Production Z-surface types — scoped here so the shared top-of-file import
    // block (which never needed the XML builder) stays untouched (minimal diff).
    use prro::xml::{
        build_canonical_xml, CanonicalDoc, DocumentHeader, ZReportCheckCount, ZReportPayload,
        ZReportPaymentSum, ZReportTaxSummary,
    };

    const NAME: &str = "Live recovery (close leftover OPEN SHIFT — Z_REPORT DocType=80)";
    if !live_armed(NAME) {
        return;
    }
    let Some(ek) = load_signing_key(NAME) else {
        return;
    };
    let host = resolve_host();
    let fiscal_number = resolve_fn();
    let tn = LIVE_TN;

    let z_sum_kop = resolve_env_i64(ENV_Z_SUM_KOP, DEFAULT_Z_SUM_KOP);
    let z_no = resolve_env_u32(ENV_Z_NO, DEFAULT_Z_NO);
    let z_di = resolve_env_u32(ENV_Z_DI, DEFAULT_Z_DI);
    // Optional short-form <TXS>: only when PRRO_LIVE_DPS_Z_TAX is a valid tax
    // group number.  None → no <TXS> (matches run-3's tax-group-less SELL).
    let z_tax: Option<i64> = std::env::var(ENV_Z_TAX)
        .ok()
        .and_then(|v| v.trim().parse().ok());

    println!("\n=== RECOVERY: close leftover OPEN SHIFT via ONLINE Z_REPORT (DocType=80) ===");
    println!("FN: {fiscal_number}  TN: {tn}");
    println!("Endpoint: {host}");
    println!(
        "Z form: production <Z NO> builder (A′.2 Z-surface), ONLINE bare-<MAC> (mac_id=None), \
         typCheck=ZReport(2), empty wire id_offline."
    );
    println!(
        "Z totals (env-overridable): CASH SMI={z_sum_kop}kop ({}) [{ENV_Z_SUM_KOP}]  \
         Z_NO={z_no} [{ENV_Z_NO}]  DI={z_di} [{ENV_Z_DI}]  TXS_tax={z_tax:?} [{ENV_Z_TAX}]",
        if z_sum_kop == DEFAULT_Z_SUM_KOP {
            "run-3 SELL default"
        } else {
            "env override"
        }
    );
    println!(
        "Goal: DPS ACCEPTS the Z → the leftover shift CLOSES (open_shift should flip false)\n"
    );

    // ── Step 1: connect + read the CURRENT DPS state ───────────────────────
    let channel = GrpcDpsChannel::connect(&host, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("RECOVERY FAIL: GrpcDpsChannel::connect: {e:?}"));
    let fn_sign = sign_fn_blob(&ek, &fiscal_number);

    println!("--- PRE-STATE ---");
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        println!(
            "  pre statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        );
        if !s.open_shift {
            println!(
                "  NOTE: DPS reports open_shift=false already — the shift may ALREADY be \
                 closed.  The Z below will confirm (a Z with no open shift typically draws a \
                 shift-state reject, which this routine surfaces)."
            );
        }
    }

    // ── Step 2: read + SETTLE the current chain tip ────────────────────────
    // Poll `last_chk` until the tip is STABLE across two consecutive reads so the
    // Z's `<MAC>` chains off DPS's REAL current tip (identical logic to the END
    // recovery above; each read decodes `data_sign` (ATTACHED CMS) →
    // sha256(eContent) = the hash the NEXT doc must carry in `<MAC>`).
    println!("\n--- SETTLE CHAIN TIP (poll last_chk until stable across 2 reads) ---");
    let settled_tip_hex: String = {
        async fn read_tip_hex(
            channel: &GrpcDpsChannel,
            fn_sign: &CheckSignBlob,
        ) -> Result<Option<String>, DpsError> {
            let ack = channel.last_chk(fn_sign).await?;
            if ack.data_sign.is_empty() {
                return Ok(Some(String::new()));
            }
            match extract_econtent(&ack.data_sign) {
                Ok(inner) => {
                    let tip: [u8; 32] = Sha256::digest(&inner).into();
                    Ok(Some(hex_lower(&tip)))
                }
                Err(_) => Ok(None),
            }
        }

        let mut prev: Option<String> = None;
        let mut settled: Option<String> = None;
        for attempt in 1..=20u32 {
            match read_tip_hex(&channel, &fn_sign).await {
                Ok(Some(tip)) => {
                    println!(
                        "  poll {attempt}: DPS tip = {}",
                        if tip.is_empty() {
                            "<genesis/empty>"
                        } else {
                            &tip
                        }
                    );
                    if prev.as_ref() == Some(&tip) {
                        println!(
                            "  DPS tip SETTLED (stable across 2 reads) after {attempt} poll(s)"
                        );
                        settled = Some(tip);
                        break;
                    }
                    prev = Some(tip);
                }
                Ok(None) => {
                    println!("  poll {attempt}: tip not yet decodable — retry");
                }
                Err(DpsError::Server { code: -4, message }) => {
                    println!(
                        "RECOVERY SKIP: DPS rate-limit (-4) on last_chk: {message}. Cool down 5+ min."
                    );
                    return;
                }
                Err(DpsError::Transport(msg)) => {
                    panic!("RECOVERY FAIL: Transport on last_chk: {msg}");
                }
                Err(e) => {
                    println!("  poll {attempt}: last_chk non-fatal error (retry): {e:?}");
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        match settled {
            Some(tip) => tip,
            None => {
                println!(
                    "RECOVERY SKIP: DPS chain tip did not stabilise within the poll window \
                     (~100s) — cannot safely chain the Z off a moving tip.  Re-run later."
                );
                return;
            }
        }
    };
    println!(
        "  settled previous_hash for the Z: {}",
        if settled_tip_hex.is_empty() {
            "<genesis/empty>"
        } else {
            &settled_tip_hex
        }
    );

    // ── Step 3: build the ONLINE Z_REPORT via the PRODUCTION builder ───────
    // `DocumentHeader::with_defaults` sets `mac_id = None` → bare `<MAC>` (the
    // ONLINE form).  One `business_ts` instant drives BOTH the signed `<TS>`
    // (ts_str) and the wire `date_time` (fake-epoch), exactly as production
    // stage_sign + stage_send do.  `<TS>` is Kyiv-local `YYYYMMDDHHMMSS`; the
    // `<TXS TS>` date prefix (when a tax group is injected) is its first 8 chars.
    println!("\n--- BUILD Z_REPORT (<Z NO>, production builder, ONLINE bare-<MAC>) ---");
    let business_ts = iso_now();
    let ts_str = recover_kyiv_ts_str(&business_ts);
    let date_time = recover_kyiv_local_epoch(&business_ts);

    let header = DocumentHeader::with_defaults(
        fiscal_number.clone(),
        tn,
        z_no,
        ts_str.clone(),
        settled_tip_hex.clone(),
    );

    // Optional short-form <TXS> from the env tax code (SMI/SMO/TX only — the
    // exact shape `derive_z_report_tax_summaries` emits for an UNRESOLVED group,
    // which is what run-3's tax-group-less SELL yields; we have no live rate
    // snapshot to compute TXI/TXO, so short form is the faithful choice).
    let tax_summaries: Vec<ZReportTaxSummary> = match z_tax {
        Some(tx) => vec![ZReportTaxSummary {
            tx,
            tx_short_form: true,
            txpr: String::new(),
            txal: 0,
            txty: 0,
            dtpr: String::new(),
            smi: z_sum_kop,
            smo: 0,
            txi: 0,
            txo: 0,
            // ts_prefix is IGNORED in short form (only SMI/SMO/TX emit); the
            // date prefix would be `ts_str[..8]` in full form.
            ts_prefix: ts_str.chars().take(8).collect(),
        }],
        None => Vec::new(),
    };

    let z_payload = ZReportPayload {
        header,
        local_number: z_di,
        tax_summaries,
        // The shift's single receipt was a CASH sale — mirror it as one <M>.
        payments: vec![ZReportPaymentSum {
            name: "CASH".into(),
            sum_in: z_sum_kop,
            sum_out: 0,
            type_code: "0".into(), // "0" = cash
        }],
        service_sums: Vec::new(),
        // run-3 minted exactly one SELL and zero returns in this shift.
        check_count: ZReportCheckCount {
            sell_count: 1,
            return_count: 0,
        },
        epz: None,
    };

    let z_xml_bytes = build_canonical_xml(&CanonicalDoc::ZReport(z_payload))
        .expect("production Z builder must succeed (cp1251-encodable content)");
    // The wire bytes are cp1251; the Z body (<Z>/<M>/<NC>/<TXS>) is ASCII and the
    // NDv device-name default (`ПРО_каса`) is Cyrillic, so lossy-UTF8 is a
    // faithful human-readable render of the exact bytes signed + sent.
    println!(
        "  Z <Z NO='{z_no}'> XML ({} bytes, cp1251): {}",
        z_xml_bytes.len(),
        String::from_utf8_lossy(&z_xml_bytes)
    );
    println!("  Z <TS>={ts_str}  wire date_time(fake-epoch)={date_time}");
    println!(
        "  Z <MAC>=bare (NO ID=)  wire id_offline=<empty>  typCheck=ZReport(2)  DI={z_di}"
    );

    // ── Step 4: sign with the LIVE key (ATTACHED CAdES-BES) ────────────────
    // Same CmsSigner / Dstu4145WithGost34311Pb block as the END / SELL path.
    println!("\n--- SIGN Z (native ATTACHED CAdES-BES, live key) ---");
    let cert_der: &[u8] = ek
        .signing_cert()
        .expect("JKS must carry a signing certificate");
    let curve = Curve::dstu_pb_257();
    let d = FieldEl::from_le_bytes(&ek.param_d[..], curve.mod_words);
    let signer_inner = DstuInProcessSigner::new(d);
    let cms_signer_z = CmsSigner {
        cert_der,
        signer: &signer_inner,
        profile: CmsProfile::Dstu4145WithGost34311Pb,
    };
    let z_signed = cms_signer_z
        .sign_with(
            &z_xml_bytes,
            CmsBuildOptions {
                attached: true,
                signing_time: Some(SystemTime::now()),
            },
        )
        .expect("Z sign must succeed")
        .cms_der;
    let econtent_ok = extract_econtent(&z_signed)
        .map(|inner| inner == z_xml_bytes)
        .unwrap_or(false);
    println!(
        "  Z SIGNED: {} bytes (ATTACHED CMS; eContent==canonical XML: {econtent_ok})",
        z_signed.len()
    );

    // ── Step 5: send the Z + capture the FULL DPS response ─────────────────
    // typCheck = ZReport (proto ZREPORT=2); local_number = DI; id_offline =
    // EMPTY (ONLINE issuance — no offline code); date_time = fake-epoch.
    println!("\n--- SEND Z → live DPS ---");
    let z_envelope = CheckEnvelope {
        rro_fn: fiscal_number.clone(),
        date_time,
        check_sign: z_signed,
        local_number: z_di as i32,
        check_type: DpsCheckType::ZReport,
        // ONLINE issuance: NO offline code on the wire.
        id_offline: String::new(),
        id_cancel: String::new(),
    };

    let mut accepted = false;
    match channel.send_chk(z_envelope).await {
        Ok(ack) => {
            accepted = true;
            println!(
                "  Z DPS OK — shift CLOSED.  assigned id (server_fiscal_no)={:?}  \
                 id_sign={} bytes  data_sign={} bytes",
                ack.id,
                ack.id_sign.len(),
                ack.data_sign.len()
            );
        }
        Err(DpsError::Server { code: -4, message }) => {
            println!(
                "RECOVERY SKIP: DPS rate-limit (-4) on Z send_chk: {message}. Cool down 5+ min."
            );
            return;
        }
        Err(DpsError::Server { code, message }) => {
            // The Z reject reason — print it loudly for the operator.
            println!(
                "  Z DPS REJECT — dps_code={code}  message={message:?}\n  \
                 (this is the exact DocType=80 Z_REPORT reject reason.  Interpretation: \
                 -6 (ERROR_NOT_PREV_ZREPORT) → the Z NUMBER is out of the FN's per-RRO \
                 sequence — bump {ENV_Z_NO}; -10 (ERROR_XML_ZREPORT) → the Z body/totals are \
                 malformed or mismatch the shift's receipts — adjust {ENV_Z_SUM_KOP} / \
                 {ENV_Z_TAX} to match run-3's SELL; a shift-state reject → the shift is \
                 already closed (see pre-state); a MAC/hash reject → the settled-tip fix \
                 needs revisiting)"
            );
        }
        Err(DpsError::Transport(msg)) => {
            panic!("RECOVERY FAIL: Transport error on Z send_chk: {msg}");
        }
        Err(e) => {
            panic!("RECOVERY FAIL: unexpected error on Z send_chk: {e:?}");
        }
    }

    // ── Step 6: re-read state — did the shift close? ───────────────────────
    println!("\n--- POST-STATE ---");
    let mut post_open_shift: Option<bool> = None;
    if let Ok(s) = channel.status_rro(&fn_sign).await {
        post_open_shift = Some(s.open_shift);
        println!(
            "  post statusRro: open_shift={} online={} last_signer={:?}",
            s.open_shift, s.online, s.last_signer
        );
        if !s.open_shift {
            println!("  → open_shift flipped FALSE: the leftover shift is CLOSED.");
        } else {
            println!(
                "  → open_shift still TRUE: the shift did NOT close (see the Z reject code above)."
            );
        }
    }

    // ── Step 7: verdict ────────────────────────────────────────────────────
    // PASS iff DPS ACCEPTED the Z.  On reject we FAIL LOUDLY (the dps_code +
    // message are printed above) so the operator learns the real reject reason —
    // most usefully a totals mismatch, which the env sums adjust.
    assert!(
        accepted,
        "RECOVERY FAIL: DPS did NOT accept the Z_REPORT — the leftover shift is STILL OPEN. \
         The exact dps_code + message are printed above (Step 5).  This Z is the production \
         ONLINE bare-<MAC> form (typCheck=ZReport(2), empty id_offline).  If -10 / a totals \
         mismatch, adjust {ENV_Z_SUM_KOP} / {ENV_Z_TAX} to match run-3's SELL; if -6, bump \
         {ENV_Z_NO}; if a shift-state reject, the shift was already closed."
    );
    println!(
        "\nRECOVERY PASS: DPS ACCEPTED the settled-chain ONLINE Z_REPORT — the leftover shift \
         is CLOSED (post open_shift={:?}).  A fresh smoke's SHIFT_OPEN should no longer draw -2.",
        post_open_shift
    );
}
