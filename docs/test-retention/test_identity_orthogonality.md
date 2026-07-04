# Retention: `tests/test_identity_orthogonality.py`

**Category:** Merged-graph guard

## What it tests

The centrepiece ethical invariant: the identity axes are ORTHOGONAL.

Retained dynamic tests:

- `test_annotation_set_covers_the_historical_seven_axes` — Every historical axis carries gmeow:coequalFacet true.
- `test_coequal_facet_lint_holds_on_the_real_matrix` — The annotation-driven lint (the live enforcement) is clean.
- `test_coequal_facet_lint_catches_seeded_violations` — Each violation class is detected when seeded into a copy of the graph.
- `test_every_axis_property_exists_with_its_own_range` — Retained dynamic test.
- `test_no_axis_is_inferred_from_another` — For every ordered pair, no subProperty/equivalence bridge in either direction.
- `test_identity_axes_are_disjoint_classes_axiom` — The matrix is now also an OWL theorem , not only a Python guard.
- `test_no_preferred_or_primary_identity_term` — Co-equality across identity axes: no preferred/primary marker anywhere.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
