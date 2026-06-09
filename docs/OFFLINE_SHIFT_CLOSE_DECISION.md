# Offline SHIFT_CLOSE Decision

> **M3b update 2026-05-17 (PR #63 merged at `e04031b`):** The 3-case `Closing` state overload analysis in §6.2 (online SHIFT_CLOSE in flight, offline local Z_REPORT, crash-recovery) drove the M3b shift state expansion. RESOLVED by introducing `ClosingLocalPendingDrain` + `RequiresManualReconciliation` states per `docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md` §3.1. Offline Z_REPORT now lands in `ClosingLocalPendingDrain` (not `Closing`), then drain → `Closed` (edge 13) or drain reject → `RequiresManualReconciliation` (edge 14). The "Closing overload" problem this doc identified is now design-resolved.

## 0. M3b correction 2026-05-16 (authoritative)

This document is the **authoritative policy note** for offline shift close-of-day in the Rust gateway.  After Round 0 of the M3b W10 design we corrected an architectural error in the earlier framing: a blanket *"block Z-report whenever offline backlog or active offline session exists"* rule would trap an offline shift against the 24h legal limit with no compliant close-of-day exit.  The corrected policy distinguishes two distinct close-of-day paths:

1. **Online Z_REPORT over a pending offline backlog → MUST be blocked.**  DPS would record a Z-report that omits offline receipts not yet drained — illegal and forensically unrecoverable.
2. **Offline-mode local Z_REPORT close-of-day → MUST be allowed** as a Pattern C document.  The Z_REPORT is emitted via the offline ladder (`Signed → OfflineLocalAck`), consumes an offline code, and becomes the final local document of the offline shift.  W9b drain later replays it through the wire-send ladder in strict `lnd` ASC after all prior offline sales/returns, and W12 confirms KVT2 → final `ACK`.
3. **Post-local-close lockout.**  After the offline Z_REPORT lands in `OfflineLocalAck`, new sale / return documents on the same shift MUST be refused until the next allowed shift-open policy is satisfied.  Until that policy is precisely defined (operator + legal review), W10 leaves the audit seam typed so the rule can land in a follow-up without resurrecting the blanket-blocker design.
4. **Legal timers (24h shift / 36h continuous offline / 168h monthly offline) are active engineering risks**, not non-goals — see [`docs/LEGAL_INVARIANTS.md`](LEGAL_INVARIANTS.md).  The offline-mode local Z_REPORT path exists precisely so the 24h shift limit can be honoured even when DPS is unreachable; sales may be blocked at limits, but the close/reporting path must always have an exit.
5. **Hard close-code reserve = 1 currently-unconsumed code in the FN-scoped pool while that FN has an open shift and the local offline Z_REPORT has not been emitted.**  *Storage-side wording precision (2026-05-16):* `offline_codes` is FN-scoped (PRIMARY KEY `(fiscal_number, code_lnd)` per `rust/prro/migrations/015_offline_normalize.sql:227`), not session/shift-partitioned; the reserve operates on the FN's pool and is activated by the predicate "FN has open shift AND local offline Z_REPORT not yet emitted".  The offline Z_REPORT close path described in (2) needs one such free code at the moment of emission.  If ordinary offline `SELL` / `RETURN` / `SERVICE_*` docs are allowed to consume the entire pool while the predicate holds, the operator reaches close-of-day with zero free codes — and the Pattern C local Z_REPORT close-of-day exit becomes inaccessible, re-asserting the 24h trap.  Therefore W10 enforces a hard reserve of exactly one row left `consumed_at IS NULL` while the predicate holds: ordinary fiscal docs refuse-with-audit (`OFFLINE_CODE_RESERVED_FOR_CLOSE`) when only the reserved code remains; the offline Z_REPORT MAY consume it.  This is a **legal escape hatch**, not an operational watermark — the latter (`min_offline_codes`, commonly set ~10 by operators) is the *upstream* refill trigger; the hard reserve is the *last-line* legal guarantee.  Pool=0 at close-of-day time is pilot-critical / legal-critical: the offline Z_REPORT itself is refused with `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED` + `reason: "code_pool_exhausted"` + **severity Critical** (NOT the default Warning of the policy-refusal sibling reasons — the 24h trap is functionally re-asserted for this FN), and the operational refill watermark has failed upstream.

   **Channel-specific reserve shape** (see plan §"DPS Channel Taxonomy"):
   - **WebCheck / gRPC channel** (M3b W10 in-scope): reserve = one row in `offline_codes` left `consumed_at IS NULL` per active offline shift.  The W5 `acquire_code_tx` CAS is gated by the W10 policy guard's reserve check on ordinary doc types; Z_REPORT bypasses the reserve check.
   - **DFS HTTP / XML channel** (future implementation, NOT in M3b): reserve = one offline local ordinal / control-number slot in the `OfflineSessionId.localOfflineNum.controlNumber` derivation per `PRRODPS.DFS/DFSApi.cs::MakeOfflineNum`.  Same conceptual rule (exactly one close-capable slot held back from ordinary docs), different storage mechanism — the DFS implementation must adapt the reserve check to the DFS offline-numbering pipeline.

6. **Maria 304 is NOT a DPS channel.**  It is an ingress / POS adapter (the same role REST / XML-RPC / Maria-TCP shells play for the Rust gateway).  Channel-switch rules in §1 / §7 below apply to the *DPS-side* channel pinned to the open shift (WebCheck/gRPC vs DFS HTTP/XML), NOT to which ingress adapter accepts the POS message.

7. **No channel switch with open shift.**  Once a shift is opened against one DPS channel family, the channel is pinned for that shift until `Z_REPORT` close-of-day completes AND any offline backlog from that shift drains to final ACK on that same channel.  This is a strengthening of frozen invariant 3 (channel switch forbidden with open shift) — see `docs/LEGAL_INVARIANTS.md`.  Mid-shift channel switching would lose forensic continuity: offline numbers, ticket shapes, and KVT2 evidence formats are channel-specific (see plan taxonomy) and cannot be reconciled across channels post-hoc.

W10 is the single seam that implements both decisions.  See `docs/superpowers/plans/2026-05-14-m3b-implementation.md` §Task 10 for the gate surface (`PolicyDecision::{AllowOnline, AllowOfflineLocalClose, RefuseOnlineBacklogPending, ...}`) and the full audit vocabulary: `ONLINE_Z_REPORT_BLOCKED_BACKLOG` (Warning), `OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED` (Info), `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED` (Warning by default — **escalated to Error/Critical when `reason == "code_pool_exhausted"`**, see point (5) below: that case means the operator has lost the compliant close-of-day exit), `POST_LOCAL_CLOSE_SALE_REFUSED` (Warning), `OFFLINE_CODE_RESERVED_FOR_CLOSE` (Warning).  The §Task 10 source-of-truth list MUST stay in sync with this enumeration when new events are added.

The older "Step A — legal blocker first" recommendation in §10 below is preserved as historical context — it described a stop-gap before the full compound contour landed.  The corrected W10 supersedes it: the policy must be ONLINE-vs-OFFLINE-aware from the first implementation, not a blanket blocker that gets refined later.

## 1. Problem Statement

Проєкт уже підтримує `OFFLINE_LOCAL_ACK` для офлайн-документів та ручний `OfflineSyncService`, але контур закриття зміни/дня залишається неповним.

Потрібно відповісти на два різні питання:

1. Чи можна в нашій моделі дозволити `SHIFT_CLOSE` під час `OFFLINE`?
2. Чи є `SHIFT_CLOSE` самодостатньою доменною операцією, чи він має розглядатися разом із `Z_REPORT` / щоденним фіскальним звітним чеком?

Це decision-документ, не implementation plan у коді.

## 2. What Current Code Does Today

### 2.1 Write-path

- `SHIFT_CLOSE` зараз **не підтримується** в offline path:
  [src/prro_gateway/services/write_path.py](/mnt/d/PRRO_GATE/src/prro_gateway/services/write_path.py:734)
- До offline path включено `Z_REPORT`, але не `SHIFT_CLOSE`:
  [src/prro_gateway/services/write_path.py](/mnt/d/PRRO_GATE/src/prro_gateway/services/write_path.py:734)
- Водночас `_sync_shift_close_locked()` уже вважає `OFFLINE_LOCAL_ACK` достатнім для локального переведення зміни в `CLOSED`:
  [src/prro_gateway/services/write_path.py](/mnt/d/PRRO_GATE/src/prro_gateway/services/write_path.py:537)

Це означає:

- сьогодні `offline SHIFT_CLOSE` фактично не запускається;
- але якщо просто додати `SHIFT_CLOSE` в `_operation_supports_offline()`, зміна почне локально закриватися на `OFFLINE_LOCAL_ACK`, що є небезпечною напівреалізацією.

### 2.2 Transport semantics

- Для Checkbox `SHIFT_CLOSE` мапиться на `POST /shifts/close`:
  [src/prro_gateway/transports/checkbox_rest.py](/mnt/d/PRRO_GATE/src/prro_gateway/transports/checkbox_rest.py:336)
- `status='CLOSED'` для `SHIFT_CLOSE` мапиться в `DocumentState.ACK`:
  [src/prro_gateway/transports/checkbox_rest.py](/mnt/d/PRRO_GATE/src/prro_gateway/transports/checkbox_rest.py:413)
- `Z_REPORT` у канонічній моделі існує, але поточний Checkbox transport його **не реалізує**:
  [src/prro_gateway/transports/checkbox_rest.py](/mnt/d/PRRO_GATE/src/prro_gateway/transports/checkbox_rest.py:364)

### 2.3 Shift persistence model

- Модель `ShiftRecord` уже має поля:
  - `close_document_id`
  - `z_report_document_id`
  [src/prro_gateway/models/storage.py](/mnt/d/PRRO_GATE/src/prro_gateway/models/storage.py:105)
- Але `ShiftRepository` зараз керує майже тільки `state` і не наповнює ці link-поля:
  [src/prro_gateway/repositories/shifts.py](/mnt/d/PRRO_GATE/src/prro_gateway/repositories/shifts.py:10)

Висновок: доменна модель натякає на зв'язок `shift close` і `z-report`, але реалізація цього ще не доведена.

## 3. What Current Project Docs Say

- План проєкту фіксує:
  - `SHIFT_CLOSE` closes the active shift after fiscal success
  - `Z-report/fiscal daily report must respect pending/offline document constraints`
  [docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md](/mnt/d/PRRO_GATE/docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md:222)
- У Sprint scope вже записано, що треба блокувати `Z-report/shift close` там, де є pending offline docs:
  [docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md](/mnt/d/PRRO_GATE/docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md:422)
- Acceptance snapshot теж фіксує gap `SHIFT-CLOSE-01`:
  [docs/ACCEPTANCE_COVERAGE_SNAPSHOT.md](/mnt/d/PRRO_GATE/docs/ACCEPTANCE_COVERAGE_SNAPSHOT.md:132)
- `LEGAL_INVARIANTS.md` формулює, що фіскальний денний звіт і закриття зміни не повинні завершуватись при наявності непереданих офлайн-документів:
  [docs/LEGAL_INVARIANTS.md](/mnt/d/PRRO_GATE/docs/LEGAL_INVARIANTS.md:103)

### Terminology drift

У `LEGAL_INVARIANTS.md` ще використовується `OFFLINE_PENDING_SYNC`, тоді як реальний код уже перейшов на `OFFLINE_LOCAL_ACK`.

Це **переважно термінологічний drift**, а не окрема нова state-machine, але документ треба оновити в майбутньому.

## 4. Official / Legal Sources Reviewed

### 4.1 Official sources

- Наказ Мінфіну №317 / offline range contour:
  https://zakon.rada.gov.ua/go/z0636-20
- Офіційна брошура ДПС про ПРРО та offline режим:
  https://kyiv.tax.gov.ua/data/material/000/349/444462/broshura_2.pdf
- Роз'яснення ДПС Закарпаття, опубліковано **30 січня 2024**:
  https://zak.tax.gov.ua/media-ark/news-ark/750776.html
- ЗІР ДПС:
  https://zir.tax.gov.ua/main/bz/view/?id=39842&src=ques

### 4.2 Facts established from official sources

Факт:

- в офіційному матеріалі ДПС сказано, що якщо під час offline настає час формування `z-звіту`, йому також присвоюється фіскальний номер із діапазону; усі такі документи після відновлення зв'язку надсилаються пакетом і зберігаються до підтвердження прийняття;
- у роз'ясненні ДПС від **30 січня 2024** сказано: якщо при закритті зміни в `Z-звіті` не відображаються offline-операції, операцію треба скасувати, переконатися, що дані передані на фіскальний сервер, і повторити;
- у ЗІР зафіксовано, що якщо ПРРО був переведений у `offline`, закриття робочої зміни на інших пристроях заборонене.

Inference:

- офіційний контур допускає offline close-of-day щонайменше через `Z_REPORT`;
- при цьому close/day-close повинен залишатися пов'язаним з тим самим пристроєм/екземпляром ПРРО і з коректною доставкою offline backlog;
- пряма теза `offline SHIFT_CLOSE завжди дозволений` з цих джерел не випливає.

## 5. Architectural Decision

### 5.1 Main decision

**Цільова підтримка offline close-of-day потрібна. Але її не можна реалізовувати як ізольовану пряму операцію шляхом простого додавання `SHIFT_CLOSE` до `_operation_supports_offline()`.**

Це небезпечно, бо:

- поточний код одразу переводитиме зміну в `CLOSED` на `OFFLINE_LOCAL_ACK`;
- у системі ще немає повністю реалізованого `Z_REPORT` contour для Checkbox;
- це створить юридично сумнівний стан: зміна локально закрита, а денний фіскальний контур не доведений до DPS-підтвердження.

### 5.2 Recommended domain interpretation

Для нашого ядра потрібно розрізняти:

- `SHIFT_CLOSE`:
  backend-specific operational close request;
- `Z_REPORT`:
  юридично значущий daily fiscal report / close-of-day document.

**Рекомендована модель: close-of-day — це compound contour, у якому `Z_REPORT` є головним фіскальним документом, а `SHIFT_CLOSE` є transport/backend action only where the backend requires it.**

Для поточного Checkbox contour:

- `POST /shifts/close` слід вважати операційним close call;
- юридично безпечне offline close-of-day не можна будувати лише на ньому;
- offline day-close має бути прив'язаний до `Z_REPORT` і до подальшої впорядкованої доставки offline backlog.

## 6. Recommended State Machine

### 6.1 Minimal safe recommendation

Не додавати нові `DocumentState` на цьому етапі.

Використовувати на рівні документів уже наявні стани:

- `OFFLINE_LOCAL_ACK`
- `SENT`
- `KVT1`
- `KVT2`
- `ACK`
- `REJECTED`
- `REQUIRES_MANUAL_RECONCILIATION`

### 6.2 Shift lifecycle recommendation

Для shift lifecycle мінімально безпечна інтерпретація така:

- `OPENED`
- `CLOSING`
- `CLOSED`

Але `CLOSING` має стати змістовним, а не лише проміжним async-online станом.

`CLOSING` повинний охоплювати:

- online close sent but not finally acknowledged;
- offline day-close created locally (`Z_REPORT` у `OFFLINE_LOCAL_ACK`);
- будь-який case, де зміна вже не повинна приймати нові операції, але close/day-close ще не завершено фіскально.

### 6.3 Linkage requirement

Shift record повинен бути зв'язаний з документами закриття:

- `close_document_id`
- `z_report_document_id`

Поки ці зв'язки не наповнюються і не використовуються, contour close-of-day вважається неповним.

## 7. Required Guards

### 7.1 Guards that should exist eventually

1. `SHIFT_CLOSE` online must be blocked if:
   - є pending offline documents того самого `fiscal_number`;
   - або вже існує unresolved close/day-close contour попередньої зміни.

2. **Online** `Z_REPORT` must be blocked if:
   - offline backlog, який має потрапити до денного контуру, ще не доставлений або не узгоджений з close-of-day semantics.

   **Offline-mode local** `Z_REPORT` close-of-day is the explicit *allowed* path under these conditions — see §0 above and W10 plan task.  It MUST NOT be conflated with the online block.

3. `SHIFT_OPEN` must be blocked if:
   - попередня зміна знаходиться в `CLOSING`;
   - linked close/day-close documents не мають фінального допустимого стану;
   - або попередній shift був закритий локально через offline `Z_REPORT` і нова shift-open policy ще не виконана (`POST_LOCAL_CLOSE_SALE_REFUSED` lockout — see §0).

4. Close/day-close during or after offline must remain single-device / single-channel scoped:
   - не можна допускати закриття зміни на іншому пристрої/іншому каналі під час unresolved offline contour.

### 7.2 Immediate safety boundary before full contour

Поки compound contour не реалізований:

- не слід відкривати в runtime **standalone direct** `offline SHIFT_CLOSE`, який локально закриває зміну без повного day-close contour;
- **online** `SHIFT_CLOSE` / **online** `Z_REPORT` слід блокувати при наявності `OFFLINE_LOCAL_ACK` backlog для того ж `fiscal_number`, де це юридично необхідно;
- **offline-mode local** `Z_REPORT` close-of-day, навпаки, MUST бути дозволений як Pattern C `OFFLINE_LOCAL_ACK` document (див. §0 + W10 plan task) — це не "проміжна позиція", це фінальна архітектурна гарантія, що 24h shift limit має compliant exit.

Це безпечна позиція до повного compound contour'у: ONLINE block чітко відокремлений від OFFLINE local close.

## 8. Recovery / Restart Semantics

Після restart система повинна відновлювати не лише active shift, а й pending close-of-day contour.

Мінімальні вимоги:

1. Якщо `shift.state == CLOSING`, новий `SHIFT_OPEN` не дозволяється автоматично.
2. Якщо linked `z_report_document_id` у `OFFLINE_LOCAL_ACK`, документ повинен проходити через `OfflineSyncService` у строгому порядку після раніше створених offline документів.
3. Якщо `close_document_id` або `z_report_document_id` у `SENT/KVT1/KVT2`, контур завершується через reconciliation, а не новим write-path документом.
4. Лише після фінально допустимого результату close-of-day contour зміна переходить у `CLOSED`.

## 9. Risks If We Do Nothing

1. Хтось може спростити реалізацію і просто додати `SHIFT_CLOSE` до offline-supported operations, після чого зміна буде локально закриватись на `OFFLINE_LOCAL_ACK` без повного close-of-day contour.
2. `SHIFT_CLOSE` і `Z_REPORT` залишаться напіврозірваними доменами, хоча модель shifts уже має `z_report_document_id`.
3. Після offline activity можна буде закривати/відкривати зміну без чіткої гарантії, що денний фіскальний контур завершений правильно.
4. Документація й код продовжать розходитися: docs already expect legal blockers, code still lacks full implementation.

## 10. Recommended Bounded Implementation Sequence

### Step A — legal blocker first (historical, superseded by M3b W10 — see §0)

> **Superseded 2026-05-16.**  The original "Step A" wording below described a stop-gap blanket-blocker.  M3b W10 implements the corrected ONLINE-vs-OFFLINE-distinguished policy directly — the stop-gap is skipped.

Зробити вузький guard:

- блокувати **online** `SHIFT_CLOSE` та **online** `Z_REPORT` там, де є pending `OFFLINE_LOCAL_ACK` backlog для відповідного `fiscal_number`, якщо повний close-of-day contour ще не реалізований;
- **offline-mode** local `Z_REPORT` close-of-day MUST залишатися дозволеним як Pattern C document — це не частина блокатора, це окрема routed-acceptance ARM (див. §0);
- додати окремий canonical error code для legal blocker (online refusal) та окремий audit event для offline accepted-local-close (`OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED`).

### Step B — shift/document linkage

Довести linkage:

- почати реально заповнювати `close_document_id` і `z_report_document_id`;
- зробити `CLOSING` відновлюваним, а не merely cosmetic.

### Step C — online `Z_REPORT` support for Checkbox

Додати реальний transport contour для `Z_REPORT` у Checkbox.

Поки цього немає, повний day-close contour для Checkbox вважається неповним.

### Step D — offline `Z_REPORT` contour

Реалізувати offline `Z_REPORT` як документ close-of-day:

- local creation with `OFFLINE_LOCAL_ACK`;
- strict sync ordering after earlier offline docs;
- recovery-safe finalization to `ACK` / manual path.

### Step E — offline/day-close orchestration

Після цього реалізувати керований day-close contour, який підтримує offline close-of-day у повній моделі.

Він може бути оформлений як higher-level operator action:

- `close_day`

яка оркеструє:

- `Z_REPORT`
- backend-specific `SHIFT_CLOSE`

в потрібному порядку для конкретного transport/backend.

## 11. Final Recommendation

**Safest implementable interpretation (updated 2026-05-16 per §0 correction):**

- продукт повинен підтримати offline close-of-day;
- але `offline SHIFT_CLOSE` як окрема пряма transport-level операція не повинна відкриватися в поточній архітектурі;
- offline close-of-day моделюється через `Z_REPORT`-centric contour: **offline-mode local `Z_REPORT` close → Pattern C `OFFLINE_LOCAL_ACK` document** (consumes offline code, ordered after prior offline docs by `lnd`, drained later through online ladder);
- legal blockers застосовуються **тільки до ONLINE-варіантів** при pending offline backlog:
  - **online** `SHIFT_CLOSE` blocked → typed refusal + audit;
  - **online** `Z_REPORT` blocked → `ONLINE_Z_REPORT_BLOCKED_BACKLOG` audit;
  - **offline-mode local** `Z_REPORT` close-of-day, навпаки, MUST бути дозволений як Pattern C document — це **не** частина блокатора, це окрема routed-acceptance ARM (`OFFLINE_Z_REPORT_LOCAL_CLOSE_ACCEPTED`);
- post-local-close lockout: після того як offline `Z_REPORT` досяг `OfflineLocalAck`, нові sale / return документи MUST бути refused (`POST_LOCAL_CLOSE_SALE_REFUSED`) until next allowed shift-open policy satisfied;
- **hard close-code reserve = 1 offline code per active offline shift**: while a shift is open and the offline `Z_REPORT` has NOT yet been emitted, ordinary fiscal docs MUST NOT consume the last free offline code (refused with `OFFLINE_CODE_RESERVED_FOR_CLOSE` audit; code row stays `consumed_at IS NULL`); the offline `Z_REPORT` MAY consume it.  Without this reserve, ordinary docs could exhaust the pool before close-of-day and the Pattern C exit becomes inaccessible — re-asserting the 24h trap that this whole architecture eliminates.  This is NOT an operational refill watermark (`min_offline_codes`, commonly ~10) — it is the *last-line* legal guarantee, exactly 1 code, enforced by W10 regardless of the configured watermark;
- довести linkage між shift і close/day-close documents (`close_document_id` / `z_report_document_id`).

Це рішення мінімізує ризик юридично хибного `shift closed` без завершеного фіскального денного контуру AND гарантує compliant exit з offline shift'а проти 24h legal limit ("24h trap" mitigation — see §0 + `docs/LEGAL_INVARIANTS.md` §8).
