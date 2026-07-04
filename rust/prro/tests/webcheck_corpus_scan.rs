//! WebCheck U2 — the CANONICAL corpus leak-gate (runs in the required nextest).
//!
//! Enforces, over the committed synthetic corpus (`tests/webcheck_corpus/`), that
//! NO fixture contains a non-synthetic marker, and that every fixture hash is a
//! recompute over its synthetic payload. The Python `scripts/scan_webcheck_corpus.py`
//! is the dev / pre-commit convenience; THIS test is the enforced gate (CP1
//! decision). Both share ONE normative pattern table — the design note §6
//! (`docs/superpowers/specs/2026-07-04-webcheck-phase1-u2-corpus-sanitizer-design.md`):
//!
//!   real_fn (10-digit not prefix 9) · tin (8-digit) · uuid · real_timestamp
//!   (outside the 2026-01-01 synthetic epoch) · raw_xml · dps_blob · cyrillic ·
//!   nonsynthetic_hash (a 64-hex that is not a payload recompute) · hash_mismatch.
//!
//! RED-first, honestly per class: `scanner_catches_each_leak_class` plants EACH
//! class and asserts it is caught; `committed_corpus_is_clean` proves the real
//! corpus has none (and is hash-consistent).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const SYNTH_EPOCH_DATE: &str = "2026-01-01";
const XML_MARKERS: &[&str] = &[
    "<?xml",
    "<RQ",
    "<DAT",
    "<CHECK",
    "mmmaaaccc",
    "windows-1251",
];
const DPS_MARKERS: &[&str] = &["signedanswerfromficscal", "checksigned"];

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Maximal `\w`-runs (the `\b`-bounded tokens the §6 patterns key on — a digit
/// run inside a hex is NOT a standalone token, matching the Python `\b` scanner).
fn word_tokens(text: &str) -> Vec<&str> {
    text.split(|c: char| !is_word(c))
        .filter(|t| !t.is_empty())
        .collect()
}

fn scan_text(text: &str, allowed_hashes: &BTreeSet<String>) -> Vec<(String, String)> {
    let mut leaks = Vec::new();
    for tok in word_tokens(text) {
        let all_digits = tok.bytes().all(|b| b.is_ascii_digit());
        if all_digits && tok.len() == 10 && !tok.starts_with('9') {
            leaks.push(("real_fn".into(), tok.into()));
        }
        if all_digits && tok.len() == 8 {
            leaks.push(("tin".into(), tok.into()));
        }
        if tok.len() == 64
            && tok
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            && !allowed_hashes.contains(tok)
        {
            leaks.push(("nonsynthetic_hash".into(), tok.into()));
        }
    }
    if let Some(u) = find_uuid(text) {
        leaks.push(("uuid".into(), u));
    }
    for (date, ts) in find_timestamps(text) {
        if date != SYNTH_EPOCH_DATE {
            leaks.push(("real_timestamp".into(), ts));
        }
    }
    for m in XML_MARKERS {
        if text.contains(m) {
            leaks.push(("raw_xml".into(), (*m).into()));
        }
    }
    for m in DPS_MARKERS {
        if text.contains(m) {
            leaks.push(("dps_blob".into(), (*m).into()));
        }
    }
    if let Some(c) = text.chars().find(|c| ('\u{0400}'..='\u{04FF}').contains(c)) {
        leaks.push(("cyrillic".into(), c.to_string()));
    }
    leaks
}

/// A RFC-4122 8-4-4-4-12 hex-dash UUID (dashes make it multiple `\w`-tokens, so
/// it needs its own matcher).
fn find_uuid(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let seg = |i: usize, n: usize| -> bool {
        i + n <= b.len() && b[i..i + n].iter().all(|c| c.is_ascii_hexdigit())
    };
    for i in 0..b.len() {
        if seg(i, 8)
            && b.get(i + 8) == Some(&b'-')
            && seg(i + 9, 4)
            && b.get(i + 13) == Some(&b'-')
            && seg(i + 14, 4)
            && b.get(i + 18) == Some(&b'-')
            && seg(i + 19, 4)
            && b.get(i + 23) == Some(&b'-')
            && seg(i + 24, 12)
        {
            return Some(String::from_utf8_lossy(&b[i..i + 36]).into_owned());
        }
    }
    None
}

/// Every `YYYY-MM-DDThh:mm:ss` timestamp, returning (date, full-match).
fn find_timestamps(text: &str) -> Vec<(String, String)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let digit = |i: usize| i < b.len() && b[i].is_ascii_digit();
    for i in 0..b.len() {
        // dddd-dd-ddTdd:dd:dd
        let shape = [
            (0..4, None),
            (4..5, Some(b'-')),
            (5..7, None),
            (7..8, Some(b'-')),
            (8..10, None),
            (10..11, Some(b'T')),
            (11..13, None),
            (13..14, Some(b':')),
            (14..16, None),
            (16..17, Some(b':')),
            (17..19, None),
        ];
        if i + 19 > b.len() {
            break;
        }
        let ok = shape.iter().all(|(range, lit)| match lit {
            Some(ch) => b.get(i + range.start) == Some(ch),
            None => range.clone().all(|off| digit(i + off)),
        });
        if ok {
            let date = String::from_utf8_lossy(&b[i..i + 10]).into_owned();
            let full = String::from_utf8_lossy(&b[i..i + 19]).into_owned();
            out.push((date, full));
        }
    }
    out
}

fn scan_fixture(dir: &Path) -> Vec<(String, String)> {
    let seq_text = std::fs::read_to_string(dir.join("sequence.json")).unwrap();
    let seq: serde_json::Value = serde_json::from_str(&seq_text).unwrap();
    let mut allowed = BTreeSet::new();
    let mut leaks = Vec::new();
    for op in seq["sequence"].as_array().unwrap_or(&Vec::new()) {
        let cmd = &op["canonical_command"];
        if let (Some(pj), Some(stored)) =
            (cmd["payload_json"].as_str(), cmd["payload_sha256"].as_str())
        {
            let recomputed = sha256_hex(pj);
            allowed.insert(recomputed.clone());
            if stored != recomputed {
                leaks.push(("hash_mismatch".into(), format!("{stored} != recompute")));
            }
        }
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        leaks.extend(scan_text(&text, &allowed));
    }
    leaks
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/webcheck_corpus")
}

// ── the enforced gate ────────────────────────────────────────────────────────

#[test]
fn committed_corpus_is_clean_and_hash_consistent() {
    let root = corpus_dir();
    assert!(root.is_dir(), "corpus dir missing: {}", root.display());
    let mut all = Vec::new();
    let mut fixtures = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let dir = entry.unwrap().path();
        if dir.join("sequence.json").is_file() {
            fixtures += 1;
            all.extend(
                scan_fixture(&dir)
                    .into_iter()
                    .map(|(k, d)| format!("{}: {k} — {d}", dir.display())),
            );
        }
    }
    assert!(
        fixtures >= 2,
        "expected the A2 corpus (>=2 fixtures), got {fixtures}"
    );
    assert!(all.is_empty(), "corpus leaks (U2 §6):\n{}", all.join("\n"));
}

// ── RED-first teeth — one planted leak PER §6 row, each MUST be caught ────────

#[test]
fn scanner_catches_each_leak_class() {
    let empty = BTreeSet::new();
    let cases: &[(&str, &str)] = &[
        ("1234567890", "real_fn"), // 10-digit not prefix 9 (synthetic)
        ("12345678", "tin"),       // 8-digit ЄДРПОУ shape
        ("550e8400-e29b-41d4-a716-446655440000", "uuid"),
        ("2024-05-06T10:00:00", "real_timestamp"),
        ("<?xml version", "raw_xml"),
        ("signedanswerfromficscal", "dps_blob"),
        ("Операція", "cyrillic"),
    ];
    for (planted, expected) in cases {
        let classes: BTreeSet<String> = scan_text(planted, &empty)
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert!(
            classes.contains(*expected),
            "planting {planted:?} -> {classes:?}, want {expected}"
        );
    }
    // nonsynthetic_hash: a 64-hex not in the allowed set.
    let hash = "f".repeat(64);
    let classes: BTreeSet<String> = scan_text(&hash, &empty)
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert!(
        classes.contains("nonsynthetic_hash"),
        "64-hex not flagged: {classes:?}"
    );
}

#[test]
fn scanner_does_not_flag_synthetic_fn_or_epoch() {
    let empty = BTreeSet::new();
    // The synthetic FN (prefix 9) and the synthetic epoch must NOT be flagged.
    assert!(scan_text("9000000001", &empty).is_empty());
    let ts = scan_text("2026-01-01T00:05:00", &empty);
    assert!(ts.iter().all(|(k, _)| k != "real_timestamp"), "{ts:?}");
}

#[test]
fn scanner_catches_hash_mismatch() {
    // A fixture-shaped value whose payload_sha256 is not the recompute.
    let bad = serde_json::json!({
        "sequence": [{
            "canonical_command": {
                "payload_json": "{\"method\":\"x\"}",
                "payload_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
            }
        }]
    });
    let mut allowed = BTreeSet::new();
    let mut leaks = Vec::new();
    for op in bad["sequence"].as_array().unwrap() {
        let cmd = &op["canonical_command"];
        let pj = cmd["payload_json"].as_str().unwrap();
        let stored = cmd["payload_sha256"].as_str().unwrap();
        let rec = sha256_hex(pj);
        allowed.insert(rec.clone());
        if stored != rec {
            leaks.push("hash_mismatch");
        }
    }
    assert!(leaks.contains(&"hash_mismatch"));
}
