BEGIN;

INSERT INTO backend_profiles (
    backend_profile_id, backend_type, name, capability_flags_json, config_json, is_active
) VALUES (
    'backend_checkbox_default',
    'CHECKBOX_CLOUD_COMPAT',
    'Checkbox Cloud Default',
    json('{"supports_offline_mode": true, "supports_first_shift_open_offline": false, "requires_same_day_close": true, "max_shift_duration_hours": 24, "supports_x_report": true, "supports_service_receipts": true, "supports_cash_withdrawal": true, "supports_excise_marks": true, "supports_rounding": true, "supports_printable_html": true, "supports_printable_png": false, "supports_printable_text": true, "supports_sequence_control": true, "supports_async_ack": true, "supports_delivery_block": true, "supports_batch_submission": false, "supports_receipt_polling": true}'),
    json('{"vendor":"checkbox","notes":"Reference seed profile for local development"}'),
    1
);

INSERT INTO transport_profiles (
    transport_profile_id, kind, name, endpoint, tls_policy, timeout_config_json, retry_policy_json, config_json, is_active
) VALUES (
    'transport_checkbox_rest_default',
    'CHECKBOX_REST_TRANSPORT',
    'Checkbox REST Default',
    'https://example.checkbox.local/api/v1',
    'system-default',
    json('{"connect_timeout":5,"request_timeout":30,"poll_timeout":10,"crypto_sign_timeout":10,"lease_timeout":120,"recovery_timeout":300,"graceful_shutdown_timeout":30}'),
    json('{"max_retries":3,"retry_backoff_base":1,"strategy":"exponential"}'),
    json('{"headers":{"X-Client-Name":"prro-gateway-dev","X-Client-Version":"0.4.0"}}'),
    1
);


INSERT INTO node_state (
    node_id, fiscal_number, mode, shift_state, next_lnd,
    readiness_state, recovery_stage, current_month_bucket, current_month_offline_seconds
) VALUES (
    'node_dev_fn_0001',
    'FN-DEV-0001',
    'ONLINE',
    'CLOSED',
    1,
    'READY',
    'DONE',
    '',
    0
);

INSERT INTO prro_bindings (
    binding_id, fiscal_number, route_key, backend_profile_id, transport_profile_id, is_primary
) VALUES (
    'binding_example_primary',
    'FN-DEV-0001',
    NULL,
    'backend_checkbox_default',
    'transport_checkbox_rest_default',
    1
);

COMMIT;


INSERT OR IGNORE INTO backend_profiles (
    backend_profile_id, backend_type, name, capability_flags_json, config_json, is_active
) VALUES (
    'backend_dps_direct',
    'DPS_DIRECT',
    'DPS Direct Reference',
    '{"supports_offline_mode": true, "supports_cash_withdrawal": false, "supports_service_receipts": true}',
    '{}',
    1
);

INSERT OR IGNORE INTO transport_profiles (
    transport_profile_id, kind, name, endpoint, tls_policy, timeout_config_json, retry_policy_json, config_json, is_active
) VALUES (
    'transport_dps_grpc_default',
    'DPS_PRRO_GRPC_ECABINET',
    'DPS gRPC eCabinet Reference',
    'https://prro.tax.gov.ua:443',
    'DEFAULT',
    '{"connect_timeout":5,"request_timeout":30,"poll_timeout":10}',
    '{"max_retries":3,"retry_backoff_base":1}',
    '{}',
    1
);

INSERT OR IGNORE INTO transport_profiles (
    transport_profile_id, kind, name, endpoint, tls_policy, timeout_config_json, retry_policy_json, config_json, is_active
) VALUES (
    'transport_dps_xml_default',
    'DPS_PRRO_XML_UNIFIED_WINDOW',
    'DPS XML Unified Window Reference',
    'https://cabinet.tax.gov.ua/report',
    'DEFAULT',
    '{"connect_timeout":10,"request_timeout":60,"poll_timeout":30}',
    '{"max_retries":3,"retry_backoff_base":2}',
    '{}',
    1
);
