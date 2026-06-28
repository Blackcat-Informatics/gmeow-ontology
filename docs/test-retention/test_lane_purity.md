# Retention: `tests/test_lane_purity.py`

**Category:** Static repo guard

## What it tests

Lane-purity seal (#667, Principle 18): required lanes carry no Java/Docker.

## Why it cannot move to Rust today

A static assertion about the repository itself (Python AST / filesystem / workflow / manifest).

## What is needed to move it to Rust

Reimplement as a Rust gate (Python-aware parse) or enforce structurally via crate-layering, then delete this file.
