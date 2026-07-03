# Retention: `tests/test_cognition.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Retained guards for the cognition module.

Retained dynamic tests:

- `test_mental_moment_has_exactly_one_gufo_metaclass` — Each new class carries exactly one ontological metaclass.
- `test_cognition_sssom_rows_include_expected_alignments` — The cognition SSSOM ledger contains the expected cross-ontology rows.
- `test_cognition_sssom_includes_corrected_wikidata_qids` — The issue-supplied QIDs were rejected and replaced with verified entities.
- `test_cognition_sssom_includes_opencyc_knows_about` — OpenCyc knowsAbout is present as a relatedMatch anchor.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; sssom mapping ledger reads through the python mapping harness.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
