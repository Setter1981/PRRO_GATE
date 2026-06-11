# Kill-point матрица — спека для реализации (проход-2, пункт 2)

**Дата:** 2026-06-11
**Архитектор:** Fable 5 (дизайн залочен; ревью диффа — тоже Fable)
**Имплементатор:** Opus 4.8 по этой спеке
**Статус:** SPEC LOCKED — отступления только через эскалацию архитектору

## 1. Цель

Исполняемое доказательство: обрыв процесса на ЛЮБОЙ границе конвертов онлайн/офлайн-лестницы
`inline::run` + рестарт + штатное recovery (boot-recon, drain) сходится **без двойной
фискализации** (ровно один lnd, ровно один `send_chk` на чек) и без нарушения леджерных
инвариантов (`db::invariant_scan`).

## 2. Краш-эквивалентность (обоснование метода — НЕ менять)

Под `synchronous=FULL` закоммиченный `with_immediate` переживает kill -9; не закоммиченный —
откатывается. Поэтому состояние «процесс умер между конвертами k и k+1» **байт-в-байт равно**
состоянию «стадии 1..k выполнились, k+1 не запускалась». Отсюда два механизма построения
K-состояний:

- **Композиция стадий** (K1, K2, K5, K6): вызвать реальные стадии по одной до границы.
  Никакого дропа future — детерминированно и не зависит от таймингов.
- **Drop-инжекция** (K3, K4): единственные точки «посреди wire» — это await'ы на DPS-стабе.
  Стаб блокируется на `tokio::sync::oneshot` → тест дропает future (`drop(fut)` /
  `tokio::select!` с немедленной веткой) в точный момент. Drop-безопасно: pending-await чисто
  отменяется, закоммиченные конверты остаются (это и есть «crash»).
  ВАЖНО: инжекция только на async-await стаба; НЕ пытаться дропать внутри spawn_blocking.

## 3. Recovery-последовательность после каждого K («рестарт»)

На ТОМ ЖЕ pool (БД пережила «смерть»):
1. `boot_phase::run_boot_reconciliation(&recon_guard(), &pool, FN, deps)` — образец вызова и
   `recon_guard()` взять из `tests/app_boot_reconciliation.rs` (W2 test-seam). `deps`:
   Some(...) со свежим стабом — счётчики send/last_chk у стаба ОБЩИЕ на фазы (или суммируются),
   т.к. ассерты «ровно N вызовов» считаются СКВОЗЬ рестарт.
2. Где применимо (офлайн-доки) — `backlog_drain::drain(&common::drain_test_guard(), &pool, &view, FN)`
   (образец: `tests/backlog_drain_state_dispatch.rs`).
3. `prro::db::invariant_scan::scan(&pool)` — финальный гейт (см. ожидания по K: где
   resting-state легален, скан и так чист — он запрещает только SENDING-в-покое и пр.).

## 4. Матрица K-точек (ожидания ЗАЛОЧЕНЫ)

Базовая фикстура — как в `tests/write_path_inline.rs` (FN-конфиг, node Online, открытая смена,
NEW SELL inbox c корректным sha, `det_signing_ctx`, A4-guard). Для каждого K — отдельный
`#[tokio::test]`.

**ПОПРАВКА АРХИТЕКТОРА (2026-06-11, эскалация имплементатора — принята).** Исходные ожидания
K1/K2 (`send_chk == 0`) были ОШИБКОЙ спеки: boot-recon с `deps Some` — штатный re-driver
pre-wire состояний (boot_phase.rs:2595/2768), и это фискально корректно (PREPARED/SIGNED
провода не касались ⇒ re-drive не может дать дубль; запрет авто-resend относится только к
`SENDING`+ — K3). Истинный инвариант матрицы: **суммарно ровно один `send_chk` на чек**,
а не «ноль на boot'е». Строки K1/K2 ниже — в исправленной редакции.

| K | Состояние на «смерти» | Как построить | Recovery | ЗАЛОЧЕННЫЕ ассерты |
|---|---|---|---|---|
| **K1** | doc `PREPARED`, inbox `PROCESSING` | только `stage_acquire::run` (см. вызов в `inline.rs`) | boot (deps Some, стаб: send Ok) | boot re-drive: doc → `SENT`; **`send_chk` суммарно == 1**; 1 строка doc / 1 lnd; скан чист (Sent — легальный resting). **Плюс idempotency-пин: ВТОРОЙ прогон boot (тот же pool, deps Some, last_chk → Match) НЕ шлёт повторно — `send_chk` остаётся == 1**, doc двигается probe-путём (→ `KVT1`, как в K4) |
| **K2** | doc `SIGNED` | acquire + `stage_sign::run` | boot (deps Some, стаб: send Ok) | аналогично K1 через Signed-арм (:2756): doc → `SENT`, **`send_chk` суммарно == 1**, второй boot не переотправляет (== 1) |
| **K3** ⚠ критическая | `SENDING` закоммичен, wire «в полёте» | acquire+sign+dispatch, затем `stage_send::run` со стабом, чей `send_chk` повисает на oneshot → **drop future** | boot | doc → `ERROR_RETRYABLE` (Sending-арм :2672, Pattern B); **`send_chk` суммарно == 1 и НИКОГДА не 2** — авто-resend ЗАПРЕЩЁН (trace неполный → ER-guard обязан держать HoldIndeterminate); скан чист; коммент: разрешение зависшего ER = probe/B1 (фид в спеку B1) |
| **K4** | `SENT` закоммичен, confirm не начался | полный online-путь, но `last_chk` стаба повисает на oneshot → drop future ПОСЛЕ того как send завершился (стаб сигналит „send done“ вторым oneshot'ом — дроп строго после коммита Sent) | boot c deps, чей `last_chk` теперь отвечает Match (`ack(id==sfn, data_sign непустой)`) | boot SENT-probe (:2822) → doc `KVT1` (advance из probe), затем Kvt1-арм = passive hold (:2676); **`send_chk` суммарно == 1**; `last_chk` ≥1; скан чист; **коммент-фид B1**: финальный ACK online-Kvt1 требует ops-loop/B1 (известный OCF-5) — ассертим resting `KVT1`, НЕ форсим ACK |
| **K5** | `KVT2` закоммичен (advance 1a прошёл), finalize нет | online-путь до Sent + `online_confirm`-эквивалент: вызвать `kvt2_advance::advance_to_ack`?... НЕТ — проще: полный путь, но соберись состоянием: acquire+sign+dispatch+send(Ok)+`advance_to_ack` с подменённым finalize невозможен. ЗАЛОЧЕНО: построить K5 прямой композицией — после `Sent` вызвать только Envelope-1a-часть невозможно публично ⇒ строить через `stage_send` + ручной CAS `Sent→Kvt1→Kvt2` репозиторными `transition_state` в одном `with_immediate` + Kvt1Raw replace_tx (зеркало конверта 1a; это допустимое state-construction, НЕ продакшн-вызов) | boot | Kvt2-арм (:2719) → finalize → doc `ACK`, inbox `DONE`; `send_chk == 1`; скан ПОЛНОСТЬЮ чист (`assert_clean`) |
| **K6** | `OFFLINE_LOCAL_ACK` закоммичен, drain не бежал | офлайн-фикстура (node Offline, OPEN-сессия, код) + полный `inline::run` (он сам терминируется на offline-ack) — это уже «K6-состояние» | drain (mock-DPS: send Ok + lastChk Match) | doc `ACK`, inbox `DONE`; код потреблён ровно один раз; `send_chk` (drain'ом) == 1; `assert_clean` |

Опционально (если дёшево после K1–K6, отдельными тестами): гонка same-FN (два `run()` под
одним гейтом последовательно — второй обязан получить Replay-исход, 1 doc), double-POST одного
idempotency_key.

## 5. Жёсткие правила реализации

1. **Test-only PR**: ни строчки продакшн-кода. Если recovery ведёт себя НЕ так, как ждёт
   матрица — это находка: СТОП, зафиксировать фактическое поведение в отчёте и эскалировать
   архитектору. НЕ «подгонять» ассерты молча и НЕ чинить продакшн.
2. TDD: каждый K — сначала RED (через `todo!()`-хелпер или умышленно-ложный ассерт с
   комментом), посмотреть падение, затем GREEN. В коммит-месседже — счёт тестов.
3. Счётчики стаба — `AtomicUsize` (образец `DualQueueStub` в
   `tests/backlog_drain_state_dispatch.rs`), переживают фазы.
4. `deps` для boot-recon: посмотреть точную форму в `tests/app_boot_reconciliation.rs`
   (RuntimeView / Option-параметр) — НЕ изобретать свою.
5. Гейты перед PR: `cargo test -p prro --features test-support` полностью; fmt; clippy
   `-D warnings` (crate prro); каждый K-тест зовёт `invariant_scan::scan`/`assert_clean`
   согласно таблице.
6. Файл: `rust/prro/tests/kill_point_matrix.rs`. Ветка: `feat/kill-point-matrix` от
   свежего origin/main. PR с табличкой «K-точка → исход → счётчики» в описании.
   НЕ мерджить — мердж после ревью архитектора.
7. Не трогать: untracked файлы оператора, docs/LEGAL_INVARIANTS.md, CLAUDE.md.
