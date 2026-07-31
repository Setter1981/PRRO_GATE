# SPEC — the peer-tip axis: making `-12` a CONSEQUENCE instead of a script leaf

**Date:** 2026-07-31
**Status:** DRAFT — design only, no code written yet.
**Unblocks:** `bd PRRO_GATE-2ds` (the ambiguous-T112 generator leaf), and `PRRO_GATE-5hc` (the
MacRecovery **success** path, which is generatively untested for the same underlying reason).
**Origin:** the 2026-07-31 handoff recorded the leaf as blocked because "the model has no remote-tip
axis, and without one `Ambiguous` is locally indistinguishable from `ServerReject` — the symbol would
be vacuous coverage, worse than none."

---

## 1. What is actually missing

Not a symbol. **The fuzzer does not model the second party at all.**

Today a DPS reply is *dictated* by `DpsScript`: the generator picks `[BadHashPrev]` and the stub
returns `-12`, regardless of whether the chain actually diverged. So the oracle checks **our reaction
to `-12`** but never checks **that `-12` arises exactly when it should**. The interesting half of the
contract — *the peer disagrees with us about the chain* — is unmodelled.

That gap is why three things are stuck at once:

| stuck thing | why the peer-tip axis is the blocker |
|---|---|
| ambiguous T=112 (`2ds`) | the whole point is "DPS moved its tip, we never heard" — unrepresentable without a peer tip |
| `-12` fidelity | today a free script leaf; should be a **derived** outcome of tip divergence |
| MacRecovery **success** (`5hc`) | an operator `MacReseed` succeeds only when the supplied seed matches what the peer actually has — no peer, no success path |

And the class is not hypothetical: `PRRO_GATE-3uo` was a **P1 trap** living in exactly this seam —
after an ambiguous T=112 the peer's tip moved, ours did not, guard-B then accepted *only the value
known to be wrong*, and every turn of the loop looked like a successful operator action.

## 2. Constraints discovered while designing (checked, not assumed)

1. **The stub cannot read the chain off the wire.** `CheckEnvelope` (`transports/dps/dto.rs:32-53`)
   carries `rro_fn`, `date_time`, `check_sign`, `local_number`, `check_type`, `id_offline`,
   `id_cancel` — no XML, no `previous_hash`. And `DetCrypto::sign_cms_detached`
   (`tests/common/mod.rs:244-249`) returns the constant `b"RECOVERED-CMS"`, so `check_sign` carries
   no information at all. **A peer that validates the hash "as the real DPS does" is not
   constructible without changing the crypto stub**, and changing it would ripple through every test
   that pins those bytes.
2. **The stub is per-operation and stateless across ops.** `FuzzCtx::new_dps` (`interp.rs:530`)
   builds a fresh `ScriptedDps` for every op; only the call counters survive, via `Arc`
   (`send_calls` / `last_calls`). So peer state must live in `FuzzCtx` and be handed in the same way
   — the pattern already exists.
3. **The information the peer needs IS in the DB.** Production persists `previous_hash` on the row at
   pin time and `unsigned_xml_sha256` at persist time. Those are exactly "what went on the wire" and
   "what the peer's tip becomes if it accepts".

## 3. Options, and the one to take

**(a) Fully truthful peer** — stub reconstructs the chain from the envelope. **Rejected:** not
constructible per constraint 1, and buying it means editing `DetCrypto`'s pinned bytes.

**(b) Model-only axis** — the model tracks a peer tip; the generator is constrained to emit scripts
consistent with it. **Rejected as the primary mechanism:** consistency would be enforced by the
generator, i.e. by the same side that predicts. The oracle would be checking the generator's
arithmetic, not production's behaviour.

**(c) Peer state in the harness, fed from the REAL ledger — RECOMMENDED.**
`FuzzCtx` owns `PeerState { tip: Option<[u8;32]> }`. On a send, the stub resolves the outgoing
document's `previous_hash` **from the real DB** (by `local_number` / `request_id`) and compares it to
`peer.tip`:

- **match** → the script executes as written; on an accepting leaf the peer tip advances to that
  document's `unsigned_xml_sha256`;
- **mismatch** → the stub returns `-12` with the LIVE-captured shape
  `ERROR_BAD_HASH_PREV  store <peer.tip> chk <doc.previous_hash>` — **overriding the script**.

Independence is preserved: the peer is part of the ENVIRONMENT (it derives its answer from what
production actually wrote), while the model predicts production's REACTION from its own independent
state. They are not the same source, so a divergence still REDs.

> Note on `store`: the `3uo` fix corroborates an operator-supplied seed against the `store` field
> recorded in `transport_trace`. Feeding the peer's real tip into that field is what finally makes the
> MacReseed **success** path (`5hc`) generatively reachable — today the stub's `store` is a constant.

## 4. The axis contract — who moves the peer tip

This table is the load-bearing part. A partial implementation (some movers wired, some not) is
**worse than none**: the model would then lie confidently, which is precisely the failure mode the
`-12` fault bucket had.

| event | our seed | peer tip | note |
|---|---|---|---|
| online issuance accepted (`Sending → Sent`) | advances | advances | advance-at-SEND; the two stay equal |
| online **pre-SENT** reject (`Rejected`) | unchanged | unchanged | D2: lnd consumed, no seed advance |
| online **HELD** (ambiguous / transient after CALL_STARTED) | unchanged | **INDETERMINATE** | the peer may or may not have taken it — see §5 |
| offline issuance (`OFFLINE_LOCAL_ACK`) | advances | unchanged | the peer learns only at drain |
| drain accepted | unchanged (already advanced) | advances | |
| T=112 granted (reply received) | advances (non-doc seed) | advances | live-proven: T=112 moves the chain |
| **T=112 ambiguous (reply lost)** | **unchanged** | **advances** | ← the divergence this whole slice exists for |
| operator `MacReseed(seed)` | set to `seed` | unchanged | succeeds iff corroborated by the peer |

**Derived invariant:** whenever `our_seed != peer_tip`, the next online send MUST earn `-12`. That is
the statement worth pinning, and it is unprovable today.

## 5. The honest hard part

The HELD row above is not a detail — it is the whole difficulty. "Ambiguous" means *we do not know*
whether the peer took it. A single scalar `peer.tip` cannot express "moved or not, unknown to us",
and collapsing it to a guess (always-moved / never-moved) would make the model assert something the
system genuinely cannot know.

Two candidate shapes, to be decided BEFORE any code:

- **Peer is definite, we are ignorant.** `peer.tip` is always a concrete value (the peer really did
  or did not accept — the harness chooses which when generating the ambiguity), and the *uncertainty
  lives on our side*: the model must predict that production HOLDS and does not assume either way.
  Closer to reality; the generator picks the branch, so both are exercised.
- **Peer tip is a set** of possible values until resolved. More faithful to the epistemics, much
  heavier, and probably unnecessary — production never inspects the peer tip directly, it only
  observes the reply to the *next* send.

**Recommendation: the first.** It keeps the axis a scalar, exercises both worlds, and puts the
uncertainty exactly where production carries it.

## 6. Implementation order (each step independently green)

1. **Axis, inert.** Add `PeerState` to `FuzzCtx` + the model field. Wire the movers for the
   *agreeing* cases only (accept → both advance). Nothing changes behaviourally; the whole existing
   suite must stay green. **This step is the load test of §4**: if any existing script starts earning
   a `-12`, our understanding of who moves what is wrong, and it is better to learn it here.
2. **Peer override for `-12`.** The stub returns `-12` on mismatch, overriding the script. Now the
   derived invariant of §4 is enforceable. Existing tests should be unaffected (they never diverge);
   any that break are findings, not noise.
3. **Ambiguous T=112 leaf.** Generator symbol: the peer grants and advances, the reply is lost. The
   model predicts our seed unchanged, peer advanced. The follow-on send then earns `-12` *derivedly*.
4. **MacReseed success path (`5hc`).** With a real peer tip in `store`, the corroborated-seed branch
   becomes reachable; pin both halves (correct seed accepted, foreign seed refused) as `3uo` did
   directionally.

## 7. Teeth plan (per step, revert-canary, empirical)

- step 1 — flip one mover the wrong way (e.g. offline issuance advances the peer tip): the agreeing
  suite must RED.
- step 2 — make the peer ignore the mismatch: the derived-`-12` pin must RED.
- step 3 — make the ambiguous leaf advance OUR seed too: the divergence pin must RED.
- step 4 — accept an uncorroborated seed: the `3uo` half must RED.

Each canary must be run and its RED output recorded, per the standing rule that teeth are proven
empirically, not asserted.

## 8. Cost, stated plainly

This is a **slice, not a follow-up**: a new state axis touched by seven movers, a stub that stops
being purely scripted, and a generator symbol. The §4 table is where it is won or lost. Steps 1-2
carry the risk (they can invalidate assumptions about who advances what); steps 3-4 are then small.

Also note what this does NOT buy: it does not model a Byzantine peer (a DPS that lies or contradicts
itself) — that stays in the existing backlog item, and nothing here assumes the peer is honest beyond
"it applies our own advance rules".
