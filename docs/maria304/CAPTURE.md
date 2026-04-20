# Collecting a Capture for Support

When a bug is hard to reproduce, the fastest way to get help is to
ship the driver team a capture of the failing session.  The admin
API produces everything needed without touching production data.

## 1. Identify the session

```bash
curl -sS -H "Authorization: Bearer $MARIA304_ADMIN_TOKEN" \
     http://127.0.0.1:9202/admin/sessions
```

Copy the `session_uuid` for the affected register.

## 2. Grab the frame trace

```bash
curl -sS -H "Authorization: Bearer $MARIA304_ADMIN_TOKEN" \
     http://127.0.0.1:9202/admin/sessions/$SESSION/trace \
     > trace-$SESSION.json
```

(Endpoint lands with the follow-up to M10; for now the trace buffer
lives on the driver process and can be dumped via log entries on
shutdown — `journalctl -u maria304_driver | grep session_uuid=$SESSION`.)

## 3. Grab the metrics snapshot

```bash
curl -sS -H "Authorization: Bearer $MARIA304_ADMIN_TOKEN" \
     http://127.0.0.1:9202/admin/metrics > metrics-$(date +%Y%m%d-%H%M%S).json
```

## 4. Grab the journal around the incident

```bash
journalctl -u maria304_driver \
  --since "2 minutes ago" \
  --output=json > journal-$(date +%Y%m%d-%H%M%S).jsonl
```

## 5. Redact if needed

* `cashier_id` — often a human name; redact for GDPR unless the
  operator consents.
* `pan` / card numbers — the OLE Manager already truncates to
  `411111******1111` style masks, but double-check.

## 6. Ship

Zip the three files + your config (redact `shared_token` and
`auth_token` first) and attach to the support ticket.

```bash
zip maria304-capture-$(date +%Y%m%d-%H%M%S).zip \
    trace-*.json metrics-*.json journal-*.jsonl
```

The driver team needs all three to correlate the wire-level symptom
with the bridge / dispatcher state.
