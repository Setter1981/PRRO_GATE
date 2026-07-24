//! Typed incoming commands.
//!
//! The wire codec hands us a [`Frame`] whose `text` field is the raw
//! `<opcode><params>` string.  This module parses that string into a
//! strongly-typed [`Command`] variant so the session dispatcher can
//! pattern-match on intent rather than on 4-character prefixes.
//!
//! **Scope of parsing here:** opcode + enough structure to route the
//! command (which handler module deals with it).  Fine-grained
//! parameter parsing happens inside the per-command handlers — e.g.
//! receipt-line field decoding lives in `session::receipt` where it
//! can share state with the accumulator.
//!
//! The opcode table is the canonical inventory — every known real and
//! stubbed command from the decompiled `maria_internal.cs` is listed
//! here.  Unknown opcodes fall through to [`Command::Unknown`], which
//! the dispatcher maps to a default `DONE` response.

use crate::wire::Frame;

/// Fully-typed representation of an incoming command frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // ── Session ────────────────────────────────────────────────────
    /// `CSIN<0|1>` — enable or disable CRC validation.
    Csin(bool),
    /// `SYNC` — protocol-level keepalive after `CSIN1`.
    Sync,
    /// `UPAS<10-char pwd><cashier_id>` — cashier login.
    Upas {
        password: String,
        cashier_id: String,
    },
    /// `SVSL<mode>[<pwd>]` — system-key virtual position.
    Svsl {
        mode: char,
        password: Option<String>,
    },

    // ── Queries ────────────────────────────────────────────────────
    Conf,
    ConfLower,
    Getd,
    Glcn,
    Ccas,
    Cfis,
    Cnal,
    Artd(String),

    // ── Receipt lifecycle ──────────────────────────────────────────
    Prep(String),
    Bchn(String),
    Cvar(String),
    Grbg(String),
    Gren {
        discount_name: Option<String>,
        upcount_name: Option<String>,
    },
    Comp(String),
    Cnac,
    Ctxt,
    /// `FINF<long-name>` — attach alt name to last line.
    Finf(String),
    /// `TGCD<barcode>` — mark next line with a barcode.
    Tgcd(String),

    // ── Fiscal items ───────────────────────────────────────────────
    /// `FISC<params>` / `BFIS<params>` in `REGISTER_NEW` mode.
    Fisc(String),
    Bfis(String),
    /// `ARFI<params>` / `ARBF<params>` in `USE_PROGRAMMED` mode.
    Arfi(String),
    Arbf(String),
    /// `FICD<params>` / `BFCD<params>` in `REGISTER_BY_ACCOUNTING_CODE` mode.
    Ficd(String),
    Bfcd(String),

    // ── Tax / excise / acquirer ────────────────────────────────────
    /// `NLPR<t1-char><t2-char>` — dual tax calc mode (Cyrillic А…Ж).
    Nlpr {
        tax1_char: char,
        tax2_char: char,
    },
    /// `ACLD<LLV><LLV>...` — batched excise stamp registration.
    Acld(String),
    /// `PSDt<n><params>` — acquirer slip (electronic payment receipt).
    Psdt(String),
    /// `CSHG<params>` — card cash withdrawal.
    Cshg(String),

    // ── Cash / shift / reports ─────────────────────────────────────
    /// `CAIOI<D10 sum><desc>` — cash deposit.
    Caioi {
        sum_kopecks: u64,
        description: String,
    },
    /// `CAIOO<D10 sum><desc>` — cash withdrawal.
    Caioo {
        sum_kopecks: u64,
        description: String,
    },
    /// `ZREP[<flag>]` — X-report (no fiscal zeroing).
    Zrep,
    /// `NREP` — Z-report (closes fiscal day).
    Nrep,
    /// `nrep` — open new shift without registering turnover.
    NrepLowercase,
    /// `FIRN<D4 first><D4 last>` — periodic full report by Z-numbers.
    Firn {
        first: u16,
        last: u16,
    },
    /// `FIRP<yyyyMMdd><yyyyMMdd>` — periodic full report by date.
    Firp {
        from: String,
        to: String,
    },
    /// `IREN<D4 first><D4 last>` — periodic short report by Z-numbers.
    Iren {
        first: u16,
        last: u16,
    },
    /// `IREP<yyyyMMdd><yyyyMMdd>` — periodic short report by date.
    Irep {
        from: String,
        to: String,
    },
    Artz,
    Dizv,
    Null,
    Kass,
    /// `DBEG` — open service document.
    Dbeg,
    /// `PRTX` — close and print service document.
    Prtx,

    // ── Setup / configuration (mostly stubs that mutate device state)
    /// `ARMO<0|1|2>` — article table mode.
    Armo(u8),
    /// `DEPT<alias>` — department alias string.
    Dept(String),
    /// `HEAD<line>` — receipt header line.
    Head(String),
    /// `BOTM<line>` — single-line bottom footer.
    Botm(String),
    /// `BOTm<idx><jrn><mode><text>` — indexed bottom footer (lowercase m).
    BotmIdx(String),
    /// `NPDI<0|1>` — discount print mode.
    Npdi(u8),
    /// `ZDNM<22-char discount name><upcount name>` — totals names.
    Zdnm(String),
    /// `CTIM<hhmmss>` — set internal clock.
    Ctim(String),
    /// `SZKR<0|1>` — enable rounding.
    Szkr(u8),
    /// `PZKR<D3>` — max rounding value.
    Pzkr(u16),
    /// `STFL` — line-by-line print mode.
    Stfl,
    /// `CUTR[<cut><beep>[<partial>]]` — cutter + buzzer.
    Cutr(String),
    /// `NALG[<scheme><type><rate>]` — program tax scheme.
    Nalg(String),
    /// `NNAM<scheme><name>` — tax scheme name.
    Nnam(String),
    /// `BLFI<D2>` — inter-line spacing.
    Blfi(String),
    /// `NCDC[<0|1>]` — continuous document mode.
    Ncdc(String),
    /// `DSTR[<flag>]` — item count print toggle.
    Dstr(String),

    // ── Display / cash-box / misc passthrough ──────────────────────
    Disp(String),
    DispLower(String), // DISp
    DispX(String),     // DIsp
    Mdmd(String),
    Tses,

    // ── Unknown opcode ─────────────────────────────────────────────
    /// Any 4+ byte opcode not recognised.  Dispatcher treats as stub.
    Unknown {
        opcode: String,
        body: String,
    },
    /// Opcode recognised but parameters are malformed — wrong length,
    /// bad encoding, etc.  Dispatcher replies with the opcode-specific
    /// `SOFT*` error rather than a blanket `DONE` (which is what
    /// `Unknown` gets).
    InvalidParams {
        opcode: String,
        body: String,
    },
}

/// Split a `&str` after the first `n` *characters* (not bytes).
///
/// Returns `None` when the input has fewer than `n` chars — caller
/// decides whether that's an error.  Never panics on multi-byte UTF-8,
/// unlike `body[..n]` byte slicing.
fn split_at_nth_char(s: &str, n: usize) -> Option<(&str, &str)> {
    if n == 0 {
        return Some(("", s));
    }
    let mut iter = s.char_indices();
    for _ in 0..n - 1 {
        iter.next()?;
    }
    // At this point iter.next() yields the nth char; we want the byte
    // offset *after* that char — use char_indices position of (n+1)th,
    // or fall through to end-of-string if the nth is the last.
    match iter.nth(1) {
        Some((idx, _)) => Some(s.split_at(idx)),
        None => {
            // Confirm we actually consumed n chars (guards against
            // fewer-than-n inputs that happen to align to n-1).
            if s.chars().count() == n {
                Some((s, ""))
            } else {
                None
            }
        }
    }
}

impl Command {
    /// Parse a [`Frame`] into a typed [`Command`].
    ///
    /// Never fails — the frame text is already validated by the wire
    /// codec (length bounds, CRC, sanitization).  Parameters that the
    /// parser cannot decode are stashed verbatim in the variant's
    /// string field for the handler to deal with.
    #[must_use]
    pub fn parse(frame: &Frame) -> Self {
        Self::parse_text(&frame.text)
    }

    /// Parse a raw command string (without the outer wire framing).
    ///
    /// Exposed for tests and for the admin "replay" tool that feeds
    /// commands from a JSONL capture.
    ///
    /// # Panics
    /// Never — every `unwrap` in this function is guarded by a prior
    /// length / presence check in the match arm that calls it.
    #[must_use]
    #[allow(clippy::too_many_lines)] // Large dispatch is the point of the function.
    pub fn parse_text(text: &str) -> Self {
        // Char-count guard (not byte-count) — protects against a 3-char
        // multi-byte input (6 bytes) whose byte-level `.len() >= 4`
        // would pass but `split_at(4)` would panic on a char boundary.
        let char_count = text.chars().count();
        if char_count < 4 {
            return Self::Unknown {
                opcode: text.to_string(),
                body: String::new(),
            };
        }
        // All opcodes in the Maria protocol are ASCII 4-char — if the
        // first 4 chars contain non-ASCII, it cannot be a real opcode,
        // so treat the whole thing as Unknown safely.
        let Some((opcode, body)) = split_at_nth_char(text, 4) else {
            return Self::Unknown {
                opcode: text.to_string(),
                body: String::new(),
            };
        };
        let body_owned = body.to_string();

        match opcode {
            "CSIN" => match body.chars().next() {
                Some('0') => Self::Csin(false),
                Some('1') => Self::Csin(true),
                _ => Self::Unknown {
                    opcode: opcode.into(),
                    body: body_owned,
                },
            },
            "SYNC" => Self::Sync,
            "UPAS" => {
                // Password is 10 ASCII chars per protocol §5.1; cashier_id
                // is arbitrary (may be Cyrillic).  Char-split is used to
                // survive malformed non-ASCII passwords (fuzz input).
                if let Some((password, cashier_id)) = split_at_nth_char(body, 10) {
                    Self::Upas {
                        password: password.to_string(),
                        cashier_id: cashier_id.to_string(),
                    }
                } else {
                    Self::InvalidParams {
                        opcode: opcode.into(),
                        body: body_owned,
                    }
                }
            }
            "SVSL" => {
                let mut chars = body.chars();
                match chars.next() {
                    Some(mode) => {
                        // Remaining body after the mode char — safe char-based offset.
                        let after_mode = chars.as_str();
                        let password =
                            split_at_nth_char(after_mode, 4).map(|(pwd, _)| pwd.to_string());
                        Self::Svsl { mode, password }
                    }
                    None => Self::InvalidParams {
                        opcode: opcode.into(),
                        body: body_owned,
                    },
                }
            }

            "CONF" => Self::Conf,
            "CONf" => Self::ConfLower,
            "GETD" => Self::Getd,
            "GLCN" => Self::Glcn,
            "CCAS" => Self::Ccas,
            "CFIS" => Self::Cfis,
            "CNAL" => Self::Cnal,
            "ARTD" => Self::Artd(body_owned),

            "PREP" => Self::Prep(body_owned),
            "BCHN" => Self::Bchn(body_owned),
            "CVAL" => Self::Cvar(body_owned),
            "GRBG" => Self::Grbg(body_owned),
            "GREN" => {
                // Discount & upcount names are CHAR-count-bounded (22 symbols
                // per protocol) and may contain Cyrillic — we must never
                // byte-slice here.
                match split_at_nth_char(body, 22) {
                    Some((discount, upcount)) => Self::Gren {
                        discount_name: if discount.is_empty() {
                            None
                        } else {
                            Some(discount.to_string())
                        },
                        upcount_name: if upcount.is_empty() {
                            None
                        } else {
                            Some(upcount.to_string())
                        },
                    },
                    None => Self::Gren {
                        discount_name: if body.is_empty() {
                            None
                        } else {
                            Some(body_owned.clone())
                        },
                        upcount_name: None,
                    },
                }
            }
            "COMP" => Self::Comp(body_owned),
            "CANC" => Self::Cnac,
            "CTXT" => Self::Ctxt,
            "FINF" => Self::Finf(body_owned),
            "TGCD" => Self::Tgcd(body_owned),

            "FISC" => Self::Fisc(body_owned),
            "BFIS" => Self::Bfis(body_owned),
            "ARFI" => Self::Arfi(body_owned),
            "ARBF" => Self::Arbf(body_owned),
            "FICD" => Self::Ficd(body_owned),
            "BFCD" => Self::Bfcd(body_owned),

            "NLPR" if body.chars().count() >= 2 => {
                let mut cs = body.chars();
                Self::Nlpr {
                    tax1_char: cs.next().unwrap(),
                    tax2_char: cs.next().unwrap(),
                }
            }
            "ACLD" => Self::Acld(body_owned),
            "PSDt" => Self::Psdt(body_owned),
            "CSHG" => Self::Cshg(body_owned),

            "CAIO" => {
                // Body layout: <direction 1 char ASCII><sum 10 ASCII digits><desc ...>
                // Use char-split so a malformed non-ASCII description cannot
                // panic the parser via byte slicing mid-char.
                let Some((direction_and_sum, description)) = split_at_nth_char(body, 11) else {
                    return Self::InvalidParams {
                        opcode: opcode.into(),
                        body: body_owned,
                    };
                };
                let mut prefix_chars = direction_and_sum.chars();
                let Some(dir) = prefix_chars.next() else {
                    return Self::InvalidParams {
                        opcode: opcode.into(),
                        body: body_owned,
                    };
                };
                let sum_str: String = prefix_chars.collect();
                let sum: u64 = match sum_str.parse::<u64>() {
                    Ok(v) if v > 0 => v,
                    _ => {
                        return Self::InvalidParams {
                            opcode: opcode.into(),
                            body: body_owned,
                        }
                    }
                };
                let desc = description.to_string();
                match dir {
                    'I' => Self::Caioi {
                        sum_kopecks: sum,
                        description: desc,
                    },
                    'O' => Self::Caioo {
                        sum_kopecks: sum,
                        description: desc,
                    },
                    _ => Self::InvalidParams {
                        opcode: opcode.into(),
                        body: body_owned,
                    },
                }
            }
            "ZREP" => Self::Zrep,
            "NREP" => Self::Nrep,
            "nrep" => Self::NrepLowercase,
            "FIRN" | "IREN" => {
                // Body: two D4 decimal Z-report numbers (8 ASCII chars).
                match split_at_nth_char(body, 4)
                    .and_then(|(a, rest)| split_at_nth_char(rest, 4).map(|(b, _)| (a, b)))
                {
                    Some((a, b)) => {
                        let first = a.parse().unwrap_or(0);
                        let last = b.parse().unwrap_or(0);
                        if opcode == "FIRN" {
                            Self::Firn { first, last }
                        } else {
                            Self::Iren { first, last }
                        }
                    }
                    None => Self::InvalidParams {
                        opcode: opcode.into(),
                        body: body_owned,
                    },
                }
            }
            "FIRP" | "IREP" => {
                // Body: two yyyyMMdd strings (16 ASCII chars).
                match split_at_nth_char(body, 8)
                    .and_then(|(from, rest)| split_at_nth_char(rest, 8).map(|(to, _)| (from, to)))
                {
                    Some((from, to)) => {
                        if opcode == "FIRP" {
                            Self::Firp {
                                from: from.to_string(),
                                to: to.to_string(),
                            }
                        } else {
                            Self::Irep {
                                from: from.to_string(),
                                to: to.to_string(),
                            }
                        }
                    }
                    None => Self::InvalidParams {
                        opcode: opcode.into(),
                        body: body_owned,
                    },
                }
            }
            "ARTZ" => Self::Artz,
            "DIZV" => Self::Dizv,
            "NULL" => Self::Null,
            "KASS" => Self::Kass,
            "DBEG" => Self::Dbeg,
            "PRTX" => Self::Prtx,

            "ARMO" if !body.is_empty() => {
                let mode =
                    u8::try_from(body.chars().next().unwrap_or('0').to_digit(10).unwrap_or(0))
                        .unwrap_or(0);
                Self::Armo(mode)
            }
            "DEPT" => Self::Dept(body_owned),
            "HEAD" => Self::Head(body_owned),
            "BOTM" => Self::Botm(body_owned),
            "BOTm" => Self::BotmIdx(body_owned),
            "NPDI" if !body.is_empty() => {
                let mode =
                    u8::try_from(body.chars().next().unwrap_or('0').to_digit(10).unwrap_or(0))
                        .unwrap_or(0);
                Self::Npdi(mode)
            }
            "ZDNM" => Self::Zdnm(body_owned),
            "CTIM" => Self::Ctim(body_owned),
            "SZKR" if !body.is_empty() => {
                let mode =
                    u8::try_from(body.chars().next().unwrap_or('0').to_digit(10).unwrap_or(0))
                        .unwrap_or(0);
                Self::Szkr(mode)
            }
            "PZKR" => Self::Pzkr(body.parse().unwrap_or(0)),
            "STFL" => Self::Stfl,
            "CUTR" => Self::Cutr(body_owned),
            "NALG" => Self::Nalg(body_owned),
            "NNAM" => Self::Nnam(body_owned),
            "BLFI" => Self::Blfi(body_owned),
            "NCDC" => Self::Ncdc(body_owned),
            "DSTR" => Self::Dstr(body_owned),

            "DISP" => Self::Disp(body_owned),
            "DISp" => Self::DispLower(body_owned),
            "DIsp" => Self::DispX(body_owned),
            "MDMD" => Self::Mdmd(body_owned),
            "TSES" => Self::Tses,

            _ => Self::Unknown {
                opcode: opcode.to_string(),
                body: body_owned,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Frame;

    fn parse(s: &str) -> Command {
        Command::parse(&Frame {
            text: s.to_string(),
            had_crc: false,
        })
    }

    #[test]
    fn csin_parses_both_toggle_values() {
        assert_eq!(parse("CSIN1"), Command::Csin(true));
        assert_eq!(parse("CSIN0"), Command::Csin(false));
    }

    #[test]
    fn upas_splits_password_and_cashier() {
        let c = parse("UPAS1111111111Cashier");
        match c {
            Command::Upas {
                password,
                cashier_id,
            } => {
                assert_eq!(password, "1111111111");
                assert_eq!(cashier_id, "Cashier");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn upas_short_password_surfaces_as_invalid_params() {
        let c = parse("UPASshort"); // only 5 chars after opcode
        match c {
            Command::InvalidParams { opcode, body } => {
                assert_eq!(opcode, "UPAS");
                assert_eq!(body, "short");
            }
            other => panic!("expected InvalidParams, got {other:?}"),
        }
    }

    #[test]
    fn prep_with_department_name_captures_body_verbatim() {
        assert_eq!(parse("PREPBar1"), Command::Prep("Bar1".to_string()));
    }

    #[test]
    fn fisc_keeps_raw_body_for_receipt_handler() {
        let text = "FISC\u{001E}name\u{001E}1\u{001E}100";
        let c = parse(text);
        match c {
            Command::Fisc(body) => assert_eq!(body, &text[4..]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nlpr_splits_two_cyrillic_tax_chars() {
        let c = parse("NLPRГА"); // Г + А (cigarettes + regular)
        match c {
            Command::Nlpr {
                tax1_char,
                tax2_char,
            } => {
                assert_eq!(tax1_char, 'Г');
                assert_eq!(tax2_char, 'А');
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn caioi_parses_sum_and_description() {
        let c = parse("CAIOI0000050000Kasa ranok");
        match c {
            Command::Caioi {
                sum_kopecks,
                description,
            } => {
                assert_eq!(sum_kopecks, 50_000);
                assert_eq!(description, "Kasa ranok");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn firn_parses_two_d4_numbers() {
        let c = parse("FIRN00010025");
        match c {
            Command::Firn { first, last } => {
                assert_eq!(first, 1);
                assert_eq!(last, 25);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn firp_parses_two_dates_as_strings() {
        let c = parse("FIRP2026010120261231");
        match c {
            Command::Firp { from, to } => {
                assert_eq!(from, "20260101");
                assert_eq!(to, "20261231");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nrep_uppercase_and_lowercase_are_distinct_variants() {
        assert_eq!(parse("NREP"), Command::Nrep);
        assert_eq!(parse("nrep"), Command::NrepLowercase);
    }

    #[test]
    fn conf_and_conf_lower_are_distinct() {
        assert_eq!(parse("CONF"), Command::Conf);
        assert_eq!(parse("CONf"), Command::ConfLower);
    }

    #[test]
    fn armo_parses_single_digit_mode() {
        assert_eq!(parse("ARMO2"), Command::Armo(2));
        assert_eq!(parse("ARMO0"), Command::Armo(0));
    }

    #[test]
    fn unknown_opcode_is_still_split_into_opcode_and_body() {
        let c = parse("XYZAextra");
        match c {
            Command::Unknown { opcode, body } => {
                assert_eq!(opcode, "XYZA");
                assert_eq!(body, "extra");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn input_shorter_than_4_chars_maps_to_unknown() {
        assert!(matches!(parse("ABC"), Command::Unknown { .. }));
        assert!(matches!(parse(""), Command::Unknown { .. }));
    }

    #[test]
    fn lowercase_opcodes_are_distinct_from_uppercase() {
        // `nrep` is the official EKKA opcode for "open shift"; lowercase
        // vs uppercase is semantic, not cosmetic.
        assert_ne!(parse("NREP"), parse("nrep"));
        assert_eq!(parse("NREP"), Command::Nrep);
        assert_eq!(parse("nrep"), Command::NrepLowercase);
    }

    // ── UTF-8 safety regression tests (post-M2 review) ────────────

    #[test]
    fn upas_with_cyrillic_cashier_id_does_not_panic() {
        // This was a panic vector in the first M2 draft: byte-slicing
        // `body[..10]` would split multi-byte UTF-8 char boundaries on
        // a non-ASCII cashier suffix.
        let c = parse("UPAS1111111111Касир");
        match c {
            Command::Upas {
                password,
                cashier_id,
            } => {
                assert_eq!(password, "1111111111");
                assert_eq!(cashier_id, "Касир");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn upas_with_malformed_non_ascii_password_does_not_panic() {
        // Buggy / adversarial client sends Cyrillic in the password
        // position.  Driver must not crash — the char-based split
        // preserves the original bytes verbatim.
        let c = parse("UPAS123456789Кextra"); // 9 ASCII + 'К' (2 bytes) + 'extra'
        match c {
            Command::Upas {
                password,
                cashier_id,
            } => {
                // 10 chars taken — password = 9 ASCII digits + 'К'
                assert_eq!(password.chars().count(), 10);
                assert!(password.starts_with("123456789"));
                assert_eq!(cashier_id, "extra");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn upas_fewer_than_10_chars_is_invalid_params_not_panic() {
        let c = parse("UPAS1234");
        match c {
            Command::InvalidParams { opcode, .. } => assert_eq!(opcode, "UPAS"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn gren_with_22_cyrillic_chars_splits_correctly() {
        // 22 Cyrillic chars = 44 UTF-8 bytes.  Byte-slicing would crash
        // or produce half-length strings; char-splitting gets it right.
        let twenty_two_cyrillic = "ддддддддддддддддддддддУ"; // 22 'д' + 'У' as upcount start
        assert_eq!(twenty_two_cyrillic.chars().count(), 23);
        let input = format!("GREN{twenty_two_cyrillic}");
        let c = parse(&input);
        match c {
            Command::Gren {
                discount_name,
                upcount_name,
            } => {
                // First 22 chars of body → discount; remainder ('У') → upcount.
                assert_eq!(discount_name.as_deref(), Some("дддддддддддддддддддддд"),);
                assert_eq!(upcount_name.as_deref(), Some("У"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn gren_with_fewer_than_22_chars_lands_full_body_in_discount() {
        let c = parse("GRENshort");
        match c {
            Command::Gren {
                discount_name,
                upcount_name,
            } => {
                assert_eq!(discount_name.as_deref(), Some("short"));
                assert_eq!(upcount_name, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn svsl_with_cyrillic_mode_char_does_not_panic() {
        // Protocol says mode is ASCII.  Malformed client may send
        // Cyrillic — we must not crash.
        let c = parse("SVSLА"); // 'А' = U+0410 = 2 UTF-8 bytes
        match c {
            Command::Svsl { mode, password } => {
                assert_eq!(mode, 'А');
                // Body after mode is empty, so password is None.
                assert_eq!(password, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn caio_with_cyrillic_description_does_not_panic() {
        // Description after the 11 ASCII header chars may be Cyrillic.
        let c = parse("CAIOI0000050000Каса ранок");
        match c {
            Command::Caioi {
                sum_kopecks,
                description,
            } => {
                assert_eq!(sum_kopecks, 50_000);
                assert_eq!(description, "Каса ранок");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn caio_body_shorter_than_11_chars_is_invalid_params() {
        let c = parse("CAIOI12345"); // only 6 chars after 'CAIO'
        match c {
            Command::InvalidParams { opcode, .. } => assert_eq!(opcode, "CAIO"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn caio_with_invalid_direction_char_surfaces_as_invalid_params() {
        let c = parse("CAIOX0000050000desc");
        match c {
            Command::InvalidParams { opcode, .. } => assert_eq!(opcode, "CAIO"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn caio_invalid_string_returns_error() {
        // Non-numeric characters in the 10-digit sum field → InvalidParams.
        let c = parse("CAIOIXXXXXXXXXXdesc");
        match c {
            Command::InvalidParams { opcode, .. } => assert_eq!(opcode, "CAIO"),
            other => panic!("expected InvalidParams, got {other:?}"),
        }
    }

    #[test]
    fn caio_zero_amount_returns_error() {
        // Zero sum is semantically invalid — a deposit/withdrawal of 0 kopecks
        // is never a legitimate fiscal operation.
        let c = parse("CAIOI0000000000desc");
        match c {
            Command::InvalidParams { opcode, .. } => assert_eq!(opcode, "CAIO"),
            other => panic!("expected InvalidParams, got {other:?}"),
        }
    }

    #[test]
    fn caio_valid_amount_succeeds() {
        // Deposit: 50 000 kopecks (500 UAH), direction 'I' → Caioi.
        let c = parse("CAIOI0000050000desc");
        match c {
            Command::Caioi {
                sum_kopecks,
                description,
            } => {
                assert_eq!(sum_kopecks, 50_000);
                assert_eq!(description, "desc");
            }
            other => panic!("expected Caioi, got {other:?}"),
        }
        // Withdrawal: direction 'O' → Caioo.
        let c2 = parse("CAIOO0000050000desc");
        assert!(matches!(
            c2,
            Command::Caioo {
                sum_kopecks: 50_000,
                ..
            }
        ));
    }

    #[test]
    fn three_char_cyrillic_input_does_not_panic() {
        // "абв" = 3 chars / 6 UTF-8 bytes.  The old `text.len() < 4`
        // byte-level guard would pass (6 >= 4) and then `split_at(4)`
        // would panic on a multi-byte boundary.  Char-level guard
        // prevents this.
        let c = parse("абв");
        assert!(matches!(c, Command::Unknown { .. }));
    }

    #[test]
    fn firn_with_bad_body_length_surfaces_as_invalid_params() {
        let c = parse("FIRN0001"); // only 4 chars body, need 8
        match c {
            Command::InvalidParams { opcode, .. } => assert_eq!(opcode, "FIRN"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn firp_with_bad_body_length_surfaces_as_invalid_params() {
        let c = parse("FIRP2026");
        match c {
            Command::InvalidParams { opcode, .. } => assert_eq!(opcode, "FIRP"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cval_opcode_matches_decompiled_reference_not_cvar() {
        // Resonance OLE DLL uses `CVAL` (currency setup).  An earlier
        // draft of this parser mistyped it as `CVAR`.  This test pins
        // the correct opcode so a regression trips immediately.
        let c = parse("CVAL2USD1.00000000");
        assert!(
            matches!(c, Command::Cvar(_)),
            "CVAL must parse, not fall through"
        );
    }

    // ── Inventory guard (unchanged) ───────────────────────────────

    #[test]
    fn every_real_opcode_parses_to_a_non_unknown_variant() {
        // Inventory check — this list must include every opcode the
        // driver actually handles as a real command.  New handlers
        // must extend this list when they add a variant.
        for opcode in [
            "CSIN1",
            "SYNC",
            "UPAS1111111111",
            "SVSL1",
            "CONF",
            "CONf",
            "GETD",
            "GLCN",
            "CCAS",
            "CFIS",
            "CNAL",
            "ARTD0001",
            "PREP1",
            "BCHN123",
            "CVAL2USD1.00000000",
            "GRBG grp",
            "GREN                      ", // 26 chars body
            "COMP0000000000000000000000000000000000000000000000000000000000",
            "CANC",
            "CTXT",
            "FINF name",
            "TGCD 123456789",
            "FISC body",
            "BFIS body",
            "ARFI body",
            "ARBF body",
            "FICD body",
            "BFCD body",
            "NLPRГА",
            "ACLD00",
            "PSDt body",
            "CSHG body",
            "CAIOI0000050000desc",
            "CAIOO0000050000desc",
            "ZREP",
            "NREP",
            "nrep",
            "FIRN00010025",
            "FIRP2026010120261231",
            "IREN00010025",
            "IREP2026010120261231",
            "ARTZ",
            "DIZV",
            "NULL",
            "KASS",
            "DBEG",
            "PRTX",
            "ARMO2",
            "DEPTBar",
            "HEADLine",
            "BOTMLine",
            "BOTm01 0 0 x",
            "NPDI0",
            "ZDNM disc",
            "CTIM123000",
            "SZKR1",
            "PZKR500",
            "STFL",
            "CUTR1 1",
            "NALG",
            "NNAMA Name",
            "BLFI05",
            "NCDC",
            "DSTR",
            "DISP",
            "DISp",
            "DIsp",
            "MDMDbb",
            "TSES",
        ] {
            let cmd = parse(opcode);
            assert!(
                !matches!(cmd, Command::Unknown { .. }),
                "{opcode:?} unexpectedly parsed to Unknown",
            );
        }
    }
}
