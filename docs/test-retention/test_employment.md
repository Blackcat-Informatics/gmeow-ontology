# Retention: `tests/test_employment.py`

**Category:** Merged-graph guard

## What it tests

Standpoint guards for the employment module (retained pytest subset).

Retained dynamic tests:

- `test_employment_event_types_are_values` — Principle 9: employment events are EventType values, never Event subclasses.
- `test_contested_employment_coexists` — Two contradictory standpoint-indexed employment claims load, SHACL-pass, and are BOTH retained — neither is the ground truth.
- `test_withdrawn_employment_suppressed_not_deleted` — A closed Employment with displayable false is retained.
- `test_no_preferred_or_primary_employment_term` — Principle 9: no single slot to win — employment mints no preferred/primary selector for a contested job, role, or tenure.

## Why it cannot be deleted or moved to Rust today

Standpoint guards for the employment module (retained pytest subset).
