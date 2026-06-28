# Retention: `tests/test_slice_fix_deps.py`

**Category:** PyO3 seam

## What it tests

End-to-end test for `gmeow-dev slice-fix-deps` over the native binding (#820 S8).

## Why it cannot move to Rust today

Tests a PyO3 binding's marshalling / error-surfacing — the seam itself, which Rust cannot test from the inside; the engine substance is already Rust-tested.

## What is needed to move it to Rust

Delete when the Python surface that owns the seam is removed (the binding drops once nothing Python imports it); the engine is covered by its Rust crate.
