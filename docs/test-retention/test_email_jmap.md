# Retention: `tests/test_email_jmap.py`

**Category:** Merged-graph guard

## What it tests

Structural guards for JMAP structural identifiers: blobId, bodyStructure, and BodyValue.

Retained dynamic tests:

- `test_fixture_includes_jmap_identifiers` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
