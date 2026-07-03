# Retention: `tests/test_agentic.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The agentic extension: tool-call provenance, gated.

Retained dynamic tests:

- `test_memory_records_and_reads_tool_calls`
- `test_memory_applies_the_verbatim_or_digest_doctrine`

## Why it cannot move to Rust today

Dynamic assertions that do not map cleanly to module-scoped sparql ask/select cells.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
