# External review request — RS-2 ingress-server PLAN, round 2 (fresh eyes)

You are a senior Rust / fiscal-systems reviewer with **local read access** to this repo at `/mnt/d/PRRO_GATE`. This is a **read-only architecture review of a PLAN document** (no code exists yet for this worklet). **Do not edit files. Do not run builds.** Use `git -C /mnt/d/PRRO_GATE ...`, `rg`, and direct file reads.

## What you're reviewing

The plan: **`docs/superpowers/plans/2026-06-05-rs2-ingress-server.md`** — read it whole, but **§0 (REVISION 2) is the authoritative, implementation-ready basis** and supersedes §1/§3.3/§4/§5/§9/§10 where they conflict. §0.1 = corrections from round-1 review; §0.2 = four operator decisions now RESOLVED; §0.3 = corrected piece decomposition.

**This is round 2.** Round 1 already found 5 High + 4 Medium structural issues (all now folded into §0). **Do NOT re-report the round-1 findings as new.** Your job is the opposite of round 1:
1. **Verify round-1 is actually resolved** — does §0.1 correctly + completely close each finding against the real code? Any that are mis-resolved or only partially closed?
2. **Find what Revision 2 introduced or still misses** — new inconsistencies created by the §0 rewrite, or gaps neither round caught.
3. **Stress the 4 resolved decisions (§0.2 Q1–Q4)** for implementability against the actual code.

## Domain context (you have no shared context — read this)

This is a **local PRRO fiscal gateway** for Ukraine (Rust crate `rust/prro`). It serves **tens of fiscal_numbers (FNs)** concurrently — ~50 sites / ~70 cash registers, each register = its own FN. RS-1 (the runtime "maintenance arm": boot reconcile + offline-drain + return-online probe loops + graceful shutdown supervisor) just merged. **RS-2 is the ingress arm**: the HTTP server that accepts FRESH receipts from front-ends (`maria304_driver` today; a WebCheck COM-shim at pilot), converts wire→canonical, writes the inbox idempotently, and drives fiscalization **inline-synchronously** (the POST blocks until the receipt is signed+sent to DPS or offline-local-acked, returning `fiscal_id`). **RS-3** (separate later worklet) fills the actual write-path front-half; RS-2 calls a **seam** that returns a typed `NotImplemented` until then (no silent success). Frozen invariant #1: **no network/crypto inside a long SQLite write transaction** — the inbox insert is a short RESERVED-tx; sign/DPS happen outside it.

## Anchors to verify against (cited by the plan — confirm they say what §0 claims)

- Listener config model: `rust/prro/src/config/mod.rs:29` (`listeners: Vec<ListenerCfg>`), `:59-65` (fields), `:79` (`ListenerKind::RestHttp`); `rust/prro/tests/listener_config_parse.rs`.
- Wire DTO + mapping + gap: `rust/prro/src/runtime/ingress/dto.rs` (`:67` `CanonicalCommand`, `:80` `CanonicalResponse`, `:241/:360` mappers, `:310` hash-over-wire, gap doc `:11-56`).
- Signer target shapes + supported doc surface: `rust/prro/src/services/write_path/stage_sign.rs:799/805/920/941` (CheckJson/ZReportJson/ShiftOpenJson/parse_payload), `:137` (`derive_wire_artifact_kind` — signable surface), `:1098` (test-support seam).
- Inbox repo + FSM: `rust/prro/src/db/repositories/ingress_inbox.rs:44/65/101` (NewInboxEntry/insert/Replay), `:219` (`acquire_lease` NEW→PROCESSING), `:275/303` (mark_rejected).
- Payment forms: `rust/prro/src/db/repositories/payment_methods.rs` (per-FN `{pay_index,name,iscash}`, `<M T> = pay_index-1`, "pay_index=1 Готівка default").
- Supervisor (RS-1 F1 seam to be refactored): `rust/prro/src/runtime/supervisor.rs:216-285` (`supervise_until_shutdown`, `Wake`), `:325` (`join_both_with_grace`).
- Protocol enum: `rust/prro/src/db/models/enums.rs:84`.
- Legal: `docs/LEGAL_INVARIANTS.md:195` (X_REPORT read-only).
- Reconcile basis: `docs/architecture/2026-05-30-runtime-spine-connection-blueprint.md`, `docs/architecture/2026-05-30-webcheck-shim-ingress-spec.md`.

## Specific scrutiny targets (where my resolutions could be wrong — look hard here)

1. **Status endpoint (Q1) — does the data exist to serve it read-only?** §0.2 Q1 promises node mode + shift state + last local/fiscal number + offline counters/code-pool over **pure reads, zero write-path side effects**. Verify each of those fields is actually readable from an existing repo/table without a write or a lease. Note: shift state may be a read-projection of `node_state` (a recent arch decision) — is it queryable today, or does it depend on RS-3/WL-1 work not yet present? Flag anything that isn't readable now.

2. **Payment mapping (Q3) — is the fixed `PaymentKind → candidate pay_index` safe when `payment_methods` is per-FN configurable?** §0.2 Q3 maps `CASH→1, CASHLESS_1→2, …` then looks up the per-FN table by that index for name/validity (`type_code = pay_index-1`). If an FN can configure CASH at a different `pay_index`, the fixed candidate index is wrong. Check the `payment_methods` bootstrap/seed + any uniqueness on `iscash`/`name` — should the lookup key be `iscash`/`name` rather than a hardcoded index? This is fiscal-correctness; be precise.

3. **Hash-over-converted (High-5 resolution) — does any downstream consumer rely on `payload_sha256_canonical` being the WIRE hash?** §0.1 changes the persisted hash to the converted (signer-ready) shape. Check `mac_recovery`, the W3 parity tests (`tests/ingress_dto_parity.rs`), audit, and any reconcile path for an assumption that the stored hash equals `Sha256(wire cmd.payload)`. If anything breaks, say so.

4. **Replay matrix (High-2 resolution) — races + completeness.** §0.1 reads `fiscal_documents` on inbox `DONE` to build a truthful response. Is there a window where inbox is `DONE` but the fiscal doc row isn't yet visible (or vice-versa)? Is the `PROCESSING`-while-first-request-in-flight branch deterministic under the inline-sync model (one register = one FN = one listener = sequential — confirm that actually holds, or whether two TCP connections to one listener can race the same `idem_key`)?

5. **Named-handle supervisor refactor (Med-9) — does generalizing break RS-1's F1 guarantees?** §0.1/§0.3-piece-5 refactors `supervise_until_shutdown` from 2 positional handles to a named set, adding N per-listener servers. Assess whether the biased-select-shutdown-wins semantics, the one-shared-grace join (not N× grace), and the `SUPERVISOR_LOOP_DIED` loop-death→Err→restart path survive the generalization. Specifically: an axum `with_graceful_shutdown` task that returns `Ok(())` after the watch flip must read as NORMAL, while an axum bind/serve `Err` must read as loop-death — is that distinguishable in the proposed model?

6. **Auth middleware (Q2) — feasibility.** Loopback→no token, non-loopback→bearer from env/secret-ref. Is there an existing env/secret-reference mechanism in the codebase to resolve the token (don't invent one)? Does the `ListenerCfg` carry enough to know the bind address class at server-spawn time?

7. **Inline + multi-FN latency.** One ingress server per listener (per FN). Under inline-sync, a slow DPS blocks that listener's request. Is there any shared lock/pool that lets one slow FN's inline call starve others? (RS-3 owns the timeout, but flag any RS-2 structural coupling.)

## Output format

- **Verdict:** `PLAN-READY` (start coding piece-1/2) or `REVISE` (must fix before code).
- **Findings** grouped High / Medium / Low, each with: the claim, the **code `file:line` + plan `§/line`** anchors, why it's wrong/risky, and a concrete recommendation.
- Explicitly note **which round-1 findings you re-verified as correctly resolved** (so we have convergence signal), and **which (if any) are mis-resolved**.
- If you find nothing new and round-1 is fully resolved, say `PLAN-READY` plainly — convergence (two reviewers, no new High) is the goal.
- State that you did not edit files or run builds.
