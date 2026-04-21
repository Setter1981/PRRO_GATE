# DPS official ПРРО reference implementation — analysis

Source: `D:\ПРРО ДПС` (official Ukrainian state tax office PRRO client,
.NET 4.x WPF application). Key binaries decompiled via ilspycmd:

- `PRRODPS.exe` (7.2 MB) — WPF front-end
- `PRRODPS.Core.dll` (7.6 MB) — business logic + DFS transport
- `PRRODPS.db` — SQLite local store

## What's useful for us

### 1. Production DPS URLs (confirmed)

From `PRRODPS.Core.Utils/AppConfig.cs:35-84`:

```
ApiCmd = https://fs.tax.gov.ua:8643/fs/cmd   (JSON commands)
ApiDoc = https://fs.tax.gov.ua:8643/fs/doc   (XML documents)
ApiPck = https://fs.tax.gov.ua:8643/fs/pck   (package batches)
```

Our code targets `cabinet.tax.gov.ua:9443` (test contour). Prod is
`fs.tax.gov.ua:8643`. Different DNS + port — **must be configurable
per environment**.

### 2. Transport contract (binary wire)

From `PRRODPS.DFS/DFSWebClient.cs:190-351,495-524`:

| Layer | Detail |
|---|---|
| TLS | `SecurityProtocolType.Tls12` only |
| Cert check | Strict X509 validation + allow "NotTimeValid"-only errors |
| Content-Type | `application/octet-stream` |
| Content-Encoding | `gzip` |
| Cache buster | `?randomseed={DateTime.Now.Ticks}` on every URL |
| Payload | Signed CMS envelope, then gzipped |
| Response | gzipped + signed → ungzip + unsign |

### 3. Sign vs unsigned commands (critical)

From `DFSWebClient.cs:202` — these JSON commands are sent **UNSIGNED**:

```
ServerState, Check, CheckExt, Schemas, SendDocument, ExciseLabelState
```

Everything else goes through `DFSApi.SignData(..., signMT: true)`
(signed CMS envelope). Our Python `dps_xml` signing logic should match
this exclusion list.

### 4. XML document timestamp rewriting (gotcha!)

From `DFSWebClient.cs:480-493` (`changeDateTime`):

> Before signing any XML document, the official client rewrites
> `CHECKHEAD/ORDERDATE` and `CHECKHEAD/ORDERTIME` to `DateTime.Now`.

```csharp
xmlNode.InnerText = now.ToString("ddMMyyyy");
xmlNode2.InnerText = now.ToString("HHmmss");
```

This means the timestamp inside the fiscal document MUST match the
moment of signing (not business_ts, not adapter-layer ts). **Our
Python signer should set ORDERDATE/ORDERTIME immediately before CMS
sign**, not earlier. Worth a regression check.

### 5. CashRegister state machine

From `PRRODPS.Core.Models.Domain.CashRegister.States/`:

```
EmptyState → PreviewReceiptState → PrepareState → PaymentState
  → FiscalizingState → FiscalizedState
  (MissingTransactionState on offline recovery)
```

Maps to our canonical `DocumentState`:
- EmptyState ≈ (no document)
- PreviewReceiptState ≈ PREPARED
- PrepareState ≈ PREPARED (ongoing)
- PaymentState ≈ (in progress)
- FiscalizingState ≈ SIGNED/ENCRYPTED/SENT
- FiscalizedState ≈ KVT2/ACK
- MissingTransactionState ≈ OFFLINE_LOCAL_ACK (awaiting reconciliation)

### 6. Offline mode handling

From `DFSApi.cs:105-118` (`IsSetOfflineMode`):

- User gets **explicit dialog** before switching to offline.
- Offline switch only allowed when: `!CannotSwitchToOffline` AND shift
  is actually in-receipt (`Store.IsPresentOpenShiftInReceipt`).
- Auto-return to online happens via `DFSReturnToOnline` service.
- Cashier-level toggle, not system-level.

Our Python offline model auto-switches silently on connectivity
failure. Worth debating: should we add a guardrail analogous to
`CannotSwitchToOffline`?

### 7. Error code mapping (server-side)

From `DFSWebClient.cs:764-777` (`ProcessServerErrorCode`):

- Regex parses `"Код помилки: (N)"` from HTTP response body.
- Errors mapped to `ServerErrorCode` enum.
- Specific code `OperatorAccessToTransactionsRegistrarNotGranted`
  surfaces as "Відсутній доступ до РРО для користувача".

Worth extracting the full enum for our error-code translation table.

### 8. Crypto stack (reference-standard Ukrainian providers)

From `config/crypt.ini`:

```
ACSK0  = USC  → UniCrypt_USC.dll  (legacy)
ACSK9  = QLB  → UniCrypt_QLB.dll  (legacy)
ACSK14 = UA1  → UniCrypt_UA1.dll  (current default)
DefaultCrypt = 14
```

Uses three CA providers (USC/QLB/UA1) depending on key issuer. Our
`prro_crypto` crate targets UA1 (via `aedstu04.dll` DSTU 4145). This
matches the reference's default.

### 9. Auxiliary commands (potentially new to our model)

Commands we might not yet support but reference does:

- `CmdObjectsReq` / `CmdObjectsResp` — list business objects
- `CmdOperatorsReq` / `CmdOperatorsResp` — list operators per FN
- `CmdCheckByFiscalNumReq` — query specific fiscal doc
- `CmdDocumentsByShiftFiscalNumReq` — list docs per shift
- `CmdLastShiftTotalsReq` — last shift aggregates
- `CmdRROShiftsByPeriodReq` — shifts in date range (analogous to
  our FIRN/FIRP periodic report)
- `CmdServerStateReq` — keepalive / ping

Our `/v1/admin/*` surface mostly covers the document-level queries,
but `CmdRROShiftsByPeriodReq` is structurally similar to our M7-Py-3c
PERIODIC_REPORT aggregation. **This is the Rust-side response format
we were looking for.**

### 10. Offline session semantics

From `DFSApi.cs:308-332`:

```csharp
GetNextOfflineCheckNum()       → reads next offline ordinal
SetNextOfflineCheckNum(num)    → explicit setter (recovery tool)
IncNextOfflineCheckNum()       → bump after successful offline write
```

Simple counter-based model. Our offline codes management is more
complex (allocation ranges, exhaustion limits) — reference uses a
linear counter.

## Gaps we should probably close

1. **Prod URL config** — add `fs.tax.gov.ua:8643/fs/{cmd|doc|pck}` as
   prod-profile transport. Currently Python uses test URL.
2. **Signing exclusion list** — audit our signer to confirm ServerState
   / Check / Schemas / SendDocument don't go through sign.
3. **ORDERDATE/ORDERTIME rewrite** — verify we set these at sign time,
   not earlier.
4. **Offline-mode guardrail** — check if we have a `CannotSwitchToOffline`
   equivalent (e.g., RETURN must stay online per reference).
5. **`CmdRROShiftsByPeriodReq`** — this is the shape 1С expects for
   periodic-report responses. If we extend Rust `CanonicalResponse`
   with periodic-report payload (M7-Py-3d deferred), use the fields
   from `CmdRROShiftsByPeriodResp`.
6. **Error code enum** — mine the full `ServerErrorCode` list from the
   decompile and add to our `CanonicalErrorCode`/translation table.

## Things confirmed / validated

1. TLS 1.2 + cert validation — we already do this in `prro_sidecar`.
2. Signed CMS + gzip — we already do this via `prro_crypto` + Python
   gzip layer.
3. `application/octet-stream` — confirmed our content type is correct.
4. DSTU 4145 with UA1 root — we use the right crypto provider.
5. Separate endpoints per function (cmd/doc/pck) — our transport router
   already dispatches similarly.

## Files and paths

- Decompiled to: `/tmp/prrodps_dec/` (not checked into repo)
- Key source files:
  - `PRRODPS.DFS/DFSApi.cs` (2309 LOC) — main orchestration
  - `PRRODPS.DFS/DFSWebClient.cs` (778 LOC) — transport layer
  - `PRRODPS.DFS/DFSCheck.cs` (990 LOC) — fiscal check builder
  - `PRRODPS.DFS/DFSReturnToOnline.cs` (565 LOC) — online-recovery flow
  - `PRRODPS.Core.Models.Domain.CashRegister.States/` — state machine
  - `PRRODPS.Core.Utils/AppConfig.cs` — endpoints
  - `config/crypt.ini` — crypto provider setup
