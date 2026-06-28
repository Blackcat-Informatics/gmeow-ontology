# Retention: `tests/test_up_projection*.py`

Covers `test_up_projection.py`, `test_up_projection_descend.py`,
`test_up_projection_audit.py`.

**Category:** Python tool algorithm

## What it tests

The up-projection engine (consumer RDF → pure GMEOW): lift-map construction,
identity-vs-projection disambiguation, fact-vs-claim minting with provenance,
inverse-path handling, blank-node skipping, language-tag retagging, round-trip
recovery, and the SSSOM → up-projection invertibility audit. These assert the
*computed output* of the Python algorithm, not ontology structure.

## Why it cannot move to Rust today

The up-projection engine is a live Python implementation
(`gmeow_tools.up_projection*`). The tests assert its algorithmic output
(graph-shaped results, ambiguity decisions, provenance stamping). A slicetest
cell cannot express "given input graph X, the engine produces output graph Y";
and there is no Rust port of the engine whose tests could subsume these.

## What is needed to move it to Rust

Up-projection is not ported as a standalone heuristic — it is subsumed by the
Correspondence Calculus (`docs/APPLIED_CATEGORY_THEORY/take1.md`). Under that
design the external→GMEOW direction is the `put` leg of a `logic:Correspondence`,
**derived** (not hand-authored) for mnemomorphic cells: `put` is the projection
along the retained source-witness, law-bearing by construction. The current
`crates/pipeline/src/up_projection.rs` heuristic is then deleted under the
equivalence-before-deletion migration (the lift map becomes correspondence legs;
the "~81% liftable" audit becomes a derived loss-ledger statistic). These pytest
files lose their subject when the lowering engine + the `conformance/correspondence`
round-trip/mnemomorphism gates regenerate the same outputs and are deleted in that
change — not by a per-file Rust port.
