# Retention: `tests/test_email_behavioral.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Structural guards for email behavioral metadata: MessageKind, header facets,
and disposition-notification request. Issue #137.

Retained dynamic tests:

- `test_fixture_dsn_has_overlapping_kinds` — msgDsn is a bounce, a DSN, and auto-generated — demonstrating overlap.
- `test_fixture_auto_generated_message` — msgAuto is auto-generated and linked to a SoftwareAgent.
- `test_fixture_read_receipt_request` — msg3 requests a read receipt via dispositionNotificationTo.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
