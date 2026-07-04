# WebCheck Phase-1 U2 — Sanitized corpus + sanitizer design (Phase-1 design note)

**Date:** 2026-07-04
**Status:** **DESIGN — for CP1 hard-gate review (architect).** Phase 1 = design only; the ONLY committable
artifact is this note (contains ZERO real data). No data-derived commit until CP1 sign-off. Phase 2 (export →
sanitize → generate → scan → commit corpus) runs only after approval.
**Authoritative:** parent spec `2026-07-02-webcheck-ground-truth-phase1-design.md` §3/U2 + §5/A2 + §7/CP1
(LOCKED v2.1). U0 `docs/webcheck_reverse/WEBCHECK_GROUND_TRUTH.md` — the SOLE source of WebCheck citations
(lnd-translation = `ORDER BY ksef.ID ASC`; offline-lifecycle codes; `_TS.db` = test-cabinet mirror → EXCLUDED;
A07-ruling on `fns` consumption).
**Inputs read (this note):** parent spec; U0 GROUND_TRUTH; `scripts/export_webcheck_samples.py`;
`golden/webcheck_sell_online/` (fixture-format precedent); dump **aggregates only** (`sqlite3 -readonly`,
`COUNT`/`DISTINCT` — no row content).

> **Data discipline (this note obeys):** zero real data — no real FN/TIN/amounts/hashes; FNs referenced by
> anonymized ROLE, never by number; coverage justified by shape-class, not per-FN identifiers. Dumps were read
> read-only, aggregates only. Nothing from `~/webcheck_dumps/` transited the repo.

---

## §1 Pipeline (four stages; the middle two are NEW U2 code)

```
[~/webcheck_dumps/<FN>.db]  ──(1) EXPORT (fixed, --output-dir REQUIRED, outside-tree)──▶  ~/webcheck_dumps/exports/<run>/  (raw, SENSITIVE)
                            ──(2) SANITIZE (NEW): extract SHAPE only, discard all content, gen synthetic ──▶  synthetic fixtures (outside-tree)
                            ──(3) SCAN (NEW): mechanical leak-scan; CP1 human review ON TOP ──────────────▶  pass/fail
                            ──(4) COMMIT corpus ──▶  rust/prro/tests/webcheck_corpus/  (synthetic, hash-recomputed)
```

- Stage 1 (exporter) is **read-only** against dumps (snapshot-copy + `mode=ro`) and already exists — U2 only
  fixes its output path (§5). Its output **contains real data** (`checkxml` raw XML, `mac`, `sum`, `dt`, FN,
  goods names, `signedanswerfromficscal`) and MUST stay outside the tree.
- Stage 2 (sanitizer, NEW) is the load-bearing privacy boundary: it reads the raw export, keeps **only the
  shape** (§3), and emits **fully synthetic** `CanonicalFiscalCommand` fixtures (§7) with **recomputed hashes**
  (§4). Real content never crosses into a fixture.
- Stage 3 (scanner, NEW) is a mechanical leak-gate over the corpus dir (§6); CP1 review sits on top.
- Stage 4 commits the synthetic corpus.

---

## §2 Sanitization INVARIANTS — what provably does NOT survive (parent §3/U2)

The committed corpus must provably contain NONE of the following. For each: the export field it appears in,
and HOW the sanitizer erases/replaces it. **The sanitizer is allow-list by construction** — it emits ONLY the
synthetic fields of §7; it never copies an export field through. This table is the audit of what it discards.

| Real datum | Appears in export as | Erasure / replacement |
|---|---|---|
| Real FN / RRO fiscal number | `fiscal_number`, `checkidficscal`, `SHIFTS.RRO*` | **Discarded.** Fixture FN = a synthetic constant (`FN9000000001` style, per-corpus, not derived from any real FN). |
| Real TIN / ЄДРПОУ / company / point names | `SHIFTS.TAXTIN/ONAME/TAXNAME`, `TAXOBJECTS`, receipt org fields | Discarded; fixtures carry a fixed synthetic org (`cashier-01`, `POS-01`, no org identifiers). |
| Operator / cashier identifiers | `OPERATORS`, receipt `cashier` | Discarded; synthetic `cashier-NN`. |
| Customer identifiers | receipt customer/payment owner fields | Discarded (never emitted). |
| Item / goods names, UKTZED, barcodes | `checkxml`, `CHECKBODY`, receipt `goods[].name/uktzed/barcode` | Discarded; synthetic goods (`"item-1"`, `code:"SYNTH"`, `name:"Synthetic good"`). |
| Raw WebCheck XML | `checkxml` (export line 461) | **Never read into a fixture.** The sanitizer reads only `DocType`/`offline`/`dt`-ORDER/`ID`; the XML text is not parsed into output. |
| Real timestamps | `dt`, `SHIFTS.DATEBEG/DATEEND`, `Sessions.SessionStartDT` | Replaced by **synthetic time-buckets** — a deterministic monotone synthetic clock (`2026-01-01T00:00:00Z + n·bucket`) keyed on sequence position, NOT the real value. |
| Real amounts / sums | `sum`, receipt `price/quantity/amount/totals` | Replaced by **synthetic amounts** (fixed `5000` kop unit, or a deterministic synthetic per item), never the real figure. |
| Real offline UUIDs / offline fiscal numbers | `fns.checkidfiscal`, offline-doc `checkidficscal` | Discarded; the offline-code **consumption COUNT/pattern** is kept (§3), the UUID value is regenerated synthetic. |
| ANY real hash / MAC | `ksef.mac`, export `source.sha256`, `checksigned` | **Never emitted.** Every hash in a fixture is recomputed over synthetic bytes (§4). |
| DPS response blobs | `signedanswerfromficscal`, `checksigned` | Discarded (never read into a fixture). |
| Source provenance paths | export `source.xml_path/db_path`, `_source_snapshots` | Discarded; fixtures carry no path back to a dump. |

**Consequence:** a fixture is a synthetic `CanonicalFiscalCommand` sequence whose ONLY inheritance from the dump
is the **abstract shape** of §3. There is no field-level copy-through to leak.

---

## §3 What IS preserved — the SHAPE (parent §3/U2; grounded in U0)

Only these abstract, non-identifying shape features cross from dump to fixture:

1. **Op-type sequence** — the ordered stream of operation classes, derived from `ksef.DocType` per U0
   (`DocType 8`=SHIFT_OPEN, `80`=SHIFT_CLOSE, `9`=type-9/offline-control, sells=`0/1/3/4`; `10/12` mapped to
   their canonical op or dropped if out of the Phase-1 alphabet — see coverage §8). Mapped to our op alphabet
   {SHIFT_OPEN, SELL, SHIFT_CLOSE, offline SELL, DRAIN, Z(exported-not-replayed)}.
2. **Shift boundaries** — where SHIFT_OPEN/SHIFT_CLOSE fall in the stream (the `shiftid` grouping), so a fixture
   reproduces a real shift-run structure.
3. **Offline-code consumption pattern** — per U0/A07: WebCheck consumes an offline number at offline-ISSUE
   (`fnsupdate10`, INSERT `offline=2`); the sanitizer keeps only the COUNT and POSITION of offline issues in a
   session (how many offline sells, where the drain falls) — NOT the code values. Maps to our
   `offline_codes` consumption count.
4. **lnd-translation** — per U0 §4: the per-FN document order is `ORDER BY ksef.ID ASC`; the sanitizer assigns a
   **synthetic dense per-FN `lnd` = 1,2,3,…** over the exported doc subset in `ksef.ID` order (WebCheck's own
   per-shift `localchecknumber` is discarded, per U0 HIGH#1).
5. **offline-lifecycle codes** — per U0 §2: the `offline ∈ {-1,0,1,2,3}` transition CLASS of each doc (online /
   offline-issued-pending / drained / transitional / cancelled) informs the fixture's expected state-class
   sequence — as an abstract class label, never the raw stored value.

No amounts, names, times, ids, hashes, or XML are shape features — those are all §2-discarded.

---

## §4 Hash-recompute design (parent §3/U2, locked)

- Fixture payloads are **synthetic by construction** (§2/§7). Each fixture's `payload_json` is the canonical
  serialization of the synthetic payload (same canonicalizer the golden `webcheck_*` fixtures use).
- Each expected hash/chain value is **recomputed over the synthetic bytes**: `payload_sha256 =
  sha256(payload_json)` (matching the golden format — see `golden/webcheck_sell_online/…json`
  `payload_sha256`), and any MAC-chain observable (`previous_hash` / seed continuity) is computed over the
  synthetic per-fixture hashes, never a real `ksef.mac`.
- **Corpus CI assertion (A2):** a Rust test asserts, for EVERY fixture, `expected_hash == sha256(payload_json)`
  (byte-exact over the committed synthetic payload). A perturbed payload → hash mismatch → FAIL (RED-tooth §9).
- This keeps U3's O3 recompute-integrity check meaningful (the fixture's stored hash is verifiable, not a real
  fingerprint).

---

## §5 Exporter fix (parent §3/U2 re-check F7; scripts/, CP4-allowed)

**Current (violates never-transit):** `scripts/export_webcheck_samples.py:142` —
`output_dir = args.output_dir or Path("var") / "webcheck_samples" / f"webcheck_export_{now}"` — falls back to an
**in-tree** default when `--output-dir` is omitted (`add_argument("--output-dir", default=None)`, help says
"Defaults to var/webcheck_samples/…").

**Fix (design):**
1. Make `--output-dir` **REQUIRED** (`required=True`; drop the `default=None` help text about var/…).
2. **Remove the in-tree fallback** at line 142 (no `or Path("var")/…`); `output_dir = args.output_dir`.
3. **Guard:** refuse to run if the resolved `output_dir` is inside the repo tree (resolve + check it is not
   under the git worktree root) — a belt to stop an operator pointing it back in-tree. Emit a clear error.
4. Update the module docstring (lines 4–14) to state output is mandatory + outside-tree.

**Verification (data-free, can run in Phase 1 if approved):** a small unit/CLI test that (a) `--output-dir`
omitted → argparse error (exit 2); (b) `--output-dir` inside the tree → guard error. No dump access needed.
This exporter fix is data-free (no real data) — CP1 may authorize landing it as an early standalone `scripts/`
commit, or fold it into Phase 2.

---

## §6 Mechanical scanner (parent §3/U2 audit C5; pre-commit + CI)

A corpus-dir scanner (Rust test in `rust/prro/tests/` + a `scripts/` pre-commit hook sharing the pattern spec)
that FAILS if the committed corpus contains any non-synthetic marker. Pattern spec:

| Check | Pattern (fail if matched in a fixture) | Rationale |
|---|---|---|
| Real FN shape | a 10-digit numeric FN NOT equal to the corpus's synthetic constant(s) | dump FNs are 10-digit; synthetic uses a reserved prefix (e.g. `FN9…`) |
| TIN / ЄДРПОУ shape | 8–10 digit standalone TIN-like token outside allow-listed synthetic ids | catches leaked tax numbers |
| UUID / offline-UUID shape | any RFC-4122 UUID or WebCheck offline-id pattern not on the synthetic allow-list | catches leaked offline UUIDs |
| Real timestamp range | any ISO/`dt` timestamp outside the synthetic bucket window (`2026-01-01…` synthetic epoch) | real dumps predate the synthetic epoch |
| Raw-XML markers | `<?xml`, `<RQ`, `<DAT`, `mmmaaaccc`, `<CHECK`, `windows-1251` | catches any leaked WebCheck XML |
| Non-synthetic hash | any 64-hex string whose value ≠ `sha256(that fixture's payload_json)` | a hash that isn't a recompute is a real fingerprint |
| DPS-blob markers | `signedanswerfromficscal`, `checksigned`, base64 blobs over N bytes | catches DPS response leakage |
| In-tree export dir | `var/webcheck_samples/` exists in the working tree | F7 belt |
| Cyrillic org/name leak | non-allow-listed Cyrillic tokens (synthetic strings are ASCII) | catches org/goods names |

- The allow-list = the exact synthetic constants the sanitizer emits (FN, cashier, good name, epoch). Anything
  outside = fail.
- **CP1 human review sits ON TOP** of the scanner (parent §7/CP1): the mechanical gate is necessary, not
  sufficient — the architect reviews the sanitizer + a sample fixture before any data-derived commit.
- **CI-leakage safe (audit C4):** fixtures are synthetic by construction, so failure logs/artifacts are safe
  *because* the scanner enforces it.

---

## §7 Fixture format + layout

- **Layout:** `rust/prro/tests/webcheck_corpus/<shape-name>/` — one dir per shape fixture, mirroring the
  `golden/webcheck_*` precedent. Names describe the SHAPE, never a FN: e.g. `online_sell_run/`,
  `offline_session_drain/`.
- **Per-fixture files** (golden-style):
  - `sequence.json` — the ordered list of synthetic `CanonicalFiscalCommand`s (each like
    `golden/webcheck_sell_online/expected_canonical_command.json`: `schema_version`, `operation_type`,
    synthetic `fiscal_number`, synthetic `payload`, `payload_json`, `payload_sha256`), PLUS per-op expected
    observables.
  - `expected_observables.json` — the invariant-level expectations U3 diffs: per-FN `lnd` sequence (dense,
    §3.4), state-class sequence, offline-code consumption counts, chain over the recomputed hashes, issued-set
    membership.
  - `SHAPE.md` — a one-paragraph provenance-free description (which shape, which U0 table rows it exercises;
    **no FN, no counts that identify**).
- **U3 consumption (parent §3/U3):** U3's harness loads these as golden `CanonicalFiscalCommand` JSON and drives
  `inline::run` directly, with ScriptedDps from abstract response classes (`Ack`/`Reject`/…) derived from each
  op's outcome CLASS — never a raw WebCheck response blob. The corpus provides the sequence + the abstract
  outcome class per op (from the U0 offline-lifecycle class), not DPS bytes.

---

## §8 Coverage plan (parent §5/A2; aggregate-justified, FN-anonymized)

A2 minimum = {**online sell run**, **offline session + drain**}; **Z exported-but-NOT-replayed** (parent MED#8 —
there is no Z op in the alphabet, inline fail-closes Z pre-fiscal).

**Selection method (from read-only aggregates — `COUNT` by `DocType`/`offline`/shift per FN, no row content):**
- **online_sell_run** ← a **low-volume** production FN whose ksef is dominated by online sells (`offline=0`) with
  a clean SHIFT_OPEN→sells→SHIFT_CLOSE structure — chosen for a *compact* fixture (a single shift's worth, not
  the whole FN; bounded via the exporter `--limit`).
- **offline_session_drain** ← a FN with a **high offline-issue ratio** (a visible burst of `offline=1`
  drained-offline docs within a session) — chosen so one contiguous offline session + its drain is
  representable. Offline codes are present in every production FN's `fns` table.
- **(optional) cancelled edge** ← a FN that shows a resting `offline=-1` (cancelled shift-control doc) — a small
  extra fixture exercising the U0 cancel class, if CP1 wants it.
- **Z** ← extracted (a `DocType`-Z / shift-Z shape) into the corpus for provenance, flagged `replay:false`.

Aggregate facts that shaped this (NO identifiers): all 15 production FNs carry the rich DocType set
(shift-open/close + multiple sell types + type-9 + offline); online dominates; drained-offline docs appear in
all but one (pure-online) FN; cancelled docs rest in two FNs; the demo/test FN and every `_TS.db` are EXCLUDED
(U0 §6 — test-cabinet mirrors / demo, not production shapes). Exact FN identifiers and counts are withheld
(data discipline); the selection is by shape-class only, re-derivable at Phase-2 time from the same aggregates.

**Each fixture stays SMALL** — a representative slice (one shift / one session), not a bulk FN export — bounded
by the exporter `--limit`, so the corpus is a handful of short sequences, not a data dump.

---

## §9 Phase-2 execution plan + RED-first (runs only after CP1 sign-off)

1. Land the **exporter fix** (§5) — data-free `scripts/` change; unit test for required/guard.
2. **Sanitizer** (`scripts/sanitize_webcheck_corpus.py`, NEW) — reads a raw export dir, emits synthetic fixtures
   (§2/§3/§4/§7) into an outside-tree staging dir.
3. **Scanner** (§6) — Rust test `webcheck_corpus_has_no_real_data` + `scripts/` pre-commit. **RED-first:**
   - POS tooth: a throwaway fixture with a **planted** leak (a real-shaped TIN / a UUID / a non-recompute hash /
     an `<?xml` marker) → scanner FAILS (each pattern proven to bite).
   - NEG tooth: a clean synthetic fixture → scanner PASSES (no false-positive).
4. **Hash-consistency** Rust test (§4): every fixture `expected_hash == sha256(payload_json)`. **RED-first:**
   perturb a payload byte → FAIL; correct → PASS.
5. Export → sanitize → generate corpus → run scanner + hash test → **commit corpus** (only after all green + the
   CP1 sign-off is recorded).
6. Gate: `fmt --check` + `clippy -D warnings` + full `nextest` + scanner green + every fixture-hash consistent;
   `git log`/tree mechanical check for raw-dump traces (no `.db`, no `var/webcheck_samples/`, no real markers).

---

## §10 CP1 sign-off checklist (for the architect, hard gate)

- [ ] §2 invariant table complete — every sensitive export field has a discard/replace rule; allow-list-by-construction confirmed.
- [ ] §3 shape is non-identifying (no amounts/names/times/ids/hashes/XML cross over).
- [ ] §4 hash-recompute over synthetic bytes; corpus-CI equality assertion defined.
- [ ] §5 exporter fix removes the in-tree default + adds the in-tree guard.
- [ ] §6 scanner patterns adequate (TIN/UUID/timestamp/XML/hash/DPS/Cyrillic/in-tree-dir); RED-first plan for each.
- [ ] §7 fixture format = synthetic golden-style; U3-consumable.
- [ ] §8 coverage = {online sell run, offline session+drain}; Z exported-not-replayed; slices SMALL; `_TS`/demo excluded.
- [ ] Data discipline: no data-derived commit precedes this sign-off; exports outside-tree; scanner + hash gate before corpus commit.

**On sign-off, record the CP1 approval (parent A2 "CP1 sign-off recorded") and authorize Phase 2.**

---

## References
- Parent: `docs/superpowers/specs/2026-07-02-webcheck-ground-truth-phase1-design.md` (§3/U2, §5/A2, §7/CP1).
- U0: `docs/webcheck_reverse/WEBCHECK_GROUND_TRUTH.md` (lnd-translation §4; offline codes §2; `_TS` §6; A07 §1.3/§5).
- Exporter: `scripts/export_webcheck_samples.py` (fix target: line 142 in-tree default; `--output-dir`).
- Fixture precedent: `golden/webcheck_sell_online/`, `golden/webcheck_service_in/`.
- U3 consumer: parent §3/U3 (golden `CanonicalFiscalCommand` → `inline::run`; abstract DPS classes).
