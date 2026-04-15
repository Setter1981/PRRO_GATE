-- E2E DPS test seed: FN 4000162280 on cabinet.tax.gov.ua:9443
-- Run after auto_migrate to provision the test fiscal number

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
    'transport_dps_grpc_test',
    'DPS_PRRO_GRPC_ECABINET',
    'DPS gRPC eCabinet Test',
    'https://cabinet.tax.gov.ua:9443',
    'DEFAULT',
    '{"connect_timeout":10,"request_timeout":30,"poll_timeout":10}',
    '{"max_retries":3,"retry_backoff_base":1}',
    '{}',
    1
);

INSERT OR IGNORE INTO node_state (
    node_id, fiscal_number, mode, shift_state, next_lnd,
    readiness_state, recovery_stage, current_month_bucket, current_month_offline_seconds,
    last_known_mac
) VALUES (
    'node_e2e_test_4000162280',
    '4000162280',
    'ONLINE',
    'CLOSED',
    1,
    'READY',
    'DONE',
    '',
    0,
    'c1257bfbd84b737ee0fb49fe059fcfcc6d293b92e06f63fc9c900773ccb833c9'
);

INSERT OR IGNORE INTO prro_bindings (
    binding_id, fiscal_number, route_key, backend_profile_id, transport_profile_id, is_primary
) VALUES (
    'binding_e2e_dps_test',
    '4000162280',
    NULL,
    'backend_dps_direct',
    'transport_dps_grpc_test',
    1
);
