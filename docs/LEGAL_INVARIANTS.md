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

| Категорія | Статус |
|---|---|
| Single-writer / LND | ✅ Реалізовано і покрито тестами |
| Shift lifecycle guards | ✅ Реалізовано і покрито тестами |
| Channel lock enforcement | ✅ Реалізовано і покрито тестами |
| Idempotency | ✅ Реалізовано і покрито тестами |
| Offline time limits (36h / 168h) | ✅ Реалізовано і покрито тестами |
| Offline range allocation | ✅ Реалізовано і покрито тестами |
| Offline state model (OFFLINE_LOCAL vs DPS_ACK) | ❌ GAP — Sprint 1 |
| Offline sync service | ❌ GAP — Sprint 2 |
| Z-report / shift close blocking | ❌ GAP — Sprint 2+ |
| Crypto seam (passthrough/sidecar) | ✅ Реалізовано і покрито тестами |
| Production crypto startup gate | ✅ Реалізовано |
| Excise mark protection | ✅ Часткове — no fiscal validator |
| Ukrainian fiscal receipt validator | ❌ GAP — P0 |
| Real DPS transports | ❌ Stub — P1 |
| Recovery / reconciliation | ✅ Реалізовано і покрито тестами |
| Audit / trace | ✅ Реалізовано і покрито тестами |

---

*Цей документ фіксує стан на дату Sprint 0. Потребує оновлення після Sprint 1 та перед production release.*
