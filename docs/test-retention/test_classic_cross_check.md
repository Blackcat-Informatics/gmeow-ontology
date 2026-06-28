# Retention: `tests/test_classic_cross_check.py`

**Category:** Oracle / Docker orchestration

## What it tests

Tests for the enforced native↔oracle divergence cross-check (#666, Task 4).

## Why it cannot move to Rust today

Drives an external reasoner (HermiT/ELK/ROBOT) or the rdflib trust-anchor — an independent oracle, by design not a Rust twin.

## What is needed to move it to Rust

As the native execution engine subsumes the lanes fragment-by-fragment (oracle-gated) and the classic-cross-check lane is retired, the orchestration loses its subject; delete then.
