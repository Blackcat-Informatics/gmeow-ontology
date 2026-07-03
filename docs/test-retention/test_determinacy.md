# Retention: `tests/test_determinacy.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The universal determinacy vocabulary (#71).

Retained dynamic tests:

- `test_no_preferred_or_primary_term_is_declared` — No GMEOW vocabulary term is a preferred/primary selector (Principle 9).

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
