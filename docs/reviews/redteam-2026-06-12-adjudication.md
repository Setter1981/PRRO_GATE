# Сторонний red-team аудит (2026-06-12) — адъюдикация архитектора

**Источник:** независимый внешний аудитор (другая лаборатория), линзы A1-A4
(attack-surface), 6 кандидатов FT/HIGH + секция verified-defences.
**Адъюдикация:** Fable 5, каждый кандидат перепроверен в коде лично, 2026-06-12.
**Контекст:** inline write-path (A2.1b) dormant — supervisor wires
UnimplementedWritePath; live-поверхности сегодня: boot reconciliation, B1 tick,
offline drain, ingress shell.

## Рулинги

### RT-1 | FT | CONFIRMED — double-send через ER-redrive без probe
`error_routing.rs:283` — `DpsError::Transport(_)` (включая ambiguous timeout
ПОСЛЕ возможного приёма ДПС) → `TransientRetry` → doc `ErrorRetryable`.
`boot_phase.rs:2637-2640` — `ErRedriveDecision::Redrive` → `stage_send::run`:
реальный повторный `send_chk` ТОГО ЖЕ конверта, без probe-резолюции, до
MAX_BOOT_ATTEMPTS (=5) суммарных send.
**Reachability СЕГОДНЯ:** drain шлёт OFFLINE_LOCAL_ACK через stage_send (W9a);
transport-timeout на drain → ER/TransientRetry → следующий boot re-send. Не
только future-A2.4.
**Ограничители:** конверт байт-идентичен (тот же doc_id/lnd/MAC) — исход
зависит от ДПС-семантики дубликата идентичного подписанного конверта
(неизвестна; в goldens/эмпирике нет случая повторной подачи).
**Это материализация ИЗВЕСТНОГО отложенного пункта** «ER-redrive + K3-probe без
sfn (нужна протокол-спека)» (B1 v1 deferred list) — теперь с конкретным
attack-нарративом и FT-меткой.
**Disposition:** протокол-спека probe-before-redrive — ПЕРЕД пилотом (drain
live). Кандидат-механика: split `Transport` на ConnectFail (pre-wire, retry
безопасен) vs AmbiguousTimeout (post-wire, ProbeRequired); НО
`HoldProbeRequired` сегодня без резолвера (M5 reconciler pending) — одна смена
классификации застрянет доки навсегда. Спека обязана решить резолюцию probe
без sfn (match по lnd/контенту lastChk). Привязка: M5 (transports/classify) +
отдельная спека Fable. Доп.: live-DPS эксперимент «повторная подача идентичного
конверта» в тест-кампанию.

### RT-2 | FT | CONFIRMED (known-documented gap) — нет резерва кода под offline Z
`stage_offline_ack.rs` Step 6 — `acquire_code_tx` без проверки остатка;
`OFFLINE_CODE_RESERVED_FOR_CLOSE` в коде отсутствует (0 hits).
LEGAL_INVARIANTS.md «Hard close-code reserve = 1» — правило W10 с явной
пометкой **implementation pending**. Репро аудитора валиден; находка = известный
задокументированный пробел, не пропущенный дефект.
**Disposition:** реализовать до пилота. Малый, хорошо специфицированный фикс:
отказ обычного SELL/RETURN/SERVICE_* при последнем `consumed_at IS NULL` коде
(shift open && локальный Z ещё не эмитирован) → refusal
`OFFLINE_CODE_RESERVED_FOR_CLOSE`; offline Z_REPORT может потреблять резерв.
Естественная пара к RT-5 одним offline-hardening батчем ПОСЛЕ ревью M2
(offline_sync — следующий модуль legacy-ревью; фикс по горячим следам досье).

### RT-3 | HIGH (понижено с FT) | PARTIALLY CONFIRMED — RETURN без линковки
Факты подтверждены: `dto.rs:58` парсит `return_check_number`, дальше поле
ТЕРЯЕТСЯ (0 проводок в canonical/XML). НО: wire-диалект WebCheck вообще НЕ
имеет поля линковки (OperationType 0/1 — всё; Python-референс идентично терял
поле; ORDERRETNUM отсутствует в обоих стеках) — это реальность канала, не дрейф
шлюза. Локальный пробел реален: шлюз принимает произвольный RETURN без
локальной валидации оригинала; `related_receipt_id` в схеме нет — локальная
форензика не свяжет возврат с оригиналом.
**Мета-находка M-1 (важнее самого кандидата): в goldens НОЛЬ чеков возврата** —
RETURN-путь не покрыт эмпирикой вовсе; поведение живой ДПС на возвраты
неизвестно (включая enforcement линковки на их стороне).
**Disposition:** (1) тест-кампания ОБЯЗАНА включить живые возвраты (WebCheck-
корпус оператора); (2) policy-решение до пилота: локальная валидация RETURN
(lookup оригинала в ledger; configurable strictness — оригинал мог быть пробит
до установки шлюза/на другой кассе, hard-fail по умолчанию нельзя);
(3) кандидат в схему: nullable `related_receipt_id` (migration reasoning
отдельно). Понижение до HIGH: шлюз не ломает wire-контракт, отсутствует
defense-in-depth.

### RT-4 | FT by construction | ACCEPTED-RISK (pilot) — клон БД = два писателя FN
`singleton.rs:25` — PidLock = advisory file-lock по ПУТИ файла БД; FnWriteGate
in-process. Копия БД на второй путь/узел → два независимых шлюза одного FN:
дубликаты lnd, та же ветка previous_hash, offline — оба клона раздают локальные
ACK из копии одного пула кодов (DUR-1-класс ущерба при drain).
Локально непредотвратимо в принципе (клон байт-идентичен). Online-сторона
частично самозащищается: tip-guard (PR#141/146) блокирует отставший клон на
boot-wire; drain-конфликт ловится ДПС. Offline-окно — реальная экспозиция.
**Disposition:** принятый риск пилота + явная строка в licensing/monitoring
backlog: «FN split-brain detection» (dual-server alert уже в licensing-плане;
monitoring v1 — twin-instance heartbeat). Документировать в RUNBOOK как
запрещённую операторскую операцию (копия БД на второй узел).

### RT-5 | FT | CONFIRMED (known-documented active risk) — 36h/168h не enforced
LEGAL_INVARIANTS.md: строки 24h/36h/168h помечены «Active engineering risk —
not yet enforced in the Rust gateway; must be enforced before production OR
explicitly risk-accepted with sign-off». Код подтверждает: offline-роутинг без
elapsed/monthly-budget проверок. Сам аудитор отмечает «not a surprise bug».
**Disposition:** frozen invariant 5 — enforcement ДО пилота (или подписанный
risk-accept в pilot log). Дом реализации: stage_acquire/stage_offline_ack
elapsed-чеки от `offline_sessions.opened_at` + месячный бюджет. В один
offline-hardening батч с RT-2 после ревью M2.

### RT-6 | MED (понижено с HIGH) | CONFIRMED by design — loopback без authn
Non-loopback fail-closed на старте (верифицировано аудитором); на loopback
authn нет by design (пилотная поза, локальный POS). `signer_guard` — integrity-
чек (равенство открывшему смену), не authz. Понижение: фискальная подпись
делается ключом узла НЕЗАВИСИМО от заявленного cashier_id — юридическая
идентичность подписи не спуфится; спуфится только bookkeeping-метка в локальном
ledger. Злонамеренный локальный софт на том же хосте и так вне модели угроз
пилота (он мог бы просто слать SELL).
**Disposition:** принятая пилотная поза; authn/authz токены — post-pilot
(licensing/UI backlog). Зафиксировать residual в SECURITY-доке.

## Verified defences (аудитором; принято без перепроверки)
Idempotency replay/conflict; non-loopback fail-closed; X_REPORT read-only;
empty CheckAck.id не становится SENT; double-consume кода под BEGIN IMMEDIATE +
partial unique index; Blocked/StopMode/CryptoDegraded/GoingOnline отказ acquire.

## Сводка дispositions
| ID | Вердикт | Куда |
|---|---|---|
| RT-1 | FT CONFIRMED | спека probe-before-redrive (Fable) до пилота; M5-привязка; live-DPS эксперимент дубликата |
| RT-2 | FT known-gap | offline-hardening батч после M2 (reserve=1) |
| RT-3 | HIGH partial | тест-кампания (живые возвраты!) + policy до пилота + schema-кандидат |
| RT-4 | accepted-risk | licensing/monitoring backlog (split-brain detection) + RUNBOOK |
| RT-5 | FT known-gap | offline-hardening батч после M2 (36h/168h + 24h) |
| RT-6 | MED accepted | post-pilot auth backlog; residual в SECURITY-док |

Мета: M-1 — ноль RETURN-эмпирики в goldens (кормит тест-кампанию).
Кросс-чек: RT-1 усиливает приоритет M5 в legacy-ревью (таксономия ошибок —
линза L4 уже включает класс «timeout ≠ not-delivered»).
