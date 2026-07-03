# Retention: `tests/test_disclosure.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Tests for the consumer projection policy / disclosure control facility.

Retained dynamic tests:

- `test_no_preferred_or_primary_disclosure_term` — No `gmeow:primary*` / `gmeow:preferred*` disclosure term.
- `test_project_when_in_sparql_query` — The schema-org SPARQL projection contains the projectWhen FILTER EXISTS guard.
- `test_public_candidates_query_runnable` — public-candidates.
- `test_privacy_leaks_query_runnable` — privacy-leaks.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; consumer-projection round-trips exercised through the python projection harness; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
