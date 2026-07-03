# Retention: `tests/test_rights.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Tests for the rights / IP / trademark / licensing facility (#21).

Retained dynamic tests:

- `test_expanded_action_vocabulary_is_seeded` — The ODRL Common-Vocabulary actions are seeded (maximal, not a thin stub).
- `test_odrl_projection_emits_a_policy_with_rules`
- `test_odrl_projection_emits_constraint_and_conflict_logic`
- `test_spdx_projection_emits_listed_license`
- `test_cc_projection_emits_license_and_attribution`
- `test_dcterms_projection_emits_flat_rights`
- `test_schema_projection_emits_rights_cluster`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; consumer-projection round-trips exercised through the python projection harness; abox fixture instance checks.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
