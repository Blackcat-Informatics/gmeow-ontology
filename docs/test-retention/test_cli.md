# Retention: `tests/test_cli.py`

**Category:** Python CLI surface

## What it tests

Tests for CLI command behaviour.

Retained dynamic tests:

- `test_quality_strict_fails_when_oops_raises` — Retained dynamic test.
- `test_quality_best_effort_skips_when_oops_raises` — Retained dynamic test.
- `test_quality_foops_strict_fails_when_foops_raises` — Retained dynamic test.
- `test_quality_foops_best_effort_skips_when_foops_raises` — Retained dynamic test.
- `test_describe_unknown_language_fails` — Retained dynamic test.
- `test_describe_renders_french` — A fixture with French labels renders them without an English fallback marker.
- `test_describe_renders_mandarin` — A fixture with Mandarin labels renders them without a fallback marker.
- `test_describe_unknown_language_error_is_content_aware` — When content is limited, the error list does not advertise the full catalog.
- `test_describe_fallback_marker_for_missing_language` — An English-only fixture falls back when French is requested.
- `test_describe_env_language_rejected_if_unknown` — Retained dynamic test.
- `test_describe_explicit_empty_lang_overrides_env` — --lang '' wins over GMEOW_LANG and selects the default English carrier.
- `test_describe_env_empty_lang_defaults_to_english` — An empty GMEOW_LANG env value maps to the default English carrier.
- `test_export_respects_language_selector` — Retained dynamic test.
- `test_export_lang_flag_wins_over_env` — --lang wins over GMEOW_LANG when exporting CSVs.
- `test_public_cli_excludes_checkout_commands` — Retained dynamic test.
- `test_public_gts_cli_excludes_compile_commands` — Retained dynamic test.
- `test_gts_shim_fails_when_binary_missing` — Retained dynamic test.
- `test_gts_shim_injects_snapshot_for_default_subcommands` — Retained dynamic test.
- `test_gts_shim_forwards_explicit_file` — Retained dynamic test.
- `test_gts_shim_forwards_non_default_command` — Retained dynamic test.
- `test_gts_shim_runs_help_when_no_args` — Retained dynamic test.
- `test_gts_shim_injects_snapshot_before_flags` — Retained dynamic test.
- `test_gts_shim_does_not_inject_when_file_follows_flags` — Retained dynamic test.
- `test_gts_shim_does_not_inject_after_double_dash` — Retained dynamic test.
- `test_gts_shim_injects_snapshot_for_extract_key` — Retained dynamic test.
- `test_gts_shim_does_not_inject_for_extract_key_with_file` — Retained dynamic test.
- `test_gts_shim_handles_os_error` — Retained dynamic test.
- `test_dev_cli_keeps_checkout_commands` — Retained dynamic test.
- `test_dev_cli_has_compile_gts_commands` — Retained dynamic test.
- `test_dev_i18n_help_lists_sync_english` — Retained dynamic test.
- `test_dev_i18n_sync_english_dry_run` — Retained dynamic test.
- `test_dev_i18n_extract` — Retained dynamic test.
- `test_dev_i18n_extract_produces_docs_pot_files` — Retained dynamic test.
- `test_dev_i18n_extract_lang_includes_language_tag_in_paths` — Retained dynamic test.
- `test_dev_i18n_extract_terms_only_skips_docs` — Retained dynamic test.
- `test_dev_i18n_merge_outputs_multilingual_graph` — Retained dynamic test.
- `test_dev_i18n_merge_writes_stdout` — Retained dynamic test.
- `test_dev_i18n_help_lists_export_commands` — Retained dynamic test.
- `test_dev_i18n_export_csv_shape` — Retained dynamic test.
- `test_dev_i18n_export_csv_to_file` — Retained dynamic test.
- `test_dev_i18n_export_xliff_shape` — Retained dynamic test.
- `test_dev_i18n_export_xliff_escapes_xml` — Retained dynamic test.
- `test_dev_i18n_export_xliff_to_file` — Retained dynamic test.
- `test_workspace_declares_separate_dev_package` — Retained dynamic test.
- `test_dev_validate_unsigned_gts_passes` — An unsigned, ontologically valid bundle validates normally.
- `test_dev_validate_unsigned_gts_require_signed_fails` — --require-signed aborts an unsigned bundle with signature.
- `test_dev_validate_signed_trusted_gts_passes` — A signed bundle whose signer is in the trust policy passes.
- `test_dev_validate_signed_untrusted_gts_fails` — A signed bundle whose signer is not trusted fails with signature.
- `test_dev_validate_require_signed_without_gts_errors` — Signature flags are only meaningful together with --gts.
- `test_dev_validate_gts_with_trusted_key_cli_flag` — A signed bundle validates when the signer key is passed via --trusted-key.
- `test_dev_validate_gts_with_untrusted_key_cli_flag_fails` — A wrong --trusted-key cannot verify the signature, so validation fails.

## Why it cannot be deleted or moved to Rust today

The CLIs under test are Typer applications; their behavior is exercised through CliRunner and subprocess integration, which is inherently Python-only surface.
