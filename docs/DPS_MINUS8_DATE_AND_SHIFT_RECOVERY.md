# ⚠️ DPS `-8` (ERROR_XML_DATE) — SOLVED — the ONLINE envelope `date_time` bug

**Prominent by operator instruction (2026-07-12): "как победишь отметь в документации на видном месте".** B10 hunted this for a *month* and wrongly filed it as a "seed artifact / не баг". **It is a REAL production bug. Do not re-derive.**

## ✅ RESOLUTION (live-proven 2026-07-12 — stuck shift FN 4000162280 CLOSED)

**Root cause:** the ONLINE send path stamps the gRPC `Check.date_time` (envelope) via **`kyiv_local_epoch`** (`stage_send.rs`) — a Unix epoch of the Kyiv wall-clock RE-INTERPRETED AS UTC, i.e. **+3h ahead of real UTC**. DPS decodes that epoch against its real UTC clock → the envelope date is **3 hours in the future** and does NOT equal the check's `<TS>` (Kyiv wall-clock `YYYYMMDDHHMMSS`) → **`-8` = "дата не відповідає Check.date"** (envelope-date ≠ Check-`<TS>`-date).

**The fix (proven):** stamp `date_time` with **`kyiv_comp_date`** (the `YYYYMMDDHHMMSS` integer, identical to `<TS>`) for ONLINE docs too — exactly what the OFFLINE branch already does. Forcing `let date_time = kyiv_comp_date(&inputs.business_ts)?;` → the live Z was **ACCEPTED** (`state=SENT`, `server_fiscal_no=U0dW8tiNLgI`), `statusRro open_shift=false`. Shift closed.

**⚠️ «УЖЕ НАСТУПАЛИ НА ЭТО» — this is the SAME bug B10 root-caused, fixed HALF.** B10 (PR #252, 2026-07-10, memory [[project_b10_offline_drain_handshake]] line 37, commit `9a7ad7c` "align drain date with signed TS") already found: *"корень -8 = ФОРМАТ КОНВЕРТНОЙ ДАТЫ — слали fake-epoch `kyiv_local_epoch`, DPS ждал 14-значный `<TS>`-int; фикс `kyiv_comp_date` для offline-origin+BEGIN/END, **online оставлен epoch**."* B10 fixed the OFFLINE branch and **deliberately LEFT ONLINE on the buggy epoch** (believed online was fine). Today's stuck shift is the **ONLINE Z-close half of that same bug** — it was never fixed. The `stage_send.rs` `date_time` branch: `offline → kyiv_comp_date` (fixed), `else → kyiv_local_epoch` (STILL BUGGY).

**Why it hid so long:** the OFFLINE path uses `kyiv_comp_date` → always worked. Online SELLs apparently pass DPS's *lenient* per-check date check, but the **Z-report (shift-close) date is validated STRICTLY** → only the online Z surfaced `-8`. The `kyiv_local_epoch` encoding (mirrored from `dps_fiscal_server.py:55-81`) is wrong for the live cabinet.

**Proper fix = a real increment (RED-first):** replace the online `kyiv_local_epoch` with `kyiv_comp_date` (align online↔offline); verify a live online SELL + Z both accepted with comp_date; pin with a wire-oracle test (RAGE W2 catches this in one run). Was the `date_time` field 2 of `com.programika.rro.ws.chk.Check`.

---

## 1. What `-8` ACTUALLY means (authoritative, not a guess)

FSCO/DPS error table — `docs/dps_protocol/251051_(1).md:374` **and** `262576_(1).md:490`:

> `-8  ERROR_XML_DATE  =  "невірний формат XML, дата НЕ ВІДПОВІДАЄ Check.date"`

**It is NOT "the `<TS>` string is mis-formatted".** It is **"the date in the submission does not correspond to the check/shift date DPS expects."** A byte-perfect `<TS>` still gets `-8` if its *value* doesn't match the shift's real timeframe.

## 2. Our `<TS>` format is CORRECT (proven by byte-bisect, 2026-07-12)

Live byte-bisect of the exact XML we send (`live_smoke_7_z_report`, FN 4000162280):
```xml
<RQ NDv="ПРО_каса"[cp1251] PrV="1.1" V="1">
  <DAT DI="2" FN="4000162280" TN="13667753" V="1" ZN="1">
    <Z NO="1"><M NM="CASH" SMI="9000" SMO="0" T="0"/><NC NI="1" NO="0"/></Z>
    <TS>20260712143103</TS>
  </DAT>
  <MAC>6ee611…</MAC>
</RQ>
```
At send time UTC was `11:31`; TS `14:31` = `11:31 + 3h` = **correct Europe/Kiev local** (`format_kyiv_local`, `stage_sign.rs`). Format `YYYYMMDDHHMMSS` matches FSCO §197. **The TS was never the bug.**

- WebCheck parity: WebCheck's Z `<TS>` = `СurrentCompDate()` = local (Kyiv) `YYYYMMDDHHMMSS` integer (`All.cs:305`). Same as ours.
- `ZN` is a **hardware-RRO** field (заводський номер). We are a software PRRO → WebCheck sends `ZN='0'`; our `ZN=z_number` is a cosmetic mismatch, **NOT** the `-8` cause (operator-confirmed: "ZN не для нас").

## 3. The REAL cause of THIS `-8`

The stuck shift on `cabinet.tax.gov.ua` (FN 4000162280) was opened during the campaign (≈2026-07-09, offline-origin — see [[project_stuck_shift_recovery]] / B10) and is **>24h old**. Our reseeded Z is dated **now**. DPS: `дата не відповідає Check.date` → `-8`. We cannot date the Z to the shift's real window because a fresh reseed does not know it.

## 4. THE OVERTURN — reseed-recovery is SOLVABLE (earlier conclusion was WRONG)

The reseed research concluded "DPS exposes no shift turnover/dates (only `statusRro`/`infoRro`/`lastChk`) → shift context unrecoverable." **That missed two commands** (`docs/dps_protocol/263155.md`, update 13.10.2021):

- **«Запит переліку документів зміни»** — request the LIST of a shift's documents.
- **«Запит відомостей про документ за локальним номером»** — request a document by local number.

→ DPS **DOES** expose the shift's documents (with dates + turnover). So the reseed-recovery gap (`INGRESS`/24h-limit/Z-close-on-adopted-shift) is **reconstructable**, not fundamental:

**Recovery recipe:** `Запит переліку документів зміни` → fetch the open shift's docs → derive the shift's real dates + turnover → build a Z whose `<TS>`/turnover MATCH → close accepted.

## 5. Status / next

- **Understood precisely; shift NOT yet closed** — closing needs the shift-doc-list command implemented (that IS the reseed-recovery increment) OR DPS 24h auto-close of the over-limit shift.
- Fuzzer wire-oracle (RAGE W2) already flags this class: the envelope/date oracle catches a `-8`-by-date in one run — see `docs/FUZZER_TIER2_RAGE_DOSSIER.md`.
- Related: [[project_stuck_shift_recovery]], `docs/INGRESS_INPUT_VALIDATION_DOSSIER.md` (V2/V3 live-probe blocked on this shift).

---

## 6. Deep byte-bisect session (2026-07-12) — what was ELIMINATED (so nobody re-hunts)

Live byte-bisect via a NEWLY-discovered anon gRPC getter (see §7). The `-8`
(`дата не відповідає Check.date`) is NOT any of these — all ruled out on the wire:
- **`<TS>` format** — correct `YYYYMMDDHHMMSS` Kyiv-local (`format_kyiv_local`), matches FSCO §197 + WebCheck `СurrentCompDate()`.
- **`<TS>` value** — current Kyiv-local (`20260712152235` = UTC 12:22 +3h). Not stale, not future.
- **`<DAT ZN=>`** — forced to `"0"` (matching accepted checks; our code wrongly emits `z_number` — a REAL separate correctness bug, byte-proven: accepted SELL `tt87uHAASI4` carries `ZN="0"`, our Z carried `ZN="1"`). **ZN="0" still got `-8`** → ZN is not the cause. *(Fix ZN→"0" for `<DAT>` regardless — separate increment, needs golden update; the Z-report number stays in `<Z NO=>`.)*
- **envelope-vs-`<TS>` date source** — both derive from the SAME `inputs.business_ts` (`Check.date_time` field 2 = `kyiv_local_epoch`; `<TS>` = `format_kyiv_local`) → consistent for online.

**CONCLUSION: matches B10's month-long hunt verdict — this `-8` on the reseeded/prepared `live_smoke_7` path is a SEED/harness artifact (`устаревший business_ts в старом seed, не баг`), NOT a prod bug.** A CLEAN close (real ingress, consistent `business_ts`, like the campaign's ACCEPTED checks) is the reliable path — it needs the shift adopted with real context (the recovery feature) or the operator's normal flow. **Do not re-hunt `-8` via the smoke seed.**

## 7. 🎯 getChk — anon reconstruction primitive (reachable, PROVEN)

The test cabinet `:9443` (our gRPC host, already allowlisted) exposes MORE than our 5-RPC proto — reflection found **`com.programika.rro.common.chk.ChkGetService.getChk(ChkGet) → ChkReturn`**:
- `ChkGet{ fn, id, date (yyyyMMdd int), number, type(XML|SIGN|TXT) }` → `ChkReturn{ check: bytes, fn, shift, openShift, offline, hash }`.
- **Anonymous, no auth.** Proven: the doc's public prod example AND our test check `tt87uHAASI4`@`20260712` both return full XML. **Works by `id`+`date` (point lookup); `number`-enumeration returns empty.**
- ⚠️ **NO gRPC list-by-shift.** The shift-doc-LIST (the enumerator that yields ids+dates for an unknown shift) is only on the HTTP `/fs/cmd` `CmdDocumentsByShiftFiscalNum` — and those ports (`:8443/:8643/:8609`) are **firewalled** from us (timeout even sandbox-off; likely whitelisted-IP-only). So getChk fetches a doc *if you already know its id+date*, but cannot enumerate a lost shift alone.
- ⇒ Reconstruction (`SHIFT_CONTEXT_RECOVERY_DOSSIER`) is REACHABLE via getChk for id-known docs, but the enumerator is firewalled → full lost-shift reconstruction needs HTTP `/fs/cmd` access (prod / whitelisted) or a surviving local id-ledger (NC-03).
