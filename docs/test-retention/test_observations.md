# Retention: `tests/test_observations.py`

**Category:** Cross-slice TBox invariant → slicetest cells

## What it tests

Observation module — retained pytest. Only
`test_kin_relationship_bridges_fire` remains: a cross-slice sub-property check
(the KinRelationship bridges are asserted in the GENEALOGY module) over the
merged graph. The four SOSA `*_mapped_to_*` SSSOM-alignment checks were removed
in the correspondence-frontend migration: the generated-mapping projection is now enforced by the native Rust
dialect lowerings and their byte-iso parity oracles, so the Python `load_mappings`
surface was the redundant dual authority.

## Why it cannot move to Rust today

Cross-slice sub-property invariant: the bridges live in the GENEALOGY module, so
a module-scoped observations slicetest cell cannot see them — ontology *shape*,
not Python logic.

## What is needed to move it to Rust

Author the assertion as a slicetest cell once the genealogy slice gains a
structural migration that covers the cross-module sub-property bridge; confirm
`make slicetest`, then delete this file. No new Rust — the harness exists.
