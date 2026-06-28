# Retention: `tests/test_transform.py`

**Category:** Projection / alignment → Correspondence Calculus

## What it tests

The transpiler driver (#34 Phase 1): MAXIMAL(G) = G + E(G) + P(G).

## Why it cannot move to Rust today

Exercises the FnO/EDOAL/SPARQL projection or up-projection (alignment) layer — live Python engine output.

## What is needed to move it to Rust

Subsumed by the Correspondence Calculus (`docs/APPLIED_CATEGORY_THEORY/take1.md`): projections become lowerings of one `logic:Correspondence` get/put leg pair; up-projection is the derived `put` leg. When the lowering engine + `conformance/correspondence` round-trip/overclaim gates regenerate these outputs byte/graph-iso, the file is deleted under equivalence-before-deletion.
