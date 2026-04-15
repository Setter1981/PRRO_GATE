# OPERATIONS

## Health endpoints

- `/health/live`
- `/health/ready`
- `/health/startup`

Each endpoint includes the current runtime `phase`.

## Startup phases

- `CREATED`
- `PHASE1_STARTING`
- `PHASE1_COMPLETE`
- `PHASE2_RECONCILING`
- `RUNNING`

## Expected production mode

`runtime.process_immediately = false`

Ingress should durable-accept commands and let the worker process them out-of-band.
`process_immediately = true` remains a dev/test convenience only.
