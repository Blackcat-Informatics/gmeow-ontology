# Retention: `tests/test_ai_claims.py`

**Category:** Merged-graph guard

## What it tests

Competency tests for the AI claim layer and the graphrag extension.

Retained dynamic tests:

- `test_no_parallel_claim_construct_exists` — gmeow:Observation IS the universal claim construct — the earlier Claim/GeneratedClaim/ExtractedClaim classes must never return.
- `test_no_parallel_evaluation_construct_exists` — Evaluation is the norms extension's Assessment (judge-as-vantage).
- `test_no_duplicate_provenance_properties` — Outputs hang off the EXISTING wasGeneratedBy — no forward duplicates.
- `test_no_winner_machinery_anywhere` — Contradictions surface; nothing ranks them (P9).
- `test_no_new_identity_axes_were_minted` — The AI layer carries WHO SAID, never WHO IS.
- `test_assessment_seam_is_the_norms_extensions` — The fixture's evaluator is an Assessment — the judge is just a vantage.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
