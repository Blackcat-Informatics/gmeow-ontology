# Retention: `tests/test_aboutness.py`

**Category:** Merged-graph guard

## What it tests

The universal aboutness vocabulary.

Retained dynamic tests:

- `test_aboutness_orthogonal_to_other_axes` — hasAboutness ⟂ every other kernel axis: no inferential bridge.
- `test_no_aboutness_truth_bridge` — Enactment never implies assertion: no axiom links aboutness to veridicality or standpoint modality (the licensed-falsehood boundary is a documented bridge, not an entailment).

## Why it cannot be deleted or moved to Rust today

The two tests RETAINED here assert ABSENCE over the whole merged graph (include_imports=False) — orthogonality across axes (gmeow:confidence, gmeow:hasGranularity, … declared in 10+ slices) and the seeds' exactly-one-type guarantee — which the module-scoped (gmeow:scopeModule) cell DSL cannot express faithfully, so they stay as Python merged-graph assertions.
