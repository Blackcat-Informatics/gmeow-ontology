# Retention: `tests/test_events.py`

**Category:** Merged-graph guard

## What it tests

The universal events facility.

Retained dynamic tests:

- `test_former_event_types_are_individuals_not_classes` — The ~30 LifeEvent subclasses became gmeow:eventType VALUE individuals.
- `test_participation_mediation_axiom_present` — Retained dynamic test.
- `test_contested_event_claims_coexist_and_validate` — Two contradictory standpoint-indexed eventType claims (genocide vs armed clash) load, SHACL-pass, and are BOTH retained -- neither is the ground truth.
- `test_schema_role_projection_keys_by_role` — The reified Participation downcasts to the role-keyed flat schema.
- `test_schema_role_projection_suppresses_withdrawn_participation` — A superseded participation (gmeow:displayable false) is NOT projected -- the flat downcast honours suppression-not-erasure.
- `test_schema_fuzzy_time_projects_earliest_bound` — Retained dynamic test.
- `test_ical_vevent_interval_has_start_end_and_location` — A crisp-interval event projects to a VEVENT with DTSTART/DTEND + LOCATION.
- `test_ical_vevent_point_has_start_only` — Retained dynamic test.
- `test_ical_vevent_fuzzy_spans_the_bounds` — A circa-dated event becomes a VEVENT spanning earliestStart->latestEnd.
- `test_ical_summary_is_the_event_type_label` — The open eventType vocabulary collapses to a human-readable SUMMARY label.
- `test_owl_time_projection_emits_pure_interval_relations` — The owl-time profile downcasts each Allen relation to OWL-Time's interval* relation, 1:1 -- and no relation bleeds across (distinct CONSTRUCT variables).
- `test_observational_activity_chain_on_was_associated_with` — The DL-regular property chain generatedObservation o vantage =< wasAssociatedWith is present with the exact ordered sequence.

## Why it cannot be deleted or moved to Rust today

Dynamic sweep: uses g.subjects(RDFS.subClassOf, GM.LifeEvent) over the whole merged graph to catch any GMEOW-prefixed subclass resurrection; narrowing to the events module would silently weaken the regression guard. -- bnode walk: inspects owl:Restriction blank nodes via g.objects() + g.items() to verify someValuesFrom axioms; bnode list structure is not expressible as a simple module-scoped ASK. -- run_shacl() call; ExampleConformance. -- run_shacl() with error-text check. -- multi-file ABox fixture loaded dynamically + run_shacl() + object sweep. -- project_graph() projection check. -- projection. -- projection bound check. -- projection check. -- projection check. -- projection bound check. -- projection label check. -- projection sweep. -- bnode list walk: inspects owl:propertyChainAxiom blank-node lists via g.objects() + g.items() to verify the exact member order of generatedObservation + vantage.
