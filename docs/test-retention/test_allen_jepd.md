# Retention: `tests/test_allen_jepd.py`

**Category:** Merged-graph guard

## What it tests

Tests for Allen interval relations and JEPD disjointness.

Retained dynamic tests:

- `test_no_owl_all_disjoint_properties_over_interval_relations` — OWL 2 DL forbids DisjointObjectProperties over non-simple (transitive) properties.

## Why it cannot be deleted or moved to Rust today

A whole-graph sweep over every owl:AllDisjointProperties to ensure no interval-level Allen relation is grouped into an OWL disjoint-properties axiom. Not expressible as a finite module-scoped SPARQL ASK.
