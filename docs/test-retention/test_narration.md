# Retention: `tests/test_narration.py`

**Category:** Merged-graph guard

## What it tests

The narration seam.

Retained dynamic tests:

- `test_seam_links_specialize_one_ancestor` — Retained dynamic test.
- `test_orientations_are_not_inverse_axioms` — No owl:inverseOf between narrates and narratedIn: EL stays clean and either orientation is usable without entailing the other (the connectsTo convention).
- `test_narration_mode_vocab_seeds` — Retained dynamic test.
- `test_no_truth_bridge_from_unreliable_mode` — narrationUnreliable is a plain vocabulary individual — no axiom links it to the deception module.
- `test_fixture_obeys_the_efficiency_budget` — The chapter-scale fixture demonstrates the doctrine: many flat links, exactly one promoted NarrationUsage (the one with a reason), and the promoted link is not duplicated as a flat quad.
- `test_competency_cooccurrence_query_over_fixture` — The DraCor primitive: co-occurrence pairs reachable through all three seam forms (flat narrates, flat narratedIn, promoted NarrationUsage).

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
