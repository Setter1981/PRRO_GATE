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
use prro::transports::dps::dto::CheckSignBlob;
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
use std::time::{Duration, SystemTime};

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
        .split(|c: char| c == '/' || c == '?' || c == '#')
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
