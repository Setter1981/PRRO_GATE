# WebCheck Ground Truth — line-cited lifecycle tables (Phase-1 U0)

> **Load-bearing grounding unit.** Every behavioral claim below carries a `file:line` citation into the
> decompiled WebCheck C# source, cross-checked against real operator dumps at the schema/aggregate level.
> **Phase rule (spec §3/U0):** nothing in U1–U3 may cite a WebCheck behavior absent from this document.

## Provenance

| Source | Identity | Notes |
|---|---|---|
| Decompiled C# tree | `docs/webcheck_reverse/WebCheckMain/WebCheck/` — content fingerprint `sha256(sorted *.cs) = 260ad256cfc4dc73`; last touched by commit `82b54e7` (2026-04-17) | Anchors are pinned to this fingerprint (immune to `main` churn). |
| Verification base | worktree from `main @ 3689695` | C# tree unchanged since `82b54e7`. |
| Real dumps | `~/webcheck_dumps/` — 15 production `<FN>.db` + 16 `<FN>_TS.db` companions = 31 files (one FN present only as a `_TS` companion) | Read via `sqlite3 3.45.1 -readonly` on the static files (no copy, no write, no WAL sidecars). |
| Schema cross-check | one representative real FN (smallest production DB) `.schema`; base-trigger + `offline`-domain scan across **all 15** main FNs; `_TS`/demo aggregates | See per-section **Dump cross-check** verdicts. Real FN identifiers are deliberately omitted (data discipline). |

**Data discipline (satisfied):** this document contains **zero real fiscal data** — only DDL, state-code
semantics, C# citations, and synthetic examples. Dumps were inspected only via `.schema`,
`PRAGMA`-equivalent `sqlite_master`, and `COUNT`/`DISTINCT` aggregates; **no row contents** were read or
transcribed. Nothing under `~/webcheck_dumps/` was copied into the repo tree, transiently or otherwise.

**Citation convention:** `File:NNN` = file under the decompiled tree above, 1-based line. All anchors were
machine-verified against the actual bytes at this fingerprint (not from prior reports — prior audits caught
anchor drift). The full machine-verified inventory is in the **Appendix**.

---

## §1 — DDL: `ksef` / `SHIFTS` / `Sessions` / `fns` (+ triggers)

All DDL is emitted by `CreateDB.cs`. Column lists are transcribed verbatim (order preserved — it is
load-bearing for the positional reads in §3).

### 1.1 Tables

| Table | Cite | Columns (in DDL order) |
|---|---|---|
| `ksef` | `CreateDB.cs:217` | `checkid TEXT, checkxml TEXT, checksigned TEXT, signedanswerfromficscal TEXT, checkidficscal TEXT, localchecknumber Integer, DocType Integer, sum DECIMAL(17,2), mac TEXT, shiftid INTEGER, dt DATETIME, ID Integer PRIMARY KEY AUTOINCREMENT, offline INTEGER DEFAULT 0` |
| `SHIFTS` | `CreateDB.cs:220` | `ID INTEGER PRIMARY KEY AUTOINCREMENT, SHIFTID INTEGER, DATEBEG DATETIME, DATEEND DATETIME, ODRFO VARCHAR(12), ONAME VARCHAR(200), TAXTIN VARCHAR(12), TAXNAME VARCHAR(300), RROFISCAL BIGINT, RROLOCAL BIGINT, OPERATORID INTEGER, LASTFISCALCHECKNUMBER INTEGER, LastLocalCheckNumber INTEGER` |
| `Sessions` | `CreateDB.cs:253` | `id INTEGER PRIMARY KEY AUTOINCREMENT UNIQUE, SessionStartDT DATETIME, SessionStatus INTEGER` |
| `fns` | `CreateDB.cs:250` | `id INTEGER PRIMARY KEY AUTOINCREMENT UNIQUE, checkidfiscal TEXT, used DATETIME, added DATETIME DEFAULT CURRENT_TIMESTAMP, sourceid TEXT` — **no ordering column** (see §5) |

**Positional note (used by §3 offline MAC read):** in `ksef`, zero-based column index `[8]` = `mac`
(`checkid`=0 … `sum`=7, `mac`=8, `shiftid`=9, `dt`=10, `ID`=11, `offline`=12).

### 1.2 Indexes (`CreateDB.cs:624`)

`offlineind ON ksef(offline)`; `closeshiftind UNIQUE ON ksef(shiftid,DocType) WHERE doctype='80' AND offline<>'-1'`;
`openshiftind UNIQUE ON ksef(shiftid,DocType) WHERE doctype='8' AND offline<>'-1'`; `checkidind`, `shiftidind`,
`DocTypeind`, `checkidficscalind`; `checkiddt1 UNIQUE ON ksef(checkid) WHERE doctype='80'`;
**`shiftuniq UNIQUE ON shifts(DATEEND)`** — the mechanism that enforces one-open-shift (see §3.4 sentinel).

### 1.3 Triggers

| Trigger | Cite | Effect (normative) |
|---|---|---|
| `checkcount` | `CreateDB.cs:28` | `AFTER INSERT ON ksef` → `UPDATE SHIFTS SET LastLocalCheckNumber=LastLocalCheckNumber+1 WHERE NEW.shiftid=SHIFTS.ID AND NEW.DocType<>8`. **Per-shift** local counter; shift-open (`DocType=8`) does **not** increment. Basis of §4. |
| `fnsupdate10` | `CreateDB.cs:50` | `AFTER INSERT ON ksef` → `UPDATE fns SET used=datetime(CURRENT_TIMESTAMP,'localtime') WHERE fns.checkidfiscal=NEW.checkidficscal AND NEW.offline=2`. Marks an offline number **consumed** at offline-issue. |
| `fnsupdateerror5` | `CreateDB.cs:94` | `AFTER UPDATE ON ksef` → same `used=datetime(...)` but `WHERE … NEW.offline=3`. **⚠ ABSENT in every dump** (see cross-check). |
| `shifts1` | `CreateDB.cs:160` | `AFTER INSERT ON SHIFTS` backup-mirror; seeds `DATEEND='NULL'` (string) and `LastLocalCheckNumber='0'` in the mirror row. |
| `ksefup1`–`ksefup4` | `CreateDB.cs:142,145,148,151` | backup-mirror of `ksef` INSERT/`offline`-UPDATE. `ksefup4` (offline→1) mirrors `checksigned=''`. |
| `ksefup8` | `CreateDB.cs:72` | `AFTER INSERT ON ksef WHEN NEW.DocType=80` (shift-close) purges `CHECKPAY/CHECKBODY/CHECKTAX` for shifts `< NEW.shiftid-1` (local retention window). |

Note the **join-key spelling trap** in `fnsupdate10`/`fnsupdateerror5`: `fns.checkidfiscal` (fns column) is
joined to `ksef.checkidficscal` (ksef column — different spelling). Both DDLs above confirm the two spellings.

### 1.4 Dump cross-check (representative real FN `.schema`; base-trigger scan across all 15 FNs)

- **MATCH (exact):** `ksef`, `SHIFTS`, `Sessions`, `fns` table DDL (columns, order, `offline INTEGER DEFAULT 0`);
  all §1.2 indexes incl. `shiftuniq`; triggers `checkcount`, `fnsupdate10`, `shifts1`, `SHIFTS2/3`,
  `ksefup1`–`ksefup4`, `ksefup8`. No extra/renamed columns; no schema drift on these four tables.
- **⚠ DIVERGE:** `fnsupdateerror5` (`CreateDB.cs:94`) is **absent from all 15 production DBs** (scan: every FN
  reports only `checkcount,fnsupdate10` among the three base `ksef` triggers). Consistent (not old-vs-new),
  so treat as ground truth: in the field corpus, the **`offline=3` UPDATE path does not consume an `fns`
  number** — only `fnsupdate10` (INSERT `offline=2`) does. **U1/U3 must not model a trigger-driven `fns`
  consumption on `offline=3`.** Surfaced to the architect as the one C#↔dump divergence found.

---

## §2 — `ksef.offline` lifecycle table

`offline` is `INTEGER DEFAULT 0` (`CreateDB.cs:217`). Codes are written/read as **quoted string literals**
in the SQL (e.g. `offline='2'`); SQLite compares them numerically against the INTEGER column. Every code
below is resolved to its write-site and its read/consume-sites.

| Code | Meaning (normative) | Write-site(s) | Read / consume / transition sites | Rests in dumps? |
|---|---|---|---|---|
| `0` | **Online-issued** (default). The online INSERT omits the `offline` column → DDL `DEFAULT 0`. | `SQLlite.cs:1061` (online INSERT has no `offline` col); DDL default `CreateDB.cs:217` | close-shift read `… DocType=80 and offline<>0` (`SQLlite.cs:1164`) | **Yes** — majority. |
| `2` | **Offline-issued, pending drain.** | offline INSERT `… offline)  VALUES(…,'2')` `SQLlite.cs:1468`; recovery reclassify online→`2` `SQLlite.cs:663`,`:675` | backlog `COUNT(*) … offline='2'` `SQLlite.cs:1278`; send-select `SELECT * … offline='2'` `SQLlite.cs:1714`; consumes `fns` via `fnsupdate10` `CreateDB.cs:50` | **Rare** — only the demo FN `7000000512` (`{2}`); production dumps drained all offline docs. |
| `1` | **Offline-doc successfully transmitted** (drained) / promoted from `3`. Not used by the online path (that rests at `0`). | drain success (non-type-9) `SendingOfflineChecks.cs:104`; `3`→`1` promotion `SQLlite.cs:1383` | recovery reads `offline=1 and signedanswerfromficscal in ('','not')` `SQLlite.cs:603`,`:615`; mirror clears `checksigned` on offline→1 `CreateDB.cs:151` | **Yes.** |
| `3` | **Transitional** — set when a type-9 document is drained; later normalized to `1`. | drain of `ReturnTyp=='9'` doc `SendingOfflineChecks.cs:94` | select `… offline='3'` `SQLlite.cs:1308`; `UPDATE ksef SET offline='1' WHERE offline='3'` `SQLlite.cs:1383`; would consume `fns` via `fnsupdateerror5` `CreateDB.cs:94` (**trigger absent — see §1.4**) | **Never** (0/15) — fully transient. |
| `-1` | **Cancelled / rolled-back** shift-control doc. Excluded from open/close-shift uniqueness. | `UPDATE ksef SET offline=-1 …` last open-shift(`DocType=8`) `SQLlite.cs:669`; last close-shift(`DocType=80`) `SQLlite.cs:672` | uniqueness carve-out `openshiftind/closeshiftind … offline<>'-1'` `CreateDB.cs:624` | **Yes, rare** — 2/15 FNs (`{-1,0,1}`). |

**Code domain.** By C# query-sites the domain is **`{-1, 0, 1, 2, 3}`**. **Observed resting domain** across all
15 production dumps: `{-1, 0, 1}` (mostly `{0,1}`; `{-1,0,1}` in 2 FNs; one FN `{0}`); the demo FN rests at
`{2}`. **Code `3` never rests** (always promoted `3`→`1`). No codes outside `{-1,0,1,2,3}` were observed.

---

## §3 — Per-lane MAC / hash flow (normative)

The MAC placeholder is the constant `MacTemp = "mmmaaaccc"` (`All.cs:24`), substituted per-lane below.
**`ksef.MAC` is NOT uniformly "our `unsigned_xml_sha256`" across lanes** — the mapping differs by lane:

### 3.1 Online lane

- **Own hash:** `ksef.MAC` = `SHA.GenerateSHA256File(PathFile)` over the check's **own** file (`SQLlite.cs:1029`),
  stored by the online INSERT (`SQLlite.cs:1061`; that INSERT omits `offline` → code `0`).
- **Previous-MAC:** injected into `mmmaaaccc` by `SubstitutePreviousMAC` (`All.cs:1481`). The previous MAC is
  fetched **from DPS** via `ReturnLastCheckMac` (`All.cs:1536`) → `SubmitPtr.LastCheck(text)` (`All.cs:1576`).
- **Restart caveat:** the fetched value is cached in the **process-static** field `MacTempOld`
  (declared `All.cs:74`; populated `All.cs:1516`; reused without re-query at `All.cs:1524–1528`). It is **not
  persisted** → after a process restart it is re-fetched from DPS. A fetch failure sets `MacTempOld="error"` →
  `errCode 32 "Переход в офлайн режим"` (drives the online→offline transition) (`All.cs:1505–1522`).

### 3.2 Offline lane

- `SaveXMLcheckOffline` reads the **latest local** `ksef.mac` via `LastMac()`:
  `SELECT * FROM ksef WHERE ID=(SELECT MAX(ID) FROM ksef)` (`SQLlite.cs:1665`) returning column `[8]` = `mac`
  (`SQLlite.cs:1668`; def. `SQLlite.cs:1651`). **The chain roots in local state, not DPS** — and it is the
  last row by `MAX(ID)`, **not** filtered by shift or lane.
- That previous local MAC is injected into `mmmaaaccc` (`SQLlite.cs:1438`); this doc's MAC =
  `MakCheck(transformedXML)` = `SHA256` of the transformed XML written to `LastMAK.xml`
  (call `SQLlite.cs:1439`; def `SQLlite.cs:1588–1592`).
- Stored by the offline INSERT with `MAC=<that hash>` and `offline='2'` (`SQLlite.cs:1468`).

### 3.3 Drain lane

- `SendingOfflineChecks.SendDoc` re-substitutes `mmmaaaccc` at send time from `ReturnLastCheckMac`
  (`SendingOfflineChecks.cs:47–48`), signs, and submits.
- On success it updates, **for the same `ksef.ID`**: `offline`→`'1'` (non-type-9, `:104`) or `'3'`
  (`ReturnTyp=='9'`, `:94`); `signedanswerfromficscal` ← the DPS answer (`:112`); and **rewrites `checksigned`
  with the sent `ReturnXML`** (`:119`). **The `ksef.mac` column is NOT recomputed** — the stored MAC remains
  the offline-locally-computed hash from §3.2.

### 3.4 Normative conclusions (for replay)

- Canonical hash column for replay = the DB value **`ksef.MAC`**; **`previous_hash` is NOT stored** (online
  prev comes from DPS `LastCheck`; offline prev comes from the last-local-`mac` at write time).
- **`DATEEND='NULL'` sentinel:** an open shift carries `DATEEND` = the **string literal** `'NULL'`, **not** SQL
  `NULL` — seeded at shift INSERT (`SQLlite.cs:67`, alongside `LastLocalCheckNumber='0'`) and queried as
  `WHERE DATEEND='NULL'` throughout (`SQLlite.cs:319, 356, 403, 486, 606, 657, 666, 2661`). Single-open-shift
  is enforced by `shiftuniq UNIQUE ON shifts(DATEEND)` (`CreateDB.cs:624`) — a *string* `'NULL'` makes the
  unique index bind (SQL `NULL`s would not collide). **Data-quality trap** for any exporter/mapping.

---

## §4 — lnd translation rule (WebCheck per-shift → synthetic per-FN)

**The semantics do not transfer** (spec §2, HIGH#1):

- **WebCheck `localchecknumber` is PER-SHIFT.** The `checkcount` trigger (`CreateDB.cs:28`) increments
  `SHIFTS.LastLocalCheckNumber` per non-open insert; the counter is seeded `'0'` at shift INSERT
  (`SQLlite.cs:67`); each doc's stored `localchecknumber` = `LastLocalCheckNumber+1` (online `SQLlite.cs:1046`,
  offline `SQLlite.cs:1453`; shift-open `DocType=8` uses `'0'` `SQLlite.cs:1035, 1443`). It **resets every
  shift** and therefore cannot serve as a per-FN key.
- **Our `lnd` is PER-FN monotonic** — `ux_fd_fn_lnd` (`rust/prro/migrations/001_baseline.sql`) with allocator
  `node_state.next_lnd` (ADR-M3-A1), fail-closed.

**Translation rule (normative).** Each WebCheck dump is exactly one FN. The per-FN document order is
**`ORDER BY ksef.ID ASC`** — assign a synthetic dense `lnd = 1, 2, 3, …` over the exported doc subset in
`ksef.ID` ascending order.

**Why `ksef.ID` (not `localchecknumber`, `dt`, or `rowid`-alias):** `ksef.ID` is `INTEGER PRIMARY KEY
AUTOINCREMENT` (`CreateDB.cs:217`) → strictly monotone in true local insertion order, per-FN, never reset and
never reused. `localchecknumber` resets per shift (proven by `checkcount`, above); `dt` is `localtime`
`datetime` (second granularity) and can tie. `ksef.ID` is the only per-FN monotone non-resetting key.

**Subset policy is a U2 decision, not U0's:** whether to include shift-control docs (`DocType 8`/`80`) and
cancelled docs (`offline=-1`) in the numbered sequence is an export-policy choice for U2. U0 pins only
(a) the ordering key (`ksef.ID ASC`) and (b) the fact that WebCheck's own `localchecknumber` is discarded and
`lnd` is re-derived.

---

## §5 — `fns` (offline number pool) semantics

| Fact | Cite | Normative statement |
|---|---|---|
| Availability | `NumbersOfflineUse.cs:49` (`COUNT(*) … used is NULL`), `:97` (`SELECT checkidfiscal … used is NULL LIMIT 1`) | `used IS NULL` = **available**. Selection is `LIMIT 1` with **no `ORDER BY`** (`:97`); an optional variant also excludes already-used ids `… NOT IN (SELECT checkidficscal FROM ksef)` (`:93`). |
| Consumption | `CreateDB.cs:50` (`fnsupdate10`), `CreateDB.cs:94` (`fnsupdateerror5`) | Consumption sets `used=datetime(...)` via **triggers on `ksef`**, not by app `UPDATE`. Only the INSERT/`offline=2` trigger (`:50`) fires in the field (`:94` **absent** — §1.4). |
| Loading | `NumbersOfflineUse.cs:22` | New numbers inserted `(checkidfiscal, sourceid, added)`. |
| Bulk-invalidation | `NumbersOfflineUse.cs:172` | `UPDATE fns SET used='2' WHERE used IS NULL` — the **string** `'2'` written into the `DATETIME` `used` column as an invalidation sentinel before loading fresh numbers. **Not** normal consumption. |
| No ordering | `CreateDB.cs:250` | The `fns` DDL has **no ordering column** (`id, checkidfiscal, used, added, sourceid`). |

**Normative:** model WebCheck offline numbers as **availability / consumption COUNTS only**. There is **no
`code_lnd` mapping** and no defined issue order (no `ORDER BY`, no ordering column). Join key to `ksef` is the
spelling-mismatched pair `fns.checkidfiscal ↔ ksef.checkidficscal` (§1.3).

**`Sessions` (offline-session ledger), for §-boundary context:** a session opens with
`INSERT INTO Sessions(SessionStartDT, SessionStatus) VALUES(datetime(...), '1')` (`SQLlite.cs:2038`); the
active session is `WHERE SessionStatus='1'` (`SQLlite.cs:2087`, count `:2186`); status transitions via
`UPDATE Sessions SET SessionStatus=…` (`SQLlite.cs:2123, 2158`); `SessionStartDT` drives offline age-limit
checks (`SQLlite.cs:1997, 2009`); reset via `Delete from Sessions` (`SQLlite.cs:681, 2239`).

---

## §6 — Role of the `<FN>_TS.db` companions

**Determined from C# (not undetermined):** `_TS` is the **DPS test-cabinet mirror database**, not a timestamp
/ TSP store and not a journal.

- `internal const string TestNameS = "_TS"` (`All.cs:36`) — the suffix is a **test**-name constant.
- Selection is gated on the DPS **test cabinet**: `ClassFiscal.cs:3484` uses `FN + "_TS"` **iff**
  `FiscalMode == "cabinet.tax.gov.ua:9443"` (the DPS acceptance/test cabinet, port `9443`), else the plain
  production `FN`.
- When a `<FN>_TS` file exists, the whole SQLite **connection string is swapped** to it
  (`All.cs:613`, `:615`) — i.e. a full parallel DB with the **same schema**, selected in test mode.
- Created on demand for a production FN when the `_TS` file is missing (`FormNewPro.cs:2338–2340`).
- Test-mode archive PDFs are written under a separate `…\Archive\<FN>\_TS\…` path
  (`StringXML.cs:2454, 2549`; `PrintExportCheck.cs:340–355`).

**Dump evidence (cross-check):** each `_TS.db` has a table set **identical** to its main DB (same 14 tables +
`sqlite_sequence`). Row counts: **13 of 16 are empty** (`ksef` rows = 0); only 2 hold a handful of test rows
(25 and 6). The uniform ~124 KB size = an empty schema-only SQLite file.

**Conclusion / relevance to U2–U3:** the field corpus must be exported from the **production `<FN>.db` only**.
`_TS` companions carry DPS **test-cabinet** data (usually none) and are **out of scope** for the corpus. The
one FN present solely as a `_TS` companion, and the demo FN (`7000000512` — the value is a hardcoded in-source
constant, special-cased at `SQLlite.cs:1093`, `All.cs:1487`, `NumbersOfflineUse.cs:76`), are not production
sources.

---

## Appendix — machine-verified anchor inventory

Every anchor below was read at the fingerprint in **Provenance**. `✓C#` = verified by reading the C# line;
`✓dump` = corroborated by dump schema/aggregate.

| # | Claim | Anchor | Verified |
|---|---|---|---|
| A01 | `ksef` DDL (incl. `offline INTEGER DEFAULT 0`) | `CreateDB.cs:217` | ✓C# ✓dump |
| A02 | `SHIFTS` DDL | `CreateDB.cs:220` | ✓C# ✓dump |
| A03 | `Sessions` DDL | `CreateDB.cs:253` | ✓C# ✓dump |
| A04 | `fns` DDL — no ordering column | `CreateDB.cs:250` | ✓C# ✓dump |
| A05 | `checkcount` trigger (per-shift, `DocType<>8`) | `CreateDB.cs:28` | ✓C# ✓dump |
| A06 | `fnsupdate10` (fns used on INSERT `offline=2`) | `CreateDB.cs:50` | ✓C# ✓dump |
| A07 | `fnsupdateerror5` (fns used on UPDATE `offline=3`) — **absent in dumps** | `CreateDB.cs:94` | ✓C# ✓dump(absent) |
| A08 | indexes incl. `shiftuniq`, `open/closeshiftind … offline<>'-1'` | `CreateDB.cs:624` | ✓C# ✓dump |
| A09 | online MAC = `SHA256` own file | `SQLlite.cs:1029` | ✓C# |
| A10 | online INSERT (no `offline` col → 0), stores `MAC` | `SQLlite.cs:1061` | ✓C# |
| A11 | offline INSERT `offline='2'` | `SQLlite.cs:1468` | ✓C# |
| A12 | backlog `COUNT(*) … offline='2'` | `SQLlite.cs:1278` | ✓C# |
| A13 | send-select `SELECT * … offline='2'` | `SQLlite.cs:1714` | ✓C# |
| A14 | `UPDATE … offline='1' WHERE offline='3'` | `SQLlite.cs:1383` | ✓C# |
| A15 | cancel `offline=-1` (open / close shift) | `SQLlite.cs:669, 672` | ✓C# ✓dump(rests) |
| A16 | recovery reclassify online→`2` | `SQLlite.cs:663, 675` | ✓C# |
| A17 | `LastMac()` reads latest local `ksef.mac` (`MAX(ID)`, col `[8]`) | `SQLlite.cs:1651, 1665, 1668` | ✓C# |
| A18 | offline prev-MAC inject + `MakCheck` call | `SQLlite.cs:1438, 1439` | ✓C# |
| A19 | `MakCheck` def = `SHA256` of transformed XML | `SQLlite.cs:1588–1592` | ✓C# |
| A20 | `SubstitutePreviousMAC` (online prev-MAC) | `All.cs:1481` | ✓C# |
| A21 | `ReturnLastCheckMac` → `SubmitPtr.LastCheck` (DPS source) | `All.cs:1536, 1576` | ✓C# |
| A22 | `MacTempOld` process-static cache (restart caveat) | `All.cs:74, 1505, 1516` | ✓C# |
| A23 | `MacTemp = "mmmaaaccc"` placeholder const | `All.cs:24` | ✓C# |
| A24 | drain success `offline`→`'3'`(type-9) / `'1'`(else) | `SendingOfflineChecks.cs:94, 104` | ✓C# |
| A25 | drain rewrites `checksigned`; `mac` not recomputed | `SendingOfflineChecks.cs:112, 119` | ✓C# |
| A26 | `DATEEND='NULL'` string seed + `LastLocalCheckNumber='0'` | `SQLlite.cs:67` | ✓C# ✓dump |
| A27 | per-doc `localchecknumber = LastLocalCheckNumber+1` | `SQLlite.cs:1046, 1453` | ✓C# |
| A28 | fns available `used IS NULL`, no `ORDER BY` | `NumbersOfflineUse.cs:49, 97` | ✓C# |
| A29 | fns bulk-invalidation `used='2'` | `NumbersOfflineUse.cs:172` | ✓C# |
| A30 | fns loading INSERT | `NumbersOfflineUse.cs:22` | ✓C# |
| A31 | `_TS` const + connection swap | `All.cs:36, 613, 615` | ✓C# ✓dump |
| A32 | `_TS` gated on test cabinet `…:9443` | `ClassFiscal.cs:3484` | ✓C# |
| A33 | `Sessions` open/active/transition/reset | `SQLlite.cs:2038, 2087, 2123, 2239` | ✓C# ✓dump |

**Open item surfaced to the architect (checkpoint):** the single C#↔dump divergence is `fnsupdateerror5`
(A07) — defined in `CreateDB.cs:94` but **absent from all 15 production DBs**. This is consistent across the
corpus (a version-drift artifact of base triggers not being re-created on existing DBs), so it is reported as
ground truth, not a blocker: **U1/U3 must not assume an `offline=3` trigger-driven `fns` consumption.**
