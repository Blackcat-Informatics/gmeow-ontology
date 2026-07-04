# Retention: `tests/test_privacy.py`

**Category:** Merged-graph guard

## What it tests

Tests for the privacy / consent / redaction facility.

Retained dynamic tests:

- `test_no_preferred_or_primary_sensitivity_term` — No `gmeow:primary*` or `gmeow:preferred*` privacy term.
- `test_odrl_projection_emits_privacy_policy` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
