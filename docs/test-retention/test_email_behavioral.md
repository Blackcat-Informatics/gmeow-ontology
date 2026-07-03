# Retention: `tests/test_email_behavioral.py`

**Category:** Merged-graph guard

## What it tests

Structural guards for email behavioral metadata: MessageKind, header facets, and disposition-notification request.

Retained dynamic tests:

- `test_fixture_dsn_has_overlapping_kinds` — msgDsn is a bounce, a DSN, and auto-generated — demonstrating overlap.
- `test_fixture_auto_generated_message` — msgAuto is auto-generated and linked to a SoftwareAgent.
- `test_fixture_read_receipt_request` — msg3 requests a read receipt via dispositionNotificationTo.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
