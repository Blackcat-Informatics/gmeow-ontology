# Retention: `tests/test_reason_verify_chain.py`

**Category:** Oracle / Docker orchestration

## What it tests

Tests for the reason → verify chain, including the pre-reasoned fast path.

## Why it cannot move to Rust today

Drives an external reasoner (HermiT/ELK/ROBOT) or the rdflib trust-anchor — an independent oracle, by design not a Rust twin.

## What is needed to move it to Rust

As the native execution engine subsumes the lanes fragment-by-fragment (oracle-gated) and the classic-cross-check lane is retired, the orchestration loses its subject; delete then.
