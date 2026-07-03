# Retention: `tests/test_email_calendar.py`

**Category:** Merged-graph guard

## What it tests

Structural guards for the calendar invitation email→event bridge.

Retained dynamic tests:

- `test_fixture_calendar_invitation_links_to_event` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
