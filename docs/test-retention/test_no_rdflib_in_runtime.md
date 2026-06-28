# Retention: `tests/test_no_rdflib_in_runtime.py`

**Category:** Static repo guard

## What it tests

The purrdf P0 self-host gate: gmeow's own code must not import ``rdflib``.

## Why it cannot move to Rust today

A static assertion about the repository itself (Python AST / filesystem / workflow / manifest).

## What is needed to move it to Rust

Reimplement as a Rust gate (Python-aware parse) or enforce structurally via crate-layering, then delete this file.
