# Retention: `tests/test_aboutness.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The universal aboutness vocabulary (#349, EPIC #348).

Retained dynamic tests:

- `test_aboutness_orthogonal_to_other_axes` — hasAboutness ⟂ every other kernel axis: no inferential bridge (Principle 9).
- `test_no_aboutness_truth_bridge` — Enactment never implies assertion: no axiom links aboutness to veridicality or standpoint modality (the licensed-falsehood boundary is a documented bridge, not an entailment).

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
