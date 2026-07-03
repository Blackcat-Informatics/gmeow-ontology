# Retention: `tests/test_calendar.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The calendar and scheduling slice (#62) — RETAINED pytest guards.

Retained dynamic tests:

- `test_calendar_temporal_datatypes_are_datetime_or_duration`
- `test_calendar_axes_are_independent`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
