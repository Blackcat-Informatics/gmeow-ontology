# Retention: `tests/test_trust.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Retained pytest guards for the trust (Web-of-Trust) slice.

Retained dynamic tests:

- `test_three_axes_are_orthogonal_in_trust` — accordingTo ⟂ wasAttributedTo ⟂ confidence: no inferential bridge in the trust module (mirrors test_three_axes_are_orthogonal in test_standpoint.
- `test_no_preferred_or_primary_trust_term` — Principle 9: no single slot to win — trust mints no preferred/primary selector for a contested certification or trust level.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
