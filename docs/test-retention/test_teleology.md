# Retention: `tests/test_teleology.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The teleology core slice (#350, EPIC #348).

Retained dynamic tests:

- `test_no_preferred_or_primary_goal_terms` — No preferredGoal / primaryIntention selectors exist (Principle 9).

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
