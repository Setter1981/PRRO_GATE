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

**Engineering enforcement:** `UNIQUE INDEX uq_active_shift_per_fiscal ON shifts(fiscal_number) WHERE state NOT IN ('CLOSED','ERROR')`.  
**Порушення:** подвійна фіскалізація, неузгодженість контрольної стрічки.

### INV-05 — Зміна каналу під час активної зміни заборонена
Перемикання між `DPS_UNIFIED_WINDOW` і `DPS_PRRO_FISCAL_SERVER` під час відкритої зміни заборонено безумовно.

**Engineering enforcement:** channel lock через `backend_profile_id + transport_profile_id + protocol + integration_owner`; перевірка в write-path guard stage.  
**Порушення:** фіскальні документи в одній зміні через різні канали — юридично недійсно.

### INV-06 — Failover між DPS-каналами тільки поза активною зміною
Failover між `DPS_UNIFIED_WINDOW` і `DPS_PRRO_FISCAL_SERVER` дозволений тільки: поза активною зміною, або після контрольованого закриття/відкриття зміни з явним рішенням оператора, аудит-подією та доказом ідемпотентності.

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

**Engineering enforcement:** `WritePathWorker.MAX_OFFLINE_CONTINUOUS_SECONDS = 36 * 3600`; `_check_offline_limits()` в write-path.  
**Джерело:** Наказ МФ №317.

### INV-10 — Офлайн не більше 168 годин на календарний місяць
Накопичений офлайн-час за поточний місяць не може перевищувати 168 годин.

**Engineering enforcement:** `WritePathWorker.MAX_OFFLINE_MONTH_SECONDS = 168 * 3600`; `current_month_offline_seconds` в `node_state`.  
**Джерело:** Наказ МФ №317.

### INV-11 — Офлайн-операція вимагає попередньо виданого діапазону фіскальних номерів
Без активного `offline_ranges` запис не може бути присвоєно.

**Engineering enforcement:** `_allocate_offline_fiscal_no()` в write-path перевіряє активний діапазон; повертає помилку при відсутності.

### INV-12 — Один офлайн-номер — один електронний документ
Офлайн-фіскальний номер із `offline_ranges` не може бути використаний для двох документів.

**Engineering enforcement:** `UNIQUE INDEX uq_fiscal_documents_offline_no ON fiscal_documents(offline_fiscal_no) WHERE offline_fiscal_no IS NOT NULL`; атомарне інкрементування `next_fiscal_no` під `BEGIN IMMEDIATE`.

### INV-13 — Офлайн-чек не є фінальним підтвердженням DPS до передачі та ACK
Локально створений офлайн-документ є тимчасовим. Він стає фіскально легітимним тільки після отримання ACK від DPS.

**Engineering enforcement:** **КРИТИЧНИЙ GAP** — зараз офлайн-документ повертається з `document_state=ACK`, що неправильно. Потрібні стани `OFFLINE_LOCAL_ACK → OFFLINE_PENDING_SYNC → DPS_ACK`. Виправлення в Sprint 1.

### INV-14 — Офлайн-документи зберігаються локально до підтвердження DPS
Документи в стані `OFFLINE_LOCAL_ACK` або `OFFLINE_PENDING_SYNC` не повинні видалятись або архівуватись до отримання `DPS_ACK`.

**Engineering enforcement:** **GAP** — `OfflineSyncService` не реалізований. Виправлення в Sprint 2.

### INV-15 — Z-звіт / зміна не може закритись при наявності непереданих офлайн-документів
Фіскальний денний звіт та закриття зміни повинні блокуватись при наявності офлайн-документів без DPS-підтвердження.

**Engineering enforcement:** **GAP** — shift close guard не перевіряє `OFFLINE_PENDING_SYNC` документи. Виправлення в Sprint 2+.

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

**Engineering enforcement:** `ReconciliationService`; стани `ERROR_RETRYABLE`, `REQUIRES_MANUAL_RECONCILIATION`; admin retry endpoint; recovery ceiling.

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
| **Hard close-code reserve = 1** | ⚠ M3b W10 rule (2026-05-16) — while a shift is open and the offline `Z_REPORT` has NOT yet been emitted, ordinary offline `SELL` / `RETURN` / `SERVICE_*` docs MUST NOT consume the last free offline code (refused with `OFFLINE_CODE_RESERVED_FOR_CLOSE` audit; code row stays unconsumed).  The offline `Z_REPORT` close-of-day MAY consume the reserved code.  **Hard reserve is exactly 1** — it is the *last-line* legal guarantee that the offline Z_REPORT close path always has a code while a shift is open, NOT an operational refill watermark.  The operational watermark (`min_offline_codes`, commonly ~10) sits well above 1 and triggers refill *before* exhaustion; it is a recommendation, not the legal reserve.  pool=0 at close time → `OFFLINE_Z_REPORT_LOCAL_CLOSE_REFUSED` with `reason: "code_pool_exhausted"`; pilot-critical signal that the operational watermark failed upstream.  Without this reserve, ordinary docs could exhaust the pool before close-of-day, leaving the offline Z_REPORT path empty and re-asserting the 24h trap.  W10 implementation pending. |
| Crypto seam (passthrough/sidecar) | ✅ Реалізовано і покрито тестами |
| Production crypto startup gate | ✅ Реалізовано (M3a) |
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
