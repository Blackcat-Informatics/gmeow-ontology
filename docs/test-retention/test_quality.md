# Retention: `tests/test_quality.py`

**Category:** Merged-graph guard

## What it tests

Data-quality layer — whole-ontology Principle-9 sweep.

Retained dynamic tests:

- `test_no_preferred_or_primary_term_is_declared` — No GMEOW vocabulary term is a preferred/primary selector.

## Why it cannot be deleted or moved to Rust today

What remains is the **whole-ontology** dynamic sweep below: it iterates the entire merged graph's subject set, so it is NOT a quality-module-scoped assertion and a module-scoped slicetest cell would silently narrow it. It is retained here as a dynamic-set sweep.
