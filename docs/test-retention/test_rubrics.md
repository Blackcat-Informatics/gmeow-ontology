# Retention: `tests/test_rubrics.py`

**Category:** Merged-graph guard

## What it tests

The rubrics facility, in the norms slice.

Retained dynamic tests:

- `test_no_preferred_assessment_machinery` — No preferredScore / canonicalAssessment selectors : two judges disagreeing are two coexisting cells.
- `test_two_judges_disagree_without_contradiction` — The LLM-judge doctrine in fixture form: one chunk, two vantages, two scores — both cells stand.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
