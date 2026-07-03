# Retention: `tests/test_registers.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The registers & personas facility, in the norms slice.

Retained dynamic tests:

- `test_no_primary_persona_machinery` — No primaryPersona / preferredRegister selectors exist (Principle 9).
- `test_divergence_query_surfaces_legal_divergence` — Add a private-only norm: the query reports it (and SHACL still conforms — divergence is not a violation).

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; shacl conformance calls against abox fixture data; abox fixture instance checks; python-only file or value inspections.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
