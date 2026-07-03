# Retention: `tests/test_deception.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Deception -- SHACL / dynamic tests retained.

Retained dynamic tests:

- `test_blame_deflection_example_uses_doxastic_standpoint_claims` — Every held/projected standpoint in the blame-deflection example is typed gmeow:DoxasticStandpointClaim.
- `test_licensed_falsehood_not_a_lie` — Negative guard: a fiction claim under a NarrativeReferenceFrame must NOT be typed as a lie event — the licensed-falsehood safety property.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; shacl conformance calls against abox fixture data.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
