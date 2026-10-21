# Validation

Source frozen for root review. No formatting, builds, tests, host probes, producer, commit or publication run by this lane. Root Make/Cargo resolved Cargo.lock: only transcode removes serde/serde_json and adds purrdf-lex. Root formatting, all7 selected pure tests and strict producer-independent prebuild Clippy passed. Artifact parity remains unrun.

## Source observations

Only crates/transcode/Cargo.toml, src/lib.rs and src/lib.tests.rs changed. No serde/serde_json reference remains in that crate. RealizedLoss and MatrixEntry now use the existing owner's ToJson-only json_record declarations, preserving their prior declaration-order members. The public String-returning functions and all production callers remain unchanged. Realized losses still enter LossLedger and serialize its sorted/aggregated rows. JSON output uses infallible write_pretty; silent serialization-failure empty arrays are gone.

The static substrate matrix flows through the actual supported_loss_matrix_json helper, using record::read_document for recursive sorted-last-wins objects. The owner matrix must be valid JSON and an array; violation retains explicit hard failure. Filtering behavior for unsupported/non-string endpoint rows is preserved. Embedded numeric lexemes follow the owner rule and are pinned explicitly; there are no numeric floats in the authored output records.

## New pure coverage (all five executed and passed)

- json_realized_loss_preserves_literal_record_bytes_and_ledger_aggregation: complete literal member/order/Unicode/escape/u64-max/zero/count aggregation output, reordered inputs, empty report and absence of terminal newline.
- json_matrix_records_preserve_literal_field_order: literal record bytes, actual complete matrix member order, sorted codec pairs/loss codes and one terminal newline.
- json_free_form_loss_matrix_keeps_sorted_last_wins_objects_and_newline: actual filtering helper, duplicate endpoint, nested key order, null/boolean, unsupported rows, explicit number-lexeme rule and empty output newline.
- json_static_loss_matrix_refuses_malformed_owner_data and json_static_loss_matrix_refuses_non_array_owner_data: fail closed with operation-specific static-owner messages.

Existing transcode_loss_matrix_contains_only_supported_codecs now reads through the owner. All existing codec/refusal tests and transcode_matrix_has_not_drifted remain intact. The latter requires the separately produced generated/transcode-matrix.json and is not producer-independent.

Suggested initial root selection: make nextest-synthetic NEXTEST_FILTER='package(gmeow-transcode) and (test(json_) or test(transcode_matrix_is_deterministic) or test(transcode_loss_matrix_contains_only_supported_codecs))'. Actual CLI/MCP integration and generated artifact parity remain unrun. Full #1776 remains open; this unit removes only transcode's two manifest dependencies.

Root qualification: make fmt passed; make nextest-synthetic with the stated seven-test selection passed (7 executed,7 passed,19 package tests skipped); complete native-qualification.log retained. Strict make clippy-prebuild passed with warnings denied (embedded consumer packages excluded). Normal signed commit/push hooks passed. Complete current hosted capture has no formal/inline/thread finding and one bot progress entry. No generated-artifact drift, corpus/full-gate, installed CLI or actual MCP execution is claimed; full1776 remains open under the campaign exception.
