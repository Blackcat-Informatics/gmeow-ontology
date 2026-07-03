# Retention: `tests/test_norms.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The norms extension + rights graft (#351 / #352, EPIC #348) — RETAINED tests.

Retained dynamic tests:

- `test_graft_axioms_live_extension_side_only` — Zero core churn: the core rights module contains no reference to any norms-extension IRI — the graft is asserted in the norms module.

## Why it cannot move to Rust today

Dynamic assertions that do not map cleanly to module-scoped sparql ask/select cells.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
