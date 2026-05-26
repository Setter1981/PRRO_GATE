//! W2 PR-B — LOW-PR90-01 password input behavior.
//!
//! Per `docs/superpowers/plans/2026-05-25-m4-ingress-plan.md` §3 W2
//! Acceptance LOW-PR90-01:
//!
//!   "password input behavior typed (TTY double-input confirmation;
//!    non-TTY single stdin line; empty refusal).  Test
//!    add_operator_cli_password_input.rs covers all three branches."
//!
//! Four explicit sub-cases:
//!
//!   1. TTY with two matching passwords → returns the password bytes.
//!   2. TTY with two mismatched passwords → `AdminError::PasswordMismatch`.
//!   3. Non-TTY single-line stdin → returns the password bytes.
//!   4. Empty input (either mode) → `AdminError::EmptyPassword`.
//!
//! Plus one IO-error case (PasswordReadIo exit code 74).

use prro::admin::{acquire_password, AdminError, PasswordPrompter};
use std::collections::VecDeque;
use std::io;

/// Test prompter — returns scripted responses in order.
struct ScriptedPrompter {
    responses: VecDeque<io::Result<String>>,
    prompts_seen: Vec<String>,
}

impl ScriptedPrompter {
    fn ok(responses: &[&str]) -> Self {
        Self {
            responses: responses.iter().map(|s| Ok(s.to_string())).collect(),
            prompts_seen: Vec::new(),
        }
    }

    fn io_err(kind: io::ErrorKind, msg: &str) -> Self {
        let mut q = VecDeque::new();
        q.push_back(Err(io::Error::new(kind, msg.to_string())));
        Self {
            responses: q,
            prompts_seen: Vec::new(),
        }
    }
}

impl PasswordPrompter for ScriptedPrompter {
    fn prompt(&mut self, prompt: &str) -> io::Result<String> {
        self.prompts_seen.push(prompt.to_string());
        self.responses
            .pop_front()
            .unwrap_or_else(|| Err(io::Error::new(io::ErrorKind::UnexpectedEof, "no more")))
    }
}

#[test]
fn tty_two_matching_passwords_returns_bytes() {
    let mut p = ScriptedPrompter::ok(&["matching-pw-123", "matching-pw-123"]);
    let bytes = acquire_password(&mut p, true).expect("matching passwords accepted");
    assert_eq!(&bytes[..], b"matching-pw-123");
    assert_eq!(
        p.prompts_seen.len(),
        2,
        "TTY mode must prompt twice (password + repeat)"
    );
}

#[test]
fn tty_mismatched_passwords_returns_mismatch_error() {
    let mut p = ScriptedPrompter::ok(&["first-attempt", "different-second"]);
    let err = acquire_password(&mut p, true).expect_err("mismatch must reject");
    assert!(matches!(err, AdminError::PasswordMismatch));
    assert_eq!(err.exit_code(), 64, "EX_USAGE for operator misuse");
}

#[test]
fn non_tty_single_line_returns_bytes() {
    let mut p = ScriptedPrompter::ok(&["scripted-pw-from-pipe"]);
    let bytes = acquire_password(&mut p, false).expect("non-TTY single line accepted");
    assert_eq!(&bytes[..], b"scripted-pw-from-pipe");
    assert_eq!(
        p.prompts_seen.len(),
        1,
        "non-TTY mode must prompt once (no confirmation)"
    );
}

#[test]
fn empty_input_tty_returns_empty_password_error() {
    let mut p = ScriptedPrompter::ok(&["", ""]);
    let err = acquire_password(&mut p, true).expect_err("empty TTY input must reject");
    assert!(matches!(err, AdminError::EmptyPassword));
    assert_eq!(err.exit_code(), 64);
}

#[test]
fn empty_input_non_tty_returns_empty_password_error() {
    let mut p = ScriptedPrompter::ok(&[""]);
    let err = acquire_password(&mut p, false).expect_err("empty stdin must reject");
    assert!(matches!(err, AdminError::EmptyPassword));
}

#[test]
fn io_error_on_first_prompt_returns_password_read_io_error() {
    let mut p = ScriptedPrompter::io_err(io::ErrorKind::BrokenPipe, "tty broken");
    let err = acquire_password(&mut p, true).expect_err("IO error must surface");
    match err {
        AdminError::PasswordReadIo(msg) => {
            assert!(
                msg.contains("tty broken") || msg.contains("BrokenPipe"),
                "wrapped IO error should mention the underlying cause, got: {msg}"
            );
        }
        other => panic!("expected PasswordReadIo, got: {other:?}"),
    }
}
