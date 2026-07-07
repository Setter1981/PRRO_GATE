//! `prro doctor --live` — FormTest-parity live diagnostic against DPS.
//!
//! Parity reference: `docs/webcheck_reverse/WebCheckMain/WebCheck/FormTest.cs`
//! (calls `CheckLastCheck.NumberLastCheckTest`) and `CheckLastCheck.cs`.
//!
//! ## Read-only contract
//! This module issues only READ RPCs (`status_rro`, `last_chk`) and only
//! SELECT queries against the local DB.  No document submission, no DB
//! writes, no state mutation.
//!
//! ## Unit-testable surface (verdict functions)
//! All verdict logic lives in pure functions that accept plain values — they
//! are tested in `tests/doctor_live_verdicts.rs`.  The network-touching
//! `run_live` function is thin glue; it is verified only by a live operator
//! run with a real key + FN.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::db::repositories::{fiscal_documents, fiscal_number_config as fn_cfg};
use crate::transports::dps::channel::DpsChannel;
use crate::transports::dps::dto::CheckSignBlob;
use crate::transports::dps::error::DpsError;

// ── Pure verdict types ──────────────────────────────────────────────────

/// Classification of the operator's signing certificate validity window.
#[derive(Debug)]
pub enum CertWindowVerdict {
    /// Certificate valid; ≥30 days remain.
    Ok {
        not_after: String,
        days_remaining: i64,
    },
    /// Certificate valid but expires in <30 days — operator should rotate.
    Warn {
        not_after: String,
        days_remaining: i64,
    },
    /// Certificate has already expired — DPS will reject any signed payload.
    Expired { not_after: String },
}

/// Classify the signing-cert validity window against `now`.
///
/// `not_after_rfc3339` is the cert's `notAfter` in RFC 3339 format as
/// returned by `prro_crypto::cms::envelope::parse_cert_basic_fields`.
pub fn classify_cert_window(not_after_rfc3339: &str, now: DateTime<Utc>) -> CertWindowVerdict {
    let parsed = DateTime::parse_from_rfc3339(not_after_rfc3339).map(|d| d.with_timezone(&Utc));
    match parsed {
        Err(_) => CertWindowVerdict::Expired {
            not_after: not_after_rfc3339.to_string(),
        },
        Ok(expiry) => {
            let days = (expiry - now).num_days();
            if days < 0 {
                CertWindowVerdict::Expired {
                    not_after: not_after_rfc3339.to_string(),
                }
            } else if days < 30 {
                CertWindowVerdict::Warn {
                    not_after: not_after_rfc3339.to_string(),
                    days_remaining: days,
                }
            } else {
                CertWindowVerdict::Ok {
                    not_after: not_after_rfc3339.to_string(),
                    days_remaining: days,
                }
            }
        }
    }
}

/// Classification of the local ledger tip vs server chain tip.
#[derive(Debug)]
pub enum LedgerSyncVerdict {
    /// Local and server tips agree — ledger is in sync.
    InSync { sfn: String },
    /// Tips differ — possible ledger divergence; operator should investigate.
    Diverged { local: String, server: String },
    /// No submitted doc in the local DB — informational; not an error.
    NoLocalState,
}

/// Compare the local DB's last submitted `server_fiscal_no` against the
/// server-returned tip id from `last_chk`.
pub fn classify_ledger_sync(local_tip: Option<String>, server_id: &str) -> LedgerSyncVerdict {
    match local_tip {
        None => LedgerSyncVerdict::NoLocalState,
        Some(local) if local == server_id => LedgerSyncVerdict::InSync { sfn: local },
        Some(local) => LedgerSyncVerdict::Diverged {
            local,
            server: server_id.to_string(),
        },
    }
}

/// Classification of the local offline code-pool level.
#[derive(Debug)]
pub enum OfflinePoolVerdict {
    /// Available codes ≥ configured minimum.
    Ok { available: i64, min: i64 },
    /// Available codes < configured minimum — operator should seed more.
    Warn { available: i64, min: i64 },
}

/// Classify the offline code-pool level against the configured minimum.
pub fn classify_offline_pool(available: i64, min_codes: i64) -> OfflinePoolVerdict {
    if available >= min_codes {
        OfflinePoolVerdict::Ok {
            available,
            min: min_codes,
        }
    } else {
        OfflinePoolVerdict::Warn {
            available,
            min: min_codes,
        }
    }
}

// ── Live-section args ───────────────────────────────────────────────────

/// Arguments passed to the live section of `prro doctor`.
pub struct LiveArgs {
    /// Fiscal number to probe (passed via `--fn`; required for `--live`).
    pub fiscal_number: String,
    /// DPS endpoint URL (default `https://cabinet.tax.gov.ua:9443`).
    pub host: String,
}

// ── Live-section runner (network-touching; not unit-tested) ────────────

/// Execute the five live checks (Key → Reachability → ServerState →
/// LedgerSync → OfflinePool) using an already-connected `channel` and
/// an already-built `fn_sign` blob.
///
/// Returns `true` if all checks are OK or WARN (non-blocking), `false` if
/// any check is FAIL (blocking — caller should surface a non-zero exit).
///
/// This function is called by `run_live_from_env` after key loading and
/// channel construction; it is separate so future tests can supply a stub
/// channel without going through env-var key loading.
pub async fn run_live(
    pool: &SqlitePool,
    channel: &dyn DpsChannel,
    fn_sign: &CheckSignBlob,
    fiscal_number: &str,
    cert_not_after: &str,
    cert_subject_dn: &str,
) -> bool {
    let now = Utc::now();
    let mut all_ok = true;

    // ── Check 1: cert validity window ──────────────────────────────────
    let cert_verdict = classify_cert_window(cert_not_after, now);
    match &cert_verdict {
        CertWindowVerdict::Ok {
            not_after,
            days_remaining,
        } => {
            println!(
                "[OK]   key:    cert valid until {not_after} ({days_remaining} days) — {cert_subject_dn}"
            );
        }
        CertWindowVerdict::Warn {
            not_after,
            days_remaining,
        } => {
            println!(
                "[WARN] key:    cert expires soon — {not_after} ({days_remaining} days) — {cert_subject_dn}"
            );
        }
        CertWindowVerdict::Expired { not_after } => {
            println!("[FAIL] key:    cert EXPIRED — {not_after} — {cert_subject_dn}");
            all_ok = false;
        }
    }

    // ── Check 2: reachability — already connected at this point ────────
    println!("[OK]   reach:  DPS channel connected to server");

    // ── Check 3: server state ───────────────────────────────────────────
    match channel.status_rro(fn_sign).await {
        Ok(s) => {
            println!(
                "[OK]   status: open_shift={} online={} last_signer={}",
                s.open_shift, s.online, s.last_signer
            );
        }
        Err(e) => {
            println!("[FAIL] status: status_rro failed — {e}");
            all_ok = false;
        }
    }

    // last_chk — server chain tip + derived chain-MAC.
    // Chain convention (WebCheck + live smokes `seed_mac_from_lastchk`): the
    // value the NEXT document must carry in <MAC> is SHA-256 over the
    // CMS-STRIPPED eContent of the tip's data_sign — NOT over the whole CMS
    // blob.
    let server_tip_id: Option<String> = match channel.last_chk(fn_sign).await {
        Ok(ack) => {
            if ack.data_sign.is_empty() {
                println!(
                    "[OK]   last_chk: id={} data_sign empty — FN genesis (next <MAC> is empty)",
                    ack.id,
                );
            } else {
                match prro_crypto::cms::signed_data::extract_econtent(&ack.data_sign) {
                    Ok(inner) => {
                        let mac = Sha256::digest(&inner);
                        println!(
                            "[OK]   last_chk: id={} data_sign_len={} chain_mac_sha256={}",
                            ack.id,
                            ack.data_sign.len(),
                            hex_lower(&mac),
                        );
                    }
                    Err(e) => {
                        println!(
                            "[FAIL] last_chk: id={} data_sign_len={} CMS eContent strip failed: {e}",
                            ack.id,
                            ack.data_sign.len(),
                        );
                        all_ok = false;
                    }
                }
            }
            Some(ack.id)
        }
        Err(DpsError::NotFound) => {
            println!(
                "[OK]   last_chk: NotFound — FN has no submitted docs on server (informational)"
            );
            None
        }
        Err(e) => {
            println!("[FAIL] last_chk: {e}");
            all_ok = false;
            None
        }
    };

    // ── Check 4: ledger sync ────────────────────────────────────────────
    let local_tip =
        match fiscal_documents::last_submitted_server_fiscal_no(pool, fiscal_number).await {
            Ok(tip) => tip,
            Err(e) => {
                println!("[FAIL] ledger:  DB query failed — {e}");
                all_ok = false;
                return all_ok;
            }
        };

    match server_tip_id {
        None => {
            // Server has no docs → ledger sync is vacuously informational.
            let verdict = classify_ledger_sync(local_tip, "");
            match verdict {
                LedgerSyncVerdict::NoLocalState => {
                    println!("[OK]   ledger:  NO_LOCAL_STATE (no server tip; no local tip)");
                }
                _ => {
                    // local has submitted docs but server has none — unusual but not a FAIL here
                    println!(
                        "[WARN] ledger:  local has submitted docs but server returned NotFound"
                    );
                }
            }
        }
        Some(ref sid) => {
            let verdict = classify_ledger_sync(local_tip, sid);
            match &verdict {
                LedgerSyncVerdict::InSync { sfn } => {
                    println!("[OK]   ledger:  IN_SYNC — sfn={sfn}");
                }
                LedgerSyncVerdict::Diverged { local, server } => {
                    println!("[FAIL] ledger:  DIVERGED — local_tip={local} server_tip={server}");
                    all_ok = false;
                }
                LedgerSyncVerdict::NoLocalState => {
                    println!(
                        "[OK]   ledger:  NO_LOCAL_STATE — server tip exists but local DB has \
                         no submitted docs (new or empty ledger)"
                    );
                }
            }
        }
    }

    // ── Check 5: offline code-pool ──────────────────────────────────────
    let fn_config = match fn_cfg::get(pool, fiscal_number).await {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("[FAIL] pool:    DB query failed — {e}");
            all_ok = false;
            return all_ok;
        }
    };

    match fn_config {
        None => {
            println!(
                "[SKIP] pool:    FN={fiscal_number} not in local fiscal_number_config (NO_CONFIG)"
            );
        }
        Some(cfg) => {
            let available: i64 = match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM offline_codes \
                 WHERE fiscal_number = ? AND consumed_at IS NULL",
            )
            .bind(fiscal_number)
            .fetch_one(pool)
            .await
            {
                Ok(n) => n,
                Err(e) => {
                    println!("[FAIL] pool:    DB query failed — {e}");
                    all_ok = false;
                    return all_ok;
                }
            };
            let verdict = classify_offline_pool(available, cfg.min_offline_codes);
            match verdict {
                OfflinePoolVerdict::Ok { available, min } => {
                    println!("[OK]   pool:    {available} available (min={min})");
                }
                OfflinePoolVerdict::Warn { available, min } => {
                    println!(
                        "[WARN] pool:    {available} available < min={min} — seed more codes \
                         (`prro admin seed-codes`)"
                    );
                }
            }
        }
    }

    all_ok
}

/// Entry point called from `doctor::run` when `--live` is set.
///
/// Loads the JKS key from `PRRO_LIVE_DPS_JKS_PATH` / `PRRO_LIVE_DPS_JKS_PASS`,
/// builds the `fn_sign` blob, connects `GrpcDpsChannel`, then delegates to
/// `run_live`.  Exits non-zero (via returned `Err`) if any FAIL verdict fires.
pub async fn run_live_from_env(pool: &SqlitePool, args: &LiveArgs) -> anyhow::Result<()> {
    use crate::crypto::session::SigningSession;
    use crate::runtime::key_loader::build_fn_sign;
    use prro_crypto::cms::envelope::parse_cert_basic_fields;
    use prro_crypto::interop::prro::containers::extract_private_key;

    println!(
        "\n== prro doctor --live (FN={} host={}) ==",
        args.fiscal_number, args.host
    );

    // ── Key loading ────────────────────────────────────────────────────
    let jks_path = std::env::var("PRRO_LIVE_DPS_JKS_PATH")
        .map_err(|_| anyhow::anyhow!("PRRO_LIVE_DPS_JKS_PATH not set (required for --live)"))?;
    let jks_pass = std::env::var("PRRO_LIVE_DPS_JKS_PASS")
        .map_err(|_| anyhow::anyhow!("PRRO_LIVE_DPS_JKS_PASS not set (required for --live)"))?;

    let jks_data = std::fs::read(&jks_path)
        .map_err(|e| anyhow::anyhow!("cannot read JKS at {jks_path}: {e}"))?;

    let extracted = extract_private_key(&jks_data, &jks_pass)
        .map_err(|e| anyhow::anyhow!("JKS load / decrypt failed: {e:?}"))?;

    // Parse cert fields from the signing cert BEFORE moving `extracted`.
    let cert_der: Vec<u8> = extracted
        .signing_cert()
        .ok_or_else(|| anyhow::anyhow!("no signing certificate found in JKS container"))?
        .to_vec();
    let cert_fields = parse_cert_basic_fields(&cert_der)
        .map_err(|e| anyhow::anyhow!("cert parse failed: {e:?}"))?;

    let session = SigningSession::from_extracted("diag".to_string(), extracted)
        .map_err(|e| anyhow::anyhow!("session assembly failed: {e:?}"))?;

    // Build the fn_sign blob (ATTACHED CAdES-BES over the FN string).
    let fn_sign = build_fn_sign(&session, &args.fiscal_number)
        .map_err(|e| anyhow::anyhow!("fn_sign build failed: {e:?}"))?;
    println!(
        "[OK]   key:    JKS loaded, fn_sign built ({} bytes)",
        fn_sign.0.len()
    );

    // ── Connect DPS ────────────────────────────────────────────────────
    use crate::transports::dps::grpc::GrpcDpsChannel;
    use std::time::Duration;

    let channel = GrpcDpsChannel::connect(&args.host, Duration::from_secs(15))
        .await
        .map_err(|e| anyhow::anyhow!("DPS connect failed: {e:?}"))?;

    // ── Run live checks ────────────────────────────────────────────────
    let all_ok = run_live(
        pool,
        &channel,
        &fn_sign,
        &args.fiscal_number,
        &cert_fields.valid_to,
        &cert_fields.subject_dn,
    )
    .await;

    if all_ok {
        println!("== ALL LIVE CHECKS PASSED ==");
        Ok(())
    } else {
        anyhow::bail!("-- LIVE CHECKS FAILED (see FAIL lines above) --")
    }
}

// ── Internal helpers ────────────────────────────────────────────────────

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
