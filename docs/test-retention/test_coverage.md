# Retention: `tests/test_coverage.py`

**Category:** Python tool algorithm

## What it tests

The surface-vocabulary alignment-coverage audit harness (`gmeow_tools.coverage`:
`run_coverage`, `to_diagnostics_report`) — that key entity kinds and per-slice
terms are covered/gap/ignored as expected, and the diagnostics report stays `ok`
with info-level gaps.

## Why it cannot move to Rust today

The classification logic is mirrored in `crates/validate/src/coverage.rs`, but
`run_coverage` (the audit that walks the merged surface and produces the report)
is still driven from **Python** (`gmeow-dev` consumes it). These tests assert the
Python harness's report over the real surface, not the unit classifier.

## What is needed to move it to Rust

Port the coverage audit harness (surface walk + report assembly) to the
`crates/validate` coverage module with a crate test over the real bundle, then
delete this file and its dossier.
