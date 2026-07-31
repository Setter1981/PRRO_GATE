# Handoff — 2026-07-31: the T=112 live capture, and the `-12` trap it uncovered

`main` tip at handoff: **`893d1079`**. Nine PRs landed today (#348 … #356); no PR open, no
worktree left behind.

> **Read this section first if you read nothing else.** Two claims I published into `RULING 2`
> today were **wrong**, and both were wrong because I verified them against a **stale code
> comment** instead of against the **pins**. They were retracted the same day (#354). If you
> find yourself about to state how `-12` behaves, go read
> `minus_12_bad_hash_prev_records_held_stop_no_second_wire` and
> `rc05_bad_hash_prev_held_stop_mode` — not `error_routing.rs:95`, which still describes a
> mechanism CS-3 S7-1 retired.

---

## 1. What landed

| PR | what |
|---|---|
| #348 | generative `Replenish` (T=112) fuzzer symbol; found the S7-2 fence contract |
| #349 | the stranded rulings/roadmap/dossiers reached `main` for the first time |
| #350 | production comment: the H1/H2 cost contradiction split into hypotheses |
| #351 | **live capture harness** for the ambiguous T=112 + the run's results |
| #352 | CI docs-only fast path — fail-closed, with teeth |
| #353 | `RULING 2` §4 `known-red` discharged |
| #354 | **retraction** of the wrong `-12` mechanism claim |
| #355 | **P1 fix** — guard-B accepts a peer-corroborated seed |
| #356 | fuzzer: pin the REAL `-12` contract, delete the retry machinery |

## 2. The live capture (bd `PRRO_GATE-2ds`, now CLOSED)

`RULING 2` §4 had demanded one real ambiguous T=112 since 2026-07-10. It ran.

**Method** (`rust/prro/tests/live_capture_ambiguous_t112.rs`): a TCP forwarder relays the request
to the DPS **test** cabinet and tears the connection down **on the server's first reply byte** —
so DPS provably processed it and we never heard the answer. A client-side timeout would not do: it
cannot establish the request arrived, which is the entire meaning of "ambiguous". TLS stays
end-to-end (the forwarder never terminates it); the client reaches loopback while validating the
real cabinet certificate via tonic's `domain_name` override, so production needed **no**
test-only seam.

**Results, both previously unknown:**

- **Offline-code ranges do NOT leak.** After the processed-but-lost call, a fresh T=112 returned
  the *same* unconsumed code. A flapping link does not burn a range per break; the code-reserve
  floor (`PRRO_GATE-255`) is not eaten by it. **N=1** — supported, not proven.
- **The `-12` message format is confirmed live**, twice on different hashes. Verbatim, note the
  **two** spaces: `ERROR_BAD_HASH_PREV  store <64 hex> chk <64 hex>`.

Gates: cargo feature `live-dps` + `#[ignore]` + `PRRO_LIVE_DPS=1` + **`PRRO_LIVE_DPS_CAPTURE=1`**
(its own switch, because the run was potentially destructive) + a default-deny TEST-cabinet host
allowlist that **panics** against `prro.tax.gov.ua` — verified, not assumed.

## 3. The P1 trap (bd `PRRO_GATE-3uo`, FIXED in #355)

Chasing the capture's second result uncovered this.

After an ambiguous T=112 the DPS tip moved and ours did not — that arm returns before the persist
envelope, so **no witness** is written and `active_chain_tip` stays stale. The next receipt earns
`-12`, which since S7-1 (R3) is a `MacReseedPending` **HELD**: node `STOP_MODE`, doc resting
`SENDING`, **no automatic second wire**. Only an operator `MacReseed` clears it — and guard-B
required `seed == active_chain_tip`.

Proven by test, not argued:

```
operator supplies Hs (DPS's actual tip)    -> MacReseedSeedMismatch
operator supplies Hp (the stale local tip) -> accepted
```

Guard-B accepted **only the value known to be wrong**. Not a deadlock — a **trap**: the one
permitted action re-installed the stale tip, the next send earned `-12` again, and every turn of
the loop looked like the operator had done something.

**The fix was half the size it first appeared.** `Hs` is not lost: DPS names it in the `store`
field, and `stage_send` records that message durably (`transport_trace`; 160 bytes live against a
512 cap, never truncated). So no new write in the hot path — **one added disjunct** in guard-B:
accept a seed corroborated by the recorded `store`. The #338 hardening is *strengthened*, not
weakened — the seed must now match something the peer actually said, and a seed matching neither
is still refused (asserted permanently in the test).

The test was **inverted, not deleted**, through the supersession registry.

## 4. Open P1s — precise next steps

### `PRRO_GATE-q5u` — the only one that is code work
A deterministic tax-config defect parks a document in `PREPARED` **forever**: `boot_phase`
erases the error type into `anyhow`, returns `Ok(())`, and the next tick re-dispatches identically.
Same class as bug #192, which was P1.

**Design is complete on the bd.** Two facts make it small:
- `stage_sign.rs:148` already documents `SignError::TaxSummary` as *"payload defect — NEVER
  retry"*. The contract is declared on the type; the dispatcher just ignores it.
- `(Prepared, Aborted)` is **already a legal transition** (`fiscal_documents.rs:184`), and #192
  established `Aborted` as the terminal for pre-issuance refusals. No new state, no new edge.

**Do not forget the second site:** the inline path has the same hole — `inline_map.rs:278-290`
maps to `SIGN_INTERNAL` and `inline.rs:862` terminates only the *inbox* row, leaving the document
non-terminal. A boot-only fix leaves inline uncovered.

Deliberately not started at the tail of a long session: it is a document-state transition on a
boot-recovery path and wants RED-first tests with a clear head.

### `PRRO_GATE-6bj` — do NOT implement literally
The ticket asks for a bounded resend on `-3`. CS-3 S7-1 R6 **deliberately removed** auto-redrive to
prevent double issuance. Implementing the AC as written is a regression. The AC needs rewriting
toward a paced reconciliation via `lastChk` — ask DPS what it actually has — not a blind resend.

### `PRRO_GATE-0ps` — two signatures, not a slice
Pin `verAPI` (de-facto `2` at `xml/mod.rs:860` — record it or hard-code it on the wire) and tick
the reconciliation box. `delLastChk` was **split out** to its own P3 by operator decision:
worth supporting for reference completeness, definitely not a pilot blocker, applicability narrow.
Its first step is a **probe, not code** — establish what DPS does to a fiscalised check, and
whether a deletion leaves a hole in the numbering or the MAC chain. Our invariants assume
append-only.

`PRRO_GATE-k54` (TLS CA bundle) was **closed as a signed waiver**: the key material is identical
across contours — only the servers differ — so there is no separate test CA to isolate. The
separation that matters is enforced at the hostname layer by the default-deny allowlist, which was
exercised directly today.

## 5. Fuzzer state, and what is still missing

`-12` now has a pin of its REAL contract (`minus_12_holds_the_node_and_rests_the_doc_sending`:
doc `SENDING`, node `STOP_MODE`, recovery counter stays 0). The stub's `-12` carries the
live-captured `store <hash>` — no consumer today, but the `3uo` fix corroborates against exactly
that field, so a bare-string stub could not exercise it.

**Still open, recorded rather than skipped:**
- `-12` remains in the assertion-free `FaultOrRecovery` bucket — `check_differential` answers it
  with a bare `Ok(())`. That made sense while it was believed non-deterministic; the contract is
  now fully deterministic and belongs in the oracle.
- The **ambiguous-T112 generator leaf** is still off. The evidence blocker is gone, but the model
  has **no remote-tip axis** (verified absent), and without one `Ambiguous` is locally
  indistinguishable from `ServerReject` — the symbol would be *vacuous coverage*, worse than none.

## 6. Environment notes for the next session

- The primary worktree `/home/setter/prro_gate` is still on **`fuzzer-tier1-dossier`**, which is
  fully merged (via #349) and therefore stale. Switch it to `main` before starting.
- CI now takes a **fast path on docs-only PRs**. The required context still always runs and always
  reports; it decides internally. Anything under `rust/`, `.github/`, `scripts/`, `Cargo.*`,
  `.sqlx/` or **`docs/cs1r/`** forces the full leg — `docs/cs1r/` is the subtle one, the CS-1
  manifests live there.
- After any PR that used the supersession registry merges, **prune the row** — a merged removal is
  no longer a removal-vs-base and reads STALE (the gate says so out loud).
- `maria304_driver::listener::cooldown::tests::restart_extends_cooldown` is **load-sensitive**: it
  can fail under a full parallel suite and passes 3/3 in isolation. Filed; not a regression.
- `cargo clippy --all-features` fails on a pre-existing lint in `prro_crypto` that CI does not
  gate. Filed; unrelated to any current work.

## 7. Next per the roadmap

**CS-4** — author spec #6 and implement the thin per-FN coordinator, routing exactly one command
through it. Worth doing `q5u` first: leaving a P1 whose failure mode is *a halted register* open
while starting an architectural slice is the wrong order.
