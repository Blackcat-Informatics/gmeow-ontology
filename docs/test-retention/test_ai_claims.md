# Retention: `tests/test_ai_claims.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Competency tests for the AI claim layer and the graphrag extension.

Retained dynamic tests:

- `test_no_parallel_claim_construct_exists` — gmeow:Observation IS the universal claim construct — the earlier parallel
- `test_no_parallel_evaluation_construct_exists` — Evaluation is the norms extension's Assessment (judge-as-vantage).
- `test_no_duplicate_provenance_properties` — Outputs hang off the EXISTING wasGeneratedBy — no forward duplicates.
- `test_no_winner_machinery_anywhere` — Contradictions surface; nothing ranks them (P9).
- `test_no_new_identity_axes_were_minted` — The AI layer carries WHO SAID, never WHO IS.
- `test_assessment_seam_is_the_norms_extensions` — The fixture's evaluator is an Assessment — the judge is just a vantage.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; abox fixture instance checks.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
