## v1.4.1

- Sync package root directory and version strings to 1.4.1.
- Wire write-path audit calls through AuditRepository.log_events() for batch-capable API usage.

## v1.4.0

- cached column-list generation for repository hot path
- reconciliation expected_states normalized to enums only
- graceful shutdown now waits for in-flight ingress operations up to configured timeout
- derived IDs reduce uuid usage on hot path
- protocol trace IDs made collision-safe without uuid4
- config gains graceful_shutdown_timeout_seconds

# Changelog

## v1.3.0
- removed staticmethod decorators from repository namespaces
- switched repository hot-path queries to explicit column lists
- added reconciliation optimistic locking via expected state checks
- moved persistent WAL setup to startup and kept per-connection PRAGMAs minimal
- adopted shared pytest conn fixture across repository/write-path/reconciliation/transport tests
- excluded __pycache__ and *.pyc from release artifacts

## v1.2.0
- refactored write path into staged pipeline helpers
- added structured logging module and runtime logging hooks
- added env var overrides for common config fields
- added graceful shutdown hooks to runtime container and shells
- added pytest conftest and runtime ops improvement tests

## v1.1.0
- runtime metrics collector and alert sink
- REST endpoints `/metrics` and `/v1/ops/summary`
