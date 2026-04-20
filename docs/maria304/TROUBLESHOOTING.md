# Maria 304 Driver — Troubleshooting

Quick triage for the most common issues.  Every diagnostic below
assumes you can read `journalctl -u maria304_driver -f` on the
driver host.

## "1C can connect but every command times out"

Cause: the Python bridge is unreachable.

```bash
# Verify connectivity from the driver host.
curl -sS $MARIA304_GATEWAY_URL/health
```

If that fails, check `bridge.gateway_url` in
`/etc/maria304_driver/config.yaml` and whether the Python gateway
process is up.  The driver returns `SOFTBLOCK` frames when the
bridge is down — 1C sees these as "апарат зайнятий" and retries.

## "Duplicate connect gets SOFTBLOCK"

That's the exclusion gate working as intended.  Real Maria hardware
accepts one connection at a time; the driver mirrors this.  Close
the first 1C session before opening a new one, or wait the 3-second
post-disconnect cooldown.

## "CSIN1 response looks corrupt on the client"

Fixed in the M6 review pass.  CSIN1 response is now written with
the pre-toggle CRC mode (off), matching how the OLE Manager reads
it.  If you still see this symptom, the driver is older than
`55b1227`; upgrade.

## "1C error: невідома команда"

The wire protocol received an opcode the driver does not yet model.
By default these return `DONE` so 1C never aborts; if you see
"невідома команда" the OLE Manager is interpreting something else
(config panel, firmware check).  Grab the frame trace:

```bash
curl -sS -H "Authorization: Bearer $MARIA304_ADMIN_TOKEN" \
     http://127.0.0.1:9202/admin/sessions
```

And share the output with the driver team.

## "Log is blank even though I see 1C traffic"

Either the log level is too high or the JSON subscriber was never
installed.  Verify:

```bash
MARIA304_LOG=debug systemctl restart maria304_driver
journalctl -u maria304_driver --since "1 minute ago"
```

If JSON lines don't appear, the binary may have been built without
`tracing-subscriber`.  Rebuild with the default features.

## "Python gateway sees envelopes but no DPS submission"

Deployment is in shadow mode.  Check:

```bash
grep -i mode /etc/maria304_driver/config.yaml
```

Should be `mode: live`.  If it says `shadow` or `dry-run`, the
Python side suppresses DPS calls intentionally — flip to `live` and
restart.

## "Admin API returns 403"

The bearer token doesn't match.  Compare:

```bash
echo $MARIA304_ADMIN_TOKEN
grep auth_token /etc/maria304_driver/config.yaml
```

Rotate both together; systemd `EnvironmentFile` is the usual place
to inject the env var.
