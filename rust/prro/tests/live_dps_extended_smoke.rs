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

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
