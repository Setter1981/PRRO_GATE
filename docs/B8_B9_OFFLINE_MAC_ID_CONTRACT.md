# Implementer Contract — B9: offline `<MAC ID>` in the signed XML

**Status:** LOCKED by reviewing architect (2026-07-08, revised after base reconciliation). Strict RED-first TDD. Isolated worktree; architect reviews + merges.

**Base:** **`origin/main` @ `8c3669f`** (NOT the local working-dir HEAD `f86a05a`, which is 2 commits BEHIND origin/main — the operator's checkout has not pulled #247/#248. Agent worktrees branch off origin/main, so you are on the correct base; **verify current state with `git show 8c3669f:<path>` / your worktree, NOT the operator's stale working dir**).

**Priority:** HIGH — last INV-11 legally-usable-offline blocker. B8 is already merged; only the signed `<MAC ID>` remains.

---

## 1. What is already DONE (B8, #248 `8c3669f` — DO NOT redo; verify only)

`#248 feat(b8): opaque DPS offline codes reach the wire on drain` already landed on origin/main:
- `migrations/029_fiscal_documents_offline_dps_code.sql` — **`offline_dps_code TEXT` column on `fiscal_documents` EXISTS.**
- `db/repositories/offline_sessions.rs` — `AcquiredCode { …, pub dps_code: String }` (`:125`); `acquire_code_tx` (`:388`) guards `AND dps_code IS NOT NULL` and RETURNs the opaque `dps_code`.
- `services/write_path/stage_offline_ack.rs:347,364` — acquires the code and stamps `offline_fiscal_no + offline_dps_code + offline_session_id` (**at offline-ack, i.e. AFTER `stage_sign`**).
- `services/write_path/stage_send.rs:471` — `let id_offline = inputs.offline_dps_code.clone().unwrap_or_default();` (**wire `id_offline` = opaque string, DONE**).
- Tests: `tests/b8_acquire_real_first.rs`, `tests/b8_render_id_offline.rs`, `tests/b8_stamp_offline_dps_code.rs`.

**So: the opaque `offline_dps_code` is already a persisted doc column, already surfaced by acquire, already on the wire.** What is missing: it is stamped *after* signing, and the signed XML still emits a bare `<MAC>`.

## 2. What remains (B9)

`xml/mod.rs:826 / :1150 / :1290` all still emit `tag_text(out, "MAC", &h.previous_hash)` → bare `<MAC>{hash}</MAC>` for ALL docs. Live DPS rejects offline drain with **`-9 "not ID in MAC"`** (proven 2026-07-08, `live_smoke_11`). Offline receipts MUST carry `<MAC ID='{offline_dps_code}'>{hash}</MAC>` in the **signed** bytes.

---

## 3. Ground-truth (WebCheck, file:line — resolved, do NOT re-litigate)

1. **The `<MAC ID>` value is the opaque DPS code string** (`fns.checkidfiscal` in WebCheck; `NumbersOfflineUse.cs:97`) = our **`offline_dps_code`** (NOT the integer `offline_fiscal_no`).
2. **Wire `id_offline` == `<MAC ID>`** — the same opaque string (`Client.cs:101-107` vs `SendingOfflineChecks.cs:169`). Our `id_offline` already renders `offline_dps_code` (B8), so `<MAC ID>` must use the SAME value.
3. **ALL offline doc types carry `<MAC ID>`** (SHIFT_OPEN=8, SELL/SMO, Z=80, close=10 via `SaveXMLcheckOffline`→`StringXML.cs:1438`). Bare `<MAC>` ONLY for pure-online (`SendingOfflineChecks.cs:238`). **Branch on `fs_mode` (offline vs online), NOT on doc-type.**

---

## 4. Core decision — ORDERING (Option B, LOCKED)

`<MAC ID>` must be in the **signed** XML, but `offline_dps_code` is currently acquired + stamped at `stage_offline_ack` (post-sign). So for **offline** docs (`fs_mode == 'OFFLINE'`, decided at `stage_acquire`, read back in `stage_sign`), **move the `acquire_code_tx` + stamp of `offline_fiscal_no`/`offline_dps_code` from `stage_offline_ack` into the `stage_sign` pin-tx, BEFORE the canonical XML is built** — so the builder can emit `<MAC ID='{offline_dps_code}'>` into the signed bytes. `stage_offline_ack` then no longer acquires; it only advances `SIGNED → OFFLINE_LOCAL_ACK`.

- Crypto sign stays **outside** the tx (INV-1). The code CAS is one atomic UPDATE inside the tx `stage_sign` already owns; the XML build is pure.
- **Refusal-point shift:** a pool-exhausted offline sign now refuses BEFORE minting signed bytes (pre-SENT reject; D2 pin — non-issued, seed NOT advanced). Preserve the #192/P1 refusal semantics.
- **If Option B surfaces a concrete write-path blocker, STOP and escalate to the architect** — do not silently switch ordering.

---

## 5. RED-first slices (strict TDD — failing test first, watch it fail, minimal green)

**Slice A — ordering reorder (HOT ZONE).**
- RED: after `stage_sign` on an OFFLINE doc, `offline_dps_code` (and `offline_fiscal_no`) are already stamped (currently NULL until offline-ack). **Crash canary:** a `SIGNED` offline doc with a consumed code must resume to `OFFLINE_LOCAL_ACK` (not rest non-terminal, not waste the code — #192/P1 pin); revert-the-guard must FAIL. Pool-exhausted-at-sign → refuse before signing (D2: non-issued, seed not advanced).
- GREEN: move acquire+stamp from `stage_offline_ack` into `stage_sign` pin-tx (offline branch only); `stage_offline_ack` stops acquiring, only advances state. Keep `stage_send.rs:471` `id_offline` working (it reads the same stamped column).

**Slice B — `<MAC ID>` builder.**
- RED: offline canonical XML (SELL/RETURN + SHIFT_OPEN + Z) contains `<MAC ID='{offline_dps_code}'>{hash}</MAC>`; **online** canonical XML stays bare `<MAC>{hash}</MAC>` (proven live — regression-pin it). Chain-consistency: the ID-bearing MAC is in the signed bytes → flows into `unsigned_xml_sha256` → next doc's `previous_hash` (WebCheck does the same — assert the hash covers the ID attribute).
- GREEN: `xml/mod.rs:826/1150/1290` — branch on offline; thread `offline_dps_code` into the builder input (`DocumentHeader`/`CheckPayload`); add attribute support (a `tag_text_attr` helper or inline). Offline → `<MAC ID='{n}'>`, online → bare `<MAC>`.

---

## 6. Invariant guards (state in the PR how each is preserved)

- **INV-1** — crypto sign stays out-of-tx.
- **INV-4** — idempotency: re-running acquire/stamp on an already-stamped doc is a no-op (no double-consume).
- **INV-5** — offline code consumed exactly once (single CAS, just earlier).
- **#192 pin** — no doc rests non-terminal `SIGNED` at a quiescent boundary; boot-resume completes a SIGNED-with-`offline_dps_code` offline doc to `OFFLINE_LOCAL_ACK`.
- **Online unaffected** — `<MAC ID>` branch is offline-only; online bare `<MAC>` (+ empty `id_offline`) proven live — regression-pin, must not change.

---

## 7. Acceptance gate

- **Implementer green-loop:** deterministic nextest pins (slices A+B) + full `cargo nextest run` green. The B8 tests (`b8_*.rs`) must stay green.
- **Final live acceptance (operator-run, NOT your loop):** `live_smoke_11` must now pass `-9` (needs JKS key + test-FN + `PRRO_LIVE_DPS=1`).

---

## 8. Risks / process

- Write-path reorder = high-risk hot zone. Isolated worktree. integration-tester + security-reviewer before architect merge.
- Minimal, vertical diff. No unrelated refactors.
- Branch `feat/b9-offline-mac-id`, base `main`. Draft PR for architect review; **do NOT merge**.
- Delivery message: 7-item project format.
