# Retention: `tests/test_allen_jepd.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Tests for Allen interval relations and JEPD disjointness (issue #67).

Retained dynamic tests:

- `test_no_owl_all_disjoint_properties_over_interval_relations` — OWL 2 DL forbids DisjointObjectProperties over non-simple (transitive) properties.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
