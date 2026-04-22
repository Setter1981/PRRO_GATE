//! Command dispatcher — the core state-machine driver.
//!
//! `dispatch(&mut Session, Command, &Identity) -> Vec<Response>` is a
//! pure function: it mutates the session, produces the frames the
//! driver needs to send back, and never does I/O.  The real firmware
//! sends a sequence of frames per command (`WAIT` → work → data →
//! `DONE` → `READY`); we return a `Vec<Response>` in the exact order
//! it must hit the wire.
//!
//! Only synchronous commands are handled in M3.  Anything that would
//! normally talk to the bridge (`COMP`, `NREP`, reports) returns a
//! defensive `SOFTBLOCK` for now — M4/M5/M7 replace those arms with
//! real handlers.

use crate::bridge::{
    Bridge, BridgeError, CanonicalCommand, CanonicalResponse, CommandType, RawFrame,
    ReceiptDirection, ReceiptPayload,
};
use crate::bridge::dto::{classify_response, DocumentOutcome};
use crate::protocol::{
    error_codes::ErrorCode, Command, CompBuilder, ConfBuilder, ConfMode, Response, SysKey,
};

use super::state::{OpenReceipt, Session, SessionState};

/// Identity material that does not change during a session — set at
/// listener startup from config.  M3 keeps this borrowed; M6 wires it
/// through the listener.
#[derive(Debug, Clone)]
pub struct Identity {
    pub factory_serial: String,
    pub fiscal_number: String,
    pub merchant: String,
    pub fw_version_id: String,
    pub fw_build_date: String,
    pub currency: String,
    pub currency_date: String,
    pub decimals: u8,
    /// 10-char cashier password that `UPAS` must match.  Hardcoded
    /// to `"1111111111"` in the reference firmware.
    pub cashier_password: String,
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            factory_serial: "MRY304-DEV".to_string(),
            fiscal_number: "3000000000".to_string(),
            merchant: "Virtual Maria 304".to_string(),
            fw_version_id: "4120".to_string(),
            fw_build_date: "20250115".to_string(),
            currency: "Грн".to_string(),
            currency_date: "20250101".to_string(),
            decimals: 2,
            cashier_password: "1111111111".to_string(),
        }
    }
}

/// Current real-time clock values the dispatcher injects into CONF /
/// GETD responses.  Taken as a parameter rather than read here so the
/// dispatcher stays pure and tests can pin a fixed timestamp.
#[derive(Debug, Clone, Copy)]
pub struct Clock<'a> {
    pub date: &'a str, // ggggmmdd
    pub time: &'a str, // hhmmss
}

/// Session-level correlation bits used to build
/// [`CanonicalCommand::idempotency_key`].  M6 will populate
/// `session_uuid` per TCP connection; `receipt_seq` monotonically
/// increments per closed receipt so two replays of the same COMP
/// share the same idempotency key.
#[derive(Debug, Clone)]
pub struct Correlation {
    pub session_uuid: String,
    pub receipt_seq: u64,
}

/// Dispatch one command — mutates `session` and returns the frames
/// the driver should emit.
///
/// The last frame is **always** `Response::Ready` unless the arm
/// returned an error (in which case `Ready` is NOT emitted — the
/// real firmware only frees the channel after a successful command).
///
/// # Panics
/// Never on valid identity input.  Internal `expect` calls cover
/// payload sizes that are provably within the 4..=252-byte wire
/// bounds (CONF = 152 chars, COMP = 94 chars, GLCN = 38 chars).
#[allow(clippy::too_many_lines)]
pub fn dispatch(
    session: &mut Session,
    command: Command,
    identity: &Identity,
    clock: Clock<'_>,
    bridge: &dyn Bridge,
    correlation: &mut Correlation,
) -> Vec<Response> {
    // Fiscal commands need a logged-in cashier.  Only a handful of
    // handshake / identity commands are legal before `UPAS`.
    if !session.cashier_registered() && !is_preauth_command(&command) {
        return err(ErrorCode::SoftUpas);
    }

    match command {
        // ── Session / handshake ──────────────────────────────────
        Command::Csin(enabled) => {
            session.crc_enabled = enabled;
            session.mark_command_ok("CSIN");
            ok(None)
        }
        Command::Sync => {
            session.mark_command_ok("SYNC");
            ok(None)
        }
        Command::Upas { password, cashier_id } => {
            if password != identity.cashier_password {
                // Protocol §5.1 — a failed UPAS clears the cashier
                // registration flag, requiring the caller to retry
                // authentication.  We also drop back to Connected so
                // every subsequent fiscal command surfaces SOFTUPAS.
                session.cashier_id = None;
                session.state = SessionState::Connected;
                return err(ErrorCode::SoftUpas);
            }
            session.cashier_id = Some(cashier_id);
            if session.state == SessionState::Connected {
                session.state = SessionState::Authenticated;
            }
            // If already authenticated (or even in a receipt), we just
            // refresh the cashier id without kicking state backwards.
            session.mark_command_ok("UPAS");
            ok(None)
        }
        Command::Svsl { mode, .. } => {
            session.sys_key = match mode {
                '0' => SysKey::Off,
                '1' => SysKey::Work,
                '2' => SysKey::XReport,
                '4' => SysKey::ZReport,
                '8' => SysKey::Programming,
                _ => return err(ErrorCode::SoftKey),
            };
            session.mark_command_ok("SVSL");
            ok(None)
        }

        // ── Identity / state queries ─────────────────────────────
        Command::Conf => ok(Some(build_conf(session, identity, clock, ConfMode::Binary))),
        Command::ConfLower => ok(Some(build_conf(session, identity, clock, ConfMode::Ascii))),
        Command::Getd => {
            let payload = format!("GETD{}{}", clock.date, clock.time);
            let data = Response::data(payload).expect("GETD payload ≥ 4 chars");
            ok(Some(data))
        }
        Command::Glcn => {
            // Zero-number stub — replaced in M4/M5 with live counters.
            let payload = format!(
                "GLCN{:0>10}{:0>10}{:0>10}0000",
                0, 0, 0
            );
            let data = Response::data(payload).expect("GLCN payload ≥ 4 chars");
            ok(Some(data))
        }
        Command::Ccas => stubbed_query(session, "CCAS"),
        Command::Cfis => stubbed_query(session, "CFIS"),
        Command::Cnal => stubbed_query(session, "CNAL"),
        Command::Artd(_) => stubbed_query(session, "ARTD"),

        // ── Receipt lifecycle (structural only; M4 adds assembly) ─
        Command::Prep(dept) => {
            if session.receipt_open() {
                return err(ErrorCode::SoftCheck);
            }
            let return_check_number = session.pending_return_check_number.take();
            let direction = if return_check_number.is_some() {
                ReceiptDirection::Return
            } else {
                ReceiptDirection::Sale
            };
            let receipt = OpenReceipt {
                department: dept,
                return_check_number,
                direction,
                raw_frames: Vec::new(),
                dual_tax_mode: None,
                totals: crate::bridge::Totals::default(),
                psdt_sequence: 0,
            };
            session.state = SessionState::ReceiptOpen(receipt);
            session.psdt_sequence = 0;
            session.mark_command_ok("PREP");
            ok(None)
        }
        Command::Bchn(num) => {
            session.pending_return_check_number = Some(num);
            session.mark_command_ok("BCHN");
            ok(None)
        }
        Command::Cnac => {
            session.state = SessionState::Authenticated;
            session.psdt_sequence = 0;
            session.pending_return_check_number = None;
            session.mark_command_ok("CANC");
            ok(None)
        }
        Command::Ctxt => {
            // CTXT = clear free-text buffer only.  Does NOT close the
            // receipt or reset accumulated state — the previous draft
            // treated it like CANC which would silently discard
            // in-progress fiscal lines (real bug caught in M3 review).
            session.mark_command_ok("CTXT");
            ok(None)
        }
        // Receipt-building commands — accumulated into OpenReceipt.raw_frames
        // so the Python adapter (M7) can do the rich parse at COMP time.
        Command::Fisc(ref body) | Command::Bfis(ref body) | Command::Arfi(ref body)
        | Command::Arbf(ref body) | Command::Ficd(ref body) | Command::Bfcd(ref body)
        | Command::Finf(ref body) | Command::Tgcd(ref body) | Command::Grbg(ref body)
        | Command::Acld(ref body) | Command::Psdt(ref body) | Command::Cshg(ref body)
        | Command::Cvar(ref body) => {
            if !session.receipt_open() {
                return err(ErrorCode::SoftNoDoc);
            }
            let opcode = command_opcode(&command);
            append_receipt_frame(session, &opcode, body.clone());
            if matches!(command, Command::Bfis(_) | Command::Arbf(_) | Command::Bfcd(_)) {
                if let SessionState::ReceiptOpen(ref mut r) = session.state {
                    r.direction = ReceiptDirection::Return;
                }
            }
            if matches!(command, Command::Psdt(_)) {
                session.psdt_sequence = session.psdt_sequence.saturating_add(1);
                if let SessionState::ReceiptOpen(ref mut r) = session.state {
                    r.psdt_sequence = session.psdt_sequence;
                }
            }
            session.mark_command_ok(&opcode);
            ok(None)
        }
        Command::Gren { ref discount_name, ref upcount_name } => {
            if !session.receipt_open() {
                return err(ErrorCode::SoftNoDoc);
            }
            let body = format!(
                "{}{}",
                discount_name.as_deref().unwrap_or(""),
                upcount_name.as_deref().unwrap_or(""),
            );
            append_receipt_frame(session, "GREN", body);
            session.mark_command_ok("GREN");
            ok(None)
        }
        Command::Nlpr { tax1_char, tax2_char } => {
            if !session.receipt_open() {
                return err(ErrorCode::SoftNoDoc);
            }
            let tax1 = cyrillic_tax_to_group(tax1_char);
            let tax2 = cyrillic_tax_to_group(tax2_char);
            if let SessionState::ReceiptOpen(ref mut r) = session.state {
                r.dual_tax_mode = Some(crate::bridge::DualTaxMode {
                    tax_group_1: tax1,
                    tax_group_2: tax2,
                });
            }
            let body = format!("{tax1_char}{tax2_char}");
            append_receipt_frame(session, "NLPR", body);
            session.mark_command_ok("NLPR");
            ok(None)
        }
        Command::Comp(ref body) => {
            if !session.receipt_open() {
                return err(ErrorCode::SoftNoDoc);
            }
            // Build the canonical envelope from accumulated receipt
            // state and submit to the Python bridge.  A bridge error
            // is mapped to the corresponding `SOFT*` wire code and
            // the receipt stays open so the caller can retry or CANC.
            append_receipt_frame(session, "COMP", body.clone());
            let envelope = build_canonical(session, identity, correlation);
            match bridge.submit(&envelope) {
                Ok(resp) => match classify_response(&resp) {
                    DocumentOutcome::Accepted { fiscal_id, sale, ret } => {
                        let comp_payload = CompBuilder::new(fiscal_id, sale, ret).to_wire_payload();
                        session.state = SessionState::Authenticated;
                        session.psdt_sequence = 0;
                        session.pending_return_check_number = None;
                        correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
                        session.mark_command_ok("COMP");
                        ok(Some(
                            Response::data(comp_payload).expect("COMP payload is 94 chars"),
                        ))
                    }
                    DocumentOutcome::Terminal(code) => {
                        // DPS rejected permanently — close receipt, cannot retry this doc.
                        session.state = SessionState::Authenticated;
                        session.psdt_sequence = 0;
                        session.pending_return_check_number = None;
                        correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
                        session.mark_command_ok("COMP_REJECTED");
                        err(code)
                    }
                    DocumentOutcome::Retryable(code) => {
                        // DPS may not have seen this — leave receipt open for retry or CANC.
                        err(code)
                    }
                },
                Err(bridge_err) => {
                    // Transport failure — leave receipt open so caller can retry or CANC.
                    let code = map_bridge_error(&bridge_err);
                    err(code)
                }
            }
        }

        // ── Fiscal reports (M5 — submit via bridge) ──────────────
        Command::Zrep => submit_report(
            session, identity, bridge, correlation,
            CommandType::XReport, "ZREP", String::new(),
        ),
        Command::Nrep => {
            if session.receipt_open() {
                return err(ErrorCode::SoftCheck);
            }
            submit_report(
                session, identity, bridge, correlation,
                CommandType::ZReport, "NREP", String::new(),
            )
        }
        Command::NrepLowercase => submit_report(
            session, identity, bridge, correlation,
            CommandType::ShiftOpen, "nrep", String::new(),
        ),
        Command::Firn { first, last } => {
            if session.receipt_open() {
                return err(ErrorCode::SoftCheck);
            }
            submit_report(
                session, identity, bridge, correlation,
                CommandType::PeriodicReport, "FIRN",
                format!("{first:04}{last:04}"),
            )
        }
        Command::Iren { first, last } => {
            if session.receipt_open() {
                return err(ErrorCode::SoftCheck);
            }
            submit_report(
                session, identity, bridge, correlation,
                CommandType::PeriodicReport, "IREN",
                format!("{first:04}{last:04}"),
            )
        }
        Command::Firp { ref from, ref to } => {
            if session.receipt_open() {
                return err(ErrorCode::SoftCheck);
            }
            submit_report(
                session, identity, bridge, correlation,
                CommandType::PeriodicReport, "FIRP",
                format!("{from}{to}"),
            )
        }
        Command::Irep { ref from, ref to } => {
            if session.receipt_open() {
                return err(ErrorCode::SoftCheck);
            }
            submit_report(
                session, identity, bridge, correlation,
                CommandType::PeriodicReport, "IREP",
                format!("{from}{to}"),
            )
        }
        // Local-only reports — no bridge call, no state change.
        Command::Artz => done("ARTZ", session),
        Command::Dizv => done("DIZV", session),
        Command::Null => done("NULL", session),
        Command::Kass => done("KASS", session),
        Command::Dbeg => done("DBEG", session),
        Command::Prtx => done("PRTX", session),

        // ── Cash deposit (CAIOI) / service withdrawal (CAIOO) ──────
        // Both are standalone fiscal operations — must not fire while
        // a receipt is open.  The wire body `<D10 sum><description>` is
        // preserved verbatim into raw_frames so the Python adapter can
        // parse the sum (see adapters/maria304_native.py::
        // _enrich_service_payload).
        Command::Caioi { sum_kopecks, description } => {
            if session.receipt_open() {
                return err(ErrorCode::SoftCheck);
            }
            let body = format!("{sum_kopecks:010}{description}");
            submit_report(
                session, identity, bridge, correlation,
                CommandType::ServiceIn, "CAIOI", body,
            )
        }
        Command::Caioo { sum_kopecks, description } => {
            if session.receipt_open() {
                return err(ErrorCode::SoftCheck);
            }
            let body = format!("{sum_kopecks:010}{description}");
            submit_report(
                session, identity, bridge, correlation,
                CommandType::ServiceOut, "CAIOO", body,
            )
        }

        // ── Setup / configuration + display / misc passthrough ──
        // All of these simply acknowledge — the opcode is reflected
        // into `last_command_id` so CONF surfaces it, but no other
        // state mutation happens here.
        Command::Armo(_) | Command::Dept(_) | Command::Head(_) | Command::Botm(_)
        | Command::BotmIdx(_) | Command::Npdi(_) | Command::Zdnm(_) | Command::Ctim(_)
        | Command::Szkr(_) | Command::Pzkr(_) | Command::Stfl | Command::Cutr(_)
        | Command::Nalg(_) | Command::Nnam(_) | Command::Blfi(_) | Command::Ncdc(_)
        | Command::Dstr(_)
        | Command::Disp(_) | Command::DispLower(_) | Command::DispX(_)
        | Command::Mdmd(_) | Command::Tses => done(&command_opcode(&command), session),

        // ── Defensive defaults ───────────────────────────────────
        Command::Unknown { ref opcode, .. } => {
            // Mirror the unknown opcode verbatim in last_command_id
            // (char-truncated to 4) so `CONF` surfaces accurate
            // diagnostic data to the caller.
            let mark: String = opcode.chars().take(4).collect();
            done(&mark, session)
        }
        Command::InvalidParams { opcode, .. } => {
            // Typed malformed-params — respond with a softer block
            // than a generic DONE so the caller can retry properly.
            // Char-based truncation (not byte-based) so a Cyrillic
            // opcode cannot split a UTF-8 boundary.
            let mark: String = opcode.chars().take(4).collect();
            session.mark_command_ok(&mark);
            err(ErrorCode::SoftCheck)
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────

fn is_preauth_command(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Csin(_)
            | Command::Sync
            | Command::Upas { .. }
            | Command::Conf
            | Command::ConfLower
            | Command::Getd
            | Command::Glcn
    )
}

fn command_opcode(cmd: &Command) -> String {
    match cmd {
        Command::Csin(_) => "CSIN",
        Command::Sync => "SYNC",
        Command::Upas { .. } => "UPAS",
        Command::Svsl { .. } => "SVSL",
        Command::Conf => "CONF",
        Command::ConfLower => "CONf",
        Command::Getd => "GETD",
        Command::Glcn => "GLCN",
        Command::Ccas => "CCAS",
        Command::Cfis => "CFIS",
        Command::Cnal => "CNAL",
        Command::Artd(_) => "ARTD",
        Command::Prep(_) => "PREP",
        Command::Bchn(_) => "BCHN",
        Command::Cvar(_) => "CVAL",
        Command::Grbg(_) => "GRBG",
        Command::Gren { .. } => "GREN",
        Command::Comp(_) => "COMP",
        Command::Cnac => "CANC",
        Command::Ctxt => "CTXT",
        Command::Finf(_) => "FINF",
        Command::Tgcd(_) => "TGCD",
        Command::Fisc(_) => "FISC",
        Command::Bfis(_) => "BFIS",
        Command::Arfi(_) => "ARFI",
        Command::Arbf(_) => "ARBF",
        Command::Ficd(_) => "FICD",
        Command::Bfcd(_) => "BFCD",
        Command::Nlpr { .. } => "NLPR",
        Command::Acld(_) => "ACLD",
        Command::Psdt(_) => "PSDt",
        Command::Cshg(_) => "CSHG",
        Command::Caioi { .. } | Command::Caioo { .. } => "CAIO",
        Command::Zrep => "ZREP",
        Command::Nrep => "NREP",
        Command::NrepLowercase => "nrep",
        Command::Firn { .. } => "FIRN",
        Command::Firp { .. } => "FIRP",
        Command::Iren { .. } => "IREN",
        Command::Irep { .. } => "IREP",
        Command::Artz => "ARTZ",
        Command::Dizv => "DIZV",
        Command::Null => "NULL",
        Command::Kass => "KASS",
        Command::Dbeg => "DBEG",
        Command::Prtx => "PRTX",
        Command::Armo(_) => "ARMO",
        Command::Dept(_) => "DEPT",
        Command::Head(_) => "HEAD",
        Command::Botm(_) => "BOTM",
        Command::BotmIdx(_) => "BOTm",
        Command::Npdi(_) => "NPDI",
        Command::Zdnm(_) => "ZDNM",
        Command::Ctim(_) => "CTIM",
        Command::Szkr(_) => "SZKR",
        Command::Pzkr(_) => "PZKR",
        Command::Stfl => "STFL",
        Command::Cutr(_) => "CUTR",
        Command::Nalg(_) => "NALG",
        Command::Nnam(_) => "NNAM",
        Command::Blfi(_) => "BLFI",
        Command::Ncdc(_) => "NCDC",
        Command::Dstr(_) => "DSTR",
        Command::Disp(_) => "DISP",
        Command::DispLower(_) => "DISp",
        Command::DispX(_) => "DIsp",
        Command::Mdmd(_) => "MDMD",
        Command::Tses => "TSES",
        Command::Unknown { opcode, .. } | Command::InvalidParams { opcode, .. } => {
            return opcode.clone();
        }
    }
    .to_string()
}

fn append_receipt_frame(session: &mut Session, opcode: &str, body: String) {
    if let SessionState::ReceiptOpen(ref mut r) = session.state {
        r.raw_frames.push(RawFrame {
            opcode: opcode.to_string(),
            body,
        });
    }
}

fn cyrillic_tax_to_group(ch: char) -> u8 {
    // "АБВГДЕЖЗ" — protocol's dual-tax group naming.
    const GROUPS: &str = "АБВГДЕЖЗ";
    for (i, g) in GROUPS.chars().enumerate() {
        if g == ch {
            return u8::try_from(i + 1).unwrap_or(0);
        }
    }
    0
}

fn map_bridge_error(err: &BridgeError) -> ErrorCode {
    match err {
        // Only accept SOFT-prefixed codes from the bridge — anything
        // else is a protocol violation by the Python adapter and we
        // fall back to SOFTBLOCK so the caller sees a retryable signal.
        BridgeError::Rejected { code, .. } if code.starts_with("SOFT") => {
            match ErrorCode::parse(code) {
                Some(ErrorCode::Custom(_)) | None => ErrorCode::SoftBlock,
                Some(known) => known,
            }
        }
        _ => ErrorCode::SoftBlock,
    }
}

fn build_canonical(
    session: &Session,
    identity: &Identity,
    correlation: &Correlation,
) -> CanonicalCommand {
    let (department, return_check_number, direction, raw_frames, dual_tax_mode, totals) =
        match session.state {
            SessionState::ReceiptOpen(ref r) => (
                Some(r.department.clone()),
                r.return_check_number.clone(),
                r.direction,
                r.raw_frames.clone(),
                r.dual_tax_mode,
                r.totals,
            ),
            _ => unreachable!("build_canonical called outside ReceiptOpen state"),
        };
    // A receipt that carries a CSHG frame is a cashback/ATM-style
    // withdrawal (per Maria 304 protocol §54 — always its own receipt).
    // Override Sell/Return with CashWithdrawal so the Python adapter
    // selects the correct canonical operation path.
    let has_cshg = raw_frames.iter().any(|f| f.opcode == "CSHG");
    let payload = ReceiptPayload {
        direction,
        goods: Vec::new(),
        payments: Vec::new(),
        dual_tax_mode,
        totals,
        raw_frames,
    };
    let command_type = if has_cshg {
        CommandType::CashWithdrawal
    } else {
        match direction {
            crate::bridge::ReceiptDirection::Sale => CommandType::Sell,
            crate::bridge::ReceiptDirection::Return => CommandType::Return,
        }
    };
    CanonicalCommand {
        schema_version: "1.0".to_string(),
        fiscal_number: identity.fiscal_number.clone(),
        command_type,
        idempotency_key: format!(
            "maria304:{}:{}:{}",
            identity.fiscal_number, correlation.session_uuid, correlation.receipt_seq,
        ),
        cashier_id: session.cashier_id.clone(),
        department,
        return_check_number,
        payload,
    }
}

/// Submit a report-style command to the bridge and compose the
/// response frame sequence.  The canonical envelope carries a single
/// `raw_frames` entry with the opcode + body so the Python adapter
/// can extract date ranges / Z-numbers / etc.
///
/// On success: `DONE + READY`.  On bridge error: the mapped `SOFT*`
/// frame (and no subsequent frames).
fn submit_report(
    session: &mut Session,
    identity: &Identity,
    bridge: &dyn Bridge,
    correlation: &mut Correlation,
    command_type: CommandType,
    opcode: &str,
    body: String,
) -> Vec<Response> {
    let envelope = CanonicalCommand {
        schema_version: "1.0".to_string(),
        fiscal_number: identity.fiscal_number.clone(),
        command_type,
        idempotency_key: format!(
            "maria304:{}:{}:{}:{}",
            identity.fiscal_number, correlation.session_uuid, correlation.receipt_seq, opcode,
        ),
        cashier_id: session.cashier_id.clone(),
        department: None,
        return_check_number: None,
        payload: ReceiptPayload {
            raw_frames: vec![RawFrame { opcode: opcode.to_string(), body }],
            ..Default::default()
        },
    };
    match bridge.submit(&envelope) {
        Ok(resp) if resp.ok => {
            session.mark_command_ok(opcode);
            // Every report increments the correlation sequence so
            // replays use a fresh idempotency_key.  Z-report
            // additionally signals "new fiscal day" — M7 will track
            // this via the response but for now just bump the seq.
            correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
            ok(None)
        }
        Ok(resp) => {
            // Bridge returned ok=false — surface as a soft error.
            // Use document_state as the code so SOFT-prefixed states
            // (e.g. "SOFTBADART") map to the right ErrorCode; anything
            // else falls back to SoftBlock via map_bridge_error.
            err(map_bridge_error(&BridgeError::Rejected {
                code: resp.document_state.clone(),
                message: format!("submit_report ok=false: {}", resp.document_state),
            }))
        }
        Err(bridge_err) => err(map_bridge_error(&bridge_err)),
    }
}

fn build_conf(session: &Session, id: &Identity, clock: Clock<'_>, mode: ConfMode) -> Response {
    let cashier = session.cashier_id.as_deref().unwrap_or("");
    let builder = ConfBuilder {
        factory_serial: &id.factory_serial,
        fiscal_number: &id.fiscal_number,
        merchant: &id.merchant,
        now: (clock.date, clock.time),
        sys_key: session.sys_key,
        expected_cmd: session.expected_cmd(),
        cashier_registered: session.cashier_registered(),
        cashier_id: cashier,
        z_report_done: false,
        z_report_number: 0,
        last_receipt_number: 0,
        last_command_id: &session.last_command_id,
        fw_version_id: &id.fw_version_id,
        fw_build_date: &id.fw_build_date,
        receipt_info_line: "",
        currency_date: &id.currency_date,
        decimals: id.decimals,
        currency: &id.currency,
    };
    let payload = builder.to_wire_payload(mode);
    Response::data(payload).expect("CONF payload is 152 chars, within 4..=252")
}

fn stubbed_query(session: &mut Session, opcode: &str) -> Vec<Response> {
    // A query that we cannot answer meaningfully yet.  Echo the
    // opcode + spaces so the OLE DLL sees a well-formed frame but
    // no useful data — close enough for the handshake smoke tests.
    session.mark_command_ok(opcode);
    let mut payload = opcode.to_string();
    for _ in payload.chars().count()..16 {
        payload.push(' ');
    }
    let data = Response::data(payload).expect("stub payload ≥ 4 chars");
    ok(Some(data))
}

fn done(opcode: &str, session: &mut Session) -> Vec<Response> {
    session.mark_command_ok(opcode);
    ok(None)
}

fn ok(data: Option<Response>) -> Vec<Response> {
    match data {
        Some(d) => vec![d, Response::Done, Response::Ready],
        None => vec![Response::Done, Response::Ready],
    }
}

fn err(code: ErrorCode) -> Vec<Response> {
    vec![Response::Error(code)]
}

// ── Two-stage dispatch for spawn_blocking integration ─────────────────────────

/// Result of [`dispatch_prepare`]: either fully computed responses, or a
/// canonical envelope that must be submitted to the bridge.
#[derive(Debug)]
pub enum DispatchPrepared {
    /// All processing complete — send these responses.
    Done(Vec<Response>),
    /// Bridge I/O required.  Call `bridge.submit(&envelope)` (ideally via
    /// `tokio::task::spawn_blocking`), then pass the result to
    /// [`dispatch_with_result`].
    NeedsBridge(CanonicalCommand),
}

/// First stage of two-stage dispatch.
///
/// For COMP and fiscal-report commands, builds the canonical envelope without
/// performing any I/O and returns [`DispatchPrepared::NeedsBridge`].  All other
/// commands are dispatched immediately and the responses returned as
/// [`DispatchPrepared::Done`].
pub fn dispatch_prepare(
    session: &mut Session,
    command: &Command,
    identity: &Identity,
    clock: Clock<'_>,
    correlation: &mut Correlation,
) -> DispatchPrepared {
    if !session.cashier_registered() && !is_preauth_command(command) {
        return DispatchPrepared::Done(err(ErrorCode::SoftUpas));
    }
    match command {
        Command::Comp(ref body) => {
            if !session.receipt_open() {
                return DispatchPrepared::Done(err(ErrorCode::SoftNoDoc));
            }
            append_receipt_frame(session, "COMP", body.clone());
            DispatchPrepared::NeedsBridge(build_canonical(session, identity, correlation))
        }
        Command::Zrep => DispatchPrepared::NeedsBridge(
            build_report_envelope(session, identity, correlation, CommandType::XReport, "ZREP", String::new()),
        ),
        Command::Nrep => {
            if session.receipt_open() {
                return DispatchPrepared::Done(err(ErrorCode::SoftCheck));
            }
            DispatchPrepared::NeedsBridge(
                build_report_envelope(session, identity, correlation, CommandType::ZReport, "NREP", String::new()),
            )
        }
        Command::NrepLowercase => DispatchPrepared::NeedsBridge(
            build_report_envelope(session, identity, correlation, CommandType::ShiftOpen, "nrep", String::new()),
        ),
        Command::Firn { first, last } => {
            if session.receipt_open() {
                return DispatchPrepared::Done(err(ErrorCode::SoftCheck));
            }
            let (f, l) = (*first, *last);
            DispatchPrepared::NeedsBridge(
                build_report_envelope(session, identity, correlation, CommandType::PeriodicReport, "FIRN", format!("{f:04}{l:04}")),
            )
        }
        Command::Iren { first, last } => {
            if session.receipt_open() {
                return DispatchPrepared::Done(err(ErrorCode::SoftCheck));
            }
            let (f, l) = (*first, *last);
            DispatchPrepared::NeedsBridge(
                build_report_envelope(session, identity, correlation, CommandType::PeriodicReport, "IREN", format!("{f:04}{l:04}")),
            )
        }
        Command::Firp { ref from, ref to } => {
            if session.receipt_open() {
                return DispatchPrepared::Done(err(ErrorCode::SoftCheck));
            }
            let (from, to) = (from.clone(), to.clone());
            DispatchPrepared::NeedsBridge(
                build_report_envelope(session, identity, correlation, CommandType::PeriodicReport, "FIRP", format!("{from}{to}")),
            )
        }
        Command::Irep { ref from, ref to } => {
            if session.receipt_open() {
                return DispatchPrepared::Done(err(ErrorCode::SoftCheck));
            }
            let (from, to) = (from.clone(), to.clone());
            DispatchPrepared::NeedsBridge(
                build_report_envelope(session, identity, correlation, CommandType::PeriodicReport, "IREP", format!("{from}{to}")),
            )
        }
        Command::Caioi { sum_kopecks, description } => {
            if session.receipt_open() {
                return DispatchPrepared::Done(err(ErrorCode::SoftCheck));
            }
            let body = format!("{sum_kopecks:010}{description}");
            DispatchPrepared::NeedsBridge(
                build_report_envelope(session, identity, correlation, CommandType::ServiceIn, "CAIOI", body),
            )
        }
        Command::Caioo { sum_kopecks, description } => {
            if session.receipt_open() {
                return DispatchPrepared::Done(err(ErrorCode::SoftCheck));
            }
            let body = format!("{sum_kopecks:010}{description}");
            DispatchPrepared::NeedsBridge(
                build_report_envelope(session, identity, correlation, CommandType::ServiceOut, "CAIOO", body),
            )
        }
        _ => {
            struct NeverBridge;
            impl Bridge for NeverBridge {
                fn submit(&self, _: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
                    unreachable!("NeverBridge::submit — bridge command slipped into dispatch_prepare non-bridge arm")
                }
            }
            DispatchPrepared::Done(dispatch(session, command.clone(), identity, clock, &NeverBridge, correlation))
        }
    }
}

/// Second stage of two-stage dispatch — produce final responses after the
/// bridge call.
///
/// Must only be called when [`dispatch_prepare`] returned
/// [`DispatchPrepared::NeedsBridge`] for the same `command`.
pub fn dispatch_with_result(
    session: &mut Session,
    command: &Command,
    bridge_result: Result<CanonicalResponse, BridgeError>,
    correlation: &mut Correlation,
) -> Vec<Response> {
    match command {
        Command::Comp(_) => match bridge_result {
            Ok(ref resp) => match classify_response(resp) {
                DocumentOutcome::Accepted { fiscal_id, sale, ret } => {
                    let comp_payload = CompBuilder::new(fiscal_id, sale, ret).to_wire_payload();
                    session.state = SessionState::Authenticated;
                    session.psdt_sequence = 0;
                    session.pending_return_check_number = None;
                    correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
                    session.mark_command_ok("COMP");
                    ok(Some(Response::data(comp_payload).expect("COMP payload is 94 chars")))
                }
                DocumentOutcome::Terminal(code) => {
                    session.state = SessionState::Authenticated;
                    session.psdt_sequence = 0;
                    session.pending_return_check_number = None;
                    correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
                    session.mark_command_ok("COMP_REJECTED");
                    err(code)
                }
                DocumentOutcome::Retryable(code) => err(code),
            },
            Err(e) => err(map_bridge_error(&e)),
        },
        cmd => {
            let opcode = report_opcode(cmd)
                .expect("dispatch_with_result called for non-bridge command");
            match bridge_result {
                Ok(resp) if resp.ok => {
                    session.mark_command_ok(opcode);
                    correlation.receipt_seq = correlation.receipt_seq.saturating_add(1);
                    ok(None)
                }
                Ok(resp) => err(map_bridge_error(&BridgeError::Rejected {
                    code: resp.document_state.clone(),
                    message: format!("submit_report ok=false: {}", resp.document_state),
                })),
                Err(e) => err(map_bridge_error(&e)),
            }
        }
    }
}

/// Build a report-type canonical envelope without calling the bridge.
fn build_report_envelope(
    session: &Session,
    identity: &Identity,
    correlation: &Correlation,
    command_type: CommandType,
    opcode: &str,
    body: String,
) -> CanonicalCommand {
    CanonicalCommand {
        schema_version: "1.0".to_string(),
        fiscal_number: identity.fiscal_number.clone(),
        command_type,
        idempotency_key: format!(
            "maria304:{}:{}:{}:{}",
            identity.fiscal_number, correlation.session_uuid, correlation.receipt_seq, opcode,
        ),
        cashier_id: session.cashier_id.clone(),
        department: None,
        return_check_number: None,
        payload: ReceiptPayload {
            raw_frames: vec![RawFrame { opcode: opcode.to_string(), body }],
            ..Default::default()
        },
    }
}

fn report_opcode(command: &Command) -> Option<&'static str> {
    match command {
        Command::Zrep => Some("ZREP"),
        Command::Nrep => Some("NREP"),
        Command::NrepLowercase => Some("nrep"),
        Command::Firn { .. } => Some("FIRN"),
        Command::Iren { .. } => Some("IREN"),
        Command::Firp { .. } => Some("FIRP"),
        Command::Irep { .. } => Some("IREP"),
        Command::Caioi { .. } => Some("CAIOI"),
        Command::Caioo { .. } => Some("CAIOO"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock() -> crate::bridge::MockBridge { crate::bridge::MockBridge::new() }
    fn corr() -> Correlation { Correlation { session_uuid: "sess".to_string(), receipt_seq: 0 } }

        fn clock() -> Clock<'static> {
        Clock { date: "20260420", time: "101530" }
    }

    fn logged_in() -> Session {
        let mut s = Session::new();
        s.state = SessionState::Authenticated;
        s.cashier_id = Some("c1  ".to_string());
        s
    }

    #[test]
    fn preauth_upas_succeeds_with_default_password() {
        let mut s = Session::new();
        let cmd = Command::Upas {
            password: "1111111111".to_string(),
            cashier_id: "Casher1".to_string(),
        };
        let out = dispatch(&mut s, cmd, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Done, Response::Ready]);
        assert_eq!(s.state, SessionState::Authenticated);
        assert_eq!(s.cashier_id.as_deref(), Some("Casher1"));
    }

    #[test]
    fn upas_with_wrong_password_emits_softupas() {
        let mut s = Session::new();
        let cmd = Command::Upas {
            password: "0000000000".to_string(),
            cashier_id: "x".to_string(),
        };
        let out = dispatch(&mut s, cmd, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftUpas)]);
        assert_eq!(s.state, SessionState::Connected);
        assert!(s.cashier_id.is_none());
    }

    #[test]
    fn fiscal_command_before_login_emits_softupas() {
        let mut s = Session::new();
        let out = dispatch(&mut s, Command::Prep("1".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftUpas)]);
        assert!(!s.receipt_open());
    }

    #[test]
    fn csin_and_sync_work_before_login() {
        let mut s = Session::new();
        let out1 = dispatch(&mut s, Command::Csin(true), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out1, vec![Response::Done, Response::Ready]);
        assert!(s.crc_enabled);

        let out2 = dispatch(&mut s, Command::Sync, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out2, vec![Response::Done, Response::Ready]);
    }

    #[test]
    fn conf_works_before_login_and_returns_a_data_frame() {
        let mut s = Session::new();
        let out = dispatch(&mut s, Command::ConfLower, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out.len(), 3);
        match &out[0] {
            Response::Data(payload) => {
                assert!(payload.starts_with("CONf"));
                // CONF body = 148 chars, opcode = 4, so total = 152 chars.
                assert_eq!(payload.chars().count(), 4 + 148);
            }
            other => panic!("expected Data, got {other:?}"),
        }
        assert_eq!(out[1], Response::Done);
        assert_eq!(out[2], Response::Ready);
    }

    #[test]
    fn prep_opens_receipt_and_clears_pending_return_number() {
        let mut s = logged_in();
        s.pending_return_check_number = Some("42".to_string());
        let out = dispatch(&mut s, Command::Prep("Bar".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Done, Response::Ready]);
        assert!(s.receipt_open());
        assert!(s.pending_return_check_number.is_none());
        match &s.state {
            SessionState::ReceiptOpen(r) => {
                assert_eq!(r.department, "Bar");
                assert_eq!(r.return_check_number.as_deref(), Some("42"));
                assert_eq!(r.direction, crate::bridge::ReceiptDirection::Return);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn prep_while_receipt_open_emits_softcheck() {
        let mut s = logged_in();
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());
        let out = dispatch(&mut s, Command::Prep("x".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftCheck)]);
    }

    #[test]
    fn canc_closes_receipt_without_emitting_comp() {
        let mut s = logged_in();
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());
        let out = dispatch(&mut s, Command::Cnac, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Done, Response::Ready]);
        assert!(!s.receipt_open());
    }

    #[test]
    fn comp_without_open_receipt_emits_softnodoc() {
        let mut s = logged_in();
        let out = dispatch(&mut s, Command::Comp(String::new()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftNoDoc)]);
    }

    #[test]
    fn comp_emits_data_frame_then_done_ready_and_closes_receipt() {
        let mut s = logged_in();
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());
        let out = dispatch(&mut s, Command::Comp(String::new()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out.len(), 3);
        match &out[0] {
            Response::Data(p) => assert!(p.starts_with("COMP")),
            other => panic!("{other:?}"),
        }
        assert_eq!(out[1], Response::Done);
        assert_eq!(out[2], Response::Ready);
        assert!(!s.receipt_open());
    }

    #[test]
    fn fiscal_line_without_receipt_emits_softnodoc() {
        let mut s = logged_in();
        let out = dispatch(&mut s, Command::Fisc("x".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftNoDoc)]);
    }

    #[test]
    fn psdt_counter_increments_per_receipt_and_resets_on_comp() {
        let mut s = logged_in();
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());
        for expected in 1..=3 {
            dispatch(&mut s, Command::Psdt("x".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
            assert_eq!(s.psdt_sequence, expected);
        }
        dispatch(&mut s, Command::Comp(String::new()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.psdt_sequence, 0);
    }

    #[test]
    fn unknown_opcode_is_accepted_with_done() {
        let mut s = logged_in();
        let cmd = Command::Unknown {
            opcode: "ZZZZ".to_string(),
            body: String::new(),
        };
        let out = dispatch(&mut s, cmd, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Done, Response::Ready]);
    }

    #[test]
    fn invalid_params_emits_softcheck_not_done() {
        let mut s = logged_in();
        let cmd = Command::InvalidParams {
            opcode: "UPAS".to_string(),
            body: "short".to_string(),
        };
        let out = dispatch(&mut s, cmd, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftCheck)]);
    }

    #[test]
    fn svsl_updates_sys_key_position() {
        let mut s = logged_in();
        dispatch(&mut s, Command::Svsl { mode: '2', password: None }, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.sys_key, SysKey::XReport);
    }

    #[test]
    fn svsl_with_unknown_mode_emits_softkey() {
        let mut s = logged_in();
        let out = dispatch(&mut s, Command::Svsl { mode: 'X', password: None }, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftKey)]);
    }

    // ── Post-self-review additions (proof-level tests) ────────────

    #[test]
    fn svsl_before_login_is_rejected_with_softupas() {
        // SVSL is not a handshake command — requires logged-in cashier.
        let mut s = Session::new();
        let out = dispatch(&mut s, Command::Svsl { mode: '2', password: None }, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftUpas)]);
        // State unchanged.
        assert_eq!(s.sys_key, SysKey::Work);
    }

    #[test]
    fn getd_works_before_login_and_carries_clock_values() {
        let mut s = Session::new();
        let out = dispatch(
            &mut s,
            Command::Getd,
            &Identity::default(),
            Clock { date: "20261231", time: "235959" },
            &mock(),
            &mut corr(),
        );
        match &out[0] {
            Response::Data(p) => {
                assert!(p.starts_with("GETD"));
                assert!(p.contains("20261231"));
                assert!(p.contains("235959"));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(out[1], Response::Done);
        assert_eq!(out[2], Response::Ready);
    }

    #[test]
    fn csin_toggle_persists_across_later_commands() {
        let mut s = Session::new();
        dispatch(&mut s, Command::Csin(true), &Identity::default(), clock(), &mock(), &mut corr());
        assert!(s.crc_enabled);

        // Further commands should not touch the CRC flag.
        dispatch(&mut s, Command::Sync, &Identity::default(), clock(), &mock(), &mut corr());
        assert!(s.crc_enabled);

        let login = Command::Upas {
            password: "1111111111".to_string(),
            cashier_id: "x".to_string(),
        };
        dispatch(&mut s, login, &Identity::default(), clock(), &mock(), &mut corr());
        assert!(s.crc_enabled);

        // CSIN0 clears it.
        dispatch(&mut s, Command::Csin(false), &Identity::default(), clock(), &mock(), &mut corr());
        assert!(!s.crc_enabled);
    }

    #[test]
    fn two_receipts_in_sequence_both_close_cleanly() {
        let mut s = logged_in();
        for i in 0..2 {
            let out = dispatch(&mut s, Command::Prep(format!("Dept{i}")), &Identity::default(), clock(), &mock(), &mut corr());
            assert_eq!(out, vec![Response::Done, Response::Ready]);
            assert!(s.receipt_open());

            let out = dispatch(&mut s, Command::Comp(String::new()), &Identity::default(), clock(), &mock(), &mut corr());
            assert_eq!(out.len(), 3);
            assert!(!s.receipt_open());
        }
    }

    #[test]
    fn setup_commands_stub_to_done_without_touching_state() {
        let mut s = logged_in();
        let before = s.clone();
        let out = dispatch(&mut s, Command::Head("TOV Приклад".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Done, Response::Ready]);
        // Only last_command_id should have changed.
        assert_eq!(s.state, before.state);
        assert_eq!(s.cashier_id, before.cashier_id);
        assert_eq!(s.crc_enabled, before.crc_enabled);
        assert_eq!(s.last_command_id, "HEAD");
    }

    #[test]
    fn unknown_opcode_still_marks_last_command_id() {
        let mut s = logged_in();
        dispatch(&mut s, Command::Unknown { opcode: "ZZZZ".to_string(), body: String::new() }, &Identity::default(), clock(), &mock(), &mut corr());
        // Unknown opcode is reflected verbatim in last_command_id —
        // CONF then surfaces it for caller diagnostics.
        assert_eq!(s.last_command_id, "ZZZZ");
    }

    #[test]
    fn every_successful_dispatch_ends_with_ready() {
        let mut s = logged_in();
        let cmds = [
            Command::Csin(true),
            Command::Sync,
            Command::Svsl { mode: '1', password: None },
            Command::Conf,
            Command::ConfLower,
            Command::Getd,
            Command::Glcn,
            Command::Ccas,
            Command::Head("h".to_string()),
            Command::Null,
            Command::Artz,
        ];
        for cmd in cmds {
            let out = dispatch(&mut s, cmd.clone(), &Identity::default(), clock(), &mock(), &mut corr());
            assert_eq!(
                out.last(),
                Some(&Response::Ready),
                "success should always finish with Ready for {cmd:?}",
            );
        }
    }

    #[test]
    fn error_dispatch_never_ends_with_ready() {
        // Invariant: firmware only frees the channel after success.
        // Failure paths stop at the SOFT frame.
        let mut s = Session::new();
        let out = dispatch(&mut s, Command::Fisc("x".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert!(!matches!(out.last(), Some(Response::Ready)));
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftUpas)]);
    }

    #[test]
    fn fisc_accumulates_state_only_while_receipt_open() {
        let mut s = logged_in();
        // Without receipt — rejected.
        let out = dispatch(&mut s, Command::Fisc("x".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftNoDoc)]);

        // Open receipt — now fisc accepts.
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());
        let out = dispatch(&mut s, Command::Fisc("y".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Done, Response::Ready]);
        assert_eq!(s.last_command_id, "FISC");
    }

    #[test]
    fn psdt_only_increments_sequence_on_psdt_not_other_receipt_cmds() {
        // Regression for a subtle bug: the PSDt counter must not
        // increment for e.g. FISC.
        let mut s = logged_in();
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());

        dispatch(&mut s, Command::Fisc("x".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.psdt_sequence, 0);

        dispatch(&mut s, Command::Psdt("x".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.psdt_sequence, 1);

        dispatch(&mut s, Command::Acld("x".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.psdt_sequence, 1);
    }

    #[test]
    fn pending_return_check_is_consumed_by_prep_exactly_once() {
        let mut s = logged_in();
        dispatch(&mut s, Command::Bchn("999".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.pending_return_check_number.as_deref(), Some("999"));

        dispatch(&mut s, Command::Prep("dep".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert!(s.pending_return_check_number.is_none());
        match &s.state {
            SessionState::ReceiptOpen(r) => {
                assert_eq!(r.return_check_number.as_deref(), Some("999"));
                assert_eq!(r.direction, crate::bridge::ReceiptDirection::Return);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn conf_reflects_last_successful_command() {
        let mut s = logged_in();
        // Do a handshake to set last_command_id.
        dispatch(&mut s, Command::Sync, &Identity::default(), clock(), &mock(), &mut corr());
        let out = dispatch(&mut s, Command::ConfLower, &Identity::default(), clock(), &mock(), &mut corr());
        match &out[0] {
            Response::Data(p) => {
                // "SYNC" must appear at the last-command-id slot
                // (offset 4 "CONf" opcode + 98 = 102, width 4).
                let chars: Vec<char> = p.chars().collect();
                let slot: String = chars[4 + 102..4 + 102 + 4].iter().collect();
                assert_eq!(
                    slot, "SYNC",
                    "CONF must surface last successful command; payload={p}",
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn invalid_params_marks_last_command_id_truncated_to_4_chars() {
        let mut s = logged_in();
        let cmd = Command::InvalidParams {
            opcode: "PREPOSTEROUS".to_string(),
            body: String::new(),
        };
        dispatch(&mut s, cmd, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.last_command_id, "PREP");
    }

    // ── Control-review bug regression tests ───────────────────────

    #[test]
    fn failed_upas_clears_prior_cashier_registration() {
        // Protocol §5.1: unsuccessful UPAS clears the registration
        // flag.  An already-logged-in session that sees a wrong
        // password must NOT retain its prior cashier — a later
        // fiscal command then correctly surfaces SOFTUPAS.
        let mut s = logged_in();
        assert!(s.cashier_registered());

        let out = dispatch(&mut s, Command::Upas {
                password: "WRONGPWDXX".to_string(),
                cashier_id: "x".to_string(),
            }, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftUpas)]);
        assert!(!s.cashier_registered(), "failed UPAS must clear cashier_id");
        assert_eq!(s.state, SessionState::Connected);

        // Subsequent fiscal command now fails because session is
        // effectively unauthenticated.
        let out = dispatch(&mut s, Command::Prep("d".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Error(ErrorCode::SoftUpas)]);
    }

    #[test]
    fn ctxt_does_not_close_receipt() {
        // CTXT is "clear free-text lines" per protocol; it must NOT
        // close an in-flight receipt or wipe the psdt_sequence /
        // pending_return_check_number counters.  A previous draft
        // treated CTXT like CANC which would have silently lost
        // accumulated state.
        let mut s = logged_in();
        s.state = SessionState::ReceiptOpen(OpenReceipt {
            department: "Bar".to_string(),
            ..OpenReceipt::default()
        });
        s.psdt_sequence = 2;
        s.pending_return_check_number = None;

        let out = dispatch(&mut s, Command::Ctxt, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(out, vec![Response::Done, Response::Ready]);
        assert!(s.receipt_open(), "CTXT must not close the receipt");
        assert_eq!(s.psdt_sequence, 2, "CTXT must not reset psdt_sequence");
        assert_eq!(s.last_command_id, "CTXT");
    }

    #[test]
    fn abort_check_resets_psdt_sequence_and_pending_return_number() {
        // CANC, unlike CTXT, is the real "abort receipt" path — it
        // SHOULD wipe accumulated state.
        let mut s = logged_in();
        s.state = SessionState::ReceiptOpen(OpenReceipt::default());
        s.psdt_sequence = 3;
        s.pending_return_check_number = Some("99".to_string());

        dispatch(&mut s, Command::Cnac, &Identity::default(), clock(), &mock(), &mut corr());
        assert!(!s.receipt_open());
        assert_eq!(s.psdt_sequence, 0);
        assert!(s.pending_return_check_number.is_none());
    }

    #[test]
    fn successful_reauthentication_does_not_kick_state_backwards() {
        // If a logged-in cashier sends UPAS again (maybe to change
        // cashier_id mid-shift), state must stay Authenticated.
        let mut s = logged_in();
        s.state = SessionState::Authenticated;
        dispatch(&mut s, Command::Upas {
                password: "1111111111".to_string(),
                cashier_id: "csh2".to_string(),
            }, &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.state, SessionState::Authenticated);
        assert_eq!(s.cashier_id.as_deref(), Some("csh2"));
    }

    #[test]
    fn conf_after_every_kind_of_command_returns_valid_148_char_body() {
        // CONF is 1C's liveness check — it MUST succeed regardless
        // of prior command flow.  Brute-force invariant: after any
        // command (valid or not), CONf returns a well-formed 152-char
        // payload.
        let identity = Identity::default();
        let sample_cmds = [
            Command::Csin(true),
            Command::Sync,
            Command::Prep("d".to_string()),
            Command::Cnac,
            Command::Head("h".to_string()),
            Command::Unknown { opcode: "ZZZZ".to_string(), body: String::new() },
        ];
        for cmd in sample_cmds {
            let mut s = logged_in();
            let _ = dispatch(&mut s, cmd, &identity, clock(), &mock(), &mut corr());
            let out = dispatch(&mut s, Command::ConfLower, &identity, clock(), &mock(), &mut corr());
            match &out[0] {
                Response::Data(p) => {
                    assert!(p.starts_with("CONf"));
                    assert_eq!(p.chars().count(), 4 + 148);
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn bchn_does_not_transition_state() {
        // BCHN just stages the return-check number for the next PREP
        // — it must not change state or affect an in-flight receipt.
        let mut s = logged_in();
        dispatch(&mut s, Command::Bchn("42".to_string()), &Identity::default(), clock(), &mock(), &mut corr());
        assert_eq!(s.state, SessionState::Authenticated);
        assert_eq!(s.pending_return_check_number.as_deref(), Some("42"));
    }
}
