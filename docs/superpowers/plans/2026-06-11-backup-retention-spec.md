# Backup / restore / retention — прод-грейд (проход-2, пункт 4)

**Дата:** 2026-06-11
**Архитектор:** Fable 5 (спека залочена; ревью диффа — Fable; мердж после ревью)
**Имплементатор:** Opus 4.8
**Статус:** SPEC LOCKED. Два инкремента → **два отдельных PR** (PR-A низкорисковый,
PR-B — hot-zone boot-recon с жёсткими fence). Эскалация — как в B1: расхождение
факта со спекой, новый state-переход, новая SQL-envelope → СТОП.

## 0. Почему это прод, а не «бэкап как у всех»

Restore на ФИСКАЛЬНОМ леджере опасен сам по себе: восстановив вчерашний снапшот,
узел оказывается «позади» ДПС → переиспользует `lnd`, ломает MAC-цепочку, двоит
фискальные номера. Поэтому прод-грейд = снапшоты+ретенция+верификация (PR-A)
**плюс boot stale-tip guard** (PR-B): после restore (и вообще на каждом boot'е)
узел сверяет свой ACK-хвост с ДПС и при расхождении **блокируется**, а не торгует.

Скоуп данных: оба файла БД — `prro.db` (main) + secure-БД (операторы, Z0-конфиг).
Secure ВКЛЮЧЁН в бэкап (turnkey-restore важнее: без него после смерти диска —
ре-онбординг операторов), но: **все артефакты бэкапа получают owner-only права
(0600/0700), как HIGH-AUDIT-01 делает для живого secure-файла**, и ретенция-док
честно фиксирует: носитель бэкапа наследует security-постуру живого диска
(в secure лежит plaintext JKS-пароль — finding #6, KMS после пилота).

Сознательно НЕ в этой спеке (не делать): загрузка в облако/сеть (оператор может
указать сетевой путь — нам всё равно), шифрование бэкапов (пост-KMS), бэкап по
событию закрытия смены (A2.2-территория), per-snapshot invariant_scan в прод-бинаре
(скан остаётся test-support; в проде — `integrity_check`).

---

## PR-A — снапшоты, ретенция, верификация, restore-drill

### A1. Модуль `src/db/backup.rs` (NEW)

`pub async fn snapshot(pool, live_db_path, backup_dir, label) -> anyhow::Result<SnapshotReport>`:
1. Механизм: **`VACUUM INTO '<tmp>'`** одним statement через пул (живая БД, WAL —
   консистентный компактный снимок без остановки записи; bundled SQLite 3.46 ≥ 3.27).
   Имя: `<label>-YYYYMMDD-HHMMSS-<8hex случайных>.db` (БЕЗ перезаписи существующих;
   на Windows rename-over не работает — поэтому tmp в ТОЙ ЖЕ директории →
   `std::fs::rename` в финальное имя, имена уникальны).
2. Сразу после rename: **owner-only права** (unix: 0600; windows: best-effort,
   зеркало того, что HIGH-AUDIT-01 делает в `open_secure_pool` — прочитать и
   переиспользовать/вынести его helper, не копировать).
3. **Верификация обязательна**: открыть снапшот read-only свежим соединением →
   `PRAGMA integrity_check` == "ok" → закрыть. Не-ok → снапшот УДАЛИТЬ, вернуть Err.
4. `SnapshotReport { path, bytes, duration_ms, verified: true }`.
5. Никаких `with_immediate`; VACUUM INTO — read-транзакция со стороны источника
   (инвариант #1 не затронут; долгих write-локов нет).

`pub fn prune(backup_dir, label, keep_last, max_age_days) -> anyhow::Result<PruneReport>`:
по соглашению имени файла; удаляет сверх `keep_last` И старше `max_age_days`
(оба условия конфигурируемы); НИКОГДА не трогает файлы, не подходящие под наш
шаблон имени (чужие файлы в директории — не наши).

### A2. Четвёртый supervisor-loop

Структурная копия `spawn_convergence_loop` (F1 Ok-only, F2-семантика, biased
shutdown, MissedTickBehavior::Skip): каждый тик — `snapshot` main-БД + secure-БД
(два файла, label `main` / `secure`) + `prune` для обоих. Ошибка бэкапа —
**WARN-лог + продолжаем** (бэкап никогда не валит фискальный путь и не блокирует
loop; F1). Пути живых БД достать так, как их получает boot (конфиг), НЕ хардкод.

Доп. проверка на старте loop'а (один раз): если `backup_dir` лежит на том же
устройстве, что и живая БД (unix: `MetadataExt::dev()`; windows/прочее:
best-effort сравнение корня пути) — **WARN-аудит «backup on same device»**
(не фатально: лучше же, чем ничего, но оператор должен знать).

### A3. Конфиг (зеркало B1-паттерна)

`[backup]`: `enabled` (default **true**), `dir` (default `var/backups`),
`interval_seconds` (default **3600**, clamp **300–86400** + `(u64,bool)`-clamp-fn
+ WARN), `keep_last` (default **30**), `max_age_days` (default **14**).
`enabled=false` → loop не спавнится (лог INFO).

### A4. Доки (двa файла)

1. `docs/operations/BACKUP_RESTORE_RUNBOOK.md`: процедура restore по шагам —
   остановить сервис → скопировать снапшоты на место живых файлов → старт →
   **boot tip-guard сверит хвост с ДПС (PR-B); до зелёного guard'а узел НЕ торгует**;
   что делать при BLOCKED (см. PR-B); как проверить успех (audit-записи, режим).
2. `docs/operations/RETENTION_POLICY.md`: леджер НЕ удаляется никогда (фискальные
   данные); прунинг касается только снапшотов; security-постура носителя бэкапа;
   рекомендация второго физического устройства.

### A5. Тесты PR-A (TDD; `tests/backup_restore.rs`)

| # | Тест | Залочено |
|---|---|---|
| 1 | snapshot живой БД с данными | файл создан, `integrity_check` ok, открывается, содержит наши строки; права 0600 (unix-only assert) |
| 2 | snapshot ПОД конкурентной записью | параллельный writer (цикл insert'ов в отдельной task) + snapshot → оба успешны, снапшот консистентен |
| 3 | prune | насоздавать файлов по шаблону (+1 чужой) → keep_last/max_age соблюдены, чужой файл НЕ ТРОНУТ |
| 4 | verify-fail путь | подсунуть битый файл как «снапшот» (или мок) — точнее: проверить, что не-ok integrity → Err и файл удалён (можно через прямую порчу tmp до verify — если неудобно, эскалировать с альтернативой) |
| 5 | **restore-and-continue e2e** (ключевой) | реальный сценарий: fixture (как kill-матрица) → довести чек до ACK → snapshot → выдать ЕЩЁ один чек → «смерть» → restore снапшота в НОВЫЙ путь → `open_pool` (миграции идемпотентны) → boot-recon → `invariant_scan::assert_clean` → выдать следующий чек через inline-fixture → ACK. Доказывает: восстановленная БД — это просто «старый crash-state», конвергенция уже доказана матрицей |
| 6 | backup-loop изоляция | несуществующий/недоступный dir → snapshot Err → loop-тело логирует и НЕ падает (юнит на тело тика, не на spawn) |
| 7 | конфиг-clamp | default/min/max/`was_clamped` |

---

## PR-B — boot stale-tip guard (hot zone: boot-recon; отдельный PR, отдельное ревью)

### B-1. Семантика (ЗАЛОЧЕНА)

На boot'е, per FN, ПОСЛЕ существующих recovery-арм-ов (хвост уже доведён ими),
при `deps Some` и непустом ACK-хвосте:
1. Взять sfn ПОСЛЕДНЕГО (max lnd) `ACK`-дока FN из леджера.
2. **Переиспользовать `last_chk_probe::probe(dps, fn_sign, expected = этот sfn)`**:
   - `Match` → хвост консистентен → INFO-аудит `TIP_GUARD_OK`, узел живёт дальше;
   - `Mismatch` → мы позади ДПС (stale restore) ЛИБО на нашем ФН фискалил кто-то
     ещё → **`node_state.mode → BLOCKED`** (существующим setter'ом/CAS; если
     setter'а нет — эскалация, НЕ изобретать) + CRITICAL-аудит
     `TIP_GUARD_STALE_LEDGER` с обоими id; узел НЕ принимает фискальные команды
     (BLOCKED-семантика уже существует);
   - `TransportRetry` (ДПС недоступна) → WARN-аудит + **продолжаем** (offline-first:
     boot без сети не должен блокировать кассу; guard добежит на следующем boot'е);
   - `NotFound` при непустом нашем ACK-хвосте → аномалия того же класса, что
     Mismatch → BLOCKED + CRITICAL;
   - `DecodeEscalate` → как TransportRetry, но ERROR-лог.
3. Пустой ACK-хвост (свежий ФН) → guard молча пропускается.
4. Конфиг: `[backup] tip_guard_enabled` default **true**; false → INFO-лог
   «guard disabled» (kill-switch на случай ложных срабатываний в поле).

### B-2. Fence (жёстче B1)

- НИ ОДНОГО нового state-перехода документов; guard трогает ТОЛЬКО `node_state.mode`
  существующим механизмом.
- Никакого auto-resend, никакого исправления леджера — только детект + BLOCKED.
- Wire-вызов (`last_chk`) вне любых транзакций (инвариант #1).
- Расположение: `boot_phase` (после арм-ов, до объявления FN готовым) — если
  точка врезки неочевидна, эскалировать с предложением, не выбирать самовольно.

### B-3. Тесты PR-B (TDD; в `tests/backup_restore.rs` или рядом)

| # | Тест | Залочено |
|---|---|---|
| 8 | **stale-restore детект e2e** | ACK#1 → snapshot → ACK#2 → restore снапшота (леджер знает только #1) → boot со стабом lastChk = sfn#2 → `Mismatch` → mode == `BLOCKED`, CRITICAL-аудит есть; счётчик send == 0 на boot'е (ничего не переотправлено) |
| 9 | happy tip | хвост ACK#2 + lastChk = sfn#2 → Match → mode не тронут, INFO-аудит |
| 10 | ДПС недоступна | lastChk → Transport err → узел НЕ заблокирован, WARN |
| 11 | свежий ФН | ноль ACK → ноль wire-вызовов guard'а |
| 12 | kill-switch | tip_guard_enabled=false → ноль wire, ноль аудитов guard'а |

---

## Процедура

PR-A: ветка `feat/backup-retention` от свежего origin/main (`git fetch` ПЕРЕД
созданием ветки!). PR-B: ветка `feat/boot-tip-guard` от main ПОСЛЕ мерджа PR-A.
Гейты каждого PR: полный `cargo test -p prro --features test-support`
(база 1371 — ничего старого не сломано), fmt, clippy -D. PR-body: таблица
тестов + разбор инвариантов (#1/#2/#4/#8/#9) + «Findings for architect review».
НЕ мерджить — мердж после ревью архитектора. Не трогать: untracked-файлы
оператора, LEGAL_INVARIANTS.md, CLAUDE.md, workflows.
