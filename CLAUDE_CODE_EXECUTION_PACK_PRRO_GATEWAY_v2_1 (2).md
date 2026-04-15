# Claude Code Execution Pack v2.1 — Multi-Protocol PRRO Gateway

## Назначение

Этот документ — обновлённый execution pack для Claude Code.

Он согласован с roadmap v2.1 и усиливает ранние этапы разработки по четырём направлениям:
1. реальный `CryptoProvider` seam уже в Gate 1;
2. ранний seam для online/offline allocation strategy;
3. обязательный Protocol Shape Audit для Maria / WebCheck;
4. Gate 1 закрывается только после recovery/channel-lock/concurrency доказательств, а не по happy path.

Человек в контуре: **ChatGPT принимает промежуточные результаты, ревьюит их, критикует и разрешает следующий bounded step.**

---

## 1. Базовая установка

Работаем от baseline:
- архив: `prro_gateway_dev_pack_v1_4_1.zip`
- текущая линия: Checkbox transport patches + runtime wiring + shift lifecycle
- продукт: локальный PRRO gateway для Украины

Не считать проект пустым.  
Не перепридумывать фундаментальные решения.

---

## 2. Абсолютные инварианты

1. **SQLite hot store — source of truth.**
2. **Один `fiscal_number` = один логический single-writer write-path.**
3. **Никаких network/crypto внутри длинной SQLite-транзакции.**
4. **Idempotency обязательна.**
5. **Channel lock обязателен.**
6. **Offline path обязан уважать лимиты и состояние смены.**
7. **Canonical envelopes обязаны содержать `schema_version`.**
8. **Adapters обязаны строить полный canonical payload.**
9. **Transport/reconciliation не должны ломать state transitions.**
10. **Graceful shutdown обязателен.**
11. **Никакого broad refactor ради красоты.**
12. **Pilot MVP не может быть synthetic в критических местах.**

---

## 3. Что изменилось в рабочей стратегии

Теперь недостаточно просто «сделать живой online contour».

Claude Code обязан проверять:
- не synthetic ли это contour;
- не скрыты ли реальные проблемы в crypto seam;
- не зашита ли future offline-логика в сердце worker;
- совместим ли ingress contract с stateful protocols;
- выдерживает ли single-writer burst-конкуренцию.

---

## 4. Gates

### Gate 0 — Baseline Confidence
Должны быть:
- CI / test layers;
- baseline architecture map;
- acceptance coverage snapshot.

### Gate 1 — Live Online Core
Должны быть:
- runtime ingress;
- real Checkbox transport;
- `open_shift -> sell -> poll -> close_shift`;
- реальный `CryptoProvider` seam;
- allocation strategy seam;
- traces/audit/basic archive.

**Gate 1 не закрыт без:**
- Atomic Sale DB assertions
- Restart & Recovery Smoke
- Channel Lock Enforcement
- Concurrency Smoke

### Gate 2 — Operational Safety
Должны быть:
- crypto sidecar;
- retry/backoff/circuit breaker;
- rate limiting;
- metrics;
- graceful shutdown;
- integrity/backup/corruption reaction.

### Gate 3 — Offline Viability
Должны быть:
- `GO_OFFLINE`
- `ASK_OFFLINE_CODES`
- `GO_ONLINE`
- offline ranges / limits / watermarks
- reconnect reconcile
- LND sequence recovery after offline crash

### Gate 4 — Multi-Protocol Acceptance
Должны быть:
- XML-RPC / Maria listeners
- unsupported method semantics
- Cloud/outbox contour
- rendering
- acceptance matrix
- E2E pack из ТЗ

---

## 5. Новые обязательные ранние задачи

### 5.1 Protocol Shape Audit
До глубоких работ по XML-RPC / Maria Claude Code обязан провести аудит:

Вопросы:
- Stateless или session-based?
- Нужна ли сборка позиций между вызовами?
- На каком уровне рождается финальный `CanonicalFiscalCommand`?
- Не конфликтует ли session ingress с durable inbox / idempotency?
- Как будет выглядеть session aggregation boundary?

**Выход:** `docs/PROTOCOL_SHAPE_AUDIT.md`

### 5.2 Allocation Strategy Seam
Уже в раннем worker должен быть явный seam:
- online allocator
- offline allocator stub
- unified policy point for numbering decision

### 5.3 Crypto Async Semantics
Даже если используется passthrough:
- он должен стоять за `CryptoProvider` Protocol;
- должны быть тесты на latency / timeout / temporary failure;
- pipeline не должен зависеть от zero-latency sign.

### 5.4 Concurrency Smoke
Нужен ранний bounded stress:
- 50-100 concurrent requests on one `fiscal_number`
- нет duplicate fiscalization
- наблюдаемый FIFO / busy timeout / sqlite busy signals

---

## 6. Порядок исполнения

### Phase A — Pilot MVP Track

#### A0. Baseline + risk audit
Сделать:
- baseline confidence;
- protocol shape audit;
- allocation strategy note;
- crypto async test harness note;
- concurrency risk note.

#### A1. Live Online Core
Сделать:
- runtime wiring;
- Checkbox transport stabilization;
- shift lifecycle;
- online sale/return/service;
- real `CryptoProvider` seam;
- allocation seam;
- basic archive/trace;
- Gate 1 tests.

#### A2. Resilience
Сделать:
- sidecar integration;
- timeout/retry/backoff;
- circuit breaker;
- DEAD/retryable semantics.

#### A3. Observability & edge safety
Сделать:
- `/metrics`;
- ingress rate limiting;
- shutdown hardening;
- minimal heartbeat/outbox stub.

#### A4. Storage safety
Сделать:
- integrity check;
- backup;
- corruption stop mode;
- retention minimum.

#### A5. Offline core
Сделать:
- `GO_OFFLINE / ASK_OFFLINE_CODES / GO_ONLINE`;
- ranges / limits / watermarks;
- reconnect reconcile;
- LND sequence recovery.

### Phase B — Full MVP by TZ
- rendering
- Cloud contour
- real servers
- acceptance closure

### Phase C — Phase 1.1
- deployment / upgrade / migration
- DPS
- feature flags
- expanded metrics
- extended concurrency policy

---

## 7. Как Claude Code должен выбирать bounded step

Выбирать **один** bounded step по таким правилам:

### Предпочитать
- то, что убирает архитектурную неизвестность;
- то, что делает live contour менее synthetic;
- то, что усиливает recovery / correctness;
- то, что повышает pilot readiness;
- то, что усиливает acceptance-critical tests.

### Не предпочитать
- broad refactor;
- cosmetic rename/repack;
- новую общую abstraction layer без немедленной пользы;
- developer-experience improvements без product value;
- вторичный optimization work до stress findings.

### Если есть выбор между feature и proof
Сначала выбрать **proof**, если:
- он может выявить поздний rework;
- он проверяет risky invariant;
- он сужает архитектурную неопределённость.

---

## 8. Когда Claude Code обязан остановиться

Остановка обязательна, если:
- меняется signature между слоями;
- меняется state machine;
- меняется транзакционная граница;
- меняется retry/reconciliation semantics;
- добавляется новый config surface;
- затрагивается numbering/allocation logic;
- возникает сомнение stateless vs session-based ingress;
- выясняется, что current canonical contract не покрывает Maria/WebCheck;
- concurrency smoke показывает unexpected `SQLITE_BUSY` / duplicate / starvation behavior.

---

## 9. Обязательный шаблон промежуточного отчёта

```text
STEP:
<краткое имя шага>

GOAL:
<что закрывали>

WHY THIS STEP:
- какая ценность
- почему сейчас
- почему не другой шаг

WHAT WAS DISCOVERED:
- фактическое состояние
- узкие места
- подтверждённые риски

CHANGED FILES:
- path/to/file1
- path/to/file2

WHAT CHANGED:
- ...
- ...

INVARIANTS CHECK:
- network/crypto inside long SQLite tx: yes/no + explanation
- single-writer semantics affected: yes/no + explanation
- idempotency affected: yes/no + explanation
- channel lock affected: yes/no + explanation
- schema_version handling affected: yes/no + explanation
- crypto seam realism affected: yes/no + explanation
- allocation seam affected: yes/no + explanation

TESTS RUN:
- <command>
- <command>

TEST RESULT:
- passed/failed
- counts

GATE IMPACT:
- which gate moved forward
- what remains for that gate

KNOWN GAPS:
- ...

RISKS / DECISION POINTS:
- ...

NEXT RECOMMENDED STEP:
- one bounded step only
```

---

## 10. Обязательные Gate 1 тесты

### 10.1 Atomic Sale
Проверить:
- durable inbox write;
- lease acquisition;
- document prepared before send;
- durable state transitions;
- final consistent completion.

### 10.2 Restart & Recovery Smoke
Проверить:
- crash near send/poll boundary;
- restart;
- stage-1 recovery;
- fate reconciliation;
- deliberate readiness behavior.

### 10.3 Channel Lock Enforcement
Проверить:
- fast reject path;
- authoritative reject path.

### 10.4 Idempotency
Проверить:
- same key does not create second fiscalization.

### 10.5 Traceability
Проверить:
- protocol_trace_log
- transport_trace_log
- audit trace linkage

### 10.6 Graceful Shutdown
Проверить:
- no dangling active lease.

### 10.7 Concurrency Smoke
Проверить:
- 50-100 concurrent requests
- no duplicate fiscalization
- observable queue/busy behavior

---

## 11. Стартовый prompt для Claude Code

Используй этот prompt как старт нового этапа:

### START PROMPT
Ты работаешь в репозитории Multi-Protocol PRRO Gateway.

Твоя роль: senior staff engineer + senior architect + senior test engineer.

Работаем от существующего baseline.  
Нельзя перепридумывать архитектуру.  
Нужно двигать систему к pilot-ready состоянию маленькими безопасными bounded steps.

Сначала:
1. изучи текущее состояние репозитория;
2. соотнеси его с roadmap v2.1;
3. выбери один лучший bounded step;
4. выполни только его;
5. остановись и выдай отчет по шаблону execution pack v2.1.

Особо проверь:
- synthetic vs real crypto seam;
- наличие allocation strategy seam;
- риски stateful ingress для Maria/WebCheck;
- readiness/recovery correctness;
- single-writer burst behavior.

Не делай второй этап автоматически.

---

## 12. Prompt для следующего шага после human review

### FOLLOW-UP PROMPT
Продолжаем от предыдущего результата.  
Учти замечания ревью.  
Не расширяй scope.  
Выбери один следующий bounded step, который:
- либо снимает архитектурную неизвестность,
- либо усиливает correctness/recovery,
- либо улучшает pilot readiness,
- либо закрывает один конкретный gate test.

Снова остановись после одного bounded step и выдай отчет по шаблону.

---

## 13. Что приносить человеку на ревью

Claude Code должен приносить:
1. summary diff;
2. список изменённых файлов;
3. точные команды тестов;
4. вывод результатов;
5. открытые риски;
6. следующий bounded step.

Если есть decision point — формулировать его явно, а не замалчивать.

---

## 14. Запрет на ложную завершённость

Claude Code не имеет права писать:
- «готово полностью»
- «gate closed»
- «production-ready»

если нет:
- тестов;
- проверки инвариантов;
- описания остаточных рисков;
- явного human review checkpoint.

---

## 15. Краткий вывод

Execution pack v2.1 переводит Claude Code из режима «кодогенератор по задачам» в режим **управляемого инженерного исполнителя**, который:
- раньше вскрывает архитектурные риски;
- не маскирует synthetic contour как живой;
- приносит человеку качественные промежуточные результаты;
- двигает продукт к pilot readiness с меньшим риском позднего rework.