# Retention: `tests/test_email_calendar.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Structural guards for the calendar invitation email→event bridge.

Retained dynamic tests:

- `test_fixture_calendar_invitation_links_to_event`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
