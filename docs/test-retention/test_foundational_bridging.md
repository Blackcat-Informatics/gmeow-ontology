# Retention: `tests/test_foundational_bridging.py`

**Category:** Merged-graph guard

## What it tests

Tests for the gUFO↔BFO foundational-spine bridge.

Retained dynamic tests:

- `test_expected_cells_present_in_alignment_graph` — Retained dynamic test.
- `test_bridge_uses_closematch_only` — Retained dynamic test.
- `test_every_bfo_iri_is_a_real_class_in_the_snapshot` — Principle 7: each emitted BFO IRI is a declared owl:Class with the stated label, verified offline against the vendored snapshot.
- `test_bridge_is_link_only_no_import` — No BFO class enters the reasoned import closure — the bridge is by reference.
- `test_bfo_is_import_ok_upper_ontology` — Retained dynamic test.
- `test_coverage_reported` — Retained dynamic test.
- `test_vendored_snapshot_matches_live_bfo` — The offline snapshot must not silently rot: every BFO IRI we reference still exists, as a class, with the same label, in the live ontology.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
