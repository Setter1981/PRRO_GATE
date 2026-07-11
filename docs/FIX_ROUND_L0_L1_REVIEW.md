# Fix-round — L0+L1 review holes (PR #257)

**Branch:** `feat/l0-l1-cash-ledger` (worktree `/home/setter/prro_gate/.claude/worktrees/l0-l1-cash-ledger`), continue on it (add fix commits). Strict RED-first. Do NOT push to main. **Operator: fix ALL 3, build REAL teeth (no known-red shortcut).**

## Process rules (learned this round — non-negotiable)
- **Gate = `cargo nextest run --features test-support`** from `rust/prro/` (123 tests; without the feature the test binaries don't even compile). Do NOT claim green without THIS exact run pasted.
- After any teeth experiment (disabling a guard), **RESTORE it and leave the working tree CLEAN** (`git status` empty). A prior run left an uncommitted `if false` that caused a false RED. Commit only intended changes.
- Report honestly: verified vs inferred. Paste the actual `nextest`/`clippy` tail.

## ★ The synergy (do Hole 2 first — it enables Hole 1's real teeth)
The fuzzer enters at `inline::run` (write-path) via `seed_inbox_return()`, **below** `convert_to_signer_payload` where the current INV-21 guard lives → the guard is unreachable from the fuzzer (that is why the fuzzer stayed 108/108 green with the guard disabled = fake teeth). Fixing Hole 2 by adding the cash-floor re-check **inside the write-path under the FN lease** puts the enforcement point **in the fuzzer's lane**, so Hole 1's model prediction then has a real prod counterpart to diverge from.

## HOLE 2 (MUST) — INV-21 TOCTOU → in-lease re-check
**Defect:** the guard reads `cash_on_hand` pre-inbox, pre-lease (`convert.rs:857-867`); two concurrent same-FN cash RETURNs (distinct idem keys) both read the same balance, both pass, both mint under the lease → cash < 0, both issued. The FN lease serializes mints but never re-checks cash.
**Fix:** keep the pre-inbox guard as the fast row-less UX refusal. ADD a **second cash-floor check inside the write-path, under the FN lease** (the single-writer serialized section — `stage_acquire`/`stage_guard`, same envelope that holds `acquire_lease`/`with_immediate`), computed from the **same-tx snapshot** right before the RETURN is minted. On shortfall → terminalise the doc fail-closed (a **pre-SEND refusal**: `Sending`-never-reached → a resting non-issued terminal per the CLAUDE.md persistence model — mirror the existing pre-send refusal/abort pattern, e.g. how `CodePoolExhausted` terminalises), with an audit. Seed NOT advanced, doc NOT issued.
**RED pins:** (1) two serial RETURNs that each pass the pre-inbox guard but the second must be refused in-lease when the first consumed the drawer (simulate by minting the first, then the second re-check fails); (2) the in-lease refusal terminalises row-non-issued (no server_fiscal_no, seed unchanged). Teeth: revert the in-lease check → pin RED.

## HOLE 1 (MUST) — real fuzzer teeth
**Defect:** `model.rs::apply_sell(is_return)` deliberately does NOT predict INV-21 refusal and mirrors prod's un-guarded arithmetic → prod & model both go −15000 on an empty-drawer RETURN → oracle green even with the guard disabled. `check_cash_on_hand` is wired into ZERO generated sequences (only 2 hand tests).
**Fix (now enabled by Hole 2):** (a) model the INV-21 refusal in the RefModel — when `cash_on_hand < return_cash`, predict **NoMutation + no cash delta** (the RETURN is refused, not issued); (b) call `check_cash_on_hand` **inside `drive_sequence`** after every op (currently it asserts nothing generatively). Now prod (with the in-lease guard from Hole 2, in the fuzzer's lane) refuses exactly when the model refuses → match; **disable the guard → prod mints while model refuses → divergence → RED.**
**★ ACCEPTANCE (prove it, don't claim it):** disable the in-lease guard, run the FULL `invariant_fuzzer` binary `--features test-support` → it must go **RED** on a generated/proptest sequence (not just the static pins). Paste the RED. Then RESTORE + confirm GREEN. Update the oracle/model docstrings to state the teeth is now real (remove the "prediction deliberately not modelled" comment).

## HOLE 3 (SHOULD) — wire reconcile + audit fallback
- `reconcile_opening_anchor` (`cash_ledger.rs:190-231`) is called from NO prod path (Check 16 = no-op comment; `CashAnchorDrift` never constructed). **Wire it into the boot recovery pass** (`invariant_scan` Check 16), guarded to skip the known force-close test seam (the `SellWithClosedShift` helper that would false-positive). On drift → construct `CashAnchorDrift` + audit (journal authoritative).
- `stage_send.rs:1657` `derive_closing_cash(...).unwrap_or(opening_kop)` silently drops the shift's cash movements on a derive error. **`audit_log` the fallback when it fires** (keep the fallback, but make it observable).
**RED pins:** reconcile detects+audits a corrupted anchor via the BOOT path (not just the direct fn); the fallback emits an audit row when derive errors.

## Deliver
Fix commits on the branch, gate `--features test-support` 123+ green (paste tail), clippy `-D warnings` + fmt clean, working tree CLEAN. Update PR #257 body with the 3 fixes + the teeth-proof (RED-with-guard-disabled paste). 7-point report. Do NOT merge — I re-review + merge.
