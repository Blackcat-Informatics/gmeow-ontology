# Retention: `tests/test_feedback_bundle.py`

**Category:** Python tool algorithm

## What it tests

Tests for the self-describing diagnostics feedback bundle.

Retained dynamic tests:

- `test_feedback_bundle_carries_sarif_and_findings_blobs` — Retained dynamic test.
- `test_feedback_bundle_self_attests` — The embedded report's stamped snapshot id matches the bundle.
- `test_feedback_bundle_is_deterministic` — Retained dynamic test.
- `test_empty_report_bundle_round_trips` — Retained dynamic test.
- `test_verify_returns_false_on_garbage_bytes` — A verifier on a trust boundary must not raise on unreadable input.
- `test_verify_returns_false_on_truncated_bundle` — A truncated (tampered) bundle is not a valid self-attestation, not a crash.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.
