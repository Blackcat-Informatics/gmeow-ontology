# Retention: `tests/test_compliance.py`

**Category:** Oracle / Docker orchestration

## What it tests

Compliance-report tests.

Retained dynamic tests:

- `test_report_is_valid_turtle_covering_every_principle` — Retained dynamic test.
- `test_report_carries_supersession_edges` — The report flows the manifest's supersession/extends edges per principle.
- `test_runnable_gates_report_passed_and_failures_propagate` — Retained dynamic test.
- `test_out_of_process_enforcement_is_gated_in_ci_never_silent` — Retained dynamic test.
- `test_report_carries_provenance` — Retained dynamic test.
- `test_prior_gate_evidence_mode_marks_runnable_gates_passed` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Drives external reasoners or Docker-backed tooling that has no Rust twin by design.
