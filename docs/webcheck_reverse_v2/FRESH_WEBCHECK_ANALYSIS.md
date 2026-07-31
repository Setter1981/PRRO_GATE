# Fresh WebCheck (PRRO32 v6.0.8.1368) — Analysis & Reference

Decompiled 2026-07-08 (ilspycmd 8.2) from `C:\Program Files (x86)\WebCheck\PRRO32\WebCheck.dll` → `docs/webcheck_reverse_v2/WebCheck/` (148 .cs). Analyzed vs the older decompile in `docs/webcheck_reverse/`.

## 0. VERDICT
**Fresh = OLD fiscal core + NEW features. ZERO fiscal-protocol / offline-flow / DPS-wire-format changes.** Our offline design (B10/B11) built from the old decompile **HOLDS on the current vendor version.** `SendingOfflineChecks.cs` is byte-identical old↔new; `ClassFiscal/SubmitPtr/StringXML/SQLlite/Dispatch` stable.

**Real deltas:** version bump 6.0.7.1331→6.0.8.1368; **`Retries` 2→18** (DPS submit retries before giving up — signal for our offline-fallback timing); `-8`(EPZ-cashback)→proto=1; `OfflinePullKsefCheck` (dedup offline codes vs ksef); `grpcproxy` INI override; **`MonoBankkProtocol.cs`** NEW but DEAD (unused `MBP` field in FormTerminal — future acquiring groundwork, fiscally irrelevant). FormTerminal "ліміт" = `LimitTimeUp` = terminal COMMS timeout (35s), NOT a fiscal cap. FormTestErrors `BugFix(1..11)` self-heal byte-identical.

## 1. OFFLINE PROTOCOL (B10/B11 ground truth)
**Channel:** SZZD gRPC `sendChkV2` (cabinet:9443) — SAME as online. NOT the EVPZ/ApiPck package channel.

**Doc types (SZZD; `<C T=...>` in XML — verAPI=2 adds +100):** 8/108=shift-open, 80=Z/close, **9/109=Початок офлайн сесії (offline BEGIN)**, **10/110=Завершення (offline END)**, 12/112=ASK_OFFLINE_CODES. typCheck (gRPC envelope): 0/1=Chk, 2=ZReport, 3=ServiceChk (shift-open + offline-begin/end).

**ENTRY (auto):** on DPS `returnStatus ∈ {0,-1}` after `Retries`(=18) attempts → `OfflineOn→OfflineOnManually` (gated: !offline + FullVersion + OfflineAllowed). Creates **DocType=9** (consumes a pool code, `offline='2'`, MAC WITH ID). **Guard: `MaxID(ksef)<1` → cannot go offline without a prior online doc (errCode=89).** `-16`(invalid offline ID)→`OfflineOnTechno` (`offline='3'`, "ТехнічнийЧек", no pool code). Manual entry: `ClassFiscal.OnlineToOffline`.

**EXIT/DRAIN (`Dispatch.OfflineToOnline`):** one doc per call; sends `offline='2'` docs in insertion order → **DocType=9 first (opens DPS offline window) → content → DocType=10 last** (built at drain-end, `СurrentCompDate` timestamp). Guard before the 10: re-check no new offline docs (abort if appeared). Then `CloseOfflineKsef` (`offline` '2'→'3'→'1'). `OfflineTrue()`=`offline>'1'`.

**168h/time:** computed from the **DocType=9 timestamp** (`SELECT dt FROM ksef WHERE offline>'1' AND DocType='9'`) — per-session cap 36h/2160min; monthly 168h accumulated in INI `OfflineTime`. **⇒ docs-as-canon: DPS counts offline state+time from 9/10; we mirror.**

**Byzantine/error on drain:** probe (`LastCheckAllInfa`) to disambiguate; idempotency guard for DocType=9 re-send after crash; duplicate detection. **No RMR — WebCheck just stops+logs+retries next cycle. OUR RMR is architecturally better/safer.**

## 2. ONLINE FISCAL CORE
Build (`StringXML`) → sign (`.p7s`) → `SubmitPtr.SubmitCheck` → `client.Check(verAPI,...)`. Fiscal number = `answer.Id` on Status==1 → `ksef.checkidficscal`. `localchecknumber` = `SHIFTS.LastLocalCheckNumber+1` (trigger `checkcount` increments on INSERT where DocType<>8). `DI` = `MAX(ksef.ID)+1`.

**Shift:** open=DocType 8 (typCheck=3, OpenCloseShift=true); Z/close=80 (typCheck=2). `SHIFTS.DATEEND='NULL'` (string) = open-shift sentinel; `ReturnOpenShift` errors if multiple open. Offline shift/Z → `SaveXMLcheckOffline(...,"8"/"80")` `offline='2'`.

**Chain/MAC:** `LastMac()`=`mac` column of MAX(ID) ksef row. `MakCheck`=SHA256(XML file). **Online: `<MAC>{prev}</MAC>` (no ID). Offline: `<MAC ID='{offline_code}'>{prev}</MAC>`** (offline_code = `fns.checkidfiscal`). First doc/empty chain → empty MAC body.

## 3. DPS ERROR-CODE → ACTION MAP (authoritative; validate our `error_routing` against this)
| Code | Meaning | Action |
|---|---|---|
| 1 | success | returnNumber = fiscal ID |
| 0 | no DPS connection | after Retries → **OfflineOn** |
| -1 | signature verify error | after Retries → **OfflineOn** |
| -2 | RRO check error | OpenClose→local-open+poll; else terminal |
| -3 | ERROR_SAVE (write) | **retry 7× (333ms)** then terminal → confirms [[project_backlog_dps_error_save]] |
| -4 | general | terminal |
| -5 | wrong package type | terminal (Robot: if last=110, FixOffline) — **our smoke's "offline id in online mode" was -5** |
| -6 | no Z for prior day | terminal |
| -7 | invalid XML struct/FN | terminal |
| -8 | invalid XML date | terminal |
| -9 | invalid receipt format | terminal (our B9 "-9 not ID in MAC" is this class) |
| -10 | invalid Z format | terminal |
| -11 | **RRO blocked, 168h exceeded** | terminal (no fallback — already blocked) |
| -12 | invalid previous hash | terminal |
| -13 | PRRO not registered | treated as no-data in LastCheck |
| -14 | operator not registered | terminal |
| -15 | **shift not open** | OpenClose→poll CheckLastCheck; else terminal |
| -16 | invalid offline ID | CHECKHEAD≥1 or Techno→continue; else errCode97 emergency |
| other <-1 | undefined | **fail-closed terminal** (errCode 25) |
⚠️ Note the code→meaning here (-12=bad-hash, -15=shift-not-open) — reconcile with our `error_routing` labels and our live `-15 ERROR_BAD_HASH_PREV` pairing (test-cabinet quirk or our labeling — verify).

## 4. DB SCHEMA
**`ksef`:** checkid, checkxml, checksigned, signedanswerfromficscal (only for DocType=80), checkidficscal (fiscal-no OR offline code), localchecknumber, DocType, sum, **mac** (SHA256 chain), shiftid, dt, ID PK AUTOINCREMENT, **offline** (0=online-pending, 1=online-sent, 2=offline-pending-drain, 3=techno-offline, -1=voided).
**`fns`** (offline code pool): checkidfiscal, used (NULL=available), added, sourceid. `OfflineID`=`WHERE used IS NULL AND checkidfiscal NOT IN (SELECT checkidficscal FROM ksef) LIMIT 1`.
**Triggers:** `checkcount` (local-check-number++), `fnsupdate10` (mark fns.used on offline=2 insert), `fnsupdateerror5` (mark on offline=3), `ksefup8` (GC old CHECK* on Z).

## 5. REUSABLE RULES (for our gateway / backlog validation)
- **50000 CASH cap:** `DopNal/AllowableCash` default 50000, hard `if(>50000)=50000` (All.cs:884). WebCheck caps CASH (готівка). Reconcile with our gap #5 (operator: "49999.99 per receipt") — confirm cash-portion vs receipt-total. → [[project_fiscal_correctness_gaps]]
- **Tax letters/rates:** А=20%VAT, Б=0%, В=7%, Г*=excise5%, Д*=excise7.5%, Е/Ж/З=0%. Rounding: 20%→TXI=SMI/6; ГА→excise=SMI·5/105; ДА→excise=SMI/1.275·0.075. → validates our calc_tax + gap #3.
- **operationtype:** 0=SELL, 1=RETURN(+idcancel), 8/-8=service; other→errCode19.
- **UUID idempotency:** `checkUID` flag (off by default) → CountUID>0 rejects errCode88.
- **FN:** numeric, exactly 10 digits.
- **PayForms:** max 180.
- **BugFix(10) offline recovery:** `DELETE FROM fns WHERE used NOT NULL` (pool reset — "Пошук помилок" nuclear option) → [[project_backlog_operator_recovery]].
- **First doc must be online** (offline blocked if ksef empty, errCode89).

## 6. IMPLICATIONS FOR US
- **B10 (offline drain):** add DocType=9/10 (T=109/110) begin/end + drain sequence 9→content→10. Design confirmed valid on current version.
- **B11 (offline entry):** auto on returnStatus 0/-1 after ~18 retries; guard MaxID≥1; -16→techno.
- **error_routing:** validate against §3 table (esp. -3 retryable, -11=168h-block, unknown→fail-closed; reconcile -12/-15 labels).
- **Byzantine:** keep our RMR (better than WebCheck's silent-retry); add hardened response decode → [[project_backlog_byzantine_dps_handling]].
- **50k / tax:** §5 validates gaps #3/#5.
</content>
