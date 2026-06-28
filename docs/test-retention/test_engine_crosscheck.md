# Retention: `tests/test_engine_crosscheck.py`

**Category:** Oracle orchestration

## What it tests

The dual-engine query cross-check: the native `gmeow_rdf` store's SPARQL results
are compared against upstream **rdflib** as a trust anchor. rdflib is the
independent oracle that the fast native engine is validated against.

## Why it cannot move to Rust today

The whole point is differential testing against a *different* implementation in a
*different* language (Python's rdflib). A Rust test of the native engine cannot
be its own oracle — that would be circular. The value is precisely that an
independent, externally-maintained engine agrees with ours.

## What is needed to move it to Rust

Either (a) retire the cross-check once the native engine's SPARQL conformance is
independently established by the W3C suites in `crates/sparql-conformance` /
`crates/sparql-eval` (at which point rdflib adds no signal), or (b) wrap a
non-Python reference engine the harness can call from Rust. Until then this is
the sole first-party rdflib consumer and the engine-equivalence guard.
