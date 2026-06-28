# Retention: `tests/test_notation.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Retained cross-slice and dynamic guards for the notation and symbolic

## Why it cannot move to Rust today

Structural / competency / cross-slice invariants over the module (or merged) graph — ontology *shape*, not Python logic.

## What is needed to move it to Rust

Author the assertions as slicetest cells in the **owning** slice (`structural.ttl` MUST/MUST-NOT, `competency.ttl` ASK/SELECT) per `docs/SLICE_QA.md`; cross-slice subjects go in the slice that *defines* the term. Confirm `make slicetest`, then delete this file. No new Rust — the harness exists.
