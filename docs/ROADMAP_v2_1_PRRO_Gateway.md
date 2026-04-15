# ROADMAP v2.1: Multi-Protocol PRRO Gateway

## Переработанная версия после архитектурной критики

**Назначение документа:** усилить roadmap v2 так, чтобы ранние этапы доказывали не только «живой happy path», но и инженерную состоятельность ядра под реальными рисками: crypto latency, restart/recovery, stateful ingress, single-writer concurrency и будущий offline contour.

**Базовая линия:** кодовая база `v1.4.1` + Checkbox real transport patch v3.  
**Текущая оценка:** архитектурное ядро сильное; главный риск — не отсутствие слоёв, а позднее вскрытие интеграционных и эксплуатационных дефектов.

---

## 1. Что изменилось по сравнению с v2

В v2.1 внесены пять принципиальных усилений:

1. **Crypto seam перенесён в ранний execution contour.**  
   Gate 1 допускает passthrough-провайдера, но только если worker уже работает через реальный `CryptoProvider` seam и умеет жить с latency / timeout / retry semantics.

2. **Offline не переносится целиком вперёд, но его seam фиксируется рано.**  
   Уже на ранних этапах worker обязан иметь явную точку выбора allocation strategy: online numbering path vs future offline numbering path.

3. **Добавлен Protocol Shape Audit для Maria / WebCheck.**  
   Рано проверяется, совместим ли текущий canonical ingress contract со stateful-протоколами и нужен ли session aggregation layer.

4. **Gate 1 усилен инженерными доказательствами.**  
   Недостаточно «open -> sell -> close». Обязательны Atomic Sale DB assertions, Restart & Recovery Smoke, Channel Lock Enforcement и Concurrency Smoke.

5. **Добавлен ранний stress/smoke для single-writer модели.**  
   До наращивания сложности проверяется, что базовая модель на SQLite и lease semantics не разваливаются под burst-нагрузкой.

---

## 2. Главный принцип v2.1

Roadmap разделён на две дорожки:

### Track A — Pilot MVP
Цель: как можно раньше получить **живой, безопасный и ограниченно эксплуатируемый pilot-ready contour**.

### Track B — Full MVP by TZ v1.2
Цель: довести систему до **формального полного соответствия acceptance matrix** из ТЗ.

**Ключевая мысль:**  
Track A не должен быть синтетическим demo-контуром.  
Он должен доказывать, что архитектурное ядро переживает реальные operational risks уже на раннем этапе.

---

## 3. Архитектурные аксиомы, которые управляют roadmap

1. **SQLite hot store остаётся source of truth.**
2. **Один `fiscal_number` = один логический single-writer write-path.**
3. **Никаких network/crypto вызовов внутри длинной SQLite-транзакции.**
4. **Idempotency, channel lock и recovery semantics не размываются ради скорости поставки.**
5. **Pilot MVP можно упростить по объёму, но нельзя фальсифицировать по критическим рискам.**
6. **Offline, crypto, stateful ingress и reconciliation должны иметь ранние seams, даже если их полный функционал закрывается позже.**
7. **Любой новый слой обязан доказывать не только happy path, но и сбойные сценарии.**

---

## 4. Gates v2.1

### Gate 0 — Baseline Confidence
Система и репозиторий подготовлены к управляемой разработке:
- CI зелёный;
- тесты разделены на `unit / integration / e2e`;
- baseline coverage snapshot зафиксирован;
- внешние зависимости заведены;
- current architecture map обновлён.

### Gate 1 — Live Online Core
Система доказывает жизнеспособность в реальном online-контуре:
- runtime ingress работает;
- real Checkbox transport работает;
- `open_shift -> sell -> poll -> close_shift` проходит;
- `CryptoProvider` seam уже реален по интерфейсу;
- worker имеет early seam для numbering/allocation strategy;
- idempotency и channel lock работают;
- trace/audit/basic archive сохраняются.

**Gate 1 не считается закрытым без четырёх обязательных проверок:**
1. **Atomic Sale DB assertions**
2. **Restart & Recovery Smoke**
3. **Channel Lock Enforcement**
4. **Concurrency Smoke (burst on one fiscal_number)**

### Gate 2 — Operational Safety
Система доказала эксплуатационную устойчивость:
- crypto sidecar integrated;
- timeout/retry/backoff/circuit breaker работают;
- rate limiting включён;
- metrics/health достаточны для pilot;
- graceful shutdown корректен;
- integrity/backup/corruption reaction присутствуют.

### Gate 3 — Offline Viability
Система доказала контролируемую работу вне сети:
- `GO_OFFLINE / ASK_OFFLINE_CODES / GO_ONLINE`;
- offline ranges / watermark logic;
- лимиты 36h / 168h;
- auto return online;
- reconnect reconcile;
- **LND sequence recovery after offline crash**.

### Gate 4 — Multi-Protocol Acceptance
Система доказала полноту MVP:
- XML-RPC и Maria listeners работают;
- unsupported methods/commands штатны;
- Cloud Hub outbox contour работает;
- rendering присутствует;
- acceptance matrix покрыта тестами;
- E2E из ТЗ проходят.

### Gate 5 — Phase 1.1 Readiness
Система готова к следующему контуру:
- deployment topology formalized;
- upgrade/rollback formalized;
- migration/import formalized;
- DPS transports и expanded metrics готовы;
- feature flags и расширенная concurrency policy есть.

---

## 5. Что считается настоящим Pilot MVP

### Обязательно входит
- REST runtime;
- runtime container;
- real Checkbox transport;
- real `CryptoProvider` seam;
- shift lifecycle;
- online sale / return / service operations;
- idempotency;
- channel lock;
- reconciliation для pending operations;
- health probes;
- graceful shutdown;
- basic traces/audit;
- document persistence / basic archive;
- минимальные технические метрики;
- restart/recovery smoke;
- concurrency smoke;
- ранний e2e vertical slice.

### Может быть отложено
- полный offline lifecycle;
- production-grade Cloud Hub;
- реальные XML-RPC / Maria listeners;
- полный rendering contour;
- полная acceptance matrix;
- DPS transports;
- import/migration/upgrade policy;
- весь Phase 1.1.

### Важно
Pilot MVP **не обязан быть полным**, но **обязан быть честным**:
- нельзя подменять реальные риски synthetic flow;
- нельзя закрывать gate только happy-path тестом.

---

## 6. Ранняя архитектурная проверка, которую нельзя пропускать

### 6.1 Protocol Shape Audit
До активной разработки multi-protocol ingress команда обязана ответить на вопросы:

- Maria и WebCheck — stateless или session-based?
- Требуют ли они накопления позиций между несколькими вызовами?
- Где должен происходить assembly финального `CanonicalFiscalCommand`?
- Не конфликтует ли текущая durable inbox модель с session-based ingress?
- Как обеспечивается idempotency для stateful frontend-сценариев?

**Выход артефакта:** `docs/PROTOCOL_SHAPE_AUDIT.md`

### 6.2 Allocation Strategy Seam
Уже в раннем worker должен существовать явный seam:
- `online_lnd_allocator`
- `offline_lnd_allocator` (может быть stub / not implemented)
- единая policy-точка выбора источника номера/режима документа

**Цель:** чтобы offline потом не вскрывал сердце write-path.

### 6.3 Crypto Async Semantics Harness
Даже если Gate 1 стартует с passthrough:
- provider должен вызываться через реальный `CryptoProvider` Protocol;
- должны быть тесты на artificial latency / timeout / temporary failure;
- write-path не должен зависеть от «мгновенной подписи».

---

## 7. Последовательность спринтов v2.1

## Sprint 0 — Подготовка и baseline confidence

**Цель:** убрать организационные и тестовые блокеры.  
**Длительность:** 1 неделя  
**Исполнители:** оба

### Задачи
1. CI: `unit + integration` на каждый PR, `e2e` на merge/nightly.
2. Реорганизация тестов по слоям.
3. Baseline architecture map.
4. Coverage snapshot и acceptance coverage map.
5. Cloud Hub draft contract.
6. Запросить Maria spec / pcap.
7. Запросить DPS test access.
8. Зафиксировать список открытых architectural unknowns.

### Артефакты
- `docs/ARCHITECTURE_BASELINE.md`
- `docs/ACCEPTANCE_COVERAGE_SNAPSHOT.md`
- `docs/CLOUD_HUB_API_DRAFT.md`

### Критерий завершения
- Gate 0 закрыт.

---

## Sprint 0.5 — Architecture Risk Audit

**Цель:** поймать архитектурные сюрпризы до coding-heavy этапов.  
**Длительность:** 3-4 дня  
**Исполнители:** Dev A + Dev B

### Задачи
1. Protocol Shape Audit для Maria/WebCheck.
2. Allocation Strategy Seam design note.
3. Crypto async semantics harness design.
4. Concurrency risk hypothesis review для SQLite single-writer модели.

### Артефакты
- `docs/PROTOCOL_SHAPE_AUDIT.md`
- `docs/ALLOCATION_STRATEGY_NOTE.md`
- `docs/CRYPTO_ASYNC_TEST_HARNESS.md`
- `docs/CONCURRENCY_RISK_NOTE.md`

### Критерий завершения
- нет иллюзий относительно stateful ingress;
- offline и crypto не будут «вскрывать» worker внезапно.

---

## Sprint 1 — Live Online Core

**Цель:** доказать, что продукт живой в реальном online-контуре.  
**Длительность:** 2 недели  
**Исполнители:** оба

### Задачи
1. Финализировать runtime wiring.
2. Закрепить real Checkbox transport.
3. Финализировать shift lifecycle.
4. Довести online operations (`SELL`, `RETURN`, `SERVICE_*`).
5. Ввести реальный `CryptoProvider` seam:
   - допускается passthrough implementation;
   - недопустим direct ad-hoc sign bypass без seam.
6. Ввести allocation strategy seam:
   - online implementation;
   - offline stub с чёткой точкой расширения.
7. Добавить basic archive / document persistence / trace.
8. Собрать ранний e2e vertical slice.

### Обязательные gate tests
#### 1. Atomic Sale
Проверяет:
- запись в `ingress_inbox`;
- lease acquisition;
- подготовку документа;
- корректное durable state before send;
- финальный `DONE / ACK`.

#### 2. Restart & Recovery Smoke
Проверяет:
- crash вблизи `transport.send`;
- restart;
- recovery этап 1;
- корректную reconcile судьбы документа;
- корректный readiness semantics.

#### 3. Channel Lock Enforcement
Проверяет:
- fast reject;
- authoritative reject на worker-уровне.

#### 4. Concurrency Smoke
Проверяет:
- 50-100 concurrent requests на один `fiscal_number`;
- отсутствие двойной фискализации;
- контролируемое FIFO / busy timeout поведение;
- наблюдаемость `SQLITE_BUSY`.

### Критерий завершения
- Gate 1 закрыт не только happy path, но и инженерными доказательствами.

---

## Sprint 2 — Crypto Sidecar + Resilience

**Цель:** сделать online contour эксплуатационно устойчивым.  
**Длительность:** 2 недели  
**Исполнитель:** Dev A

### Задачи
1. Реализовать `CryptoSidecarClient`.
2. Provider selection: `sidecar | passthrough`.
3. Timeout / retry / circuit breaker / healthcheck.
4. Worker semantics:
   - `ERROR_RETRYABLE`
   - requeue/backoff
   - `DEAD`
5. Transport retry policy.
6. Добавить тесты на latency / timeout / degraded crypto.

### Критерий завершения
- sidecar реально интегрирован;
- temporary failures не разрушают write-path.

---

## Sprint 3 — Pilot Observability & Edge Safety

**Цель:** дать минимально достаточную эксплуатационную безопасность.  
**Длительность:** 2 недели  
**Исполнитель:** Dev B

### Задачи
1. Basic metrics:
   - inbox backlog
   - pending docs
   - sign time
   - transport RTT
   - sqlite busy count
   - circuit breaker state
2. `/metrics`
3. ingress rate limiting
4. graceful shutdown hardening
5. minimal heartbeat/outbox stub:
   - необязательно production Cloud Hub;
   - желательно раннее наблюдение состояния кассы/узла.

### Критерий завершения
- pilot node наблюдаем;
- перегрузка и shutdown ведут себя предсказуемо.

---

## Sprint 4 — Storage Safety

**Цель:** не бояться хранилища на пилоте.  
**Длительность:** 2 недели  
**Исполнитель:** Dev B

### Задачи
1. periodic integrity check
2. online SQLite backup
3. corruption stop mode
4. retention/purge для traces/inbox
5. restore script + docs

### Критерий завершения
- Gate 2 закрыт полностью.

---

## Sprint 5 — Offline Core

**Цель:** добавить реальный offline lifecycle без вскрытия архитектуры.  
**Длительность:** 2 недели  
**Исполнитель:** Dev A

### Задачи
1. `GO_OFFLINE`
2. `GO_ONLINE`
3. `ASK_OFFLINE_CODES`
4. offline sessions / ranges / watermarks
5. offline limits 36h / 168h
6. reconnect auto-online
7. reconcile after reconnect
8. LND sequence recovery after offline crash

### Критерий завершения
- Gate 3 закрыт.

---

## Sprint 6 — Rendering + Cloud Contour Minimum

**Цель:** довести результат до более полного эксплуатационного контура.  
**Длительность:** 2 недели  
**Исполнитель:** Dev B

### Задачи
1. TEXT receipt
2. HTML receipt
3. formatted print lines
4. save render artifacts
5. Cloud Hub client minimum
6. Outbox sender
7. heartbeat
8. error mapping / retry handling

### Критерий завершения
- есть печатные артефакты;
- outbox/hub contour существует в рабочем минимуме.

---

## Sprint 7 — Real Servers and Multi-Protocol Ingress

**Цель:** довести входные контуры до полноты MVP.  
**Длительность:** 2 недели  
**Исполнители:** оба

### Задачи
1. XML-RPC server
2. Maria TCP server
3. unsupported method / command semantics
4. e2e infra для всех ingress
5. cold store integration

### Критерий завершения
- Gate 4 частично закрыт по ingress полноте.

---

## Sprint 8 — Acceptance Closure

**Цель:** формально закрыть MVP по ТЗ.  
**Длительность:** 2 недели  
**Исполнители:** оба

### Задачи
1. negative golden tests
2. acceptance matrix
3. mapping each criterion -> test
4. e2e regression pack
5. docs refresh

### Критерий завершения
- Gate 4 закрыт полностью.

---

## Sprint 9 — Stabilization Buffer

**Цель:** стабилизация после полного acceptance.  
**Длительность:** 2-3 недели  
**Исполнители:** оба

### Задачи
1. bug-fixing
2. performance bottlenecks after real tests
3. packaging polish
4. install/ops hardening

### Критерий завершения
- Full MVP by TZ можно считать закрытым без самообмана.

---

## Sprint 10-11 — Phase 1.1

### Направления
- deployment topology
- upgrade/rollback
- migration/import tooling
- DPS transports
- key inspection: read-only parse of uploaded key/container, password check, key identifier extraction, certificate identity display, and automatic certificate/chain pull for trust evaluation without storing the key
- PRRO onboarding from uploaded key: after explicit inspection, discover available fiscal numbers that the signer can service, then bind the selected PRRO
- scheduled signer binding: allow a saved key-to-PRRO bind to become active only within an explicit operator-defined date/time window, in addition to certificate validity
- multi-operator PRRO binding: allow multiple operators/signers to be bound to one PRRO, while still enforcing the fiscal-server rule that one open shift may have only one active signer
- operator role sync from DPS: pull cashier/senior-cashier metadata from `infoRro` so local operator UX reflects fiscal-server truth
- signer UX / ops hardening: key fingerprint visibility, preflight readiness, expiry warnings, no-signer-switch during open shift, cached DPS snapshots, and audit trail
- reliability diagnostics and DPS reconciliation: local SQLite integrity/state checks, explicit PRRO state comparison against DPS, and failure-pattern classification for DNS/TLS/gRPC/crypto/network incidents before any offline fallback
- cash transaction legal limit validation: block over-limit cash sales before signing/submission and show an explicit operator-facing legal/compliance error
- excise mark compliance validation: validate excise-mark format and reject duplicate excise marks locally before signing/submission, both within one document and across the local sales history
- tax group validation and mapping: validate allowed tax groups per item/document and handle combinations that include excise taxation correctly before signing/submission
- fiscal-policy groups scoped per PRRO/fiscal number: tax, excise, and UKTZED requirements should bind to the effective fiscal number configuration rather than one global system-wide catalog
- declaration-oriented excise grouping: support operator/accounting-friendly subgrouping such as vodka/wine/cigarettes on top of fiscal-policy groups for excise declaration preparation and reconciliation
- operational goods reports: local day/month goods reports for accounting/ops, distinct from fiscal Z/X reports and built from the local PRRO data model
- feature flags
- expanded business metrics
- extended concurrency policy

---

## 8. Acceptance rules for Gate 1

Gate 1 считается закрытым только если выполнены все проверки ниже.

### 8.1 Atomic Sale
- inbox durable write
- lease transition
- durable state before send
- correct document lifecycle
- final status is consistent

### 8.2 Restart & Recovery Smoke
- process can be killed near send/poll boundary
- on restart, stuck `PROCESSING` is reconciled correctly
- readiness semantics are deliberate, not accidental

### 8.3 Channel Lock Enforcement
- fast reject path tested
- authoritative worker path tested

### 8.4 Idempotency
- repeated request with same idempotency key does not create duplicate fiscalization

### 8.5 Schema Versioning
- canonical envelopes contain `schema_version`

### 8.6 Graceful Shutdown
- no dangling active lease after shutdown

### 8.7 Traceability
- protocol and transport trace exist for request lifecycle

### 8.8 Concurrency Smoke
- burst on one `fiscal_number` is observable and controlled

---

## 9. Риски v2.1

| Риск | Вероятность | Влияние | Митигация |
|------|-------------|---------|-----------|
| Synthetic Gate 1 без реального crypto seam | Средняя | Высокое | Ввести `CryptoProvider` seam в Sprint 1 |
| Offline вскроет worker слишком поздно | Средняя | Высокое | Ранний allocation strategy seam |
| Maria/WebCheck окажутся stateful и сломают current ingress assumptions | Высокая | Высокое | Protocol Shape Audit в Sprint 0.5 |
| Single-writer SQLite поведёт себя хуже ожиданий под burst | Средняя | Высокое | Early Concurrency Smoke |
| Recovery semantics окажутся слабее ожидаемого | Средняя | Высокое | Restart & Recovery Smoke в Gate 1 |
| Cloud contour затянется | Средняя | Среднее | Minimal heartbeat/outbox stub раньше полного Hub client |
| Crypto sidecar нестабилен | Средняя | Высокое | Passthrough only as seam-compatible fallback, not as architectural bypass |

---

## 10. Итоговая оценка сроков

### Для двух разработчиков
- **Pilot MVP:** ~9-11 недель
- **Full MVP by TZ:** ~20-24 недели
- **Phase 1.1:** ещё ~4-6 недель

### Для одного разработчика
Ориентир:
- Pilot MVP: ~14-17 недель
- Full MVP by TZ: ~30+ недель

### Управленческий вывод
Сроки надо считать не по optimistic coding path, а по пути, в котором:
- есть recovery bugs,
- есть integration unknowns,
- есть rework после первых stress/e2e прогонов.

---

## 11. Что не входит в roadmap

- полноценный cashier cabinet
- собственный acquiring gateway
- полноценная товароучётная система
- отраслевые спецрежимы beyond MVP
- mobile app
- heavy web UI

---

## 12. Краткий вывод

v2.1 делает roadmap не шире, а **честнее**.

Главное улучшение:
- мы не просто раньше доказываем live contour;
- мы раньше доказываем, что contour выдерживает реальные архитектурные риски.

Именно это снижает шанс позднего болезненного rework.
