# Retention: `tests/test_config.py`

**Category:** Static repo guard

## What it tests

Tests for the license-aware link policy (the core safety mechanism).

## Why it cannot move to Rust today

A static assertion about the repository itself (Python AST / filesystem / workflow / manifest).

## What is needed to move it to Rust

Reimplement as a Rust gate (Python-aware parse) or enforce structurally via crate-layering, then delete this file.
