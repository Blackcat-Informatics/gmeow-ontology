# Retention: `tests/test_mereology.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Universal mereology spine — structural TBox well-formedness (#76).

Retained dynamic tests:

- `test_no_winner_or_cardinality_terms_for_parts`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
