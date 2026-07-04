# Retention: `tests/test_interior.py`

**Category:** Merged-graph guard

## What it tests

The interior parcel: affect + arcs/roles/motifs.

Retained dynamic tests:

- `test_plutchik_seeds_are_present_and_open` — Retained dynamic test.
- `test_appraisal_is_a_vantage_indexed_observation` — Retained dynamic test.
- `test_no_emotion_tenure_class_exists` — Thin means thin: episodic scope rides validFrom/validUntil; a tenure class arrives only on consumer demand (docs record the bar).
- `test_arc_sample_constituents` — Retained dynamic test.
- `test_character_arc_extension_is_additive` — Retained dynamic test.
- `test_no_primary_protagonist_machinery` — Retained dynamic test.
- `test_motif_rides_the_seam` — Retained dynamic test.
- `test_trajectory_query_orders_and_surfaces_disagreement` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
