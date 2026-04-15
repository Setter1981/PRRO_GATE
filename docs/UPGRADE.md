# UPGRADE

## Current target

This package is `v1.0-rc1` and upgrades from `v0.9.1` via the same SQLite migration runner.

## Procedure

1. Stop ingress service gracefully.
2. Take SQLite online backup.
3. Install new package code.
4. Start service; startup supervisor runs migration phase automatically.
5. Confirm `/health/startup` and `/health/ready` are green.

## Rollback

1. Stop service.
2. Restore previous code bundle.
3. Restore SQLite backup only if a migration changed schema incompatibly.
4. Restart and confirm health endpoints.
