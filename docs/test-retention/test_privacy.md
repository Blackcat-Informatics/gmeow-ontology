# Retention: `tests/test_privacy.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Tests for the privacy / consent / redaction facility (#73, PRIV-GEN).

Retained dynamic tests:

- `test_no_preferred_or_primary_sensitivity_term` — No `gmeow:primary*` / `gmeow:preferred*` privacy term.
- `test_odrl_projection_emits_privacy_policy`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; consumer-projection round-trips exercised through the python projection harness; abox fixture instance checks.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
