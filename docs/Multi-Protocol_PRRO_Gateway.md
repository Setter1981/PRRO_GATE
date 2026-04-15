ТЕХНИЧЕСКОЕ ЗАДАНИЕ
Multi-Protocol PRRO Gateway
Финальная редакция v1.1
Локальный шлюз ПРРО с поддержкой нескольких протоколов,
offline-режима, локального архива, канонической модели,
совместимости с Checkbox/WebCheck/Maria и транспортных профилей ДПС.

Статус	Готово к передаче в разработку
Основной контур	Phase 1.0 / MVP
Следующий контур	Phase 1.1 (без блокировки старта)
Язык документа	Русский
Назначение	Архитектурный и инженерный контракт для реализации системы
Документ разделен на обязательный контур MVP и перечень ближайших усилений Phase 1.1.
 
Краткое резюме
Система создается как универсальный локальный PRRO gateway, который принимает команды из нескольких несовместимых протоколов, приводит их к единой canonical model, последовательно обрабатывает критические операции и обеспечивает офлайн-режим, локальный архив, recovery и трассировку.
Ключевая идея архитектуры: источник истины - локальная SQLite БД, а не in-memory очередь. Внешние протоколы изолированы от внутренней модели. Криптография вынесена в sidecar. Для каждого fiscal number поддерживается собственный stateful write-path и собственный channel lock.
Документ фиксирует как обязательные требования MVP: schema versioning, contract testing адаптеров, SQLite backup/corruption policy, health/readiness/startup probes, timeout policy, rate limiting, graceful shutdown, retention policy, явный контракт Cloud Hub API и двухслойную проверку channel lock.
Оглавление
•	Часть A. MVP / Phase 1.0
•	1. Преамбула и цели
•	2. Архитектурные инварианты
•	3. Логическая архитектура
•	4. Протоколы, backend и transport abstraction
•	5. Изоляция по fiscal number и ingress model
•	6. Функциональный объем MVP
•	7. Canonical model и schema versioning
•	8. Акцизные марки, эквайринг и печать
•	9. Shift, channel lock и offline
•	10. Inbox, SQLite, backup и corruption policy
•	11. Таблицы БД и state machines
•	12. Tracing, crypto, health и graceful shutdown
•	13. Contract testing, Cloud Hub API, метрики и приемка
•	Часть B. Phase 1.1
•	14. Deployment topology
•	15. Upgrade / rollback
•	16. Migration strategy
•	17. Расширенные business metrics
•	18. Feature flags / config model
•	19. Конкурентные запросы от нескольких POS
Часть A. MVP / Phase 1.0
1. Преамбула и цели
На украинском рынке вокруг ПРРО сосуществуют несколько несовместимых контуров: Checkbox-style REST, XML-RPC/HTTP интеграции в стиле WebCheck, бинарные TCP-протоколы в стиле «Мария», а также разные транспортные профили налоговой службы. Задача проекта - стабилизировать эти интеграции в одной локальной точке обработки.
Продукт не является только адаптером под Checkbox. Он должен работать как собственное локальное ядро ПРРО с pluggable backend/transport layer, при этом сохраняя совместимость с уже существующими фронтами и драйверами.
Основные цели системы:
•	поддержка нескольких входных протоколов без доработки существующего фронта;
•	транзакционная обработка чеков, смен, offline-кодов и локальных номеров документов;
•	предсказуемый recovery после crash, timeout, частичной отправки и сбоев криптографии;
•	локальный архив и выгрузка в Cloud Hub;
•	совместимость со старыми front-office контурами через Checkbox/WebCheck/Maria ingress при целевом direct DPS egress.
2. Архитектурные инварианты
Источник истины. Источником истины по состоянию документа, смены, офлайна и recovery является SQLite hot store, а не in-memory очередь.
Single writer. Для одного fiscal number существует ровно один логический writer на write-path.
Channel lock. После открытия смены фиксируется backend profile, transport profile, integration owner и протокольный адаптер. До закрытия смены переключение канала запрещено.
Идемпотентность. Один и тот же idempotency_key не может породить повторную фискализацию.
Offline. Offline допускается только при отсутствии связи, наличии кодов, непревышении лимитов времени и корректном состоянии смены/кассы.
Protocol compatibility. WebCheck и Maria считаются поддержанными только если существующий фронт работает без своей доработки.
Version safety. Все canonical envelope, архивные записи и payload для Cloud Hub обязаны содержать schema_version.
3. Логическая архитектура
Edge Node состоит из ingress adapters, durable inbox, internal queue, fiscal worker, crypto sidecar/provider, SQLite hot store, filesystem cold store, sync outbox, health endpoints и trace-подсистем.
Cloud Hub состоит из API приема документов и отчетов, архива, ERP API, health/monitoring и конфигурационного канала.
Слой	Назначение	Ключевые требования
Ingress adapters	Прием REST / XML-RPC / TCP-команд	Нормализация, idempotency key, durable accept, rate limiting
Durable inbox	Буфер и источник истины для новых команд	Lease model, crash safety, retry
Fiscal worker	Single writer на один fiscal number	Channel lock, LND/offline allocation, state machine
Crypto sidecar	Изолированная подпись и работа с ключом	Timeout, circuit breaker, ERROR_RETRYABLE
Hot/Cold store	Состояние и архив	SQLite WAL, backups, corruption policy, archival files
Cloud Hub	Центральная агрегация и управление	Versioned API contract, idempotent intake
4. Протоколы, backend и transport abstraction
Система поддерживает три обязательных входных протокола: Checkbox-style REST, WebCheck XML-RPC/HTTP и бинарный TCP-протокол «Мария». Для WebCheck и Maria используется registry model со статусами SUPPORTED, UNSUPPORTED, PLANNED, EXPERIMENTAL.
Не реализованные команды Maria не разрывают соединение без необходимости: они распознаются, логируются и возвращают штатную протокольную ошибку unsupported/not implemented.
Backend abstraction разделяется на два уровня:
•	IFiscalBackend - бизнес-интерфейс: open_shift, close_shift, sale, return, service_in, service_out, cash_withdrawal, x_report, z_report, go_offline, go_online, ask_offline_codes, get_status, reconcile;
•	ITransportProfile - транспортный интерфейс: serialize, sign, encrypt_if_needed, send, poll_status, parse_response, retry_strategy.
Минимальные backend profiles: CHECKBOX_CLOUD_COMPAT, LOCAL_PRRO_CORE, DPS_DIRECT, CUSTOM_VENDOR_BACKEND. Минимальные transport profiles: CHECKBOX_REST_TRANSPORT, DPS_PRRO_GRPC_ECABINET, DPS_PRRO_XML_UNIFIED_WINDOW.
Для каждого transport profile должны быть явно заданы connect timeout, request timeout, poll timeout, retry backoff, максимальное число повторов и поведение при timeout.
Важно: `Checkbox-style REST` — ingress compatibility layer, а `CHECKBOX_REST_TRANSPORT` — только compatibility/migration contour. Целевой production egress для проекта — прямые transport profiles ДПС.
5. Изоляция по fiscal number и ingress model
Базовая единица изоляции - fiscal number. Для каждого fiscal number изолируются state machine, shift state, offline state, LND sequence, offline codes, document archive, outbox, traces, metrics и worker.
Рекомендуемая реализация: отдельная SQLite БД, отдельный worker и отдельный архивный корень на один fiscal number.
В MVP на ingress отсутствует application-level auth. Edge node должна работать в доверенном локальном сегменте и не публиковаться напрямую в интернет.
Так как auth отсутствует, ingress обязан иметь rate limiting не менее чем per source IP/session с конфигурируемыми лимитами и логированием превышений.
6. Функциональный объем MVP
•	open shift / close shift;
•	sale / return;
•	service in / service out;
•	cash withdrawal как отдельный тип операции;
•	X-report / Z-report;
•	go offline / go online / ask offline codes / refill offline codes;
•	receipt status / cash register status / reconciliation;
•	контроль уникальности акцизных марок;
•	printable receipt и formatted print lines;
•	protocol trace и transport trace.
7. Canonical model и schema versioning
Все canonical envelope обязаны содержать schema_version, created_at, producer, payload_type и payload. Любое расширение модели допускается только как backward-compatible evolution с сохранением возможности читать старые записи и архивы.
CanonicalFiscalCommand включает request_id, idempotency_key, protocol, operation_type, fiscal_number, route_key, backend_profile_id, transport_profile_id, channel_owner, business_ts, payload_json, payload_sha256 и schema_version.
Receipt header должен поддерживать как минимум receipt_type, shift_id, status, business_ts, offline fields, fiscal fields, related_receipt_id, previous_receipt_id, technical_return, header/footer, barcode, delivery и rounding_enabled.
Goods должны хранить code, good_id, name, uktzed, barcode, excise_barcodes, price, quantity, sum, discounts и item_attributes_json.
Payments должны хранить не только сумму и тип оплаты, но и эквайринговые поля: commission, card_mask, bank_name, auth_code, rrn, payment_system, owner_name, terminal, acquirer_and_seller, receipt_no, signature_required, acquiring_source и acquiring_payload_json.
8. Акцизные марки, эквайринг и печать
Система должна предотвращать повторную продажу одной и той же акцизной марки. Нормализованная марка не может одновременно существовать в состояниях RESERVED или SOLD более одного раза.
Если эквайринговые данные не переданы, это не считается ошибкой: чек формируется штатно, а эквайринговый блок не печатается.
Минимальные printable modes MVP: HTML и TEXT. Кроме printable receipt обязателен режим formatted print lines.
На печать выводятся только фактически переданные поля. Пустые блоки, заголовки и подписи не отображаются.
9. Shift, channel lock и offline
При открытии смены фиксируются opened_via_backend_profile_id, opened_via_transport_profile_id, opened_via_protocol, opened_via_integration_owner и channel_lock_acquired_at.
Проверка channel lock выполняется в два слоя: быстрый reject на ingress и авторитетная проверка в worker внутри транзакции.
При нарушении возвращается canonical error SHIFT_CHANNEL_SWITCH_FORBIDDEN.
Offline ограничен 36 часами подряд и 168 часами в календарный месяц. Система обязана показывать остаток времени, останавливать новые операции при достижении лимита и автоматически возвращаться в online при восстановлении связи.
Для offline codes используются low watermark, target watermark, hard stop threshold и refill batch size.
10. Inbox, SQLite, backup и corruption policy
Любая команда сначала записывается в SQLite inbox. Worker использует lease model со статусами NEW, PROCESSING, DONE, ERROR и DEAD.
Hot store работает в WAL-режиме. Для критических write-path используется BEGIN IMMEDIATE.
В MVP обязателен backup/corruption policy:
•	периодический PRAGMA integrity_check;
•	online backup strategy;
•	явное поведение при SQLITE_CORRUPT;
•	процедура восстановления из cold archive и Cloud Hub;
•	решение: stop mode или degraded mode при corruption.
11. Таблицы БД и state machines
Минимальный набор таблиц: node_state, backend_profiles, transport_profiles, prro_bindings, ingress_inbox, shifts, offline_ranges, offline_sessions, fiscal_documents, fiscal_document_goods, fiscal_document_payments, excise_marks, document_files, sync_outbox, protocol_trace_log, transport_trace_log, audit_log, schema_registry.
Обязательные state machines:
•	Inbox: NEW -> PROCESSING -> DONE / ERROR / DEAD;
•	Document: PREPARED -> SIGNED -> ENCRYPTED -> SENT -> KVT1 -> KVT2 -> ACK / REJECTED / ERROR_RETRYABLE;
•	Shift: CREATED -> OPENING -> OPENED -> CLOSING -> CLOSED / ERROR;
•	Offline session: OPENING -> OPEN -> CLOSING -> CLOSED / ABORTED.
12. Tracing, crypto, health и graceful shutdown
Protocol trace ведется для REST, WebCheck и Maria. Transport trace ведется для backend и transport profile. Для trace и audit обязательно определяется retention policy: TTL хранения в hot store, перенос в cold/cloud, purge policy и лимиты на размер.
Crypto sidecar в MVP работает с файловым ключом и encrypted pin cache без machine binding.
Для crypto sidecar должны быть определены timeout, retry count, circuit breaker и перевод документа в ERROR_RETRYABLE. Сбой sidecar должен изолироваться на один fiscal number и не подвешивать весь edge.
Обязательные health endpoints: liveness, readiness и startup. Readiness поднимается только после завершения recovery.
Graceful shutdown обязателен: при SIGTERM система перестает принимать новые команды, завершает текущую критическую операцию с timeout, корректно закрывает trace и освобождает lease либо переводит документ в согласованное промежуточное состояние.
13. Contract testing, Cloud Hub API, метрики и приемка
Обязательная стратегия adapters -> canonical model: golden files формата raw protocol request -> expected CanonicalFiscalCommand. Покрытие должно включать REST, WebCheck, Maria, negative cases и unsupported methods.
Cloud Hub API должен быть описан явно. Для heartbeat, documents, reports, metrics, alerts, config и archive intake фиксируются request schema_version, response schema_version, idempotency strategy, retry behavior и ordering expectations.
Для sync outbox глобальный порядок не гарантируется. Порядок гарантируется только per fiscal_number и per artifact class; Hub должен уметь обрабатывать out-of-order arrival между разными fiscal number.
Минимальные технические метрики MVP: inbox backlog, pending docs, sign time, backend RTT, SQLITE_BUSY, SQLITE_CORRUPT incidents, offline time, offline codes available.
Минимальные бизнес-метрики MVP: receipts count, returns count, offline receipts count, total sales sum, average receipt amount.
Приемка MVP включает sale/return/service operations, cash withdrawal, open/close shift, channel switch forbidden, offline enter/exit, offline limit reached, excise duplicate blocked, acquiring present/absent, Maria unsupported method, recovery after crash, readiness after recovery, corruption procedure, graceful shutdown и Hub contract compatibility.
Часть B. Phase 1.1
14. Deployment topology
Во втором контуре требуется формализовать варианты развертывания: Windows bare metal, Linux Docker, VM deployment, минимальные требования по CPU/RAM/disk, правила назначения портов, допустимость нескольких edge-инстансов на одном хосте и ограничения по файловой системе и backup media.
15. Upgrade / rollback
Должны быть описаны порядок обновления edge node, совместимость версий БД, стратегия миграций, rollback policy и поведение при неуспешном обновлении в рабочее время.
16. Migration strategy
Необходимо формализовать первый запуск на точке, сценарии старта «с чистого листа», импорт состояния из прежней системы, dual-run или shadow mode при необходимости и порядок cutover с существующих решений.
17. Расширенные business metrics
Следующим этапом добавляются sales sum by shift, returns ratio, average check trend, suspicious spikes, offline share, anomaly thresholds и дополнительные operational dashboard-метрики на edge.
18. Feature flags / config model
Должна появиться формализованная модель конфигурации: где хранится конфиг, как он версионируется, как обновляется, поддерживается ли hot reload, как организованы feature flags для новых методов и backend profiles и как работает override hierarchy.
19. Конкурентные запросы от нескольких POS на один fiscal number
Phase 1.1 должен явно описать политику concurrent access: можно ли подключать несколько POS к одному fiscal number, кто получает синхронный отказ, кто ждет, как работает arbitration и какой ответ получает клиент при состоянии «касса занята».
 
Итог по приоритетам
Ниже приведено фиксированное разделение требований между обязательным MVP и ближайшим усилением Phase 1.1.
Обязательный контур MVP	Следующий контур Phase 1.1
schema versioning	deployment topology
contract testing	upgrade / rollback
SQLite backup / corruption policy	migration strategy
health / readiness / startup probes	expanded business metrics
timeout and circuit-breaker policy	feature flags / config model
rate limiting on ingress	formal POS concurrency policy
graceful shutdown	
retention policy	
explicit Cloud Hub API contract	
dual enforcement of channel lock	
Документ предназначен для передачи в разработку как инженерный контракт. При необходимости на его основе могут быть выпущены отдельные приложения: SQL DDL, canonical JSON schemas, acceptance test matrix и Cloud Hub API specification.
