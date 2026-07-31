#![cfg(feature = "live-dps")]
//! **RULING 2 §4 live capture — the ambiguous / lost-response T=112.**
//!
//! `docs/RULINGS_2026-07-10_SHIFT_T112_AUTOZ.md` §4 requires ONE real capture on
//! the DPS test cabinet: kill the connection mid-call and record what a
//! subsequent fresh T=112 returns.  Until that lands, two mutually exclusive
//! hypotheses about what a lost response COSTS are both on record as fact:
//!
//! - **H1** — each T=112 allocates a FRESH range, so a lost response LEAKS it
//!   server-side.  A flapping link then burns a range per break, eating the
//!   offline code-reserve floor (bd `PRRO_GATE-255`) and the monthly allocation.
//! - **H2** — DPS RE-ISSUES allocated-but-unconsumed codes on the next T=112, so
//!   the ambiguous case is free.
//!
//! The live-campaign observation behind H2 ("the same opaque codes returned
//! across our runs") never exercised this case: **no run ever lost a response
//! mid-call**.  This harness produces exactly that.
//!
//! It settles a SECOND unknown at the same time.  If DPS did process the killed
//! request, its chain tip advanced while ours did not, so the next fiscal send
//! earns `-12 ERROR_BAD_HASH_PREV` — which routes to the AUTOMATIC bounded
//! `MacRecovery`, not an operator MacReseed.  That self-heal parses a literal
//! `"store "` tag out of the DPS message, a format inherited from the Python
//! reference and **never observed from live DPS**.  Phase D captures a real
//! `-12` and runs the production extractor against it.
//!
//! ## Why a TCP forwarder and not a client-side timeout
//!
//! A client-side timeout proves nothing: it cannot establish that the request
//! reached DPS, which is the entire point of "ambiguous".  The forwarder relays
//! the request to the real cabinet and only then tears the connection down, so
//! the bytes provably traversed the network.  It also RECORDS whether the server
//! had begun replying before the tear-down — direct evidence that DPS processed
//! the request rather than merely received it.
//!
//! TLS stays end-to-end: the forwarder relays ciphertext and never terminates
//! TLS, so no MITM certificate is involved and the peer remains genuinely
//! authenticated.  The client reaches it via `127.0.0.1` while validating the
//! REAL cabinet certificate, using tonic's TLS `domain_name` override.  That is
//! why phase B builds its own channel instead of `GrpcDpsChannel::connect`:
//! production has no domain-override seam and **must not grow one for a test**
//! (`src/` stays frozen — this file adds zero production diff).  The bytes on the
//! wire are byte-identical to production's: same XML, same ATTACHED CAdES-BES.
//!
//! ## Gates (ALL required — this experiment is DESTRUCTIVE)
//!
//! 1. cargo feature `live-dps` — the file does not compile without it;
//! 2. `#[ignore]` — never runs in a default `cargo test`;
//! 3. `PRRO_LIVE_DPS=1` — the standard live kill-switch;
//! 4. `PRRO_LIVE_DPS_CAPTURE=1` — an ADDITIONAL gate specific to this file,
//!    because under H1 every run permanently burns a code range on the test FN.
//!    A normal live smoke is read-mostly; this one is not, so it does not ride
//!    the same switch;
//! 5. the default-deny TEST-cabinet host allowlist (duplicated from
//!    `live_dps_extended_smoke.rs`, deliberately NOT relaxed).
//!
//! Run:
//! ```text
//! PRRO_LIVE_DPS=1 PRRO_LIVE_DPS_CAPTURE=1 \
//! PRRO_LIVE_DPS_JKS_PATH=... PRRO_LIVE_DPS_JKS_PASS=... \
//! cargo test -p prro --features live-dps,test-support \
//!   --test live_capture_ambiguous_t112 -- --ignored --nocapture
//! ```
//!
//! The output is a CAPTURE LOG meant to be pasted verbatim into bd
//! `PRRO_GATE-2ds`.  This test asserts almost nothing on purpose: it is an
//! EXPERIMENT, and its job is to record what DPS actually did, not to enforce a
//! contract we have not yet earned the right to state.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use prro_crypto::cms::builder::{CmsBuildOptions, CmsSigner};
use prro_crypto::cms::profile::CmsProfile;
use prro_crypto::cms::signed_data::extract_econtent;
use prro_crypto::cms::signer::DstuInProcessSigner;
use prro_crypto::core::curve::Curve;
use prro_crypto::core::field::FieldEl;
use prro_crypto::interop::prro::containers::{extract_private_key, ExtractedKey};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use prro::transports::dps::channel::DpsChannel;
use prro::transports::dps::dto::{CheckEnvelope, CheckSignBlob, DpsCheckType};
use prro::transports::dps::error::DpsError;
use prro::transports::dps::grpc::GrpcDpsChannel;

// ─── Env contract (duplicated: `live_dps_extended_smoke.rs` is a CS-1 FROZEN
//     file and its helpers are private; duplicating ~60 lines is cheaper than
//     unfreezing it, and the allowlist below is copied VERBATIM on purpose) ───

const ENV_GATE: &str = "PRRO_LIVE_DPS";
/// Second, capture-specific gate. This experiment may permanently consume a code
/// range on the test FN — that cost is the very quantity being measured — so it
/// does NOT ride the ordinary live-smoke switch alone.
const ENV_CAPTURE_GATE: &str = "PRRO_LIVE_DPS_CAPTURE";
const ENV_HOST: &str = "PRRO_LIVE_DPS_HOST";
const ENV_FN: &str = "PRRO_LIVE_DPS_FN";
const ENV_JKS_PATH: &str = "PRRO_LIVE_DPS_JKS_PATH";
const ENV_JKS_PASS: &str = "PRRO_LIVE_DPS_JKS_PASS";

const DEFAULT_HOST: &str = "https://cabinet.tax.gov.ua:9443";
const DEFAULT_FN: &str = "4000162280";
const DEFAULT_TN: &str = "13667753";

/// Default-deny allowlist marker — see `live_dps_extended_smoke.rs`. Every
/// production endpoint (`prro`, `prro2`, `fs`.tax.gov.ua) is refused, as are
/// lookalikes such as `cabinet.tax.gov.ua.evil.com`.
const TEST_HOST_MARKER: &str = "cabinet.tax.gov.ua";

const TIMEOUT_SECS: u64 = 15;

// (the quiet-period trigger was removed after the first live run — see the
// forwarder: DPS answered inside the window and the kill never fired)
/// Minimum client→server application bytes before the tear-down is armed. A
/// signed T=112 is several KB, so this cannot fire during the TLS handshake.
const KILL_MIN_CLIENT_BYTES: u64 = 1024;

fn resolve_host() -> String {
    std::env::var(ENV_HOST).unwrap_or_else(|_| DEFAULT_HOST.to_string())
}

fn resolve_fn() -> String {
    std::env::var(ENV_FN).unwrap_or_else(|_| DEFAULT_FN.to_string())
}

fn resolve_tn() -> String {
    std::env::var("PRRO_LIVE_DPS_TN").unwrap_or_else(|_| DEFAULT_TN.to_string())
}

/// Extract the bare hostname — authority first, so a query/fragment containing
/// `@cabinet…` can never masquerade as the host.
fn host_of(endpoint: &str) -> &str {
    let after_scheme = endpoint.split("://").nth(1).unwrap_or(endpoint);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    hostport.split(':').next().unwrap_or(hostport)
}

fn port_of(endpoint: &str) -> u16 {
    let after_scheme = endpoint.split("://").nth(1).unwrap_or(endpoint);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    hostport
        .rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or(443)
}

/// All five gates. Returns `false` (with a printed SKIP) when unarmed; PANICS
/// on a non-test host rather than skipping — a misdirected destructive run must
/// be loud, not silently absent.
fn capture_armed(name: &str) -> bool {
    if std::env::var(ENV_GATE).as_deref() != Ok("1") {
        println!("=== {name} SKIP: set {ENV_GATE}=1 (live kill-switch) ===");
        return false;
    }
    if std::env::var(ENV_CAPTURE_GATE).as_deref() != Ok("1") {
        println!(
            "=== {name} SKIP: set {ENV_CAPTURE_GATE}=1 as well.\n\
             This capture is DESTRUCTIVE: under hypothesis H1 each run permanently\n\
             burns an offline-code range on the test FN. That cost is exactly what\n\
             is being measured, so it needs its own explicit arming. ==="
        );
        return false;
    }
    let endpoint = resolve_host();
    let host = host_of(&endpoint);
    let allowed = host == TEST_HOST_MARKER
        || host.ends_with(&format!(".{TEST_HOST_MARKER}"))
        || host.ends_with(&format!("-{TEST_HOST_MARKER}"));
    assert!(
        allowed,
        "{name} REFUSED: {ENV_HOST}={endpoint} resolves to host `{host}`, which is not a \
         DPS TEST cabinet (allowlist: `{TEST_HOST_MARKER}` / `*-{TEST_HOST_MARKER}` / \
         `*.{TEST_HOST_MARKER}`). Refusing to burn offline codes against a production \
         endpoint (prro/prro2/fs.tax.gov.ua)."
    );
    true
}

fn load_signing_key(name: &str) -> Option<ExtractedKey> {
    let (Some(path), Some(pass)) = (
        std::env::var(ENV_JKS_PATH).ok(),
        std::env::var(ENV_JKS_PASS).ok(),
    ) else {
        println!("=== {name} SKIP: set {ENV_JKS_PATH} + {ENV_JKS_PASS} ===");
        return None;
    };
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: cannot read JKS at {path}: {e}"));
    Some(
        extract_private_key(&bytes, &pass)
            .unwrap_or_else(|e| panic!("{name}: extract_private_key failed: {e:?}")),
    )
}

/// ATTACHED CAdES-BES over arbitrary bytes — the same profile `sendChkV2`
/// requires, and the same one production uses. Embeds the SIGNING cert, never
/// `certs[0]` (a UA EDS keystore also holds a key-agreement cert; embedding it
/// draws `CryptBadSign`).
fn cades_sign(ek: &ExtractedKey, payload: &[u8]) -> Vec<u8> {
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
    cms.sign_with(
        payload,
        CmsBuildOptions {
            attached: true,
            signing_time: Some(SystemTime::now()),
        },
    )
    .expect("ATTACHED CAdES-BES sign must succeed")
    .cms_der
}

fn sign_fn_blob(ek: &ExtractedKey, fiscal_number: &str) -> CheckSignBlob {
    CheckSignBlob(cades_sign(ek, fiscal_number.as_bytes()))
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Kyiv wall-clock as the `yyyyMMddHHmmss` NUMBER WebCheck puts in `<TS>` — not
/// an epoch. Live probes (2026-07-07) proved both epoch forms draw `-8`.
fn kyiv_comp_date() -> i64 {
    use chrono::{Datelike, TimeZone, Timelike};
    let utc_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_secs() as i64;
    let kyiv = chrono::Utc
        .timestamp_opt(utc_secs, 0)
        .single()
        .expect("valid epoch")
        .with_timezone(&chrono_tz::Europe::Kiev);
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        kyiv.year(),
        kyiv.month(),
        kyiv.day(),
        kyiv.hour(),
        kyiv.minute(),
        kyiv.second()
    )
    .parse()
    .expect("yyyyMMddHHmmss fits i64")
}

/// Byte-exact WebCheck T=112 request. Note the space in `V = '1'` on the `RQ`
/// tag — verbatim from the decompile.
#[rustfmt::skip]
fn build_t112_xml(fiscal_number: &str, tn: &str, di: u64, size: u32, ts: i64, mac_hex: &str) -> String {
    format!(
        "<RQ V = '1'><DAT FN='{fn}' TN='{tn}' ZN='' DI='{di}' V='1'><C T='112'><H SIZE='{size}'></H></C><TS>{ts}</TS></DAT><MAC>{mac}</MAC></RQ>",
        fn = fiscal_number, tn = tn, di = di, size = size, ts = ts, mac = mac_hex,
    )
}

/// Parse the `<ID>` offline codes out of a CMS-wrapped `data_sign`
/// (WebCheck `SendingOfflineChecksRobot.cs:659-667`). Deliberately tolerant: we
/// scan the raw bytes for the tag rather than DER-parsing, because the point is
/// to COMPARE code sets across phases, not to validate the envelope.
fn parse_offline_codes(data_sign: &[u8]) -> BTreeSet<String> {
    let text = String::from_utf8_lossy(data_sign);
    let mut out = BTreeSet::new();
    let mut rest: &str = &text;
    while let Some(start) = rest.find("<ID>") {
        let after = &rest[start + 4..];
        match after.find("</ID>") {
            Some(end) => {
                let code = after[..end].trim();
                if !code.is_empty() {
                    out.insert(code.to_string());
                }
                rest = &after[end + 5..];
            }
            None => break,
        }
    }
    out
}

// ─── The forwarder ──────────────────────────────────────────────────────────

/// What the forwarder observed. `server_bytes_before_kill > 0` is the strong
/// signal: DPS had begun replying, so it did not merely RECEIVE the request, it
/// PROCESSED it far enough to answer.
#[derive(Debug, Default)]
struct KillReport {
    client_to_server: u64,
    server_to_client: u64,
    server_replied_before_kill: bool,
    killed: bool,
}

/// A TCP forwarder that relays ciphertext to the real cabinet and tears the
/// connection down once the client's request is through.
///
/// It never terminates TLS (so the peer stays genuinely authenticated and no
/// MITM certificate exists) and therefore cannot read the request — the
/// tear-down is timed on the client going QUIET after pushing
/// `KILL_MIN_CLIENT_BYTES`, which for a several-KB signed T=112 cannot coincide
/// with the handshake.
///
/// The tear-down is a plain drop (FIN both ways). That is deliberately the
/// gentler of the two options: h2 surfaces it as a transport error with the
/// response never delivered, which is the ambiguous shape we want, while a RST
/// would additionally risk the server aborting its own commit.
async fn spawn_kill_forwarder(
    upstream_host: String,
    upstream_port: u16,
    report: Arc<KillReportCell>,
) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral loopback port");
    let local_port = listener.local_addr().expect("local addr").port();

    tokio::spawn(async move {
        let (inbound, _) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("forwarder: accept failed: {e}");
                return;
            }
        };
        let outbound = match TcpStream::connect((upstream_host.as_str(), upstream_port)).await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("forwarder: upstream connect failed: {e}");
                return;
            }
        };

        let (mut cr, mut cw) = inbound.into_split();
        let (mut sr, mut sw) = outbound.into_split();

        let c2s = report.client_to_server.clone();
        let s2c = report.server_to_client.clone();
        let request_through_up = report.request_through.clone();
        let request_through_down = report.request_through.clone();
        let server_replied = report.server_replied.clone();

        // client → server: relay everything, and mark when enough has flowed.
        let up = tokio::spawn(async move {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let n = match cr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                if sw.write_all(&buf[..n]).await.is_err() {
                    break;
                }
                let total = c2s.fetch_add(n as u64, Ordering::SeqCst) + n as u64;
                if total >= KILL_MIN_CLIENT_BYTES {
                    request_through_up.store(true, Ordering::SeqCst);
                }
            }
        });

        // server → client: relay the handshake, then KILL on the first reply
        // byte that arrives after the request was relayed.
        //
        // The first live run learned this the hard way. The tear-down was
        // originally armed on "the client has gone quiet for KILL_QUIET_MS",
        // and DPS answered inside that window — the RPC completed normally
        // (`killed=false`, 4493 B relayed back) and the ambiguous case was
        // never produced. Replying-time is the correct trigger: it is BOTH the
        // strongest available witness that DPS PROCESSED the request (not
        // merely received it) AND the exact instant to drop, so the response
        // never reaches the client.
        let killed_flag = report.killed.clone();
        let down = tokio::spawn(async move {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let n = match sr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                if request_through_down.load(Ordering::SeqCst) {
                    s2c.fetch_add(n as u64, Ordering::SeqCst);
                    server_replied.store(true, Ordering::SeqCst);
                    killed_flag.store(true, Ordering::SeqCst);
                    // Deliberately NOT forwarded — this is the lost response.
                    break;
                }
                if cw.write_all(&buf[..n]).await.is_err() {
                    break;
                }
                s2c.fetch_add(n as u64, Ordering::SeqCst);
            }
            // `cw` / `sr` drop here → FIN both ways → the in-flight RPC fails
            // with a transport error and the response is never delivered.
        });

        // Tear the other half down once the kill has fired (or the connection
        // ended on its own). Bounded so a stalled peer cannot hang the test.
        let watch = report.clone();
        for _ in 0..(TIMEOUT_SECS * 100) {
            if watch.killed.load(Ordering::SeqCst) || down.is_finished() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        up.abort();
        down.abort();
    });

    local_port
}

#[derive(Default)]
struct KillReportCell {
    client_to_server: Arc<AtomicU64>,
    server_to_client: Arc<AtomicU64>,
    request_through: Arc<AtomicBool>,
    server_replied: Arc<AtomicBool>,
    killed: Arc<AtomicBool>,
}

impl KillReportCell {
    fn snapshot(&self) -> KillReport {
        KillReport {
            client_to_server: self.client_to_server.load(Ordering::SeqCst),
            server_to_client: self.server_to_client.load(Ordering::SeqCst),
            server_replied_before_kill: self.server_replied.load(Ordering::SeqCst),
            killed: self.killed.load(Ordering::SeqCst),
        }
    }
}

// ─── The capture ────────────────────────────────────────────────────────────

/// **RULING 2 §4 — the ambiguous T=112 capture.** Four phases; see the module
/// docs. Asserts almost nothing by design: this records what DPS did.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "live DPS capture; DESTRUCTIVE (may burn an offline-code range). Needs PRRO_LIVE_DPS=1 + PRRO_LIVE_DPS_CAPTURE=1"]
async fn live_capture_ambiguous_t112_connection_kill() {
    const NAME: &str = "T=112 ambiguous capture";
    if !capture_armed(NAME) {
        return;
    }
    let Some(ek) = load_signing_key(NAME) else {
        return;
    };

    let endpoint = resolve_host();
    let fiscal_number = resolve_fn();
    let tn = resolve_tn();
    let upstream_host = host_of(&endpoint).to_string();
    let upstream_port = port_of(&endpoint);

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║ RULING 2 §4 CAPTURE — ambiguous / lost-response T=112         ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!("Endpoint : {endpoint}  (host={upstream_host} port={upstream_port})");
    println!("FN       : {fiscal_number}   TN: {tn}");
    println!("Paste this whole log into bd PRRO_GATE-2ds.\n");

    let channel = GrpcDpsChannel::connect(&endpoint, Duration::from_secs(TIMEOUT_SECS))
        .await
        .unwrap_or_else(|e| panic!("{NAME} FAIL: connect: {e:?}"));
    let fn_sign = sign_fn_blob(&ek, &fiscal_number);

    // Chain tip → MAC for the request. Empty data_sign = genesis.
    // MAC = sha256-hex of the **CMS-STRIPPED** previous check bytes, not of the
    // raw `data_sign`. Getting this wrong is not subtle-but-harmless: the first
    // live run hashed `data_sign` directly and every phase drew `-12`
    // ERROR_BAD_HASH_PREV, so no phase ever reached the code path under test.
    // Empty for a genesis FN.
    let mac_of = |ack: &prro::transports::dps::dto::CheckAck| -> String {
        if ack.data_sign.is_empty() {
            return String::new();
        }
        let inner =
            extract_econtent(&ack.data_sign).unwrap_or_else(|e| panic!("CMS-strip data_sign: {e}"));
        hex_lower(&Sha256::digest(&inner))
    };

    let tip0 = channel
        .last_chk(&fn_sign)
        .await
        .unwrap_or_else(|e| panic!("{NAME} FAIL: pre lastChk: {e:?}"));
    println!("--- TIP BEFORE ANYTHING ---");
    println!("  id={:?}  data_sign={} B", tip0.id, tip0.data_sign.len());

    // ── PHASE A — a normal T=112, to establish the baseline code set ──────
    println!("\n--- PHASE A: normal T=112 (baseline) ---");
    let ts_a = kyiv_comp_date();
    let xml_a = build_t112_xml(&fiscal_number, &tn, 1, 1, ts_a, &mac_of(&tip0));
    println!("  XML: {xml_a}");
    let ack_a = channel
        .send_chk(CheckEnvelope {
            rro_fn: fiscal_number.clone(),
            date_time: ts_a,
            check_sign: cades_sign(&ek, xml_a.as_bytes()),
            local_number: 1,
            check_type: DpsCheckType::ServiceChk,
            id_offline: String::new(),
            id_cancel: String::new(),
        })
        .await;
    let codes_a = match &ack_a {
        Ok(ack) => {
            let c = parse_offline_codes(&ack.data_sign);
            println!("  OK id={:?}  codes={:?}", ack.id, c);
            c
        }
        Err(e) => {
            println!("  phase A did not return codes: {e:?}");
            BTreeSet::new()
        }
    };

    // ── PHASE B — the SAME request, connection killed mid-call ────────────
    println!("\n--- PHASE B: T=112 with the connection KILLED mid-call ---");
    let cell = Arc::new(KillReportCell::default());
    let proxy_port = spawn_kill_forwarder(upstream_host.clone(), upstream_port, cell.clone()).await;
    println!("  forwarder: 127.0.0.1:{proxy_port} → {upstream_host}:{upstream_port}");

    let tip_b = channel.last_chk(&fn_sign).await.ok();
    let mac_b = tip_b.as_ref().map(mac_of).unwrap_or_default();
    let ts_b = kyiv_comp_date();
    let xml_b = build_t112_xml(&fiscal_number, &tn, 2, 1, ts_b, &mac_b);
    println!("  XML: {xml_b}");

    let killed_result = send_t112_through(
        &format!("https://127.0.0.1:{proxy_port}"),
        &upstream_host,
        CheckEnvelope {
            rro_fn: fiscal_number.clone(),
            date_time: ts_b,
            check_sign: cades_sign(&ek, xml_b.as_bytes()),
            local_number: 2,
            check_type: DpsCheckType::ServiceChk,
            id_offline: String::new(),
            id_cancel: String::new(),
        },
    )
    .await;

    let report = cell.snapshot();
    println!("  RPC outcome (expected: transport error): {killed_result:?}");
    println!(
        "  forwarder: client→server {} B, server→client {} B, killed={}",
        report.client_to_server, report.server_to_client, report.killed
    );
    println!(
        "  *** DID DPS BEGIN REPLYING BEFORE THE KILL? {} ***",
        if report.server_replied_before_kill {
            "YES — DPS processed the request, not merely received it"
        } else {
            "no — cannot conclude DPS processed it"
        }
    );

    // ── PHASE C — a fresh T=112. THE decisive observation ─────────────────
    println!("\n--- PHASE C: fresh T=112 (H1 vs H2) ---");
    let tip_c = channel.last_chk(&fn_sign).await.ok();
    let mac_c = tip_c.as_ref().map(mac_of).unwrap_or_default();
    let ts_c = kyiv_comp_date();
    let xml_c = build_t112_xml(&fiscal_number, &tn, 3, 1, ts_c, &mac_c);
    let ack_c = channel
        .send_chk(CheckEnvelope {
            rro_fn: fiscal_number.clone(),
            date_time: ts_c,
            check_sign: cades_sign(&ek, xml_c.as_bytes()),
            local_number: 3,
            check_type: DpsCheckType::ServiceChk,
            id_offline: String::new(),
            id_cancel: String::new(),
        })
        .await;
    let codes_c = match &ack_c {
        Ok(ack) => {
            let c = parse_offline_codes(&ack.data_sign);
            println!("  OK id={:?}  codes={:?}", ack.id, c);
            c
        }
        Err(e) => {
            println!("  phase C error: {e:?}");
            BTreeSet::new()
        }
    };

    let overlap: Vec<&String> = codes_a.intersection(&codes_c).collect();
    println!("\n  ┌─ VERDICT INPUT ─────────────────────────────────────────┐");
    println!("  │ phase A codes : {codes_a:?}");
    println!("  │ phase C codes : {codes_c:?}");
    println!("  │ overlap A∩C   : {overlap:?}");
    println!(
        "  │ reading       : {}",
        if codes_c.is_empty() {
            "inconclusive — phase C returned no codes"
        } else if overlap.is_empty() {
            "supports H1 (fresh allocation each call → the killed range LEAKED)"
        } else {
            "supports H2 (re-issue of allocated-but-unconsumed codes → ambiguous is free)"
        }
    );
    println!("  └─────────────────────────────────────────────────────────┘");

    // ── PHASE D — did the killed call move DPS's tip? ─────────────────────
    // If it did, our next send carries a stale previous_hash and earns `-12`,
    // whose message must contain the `"store "` tag the production extractor
    // parses. That parse has never been seen against live DPS.
    println!("\n--- PHASE D: tip divergence + the real `-12` message ---");
    match channel.last_chk(&fn_sign).await {
        Ok(tip_d) => {
            println!(
                "  tip after: id={:?} data_sign={} B",
                tip_d.id,
                tip_d.data_sign.len()
            );
            println!(
                "  tip moved vs. before-anything: {}",
                tip_d.data_sign != tip0.data_sign
            );
        }
        Err(e) => println!("  post lastChk error: {e:?}"),
    }
    // Deliberately send with a KNOWN-STALE mac to force `-12` and capture its
    // raw text. This costs nothing (it is rejected, not fiscalized) and is the
    // only way to see the real message shape.
    let ts_d = kyiv_comp_date();
    let stale_mac = hex_lower(&[0u8; 32]);
    let xml_d = build_t112_xml(&fiscal_number, &tn, 4, 1, ts_d, &stale_mac);
    let ack_d = channel
        .send_chk(CheckEnvelope {
            rro_fn: fiscal_number.clone(),
            date_time: ts_d,
            check_sign: cades_sign(&ek, xml_d.as_bytes()),
            local_number: 4,
            check_type: DpsCheckType::ServiceChk,
            id_offline: String::new(),
            id_cancel: String::new(),
        })
        .await;
    match &ack_d {
        Err(DpsError::Server { code, message }) => {
            println!("  forced stale-MAC send → server {code}: {message:?}");
            let extracted =
                prro::services::write_path::mac_recovery::regex_extract_store_hash(message);
            println!(
                "  *** production `\"store \"` extractor on a REAL DPS message: {} ***",
                match &extracted {
                    Some(h) => format!("PARSED {}", hex_lower(h)),
                    None => "NOT EXTRACTABLE — the Python-derived format does not match live DPS"
                        .to_string(),
                }
            );
        }
        other => println!("  forced stale-MAC send → {other:?} (no `-12` to inspect)"),
    }

    println!("\n=== CAPTURE COMPLETE — paste the whole log into bd PRRO_GATE-2ds ===");
}

/// Send one T=112 through an arbitrary endpoint while validating the REAL
/// cabinet certificate (`domain_name` override), so the forwarder can sit on
/// loopback without terminating TLS.
///
/// This duplicates the two lines of `GrpcDpsChannel` that matter for the wire —
/// deliberately, so production needs no test-only domain-override seam.
async fn send_t112_through(
    endpoint: &str,
    tls_domain: &str,
    envelope: CheckEnvelope,
) -> Result<(), String> {
    use prro::transports::dps::gen::chk_income_service_client::ChkIncomeServiceClient;
    use tonic::transport::{Channel, ClientTlsConfig, Endpoint};

    let tls = ClientTlsConfig::new()
        .with_native_roots()
        .domain_name(tls_domain.to_string());
    let ep: Endpoint = Endpoint::from_shared(endpoint.to_string())
        .map_err(|e| format!("endpoint: {e}"))?
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(TIMEOUT_SECS))
        .tls_config(tls)
        .map_err(|e| format!("tls: {e}"))?;
    let ch: Channel = ep.connect().await.map_err(|e| format!("connect: {e}"))?;

    ChkIncomeServiceClient::new(ch)
        .send_chk_v2(tonic::Request::new(envelope.into()))
        .await
        .map(|_| ())
        .map_err(|st| format!("{st:?}"))
}
