# B1 v1 — online-convergence tick (проход-2, пункт 3)

**Дата:** 2026-06-11
**Архитектор:** Fable 5 (дизайн залочен; ревью диффа — Fable, мердж после ревью)
**Имплементатор:** Opus 4.8
**Статус:** SPEC LOCKED. Это **первый продакшн-кодовый** пункт схемы — границы эскалации жёстче,
чем в kill-матрице: любое расхождение фактического поведения арм-ов с ожиданиями спеки,
любая необходимость НОВОГО state-перехода или НОВОЙ SQL-envelope → СТОП и эскалация.

## 1. Проблема (доказана kill-матрицей, PR#131)

Online-доки, покоящиеся в `SENT`/`KVT1`, сходятся ТОЛЬКО на boot'е — а online-`KVT1`
вообще не сходится никогда: boot-арм = `passive_hold_kvt1` (boot_phase.rs:2676), drain
обслуживает только офлайн-кохорту. На 24/7-кассе чек может висеть KVT1 днями (OCF-5).

## 2. Решение: третий tick-loop супервизора

Точно по образцу существующих 5d-loop'ов (drain + return-online, supervisor.rs:777+):
периодический «online-convergence tick» — на каждом тике, для каждого FN реестра,
доводить покоящиеся `SENT` → (probe Match) → `KVT1` → (evidence) → `ACK`.

**Переиспользование, НЕ изобретение** — тик вызывает СУЩЕСТВУЮЩИЕ арм-ы:
- `SENT`: зеркало boot Sent-арма (boot_phase.rs:2822+) — `last_chk_probe::probe(dps,
  fn_sign, sfn)` → `Match` → `advance_sent_to_kvt1_from_probe(...)`. Прочие исходы
  probe (`Mismatch`/`NotFound`/`DecodeEscalate`/`TransportRetry`) — ровно та же
  обработка, что в boot Sent-арме (прочитать арм и отзеркалить; если арм терминализирует/
  эскалирует — тик делает то же САМОЙ функцией, не копией).
- `KVT1`: зеркало drain-арма Kvt1Reentry (`backlog_drain` / его confirm-путь) —
  classify lastChk-результата → Acked(непустой data_sign) → `kvt2_advance::advance_to_ack(
  pool, doc_id, data_sign, sfn, DocState::Kvt1, attempt_no)` → тот же finalize-шаг, что
  в существующем арме (Kvt1Reentry доводит до ACK + inbox DONE — отзеркалить его
  последовательность 1:1). Hold (пустой data_sign) → док остаётся KVT1 до следующего тика.
- НИКАКИХ новых state-переходов, никаких новых SQL-конвертов. Если зеркалирование
  упирается в приватность (`pub(crate)`-арм недоступен из нового модуля) — поднять
  видимость минимально (`pub(crate)`) с doc-комментом, НЕ копировать тело.

## 3. Слои и файлы (залочено)

1. **`src/services/reconciliation/online_convergence.rs` (NEW)** —
   `pub async fn run_tick_for_fn(pool, view: &RuntimeView<'_>, fiscal_number) ->
   anyhow::Result<TickSummary>`:
   a. SELECT-first контракт: один read-only запрос док[ов] FN в (`SENT`,`KVT1`)
      ORDER BY lnd — **ноль wire-вызовов, если покоящихся доков нет**;
   b. mode-guard: читать `node_state.mode`; работать ТОЛЬКО при `Online` —
      иначе TickSummary::SkippedMode (GoingOnline/Offline-доки — чужая юрисдикция:
      drain/return-probe);
   c. по докам в порядке lnd: SENT-handler, KVT1-handler (см. §2);
   d. per-doc ошибка → лог + продолжить со следующим доком (изоляция);
   e. `TickSummary` — счётчики (advanced_sent, acked_kvt1, held, skipped...) для лога.
2. **`src/app.rs`** — метод `App::converge_online_for_fn(&self, fiscal_number, view)`:
   `let _g = self.acquire_fn_gate(fiscal_number).await;` → вызов сервиса. **Гейт
   обязателен** — это исполнение A4 forward-contract (см. док-коммент app.rs:400):
   тик и будущий inline-fiscalize сериализуются на одном FN. Wire-вызовы ПОД гейтом
   допустимы (гейт — tokio mutex, не SQL-транзакция; инвариант #1 не затронут).
3. **`src/runtime/supervisor.rs`** — `spawn_convergence_loop(...)` — точная копия
   структуры `spawn_drain_loop` (:777): interval + MissedTickBehavior::Skip, biased
   select на shutdown, **F1-инвариант: loop возвращает только Ok(())**, F2: bail между
   FN, per-FN ошибки логируются и не фатальны; fn_sign строится FRESH per tick
   (signingTime freshness — как drain_tick:834). Регистрация:
   `SupervisedTask::runs_until_shutdown("online-convergence", handle)` + поле в info-лог.
4. **`src/config/mod.rs`** — `supervisor.online_convergence_interval_seconds`:
   serde-default **60**, clamp-константы MIN=**15** / MAX=**3600**,
   `clamped_online_convergence_interval_seconds() -> (u64, bool)` + WARN-паттерн в
   супервизоре — всё зеркально drain-полю (:273-:338).
5. **`src/services/reconciliation/mod.rs`** — регистрация модуля.

## 4. Явно ВНЕ скоупа B1 v1 (не делать, не «улучшать»)

- ER-доки (и TransientRetry-redrive, и K3-класс с неполным trace): redrive = re-SEND
  машинерия, probe без sfn требует протокол-дизайна — отдельная спека.
- NEW-без-doc реапер (F1-окно) — после A2.4 (нужен полный fiscalize-вход).
- Подведение drain-tick под A4-гейт — находка для A2.4-интеграции (сегодня гонки нет:
  продакшн write-path не флипнут), в этом PR не трогать.
- Backoff per FN (как у drain) — не нужен: SELECT-first контракт делает пустой тик
  бесплатным (ноль wire).
- KVT2-доки: boot-арм finalize их кроет; в тик не включать (минимальный диф). Если
  при зеркалировании выяснится, что Kvt1Reentry-путь оставляет док в KVT2 без finalize —
  эскалация, не самодеятельность.

## 5. Тесты (TDD: RED → GREEN на каждый; файл `rust/prro/tests/online_convergence_tick.rs`)

Фикстуры и стаб — зеркало `tests/kill_point_matrix.rs` (KpStub-стиль счётчики Arc<AtomicUsize>,
композиция реальных стадий для построения SENT/KVT1 — НЕ сырой INSERT доков).

| # | Тест | Залочено |
|---|---|---|
| 1 | `tick_converges_resting_sent_to_ack` | построить SENT (как K-матрица: стадии + send Ok); тик с lastChk→Match(непустой data_sign); итог: doc `ACK`, inbox `DONE`, `send_chk` суммарно ==1 (тик НЕ шлёт), `assert_clean` |
| 2 | `tick_converges_resting_kvt1_to_ack` | построить KVT1 (SENT + probe-advance, как K4 phase-2); тик → `ACK`+`DONE`, send==1, `assert_clean` |
| 3 | `tick_zero_wire_when_nothing_resting` | чистый FN (или только ACK-доки) → тик: `send_chk`==0 И `last_chk`==0 (SELECT-first контракт) |
| 4 | `tick_skips_non_online_mode` | KVT1-док + mode=`Offline` (потом `GoingOnline`) → тик: ноль wire, док не тронут |
| 5 | `tick_hold_on_empty_data_sign` | KVT1 + lastChk Match с ПУСТЫМ data_sign → док остаётся `KVT1`, не ошибка, скан чист |
| 6 | `tick_idempotent_after_convergence` | после теста 1 — второй тик: ноль новых wire-вызовов, состояния неизменны |
| 7 | `tick_serialises_on_fn_gate` | через `App`: захватить `acquire_fn_gate(FN)` извне, запустить `converge_online_for_fn` в task, `tokio::time::timeout(короткий)` → pending; отпустить гейт → завершается. (Если App-уровневая фикстура слишком тяжела для интеграционного теста — эскалировать с предложением, НЕ выбрасывать тест молча) |
| 8 | **`kill_point_matrix.rs::k4`** — добавить phase-3 | после resting-KVT1: тик → `ACK`, `send_chk` ВСЁ ЕЩЁ ==1 — замыкание дыры, найденной матрицей. Обновить doc-коммент K4 (B1 теперь существует). K1/K2 НЕ трогать |
| 9 | конфиг-clamp | unit: default 60; clamp 1→15, 9999→3600, `was_clamped` |

## 6. Инварианты (в PR-body — обязательный разбор)

- **#1**: тик не открывает новых `with_immediate` с wire внутри — только существующие
  арм-ы (probe → отдельный конверт advance). Подтвердить чтением зеркалируемых арм-ов.
- **#2**: A4-гейт per FN на всю работу тика по FN (тест 7).
- **#4**: все advance'ы CAS-guarded существующими функциями; двойной тик — no-op (тест 6).
- **#8**: ни одного нового перехода; только whitelisted-функции арм-ов.
- **#9**: F1 (Ok-only loop) + F2 (between-FN bail) — зеркало drain-loop, те же комменты.

## 7. Процедура

Ветка `feat/b1-online-convergence` от свежего origin/main. Гейты: полный
`cargo test -p prro --features test-support` (ожидание: 1362 + новые), fmt, clippy -D.
PR НЕ мерджить — ревью архитектора. В PR-body: таблица тестов, разбор инвариантов §6,
блок «Findings for architect review» (включая всё, что пришлось поднять до `pub(crate)`).
Не трогать: untracked-файлы оператора, LEGAL_INVARIANTS.md, CLAUDE.md, workflows.
