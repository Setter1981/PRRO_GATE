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

use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::CheckSignBlob;
use prro::transports::dps::error::DpsError;
use prro::transports::dps::grpc::GrpcDpsChannel;
use std::time::Duration;

// ─── Env contract ──────────────────────────────────────────────────────

/// Hard env kill-switch: every test self-skips unless this equals `"1"`.
const ENV_GATE: &str = "PRRO_LIVE_DPS";
/// DPS endpoint override; default = test cabinet (gRPC over TLS).
const ENV_HOST: &str = "PRRO_LIVE_DPS_HOST";
/// Test fiscal number override.
const ENV_FN: &str = "PRRO_LIVE_DPS_FN";

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
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    hostport.split(':').next().unwrap_or(hostport)
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
