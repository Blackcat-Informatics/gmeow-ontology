# Retention: `tests/test_shapes.py`

**Category:** Static repo guard

## What it tests

Closed-world SHACL data-shape tests.

Retained dynamic tests:

- `test_no_nodeshape_iri_collision_across_shape_files` — Every sh:NodeShape IRI is owned by exactly one shape file.

## Why it cannot be deleted or moved to Rust today

Whole-tooling IRI uniqueness sweep across all shape files; requires Python's filesystem + rdflib graph scan and has no stable mapping to a Rust integration test.
