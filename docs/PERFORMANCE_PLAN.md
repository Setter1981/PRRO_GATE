# Performance Optimization Plan

**Дата:** 2026-04-14
**Статус:** Planned (post-pilot)
**Scope:** Per-cashdesk gateway topology (1 gateway / 2-3 registers)

---

## Філософія

**UX важливий на рівні відчуття, не секундомір.**

Касир не міряє мілісекунди — він відчуває "швидка каса" або "тормозить". Різниця між 350 мс і 500 мс — це різниця між задоволеним касиром і роздратованим.

Оптимізація для відчуття:
- Стабільна латентність (без спайків)
- Швидкий перший чек дня (не "запуск двигуна")
- Відсутність "зависань" що змушують дивитись на екран

---

## Топологія

One gateway per cash-desk (2-3 кас максимум). Не multi-tenant, не cloud-scale.

Це означає:
- GIL contention не проблема
- SQLite write lock не проблема
- Throughput не критичний
- **Per-receipt latency критична**
- **Memory footprint важливий** (edge hardware)
- **Startup time важливий** (reboot reliability)

---

## Реальні bottlenecks

### Breakdown типового SELL чека (online mode)

| Стадія | Час | Доля |
|--------|-----|------|
| REST parse + adapter | 1-5 мс | 1% |
| SQLite acquire | 2-10 мс | 2% |
| Guards | 1-3 мс | 1% |
| XML build | 1 мс | 0% |
| Crypto sign (sidecar) | 10-50 мс | 5-10% |
| **gRPC to DPS** | **100-500 мс** | **80-90%** |
| SQLite finalize | 2-10 мс | 2% |
| REST response | 1 мс | 0% |

**ДПС транспорт = 80-90% часу.** Решта оптимізацій помітні тільки якщо ДПС швидкий.

### Юридичне обмеження

У online режимі `server_fiscal_no` від ДПС **обов'язковий на чеку**. Без нього чек недійсний. Async DPS → офлайн номери → пропалюємо 168г/міс бюджет. Тому синхронний DPS roundtrip неможливо обійти.

---

## План оптимізацій (по порядку cost/benefit)

### Phase 1: gRPC tuning (найдешевше, найбільший UX виграш)

#### 1.1 Keep-alive gRPC connection

**Проблема:** кожен чек може створювати новий канал → TCP handshake + TLS handshake на кожен запит.

**Рішення:**
```python
grpc.secure_channel(
    endpoint,
    credentials,
    options=[
        ('grpc.keepalive_time_ms', 30000),
        ('grpc.keepalive_timeout_ms', 10000),
        ('grpc.keepalive_permit_without_calls', True),
        ('grpc.http2.max_pings_without_data', 0),
        ('grpc.http2.min_time_between_pings_ms', 10000),
        ('grpc.max_connection_idle_ms', 300000),
    ]
)
```

**Ефект:** TLS handshake платимо один раз, далі — одне RTT на чек.

#### 1.2 Shared channel між FN

**Проблема:** якщо канал створюється per fiscal_number — при кількох FN дублюємо handshakes і TCP з'єднання.

**Рішення:** один канал на DPS endpoint, HTTP/2 multiplexing обслуговує паралельні запити від різних FN.

**Ефект:** 
- Менше TCP з'єднань
- HTTP/2 multiplexing дозволяє паралельні SELL від різних кас без блокування
- Менше memory, швидший reconnect

#### 1.3 Pre-warming на SHIFT_OPEN

**Проблема:** перший чек дня платить повний handshake. Касир думає "чому так довго?"

**Рішення:** після успішного SHIFT_OPEN запустити фоновий `statusRro` ping який прогріває канал.

```python
def post_shift_open_hook(fiscal_number):
    threading.Thread(
        target=lambda: transport.ping(fiscal_number),
        daemon=True,
    ).start()
```

**Ефект:** перший продаж дня — швидкий. Касир не помічає "прогрів".

#### 1.4 Benchmark і proof

**Перед:** 1000 послідовних SELL без оптимізацій, p50/p90/p99
**Після:** 1000 послідовних SELL з оптимізаціями, p50/p90/p99

**Ціль:** p99 уникнути спайків, p50 стабільний без "першого чека".

**UX метрика:** різниця між першим і N-тим чеком ≤ 50 мс.

---

### Phase 2: prro_crypto Rust crate

**Вже описано в `RUST_CRYPTO_PLAN.md`.**

Короткий summary в контексті UX:
- Прибирає sidecar (один процес замість двох)
- Зменшує memory footprint
- Швидший startup після reboot
- Відкриває Android deployment
- Sign: 10-50 мс → ~5 мс (помітно на локальній частині)

Не революція в швидкості чека (бо ДПС 80% часу), але революція в архітектурі:
- Один binary замість Python + Node.js
- Без npm, без Node runtime, без HTTP sidecar
- Embedded-friendly

---

### Phase 3: Profiling-driven (якщо потрібно)

Після Phase 1+2 — виміряти реальний bottleneck через profiling.

Можливі кандидати на Rust extension:
| Кандидат | Потенційний виграш | Коли робити |
|----------|-------------------|-------------|
| MAC hash chain (previous_xml read + sha256) | 1-2 мс | Якщо профайл покаже hot path |
| XML builder (string concat) | <1 мс | Не варто, мінімальний |
| JSON serialize | 0 мс | Вже orjson (Rust) |
| SQLite writes | 0 мс | SQLite вже C, обмежено fsync |

**Принцип:** не переписуємо навмання. Numbers first, code second.

---

### Phase 4: "Fiscal core" на Rust (якщо потрібен справжній scale)

Опціонально. Тільки якщо:
- Per-cashdesk топологія змінилась на multi-register
- Потрібен міцний SLA
- Є ресурс на масштабну роботу

Обсяг: write_path, guards, serializer — все як Rust extension через PyO3.

Python стає тонкою обгорткою (REST, config, orchestration).

**Коштує:** місяці, велика зміна архітектури.
**Виграш:** GIL release, native performance, справжній embedded deployment.

---

## UX-critical metrics (моніторити в production)

| Метрика | Target | Alert |
|---------|--------|-------|
| p50 SELL latency | <150 мс | >300 мс |
| p99 SELL latency | <500 мс | >1000 мс |
| Перший чек після SHIFT_OPEN | <200 мс | >500 мс |
| Sign phase time | <20 мс | >100 мс |
| DPS roundtrip | <400 мс | >1000 мс |
| Gateway startup time | <2 с | >10 с |
| Memory footprint | <200 MB | >500 MB |

**Важливий принцип:** вимірюємо **p99, не average**. Касиру байдуже що в середньому швидко, якщо раз на 10 чеків — зависання.

---

## Порядок реалізації

1. **Sprint 10 завершити** (Step 10 + 11) — пілот-ready функціонал
2. **Benchmark baseline** — числа ДО оптимізацій
3. **Phase 1: gRPC tuning** — найдешевший UX win
4. **Benchmark** — підтвердити виграш
5. **Phase 2: prro_crypto Rust** — архітектурна зміна
6. **Benchmark** — остаточні числа
7. **Phase 3+** — за результатами профайлингу

---

## Важливо: шануємо ДПС сервер

ДПС — це shared infrastructure всієї країни. Агресивна оптимізація на нашому боці може спровокувати rate limiting або blacklisting.

**Принципи:**
- Keepalive не частіше 2 хвилин (не 30с)
- Pre-warming тільки там де реально треба (SHIFT_OPEN, не фоновий spam)
- Adaptive backoff при DPS_RATE_LIMITED
- Не тримати idle connections годинами — ДПС і так їх закриє

**Тестування перед rollout:**
1. Прогнати оптимізований gateway проти `cabinet.tax.gov.ua:9443` в реалістичному профілі
2. Моніторити rate limit responses
3. Peak hours тест (17-20) — найважчий

Якщо ДПС стабільно дає 400 мс — **це і є нормально**. Не ломимо сервер у двері заради -50 мс.

---

## Принцип

**Оптимізуємо досвід, а не секундомір.**

Касир має відчувати що каса "летить". Для цього потрібно:
- Немає "першого чека дня" синдрому
- Немає випадкових спайків
- Startup після reboot — швидкий
- Жодних зависань під час обслуговування клієнта

Секунди — це для звітів. Юзер дивиться на reaction на натиск кнопки.
