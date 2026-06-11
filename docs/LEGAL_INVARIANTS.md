# LEGAL_INVARIANTS.md

## Multi-Protocol PRRO Gateway — Юридико-Інженерні Інваріанти

**Версія:** Sprint 0 snapshot, 2026-04-11  
**Статус:** Потребує підтвердження з боку продукту та юридичного власника перед виходом у production.  
**Джерело:** Закон України №265/95-ВР, накази МФ №317, №13, №1057; `docs/PROJECT_DOCUMENTATION_AND_SPRINT_PLAN.md` §3.

---

## 1. Інваріанти єдиного запису (Single-Writer / Fiscal Number)

### INV-01 — Один writer на fiscal_number
Один `fiscal_number` повинен мати один логічний single-writer write-path у будь-який момент часу.

**Engineering enforcement:** lease-based single-writer в `WritePathWorker`; `BEGIN IMMEDIATE` на критичних write-секціях.  
**Порушення:** дублікати фіскальних документів, пошкодження LND-послідовності.

### INV-02 — LND-послідовність є суворо зростаючою
Local Document Number повинен збільшуватись атомарно та без пропусків у межах однієї фіскальної реєстраційної точки.

**Engineering enforcement:** `increment_lnd` в `NodeStateRepository` під `BEGIN IMMEDIATE`.  
**Порушення:** фіскальна відмова DPS, некоректна контрольна стрічка.

---

## 2. Інваріанти зміни (Shift Lifecycle)

### INV-03 — Зміна повинна бути відкрита перед фіскальними операціями
Продаж, повернення, службові операції заборонені без відкритої зміни.

**Engineering enforcement:** `_check_shift_state()` в write-path guard stage.  
**Порушення:** незареєстрована операція, некоректний фіскальний чек.

### INV-04 — Не може бути двох активних змін для одного fiscal_number
Спроба відкрити другу зміну при вже відкритій повинна бути відхилена.

**Engineering enforcement (M3b 9-state):** `UNIQUE INDEX uq_active_shift_per_fiscal ON shifts(fiscal_number) WHERE state NOT IN ('CLOSED','ERROR','REQUIRES_MANUAL_RECONCILIATION')`. Partial index whitelist excludes all terminal states; active in-progress states (`OPENING`, `OPENED_LOCAL_PENDING_DRAIN`, `OPENED`, `CLOSING_LOCAL_PENDING_DRAIN`, `CLOSING`) are covered by the index — only one row in any active state per fiscal_number permitted.  
Cross-ref: M3b shift state expansion spec §3.1 (9-state enum) + §5.6 (`(ShiftOpen, *active*) → ✗-ShiftAlreadyOpen`) for the runtime guard layered on top of the partial index.  
**Порушення:** подвійна фіскалізація, неузгодженість контрольної стрічки.

### INV-05 — Зміна каналу під час активної зміни заборонена
Перемикання між DPS-каналами під час відкритої зміни заборонено безумовно.  Це поширюється на обидва канали, які підтримує DPS:
- **WebCheck / gRPC channel** (target M3a + M3b W7/W8/W9a in-scope).
- **DFS HTTP / XML channel** (`/fs/cmd` + `/fs/doc` + `/fs/pck` per `PRRODPS.DFS`; future implementation, NOT in Rust M3b).

Once a shift is opened against one channel family, the channel is **pinned** for that shift until `Z_REPORT` close-of-day completes AND any offline backlog from that shift drains to final ACK on that same channel.  Mid-shift switching would lose forensic continuity: offline numbers (code-pool vs `OfflineSessionId.localOfflineNum.controlNumber`), ticket shapes (`lastChk` vs DFS XML ticket), and KVT2 evidence formats are channel-specific.  See plan §"DPS Channel Taxonomy" for the channel comparison.

`Maria 304` is NOT a DPS channel — it is an ingress / POS adapter on the same boundary as REST / XML-RPC / Maria-TCP shells.  INV-05 governs the DPS-side channel pinned to the open shift, NOT which ingress accepts the POS message.

**Engineering enforcement:** channel lock через `backend_profile_id + transport_profile_id + protocol + integration_owner`; перевірка в write-path guard stage.  
**Порушення:** фіскальні документи в одній зміні через різні канали — юридично недійсно.

### INV-06 — Failover між DPS-каналами тільки поза активною зміною
Failover між DPS-каналами (WebCheck/gRPC ↔ DFS HTTP/XML) дозволений тільки: поза активною зміною, або після контрольованого закриття/відкриття зміни з явним рішенням оператора, аудит-подією та доказом ідемпотентності.  Operationally this means: close the current shift on the existing channel (online `Z_REPORT` if backlog clean, offline local `Z_REPORT` close + drain to final ACK if backlog pending) BEFORE the next `SHIFT_OPEN` on the new channel.

**Engineering enforcement:** зараз не реалізований явно — gap (див. ACCEPTANCE_COVERAGE_SNAPSHOT.md INV-06-GAP).

---

## 3. Інваріанти ідемпотентності

### INV-07 — Ідемпотентність обов'язкова
Одна бізнес-операція не повинна створювати два фіскальних документи навіть при дублюванні запитів, збоях мережі або повторних надсиланнях.

**Engineering enforcement:** унікальний `idempotency_key` в `ingress_inbox`; повторна відповідь повертає existing документ без повторної фіскалізації.  
**Порушення:** подвійна фіскалізація — серйозне правове порушення.

---

## 4. Інваріанти офлайн-режиму

### INV-08 — Офлайн дозволений тільки при недоступності фіскального сервера
Перехід в офлайн-режим повинен бути обумовлений неможливістю зв'язку з DPS/Checkbox.

**Engineering enforcement:** переключення режиму вручну через `NodeStateRepository.update_mode()`; автоматичний GO_OFFLINE за транспортними помилками — **stub, не повністю реалізований**.

### INV-09 — Офлайн-тривалість не більше 36 годин безперервно
Безперервна тривалість офлайн-сесії не може перевищувати 36 годин.

**Engineering enforcement (M3b):** після 36 годин **безперервної офлайн-сесії** (continuous, not cumulative — cumulative monthly cap is INV-10 / 168h) для FN, ingress відхиляє всі нові документи (`OFFLINE_LIMIT_EXCEEDED_INGRESS_REFUSED` Critical audit + operator pager per M3b shift state expansion spec §16.5). Threshold пов'язаний з `node_state.offline_session_started_at + 36h < now`. Існуючі OFFLINE_LOCAL_ACK документи зберігаються; drain виконується при поверненні online. Це **НЕ** Manual recon trigger — просто freeze ingress для FN.

Synergy з 36h SHIFT_OPEN cert gate (§16.10): cert validates at SHIFT_OPEN з > 36h до expiry → offline session max 36h → cert не може expire mid-offline by design.

**Джерело:** Наказ МФ №317.

### INV-10 — Офлайн не більше 168 годин на календарний місяць
Накопичений офлайн-час за поточний місяць не може перевищувати 168 годин.

**Engineering enforcement (M3b):** `WritePathWorker.MAX_OFFLINE_MONTH_SECONDS = 168 * 3600`; `current_month_offline_seconds` в `node_state`.

**Practice note (per 4-year operator empirics):** DPS server **does NOT** in practice return `Server -11` (168h cumulative limit) — обмеження існує в регуляціях але не enforced програмно DPS endpoint'ом з яким operator взаємодіяв. Defensive gateway-side enforcement remains as safety floor; landing in `Blocked` state if hit. Cross-ref M3b spec §16.15.

**Джерело:** Наказ МФ №317.

### INV-11 — Офлайн-операція вимагає попередньо виданого діапазону фіскальних номерів
Без активного `offline_ranges` запис не може бути присвоєно.

**Engineering enforcement:** `_allocate_offline_fiscal_no()` в write-path перевіряє активний діапазон; повертає помилку при відсутності.

### INV-12 — Один офлайн-номер — один електронний документ
Офлайн-фіскальний номер із `offline_ranges` не може бути використаний для двох документів.

**Engineering enforcement:** `UNIQUE INDEX uq_fiscal_documents_offline_no ON fiscal_documents(offline_fiscal_no) WHERE offline_fiscal_no IS NOT NULL`; атомарне інкрементування `next_fiscal_no` під `BEGIN IMMEDIATE`.

### INV-13 — Офлайн-чек не є фінальним підтвердженням DPS до передачі та ACK
Локально створений офлайн-документ є тимчасовим. Він стає фіскально легітимним тільки після отримання ACK від DPS.

**Engineering enforcement (M3b):** Pattern C state machine — `OFFLINE_LOCAL_ACK → Sending → Sent → Kvt1 → Kvt2 → Ack`. Документ зберігається в `fiscal_documents` з state `OFFLINE_LOCAL_ACK` (customer-facing receipt issued); drain виконується пізніше через W9b backlog drain + W12 KVT2 confirmation. Підпис застосовується **at drain time, NOT at ingress** (validated проти WebCheck decompiled + Python adapter — cross-ref M3b spec §16.11).

**Round 8 architectural pin (§16.1):** `fiscal_documents` = ledger of issued receipts only. OFFLINE_LOCAL_ACK документи там legitimately persisted because customer has physical receipt; failed online attempts (DPS rejection) → audit_log only, NOT persisted.

### INV-14 — Офлайн-документи зберігаються локально до підтвердження DPS
Документи в стані `OFFLINE_LOCAL_ACK` не повинні видалятись або архівуватись до отримання `Ack` через drain pipeline.

**Engineering enforcement (M3b):** W9b backlog drain orchestrator (per M3b plan §Task 9, BlockedBy W7 + W8) виконує drain в lnd ASC порядку через W9a-widened stage_send pipeline. Документи переходять `OFFLINE_LOCAL_ACK → Sending → Sent → Kvt1 → Kvt2 → Ack`. Archive / cleanup тільки після final Ack.

### INV-15 — Z-звіт / зміна не може закритись при наявності непереданих офлайн-документів
Online Z_REPORT blocked while offline backlog pending. Offline-mode Z_REPORT (Pattern C local close) is the alternative escape hatch.

**Engineering enforcement (M3b):** W10 policy guard (per M3b plan §Task 10 — W10a primitive + W10b ingress wiring) перевіряє offline backlog state перед routing Z_REPORT. Online Z_REPORT з non-empty OFFLINE_LOCAL_ACK backlog → refuse з `OFFLINE_Z_REPORT_BACKLOG_DRAIN_PENDING_REFUSED` audit (per PR #62 policy correction). Offline Z_REPORT через Pattern C → ClosingLocalPendingDrain state → drain через W9b → final Closed (cross-ref M3b spec §4.1 edge 13).

Online ops resume only after FULL drain completes for FN (per M3b spec §3.3 online-ops-resume rule) — count mismatch on Z_REPORT is impossible by design (§16.14).

---

## 5. Інваріанти акцизних товарів

### INV-16 — Акцизні товари повинні містити УКТЗЕД та акцизну марку
Товари, що підлягають акцизу, зобов'язані нести УКТЗЕД та акцизний код, коли це передбачено законодавством.

**Engineering enforcement:** `excise_marks` таблиця; захист від дублювання `UNIQUE INDEX uq_excise_mark_active`; адаптер будує повний canonical payload.  
**Джерело:** Наказ МФ №13.

---

## 6. Крипто-інваріанти

### INV-17 — Production-режим не може використовувати passthrough підпис або mock-транспорти
В production конфігурації passthrough `CryptoProvider` і mock-транспорти заборонені.

**Engineering enforcement:** `PassthroughCryptoProvider` доступний тільки якщо `crypto.provider=passthrough`; **GAP** — стартовий блокер для production конфігурації не реалізований. Виправлення в A2/A4.

### INV-18 — Мережеві та крипто-виклики заборонені всередині довгих SQLite-транзакцій
`sign()` та transport-виклики повинні виконуватись поза `BEGIN...COMMIT` блоками.

**Engineering enforcement:** write-path pipeline: sign → transport → finalize. Sign і transport виконуються між транзакціями.

---

## 7. Аудит та відновлення

### INV-19 — Кожен перехід стану повинен бути відновлюваним або явно позначеним для ручної звірки
Жоден документ не повинен застрягати в невизначеному стані без діагностики та recovery-шляху.

**Engineering enforcement (M3b expanded taxonomy per spec §16.3):** замість бінарного `ERROR_RETRYABLE` / `REQUIRES_MANUAL_RECONCILIATION` — повна taxonomy з 5 recovery classes:
- **`AutoOfflineFallback`** — unknown DPS errors → auto-switch to OFFLINE + tech support notification (NOT Manual recon)
- **`TechSupportEscalation`** — hard rejects що пройшли ingress validation → hold for tech support triage
- **`KeyRotationPending`** — cashier cert near expiry → refuse SHIFT_OPEN OR auto-swap to deferred key (per §16.10 36h gate)
- **`MacReseedRecovery`** — MAC chain desync → auto-fetch DPS anchor via probe doc (WebCheck pattern)
- **`TechSupportRepair`** — boot-time mirror breach → audited manual DB repair seam (NOT Manual recon)

Manual recon ("ЧП из ЧП" per `feedback_manual_recon_catastrophe` memory + spec §3.5) **confirmed trigger families** (per spec §16.7):
- **(1) Any W9b drain reject of an `OFFLINE_LOCAL_ACK` backlog doc** on `OpenedLocalPendingDrain` / `ClosingLocalPendingDrain` → `RequiresManualReconciliation` per §6.3 universal `EscalateManual` rule + edges 6 / 14. This is the **primary Manual recon surface** because drain has crossed the local-commit threshold (customer-facing receipt outstanding); rollback semantics don't apply regardless of underlying `DpsError` class. **FN deregistered while offline** is the operator-confirmed real-world subtype (Case 10).
- **(2) Ambiguous wire timeout** on online `SHIFT_OPEN` (edge 4) or online `Z_REPORT` (edge 12) — we sent the lifecycle doc but got no response, cannot determine if DPS accepted.
- **(3) Operator-driven force seam** invocation declaring shift unsalvageable based on out-of-band context (e.g. confirmed week-long DPS maintenance).

Every Manual landing → Critical audit + forensic snapshot capture (§8.1) + ≤60s out-of-band operator pager (§8.2).

### INV-20 — Канал подання чека є частиною фіскального маршруту і повинен бути в аудиті
Кожен фіскальний документ повинен мати записаний `submission_channel`, `backend_profile_id`, `transport_profile_id`.

**Engineering enforcement:** `transport_trace_log`; `audit_log`; `fiscal_documents` зберігає profile references.

---

## 8. Статус відносно production

> **Status correction 2026-05-16 (Rust gateway M3b context).**  The original table below described the Python-era status snapshot at the time of Sprint 0.  Several rows are misleading for the Rust gateway pilot path: the Rust gateway is being built standalone (the Python path remains the production gateway today; the Rust gateway has not yet shipped) and does not yet implement the offline time-limit enforcement that the Python row claims.  The corrected status column below uses ⚠ for **active engineering risks / pilot gates** that the Rust gateway must address before production, alongside ✅ / ❌ for items unchanged.

| Категорія | Статус (Rust gateway M3b, 2026-05-16) |
|---|---|
| Single-writer / LND | ✅ Реалізовано і покрито тестами (M3a + M3b W2) |
| Shift lifecycle guards | ✅ Реалізовано і покрито тестами |
| Channel lock enforcement | ✅ Реалізовано і покрито тестами |
| Idempotency | ✅ Реалізовано і покрито тестами |
| **24h shift limit** | ⚠ **Active engineering risk** — not yet enforced in the Rust gateway; must be enforced before production OR explicitly risk-accepted with a sign-off in the pilot log.  The offline Z_REPORT local close-of-day path (M3b W10) exists precisely so this limit has a compliant exit even when DPS is unreachable — without it the system would trap an offline shift against the 24h wall. |
| **36h continuous offline limit** | ⚠ **Active engineering risk** — Python-era enforcement (the original ✅ row) does NOT apply to the Rust gateway, which is being built standalone.  Must be enforced before production OR explicitly risk-accepted.  Sales may be blocked at the limit; the close/reporting path must always have an exit (offline Z_REPORT local close). |
| **168h monthly offline limit** | ⚠ **Active engineering risk** — same shape as 36h.  Must be enforced before production OR explicitly risk-accepted. |
| Offline range allocation | ✅ Реалізовано і покрито тестами (M3b W4 + W5) |
| Offline state model (`OfflineLocalAck` typed state) | ✅ Реалізовано (M3b W4 + W6 + W7) |
| Offline sync service (W9 backlog drain) | ⚠ In progress — M3b W9a merged (`stage_send` widened for OfflineLocalAck source); W9b backlog drain orchestration + W12 KVT2 confirmation pending |
| **Z-report / shift close policy** | ⚠ M3b W10 redesigned (2026-05-16) — **ONLINE Z_REPORT** over pending offline backlog MUST be blocked; **OFFLINE-mode local Z_REPORT** close-of-day MUST be allowed as Pattern C document (consumes offline code, lands `OfflineLocalAck`, drained later in `lnd` order).  Earlier blanket-block framing was an error — see `docs/OFFLINE_SHIFT_CLOSE_DECISION.md` §0.  W10 implementation pending. |
| **Hard close-code reserve = 1** | ⚠ M3b W10 rule (2026-05-16) — reserve = one currently-unconsumed code in the FN-scoped pool (`offline_codes` PK `(fiscal_number, code_lnd)`) while that FN has an open shift and the local offline `Z_REPORT` has NOT yet been emitted; ordinary offline `SELL` / `RETURN` / `SERVICE_*` docs MUST NOT consume that last `consumed_at IS NULL` row (refused with `OFFLINE_CODE_RESERVED_FOR_CLOSE` audit; code row stays unconsumed).  The offline `Z_REPORT` close-of-day MAY consume the reserved code.  **Hard reserve is exactly 1** — it is the *last-line* legal guarantee that the offline Z_REPORT close path always has a code while a shift is open, NOT an operational refill watermark.  The operational watermark (`min_offline_codes`, commonly ~10) sits well above 1 and triggers refill *before* exhaustion; it is a recommendation, not the legal reserve.  pool=0 at close time → `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED` with `reason: "code_pool_exhausted"` and **severity Critical** (NOT Warning — the 24h shift-limit trap is functionally re-asserted for the FN; audit dashboards must surface this immediately); pilot-critical / legal-critical signal that the operational watermark failed upstream.  Without this reserve, ordinary docs could exhaust the pool before close-of-day, leaving the offline Z_REPORT path empty and re-asserting the 24h trap.  **Reserve shape is channel-specific** (see plan §"DPS Channel Taxonomy"): WebCheck/gRPC = one row in `offline_codes` left `consumed_at IS NULL`; DFS HTTP/XML = one offline local ordinal / control-number slot in the `OfflineSessionId.localOfflineNum.controlNumber` derivation.  M3b W10 implements the WebCheck variant; the audit vocabulary is channel-neutral.  W10 implementation pending. |
| **X-report read-only** | ✅ Invariant (Rust gateway, 2026-05-16) — `X_REPORT` is a mid-shift / cash-drawer **operational report**, NOT a fiscal close-of-day document.  The Rust gateway MUST NOT sign, transport, persist as `fiscal_documents`, advance `lnd`, consume an offline code (WebCheck channel) or an offline local ordinal (DFS channel), or allocate a Z-report sequence number for an `X_REPORT` request.  W10 policy does NOT block `X_REPORT` on offline backlog — it is a no-fiscal-side-effect read; if backlog exists the response MAY carry a warning / forensic note but MUST NOT mutate fiscal state.  Consistent with the WebCheck reverse-engineering finding (X-report not signed/submitted) and with the reference DFS dispatcher (`PRRODPS/Maria/Session/MariaDispatcher.cs::ZREP → X-report`, no `/fs/doc` post).  `Z_REPORT`, in contrast, IS the fiscal close-of-day document and MAY be the offline local close — see "Z-report / shift close policy" row above. |
| Crypto seam (passthrough/sidecar) | ✅ Реалізовано і покрито тестами |
| Production crypto startup gate | ⚠ GAP — стартовий блокер для production конфігурації не реалізований (cross-ref INV-17 body); виправлення в A2/A4 |
| Excise mark protection | ✅ Часткове — no fiscal validator |
| Ukrainian fiscal receipt validator | ❌ GAP — P0 |
| Real DPS transports | ⚠ M3a wire-send + W7b dispatcher live on test DPS; production-channel selection (direct DPS vs WebCheck-compatible) pending runtime-composition task |
| Recovery / reconciliation | ✅ Реалізовано і покрито тестами (M3a + M3b W2) |
| Audit / trace | ✅ Реалізовано і покрито тестами |

**Compliance gate (production-ready criterion).**  The Rust gateway MUST NOT be declared production-compliant until:
1. 24h shift limit is enforced OR explicitly risk-accepted with operator sign-off.
2. 36h continuous offline limit is enforced OR explicitly risk-accepted.
3. 168h monthly offline limit is enforced OR explicitly risk-accepted.
4. M3b W10 ONLINE-vs-OFFLINE Z-report policy is implemented + pilot-tested (Phase 6 in `docs/PILOT_ACCEPTANCE_TEST_PLAN.md` covers both paths).
5. M3b W10 hard close-code reserve = 1 is implemented + pilot-tested (pool=1 sale refused, pool=1 Z_REPORT accepted, pool=0 Z_REPORT refused with `code_pool_exhausted`).
6. M3b W9b backlog drain + W12 KVT2 confirmation deliver every offline doc to final DPS `ACK`.

The offline Z_REPORT local close-of-day path is the architectural answer to the "24h trap": without it, an offline shift would have no compliant way to close at the 24h wall.  With it, the cash desk keeps operating (and reporting) even during DPS outages, with sync to final ACK on return-online.

---

*Цей документ фіксує стан на дату Sprint 0 (Python-era baseline), оновлений 2026-05-16 для Rust gateway M3b context.  Потребує наступного оновлення після M3b W10 / W9b / W12 landing та перед production release.*
