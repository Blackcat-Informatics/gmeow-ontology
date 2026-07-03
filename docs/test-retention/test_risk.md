# Retention: `tests/test_risk.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The risk slice — retained pytest tests.

Retained dynamic tests:

- `test_no_occurrence_gate` — The no-occurrence-pattern gate: loading the risk fixtures and worked example entails ZERO gmeow:Event instances — cascades are expressible without anything having happened.
- `test_competency_severity_order_query`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
