# Mapeo de pruebas: de la Hécate en Python a la de Rust

Una fila por cada una de las 346 funciones de prueba de `hecate/tests/` en `main` (`0287c38`); una función
parametrizada cuenta una vez. La columna de Rust se rellena al trasladar cada prueba; el formato está en
[data-model.md](data-model.md).

| Prueba en Python | Prueba en Rust | Nivel | Notas |
|---|---|---|---|
| `contract/test_init_ops.py::test_init_abort_discards_the_pending_init` | `hecate-infrastructure::tests/contract_init_ops.rs::init_abort_discards_the_pending_init` | Contrato |  |
| `contract/test_init_ops.py::test_init_begin_confirm_flow_in_common_envelope` | `hecate-infrastructure::tests/contract_init_ops.rs::init_begin_confirm_flow_in_common_envelope` | Contrato |  |
| `contract/test_init_ops.py::test_missing_or_mistyped_args_are_validation` | `hecate-infrastructure::tests/contract_init_ops.rs::missing_or_mistyped_args_are_validation` | Contrato |  |
| `contract/test_init_ops.py::test_second_initialization_is_conflict_with_current_state` | `hecate-infrastructure::tests/contract_init_ops.rs::second_initialization_is_conflict_with_current_state` | Contrato |  |
| `contract/test_init_ops.py::test_short_passphrase_is_validation_not_retryable` | `hecate-infrastructure::tests/contract_init_ops.rs::short_passphrase_is_validation_not_retryable` | Contrato |  |
| `contract/test_recovery_ops.py::test_passphrase_change_completes_with_either_proof` | `hecate-infrastructure::tests/contract_recovery_ops.rs::passphrase_change_completes_with_either_proof` | Contrato |  |
| `contract/test_recovery_ops.py::test_passphrase_change_error_codes` | `hecate-infrastructure::tests/contract_recovery_ops.rs::passphrase_change_error_codes` | Contrato |  |
| `contract/test_recovery_ops.py::test_passphrase_change_while_sealed_is_conflict` | `hecate-infrastructure::tests/contract_recovery_ops.rs::passphrase_change_while_sealed_is_conflict` | Contrato |  |
| `contract/test_recovery_ops.py::test_unseal_with_bad_shares_codes` | `hecate-infrastructure::tests/contract_recovery_ops.rs::unseal_with_bad_shares_codes` | Contrato |  |
| `contract/test_recovery_ops.py::test_unseal_with_two_shares` | `hecate-infrastructure::tests/contract_recovery_ops.rs::unseal_with_two_shares` | Contrato |  |
| `contract/test_rekey_ops.py::test_abort_completes_and_discards` | `hecate-infrastructure::tests/contract_rekey_ops.rs::abort_completes_and_discards` | Contrato |  |
| `contract/test_rekey_ops.py::test_begin_confirm_flow` | `hecate-infrastructure::tests/contract_rekey_ops.rs::begin_confirm_flow` | Contrato |  |
| `contract/test_rekey_ops.py::test_error_codes` | `hecate-infrastructure::tests/contract_rekey_ops.rs::error_codes` | Contrato |  |
| `contract/test_rekey_ops.py::test_rekey_ops_while_sealed_are_conflicts` | `hecate-infrastructure::tests/contract_rekey_ops.rs::rekey_ops_while_sealed_are_conflicts` | Contrato |  |
| `contract/test_seal_op.py::test_seal_unsealed_completes` | `hecate-infrastructure::tests/contract_seal_op.rs::seal_unsealed_completes` | Contrato |  |
| `contract/test_seal_op.py::test_seal_when_sealed_is_conflict_with_state` | `hecate-infrastructure::tests/contract_seal_op.rs::seal_when_sealed_is_conflict_with_state` | Contrato |  |
| `contract/test_seal_op.py::test_seal_when_uninitialized_is_conflict` | `hecate-infrastructure::tests/contract_seal_op.rs::seal_when_uninitialized_is_conflict` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_during_a_wait_is_rate_limited` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_during_a_wait_is_rate_limited` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_not_found` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_not_found` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_of_a_destroyed_version_is_a_conflict` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_of_a_destroyed_version_is_a_conflict` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_of_a_version_that_is_not_revoked_is_a_conflict` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_of_a_version_that_is_not_revoked_is_a_conflict` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_stale_version_is_a_conflict_with_the_current_one` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_stale_version_is_a_conflict_with_the_current_one` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_validation_does_not_spend_an_attempt` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_validation_does_not_spend_an_attempt` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_with_a_wrong_proof_is_auth` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_with_a_wrong_proof_is_auth` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_with_the_passphrase` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_with_the_passphrase` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_destroy_with_two_shares` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::destroy_with_two_shares` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_lifecycle_not_found` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::lifecycle_not_found` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_lifecycle_stale_version_is_a_conflict_with_the_current_one` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::lifecycle_stale_version_is_a_conflict_with_the_current_one` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_lifecycle_transition_not_allowed_is_a_conflict` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::lifecycle_transition_not_allowed_is_a_conflict` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_lifecycle_validation` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::lifecycle_validation` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_lifecycle_while_sealed_conflicts_with_the_state` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::lifecycle_while_sealed_conflicts_with_the_state` | Contrato |  |
| `contract/test_secret_lifecycle_ops.py::test_revoke_restore_and_promote_reply_with_the_stream_version` | `hecate-infrastructure::tests/contract_secret_lifecycle_ops.rs::revoke_restore_and_promote_reply_with_the_stream_version` | Contrato |  |
| `contract/test_secret_ops.py::test_add_version_conflict_carries_the_current_version` | `hecate-infrastructure::tests/contract_secret_ops.rs::add_version_conflict_carries_the_current_version` | Contrato |  |
| `contract/test_secret_ops.py::test_add_version_of_a_missing_path_is_not_found` | `hecate-infrastructure::tests/contract_secret_ops.rs::add_version_of_a_missing_path_is_not_found` | Contrato |  |
| `contract/test_secret_ops.py::test_add_version_replies_with_the_new_version_and_its_fingerprint` | `hecate-infrastructure::tests/contract_secret_ops.rs::add_version_replies_with_the_new_version_and_its_fingerprint` | Contrato |  |
| `contract/test_secret_ops.py::test_add_version_validation` | `hecate-infrastructure::tests/contract_secret_ops.rs::add_version_validation` | Contrato |  |
| `contract/test_secret_ops.py::test_add_version_while_sealed_conflicts_with_the_state` | `hecate-infrastructure::tests/contract_secret_ops.rs::add_version_while_sealed_conflicts_with_the_state` | Contrato |  |
| `contract/test_secret_ops.py::test_add_version_with_a_value_invalid_for_the_type_is_validation` | `hecate-infrastructure::tests/contract_secret_ops.rs::add_version_with_a_value_invalid_for_the_type_is_validation` | Contrato |  |
| `contract/test_secret_ops.py::test_conflict_when_the_path_already_exists` | `hecate-infrastructure::tests/contract_secret_ops.rs::conflict_when_the_path_already_exists` | Contrato |  |
| `contract/test_secret_ops.py::test_conflict_with_the_state_while_sealed` | `hecate-infrastructure::tests/contract_secret_ops.rs::conflict_with_the_state_while_sealed` | Contrato |  |
| `contract/test_secret_ops.py::test_create_accepts_optional_metadata` | `hecate-infrastructure::tests/contract_secret_ops.rs::create_accepts_optional_metadata` | Contrato |  |
| `contract/test_secret_ops.py::test_create_replies_with_the_common_envelope` | `hecate-infrastructure::tests/contract_secret_ops.rs::create_replies_with_the_common_envelope` | Contrato |  |
| `contract/test_secret_ops.py::test_create_with_a_maximum_of_one_version_is_validation` | `hecate-infrastructure::tests/contract_secret_ops.rs::create_with_a_maximum_of_one_version_is_validation` | Contrato |  |
| `contract/test_secret_ops.py::test_get_of_a_missing_path_is_not_found` | `hecate-infrastructure::tests/contract_secret_ops.rs::get_of_a_missing_path_is_not_found` | Contrato |  |
| `contract/test_secret_ops.py::test_get_returns_metadata_stream_version_and_versions_without_the_value` | `hecate-infrastructure::tests/contract_secret_ops.rs::get_returns_metadata_stream_version_and_versions_without_the_value` | Contrato |  |
| `contract/test_secret_ops.py::test_list_returns_the_documented_shape_and_filters` | `hecate-infrastructure::tests/contract_secret_ops.rs::list_returns_the_documented_shape_and_filters` | Contrato |  |
| `contract/test_secret_ops.py::test_malformed_arguments_are_logged_without_their_content` | `hecate-infrastructure::tests/contract_secret_ops.rs::malformed_arguments_are_logged_without_their_content` | Contrato |  |
| `contract/test_secret_ops.py::test_secret_operations_do_not_exist_without_a_secrets_service` | `hecate-infrastructure::tests/contract_secret_ops.rs::secret_operations_do_not_exist_without_a_secrets_service` | Contrato |  |
| `contract/test_secret_ops.py::test_status_still_works_while_sealed` | `hecate-infrastructure::tests/contract_secret_ops.rs::status_still_works_while_sealed` | Contrato |  |
| `contract/test_secret_ops.py::test_validation_errors` | `hecate-infrastructure::tests/contract_secret_ops.rs::validation_errors` | Contrato |  |
| `contract/test_status_op.py::test_a_handler_without_secrets_keeps_the_original_status_fields` | `hecate-infrastructure::tests/contract_status_op.rs::a_handler_without_secrets_keeps_the_original_status_fields` | Contrato |  |
| `contract/test_status_op.py::test_a_secret_with_every_version_destroyed_is_no_longer_counted` | `hecate-infrastructure::tests/contract_status_op.rs::a_secret_with_every_version_destroyed_is_no_longer_counted` | Contrato |  |
| `contract/test_status_op.py::test_an_expired_secret_still_resolves` | `hecate-infrastructure::tests/contract_status_op.rs::an_expired_secret_still_resolves` | Contrato |  |
| `contract/test_status_op.py::test_expiry_counts_can_be_asked_for_directly_and_leave_no_row_in_the_access_log` | `hecate-infrastructure::tests/contract_status_op.rs::expiry_counts_can_be_asked_for_directly_and_leave_no_row_in_the_access_log` | Contrato |  |
| `contract/test_status_op.py::test_internal_error_hides_the_cause_and_is_retryable` | `hecate-infrastructure::tests/contract_status_op.rs::internal_error_hides_the_cause_and_is_retryable` | Contrato |  |
| `contract/test_status_op.py::test_status_counts_expired_and_expiring_soon_secrets_without_paths` | `hecate-infrastructure::tests/contract_status_op.rs::status_counts_expired_and_expiring_soon_secrets_without_paths` | Contrato |  |
| `contract/test_status_op.py::test_status_reports_failures_and_the_pending_wait` | `hecate-infrastructure::tests/contract_status_op.rs::status_reports_failures_and_the_pending_wait` | Contrato |  |
| `contract/test_status_op.py::test_status_when_sealed_unsealed_and_after_recovery` | `hecate-infrastructure::tests/contract_status_op.rs::status_when_sealed_unsealed_and_after_recovery` | Contrato |  |
| `contract/test_status_op.py::test_status_when_uninitialized` | `hecate-infrastructure::tests/contract_status_op.rs::status_when_uninitialized` | Contrato |  |
| `contract/test_status_op.py::test_the_counts_are_available_while_sealed` | `hecate-infrastructure::tests/contract_status_op.rs::the_counts_are_available_while_sealed` | Contrato |  |
| `contract/test_status_op.py::test_the_counts_move_with_the_clock` | `hecate-infrastructure::tests/contract_status_op.rs::the_counts_move_with_the_clock` | Contrato |  |
| `contract/test_status_op.py::test_the_limits_are_inclusive_on_the_side_of_the_nearest_date` | `hecate-infrastructure::tests/contract_status_op.rs::the_limits_are_inclusive_on_the_side_of_the_nearest_date` | Contrato |  |
| `contract/test_unseal_ops.py::test_malformed_arguments_leave_an_invalid_input_row` | `hecate-infrastructure::tests/contract_unseal_ops.rs::malformed_arguments_leave_an_invalid_input_row` | Contrato |  |
| `contract/test_unseal_ops.py::test_rate_limited_carries_retry_after_s` | `hecate-infrastructure::tests/contract_unseal_ops.rs::rate_limited_carries_retry_after_s` | Contrato |  |
| `contract/test_unseal_ops.py::test_sealed_rejects_every_other_operation` | `hecate-infrastructure::tests/contract_unseal_ops.rs::sealed_rejects_every_other_operation` | Contrato |  |
| `contract/test_unseal_ops.py::test_unknown_operations_and_status_leave_no_row` | `hecate-infrastructure::tests/contract_unseal_ops.rs::unknown_operations_and_status_leave_no_row` | Contrato |  |
| `contract/test_unseal_ops.py::test_unseal_needs_exactly_one_proof` | `hecate-infrastructure::tests/contract_unseal_ops.rs::unseal_needs_exactly_one_proof` | Contrato |  |
| `contract/test_unseal_ops.py::test_unseal_when_unsealed_is_conflict_with_state` | `hecate-infrastructure::tests/contract_unseal_ops.rs::unseal_when_unsealed_is_conflict_with_state` | Contrato |  |
| `contract/test_unseal_ops.py::test_unseal_with_passphrase_completes` | `hecate-infrastructure::tests/contract_unseal_ops.rs::unseal_with_passphrase_completes` | Contrato |  |
| `contract/test_unseal_ops.py::test_wrong_passphrase_is_auth_not_retryable` | `hecate-infrastructure::tests/contract_unseal_ops.rs::wrong_passphrase_is_auth_not_retryable` | Contrato |  |
| `integration/test_access_log_migration.py::test_a_feature_001_database_gains_the_new_columns_without_losing_rows` | `hecate-infrastructure::tests/access_log_migration.rs::a_feature_001_database_gains_the_new_columns_without_losing_rows` | Integración |  |
| `integration/test_access_log_migration.py::test_a_fresh_database_has_the_columns_from_the_start` | `hecate-infrastructure::tests/access_log_migration.rs::a_fresh_database_has_the_columns_from_the_start` | Integración |  |
| `integration/test_access_log_migration.py::test_new_rows_keep_subject_path_and_version_and_old_ones_stay_empty` | `hecate-infrastructure::tests/access_log_migration.rs::new_rows_keep_subject_path_and_version_and_old_ones_stay_empty` | Integración |  |
| `integration/test_access_log_migration.py::test_opening_twice_and_from_other_connections_is_harmless` | `hecate-infrastructure::tests/access_log_migration.rs::opening_twice_and_from_other_connections_is_harmless` | Integración |  |
| `integration/test_access_log_migration.py::test_the_log_stays_append_only_after_the_migration` | `hecate-infrastructure::tests/access_log_migration.rs::the_log_stays_append_only_after_the_migration` | Integración |  |
| `integration/test_cli.py::test_cli_reports_when_hecate_is_not_running` | `hecate-cli::tests/cli.rs::cli_reports_when_hecate_is_not_running` | Integración |  |
| `integration/test_cli.py::test_init_flow_confirms_with_two_shares` | `hecate-cli::tests/cli.rs::init_flow_confirms_with_two_shares` | Integración |  |
| `integration/test_cli.py::test_init_requires_consent_and_matching_passphrases` | `hecate-cli::tests/cli.rs::init_requires_consent_and_matching_passphrases` | Integración |  |
| `integration/test_cli.py::test_interrupting_the_confirmation_aborts_the_initialization` | `hecate-cli::tests/cli.rs::interrupting_the_confirmation_aborts_the_initialization` | Integración |  |
| `integration/test_cli.py::test_interrupting_the_rekey_confirmation_keeps_the_old_shares` | `hecate-cli::tests/cli.rs::interrupting_the_rekey_confirmation_keeps_the_old_shares` | Integración |  |
| `integration/test_cli.py::test_passphrase_change_with_shares_then_new_passphrase_unseals` | `hecate-cli::tests/cli.rs::passphrase_change_with_shares_then_new_passphrase_unseals` | Integración |  |
| `integration/test_cli.py::test_passphrase_change_with_the_current_passphrase_and_mismatched_new_one` | `hecate-cli::tests/cli.rs::passphrase_change_with_the_current_passphrase_and_mismatched_new_one` | Integración |  |
| `integration/test_cli.py::test_shares_rekey_flow_replaces_the_shares` | `hecate-cli::tests/cli.rs::shares_rekey_flow_replaces_the_shares` | Integración |  |
| `integration/test_cli.py::test_unseal_with_shares_recommends_changing_the_passphrase` | `hecate-cli::tests/cli.rs::unseal_with_shares_recommends_changing_the_passphrase` | Integración |  |
| `integration/test_cli.py::test_unseal_with_the_right_passphrase` | `hecate-cli::tests/cli.rs::unseal_with_the_right_passphrase` | Integración |  |
| `integration/test_cli.py::test_wrong_passphrases_then_the_wait_is_reported` | `hecate-cli::tests/cli.rs::wrong_passphrases_then_the_wait_is_reported` | Integración |  |
| `integration/test_concurrent_unseal.py::test_two_simultaneous_unseals_one_wins_and_the_other_gets_conflict` | `hecate-infrastructure::tests/concurrent_unseal.rs::two_simultaneous_unseals_one_wins_and_the_other_gets_conflict` | Integración |  |
| `integration/test_control_channel.py::test_malformed_request_gets_validation_error_in_common_envelope` | `hecate-infrastructure::tests/control_channel.rs::malformed_request_gets_validation_error_in_common_envelope` | Integración |  |
| `integration/test_control_channel.py::test_socket_is_private_and_status_works_when_uninitialized` | `hecate-infrastructure::tests/control_channel.rs::socket_is_private_and_status_works_when_uninitialized` | Integración |  |
| `integration/test_control_channel.py::test_status_works_with_real_sqlite_from_worker_threads` | `hecate-infrastructure::tests/control_channel.rs::status_works_with_real_sqlite_from_worker_threads` | Integración |  |
| `integration/test_crash_during_change.py::test_power_cut_inside_passphrase_change_keeps_the_previous_protection` | `hecate-infrastructure::tests/crash_during_change.rs::power_cut_inside_passphrase_change_keeps_the_previous_protection` | Integración |  |
| `integration/test_crash_during_change.py::test_power_cut_inside_rekey_confirm_keeps_the_old_shares` | `hecate-infrastructure::tests/crash_during_change.rs::power_cut_inside_rekey_confirm_keeps_the_old_shares` | Integración |  |
| `integration/test_crypto_software.py::test_aead_any_altered_byte_fails` | `hecate-infrastructure::tests/crypto_software.rs::aead_any_altered_byte_fails` | Integración |  |
| `integration/test_crypto_software.py::test_aead_roundtrip_with_associated_data` | `hecate-infrastructure::tests/crypto_software.rs::aead_roundtrip_with_associated_data` | Integración |  |
| `integration/test_crypto_software.py::test_aead_wrong_key_fails` | `hecate-infrastructure::tests/crypto_software.rs::aead_wrong_key_fails` | Integración |  |
| `integration/test_crypto_software.py::test_derive_key_is_deterministic_and_depends_on_inputs` | `hecate-infrastructure::tests/crypto_software.rs::derive_key_is_deterministic_and_depends_on_inputs` | Integración |  |
| `integration/test_crypto_software.py::test_random_bytes_length` | `hecate-infrastructure::tests/crypto_software.rs::random_bytes_length` | Integración |  |
| `integration/test_crypto_software.py::test_shamir_any_pair_recovers_and_one_never` | `hecate-infrastructure::tests/crypto_software.rs::shamir_any_pair_recovers_and_one_never` | Integración |  |
| `integration/test_crypto_software.py::test_shamir_pair_from_different_sets_does_not_recover` | `hecate-infrastructure::tests/crypto_software.rs::shamir_pair_from_different_sets_does_not_recover` | Integración |  |
| `integration/test_envelope.py::test_altering_any_byte_prevents_decryption` | `hecate-infrastructure::tests/envelope.rs::altering_any_byte_prevents_decryption` | Integración |  |
| `integration/test_envelope.py::test_another_scope_key_cannot_open_the_value` | `hecate-infrastructure::tests/envelope.rs::another_scope_key_cannot_open_the_value` | Integración |  |
| `integration/test_envelope.py::test_each_scope_key_is_different` | `hecate-infrastructure::tests/envelope.rs::each_scope_key_is_different` | Integración |  |
| `integration/test_envelope.py::test_every_version_gets_its_own_key` | `hecate-infrastructure::tests/envelope.rs::every_version_gets_its_own_key` | Integración |  |
| `integration/test_envelope.py::test_fingerprint_differs_between_identities_and_has_the_documented_shape` | `hecate-infrastructure::tests/envelope.rs::fingerprint_differs_between_identities_and_has_the_documented_shape` | Integración |  |
| `integration/test_envelope.py::test_fingerprint_does_not_contain_the_value` | `hecate-infrastructure::tests/envelope.rs::fingerprint_does_not_contain_the_value` | Integración |  |
| `integration/test_envelope.py::test_fingerprint_is_deterministic_for_the_same_identity_and_value` | `hecate-infrastructure::tests/envelope.rs::fingerprint_is_deterministic_for_the_same_identity_and_value` | Integración |  |
| `integration/test_envelope.py::test_moving_the_encrypted_data_elsewhere_prevents_decryption` | `hecate-infrastructure::tests/envelope.rs::moving_the_encrypted_data_elsewhere_prevents_decryption` | Integración |  |
| `integration/test_envelope.py::test_scope_key_does_not_open_with_another_root_identity_or_version` | `hecate-infrastructure::tests/envelope.rs::scope_key_does_not_open_with_another_root_identity_or_version` | Integración |  |
| `integration/test_envelope.py::test_scope_key_is_created_and_opened_with_the_root` | `hecate-infrastructure::tests/envelope.rs::scope_key_is_created_and_opened_with_the_root` | Integración |  |
| `integration/test_envelope.py::test_seals_and_opens_a_value_through_the_version_key` | `hecate-infrastructure::tests/envelope.rs::seals_and_opens_a_value_through_the_version_key` | Integración |  |
| `integration/test_init_crash.py::test_confirmed_init_survives_a_restart_as_sealed` | `hecate-infrastructure::tests/init_crash.rs::confirmed_init_survives_a_restart_as_sealed` | Integración |  |
| `integration/test_init_crash.py::test_process_stopped_between_begin_and_confirm_stays_uninitialized` | `hecate-infrastructure::tests/init_crash.rs::process_stopped_between_begin_and_confirm_stays_uninitialized` | Integración |  |
| `integration/test_no_secret_leak.py::test_no_passphrase_share_or_root_reaches_the_database_the_log_or_the_replies` | `hecate-infrastructure::tests/no_secret_leak.rs::no_passphrase_share_or_root_reaches_the_database_the_log_or_the_replies` | Integración |  |
| `integration/test_no_secret_leak.py::test_the_check_itself_detects_a_planted_secret` | `hecate-infrastructure::tests/no_secret_leak.rs::the_check_itself_detects_a_planted_secret` | Integración |  |
| `integration/test_restart.py::test_after_unsealing_a_restart_leaves_hecate_sealed` | `hecate-infrastructure::tests/restart.rs::after_unsealing_a_restart_leaves_hecate_sealed` | Integración |  |
| `integration/test_restart.py::test_failed_attempts_and_pending_wait_survive_a_restart` | `hecate-infrastructure::tests/restart.rs::failed_attempts_and_pending_wait_survive_a_restart` | Integración |  |
| `integration/test_restart.py::test_sc007_no_more_than_ten_evaluated_attempts_per_hour_even_restarting_and_alternating` | `hecate-infrastructure::tests/restart.rs::sc007_no_more_than_ten_evaluated_attempts_per_hour_even_restarting_and_alternating` | Integración |  |
| `integration/test_secret_concurrency.py::test_two_destroys_of_the_same_version_one_wins` | `hecate-infrastructure::tests/secret_concurrency.rs::two_destroys_of_the_same_version_one_wins` | Integración |  |
| `integration/test_secret_concurrency.py::test_two_new_versions_with_the_same_expected_version_one_wins_and_the_other_conflicts` | `hecate-infrastructure::tests/secret_concurrency.rs::two_new_versions_with_the_same_expected_version_one_wins_and_the_other_conflicts` | Integración |  |
| `integration/test_secret_crash.py::test_a_connection_that_dies_without_committing_changes_nothing` | `hecate-infrastructure::tests/secret_crash.rs::a_connection_that_dies_without_committing_changes_nothing` | Integración |  |
| `integration/test_secret_crash.py::test_a_crash_while_adding_a_version_keeps_the_previous_state` | `hecate-infrastructure::tests/secret_crash.rs::a_crash_while_adding_a_version_keeps_the_previous_state` | Integración |  |
| `integration/test_secret_crash.py::test_a_crash_while_creating_leaves_no_secret_and_no_key` | `hecate-infrastructure::tests/secret_crash.rs::a_crash_while_creating_leaves_no_secret_and_no_key` | Integración |  |
| `integration/test_secret_crash.py::test_a_crash_while_destroying_keeps_the_version_revoked_with_its_material` | `hecate-infrastructure::tests/secret_crash.rs::a_crash_while_destroying_keeps_the_version_revoked_with_its_material` | Integración |  |
| `integration/test_secret_crypto_shred.py::test_a_destroyed_version_cannot_be_recovered_with_the_root_nor_from_a_later_copy` | `hecate-infrastructure::tests/secret_crypto_shred.rs::a_destroyed_version_cannot_be_recovered_with_the_root_nor_from_a_later_copy` | Integración |  |
| `integration/test_secret_crypto_shred.py::test_the_other_versions_keep_their_material` | `hecate-infrastructure::tests/secret_crypto_shred.rs::the_other_versions_keep_their_material` | Integración |  |
| `integration/test_secret_crypto_shred.py::test_the_version_destroyed_by_the_maximum_is_erased_as_well` | `hecate-infrastructure::tests/secret_crypto_shred.rs::the_version_destroyed_by_the_maximum_is_erased_as_well` | Integración |  |
| `integration/test_secret_no_leak.py::test_fingerprints_stay_out_of_the_log_and_events_carry_no_value_or_key_field` | `hecate-infrastructure::tests/secret_no_leak.rs::fingerprints_stay_out_of_the_log_and_events_carry_no_value_or_key_field` | Integración |  |
| `integration/test_secret_no_leak.py::test_no_value_reaches_the_database_the_events_the_log_or_the_replies` | `hecate-infrastructure::tests/secret_no_leak.rs::no_value_reaches_the_database_the_events_the_log_or_the_replies` | Integración |  |
| `integration/test_secret_no_leak.py::test_the_check_itself_detects_a_planted_value` | `hecate-infrastructure::tests/secret_no_leak.rs::the_check_itself_detects_a_planted_value` | Integración |  |
| `integration/test_secret_resolve_sqlite.py::test_resolving_ten_references_takes_less_than_a_second` | `hecate-infrastructure::tests/performance.rs::resolving_ten_references_takes_less_than_a_second` | Integración |  |
| `integration/test_secret_resolve_sqlite.py::test_store_restart_unseal_and_resolve` | `hecate-infrastructure::tests/secret_resolve_sqlite.rs::store_restart_unseal_and_resolve` | Integración |  |
| `integration/test_secret_resolve_sqlite.py::test_the_access_log_keeps_the_resolutions_across_restarts` | `hecate-infrastructure::tests/secret_resolve_sqlite.rs::the_access_log_keeps_the_resolutions_across_restarts` | Integración |  |
| `integration/test_secret_store.py::test_a_committed_transaction_keeps_everything` | `hecate-infrastructure::tests/secret_store.rs::a_committed_transaction_keeps_everything` | Integración |  |
| `integration/test_secret_store.py::test_a_conflict_inside_a_transaction_rolls_back_the_rest` | `hecate-infrastructure::tests/secret_store.rs::a_conflict_inside_a_transaction_rolls_back_the_rest` | Integración |  |
| `integration/test_secret_store.py::test_a_failure_inside_the_transaction_leaves_neither_events_nor_material` | `hecate-infrastructure::tests/secret_store.rs::a_failure_inside_the_transaction_leaves_neither_events_nor_material` | Integración |  |
| `integration/test_secret_store.py::test_a_stale_expected_version_conflicts_and_writes_nothing` | `hecate-infrastructure::tests/secret_store.rs::a_stale_expected_version_conflicts_and_writes_nothing` | Integración |  |
| `integration/test_secret_store.py::test_deleting_the_material_of_a_version_keeps_its_events` | `hecate-infrastructure::tests/secret_store.rs::deleting_the_material_of_a_version_keeps_its_events` | Integración |  |
| `integration/test_secret_store.py::test_events_and_material_survive_closing_and_reopening` | `hecate-infrastructure::tests/secret_store.rs::events_and_material_survive_closing_and_reopening` | Integración |  |
| `integration/test_secret_store.py::test_events_are_appended_with_the_expected_version` | `hecate-infrastructure::tests/secret_store.rs::events_are_appended_with_the_expected_version` | Integración |  |
| `integration/test_secret_store.py::test_missing_material_is_none` | `hecate-infrastructure::tests/secret_store.rs::missing_material_is_none` | Integración |  |
| `integration/test_secret_store.py::test_streams_are_independent_and_listed_by_prefix` | `hecate-infrastructure::tests/secret_store.rs::streams_are_independent_and_listed_by_prefix` | Integración |  |
| `integration/test_secret_store.py::test_the_events_table_is_append_only` | `hecate-infrastructure::tests/secret_store.rs::the_events_table_is_append_only` | Integración |  |
| `integration/test_sqlite_store.py::test_access_log_appends_and_rejects_update_and_delete` | `hecate-infrastructure::tests/sqlite_store.rs::access_log_appends_and_rejects_update_and_delete` | Integración |  |
| `integration/test_sqlite_store.py::test_config_and_attempts_survive_reopen` | `hecate-infrastructure::tests/sqlite_store.rs::config_and_attempts_survive_reopen` | Integración |  |
| `integration/test_sqlite_store.py::test_data_dir_files_are_private` | `hecate-infrastructure::tests/sqlite_store.rs::data_dir_files_are_private` | Integración |  |
| `integration/test_sqlite_store.py::test_failure_inside_transaction_leaves_no_changes` | `hecate-infrastructure::tests/sqlite_store.rs::failure_inside_transaction_leaves_no_changes` | Integración |  |
| `unit/application/test_initialize.py::test_abort_expiry_and_unknown_id_persist_nothing` | `hecate-application::tests/initialize.rs::abort_expiry_and_unknown_id_persist_nothing` | Unitaria |  |
| `unit/application/test_initialize.py::test_begin_returns_three_distinct_shares_and_persists_nothing` | `hecate-application::tests/initialize.rs::begin_returns_three_distinct_shares_and_persists_nothing` | Unitaria |  |
| `unit/application/test_initialize.py::test_cannot_initialize_twice` | `hecate-application::tests/initialize.rs::cannot_initialize_twice` | Unitaria |  |
| `unit/application/test_initialize.py::test_confirm_needs_two_distinct_shares` | `hecate-application::tests/initialize.rs::confirm_needs_two_distinct_shares` | Unitaria |  |
| `unit/application/test_initialize.py::test_confirm_rejects_shares_that_were_not_shown` | `hecate-application::tests/initialize.rs::confirm_rejects_shares_that_were_not_shown` | Unitaria |  |
| `unit/application/test_initialize.py::test_confirm_with_any_two_shown_shares_leaves_it_sealed` | `hecate-application::tests/initialize.rs::confirm_with_any_two_shown_shares_leaves_it_sealed` | Unitaria |  |
| `unit/application/test_initialize.py::test_short_passphrase_is_rejected_without_leaving_a_pending_init` | `hecate-application::tests/initialize.rs::short_passphrase_is_rejected_without_leaving_a_pending_init` | Unitaria |  |
| `unit/application/test_initialize.py::test_stored_protections_recover_the_same_root` | `hecate-application::tests/initialize.rs::stored_protections_recover_the_same_root` | Unitaria |  |
| `unit/application/test_passphrase_change.py::test_change_needs_to_be_unsealed` | `hecate-application::tests/passphrase_change.rs::change_needs_to_be_unsealed` | Unitaria |  |
| `unit/application/test_passphrase_change.py::test_change_proving_with_two_shares_clears_the_recommendation` | `hecate-application::tests/passphrase_change.rs::change_proving_with_two_shares_clears_the_recommendation` | Unitaria |  |
| `unit/application/test_passphrase_change.py::test_change_with_the_current_passphrase` | `hecate-application::tests/passphrase_change.rs::change_with_the_current_passphrase` | Unitaria |  |
| `unit/application/test_passphrase_change.py::test_invalid_requests_are_validation_errors_and_not_failures` | `hecate-application::tests/passphrase_change.rs::invalid_requests_are_validation_errors_and_not_failures` | Unitaria |  |
| `unit/application/test_passphrase_change.py::test_shares_still_unseal_after_a_passphrase_change` | `hecate-application::tests/passphrase_change.rs::shares_still_unseal_after_a_passphrase_change` | Unitaria |  |
| `unit/application/test_passphrase_change.py::test_without_a_valid_proof_nothing_changes_and_a_failure_is_counted` | `hecate-application::tests/passphrase_change.rs::without_a_valid_proof_nothing_changes_and_a_failure_is_counted` | Unitaria |  |
| `unit/application/test_seal.py::test_can_unseal_again_after_sealing` | `hecate-application::tests/seal.rs::can_unseal_again_after_sealing` | Unitaria |  |
| `unit/application/test_seal.py::test_sealing_unsealed_hecate_wipes_the_root_and_logs_it` | `hecate-application::tests/seal.rs::sealing_unsealed_hecate_wipes_the_root_and_logs_it` | Unitaria |  |
| `unit/application/test_seal.py::test_sealing_when_sealed_is_a_conflict_that_changes_nothing` | `hecate-application::tests/seal.rs::sealing_when_sealed_is_a_conflict_that_changes_nothing` | Unitaria |  |
| `unit/application/test_seal.py::test_sealing_when_uninitialized_is_a_conflict` | `hecate-application::tests/seal.rs::sealing_when_uninitialized_is_a_conflict` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_root_is_available_only_while_unsealed` | `hecate-application::tests/sealing_public_ops.rs::root_is_available_only_while_unsealed` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_root_is_the_same_across_calls_and_unseal_methods` | `hecate-application::tests/sealing_public_ops.rs::root_is_the_same_across_calls_and_unseal_methods` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_the_counter_is_shared_with_unsealing_and_resets_on_success` | `hecate-application::tests/sealing_public_ops.rs::the_counter_is_shared_with_unsealing_and_resets_on_success` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_verify_proof_accepts_the_current_passphrase` | `hecate-application::tests/sealing_public_ops.rs::verify_proof_accepts_the_current_passphrase` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_verify_proof_accepts_two_shares` | `hecate-application::tests/sealing_public_ops.rs::verify_proof_accepts_two_shares` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_verify_proof_needs_the_service_unsealed` | `hecate-application::tests/sealing_public_ops.rs::verify_proof_needs_the_service_unsealed` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_verify_proof_rejects_a_wrong_passphrase_and_counts_the_failure` | `hecate-application::tests/sealing_public_ops.rs::verify_proof_rejects_a_wrong_passphrase_and_counts_the_failure` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_verify_proof_rejects_malformed_proofs_without_counting` | `hecate-application::tests/sealing_public_ops.rs::verify_proof_rejects_malformed_proofs_without_counting` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_verify_proof_rejects_wrong_shares_and_counts_the_failure` | `hecate-application::tests/sealing_public_ops.rs::verify_proof_rejects_wrong_shares_and_counts_the_failure` | Unitaria |  |
| `unit/application/test_sealing_public_ops.py::test_verify_proof_respects_the_wait_even_with_a_correct_proof` | `hecate-application::tests/sealing_public_ops.rs::verify_proof_respects_the_wait_even_with_a_correct_proof` | Unitaria |  |
| `unit/application/test_secret_create.py::test_a_failure_while_saving_leaves_no_trace` | `hecate-application::tests/secrets_create.rs::a_failure_while_saving_leaves_no_trace` | Unitaria |  |
| `unit/application/test_secret_create.py::test_a_malformed_path_is_never_written_to_the_log` | `hecate-application::tests/secrets_create.rs::a_malformed_path_is_never_written_to_the_log` | Unitaria |  |
| `unit/application/test_secret_create.py::test_a_value_of_exactly_64_kib_is_accepted` | `hecate-application::tests/secrets_create.rs::a_value_of_exactly_64_kib_is_accepted` | Unitaria |  |
| `unit/application/test_secret_create.py::test_create_gives_version_one_active_with_a_fingerprint` | `hecate-application::tests/secrets_create.rs::create_gives_version_one_active_with_a_fingerprint` | Unitaria |  |
| `unit/application/test_secret_create.py::test_creating_over_an_existing_path_conflicts_and_keeps_the_original` | `hecate-application::tests/secrets_create.rs::creating_over_an_existing_path_conflicts_and_keeps_the_original` | Unitaria |  |
| `unit/application/test_secret_create.py::test_creating_while_sealed_conflicts` | `hecate-application::tests/secrets_create.rs::creating_while_sealed_conflicts` | Unitaria |  |
| `unit/application/test_secret_create.py::test_invalid_input_changes_nothing` | `hecate-infrastructure::tests/contract_secret_ops.rs::arguments_that_are_not_text_are_refused_before_the_service`, `hecate-application::tests/secrets_create.rs::invalid_input_changes_nothing` | Unitaria | `value: 123` no llega al servicio en Rust (es texto por tipo): lo rechaza la comprobación de argumentos del canal |
| `unit/application/test_secret_create.py::test_metadata_given_at_creation_is_kept` | `hecate-application::tests/secrets_create.rs::metadata_given_at_creation_is_kept` | Unitaria |  |
| `unit/application/test_secret_create.py::test_refusals_are_logged_too` | `hecate-application::tests/secrets_create.rs::refusals_are_logged_too` | Unitaria |  |
| `unit/application/test_secret_create.py::test_the_access_log_records_who_where_and_which_version_but_no_value` | `hecate-application::tests/secrets_create.rs::the_access_log_records_who_where_and_which_version_but_no_value` | Unitaria |  |
| `unit/application/test_secret_create.py::test_the_first_secret_of_an_identity_creates_its_key_only_once` | `hecate-application::tests/secrets_create.rs::the_first_secret_of_an_identity_creates_its_key_only_once` | Unitaria |  |
| `unit/application/test_secret_create.py::test_the_same_value_has_different_fingerprints_in_different_identities` | `hecate-application::tests/secrets_create.rs::the_same_value_has_different_fingerprints_in_different_identities` | Unitaria |  |
| `unit/application/test_secret_create.py::test_the_value_appears_nowhere_in_the_store` | `hecate-application::tests/secrets_create.rs::the_value_appears_nowhere_in_the_store` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_a_deprecated_version_is_not_destroyable_until_its_grace_ends` | `hecate-application::tests/secrets_destroy.rs::a_deprecated_version_is_not_destroyable_until_its_grace_ends` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_a_destroyed_version_is_not_resolved_nor_restored` | `hecate-application::tests/secrets_destroy.rs::a_destroyed_version_is_not_resolved_nor_restored` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_a_failure_while_destroying_leaves_the_version_revoked_with_its_material` | `hecate-application::tests/secrets_destroy.rs::a_failure_while_destroying_leaves_the_version_revoked_with_its_material` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_a_malformed_proof_is_invalid_without_counting` | `hecate-application::tests/secrets_destroy.rs::a_malformed_proof_is_invalid_without_counting` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_a_stale_expected_version_is_refused_before_the_proof_is_checked` | `hecate-application::tests/secrets_destroy.rs::a_stale_expected_version_is_refused_before_the_proof_is_checked` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_a_wrong_proof_is_refused_and_counts_in_the_shared_counter` | `hecate-application::tests/secrets_destroy.rs::a_wrong_proof_is_refused_and_counts_in_the_shared_counter` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_destroying_with_the_passphrase_removes_the_material_and_keeps_the_events` | `hecate-application::tests/secrets_destroy.rs::destroying_with_the_passphrase_removes_the_material_and_keeps_the_events` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_destroying_with_two_recovery_shares` | `hecate-application::tests/secrets_destroy.rs::destroying_with_two_recovery_shares` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_during_a_wait_even_the_right_proof_is_rate_limited` | `hecate-application::tests/secrets_destroy.rs::during_a_wait_even_the_right_proof_is_rate_limited` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_only_a_revoked_version_can_be_destroyed` | `hecate-application::tests/secrets_destroy.rs::only_a_revoked_version_can_be_destroyed` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_sealed_hecate_refuses` | `hecate-application::tests/secrets_destroy.rs::sealed_hecate_refuses` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_the_counter_is_shared_with_unsealing` | `hecate-application::tests/secrets_destroy.rs::the_counter_is_shared_with_unsealing` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_the_log_records_the_method_of_the_proof_and_the_result` | `hecate-application::tests/secrets_destroy.rs::the_log_records_the_method_of_the_proof_and_the_result` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_unknown_version_or_path_is_not_found` | `hecate-application::tests/secrets_destroy.rs::unknown_version_or_path_is_not_found` | Unitaria |  |
| `unit/application/test_secret_destroy.py::test_wrong_shares_are_refused_too` | `hecate-application::tests/secrets_destroy.rs::wrong_shares_are_refused_too` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_a_malformed_version_is_invalid` | `hecate-application::tests/secrets_lifecycle.rs::a_malformed_version_is_invalid` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_a_new_version_after_revoking_the_active_one_becomes_active` | `hecate-application::tests/secrets_lifecycle.rs::a_new_version_after_revoking_the_active_one_becomes_active` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_a_revoked_version_cannot_be_promoted` | `hecate-application::tests/secrets_lifecycle.rs::a_revoked_version_cannot_be_promoted` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_a_stale_or_missing_expected_version_is_refused` | `hecate-application::tests/secrets_lifecycle.rs::a_stale_or_missing_expected_version_is_refused` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_an_expired_deprecated_version_can_be_restored` | `hecate-application::tests/secrets_lifecycle.rs::an_expired_deprecated_version_can_be_restored` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_an_unknown_version_or_path_is_not_found` | `hecate-application::tests/secrets_lifecycle.rs::an_unknown_version_or_path_is_not_found` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_each_operation_is_logged_with_its_version` | `hecate-application::tests/secrets_lifecycle.rs::each_operation_is_logged_with_its_version` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_none_of_these_operations_asks_for_a_proof_or_touches_the_attempt_counter` | `hecate-application::tests/secrets_lifecycle.rs::none_of_these_operations_asks_for_a_proof_or_touches_the_attempt_counter` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_promoting_a_deprecated_version_makes_it_active_and_demotes_the_active_one` | `hecate-application::tests/secrets_lifecycle.rs::promoting_a_deprecated_version_makes_it_active_and_demotes_the_active_one` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_promoting_when_nothing_is_active_only_promotes` | `hecate-application::tests/secrets_lifecycle.rs::promoting_when_nothing_is_active_only_promotes` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_restoring_a_revoked_version_leaves_it_deprecated_with_a_new_grace` | `hecate-application::tests/secrets_lifecycle.rs::restoring_a_revoked_version_leaves_it_deprecated_with_a_new_grace` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_revoking_a_deprecated_version_stops_it_before_its_grace_ends` | `hecate-application::tests/secrets_lifecycle.rs::revoking_a_deprecated_version_stops_it_before_its_grace_ends` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_revoking_the_active_version_stops_it_at_once_and_leaves_none_active` | `hecate-application::tests/secrets_lifecycle.rs::revoking_the_active_version_stops_it_at_once_and_leaves_none_active` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_sealed_hecate_refuses_all_three` | `hecate-application::tests/secrets_lifecycle.rs::sealed_hecate_refuses_all_three` | Unitaria |  |
| `unit/application/test_secret_lifecycle.py::test_transitions_that_are_not_allowed_conflict_and_change_nothing` | `hecate-application::tests/secrets_lifecycle.rs::transitions_that_are_not_allowed_conflict_and_change_nothing` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_a_malformed_path_is_invalid` | `hecate-application::tests/secrets_list_get.rs::a_malformed_path_is_invalid` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_every_consultation_is_logged_without_the_value` | `hecate-application::tests/secrets_list_get.rs::every_consultation_is_logged_without_the_value` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_get_returns_metadata_stream_version_and_versions` | `hecate-application::tests/secrets_list_get.rs::get_returns_metadata_stream_version_and_versions` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_list_does_not_treat_a_prefix_of_an_identity_as_the_identity` | `hecate-application::tests/secrets_list_get.rs::list_does_not_treat_a_prefix_of_an_identity_as_the_identity` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_list_filters_by_identity` | `hecate-application::tests/secrets_list_get.rs::list_filters_by_identity` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_list_rejects_a_malformed_identity` | `hecate-application::tests/secrets_list_get.rs::list_rejects_a_malformed_identity` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_list_returns_paths_and_metadata_sorted` | `hecate-application::tests/secrets_list_get.rs::list_returns_paths_and_metadata_sorted` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_no_response_contains_the_value` | `hecate-application::tests/secrets_list_get.rs::no_response_contains_the_value` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_sealed_hecate_refuses_both` | `hecate-application::tests/secrets_list_get.rs::sealed_hecate_refuses_both` | Unitaria |  |
| `unit/application/test_secret_list_get.py::test_unknown_path_is_not_found` | `hecate-application::tests/secrets_list_get.rs::unknown_path_is_not_found` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_a_malformed_identity_is_invalid` | `hecate-application::tests/secrets_resolve.rs::a_malformed_identity_is_invalid` | Unitaria | `5` y `None` no llegan en Rust: la identidad es texto por tipo |
| `unit/application/test_secret_resolve.py::test_a_rejection_is_logged_with_the_requested_path_and_logs_no_success` | `hecate-application::tests/secrets_resolve.rs::a_rejection_is_logged_with_the_requested_path_and_logs_no_success` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_a_sealed_refusal_is_logged_too` | `hecate-application::tests/secrets_resolve.rs::a_sealed_refusal_is_logged_too` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_a_version_that_does_not_exist_is_not_found` | `hecate-application::tests/secrets_resolve.rs::a_version_that_does_not_exist_is_not_found` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_all_or_nothing` | `hecate-application::tests/secrets_resolve.rs::all_or_nothing` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_an_identity_never_resolves_outside_its_own_space` | `hecate-application::tests/secrets_resolve.rs::an_identity_never_resolves_outside_its_own_space` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_another_identitys_secret_and_a_missing_one_are_indistinguishable` | `hecate-application::tests/secrets_resolve.rs::another_identitys_secret_and_a_missing_one_are_indistinguishable` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_every_resolution_is_logged_with_the_identity_as_subject_and_no_value` | `hecate-application::tests/secrets_resolve.rs::every_resolution_is_logged_with_the_identity_as_subject_and_no_value` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_malformed_requests_are_invalid` | `hecate-application::tests/secrets_resolve.rs::malformed_requests_are_invalid` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_rejections_are_logged_the_same_for_foreign_and_missing_secrets` | `hecate-application::tests/secrets_resolve.rs::rejections_are_logged_the_same_for_foreign_and_missing_secrets` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_resolves_a_concrete_version` | `hecate-application::tests/secrets_resolve.rs::resolves_a_concrete_version` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_resolves_several_references_in_one_request_in_order` | `hecate-application::tests/secrets_resolve.rs::resolves_several_references_in_one_request_in_order` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_resolves_the_active_version_with_the_cache_times` | `hecate-application::tests/secrets_resolve.rs::resolves_the_active_version_with_the_cache_times` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_resolving_while_sealed_conflicts` | `hecate-application::tests/secrets_resolve.rs::resolving_while_sealed_conflicts` | Unitaria |  |
| `unit/application/test_secret_resolve.py::test_what_is_resolved_is_exactly_what_was_written` | `hecate-application::tests/secrets_resolve.rs::what_is_resolved_is_exactly_what_was_written` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_a_failure_while_destroying_the_oldest_leaves_everything_as_it_was` | `hecate-application::tests/secrets_rotate.rs::a_failure_while_destroying_the_oldest_leaves_everything_as_it_was` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_a_failure_while_saving_leaves_the_secret_as_it_was` | `hecate-application::tests/secrets_rotate.rs::a_failure_while_saving_leaves_the_secret_as_it_was` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_a_malformed_path_is_invalid` | `hecate-application::tests/secrets_rotate.rs::a_malformed_path_is_invalid` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_a_missing_or_malformed_expected_version_is_invalid` | `hecate-application::tests/secrets_rotate.rs::a_missing_or_malformed_expected_version_is_invalid` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_a_stale_expected_version_conflicts_with_the_current_one_and_changes_nothing` | `hecate-application::tests/secrets_rotate.rs::a_stale_expected_version_conflicts_with_the_current_one_and_changes_nothing` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_add_version_makes_the_new_one_active_and_the_old_one_deprecated` | `hecate-application::tests/secrets_rotate.rs::add_version_makes_the_new_one_active_and_the_old_one_deprecated` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_an_unknown_path_is_not_found` | `hecate-application::tests/secrets_rotate.rs::an_unknown_path_is_not_found` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_exceeding_the_maximum_destroys_the_oldest_in_the_same_change` | `hecate-application::tests/secrets_rotate.rs::exceeding_the_maximum_destroys_the_oldest_in_the_same_change` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_list_reports_the_active_version_and_how_many_are_kept` | `hecate-application::tests/secrets_rotate.rs::list_reports_the_active_version_and_how_many_are_kept` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_no_value_is_stored_in_clear` | `hecate-application::tests/secrets_rotate.rs::no_value_is_stored_in_clear` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_refusals_are_logged` | `hecate-application::tests/secrets_rotate.rs::refusals_are_logged` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_sealed_hecate_refuses` | `hecate-application::tests/secrets_rotate.rs::sealed_hecate_refuses` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_the_access_log_records_the_admin_the_path_and_the_new_version` | `hecate-application::tests/secrets_rotate.rs::the_access_log_records_the_admin_the_path_and_the_new_version` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_the_new_version_has_its_own_fingerprint_and_the_same_scope_key` | `hecate-application::tests/secrets_rotate.rs::the_new_version_has_its_own_fingerprint_and_the_same_scope_key` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_the_previous_version_resolves_explicitly_during_the_grace_and_not_after` | `hecate-application::tests/secrets_rotate.rs::the_previous_version_resolves_explicitly_during_the_grace_and_not_after` | Unitaria |  |
| `unit/application/test_secret_rotate.py::test_the_value_must_be_valid_for_the_type_of_the_secret` | `hecate-application::tests/secrets_rotate.rs::the_value_must_be_valid_for_the_type_of_the_secret` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_abort_and_expiry_discard_the_pending_set` | `hecate-application::tests/shares_rekey.rs::abort_and_expiry_discard_the_pending_set` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_confirming_with_the_old_or_wrong_shares_changes_nothing` | `hecate-application::tests/shares_rekey.rs::confirming_with_the_old_or_wrong_shares_changes_nothing` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_invalid_proof_nothing_pending_and_a_failure_is_counted` | `hecate-application::tests/shares_rekey.rs::invalid_proof_nothing_pending_and_a_failure_is_counted` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_malformed_proofs_are_validation_errors_and_not_counted` | `hecate-application::tests/shares_rekey.rs::malformed_proofs_are_validation_errors_and_not_counted` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_new_shares_are_valid_shares_of_a_new_set` | `hecate-application::tests/shares_rekey.rs::new_shares_are_valid_shares_of_a_new_set` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_new_shares_still_work_after_a_later_passphrase_change` | `hecate-application::tests/shares_rekey.rs::new_shares_still_work_after_a_later_passphrase_change` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_rekey_needs_to_be_unsealed_and_sealing_drops_the_pending_set` | `hecate-application::tests/shares_rekey.rs::rekey_needs_to_be_unsealed_and_sealing_drops_the_pending_set` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_rekey_replaces_the_shares_and_leaves_everything_else` | `hecate-application::tests/shares_rekey.rs::rekey_replaces_the_shares_and_leaves_everything_else` | Unitaria |  |
| `unit/application/test_shares_rekey.py::test_the_proof_can_be_two_current_shares` | `hecate-application::tests/shares_rekey.rs::the_proof_can_be_two_current_shares` | Unitaria |  |
| `unit/application/test_unseal_passphrase.py::test_correct_passphrase_unseals` | `hecate-application::tests/unseal_passphrase.rs::correct_passphrase_unseals` | Unitaria |  |
| `unit/application/test_unseal_passphrase.py::test_failures_and_wait_survive_a_new_process` | `hecate-application::tests/unseal_passphrase.rs::failures_and_wait_survive_a_new_process` | Unitaria |  |
| `unit/application/test_unseal_passphrase.py::test_non_text_passphrase_is_invalid_input_and_does_not_count` | `hecate-application::tests/unseal_passphrase.rs::non_text_passphrase_is_invalid_input_and_does_not_count` | Unitaria |  |
| `unit/application/test_unseal_passphrase.py::test_unsealing_when_unsealed_or_uninitialized_is_a_conflict` | `hecate-application::tests/unseal_passphrase.rs::unsealing_when_unsealed_or_uninitialized_is_a_conflict` | Unitaria |  |
| `unit/application/test_unseal_passphrase.py::test_wait_after_third_failure_blocks_even_the_right_passphrase` | `hecate-application::tests/unseal_passphrase.rs::wait_after_third_failure_blocks_even_the_right_passphrase` | Unitaria |  |
| `unit/application/test_unseal_passphrase.py::test_wait_grows_with_each_failure` | `hecate-application::tests/unseal_passphrase.rs::wait_grows_with_each_failure` | Unitaria |  |
| `unit/application/test_unseal_passphrase.py::test_wrong_passphrase_stays_sealed_and_counts_a_failure` | `hecate-application::tests/unseal_passphrase.rs::wrong_passphrase_stays_sealed_and_counts_a_failure` | Unitaria |  |
| `unit/application/test_unseal_shares.py::test_a_badly_copied_share_is_detected_without_spending_an_attempt` | `hecate-application::tests/unseal_shares.rs::a_badly_copied_share_is_detected_without_spending_an_attempt` | Unitaria |  |
| `unit/application/test_unseal_shares.py::test_all_three_shares_also_work` | `hecate-application::tests/unseal_shares.rs::all_three_shares_also_work` | Unitaria |  |
| `unit/application/test_unseal_shares.py::test_any_ordered_pair_of_shares_unseals_and_recommends_a_new_passphrase` | `hecate-application::tests/unseal_shares.rs::any_ordered_pair_of_shares_unseals_and_recommends_a_new_passphrase` | Unitaria |  |
| `unit/application/test_unseal_shares.py::test_conflicts_when_unsealed_or_uninitialized` | `hecate-application::tests/unseal_shares.rs::conflicts_when_unsealed_or_uninitialized` | Unitaria |  |
| `unit/application/test_unseal_shares.py::test_failures_with_passphrase_and_with_shares_share_one_counter` | `hecate-application::tests/unseal_shares.rs::failures_with_passphrase_and_with_shares_share_one_counter` | Unitaria |  |
| `unit/application/test_unseal_shares.py::test_one_share_or_a_repeated_share_is_invalid_input_and_not_a_failure` | `hecate-application::tests/unseal_shares.rs::one_share_or_a_repeated_share_is_invalid_input_and_not_a_failure` | Unitaria |  |
| `unit/application/test_unseal_shares.py::test_shares_from_another_set_are_an_authentication_failure` | `hecate-application::tests/unseal_shares.rs::shares_from_another_set_are_an_authentication_failure` | Unitaria |  |
| `unit/domain/test_attempts.py::test_failures_keep_counting_after_wait_expires` | `hecate-domain::src/sealing/attempts.rs::failures_keep_counting_after_wait_expires` | Unitaria |  |
| `unit/domain/test_attempts.py::test_no_wait_until_second_failure` | `hecate-domain::src/sealing/attempts.rs::no_wait_until_second_failure` | Unitaria |  |
| `unit/domain/test_attempts.py::test_success_resets` | `hecate-domain::src/sealing/attempts.rs::success_resets` | Unitaria |  |
| `unit/domain/test_attempts.py::test_wait_counts_down_with_time` | `hecate-domain::src/sealing/attempts.rs::wait_counts_down_with_time` | Unitaria |  |
| `unit/domain/test_attempts.py::test_wait_series_after_third_failure` | `hecate-domain::src/sealing/attempts.rs::wait_series_after_third_failure` | Unitaria |  |
| `unit/domain/test_passphrase.py::test_accepts_twelve_or_more_characters` | `hecate-domain::src/sealing/passphrase.rs::accepts_twelve_or_more_characters` | Unitaria |  |
| `unit/domain/test_passphrase.py::test_rejects_fewer_than_twelve_characters` | `hecate-domain::src/sealing/passphrase.rs::rejects_fewer_than_twelve_characters` | Unitaria |  |
| `unit/domain/test_passphrase.py::test_rejects_non_text` | `hecate-infrastructure::tests/contract_init_ops.rs::a_passphrase_that_is_not_text_is_refused_before_the_service` | Unitaria | En Rust la frase es texto por tipo: una que no lo es la rechaza la comprobación de argumentos del canal |
| `unit/domain/test_passphrase.py::test_repr_and_str_do_not_show_the_value` | `hecate-domain::src/sealing/passphrase.rs::repr_and_str_do_not_show_the_value` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_a_maximum_of_two_destroys_the_oldest_with_a_third_version` | `hecate-domain::src/secrets/secret.rs::a_maximum_of_two_destroys_the_oldest_with_a_third_version` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_a_maximum_outside_2_to_100_is_rejected` | `hecate-domain::src/secrets/secret.rs::a_maximum_outside_2_to_100_is_rejected`, `hecate-application::tests/secrets_create.rs::a_maximum_that_is_not_an_integer_is_rejected_like_one_out_of_range` | Unitaria | `True` y `"2"` llegan como JSON al servicio, que los rechaza con el mismo mensaje; `None` es en el servicio el valor por defecto, como en Python |
| `unit/domain/test_secret_aggregate.py::test_a_version_past_its_grace_can_be_destroyed` | `hecate-domain::src/secrets/secret.rs::a_version_past_its_grace_can_be_destroyed` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_a_version_past_its_grace_can_be_restored_but_not_revoked_or_promoted` | `hecate-domain::src/secrets/secret.rs::a_version_past_its_grace_can_be_restored_but_not_revoked_or_promoted` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_adding_a_version_deprecates_the_previous_active_one` | `hecate-domain::src/secrets/secret.rs::adding_a_version_deprecates_the_previous_active_one` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_adding_a_version_with_nothing_active_deprecates_nothing` | `hecate-domain::src/secrets/secret.rs::adding_a_version_with_nothing_active_deprecates_nothing` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_create_gives_version_one_active_and_the_metadata` | `hecate-domain::src/secrets/secret.rs::create_gives_version_one_active_and_the_metadata` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_defaults_to_ten_versions_without_rotation_or_expiry` | `hecate-domain::src/secrets/secret.rs::defaults_to_ten_versions_without_rotation_or_expiry` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_effective_state_flips_exactly_at_the_end_of_the_grace` | `hecate-domain::src/secrets/secret.rs::effective_state_flips_exactly_at_the_end_of_the_grace` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_events_never_carry_a_value_or_a_key` | `hecate-domain::src/secrets/secret.rs::events_never_carry_a_value_or_a_key` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_exceeding_the_maximum_destroys_the_oldest_in_the_same_change` | `hecate-domain::src/secrets/secret.rs::exceeding_the_maximum_destroys_the_oldest_in_the_same_change` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_from_events_needs_a_creation_event_first` | `hecate-domain::src/secrets/secret.rs::from_events_needs_a_creation_event_first` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_only_one_version_is_ever_active` | `hecate-domain::src/secrets/secret.rs::only_one_version_is_ever_active` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_only_the_documented_transitions_are_allowed` | `hecate-domain::src/secrets/secret.rs::only_the_documented_transitions_are_allowed` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_promote_swaps_which_version_is_active` | `hecate-domain::src/secrets/secret.rs::promote_swaps_which_version_is_active` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_promote_when_nothing_is_active_only_promotes` | `hecate-domain::src/secrets/secret.rs::promote_when_nothing_is_active_only_promotes` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_rebuilds_the_same_state_from_its_events` | `hecate-domain::src/secrets/secret.rs::rebuilds_the_same_state_from_its_events` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_restore_makes_it_deprecated_with_a_new_grace` | `hecate-domain::src/secrets/secret.rs::restore_makes_it_deprecated_with_a_new_grace` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_revoking_the_active_version_leaves_no_active_one` | `hecate-domain::src/secrets/secret.rs::revoking_the_active_version_leaves_no_active_one` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_stream_version_grows_with_every_event` | `hecate-domain::src/secrets/secret.rs::stream_version_grows_with_every_event` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_the_grace_is_the_one_received` | `hecate-domain::src/secrets/secret.rs::the_grace_is_the_one_received` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_unknown_versions_are_not_found` | `hecate-domain::src/secrets/secret.rs::unknown_versions_are_not_found` | Unitaria |  |
| `unit/domain/test_secret_aggregate.py::test_with_the_minimum_of_two_the_previous_version_survives_a_rotation_in_its_grace` | `hecate-domain::src/secrets/secret.rs::with_the_minimum_of_two_the_previous_version_survives_a_rotation_in_its_grace` | Unitaria |  |
| `unit/domain/test_secret_path.py::test_every_well_formed_path_round_trips` | `hecate-domain::src/secrets/path.rs::every_well_formed_path_round_trips` | Unitaria |  |
| `unit/domain/test_secret_path.py::test_parses_a_valid_path_and_exposes_its_owner` | `hecate-domain::src/secrets/path.rs::parses_a_valid_path_and_exposes_its_owner` | Unitaria |  |
| `unit/domain/test_secret_path.py::test_reference_with_version` | `hecate-domain::src/secrets/path.rs::reference_with_version` | Unitaria |  |
| `unit/domain/test_secret_path.py::test_reference_without_version` | `hecate-domain::src/secrets/path.rs::reference_without_version` | Unitaria |  |
| `unit/domain/test_secret_path.py::test_rejects_malformed_paths` | `hecate-domain::src/secrets/path.rs::rejects_malformed_paths` | Unitaria |  |
| `unit/domain/test_secret_path.py::test_rejects_malformed_references` | `hecate-domain::src/secrets/path.rs::rejects_malformed_references` | Unitaria |  |
| `unit/domain/test_secret_path.py::test_rejects_non_text` | `hecate-infrastructure::tests/contract_secret_ops.rs::arguments_that_are_not_text_are_refused_before_the_service` | Unitaria | En Rust la ruta es texto por tipo: una que no lo es la rechaza la comprobación de argumentos del canal |
| `unit/domain/test_share.py::test_any_changed_character_is_detected` | `hecate-domain::src/sealing/share.rs::any_changed_character_is_detected` | Unitaria |  |
| `unit/domain/test_share.py::test_lowercase_spaces_and_confusable_letters_are_accepted` | `hecate-domain::src/sealing/share.rs::lowercase_spaces_and_confusable_letters_are_accepted` | Unitaria |  |
| `unit/domain/test_share.py::test_repr_does_not_show_fragments` | `hecate-domain::src/sealing/share.rs::repr_does_not_show_fragments` | Unitaria |  |
| `unit/domain/test_share.py::test_text_roundtrip` | `hecate-domain::src/sealing/share.rs::text_roundtrip` | Unitaria |  |
| `unit/domain/test_share.py::test_truncated_or_wrong_prefix_is_rejected` | `hecate-domain::src/sealing/share.rs::truncated_or_wrong_prefix_is_rejected` | Unitaria |  |
| `unit/domain/test_value_type.py::test_json_accepts_valid_and_rejects_invalid` | `hecate-domain::src/secrets/value_type.rs::json_accepts_valid_and_rejects_invalid` | Unitaria |  |
| `unit/domain/test_value_type.py::test_parse_known_and_unknown_types` | `hecate-domain::src/secrets/value_type.rs::parse_known_and_unknown_types`, `hecate-infrastructure::tests/contract_secret_ops.rs::arguments_that_are_not_text_are_refused_before_the_service` | Unitaria | `None` y `3` no llegan al dominio en Rust (el tipo es texto): los rechaza la comprobación de argumentos del canal |
| `unit/domain/test_value_type.py::test_pem_accepts_one_or_more_blocks` | `hecate-domain::src/secrets/value_type.rs::pem_accepts_one_or_more_blocks` | Unitaria |  |
| `unit/domain/test_value_type.py::test_pem_over_the_limit_is_rejected` | `hecate-domain::src/secrets/value_type.rs::pem_over_the_limit_is_rejected` | Unitaria |  |
| `unit/domain/test_value_type.py::test_pem_rejects_malformed` | `hecate-domain::src/secrets/value_type.rs::pem_rejects_malformed` | Unitaria |  |
| `unit/domain/test_value_type.py::test_size_limit_counts_bytes_not_characters` | `hecate-domain::src/secrets/value_type.rs::size_limit_counts_bytes_not_characters` | Unitaria |  |
| `unit/domain/test_value_type.py::test_size_limit_is_64_kib_for_every_type` | `hecate-domain::src/secrets/value_type.rs::size_limit_is_64_kib_for_every_type` | Unitaria |  |
| `unit/domain/test_value_type.py::test_string_accepts_utf8_text_and_returns_its_bytes` | `hecate-domain::src/secrets/value_type.rs::string_accepts_utf8_text_and_returns_its_bytes` | Unitaria |  |
| `unit/domain/test_value_type.py::test_string_rejects_empty_and_non_text` | `hecate-domain::src/secrets/value_type.rs::string_rejects_empty`, `hecate-infrastructure::tests/contract_secret_ops.rs::arguments_that_are_not_text_are_refused_before_the_service` | Unitaria | Los bytes y el sustituto suelto no llegan al dominio en Rust: un `value` que no es texto lo rechaza la comprobación de argumentos y un sustituto suelto, la lectura del JSON (FR-018) |
| `unit/test_config.py::test_data_dir_is_required` | `hecate-infrastructure::tests/config.rs::data_dir_is_required` | Unitaria |  |
| `unit/test_config.py::test_defaults_are_the_sensitive_profile_and_a_socket_in_the_data_dir` | `hecate-infrastructure::tests/config.rs::defaults_are_the_sensitive_profile_and_a_socket_in_the_data_dir` | Unitaria |  |
| `unit/test_config.py::test_moderate_profile_and_custom_socket_path_are_accepted` | `hecate-infrastructure::tests/config.rs::moderate_profile_and_custom_socket_path_are_accepted` | Unitaria |  |
| `unit/test_config.py::test_prepare_creates_a_private_directory` | `hecate-infrastructure::tests/config.rs::prepare_creates_a_private_directory` | Unitaria |  |
| `unit/test_config.py::test_unknown_profiles_are_rejected_so_no_weak_profile_exists_in_production` | `hecate-infrastructure::tests/config.rs::unknown_profiles_are_rejected_so_no_weak_profile_exists_in_production` | Unitaria |  |
| `unit/test_hardening.py::test_a_rejected_peer_is_closed_without_reading_or_handling` | `hecate-infrastructure::tests/hardening.rs::a_rejected_peer_is_closed_without_reading_or_handling` | Unitaria |  |
| `unit/test_hardening.py::test_connections_from_another_user_are_rejected_on_linux` | `hecate-infrastructure::tests/hardening.rs::connections_from_another_user_are_rejected_on_linux` | Unitaria |  |
| `unit/test_hardening.py::test_connections_from_the_same_user_are_accepted_on_linux` | `hecate-infrastructure::tests/hardening.rs::connections_from_the_same_user_are_accepted_on_linux` | Unitaria |  |
| `unit/test_hardening.py::test_core_dumps_are_disabled` | `hecate-infrastructure::tests/hardening.rs::core_dumps_are_disabled` | Unitaria |  |
| `unit/test_hardening.py::test_linux_locks_memory_and_warns_when_it_cannot` | `hecate-infrastructure::tests/hardening.rs::linux_locks_memory_and_warns_when_it_cannot` | Unitaria |  |
| `unit/test_hardening.py::test_macos_warns_that_memory_is_not_locked` | `hecate-infrastructure::tests/hardening.rs::macos_warns_that_memory_is_not_locked` | Unitaria |  |
| `unit/test_hardening.py::test_other_platforms_rely_on_the_private_socket_permissions` | `hecate-infrastructure::tests/hardening.rs::other_platforms_rely_on_the_private_socket_permissions` | Unitaria |  |

## Cambios a propósito

- `HEC2`: las partes de recuperación usan el formato `HEC2` (Shamir en GF(256)) y una parte `HEC1` se rechaza
  (FR-005). Pruebas en Rust: las de `unit/domain/test_share.py`, trasladadas con partes `HEC2`, y la nueva
  `hecate-domain::src/sealing/share.rs::a_share_in_the_python_hec1_format_is_rejected_as_badly_copied`.
- FR-018: las entradas siguen su formato estándar (el JSON del canal y del tipo `json`, los PEM y `expires_at`).
  Pruebas en Rust, nuevas: `hecate-infrastructure::tests/protocol.rs::input_that_is_not_standard_json_is_rejected`,
  `hecate-infrastructure::tests/control_channel.rs::a_request_that_is_not_standard_json_leaves_no_access_log_row`,
  `hecate-domain::src/instant.rs::instants_are_read_like_python_except_the_deliberate_changes` (también
  `expires_at`, que el servicio lee con la misma función),
  `hecate-domain::src/secrets/value_type.rs::values_are_checked_like_python_except_the_deliberate_changes` (los
  `json` y los PEM de `values.json`) y
  `hecate-infrastructure::tests/contract_secret_ops.rs::arguments_that_are_not_text_are_refused_before_the_service`
  (un valor con un sustituto suelto no es JSON estándar).
- Caminos de error de la CLI, comunicados al propietario durante la implementación y registrados por su decisión tras
  la convergencia. Cancelar con Ctrl-C o fin de entrada fuera de la confirmación de partes imprime `cancelled` y sale
  con 1, y un fallo al arrancar `hecate serve` imprime `error: …` y sale con 1; en los dos casos la versión en Python
  terminaba con una excepción sin tratar. Un `--from-file` que no se puede leer da un motivo neutro (`not found`,
  `permission denied`, `is a directory`, `not a directory` o `not valid UTF-8`) en lugar del nombre de la excepción.
  Pruebas en Rust, nuevas:
  `hecate-cli::tests/cli.rs::cancelling_a_prompt_outside_the_share_confirmation_prints_cancelled_and_exits_with_1`,
  `hecate-cli::tests/cli_secrets.rs::cancelling_the_value_prompt_creates_nothing`,
  `hecate-cli::tests/process.rs::serve_that_cannot_prepare_its_data_directory_exits_with_1_and_an_error` y
  `hecate-cli::tests/cli_secrets.rs::a_file_that_cannot_be_read_is_reported_with_its_reason`.
