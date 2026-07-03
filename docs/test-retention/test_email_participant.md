# Retention: `tests/test_email_participant.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Structural guards for the MessageParticipant relator and EmailAddress facets.

Retained dynamic tests:

- `test_resent_properties_are_multivalued_in_linkml_schema` — Regression guard: non-functional datatype properties must compile to multivalued slots (review feedback).
- `test_fixture_binds_occurrence_correctly` — The coverage fixture shows alice@example.
- `test_fixture_address_decomposition`

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
