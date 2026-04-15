# ROADMAP v3: Pilot-First PRRO Gateway

## Контекст

Roadmap v2.1 планувався для двох розробників, Checkbox-first з відкладеним DPS.
Реальність: один розробник + LLM, DPS-first contour вже доказаний live.

**Поточний стан (квітень 2026):**
- 435 тестів, all green
- Direct DPS gRPC contour працює: SHIFT_OPEN, SELL, RETURN, SERVICE_IN/OUT, Z_REPORT
- Live e2e: REST → write-path → sidecar crypto → DPS (cabinet.tax.gov.ua:9443)
- MAC auto-recovery, rate-limit classification, error handling
- 50 точок / 70 касс на WebCheck (retail + HoReCa) готові до міграції
- Доступ до JKS ключів та production DPS

**Цільова архітектура:** local PRRO core → direct DPS submission.
Checkbox — тільки ingress-сумісність.

---

## Gates (оновлені)

### Gate 0 — Baseline ✅ CLOSED
435 тестів, architecture docs, legal invariants.

### Gate 1 — Live Online Core ✅ CLOSED
Atomic sale, restart/recovery, channel lock, concurrency — все proven.

### Gate 2 — Operational Safety ⚠️ PARTIAL
- Crypto sidecar ✅ (JKS/DSTU via jkurwa)
- Timeout/circuit breaker ✅
- Rate limit classification ✅ (DPS status=-4 handling)
- Graceful shutdown ✅
- Missing: ingress rate limiting, backup/corruption stop mode, retention policy

### Gate 3 — DPS Fiscal Compliance ❌ NEW (was Offline)
Фіскальна коректність XML відповідно до ФСКО протоколу v2.2.3.

### Gate 4 — Offline Viability ❌ OPEN
Offline mode з усіма компонентами.

### Gate 5 — Production Readiness ❌ OPEN
Production endpoint, normal startup, monitoring.

---

## Спринти до пілоту

### Sprint 9 — Фіскальна коректність чеків (Gate 3) ✅ CLOSED

**Завершено:** 2026-04-14
**Тестів:** 491 (було 478)

#### Step 1: SERVICE_IN/SERVICE_OUT ✅

**Step 2: TX (податок) на товарах `<P>`**
- Додати атрибут `TX` до `<P>` element (позначення ставки ПДВ: 1, 2, 3...)
- Source: canonical payload `goods[].tax_group` або explicit mapping
- Adapter: передати tax_group з ingress
- Files: `dps_xml.py`, `checkbox_rest.py` adapter

**Step 3: Повний `<E>` (закриття чеку)**
- Додати: `NO` (lnd), `SM` (total_sum), `FN` (fiscal_number), `TS` (timestamp)
- Додати: `TX`, `TXPR`, `TXSM`, `DTPR`, `DTSM`, `TXTY`, `TXAL` — або вкладені `<TX>` теги
- Для SERVICE: тільки `N` (вже правильно)
- Files: `dps_xml.py`

**Step 4: Повний Z-звіт `<Z>`**
- Зараз: `<Z NO="1"/>` — порожній
- Потрібно: `<TXS>` (податки за зміну), `<M>` (оберти по оплатах), `<IO>` (service in/out totals), `<NC>` (кількість чеків)
- Агрегація даних з fiscal_documents за поточну зміну
- Files: `dps_xml.py`, `write_path.py` (query aggregation)

**Step 5: CZD (УКТЗЕД) та `<CA>` (акцизні марки)**
- CZD attribute на `<P>` — код підкатегорії УКТЗЕД
- `<CA>` sub-element в `<P>` — серія/номер акцизної марки
- Потрібно якщо на точках алкоголь/тютюн
- Files: `dps_xml.py`

**Acceptance criteria:**
- SELL/RETURN XML валідний по ФСКО (TX, повний E)
- Z-звіт містить TXS, M, IO, NC
- Акцизні марки сериалізуються (якщо applicable)

---

### Sprint 10 — Cash Balance та Shift Semantics

**Тривалість:** 3-5 днів
**Ціль:** коректна робота з залишком каси для POS-інтеграцій

**Step 1: Cash balance preserve/reset config**
- Configurable: `cash_balance_on_z_report: preserve | reset` (default: preserve)
- Залишок готівки = SERVICE_IN - SERVICE_OUT + SELL(cash) - RETURN(cash)
- Переноситься на наступну зміну або обнулюється
- Files: config.py, write_path.py, node_state

**Step 2: Shift close alignment**
- Перевірити що Z_REPORT → shift close → наступний SHIFT_OPEN працює коректно
- Залишок каси в SHIFT_OPEN `<O SM="...">` = залишок з попередньої зміни

**Acceptance criteria:**
- POS-система бачить правильний залишок після Z
- Configurable поведінка для різних POS

---

### Sprint 11 — Offline Foundation (Gate 4)

**Тривалість:** 1.5 тижні
**Ціль:** каса працює при відключенні інтернету

**Step 1: OFFLINE_LOCAL_ACK state fix**
- Offline документи повертають `OFFLINE_LOCAL_ACK`, не `ACK`
- Юридично коректний стан
- Files: write_path.py

**Step 2: GO_OFFLINE / GO_ONLINE (T=109/T=110)**
- XML service check для переходу online↔offline
- check_type = SERVICECHK(3)
- Files: dps_xml.py, transport, write_path

**Step 3: ASK_OFFLINE_CODES (T=112)**
- Запит діапазону офлайн-номерів у ДПС
- Response parsing → збереження в offline_ranges
- Files: dps_xml.py, transport, repositories

**Step 4: Offline sync live verification**
- OfflineSyncService: відправка накопичених offline документів після відновлення зв'язку
- Live test на test host
- Files: offline_sync.py, можливо мінімальні fix

**Step 5: Offline limits enforcement**
- 36 годин безперервно, 168 годин/місяць (вже є в тестах)
- Перевірити що працює в контексті DPS contour
- Files: write_path.py

**Acceptance criteria:**
- Каса переходить в offline при втраті зв'язку
- Чеки створюються з offline номерами
- Після відновлення — sync до ДПС
- Ліміти дотримуються

---

### Sprint 12 — Production Readiness (Gate 5)

**Тривалість:** 1 тиждень
**Ціль:** готовність до deploy на реальну точку

**Step 1: Production endpoint test**
- Один цикл на prro.tax.gov.ua:443 (не test host)
- SHIFT_OPEN → SELL → Z_REPORT

**Step 2: Normal startup path**
- run_rest.py працює з DPS config без manual seed injection
- Auto-seed profiles via config/migration

**Step 3: Мінімальний моніторинг**
- Логування в файл (structured JSON)
- Health endpoint polling script
- Basic alerting: DPS errors, crypto failures, shift state

**Step 4: Ops documentation**
- Інструкція встановлення
- Інструкція заміни ключа
- Інструкція перенесення на нову машину
- Troubleshooting guide

**Acceptance criteria:**
- Gateway запускається одним скриптом
- Оператор може побачити статус каси
- Проблеми видимі в логах

---

### Sprint 13 — Pilot на одній точці

**Тривалість:** 1 тиждень
**Ціль:** одна реальна каса працює через PRRO Gateway

**Phase A: Preflight (1-2 дні)**
- JKS з точки → sidecar → healthz
- statusRro/infoRro для fiscal number
- Порівняння: наш XML vs WebCheck XML для тих самих операцій

**Phase B: Shadow mode (1-2 дні)**
- Gateway працює паралельно з WebCheck
- Порівняння результатів без відправки в ДПС

**Phase C: Live switchover (1-2 дні)**
- Одна каса переключається на gateway
- Повний робочий день: SHIFT_OPEN → SERVICE_IN → sells → SERVICE_OUT → Z_REPORT
- Ручний моніторинг

**Phase D: Стабілізація**
- Edge cases
- Fix on fly

**Acceptance criteria:**
- Одна каса працює повний день через gateway
- Z-звіт закриває зміну коректно
- Оператор не бачить різниці в роботі

---

## Після пілоту

### Sprint 14+ — Масштабування
- Rollout на 5-10 точок
- Multi-FN support тестування
- Performance tuning

### Phase 1.1 — Advanced Features
- Key management UI
- Operator onboarding from DPS infoRro
- delLastChk / delLastChkId
- Multi-signer per PRRO
- Operational goods reports
- DPS Unified Window (другий contour)

---

## Оцінка термінів

| Sprint | Scope | Днів |
|--------|-------|------|
| 9 (фіскальна коректність) | TX, E, Z-звіт, акциз | 5-7 |
| 10 (cash balance) | Preserve/reset config | 3-4 |
| 11 (offline) | State fix, GO_OFFLINE/ONLINE, codes, sync | 7-10 |
| 12 (production) | Prod endpoint, startup, monitoring, docs | 5-7 |
| 13 (pilot) | Одна точка live | 5-7 |

**До першого пілоту: ~25-35 робочих днів (5-7 тижнів).**

Мінімальний пілот (без offline): Sprints 9-10-12-13 = ~18-25 днів (4-5 тижнів).

---

## Ризики

| Ризик | Вплив | Мітигація |
|-------|-------|-----------|
| Production DPS відрізняється від test host | Високий | Sprint 12 step 1 — ранній тест |
| Податкові групи різні на різних точках | Середній | Per-FN config в Sprint 9 |
| Offline sync має невідомі edge cases | Високий | Live verification в Sprint 11 |
| JKS ключі точок мають інший формат | Середній | Preflight в Sprint 13 |
| DPS змінить API | Низький | Протокол v2.2.3 стабільний |

---

## Що НЕ входить в цей roadmap

- Повний cashier cabinet / web UI
- Mobile app
- Товароучітна система
- Другий DPS contour (XML Unified Window)
- Cloud Hub
- Feature flags
- Heavy rendering (HTML/PDF чеки)
