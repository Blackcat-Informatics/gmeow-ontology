# Retention: `tests/test_narrative_time.py`

**Category:** Merged-graph guard

## What it tests

Narrative time frames.

Retained dynamic tests:

- `test_frame_properties_are_functional_with_correct_anchors` — Retained dynamic test.
- `test_at_narrative_position_is_domain_free_and_not_functional` — The one anchor reused by the seam , arcs , motifs.
- `test_flashback_fixture_carries_coexisting_orders` — Retained dynamic test.
- `test_competency_narrative_time_axes_query` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
