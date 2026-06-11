# WebCheck ПРРО — 1С integration contract (operator-provided instruction)

**Source:** operator-provided Google Doc (ИНСТРУКЦИЯ), fetched 2026-05-30 ·
`https://docs.google.com/document/d/1R_N88MyLI3ZdzSotrsutgXOCCIkFDgAZ-rB9BVGgBh0`
**Status:** reference (extracted summary — verify verbatim against the source before coding the adapter).
**Why it matters:** the pilot site's 1С talks to **WebCheck** (this contract), not Maria/Resonance. This is the
ingress surface a WebCheck-site drop-in must present. See architecture decision in
[[project-runtime-spine-gap]] / O1.

## COM surfaces (Windows)

| ProgId | Role | 1С call |
|--------|------|---------|
| `WebCheck.ClassFiscal` | **client fiscal component (primary)** | `Новый COMОбъект("WebCheck.ClassFiscal")` |
| `WebCheckServer.ClassFiscal` | server-side variant (multi-cash; no UI, no Init/Finalization; multithreaded) | server 1С |
| `AddIn.vk_WebCheck` | native External Component ("подключаемое оборудование", ILanguageExtender) | `Новый("AddIn.vk_WebCheck")` |
| `WebCheck.ClassCardserv` | card terminal | — |

**Architecture (decisive):** WebCheck.ClassFiscal does the fiscal logic **LOCALLY** and talks **DIRECTLY** to DPS.
There is **no re-pointable receipt backend** — `webchek.com.ua` is **LICENSING only** (`http://lic.webchek.com.ua`).
→ a cross-platform "impersonate the WebCheck server" seam **does not exist**; impersonating WebCheck means
**replacing the local COM component** (a Windows adapter on our side).

**Network the WebCheck PC needs (the gateway must reach these too):**
- Fiscal service: `https://prro.tax.gov.ua:443` (test `:9443`)
- АЦСК key-issuance server
- Licensing: `http://lic.webchek.com.ua`
- Time/info: `http://fs.tax.gov.ua:8609/fs`

## The verb sequence (the ingress API a shim must answer)

`SetSignSettings()` → `Initialization(XML)` (FN) → `GetCurrentStatus(XML)` (shift state, offline pool) →
`OpenShift(XML)` → `FiscalReceipt(XML)` → `ReportZ(XML)` (daily close) → `Finalization(XML)`.
Also: `OnlineToOffline(XML)` (force offline), `GetSettingRRO()`, server-side `StatusBarXML(FN)` /
`GetDocumentsByShift` / `GetCheck` / `GetShifts`.

## Receipt XML (`FiscalReceipt`) — canonical mapping source

- Attrs: `FN` (12-digit), `Number` (local receipt #), `OperationType` (0=sale, 1=return).
- `<Goods>`: Code, Name, Quantity, Price, Sum, **TaxRate**.
- `<Payments>`: by ID + Sum; custom forms `Pay1..PayN` + `SMB` (N = form id from `GetSettingRRO()`).
- `<L>`: text lines `UP1..UP3` (before goods) / `DN1..DN3` (after).
- Forbidden chars (escape/strip): `& < > ' " `` .`
- Response = **StatusBarXML**: `Err` (0=ok/1=fail), `CheckID` (DPS receipt id), `FN`, `ErrHelp`.

## Tax rates (fixed)

| ID | Name | VAT | Extra |
|----|------|-----|-------|
| 1 | А | 20% | — |
| 2 | Б | 0% | — |
| 3 | В | 7% | — |
| 4 | ГА | 20% | excise 5% |
| 8 | Е | 0% (untaxed) | — |

## Offline (matches HB5 spec)

36h continuous + 168h monthly; auto-transition on DPS unavailability; `OnlineToOffline(XML)` forces it; needs
configured OfflinePool + active license. Error **42** = offline time limit exceeded.

## settings.ini (per-FN [FN] section)

`Offline=1` (paid), `OfflineMax=500` (max 2000), `OfflineMin=50`, `FiscalMode=1` (1=prod/0=test),
`ShowPintForm=1`, `checkUID=1`. Global АЦСК select 0–7 (0=ospus.ini, 1=MASTERKEY, 6=ІДД ДФС, 7=DIIA).
Paths: DB `C:\ProgramData\WebCheck\DB\[FN].db`, keys `\Keys\`, settings `\settings.ini`, archive `\Archive\{FN}\`.

## Error codes (selected)

8 = no open shift · 31 = ПРРО not registered on tax server · 42 = offline limit exceeded · 45 = free-version ≤800₴ ·
77 = system time ±60min · 89 = first shift must be online · 1001 = FN not registered · 1010 = ECP key/password.

## Test

Test FN `7000000512` (offline-only, operator IPN `1111111111`), test server `https://prro.tax.gov.ua:9443`,
admin PIN `2021`. System: Windows, .NET 4.7+, Ukrainian regional.
