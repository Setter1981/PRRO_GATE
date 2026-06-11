# External review request — RS-2 ingress-server PLAN, round 3 (convergence gate)

You are a senior Rust / fiscal-systems reviewer with **local read access** to `/mnt/d/PRRO_GATE`. **Read-only review of a PLAN** (no code for this worklet yet). **Do not edit files or run builds.** Use `git -C /mnt/d/PRRO_GATE ...`, `rg`, file reads.

## What you're reviewing
**`docs/superpowers/plans/2026-06-05-rs2-ingress-server.md`** — **§0 (0.1 + 0.4) is the SOLE implementation basis**; §1–§11 are historical (contain superseded statements — do not implement from them).

**This is round 3, a CONVERGENCE GATE.** The plan already absorbed round-1 (5 High + 4 Med, external) into §0.1 and round-2 (1 external + 3 internal → 6 High + 6 Med) into §0.4 (REVISION 3). Two operator decisions are now RESOLVED in §0.4: **D1 = freeze payment slots**, **D2 = loopback-only pilot**. **Do NOT re-litigate closed findings.** Confirm convergence or find a residual blocker.

## Domain context (you have no shared context)
Local PRRO fiscal gateway, Rust crate `rust/prro`, serving TENS of FNs. RS-2 = the first HTTP ingress server: JSON `CanonicalCommand` in → FN-validate + driver_id stamp (from the per-listener `ListenerCfg`) → wire→signer-ready conversion (closes the `dto.rs:11-56` shape gap; ZReport needs ledger reads) → idempotent inbox insert (short RESERVED-tx) → **inline-synchronous** write-path SEAM (typed `NotImplemented` until the later RS-3 worklet; no silent success) → typed `CanonicalResponse`. RS-1 (a runtime supervisor with a graceful-shutdown + loop-death-supervision "F1" contract) just merged; RS-2 adds N per-listener axum servers into it. Invariant #1: no net/crypto in a long SQLite write-tx.

## Task A — did Revision 3 faithfully fold round-2?
§0.4 lists H1–H6 + M1–M6 as the round-2 fixes. For each, confirm the WRITTEN resolution is faithful to the code, implementer-actionable, and not contradicting §0.1. Flag any that are hand-wavy or internally inconsistent.

## Task B — do the D1/D2 RESOLUTIONS introduce NEW issues? (main scrutiny — these are new + unreviewed)
1. **D1 admin-guard.** "Admin must NOT mutate protected slot semantics for RS-2-enabled FNs." How does the admin layer know an FN is RS-2-enabled (= has a `RestHttp` listener in `config.listeners`)? Is config in scope at the admin mutation site (`rg payment_methods rust/prro/src/admin*.rs`, the W4-Z0 admin CLI)? Clean enforcement point, or does it force threading config into admin (scope risk)? Does the existing seed (`DEFAULT_PAYMENT_METHODS`, bootstrap) actually place CASH at pay_index 1 for every FN, so "freeze" matches reality?
2. **D1 validation site.** Where does the per-RS-2-FN payment-slot startup validation run, and does that site have BOTH the config listeners AND the payment_methods pool in scope (e.g. `runtime/supervisor.rs` `run`/`run_with_registry`, near `BindingsRegistry::build_from_db`)?
3. **D2 loopback classification.** `listen_addr` default `127.0.0.1`, refuse non-loopback. Is loopback-vs-not robustly classifiable for `::1`, `localhost`, and especially `0.0.0.0` (all-interfaces — the dangerous case that must be REFUSED)? Does the plan pin parsing `listen_addr → std::net::IpAddr` + `is_loopback()` (rejecting bare hostnames and `0.0.0.0`), or is the guard under-specified?
4. **D2 guard site.** Where does "startup refuses a non-loopback RS-2 listener" run, and is it fail-closed BEFORE any bind?

## Task C — final residuals
Any contradiction between D1/D2 and §0.4 H1/H2 (which they resolve), or between the piece-decomposition delta and the resolutions; confirm the §0 intro is PLAN-READY (not "do not start coding").

## Output
- **Verdict:** PLAN-READY or REVISE.
- **Task A:** per-finding {faithful | under-specified | contradictory}.
- **Task B:** the four D1/D2 points — code `file:line` + plan `§` anchors, finding, concrete fix.
- **Task C:** residuals or "none".
- If no new High and round-2 is faithfully folded, say `PLAN-READY` plainly (convergence = two round-3 reviewers, no new High).
- State you did not edit files or run builds.
