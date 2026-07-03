# Retention: `tests/test_events.py`

**Category:** Domain invariant → slicetest cells

## What it tests

The universal events facility (#41).

Retained dynamic tests:

- `test_former_event_types_are_individuals_not_classes` — The ~30 LifeEvent subclasses became gmeow:eventType VALUE individuals.
- `test_participation_mediation_axiom_present`
- `test_contested_event_claims_coexist_and_validate` — Two contradictory standpoint-indexed eventType claims (genocide vs armed clash) load, SHACL-pass, and are BOTH retained -- neither is the ground truth.
- `test_schema_role_projection_keys_by_role` — The reified Participation downcasts to the role-keyed flat schema.
- `test_schema_role_projection_suppresses_withdrawn_participation` — A superseded participation (gmeow:displayable false) is NOT projected -- the flat downcast honours suppression-not-erasure (Principle 10).
- `test_schema_fuzzy_time_projects_earliest_bound`
- `test_ical_vevent_interval_has_start_end_and_location` — A crisp-interval event projects to a VEVENT with DTSTART/DTEND + LOCATION.
- `test_ical_vevent_point_has_start_only`
- `test_ical_vevent_fuzzy_spans_the_bounds` — A circa-dated event becomes a VEVENT spanning earliestStart->latestEnd.
- `test_ical_summary_is_the_event_type_label` — The open eventType vocabulary collapses to a human-readable SUMMARY label.
- `test_owl_time_projection_emits_pure_interval_relations` — The owl-time profile downcasts each Allen relation to OWL-Time's interval* relation, 1:1 -- and no relation bleeds across (distinct CONSTRUCT variables).
- `test_observational_activity_chain_on_was_associated_with` — The DL-regular property chain generatedObservation o vantage =< wasAssociatedWith is present with the exact ordered sequence.

## Why it cannot move to Rust today

Whole-merged-graph sweeps that cannot be scoped to a single slice module; consumer-projection round-trips exercised through the python projection harness; shacl conformance calls against abox fixture data; abox fixture instance checks.

## What is needed to move it to Rust

Move each retained assertion to a Rust home: extend the slicetest DSL with merged-graph scopes, cover projections/SHACL/mappings with Rust crate tests, or retire the Python surface once equivalent coverage exists. When every retained test above has a Rust home, delete this file and its dossier.
