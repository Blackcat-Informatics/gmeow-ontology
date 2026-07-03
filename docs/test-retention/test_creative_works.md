# Retention: `tests/test_creative_works.py`

**Category:** Domain invariant → slicetest cells

## What it tests

WEMI creative-works spine (issue #208).

Retained dynamic tests:

- `test_wemi_tiers_subclass_information_object` — Verify each WEMI tier class is a subclass (transitively) of InformationObject.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
