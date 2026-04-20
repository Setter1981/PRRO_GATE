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
        if text.len() < 4 {
            return Self::Unknown { opcode: text.to_string(), body: String::new() };
        }
        let (opcode, body) = text.split_at(4);
        let body_owned = body.to_string();

        match opcode {
            "CSIN" => match body.chars().next() {
                Some('0') => Self::Csin(false),
                Some('1') => Self::Csin(true),
                _ => Self::Unknown { opcode: opcode.into(), body: body_owned },
            },
            "SYNC" => Self::Sync,
            "UPAS" if body.len() >= 10 => Self::Upas {
                password: body[..10].to_string(),
                cashier_id: body[10..].to_string(),
            },
            "SVSL" if !body.is_empty() => Self::Svsl {
                mode: body.chars().next().unwrap(),
                password: if body.len() >= 5 {
                    Some(body[1..5].to_string())
                } else {
                    None
                },
            },

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
            "CVAR" => Self::Cvar(body_owned),
            "GRBG" => Self::Grbg(body_owned),
            "GREN" => Self::Gren {
                discount_name: if body.len() >= 22 { Some(body[..22].to_string()) } else { None },
                upcount_name: if body.len() > 22 { Some(body[22..].to_string()) } else { None },
            },
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

            "CAIO" if body.len() >= 11 => {
                // CAIO is the real 4-char opcode; first body char is
                // direction ('I' = in, 'O' = out), next 10 chars are
                // decimal sum, rest is description.
                let dir = body.chars().next().unwrap();
                let sum: u64 = body[1..11].parse().unwrap_or(0);
                let desc = body[11..].to_string();
                match dir {
                    'I' => Self::Caioi { sum_kopecks: sum, description: desc },
                    'O' => Self::Caioo { sum_kopecks: sum, description: desc },
                    _ => Self::Unknown { opcode: opcode.into(), body: body_owned },
                }
            }
            "ZREP" => Self::Zrep,
            "NREP" => Self::Nrep,
            "nrep" => Self::NrepLowercase,
            "FIRN" if body.len() >= 8 => Self::Firn {
                first: body[..4].parse().unwrap_or(0),
                last: body[4..8].parse().unwrap_or(0),
            },
            "FIRP" if body.len() >= 16 => Self::Firp {
                from: body[..8].to_string(),
                to: body[8..16].to_string(),
            },
            "IREN" if body.len() >= 8 => Self::Iren {
                first: body[..4].parse().unwrap_or(0),
                last: body[4..8].parse().unwrap_or(0),
            },
            "IREP" if body.len() >= 16 => Self::Irep {
                from: body[..8].to_string(),
                to: body[8..16].to_string(),
            },
            "ARTZ" => Self::Artz,
            "DIZV" => Self::Dizv,
            "NULL" => Self::Null,
            "KASS" => Self::Kass,
            "DBEG" => Self::Dbeg,
            "PRTX" => Self::Prtx,

            "ARMO" if !body.is_empty() => {
                let mode = u8::try_from(body.chars().next().unwrap_or('0').to_digit(10).unwrap_or(0)).unwrap_or(0);
                Self::Armo(mode)
            }
            "DEPT" => Self::Dept(body_owned),
            "HEAD" => Self::Head(body_owned),
            "BOTM" => Self::Botm(body_owned),
            "BOTm" => Self::BotmIdx(body_owned),
            "NPDI" if !body.is_empty() => {
                let mode = u8::try_from(body.chars().next().unwrap_or('0').to_digit(10).unwrap_or(0)).unwrap_or(0);
                Self::Npdi(mode)
            }
            "ZDNM" => Self::Zdnm(body_owned),
            "CTIM" => Self::Ctim(body_owned),
            "SZKR" if !body.is_empty() => {
                let mode = u8::try_from(body.chars().next().unwrap_or('0').to_digit(10).unwrap_or(0)).unwrap_or(0);
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

            _ => Self::Unknown { opcode: opcode.to_string(), body: body_owned },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Frame;

    fn parse(s: &str) -> Command {
        Command::parse(&Frame { text: s.to_string(), had_crc: false })
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
            Command::Upas { password, cashier_id } => {
                assert_eq!(password, "1111111111");
                assert_eq!(cashier_id, "Cashier");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn upas_short_password_falls_through_to_unknown() {
        let c = parse("UPASshort"); // only 9 chars after opcode
        assert!(matches!(c, Command::Unknown { .. }));
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
            Command::Nlpr { tax1_char, tax2_char } => {
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
            Command::Caioi { sum_kopecks, description } => {
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

    #[test]
    fn every_real_opcode_parses_to_a_non_unknown_variant() {
        // Inventory check — this list must include every opcode the
        // driver actually handles as a real command.  New handlers
        // must extend this list when they add a variant.
        for opcode in [
            "CSIN1", "SYNC", "UPAS1111111111", "SVSL1",
            "CONF", "CONf", "GETD", "GLCN", "CCAS", "CFIS", "CNAL", "ARTD0001",
            "PREP1", "BCHN123", "GRBG grp",
            "GREN                      ", // 26 chars body
            "COMP0000000000000000000000000000000000000000000000000000000000",
            "CANC", "CTXT", "FINF name", "TGCD 123456789",
            "FISC body", "BFIS body", "ARFI body", "ARBF body", "FICD body", "BFCD body",
            "NLPRГА", "ACLD00", "PSDt body", "CSHG body",
            "CAIOI0000050000desc",
            "CAIOO0000050000desc",
            "ZREP", "NREP", "nrep",
            "FIRN00010025", "FIRP2026010120261231",
            "IREN00010025", "IREP2026010120261231",
            "ARTZ", "DIZV", "NULL", "KASS", "DBEG", "PRTX",
            "ARMO2", "DEPTBar", "HEADLine", "BOTMLine", "BOTm01 0 0 x",
            "NPDI0", "ZDNM disc", "CTIM123000", "SZKR1", "PZKR500",
            "STFL", "CUTR1 1", "NALG", "NNAMA Name", "BLFI05",
            "NCDC", "DSTR", "DISP", "DISp", "DIsp", "MDMDbb", "TSES",
        ] {
            let cmd = parse(opcode);
            assert!(
                !matches!(cmd, Command::Unknown { .. }),
                "{opcode:?} unexpectedly parsed to Unknown",
            );
        }
    }
}
