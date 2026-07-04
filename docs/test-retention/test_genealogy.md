# Retention: `tests/test_genealogy.py`

**Category:** Merged-graph guard

## What it tests

Standpoint + dynamic guards for the genealogy module.

Retained dynamic tests:

- `test_former_event_subclasses_are_not_reintroduced` — The ~30 LifeEvent subclasses became gmeow:eventType value individuals in the events module; genealogy must not re-introduce them as classes.
- `test_contested_parentage_coexists` — Two contradictory standpoint-indexed hasParent claims load, SHACL-pass, and are BOTH retained — neither is the ground truth.
- `test_contested_birth_date_coexists` — Two standpoint-indexed eventTime claims on the same LifeEvent coexist.
- `test_withdrawn_parentage_suppressed_not_deleted` — A refuted / withdrawn parentage claim is retained with displayable false.
- `test_no_preferred_or_primary_genealogy_term` — Principle 9: no single slot to win — genealogy mints no preferred/primary selector for a contested parent, kinship, or event.

## Why it cannot be deleted or moved to Rust today

Asserted-TBox invariants (Family/KinRelationship/ParentChildRelationship grounding and KinRelationship bridges) were migrated to slices/extensions/genealogy/tests/structural.ttl . Only the dynamic guards that cannot be expressed as module-scoped ASK cells are retained here: - whole-ontology merged-graph sweeps (_graph()), - run_shacl() ExampleConformance + ABox fixture checks.
