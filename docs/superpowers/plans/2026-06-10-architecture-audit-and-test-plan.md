# Архитектурный аудит + план тестирования (RS-3, пред-A2.4)

**Дата:** 2026-06-10
**Автор:** Fable 5 fresh-eyes аудит (по запросу оператора: «код писался более слабой моделью — не наделали ли ошибок»)
**Состояние дерева:** ветка `feat/rs3-a2-1b-core` @ `cdb3fb2` (PR #125); полный прогон 1343 passed / 5 ignored.

---

## Часть 1. Оценка архитектуры

### 1.1 Что сделано ПРАВИЛЬНО (не трогать)

1. **Стадийный write-path с per-stage конвертами** (`stage_acquire → sign → dispatch → send → confirm → finalize`), каждый — своя короткая `with_immediate`, весь IO строго между конвертами. Инвариант #1 соблюдается структурно, а не дисциплиной.
2. **Dormant-стратегия RS-3**: весь позвоночник построен и протестирован за неперевёрнутым биндингом; включение = один коммит (A2.4). Blast radius управляем.
3. **Fail-loud посадка таксономии**: fenced-коды + round-trip-тесты в HTTP-классы; exhaustive-match'и без `_` (после OCF-4); опечатка кода не может тихо деградировать 422→500.
4. **Леджер как истина** (decision e / replay joint-matrix): inbox-статус никогда не trusted сам по себе; replay джойнит с `fiscal_documents`. Это правильный фундамент фискальной честности.
5. **Seam-дисциплина**: `advance_to_ack` (A2.1a) принимает evidence-байты параметром — это уже окупилось дважды (inline reuse + открывает оптимизацию §1.2.1 без рефакторинга).
6. **Двухуровневая сериализация** (#2): A4 in-process gate (liveness) + DB lease CAS (durable backstop) — разделение корректное и документированное.

### 1.2 Архитектурные находки (ошибки/долги, по убыванию важности)

#### 1.2.1 [ИССЛЕДОВАТЬ до пилота] Двойной DPS round-trip на каждый онлайн-чек
`stage_send` получает `CheckAck{id, id_sign, data_sign}` от `sendChk` и **выбрасывает `data_sign`**; затем inline (и drain SentFresh!) делают ВТОРОЙ запрос `lastChk`, чтобы получить ту же квитанцию. На пилоте это 2× латентность ДПС и 2× поверхность отказов на happy-path (каждый Hold-202 = чек «завис» до recovery).
**Почему так**: W12-доктрина «каноническое evidence только из lastChk» (KVT1_RAW byte-for-byte контракт определён для lastChk). Артефакт переиспользования drain-машинерии.
**Рекомендация**: исследовательский тикет — подтвердить протокольную эквивалентность `sendChk.data_sign ≡ lastChk.data_sign` (по докам ДПС/OLE + live-smoke). Если эквивалентны: пробросить `data_sign` через `WireDecision::Sent`/`StageSendOutcome::Sent` и кормить `advance_to_ack` напрямую (lastChk остаётся fallback при пустом data_sign). Seam уже готов — правка минимальна. **Не делать до A2.4** (не трогать heavily-tested stage_send в этом окне).

#### 1.2.2 [MED, до пилота] Конвергенция online-`Sent` доков только на boot
Boot-recon probe'ит `Sent`-доки (W11 PR-2b), но 24/7-нода без рестартов не конвергирует Hold-202/drift чеки **никогда**. Offline-drain исключает online-доки by design.
**Рекомендация**: ops-loop (уже в плане Sprint 12 — память `project_arch_decisions`) обязан включать периодический re-probe online `Sent`-доков, переиспользуя boot-машинерию (`last_chk_probe`). Это же закрывает TA-5/OCF-5 из мульти-линзового ревью. **Зафиксировать в скоупе B1/ops-loop.**

#### 1.2.3 [MED — ИСПРАВЛЕНО `cdb3fb2`] Терминализация на unknown-truth вердикте
Мой же фикс OCF-3 (1f03117) терминализировал inbox на ЛЮБОЙ Err леджер-резолвера → чек, доконвергировавший до ACK через boot-probe, replay'ился бы как `Failed` (REJECTED шорт-кат в replay.rs идёт ДО джойна с леджером). Исправлено: `is_terminal_ledger_verdict` — терминализация только при позитивном терминальном вердикте (DpsRejected / manual-recon); unknown-drift оставляет PROCESSING.
**Урок для процесса**: каждый фикс на failure-арм-ах обязан проверяться вопросом «что увидит replay этого ключа после КАЖДОГО возможного будущего состояния леджера?»

#### 1.2.4 [LOW, в A2.4] Типовая непривязанность per-FN зависимостей
`fn_gate: &OwnedMutexGuard<()>` ничем не привязан к `row.fiscal_number`; `sign_ctx`/`fn_sign` — тоже per-FN, но тип не знает чьи. Баг проводки в A2.4 (чужой гейт / чужой ключ оператора) невидим компилятору; чужой ключ = подпись чужим сертификатом.
**Рекомендация**: в A2.4 — newtype `FnGateGuard { fiscal_number, guard }` + ранний assert `== row.fiscal_number`; биндинг держит map FN→(sign_ctx, fn_sign) и резолвит по строке. Дешёво, закрывает класс ошибок.

#### 1.2.5 [LOW, перед A2.2] `inline::run` ~540 строк глубокой вложенности
A2.2 добавит shift-edges в эту же лестницу. **Рекомендация**: перед A2.2 — извлечь онлайн-хвост (`send → confirm → advance → outcome`) в приватный helper (чистое извлечение функции, поведение 1:1, существующие 15 интеграционных тестов — и есть его pin).

#### 1.2.6 [LOW] Семантика «terminalise-everything» для infra-сбоев
`ACQUIRE_INTERNAL`/`DISPATCH_INTERNAL` (транзиентные DB-сбои) жгут idempotency-key навсегда → POS обязан пере-ключевать чек. `busy_timeout 5s` поглощает обычный contention, так что это редкость, но **контракт для POS-шима должен это документировать** (HTTP-5/F6 из ревью): «500/503 ⇒ повторная подача = НОВЫЙ idempotency_key; старый ключ ответит 422 INBOX_REJECTED». Альтернатива (вернуть строку в NEW при infra-классе) — осознанно отвергнута оператором; не переоткрывать без данных пилота.

#### 1.2.7 [LOW] Аудит-гигиена
Литерал `"ingress_inbox"` ×5 в inline.rs → const (как `AUDIT_ENTITY_DOC`); `INLINE_NOOP_UNEXPECTED` пишется с `payload=None` — добавить `{fiscal_number, operation_type}` для триажа. Тест-гигиена: `std::mem::forget(tempdir)` (унаследованный паттерн) мусорит CI-диск; hand-built `InboxRow` в фикстурах вместо re-read из БД (расхождение невозможно сейчас, но re-read честнее контракту A-H1).

### 1.3 Сводка по «не наделали ли ошибок»

За весь RS-3-цикл (A2.1a + A2.1b-core) через 3 контура ревью + этот аудит прошло: **0 HIGH** дефектов в смерженном коде; **2 MED** пойманы до мерджа (TA-1 тест-дыра; 1.2.3 — моя же, поймана аудитом через час); остальное — LOW/гигиена/forward-looking контракты, все либо исправлены, либо явно посажены в скоуп B1/A2.2/A2.4. Архитектура цельная; главный системный риск сейчас — не код, а **нетестированные сходимости** (kill-points, конкуренция, recovery-петли) — их закрывает план ниже.

---

## Часть 2. План тестирования

### Слой 0 — что уже есть (инвентарь)
1343 теста / 126 сьютов: unit-пины стадий, drain (W9/W12), boot-recon (W9b/W11), replay joint-matrix, миграции, fenced-коды round-trip, 15 интеграционных через `inline::run` (happy SELL/RETURN, 202×2, offline-ack, Z-501, SHIFT_OPEN-422, четырёх-вариантный gate, Drift-#8, Noop-resolve, BuildReject).

### Слой 1 — добить в PR #125 (до мерджа) ✅ сделано в `1f03117`+`cdb3fb2`
Закрыт. Остаточные известные дыры (зафиксированы, не блокируют): TA-3 (dispatch-Refused арм — нужен mid-flight flip ноды, только через stage-stub), Resumed-путь через inline (A2.5 по плану).

### Слой 2 — до A2.4 (обязательные ворота переворота биндинга)

| # | Тест | Что пинит | Как |
|---|---|---|---|
| 2.1 | **Same-FN конкуренция**: два `run()` на один FN, разные request_id, ОДИН гейт | #2: ровно один Proceed-поток, второй ждёт гейт; NON-IDENTITY фикстуры | tokio::join + барьер; assert последовательность аудитов |
| 2.2 | **Same-key double-POST гонка** | #4: вторая подача того же idempotency_key → Replay, не вторая фискализация | два insert+run параллельно; assert 1 doc, 1 lnd |
| 2.3 | **Kill-point матрица** (главный пробел!) | #8/#9: обрыв future между КАЖДОЙ парой конвертов (sign↔send, send↔confirm, confirm↔advance, advance-1a↔finalize) → рестарт → boot-recon → assert: doc сходится (ACK или честный terminal), **ноль двойных фискализаций** (1 lnd, 1 send_chk на чек) | tokio::select с отменой на инжектированных точках (test-support hook) либо process-kill harness |
| 2.4 | **Replay-матрица end-to-end** | таблица: {первый исход: ACK, 202-Sent, 202-ER, OfflineLocalAck, REJECTED-inbox, Rejected-doc, drift-PROCESSING} × re-POST → ожидаемый HTTP/код | table-driven через handler+replay (не только inline) |
| 2.5 | **Property-based CheckJson** | serde-поверхность: `deny_unknown_fields`, adjustments, suм-поля i64-границы | proptest/arbitrary на SELL_PAYLOAD-вариации → build_canonical+stage_sign не паникуют, отказы типизированы |
| 2.6 | **Offline-цикл сходимости** | OfflineLocalAck → (drain mock-DPS) → ACK; inbox PROCESSING→DONE | соединить inline-тест с backlog_drain::drain в одном сценарии |

### Слой 3 — вместе с A2.4 (HTTP-уровень, обязательны в его PR)
- Четырёх-вариантный gate **через axum** (wire-коды 422/500/422/503 + envelope-шейп).
- Идемпотентная гонка на HTTP (два конкурентных POST одного ключа).
- **Graceful shutdown** mid-fiscalize (#9): SIGTERM между sign и send → 202/timeout клиенту, doc resumable, гейт отпущен (RAII), рестарт сходится.
- A/B флипа: `UnimplementedWritePath` → 501 / `InlineWritePath` → живой ladder (one-line revert проверен).
- FN-binding пины из 1.2.4 (чужой FN в map → fail-loud).

### Слой 4 — пред-пилот (операционный, по существующему runbook + новые)
- **Live-DPS smoke** (`docs/operations/LIVE_DPS_SMOKE_RUNBOOK.md`) — расширить inline-ACK сценарием + замер p50/p99 двойного round-trip (вход для решения 1.2.1).
- **Soak 24h+**: цикл online↔offline (flip-flop), исчерпание offline-кодов → STOP_MODE, дрейф часов, заполнение WAL/диска.
- **Chaos-DpsChannel**: latency 1–30s, обрывы mid-response, 5xx-штормы → распределение исходов {ACK, 202, OfflineLocalAck} без терминальных 500 на transient-классах.
- **kill -9 soak**: рандомные убийства под нагрузкой N часов → после каждого рестарта инвариант-скан (ниже).
- **Нагрузка**: целевой профиль чеков/мин одного POS ×10 запас; деградация под contention.

### Сквозной инструмент — «инвариант-скан» (рекомендую сделать в test-support)
SQL-набор пост-условий, прогоняемый после ЛЮБОГО сценария/kill/soak:
1. `COUNT(lnd) = COUNT(DISTINCT lnd)` per FN (нет двойной выдачи);
2. нет doc в `SENDING` старше N мин (Pattern B recovery работает);
3. каждый ACK имеет непустой `server_fiscal_no` + `KVT1_RAW` в document_files;
4. MAC-цепочка: `previous_hash` каждого следующего == `unsigned_xml_sha256` предыдущего по lnd;
5. нет inbox `NEW`/`PROCESSING` старше N мин без живого владельца (после B1);
6. REJECTED inbox ⇒ нет accepted-doc того же request_id (анти-1.2.3);
7. offline: consumed_codes ⊆ выданных, без повторного потребления.
Это превращает каждый soak/chaos-прогон в проверку **всех** легальных инвариантов разом.

### Матрица «инвариант → пин» (текущее покрытие)

| Инвариант | Пин сейчас | Дыра |
|---|---|---|
| #1 нет IO в tx | static-scanner + assert_not_in_with_immediate | ✅ |
| #2 single-writer | fn_gate юниты + lease CAS юниты | **2.1** (сквозной) |
| #4 идемпотентность | replay-юниты, Noop-тест | **2.2, 2.4** |
| #5 offline-лимиты | W7a/коды юниты | soak 4 (исчерпание) |
| #8 recovery/переходы | drain/boot сьюты, Drift-#8 тест | **2.3 kill-points** |
| #9 graceful shutdown | cancel-safety доки/юниты стадий | **3 SIGTERM e2e** |
| Inbox-lifecycle | четырёх-вариантный gate (15 тестов) | TA-3 арм (stub) |
| MAC-цепочка | finalize chain-guard юниты | скан-пункт 4 в soak |

### Порядок исполнения
1. **Сейчас**: мердж PR #125 (после твоего сигнала) → A2.2.
2. **Слой 2** — отдельный PR сразу после A2.2 (kill-point harness — самый дорогой и самый ценный пункт; начать с него).
3. **Слой 3** — внутри PR A2.4 как его merge-gate.
4. **Слой 4 + инвариант-скан** — спринт стабилизации перед пилотом; скан написать раньше (он нужен слоям 2–4).
