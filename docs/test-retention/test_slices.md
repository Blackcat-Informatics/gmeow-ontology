# Retention: `tests/test_slices.py`

**Category:** Static repo guard

## What it tests

Slice discovery + manifest loading (Principles 15-16; issue #287).

## Why it cannot move to Rust today

A static assertion about the repository itself (Python AST / filesystem / workflow / manifest).

## What is needed to move it to Rust

Reimplement as a Rust gate (Python-aware parse) or enforce structurally via crate-layering, then delete this file.
