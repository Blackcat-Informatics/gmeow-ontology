# Retention: `tests/test_vocabulary_surface.py`

**Category:** Static repo guard

## What it tests

Vocabulary-surface integrity gates (issue #199).

## Why it cannot move to Rust today

A static assertion about the repository itself (Python AST / filesystem / workflow / manifest).

## What is needed to move it to Rust

Reimplement as a Rust gate (Python-aware parse) or enforce structurally via crate-layering, then delete this file.
