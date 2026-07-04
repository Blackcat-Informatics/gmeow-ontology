# Retention: `tests/test_trust.py`

**Category:** Merged-graph guard

## What it tests

Retained pytest guards for the trust (Web-of-Trust) slice.

Retained dynamic tests:

- `test_three_axes_are_orthogonal_in_trust` — accordingTo ⟂ wasAttributedTo ⟂ confidence: no inferential bridge in the trust module (mirrors test_three_axes_are_orthogonal in test_standpoint.
- `test_no_preferred_or_primary_trust_term` — Principle 9: no single slot to win — trust mints no preferred/primary selector for a contested certification or trust level.

## Why it cannot be deleted or moved to Rust today

Retained pytest guards for the trust (Web-of-Trust) slice.
