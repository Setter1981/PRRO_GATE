# Gap Registry — PRRO Gateway
**Зібрано:** 2026-04-15 після Sprint 11  
**Базова лінія:** 614 passed, 0 failed  
**Метод:** аудит коду + тестів + ACCEPTANCE_COVERAGE_SNAPSHOT  

---

## Легенда серйозності

| Позначка | Значення |
|---|---|
| 🔴 BUG | Ламає існуючу поведінку прямо зараз |
| 🟠 CRITICAL | Не дає вийти в production |
| 🟡 HIGH | Відкрита архітектурна дірка, операційний ризик |
| 🔵 MEDIUM | Відсутня функціональність, але обхід є |
| ⚪ LOW | Технічний борг, не впливає на коректність |

---

## ГРУПА A — Баги (треба фіксити незалежно від спринту)

| # | Проблема | Де | Серйозність |
|---|---|---|---|
| A1 | `GET_STATUS` не перехоплюється в `_MANAGEMENT_OPS` → write_path витрачає LND, намагається підписати і відправити в DPS | `write_path.py`, адаптери WebCheck/Maria | 🔴 BUG |
| A2 | `DPS_PRRO_XML_UNIFIED_WINDOW` завжди `DpsXmlUnifiedWindowTransportStub()` в container незалежно від env (на відміну від GRPC що має `!= 'development'` умову) | `container.py:349` | 🔵 MEDIUM (post-pilot) |

---

## ГРУПА B — Офлайн-режим (пріоритет)

| # | Проблема | Де | Серйозність |
|---|---|---|---|
| B1 | `GO_OFFLINE` не перевіряє наявність активного `offline_ranges` → оператор йде в офлайн без кодів, перший SELL падає з `OFFLINE_CODES_EXHAUSTED` | `write_path.py:1286` | 🟡 HIGH |
| B2 | Клієнт дізнається про вичерпання кодів тільки при спробі пробити чек, не при `GO_OFFLINE` | `write_path.py:215-219` | 🟡 HIGH |
| B3 | Немає попередження "залишилось N кодів" — нема API, нема audit event | `repositories/offline.py` | 🔵 MEDIUM |
| B4 | `OfflineSyncService.sync_pending()` не тригериться автоматично — тільки ручний виклик через `/v1/admin/offline-sync` | `offline_sync.py`, `rest_app.py` | 🟡 HIGH |
| B5 | `OfflineSyncService` і `ReconciliationService` — два незалежні шляхи відновлення, не скоординовані. `OFFLINE_LOCAL_ACK` reconciliation не торкає | `reconciliation.py`, `offline_sync.py` | 🟡 HIGH |
| B6 | Немає DPS-клієнта для запиту офлайн-кодів. `ASK_OFFLINE_CODES` приймає ready-made range, але хто його отримує від DPS — не визначено | архітектура | 🔵 MEDIUM |
| B7 | Немає API endpoint `/v1/admin/request-offline-codes` | `rest_app.py` | 🔵 MEDIUM |
| B8 | `GOING_OFFLINE` і `GOING_ONLINE` є в enum але `update_mode()` пише цільовий стан напряму без проміжного | `write_path.py:1300,1338`, `node_state.py:14` | ⚪ LOW |

---

## ГРУПА C — Node state machine

| # | Проблема | Де | Серйозність |
|---|---|---|---|
| C1 | Crypto circuit breaker відкрито (`crypto_breaker_open=True`) але `node_state.mode` залишається `ONLINE` — зовнішній моніторинг не бачить деградації | `write_path.py:376`, `enums.py:97` | 🟡 HIGH |
| C2 | `NodeMode.BLOCKED` — є в enum, ніколи не встановлюється, write_path не блокує при цьому режимі | `enums.py:95` | 🔵 MEDIUM |
| C3 | `NodeMode.STOP_MODE` — є в enum, планується для corruption detect, але backup-сервісу не існує → ніколи не встановлюється | `enums.py:96` | 🔵 MEDIUM |
| C4 | `NodeMode.CRYPTO_DEGRADED` — дублює C1, ніколи не встановлюється | `enums.py:97` | 🔵 MEDIUM |

---

## ГРУПА D — Поля бази даних без логіки

| # | Поле | Таблиця | Статус | Серйозність |
|---|---|---|---|---|
| D1 | `last_fs_ping_at` | `node_state` | Ніколи не пишеться, ніколи не читається | ⚪ LOW |
| D2 | `last_integrity_check_at` | `node_state` | Ніколи не пишеться | ⚪ LOW |
| D3 | `last_backup_at` | `node_state` | Ніколи не пишеться (backup-сервіс відсутній) | ⚪ LOW |

---

## ГРУПА E — Admin / Operator API

| # | Endpoint | Нащо потрібен | Серйозність |
|---|---|---|---|
| E1 | `GET /v1/admin/node-state` | Поточний mode, offline session, crypto breaker | 🟡 HIGH |
| E2 | `GET /v1/admin/offline-ranges` | Скільки кодів залишилось, статус range | 🟡 HIGH |
| E3 | `GET /v1/admin/offline-sessions` | Активна сесія, час офлайн, accumulated | 🟡 HIGH |
| E4 | `POST /v1/admin/reconciliation/trigger` | Явний старт reconciliation без рестарту | 🔵 MEDIUM |
| E5 | `POST /v1/admin/request-offline-codes` | Тригер запиту кодів у DPS (майбутнє) | 🔵 MEDIUM |

---

## ГРУПА F — Operational сервіси (не існують)

| # | Сервіс | Що потрібно | Серйозність |
|---|---|---|---|
| F1 | Backup job | `sqlite3.Connection.backup()`, rotation, integrity check, STOP_MODE при corruption | 🟠 CRITICAL |
| F2 | Retention / purge | DELETE старих audit/trace/inbox при досягненні TTL. Fiscal docs — ніколи | 🟡 HIGH |
| F3 | Rate limiting на ingress | Middleware в `rest_app.py`, per fiscal_number або per IP | 🔵 MEDIUM |
| F4 | Request size limits | HTTP 413 при payload > max | 🔵 MEDIUM |

---

## ГРУПА G — Fiscal Compliance (з попереднього плану Sprint 12)

| # | Що | Серйозність |
|---|---|---|
| G1 | Excise goods E2E pipeline тест (код є, тесту нема) | 🔵 MEDIUM |
| G2 | Cash balance carry-over між зміна 1 → зміна 2 (тест) | 🔵 MEDIUM |

---

## ГРУПА H — Production infrastructure (з попереднього плану Sprint 13)

| # | Що | Серйозність |
|---|---|---|
| H1 | DPS Unified Window — реальна реалізація замість stub | 🔵 MEDIUM (post-pilot) |
| H2 | Crypto sidecar — TLS + mutual auth | 🟠 CRITICAL |

---

## ГРУПА I — Технічний борг

| # | Що | Серйозність |
|---|---|---|
| I1 | Тести без pytest markers (unit/integration/e2e) — все запускається разом | ⚪ LOW |
| I2 | Operational docs: OFFLINE_SYNC.md, DPS_TRANSPORT.md, PROTOCOL_SHAPE_AUDIT.md | ⚪ LOW |

---

## Розкладка по спрінтах (чернетка для обговорення)

### Sprint 12 — Офлайн mode completeness (пріоритет)

**Мета:** закрити всі критичні offline gap-и, включно з адмін API для операторів.

| Gap | Тип роботи |
|---|---|
| A1 — GET_STATUS hotfix | мінімальний diff в write_path, додати в _MANAGEMENT_OPS |
| B1 — GO_OFFLINE перевірка кодів | guard в _handle_management_command_locked |
| B4 — OfflineSyncService auto-trigger | інтеграція в reconciliation loop або окремий scheduler |
| B5 — OfflineSync + Reconciliation координація | узгодити стани, один шлях відновлення |
| E1 — GET /v1/admin/node-state | новий endpoint |
| E2 — GET /v1/admin/offline-ranges | новий endpoint |
| E3 — GET /v1/admin/offline-sessions | новий endpoint |
| C1 — CRYPTO_DEGRADED при відкритому breaker | update_mode при threshold |

---

### Sprint 13 — Fiscal compliance + Production infra

| Gap | Тип роботи |
|---|---|
| G1 — Excise E2E тест | тест |
| G2 — Cash balance carry-over тест | тест |
| H2 — Crypto sidecar TLS | Node.js sidecar hardening |
| F3 — Rate limiting | middleware |
| F4 — Request size limits | middleware |

---

### Sprint 14 — Operational safety + Pilot

| Gap | Тип роботи |
|---|---|
| F1 — Backup job + STOP_MODE | новий сервіс + scripts |
| F2 — Retention / purge | новий сервіс + migration |
| E4 — POST /admin/reconciliation/trigger | endpoint |
| I1 — pytest markers | технічний борг |
| I2 — operational docs | документація |
| Pilot acceptance matrix | ручна/авто верифікація |

---

### Post-pilot — DPS Unified Window

| Gap | Тип роботи |
|---|---|
| H1 — DPS Unified Window реалізація | нова реалізація транспорту (потребує endpoint spec від DPS) |
| A2 — Замінити stub на реальний transport | wiring в container.py |

---

## Відкриті питання для обговорення

1. **B4/B5 (auto-sync)** — куди вбудовувати: в reconciliation loop, окремий thread, або cron-like scheduler? Reconciliation вже має свій timer.
2. **B6/B7 (offline codes від DPS)** — це окремий API до DPS (DPS Unified Window API?), чи окремий flow поза gateway?
3. **A2/H1 (Unified Window)** — перенесено в post-pilot. Потребує endpoint spec від DPS перед реалізацією.
4. **C2/C3/C4 (BLOCKED, STOP_MODE)** — чи хочемо їх реалізувати, чи залишити в enum як резерв?
5. **Sprint 12 scope** — чи вміщується C1 (CRYPTO_DEGRADED) в Sprint 12 або перенести в 13?
