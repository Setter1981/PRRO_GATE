# Глобальное ревью legacy-ядра — план и playbook

**Дата:** 2026-06-11
**Архитектор:** Fable 5 (линзы, правила, вердикты, spot-верификация)
**Ревьюеры-исполнители:** Opus 4.8, по одной сессии на модуль
**Старт:** после мерджа PR-B (boot tip-guard) — закрытие прохода-2
**Цель:** проверить legacy-ядро (написанное до-Fable моделями) ДО вживления в боевой
путь (A2.2 → A2.4). Выход: вердикт go/no-go per module + батчи фиксов.

## 0. Токен-экономика (конструктивное ограничение плана)

Бюджет Fable — узкое место. Поэтому:
1. **Fable не читает исходники модулей массово.** Чтение/охота — Opus-сессии.
   Fable читает только досье (лимит ниже) и перепроверяет репро топ-находок.
2. **Каждая CONFIRMED-находка обязана нести репро**: pinning-тест (можно
   незакоммиченный сниппет в досье), или SQL по seeded-БД, или однострочный
   grep-факт. Fable перезапускает репро (дёшево), а не повторяет охоту (дорого).
   Находка без репро = HYPOTHESIS — отдельная секция, Fable решает, копать ли.
3. **Формат досье жёсткий**: ≤300 строк; на находку ≤15 строк
   (`ID | severity | lens | file:line | claim | repro | предлагаемый класс фикса`).
   Severity: FT (fiscal-truth) / HIGH / MED / LOW / NIT.
4. Досье коммитятся в `docs/reviews/legacy-2026-06/<module>.md` на ветке
   `review/legacy-<module>` — преемственность между сессиями через репо, не чат.
5. Вердикты Fable дописываются в то же досье (секция «Architect rulings»).
6. Фиксы: NIT/LOW/механика-MED — Opus батчем по fence-правилам B1 (+pinning-тест
   на каждый); семантика/hot-zone — дизайн Fable, исполнение Opus.
7. Промпты для каждой Opus-сессии — ШАБЛОН в §4: оператор подставляет имя модуля,
   к Fable за промптом возвращаться не нужно.

## 1. Линзы (определены один раз, переиспользуются)

- **L1 fiscal-truth** — может ли внешне видимый статус чека солгать (replay,
  inbox, terminal-состояния)? Класс AUD-1.
- **L2 recovery-атомарность** — окна между конвертами: что будет при kill -9
  в каждой точке? Сверка с kill-матрицей; новые точки → кандидаты в K7+.
- **L3 охота за непроверенными допущениями** — каждый коммент/код с «always /
  never / cannot happen / всегда вернётся» проверить grep'ом на один уровень
  глубже (фильтры состояний, whitelists, кохорты). Доказанный класс ошибок
  слабых моделей (Mismatch/list_pending_for_fn в B1).
- **L4 таксономия ошибок** — terminal vs retryable: каждое место, где wire/DPS
  ошибка становится терминальной, обосновано? (Класс ERROR_SAVE -3.)
- **L5 idempotency/replay** — joint-matrix целостность: inbox-статус никогда не
  truth сам по себе; все пути входа дают консистентный replay-ответ.
- **L6 legal invariants** — INV-01..20 ↦ код, прицельно офлайн-лимиты (36ч/168ч),
  manual-recon триггеры (§16.7 M3b).
- **L7 concurrency/gates** — single-writer per FN, A4 forward-contracts,
  F1/F2-дисциплина всех loop'ов, lease vs gate.

## 2. Модули, порядок, линзы, известные подозреваемые

| # | Модуль (файлы) | Линзы | Подозреваемые (вход для охоты) |
|---|---|---|---|
| M1 | `services/reconciliation/boot_phase.rs` (~2700 строк) + `last_chk_probe.rs` | L2 L3 L4 L7 | ER-class guard (полнота trace-классификации), Encrypted 1-tick deferral, MAX_BOOT_ATTEMPTS семантика, histogram-счётчики vs реальные исходы, Mismatch/Manual-арм |
| M2 | `services/offline_sync/*` (drain, kvt2_confirm, return_online_probe, session lifecycle) + offline_codes repo | L1 L2 L5 L6 | manual-recon триггер-семейства §16.7; края потребления кодов; DRAINING-семантика; лимиты 36ч/168ч — где enforcement |
| M3 | replay/inbox: `services/write_path/replay.rs`, seam, ingress-вход | L1 L5 L3 | порядок REJECTED-short-circuit ДО ledger-join (AUD-1-семья); четырёхвариантный inbox-gate вне inline-пути; idempotency_key коллизии |
| M4 | `db/repositories/*` + `db/tx.rs` | L2 L3 L7 | дрейф CAS-whitelist vs эволюция DocState; фильтры-списки состояний (класс ДОКАЗАН в B1!); короткость конвертов |
| M5 | `transports/dps/*` (channel, dto, error, classify) | L4 L3 | маппинг статусов DPS (−3 ERROR_SAVE → сейчас терминальный REJECTED — кандидат в retryable); decode-пути; classify_check_result полнота |
| M6 | `runtime/*` (app, supervisor, server, fn_gate) + `config/*` | L7 L2 | boot-порядок, shutdown grace, F1/F2 единообразие 4 loop'ов, дефолты/clamp'ы, D2 loopback guard |
| M7 | shift-машинерия M3b (9-state) — `services/*shift*`, enums, guards | L1 L6 | сверка с M3b-спекой §16 (Round 8-9 overrides!); рёбра 4/6/12/14; готовность к A2.2-линковке |

Сжатие при нехватке времени: M5+M6 можно одной сессией; M7 можно отложить
к самому A2.2 (но не позже его старта).

## 3. Цикл на модуль

1. Opus-сессия: охота по линзам модуля → досье на ветке `review/legacy-<module>`
   → PR (docs-only!) с досье. Гейт сессии: каждая CONFIRMED имеет репро.
2. Fable: читает досье → перезапускает репро FT/HIGH/MED → дописывает rulings →
   мерджит docs-PR.
3. Фикс-батч (если есть что чинить): отдельная ветка по правилам §0.6 —
   Opus исполняет, Fable ревьюит по схеме B1.
4. После M1–M7: Fable пишет синтез (`docs/reviews/legacy-2026-06/SYNTHESIS.md`):
   кросс-модульные системные находки, go/no-go для A2.2, обновление тест-плана.

## 4. ШАБЛОН промпта Opus-сессии (подставить <MODULE> из §2)

```
Проведи ревью модуля <MODULE> проекта PRRO_GATE по playbook'у архитектора:
docs/superpowers/plans/2026-06-11-legacy-review-plan.md — git fetch origin,
прочти playbook ЦЕЛИКОМ (особенно §0.2-0.3 формат и §1 линзы), затем строку
своего модуля в §2 (линзы + подозреваемые).

РОЛЬ: ревьюер-охотник. Ты НЕ чинишь код — ты находишь и ДОКАЗЫВАЕШЬ.
- Каждая CONFIRMED-находка обязана нести репро: pinning-тест-сниппет,
  SQL по seeded-БД, или grep-факт (file:line + противоречие).
  Не смог доказать — это HYPOTHESIS, отдельная секция, без раздувания.
- Прицельно: линза L3 — каждый «always/never/cannot happen» в комментах
  и каждое перечисление состояний (фильтры, whitelists) сверить с
  фактическими enum/таблицами. Это главный класс ошибок в этом кодбейзе.
- Сверяйся с машинными якорями: tests/ (1371+), db/invariant_scan.rs,
  tests/kill_point_matrix.rs, docs/LEGAL_INVARIANTS.md,
  docs/superpowers/specs/2026-05-17-m3b-shift-state-expansion.md (§16 main!).
- Найденное при охоте, но вне линз модуля — секция OUT-OF-SCOPE одной строкой.

ВЫХОД: досье docs/reviews/legacy-2026-06/<module>.md (≤300 строк, формат §0.3),
ветка review/legacy-<module> от свежего origin/main, docs-only PR
(НЕ мерджить — мердж после rulings архитектора). Кода НЕ трогать вообще.
Коммиты: Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
Финальный отчёт: счёт находок по severity + 3 самые опасные одним абзацем.
```

## 5. Ожидаемый бюджет

- Opus: 7 сессий охоты + ~2-3 фикс-батча.
- Fable: ~1 короткий заход на модуль (досье+репро+rulings) + 1 синтез.
  Самое дорогое для Fable — M1-rulings и синтез; всё остальное — чтение
  компактных досье и перезапуск готовых репро.
