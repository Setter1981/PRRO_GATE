# Production Review Checklist — PRRO Gateway

**Дата:** 2026-04-14
**Scope:** Пілот 50 точок / 70 кас, online mode, direct DPS transport

---

## Базові виміри (кожен степ)

Ці виміри перевіряються в кожному code review:

1. **Коректність коду** — логіка, edge cases, SQL injection, параметризовані запити
2. **Архітектурний drift** — нові залежності, нові патерни, розповзання шарів, responsibility boundaries
3. **Інваріанти** — 10 frozen invariants з CLAUDE.md
4. **Доказовість тестів** — чи тест реально доводить заявлене, чи не можна зламати без падіння, чи перевіряє pipeline а не mock, чи assert на правильному рівні
5. **Порядок і повнота** (додано 2026-07-30) — якщо зміна обирає «найновіше» / «переможця» або додає нове
   джерело істини:
   - **Це повний порядок?** Побудуй два рядки, рівні за ВСІМА ключами сортування. Якщо вдалося —
     `ORDER BY ... LIMIT 1` повертає що завгодно. Пастки гранулярності: `datetime('now')` — до секунди;
     лічильник монотонний лише для подій, які його рухають (операція без алокації `lnd` лишає нічию).
   - **Хто виграє при рівності** — це доменний аргумент (`>` vs `>=`), а не смак. Наприклад: док, який
     СПОЖИВ ординал `k`, пізніший за witness, який лише ЗАРЕЗЕРВУВАВ `k`.
   - **Нічия має бути в тесті** — примусово, з рівністю за всіма провідними ключами. Тест, який
     «випадково не заничився» (рядки створені з різницею в секунди), не доводить нічого.
   - **Усі споживачі зміненої істини** — перелічити й довести, що кожен або успадковує нове правило, або
     свідомо виключений. Спільна проєкція лагодить лише тих, хто її питає; споживач із ВЛАСНИМ
     накопичувальним станом (walk із локальним `expected`) не успадковує — йому потрібне те саме
     правило окремо. Два споживачі з різними правилами — це тиха розбіжність, а не стиль.

   *Підстава:* два дефекти цього класу в одному слайсі (bd `PRRO_GATE-hpc`), обидва пройшли дизайн-рев'ю
   і були спіймані лише тестом, який будував нічию навмисне. Розгорнутий варіант —
   `.claude/skills/safe-write-path-change/SKILL.md`.

6. **Повнота предиката** (додано 2026-07-31) — якщо тест/модель/оракул гейтить щось на «прод тут
   відмовить», гейт мусить дзеркалити **повний** предикат прода, а не найпомітніший його диз'юнкт:
   - **Перелічи всі диз'юнкти** реального предиката і спитай: *який стан задовольняє прод, але НЕ те,
     що я синхронізую?* Звужений гейт зелений на малих N — розходження живе рівно в тих композиціях,
     які directed-тести не пишуть.
   - **Перевикористовуй прод-константу**, не копіюй предикат. Копія тихо розійдеться з тим, що
     реально енфорситься, і модель мispredict'итиме назавжди. Константу не бери на віру — має бути
     conformance-тест, що пінить її до міграції/DDL.
   - **Не зливай два різні предикати в одне поле.** «Чи є completable hold» і «чи піднятий fence» —
     різні питання з різними споживачами. Додавай поле, а не розширюй старе.
   - **Запиши межу чесно:** гейт на reality-synced предикаті доводить «ЗА ДАНИМ станом операція
     поводиться правильно», а НЕ «стан виникає коли треба» — друге лишається окремому тесту.

   *Підстава:* модель фаззера гейтила T=112 на `held_reservation` (один із трьох диз'юнктів S7-2
   fence). Дірку було задокументовано як known-narrow і лишено падати голосно — вона впала на
   ПЕРШОМУ ж `FUZZ_CASES=4096`, зшринкована до `[Crash(Send), Replenish(Granted)]`.

---

## Tier 1: Блокери пілоту

Перевіряти **в кожному степі** незалежно від scope зміни.

### R1 — Crash recovery: leaked inbox lease

**Ризик:** Inbox застрягає в `status=PROCESSING` після краша процесу між acquire і finalize. Lease timeout 120s, але якщо gateway рестартує швидше — документ orphaned. Каса фактично блокується на цьому fiscal_number.

**Питання ревьюеру:** Чи є sweep (startup або periodic) для inbox records stuck в PROCESSING з expired leases? Чи приводиться fiscal_document до консистентного стану (не залишається PREPARED з алокованим але невикористаним lnd)?

---

### R2 — Crash recovery: LND gap

**Ризик:** `increment_lnd` атомічно алокує lnd в acquire-транзакції. Якщо pipeline потім падає на sign/send — lnd витрачений, але документ не ACK. DPS не перевіряє gaps сьогодні, але фіскальний аудит може трактувати пропуски як приховування чеків. При 70 касах і нестабільному crypto/network — сотні gaps/місяць.

**Питання ревьюеру:** Який максимальний gap rate при нестабільному crypto/network? Чи є ops dashboard або audit query що робить lnd gaps видимими для compliance officer?

---

### R3 — Fiscal math precision

**Ризик:** `_calc_tax` в `dps_xml.py` використовує Python `round()` (banker's rounding). ФСКО може вимагати інший напрямок округлення (truncation, ceiling). 1 копійка розбіжність × тисячі чеків = розходження при звірці з ДПС = штрафи.

**Питання ревьюеру:** Чи визначає ФСКО v2.1.9 правило округлення для TXSM і DTSM? Чи `_calc_tax` відповідає йому точно? Чи перевірено що floor-division в валідаторі і banker's rounding в tax calculator не створюють internally inconsistent XML?

---

### R4 — MAC chain serialization

**Ризик:** Два concurrent документи для одного fiscal_number можуть прочитати різні `last_known_mac` / PAYLOAD_XML і обидва відправити incompatible MACs на ДПС. Результат: ERROR_BAD_HASH_PREV (-12) і потенційно зламаний ланцюг.

**Питання ревьюеру:** MAC computation серіалізований через весь pipeline (acquire→sign→send→finalize) per fiscal_number? Чи є вікно де два concurrent документи можуть прочитати різні MAC values?

---

### R5 — SQLite contention under 70 registers

**Ризик:** `BEGIN IMMEDIATE` бере write lock. З `busy_timeout=5000ms` і 70 concurrent requests — 71-й чекає 5 секунд і отримує SQLITE_BUSY. Burst сценарій: SHIFT_OPEN о 08:00 на 50 точках одночасно.

**Питання ревьюеру:** Production deployment використовує один SQLite файл per fiscal_number (рекомендовано) чи shared? Якщо shared — чи протестовано busy_timeout при 70 concurrent BEGIN IMMEDIATE? Чи є метрика SQLITE_BUSY?

---

### R6 — Signed bytes integrity (crypto round-trip)

**Ризик:** Якщо crypto sidecar або будь-який проміжний шар ре-серіалізує або нормалізує XML (наприклад, Unicode normalization кирилиці в `NM='ГОТІВКА'`), підписані байти відрізнятимуться від того що хешує ДПС → ERROR_VEREFY (-1).

**Питання ревьюеру:** Чи є тест що round-trips реальний Cyrillic XML через `build_dps_xml` → `SidecarCryptoProvider.sign_raw` → verify що signed CMS містить exact input bytes, byte-for-byte?

---

## Tier 2: Операційні ризики

Перевіряти при зміні **відповідного шару** (transport, crypto, offline, runtime, reconciliation).

### R7 — DPS rejection: terminal vs recoverable

**Ризик:** ERROR_NOT_OPEN_SHIFT (-15) класифікується як terminal (REJECTED), але операційно recoverable: оператор відкриває зміну і повторює. Документ permanently REJECTED, дані чеку втрачені з фіскальної перспективи.

**Питання:** Для кожного DPS rejection code що представляє correctable precondition — чи є operator workflow для re-issue? Чи idempotency key handling дозволяє повторну відправку семантично ідентичних чеків?

---

### R8 — Crypto sidecar timeout: thread leak

**Ризик:** `_stage_sign` створює `ThreadPoolExecutor(max_workers=1)` per crypto call. При sustained sidecar latency, TimeoutError не скасовує underlying HTTP call. 70 кас × sidecar outage = 70 orphaned threads per cycle → thread exhaustion.

**Питання:** Після `TimeoutError` — що стається з underlying HTTP connection до sidecar? Чи є connection-level timeout на `SidecarCryptoClient`? Чи `ThreadPoolExecutor` properly shutdown?

---

### R9 — Offline sync: no auto-trigger

**Ризик:** `OFFLINE_LOCAL_ACK` документи чекають вічно без operator-initiated `/v1/admin/offline-sync`. Каса що повернулася online має pending документи — але ніхто автоматично не запускає sync.

**Питання:** Чи є background scheduler або startup hook що автоматично викликає `OfflineSyncService.sync_pending` при переході OFFLINE→ONLINE?

---

### R10 — Migration safety on live SQLite

**Ризик:** `apply_migrations_to_connection` застосовує DDL в `BEGIN IMMEDIATE...COMMIT` per file. Якщо 009 проходить а 010 падає — БД в partially-migrated state. `auto_migrate=True` за замовчуванням — це відбувається при кожному рестарті.

**Питання:** Чи є pre-migration backup (WAL checkpoint + file copy) перед `auto_migrate` на production? Чи тестована деструктивна міграція (ALTER TABLE, table recreation) на атомічність?

---

### R11 — Observability: DPS rejection metrics

**Ризик:** "Скільки reject за годину по error code?" — відповідь тільки через SQLite scan audit_log. Health endpoint boolean (live/ready), не dimensional.

**Питання:** Чи може ops team відповісти на "скільки документів rejected ДПС за останню годину по error code" без query на SQLite audit_log? Чи є metrics increment per write_path outcome?

---

### R12 — Graceful shutdown: in-flight gRPC

**Ризик:** Shutdown drains ingress але не transport layer. Документ в `_stage_send_or_offline` з in-flight gRPC call — process killed after timeout → документ в SIGNED state назавжди, no inbox requeue.

**Питання:** Graceful shutdown скасовує in-flight gRPC calls? Чи є recovery path для документів в SIGNED state після unclean shutdown?

---

## Tier 3: Scale / performance

Перевіряти **перед production rollout** і при зміні persistence / transport шарів.

### R13 — Reconciliation / offline sync batch size

**Ризик:** 36г offline × 70 кас = тисячі документів. `get_pending_for_offline_sync` без LIMIT. Кожен документ → sequential gRPC call. Startup supervisor має 300s budget без batch-size cap → timeout з incomplete reconciliation.

**Питання:** Max document count після 36h offline? Чи є batch-size limit або progress checkpoint щоб partial progress committed?

---

### R15 — require_local_sign=false bypass in production

**Ризик:** `require_local_sign: false` в transport profile config silently bypasses local signing навіть якщо `crypto.provider=sidecar`. `_enforce_production_crypto_gate` перевіряє тільки тип CryptoProvider, але не конфігурацію профілів. Профіль з `require_local_sign=false` в production DB відправить документ без підпису.

**Питання:** Чи є будь-який транспортний профіль в production DB з `require_local_sign=false`? `_enforce_production_sign_gate` блокує startup якщо знаходить такий профіль — перевірено що startup проходить?

---

### R14 — Connection lifecycle: no pooling

**Ризик:** Нова `sqlite3.connect()` per context-manager + `PRAGMA synchronous = FULL` per connection. 70 concurrent requests = 70 simultaneous connections з PRAGMA overhead.

**Питання:** Load-tested при 70 rps з `synchronous=FULL` на target production storage (SD card / networked FS on edge hardware)? p99 latency?

---

## Використання

### При code review кожного степу

Ревьюер перевіряє:
1. Базові 4 виміри (коректність, архітектура, інваріанти, тести)
2. Tier 1 (R1-R6) — завжди
3. Tier 2 (R7-R12) — якщо зміна торкається відповідного шару
4. Tier 3 (R13-R14) — при зміні persistence/transport

### При acceptance gate (перед merge в main)

Всі 14 пунктів мають бути addressed: або "verified", або "not affected", або "known risk with mitigation plan".

### При production readiness review

Всі 14 пунктів мають бути "verified" з evidence (test name, load test result, ops procedure document).
