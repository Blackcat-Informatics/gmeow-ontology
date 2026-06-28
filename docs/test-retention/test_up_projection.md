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

Port the up-projection engine to a Rust crate (lift-map builder + projection
executor), then cover it with crate-level golden tests over the same fixtures
and delete these files. This is part of the Python-cull (Python = UI-only)
program; the engine is one of the larger Python algorithms still outstanding.
