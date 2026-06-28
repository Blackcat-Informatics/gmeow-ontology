# Retention: `tests/test_saturate.py`

**Category:** Python tool algorithm

## What it tests

The saturation engine (`gmeow_tools.saturate`): strong-only equivalence classes,
lint-gated denial, suppression-safe expansion, provenance annotation on minted
triples, and deterministic output.

## Why it cannot move to Rust today

`saturate()` is a live **Python** algorithm; the tests assert its computed graph
output and determinism. No Rust crate implements the saturation cell evaluator,
so there is nothing to subsume these.

## What is needed to move it to Rust

Port the saturation engine to a Rust crate with crate tests over the same
equivalence-class / suppression-safety / determinism scenarios, then delete this
file and its dossier.
