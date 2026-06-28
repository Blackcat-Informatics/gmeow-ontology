# Retention: `tests/test_software.py`

**Category:** Domain invariant → slicetest cells

## What it tests

Behavioural guards for the software module (#231 Phase A, #232 Phase B).

## Why it cannot move to Rust today

Structural / competency / cross-slice invariants over the module (or merged) graph — ontology *shape*, not Python logic.

## What is needed to move it to Rust

Author the assertions as slicetest cells in the **owning** slice (`structural.ttl` MUST/MUST-NOT, `competency.ttl` ASK/SELECT) per `docs/SLICE_QA.md`; cross-slice subjects go in the slice that *defines* the term. Confirm `make slicetest`, then delete this file. No new Rust — the harness exists.
