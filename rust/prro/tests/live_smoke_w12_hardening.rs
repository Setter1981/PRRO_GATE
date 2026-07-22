//! **Live DPS smoke tests for W12 hardening cycle (2026-05-25)**.
//!
//! All tests `#[ignore]`'d by default — opt-in execution requires
//! operator to provide live DPS endpoint:
//!
//! ```bash
//! # Default endpoint (cabinet.tax.gov.ua:9443 per operator):
//! cargo test --features test-support \
//!     --test live_smoke_w12_hardening -- --ignored --nocapture
//!
//! # Or override endpoint via env var:
//! PRRO_LIVE_DPS_ENDPOINT=https://dev-cabinet.tax.gov.ua:9443 \
//!     cargo test --features test-support \
//!     --test live_smoke_w12_hardening -- --ignored --nocapture
//! ```
//!
//! ## Smoke matrix
//!
//! - **A: DPS reachability ping** — `live_smoke_a_dps_reachability_
//!   ping`: GrpcDpsChannel::connect + last_chk з dummy CheckSignBlob.
//!   Confirms TLS handshake + HTTP/2 stream creation + wire round-trip
//!   parsing.  Only `DpsError::Transport` counts as smoke FAIL (any
//!   typed application-level response = healthy).
//!
//! - **B: Kvt1Reentry lastChk live response shape** —
//!   `live_smoke_b_kvt1_reentry_by_server_fiscal_no_typed_response`:
//!   exercises the actual W12 SentReplay / Kvt1Reentry routing
//!   surface (`DpsChannel::by_server_fiscal_no`).  Uses fictitious
//!   `expected_id`; expects NotFound / Mismatch / Server /
//!   Authorization / Decode — any typed application-level response
//!   confirms W12 routing matrix would correctly handle real DPS.
//!
//! ## Important caveats (per operator memory)
//!
//! 1. **DPS rate limit** (`project_dps_rate_limit`): test server
//!    returns status=-4 after too many errors з 5-minute cooldown.
//!    DO NOT run smoke tests in loop або frequently — sparse manual
//!    invocation only.
//! 2. **No CMS signing required**: smoke tests use dummy
//!    `CheckSignBlob(vec![0u8; 32])` — DPS will reject as
//!    Authorization / Server / Decode but that proves wire path
//!    works.  Real signing (jkurwa sidecar + JKS keys) is OUT OF
//!    SCOPE for these reachability smokes — covered by separate
//!    full receipt cycle tests (Sprint 7 style, NOT this file).
//! 3. **Hardening layer in-scope**: tests confirm that если DPS is
//!    reachable + returns typed responses, W12 hardening layer
//!    (Tier triggers / admin reset / orphan scanner) would react
//!    correctly.  Full hardening reaction proof requires manual
//!    integration test з real fiscal_documents seeding (out of
//!    smoke scope).

use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::CheckSignBlob;
use prro::transports::dps::error::DpsError;
use prro::transports::dps::grpc::GrpcDpsChannel;
use std::time::Duration;

/// Default endpoint per operator's smoke confirmation 2026-05-25.
/// Override via `PRRO_LIVE_DPS_ENDPOINT` env var to point at a
/// different endpoint (e.g., `https://dev-cabinet.tax.gov.ua:9443`).
const DEFAULT_ENDPOINT: &str = "https://cabinet.tax.gov.ua:9443";

/// Env var name для endpoint override.  Allows operator to switch
/// between prod / dev / staging without source edits.
const ENV_VAR_ENDPOINT: &str = "PRRO_LIVE_DPS_ENDPOINT";

/// Per-call deadline for smoke tests.  15s is generous (typical DPS
/// response ~1-3s); covers slow TLS handshake + intermittent network
/// hiccups during manual smoke runs.  NOT for production tuning —
/// real W12 drain uses operator-configured per-FN timeout.
const SMOKE_TIMEOUT_SECS: u64 = 15;

fn resolve_endpoint() -> String {
    std::env::var(ENV_VAR_ENDPOINT).unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string())
}

/// **Live Smoke A0 — DPS connect-only handshake (2026-05-25)**.
///
/// **Purpose**: cheapest possible reachability test — just `GrpcDpsChannel
/// ::connect`, no RPC call.  Separates "TCP + TLS handshake works" from
/// "actual gRPC RPC reaches DPS application layer".  Use коли Smoke A
/// fails з `gRPC Unknown: transport error` to differentiate:
///   - A0 PASS + A FAIL = handshake works, but mTLS / client cert /
///     gRPC negotiation issue post-handshake (likely operator-env mismatch).
///   - A0 FAIL = TCP/TLS broken (DNS / network / firewall / cert
///     trust store / endpoint URI).
///
/// **Method**: `GrpcDpsChannel::connect` only.  Connection is eagerly
/// established per `connect()` doc — connect-time TLS handshake + HTTP/2
/// SETTINGS exchange happen here.  Drop channel immediately after.
///
/// **Pass criterion**: connect succeeds.
/// **Fail criterion**: connect returns Err (Transport / invalid URI / etc.).
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via cargo test -- --ignored"]
async fn live_smoke_a0_dps_connect_only_handshake() {
    let endpoint = resolve_endpoint();
    println!("\n=== Live Smoke A0: DPS connect-only handshake ===");
    println!("Endpoint: {endpoint}");
    println!("Timeout:  {SMOKE_TIMEOUT_SECS}s\n");

    let result = GrpcDpsChannel::connect(&endpoint, Duration::from_secs(SMOKE_TIMEOUT_SECS)).await;

    match result {
        Ok(_channel) => {
            println!(
                "Smoke A0 PASS: TCP + TLS handshake + HTTP/2 SETTINGS exchange \
                 OK (channel established eagerly per connect() contract)"
            );
        }
        Err(e) => {
            panic!(
                "Smoke A0 FAIL: GrpcDpsChannel::connect — TCP / TLS handshake \
                 / DNS / firewall / cert trust store broken.  Endpoint: \
                 {endpoint}.  Error: {e:?}"
            );
        }
    }
}

/// **Live Smoke A — DPS reachability ping (2026-05-25)**.
///
/// **Purpose**: confirms Rust `GrpcDpsChannel` can establish TLS
/// handshake + HTTP/2 stream + complete a wire round-trip against
/// the configured live DPS endpoint.
///
/// **Method**: `GrpcDpsChannel::connect` + `last_chk` з dummy
/// `CheckSignBlob(vec![0u8; 32])`.  DPS will reject the dummy sign
/// (Authorization / Server error expected) but the rejection itself
/// proves: TLS works, HTTP/2 works, gRPC works, response parses.
///
/// **Pass/Fail criteria**:
/// - PASS: any `Ok(_)` OR `Err(DpsError::{NotFound, Server,
///   Authorization, Decode, ServerFiscalIdMismatch})` response.
/// - FAIL: `Err(DpsError::Transport)` — wire-level brokenness
///   (network / TLS / DNS / handshake timeout).
/// - FAIL: panic on `GrpcDpsChannel::connect` — endpoint URI
///   invalid OR cannot establish TCP/TLS.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via cargo test -- --ignored"]
async fn live_smoke_a_dps_reachability_ping() {
    let endpoint = resolve_endpoint();
    println!("\n=== Live Smoke A: DPS reachability ===");
    println!("Endpoint: {endpoint}");
    println!("Timeout:  {SMOKE_TIMEOUT_SECS}s\n");

    let channel = GrpcDpsChannel::connect(&endpoint, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| {
            panic!(
                "Smoke A FAIL: GrpcDpsChannel::connect — wire-level connectivity \
                 broken (TLS / DNS / network / handshake timeout).  Endpoint: \
                 {endpoint}.  Error: {e:?}"
            )
        });

    let dummy_sign = CheckSignBlob(vec![0u8; 32]);
    let result = channel.last_chk(&dummy_sign).await;

    println!("Response: {result:?}\n");
    match result {
        Err(DpsError::Transport(msg)) => {
            panic!(
                "Smoke A FAIL: wire-level Transport error на last_chk RPC — \
                 connection established but mid-call failure.  Likely: server \
                 reset / deadline / proxy issue.  Error: {msg}"
            );
        }
        Ok(_) | Err(_) => {
            // ANY typed application-level response = wire path healthy.
            println!("Smoke A PASS: wire path operational (TLS + HTTP/2 + gRPC + response parse)");
        }
    }
}

/// **Live Smoke B — Kvt1Reentry by_server_fiscal_no live response (2026-05-25)**.
///
/// **Purpose**: exercises the actual W12 SentReplay / Kvt1Reentry
/// routing surface (`DpsChannel::by_server_fiscal_no` = canonical
/// `evaluate_lastchk` call site per W12 closure work).  Confirms
/// hardening layer would correctly route real-DPS responses.
///
/// **Method**: `by_server_fiscal_no(dummy_sign, fictitious_id)`.
/// Default trait impl calls `last_chk` then matches response.id.
/// Fictitious id will never match real fiscal data → expect
/// `NotFound` (most likely) OR `ServerFiscalIdMismatch` (if DPS
/// happens to return some echo).
///
/// **Pass/Fail criteria**:
/// - PASS: any typed application-level response (NotFound /
///   Mismatch / Server / Authorization / Decode / unexpected Ok).
/// - FAIL: `Transport` — hardening layer would auto-offline на
///   wire issue.
///
/// **Significance**: combined з Smoke A passing, validates that
/// post-hardening Rust drain layer would react correctly given a
/// live DPS context.  Full reaction proof (Tier triggers firing,
/// admin reset working live) requires manual integration з seeded
/// fiscal_documents — out of smoke scope.
#[tokio::test]
#[ignore = "live DPS endpoint required; opt-in via cargo test -- --ignored"]
async fn live_smoke_b_kvt1_reentry_by_server_fiscal_no_typed_response() {
    let endpoint = resolve_endpoint();
    println!("\n=== Live Smoke B: by_server_fiscal_no typed response ===");
    println!("Endpoint: {endpoint}");
    println!("Timeout:  {SMOKE_TIMEOUT_SECS}s\n");

    let channel = GrpcDpsChannel::connect(&endpoint, Duration::from_secs(SMOKE_TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("Smoke B FAIL: GrpcDpsChannel::connect — {e:?}"));

    let dummy_sign = CheckSignBlob(vec![0u8; 32]);
    let fictitious_expected = "FAKE-W12-HARDENING-SMOKE-2026-05-25";
    let result = channel
        .by_server_fiscal_no(&dummy_sign, fictitious_expected)
        .await;

    println!("Response: {result:?}\n");
    match result {
        Err(DpsError::NotFound) => {
            println!(
                "Smoke B PASS: NotFound — the SentReplay SentNotFound path now \
                 escalates to RequiresManualReconciliation + node STOP (CS-3 \
                 S7-1 F2-kvt2; R6 retired the old ErRedriveQueued redrive)."
            );
        }
        Err(DpsError::ServerFiscalIdMismatch {
            expected_id,
            actual_id,
        }) => {
            println!(
                "Smoke B PASS: Mismatch — W12 StructuralDrift path would \
                 correctly halt FN drain з KVT2_CONFIRM_STRUCTURAL_DRIFT \
                 audit.  expected={expected_id} actual={actual_id}"
            );
        }
        Err(DpsError::Server { code, message }) => {
            println!(
                "Smoke B PASS: Server error — W12 Hold(DpsServer) routing \
                 would map to FailureClass::Server (REC-6 granular).  \
                 code={code} message={message}"
            );
        }
        Err(DpsError::Authorization {
            code,
            kind,
            message,
        }) => {
            println!(
                "Smoke B PASS: Authorization — W12 Hold(DpsAuthorization) \
                 routing would map to FailureClass::Authorization (REC-6).  \
                 code={code} kind={kind:?} message={message}"
            );
        }
        Err(DpsError::Decode(msg)) => {
            println!(
                "Smoke B PASS: Decode — W12 Hold(DpsDecode) routing would \
                 map to FailureClass::Decode (REC-6).  Error: {msg}"
            );
        }
        Ok(ack) => {
            println!(
                "Smoke B PASS (unexpected ack — fictitious id matched real \
                 registration OR DPS quirk): ack.id={:?}, data_sign_len={}",
                ack.id,
                ack.data_sign.len()
            );
        }
        Err(DpsError::Transport(msg)) => {
            panic!(
                "Smoke B FAIL: Transport error на by_server_fiscal_no path — \
                 hardening layer would treat as auto-offline trigger per \
                 memory feedback_offline_transition_strategy.  Error: {msg}"
            );
        }
        Err(other) => {
            panic!(
                "Smoke B FAIL: unhandled DpsError variant — W12 routing \
                 matrix may not cover this case.  Variant: {other:?}"
            );
        }
    }
}
