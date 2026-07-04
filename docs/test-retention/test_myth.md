# Retention: `tests/test_myth.py`

**Category:** Merged-graph guard

## What it tests

Deception · Myth as a logic-backed SocialObject.

Retained dynamic tests:

- `test_social_object_is_category` — Retained dynamic test.
- `test_myth_properties_exist` — Retained dynamic test.
- `test_has_myth_telling_domain_range` — Retained dynamic test.
- `test_myth_frame_is_functional` — Retained dynamic test.
- `test_propagates_from_is_derived_from_subproperty` — Retained dynamic test.
- `test_recurring_risk_exists` — Retained dynamic test.
- `test_affected_consumer_surface_exists` — Retained dynamic test.
- `test_myth_el_restriction_on_has_myth_telling` — Myth carries an EL someValuesFrom restriction on hasMythTelling.
- `test_no_truth_axiom_on_myth` — Negative guard: no truth-verdict property may target gmeow:Myth.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
