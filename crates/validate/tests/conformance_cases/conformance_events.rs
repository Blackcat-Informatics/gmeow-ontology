// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_events.py
//!
//! Migrated tests:
//! - `test_wellformed_participation_conforms` → `wellformed_participation_conforms`
//! - `test_malformed_participation_is_flagged` → `malformed_participation_is_flagged`
//! - `test_former_event_types_are_individuals_not_classes` → `former_event_types_are_individuals_not_classes`
//! - `test_participation_mediation_axiom_present` → `participation_mediation_axiom_present`
//! - `test_contested_event_claims_coexist_and_validate` → `contested_event_claims_coexist_and_validate`
//! - `test_observational_activity_chain_on_was_associated_with` → `observational_activity_chain_on_was_associated_with`
//!
//! The eight `project_graph()` projection-output tests (`schema-org` role keying +
//! withdrawn-suppression + fuzzy-earliest-bound, `ical` VEVENT
//! interval/point/fuzzy/summary, `owl-time` interval relations) are reinstated
//! natively below via `events_projected` — the committed profile CONSTRUCT `.rq`
//! run in-process (`GraphStore::construct`) over `ontology + events.ttl`, mirroring
//! the Python `_events_projected(profile)`:
//! - `test_schema_role_projection_keys_by_role` → `schema_role_projection_keys_by_role`
//! - `test_schema_role_projection_suppresses_withdrawn_participation` → `schema_role_projection_suppresses_withdrawn_participation`
//! - `test_schema_fuzzy_time_projects_earliest_bound` → `schema_fuzzy_time_projects_earliest_bound`
//! - `test_ical_vevent_interval_has_start_end_and_location` → `ical_vevent_interval_has_start_end_and_location`
//! - `test_ical_vevent_point_has_start_only` → `ical_vevent_point_has_start_only`
//! - `test_ical_vevent_fuzzy_spans_the_bounds` → `ical_vevent_fuzzy_spans_the_bounds`
//! - `test_ical_summary_is_the_event_type_label` → `ical_summary_is_the_event_type_label`
//! - `test_owl_time_projection_emits_pure_interval_relations` → `owl_time_projection_emits_pure_interval_relations`

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use purrdf::slice::rdf_query::{Object, Subject};
use std::collections::BTreeSet;

// ── IRI constants ────────────────────────────────────────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_EVENTS: &str = "https://blackcatinformatics.ca/gmeow/examples/events/";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

// Projection-target namespaces (the profile CONSTRUCT `.rq` outputs).
const SCHEMA: &str = "https://schema.org/";
const ICAL: &str = "http://www.w3.org/2002/12/cal/icaltzd#";
const TIME: &str = "http://www.w3.org/2006/time#";

// Committed profile CONSTRUCT queries (matches the `MAILMAP_RQ_REL` convention).
const SCHEMA_ORG_RQ_REL: &str = "generated/queries/schema-org.rq";
const ICAL_RQ_REL: &str = "generated/queries/ical.rq";
const OWL_TIME_RQ_REL: &str = "generated/queries/owl-time.rq";

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn ex_events(local: &str) -> String {
    format!("{EX_EVENTS}{local}")
}

/// The former genealogy event subclasses — must NOT exist as classes again.
const FORMER_EVENT_SUBCLASSES: &[&str] = &[
    "Birth",
    "Death",
    "Burial",
    "Marriage",
    "Divorce",
    "Adoption",
    "Christening",
    "NameChange",
    "Census",
    "Immigration",
];

// ── SHACL Case twins ─────────────────────────────────────────────────────────

#[batch_cases]
#[case::wellformed_participation_conforms(Case::file("shapes", "participation-wellformed"))]
// The `participation-malformed` fixture (Participation without event / participant)
// must fail SHACL with a violation mentioning one of participationEvent /
// participationParticipant (case-sensitive disjunction → `any_violation`).
#[case::malformed_participation_is_flagged(
    Case::file("shapes", "participation-malformed")
        .fails()
        .any_violation(&["participationEvent", "participationParticipant"])
)]
fn events(#[case] case: Case) {
    case.run();
}

// ── `_graph()`-based TBox + bnode twins ──────────────────────────────────────

/// Twin of `test_former_event_types_are_individuals_not_classes`.
///
/// The anti-subclass regression guard: each former LifeEvent subclass IRI is gone
/// as a class/subclass, its `eventType…` replacement is an `EventType` value (not a
/// class), and — the permanent structural lock — `gmeow:LifeEvent` has ZERO
/// GMEOW-prefixed subclasses (a dynamic subject sweep over the whole merged graph,
/// catching any accidental re-introduction, not just the known list).
#[gmeow_test_batch_macros::batch_test]
fn former_event_types_are_individuals_not_classes() {
    let g = GraphStore::ontology();
    for local in FORMER_EVENT_SUBCLASSES {
        let old = gmeow(local);
        assert!(
            !g.has(Some(&old), Some(RDF_TYPE), Some(OWL_CLASS)),
            "{local} must not be a class"
        );
        assert!(!g.has(
            Some(&old),
            Some(RDFS_SUBCLASS_OF),
            Some(&gmeow("LifeEvent"))
        ));
        assert!(!g.has(Some(&old), Some(RDFS_SUBCLASS_OF), Some(&gmeow("Event"))));
        let value = gmeow(&format!("eventType{local}"));
        assert!(
            g.has(Some(&value), Some(RDF_TYPE), Some(&gmeow("EventType"))),
            "eventType{local} must be a value"
        );
        assert!(!g.has(Some(&value), Some(RDF_TYPE), Some(OWL_CLASS)));
    }
    // Structural lock: LifeEvent must have ZERO GMEOW subclasses.
    let sub: Vec<String> = g
        .subjects(RDFS_SUBCLASS_OF, &gmeow("LifeEvent"))
        .into_iter()
        .filter(|s| s.starts_with(GMEOW))
        .collect();
    assert!(
        sub.is_empty(),
        "gmeow:LifeEvent must have no subclasses; found {sub:?}"
    );
}

/// Twin of `test_participation_mediation_axiom_present`.
///
/// Walks the blank `owl:Restriction` nodes that are `rdfs:subClassOf` of
/// `gmeow:Participation`: for each restriction with a named `owl:onProperty` and a
/// present `owl:someValuesFrom` filler, records the mediated property, and pins the
/// two relator ends (`participationEvent ∃ Event`, `participationParticipant ∃
/// Entity`). Mirrors the Python `g.objects()` + `g.value()` bnode walk.
#[gmeow_test_batch_macros::batch_test]
fn participation_mediation_axiom_present() {
    let g = GraphStore::ontology();
    let participation = Subject::Named(gmeow("Participation"));
    let mut mediated: Vec<String> = Vec::new();
    for obj in g.objects_h(&participation, RDFS_SUBCLASS_OF) {
        let Some(restriction) = GraphStore::object_as_subject(&obj) else {
            continue;
        };
        let some_values = g.objects_h(&restriction, OWL_SOME_VALUES_FROM);
        if some_values.is_empty() {
            continue;
        }
        for on in g.objects_h(&restriction, OWL_ON_PROPERTY) {
            let Object::Named(on_iri) = on else {
                continue;
            };
            mediated.push(on_iri.clone());
            if on_iri == gmeow("participationEvent") {
                assert!(
                    some_values
                        .iter()
                        .any(|o| matches!(o, Object::Named(i) if *i == gmeow("Event"))),
                    "participationEvent must be mediated by ∃ someValuesFrom Event"
                );
            }
            if on_iri == gmeow("participationParticipant") {
                assert!(
                    some_values
                        .iter()
                        .any(|o| matches!(o, Object::Named(i) if *i == gmeow("Entity"))),
                    "participationParticipant must be mediated by ∃ someValuesFrom Entity"
                );
            }
        }
    }
    assert!(
        mediated.contains(&gmeow("participationEvent")),
        "participationEvent must be mediated; got {mediated:?}"
    );
    assert!(
        mediated.contains(&gmeow("participationParticipant")),
        "participationParticipant must be mediated; got {mediated:?}"
    );
}

/// Twin of `test_contested_event_claims_coexist_and_validate`.
///
/// Two contradictory standpoint-indexed `eventType` claims (genocide vs armed
/// clash) load, SHACL-pass (fixture-only, mirroring the Python `run_shacl(g)` on
/// the parsed fixture graph), and are BOTH retained — neither is ground truth. A
/// contested date likewise coexists as two instant literals. The literal `eventTime`
/// objects are read with the bnode/literal-aware `objects_h` (the IRI-only
/// `objects` would drop literal objects).
#[gmeow_test_batch_macros::batch_test]
fn contested_event_claims_coexist_and_validate() {
    let report = validate(&fixture_as_nt("coverage", "events-contested"));
    assert!(
        ok(&report),
        "contested-events fixture must SHACL-pass; violations: {:?}",
        violations(&report)
    );

    let path = repo_root().join("tests/fixtures/coverage/events-contested.ttl");
    let g = GraphStore::parse_ttl_file(&path);
    let disputed = Subject::Named(ex_events("disputedEvent"));

    let types = g.objects_h(&disputed, &gmeow("eventType"));
    for want in ["eventTypeGenocide", "eventTypeArmedClash"] {
        let iri = ex_events(want);
        assert!(
            types
                .iter()
                .any(|o| matches!(o, Object::Named(i) if *i == iri)),
            "contested eventType {want} must coexist; got {types:?}"
        );
    }

    let dates = g.objects_h(&disputed, &gmeow("eventTime"));
    assert_eq!(
        dates.len(),
        2,
        "two contested eventTime instants must coexist; got {dates:?}"
    );
}

/// Twin of `test_observational_activity_chain_on_was_associated_with`.
///
/// Walks the blank `owl:propertyChainAxiom` `rdf:List` on `gmeow:wasAssociatedWith`
/// and asserts one chain is exactly `generatedObservation ∘ vantage`, in that order.
/// Mirrors the Python `g.objects()` + `g.items()` bnode-list walk.
#[gmeow_test_batch_macros::batch_test]
fn observational_activity_chain_on_was_associated_with() {
    let g = GraphStore::ontology();
    let subject = Subject::Named(gmeow("wasAssociatedWith"));
    let chains = g.objects_h(&subject, OWL_PROPERTY_CHAIN_AXIOM);
    assert!(
        !chains.is_empty(),
        "wasAssociatedWith must have at least one property chain axiom"
    );
    let found = chains
        .iter()
        .filter_map(GraphStore::object_as_subject)
        .any(|head| {
            let members = g.rdf_list_h(&head);
            members.len() == 2
                && members[0] == Object::Named(gmeow("generatedObservation"))
                && members[1] == Object::Named(gmeow("vantage"))
        });
    assert!(
        found,
        "wasAssociatedWith must have a chain containing \
         generatedObservation o vantage in that order"
    );
}

// ── project_graph() projection-output twins (native CONSTRUCT) ────────────────
//
// The native twin of the Python `_events_projected(profile)`: load the merged
// ontology plus the events coverage fixture into one store and run the profile's
// committed CONSTRUCT `.rq` in-process. The `ask`/`select` helpers assert over the
// projected graph (they handle LITERAL objects, which the IRI-only `has`/`objects`
// would drop). Profile → committed query:
// - "schema-org" → generated/queries/schema-org.rq
// - "ical"       → generated/queries/ical.rq
// - "owl-time"   → generated/queries/owl-time.rq

/// Project `ontology + tests/fixtures/coverage/events.ttl` through the committed
/// profile CONSTRUCT at `query_rel`. Native twin of `_events_projected(profile)`.
fn events_projected(query_rel: &str) -> GraphStore {
    let g =
        GraphStore::ontology_plus_ttl_file(&repo_root().join("tests/fixtures/coverage/events.ttl"));
    let q = std::fs::read_to_string(repo_root().join(query_rel)).expect("projection query");
    g.construct(&[], &q)
}

/// Twin of `test_schema_role_projection_keys_by_role`.
///
/// The reified Participation downcasts to role-keyed flat schema.org edges — each
/// role lands on its OWN predicate (organizer/performer/attendee), and roles do not
/// bleed across predicates (the organizer casey is not also an attendee).
#[gmeow_test_batch_macros::batch_test]
fn schema_role_projection_keys_by_role() {
    let out = events_projected(SCHEMA_ORG_RQ_REL);
    let reception = ex_events("reception");
    let casey = ex_events("casey");
    let band = ex_events("band");
    let dana = ex_events("dana");
    assert!(
        out.ask(
            &[],
            &format!("ASK {{ <{reception}> <{SCHEMA}organizer> <{casey}> }}")
        ),
        "reception must have schema:organizer casey"
    );
    assert!(
        out.ask(
            &[],
            &format!("ASK {{ <{reception}> <{SCHEMA}performer> <{band}> }}")
        ),
        "reception must have schema:performer band"
    );
    assert!(
        out.ask(
            &[],
            &format!("ASK {{ <{reception}> <{SCHEMA}attendee> <{dana}> }}")
        ),
        "reception must have schema:attendee dana"
    );
    assert!(
        !out.ask(
            &[],
            &format!("ASK {{ <{reception}> <{SCHEMA}attendee> <{casey}> }}")
        ),
        "roles must not bleed: organizer casey is not also an attendee"
    );
}

/// Twin of `test_schema_role_projection_suppresses_withdrawn_participation`.
///
/// A superseded participation (`gmeow:displayable false`) is NOT projected — the
/// flat downcast honours suppression-not-erasure (Principle 10). ex:erin's attendee
/// participation is displayable false, so it must be dropped.
#[gmeow_test_batch_macros::batch_test]
fn schema_role_projection_suppresses_withdrawn_participation() {
    let out = events_projected(SCHEMA_ORG_RQ_REL);
    let reception = ex_events("reception");
    let erin = ex_events("erin");
    assert!(
        !out.ask(
            &[],
            &format!("ASK {{ <{reception}> <{SCHEMA}attendee> <{erin}> }}")
        ),
        "erin's withdrawn (displayable false) participation must be suppressed"
    );
}

/// Twin of `test_schema_fuzzy_time_projects_earliest_bound`.
///
/// A circa-dated event projects its earliest bound as schema:startDate — some
/// ex:siege schema:startDate literal must start "1453-04-01".
#[gmeow_test_batch_macros::batch_test]
fn schema_fuzzy_time_projects_earliest_bound() {
    let out = events_projected(SCHEMA_ORG_RQ_REL);
    let siege = ex_events("siege");
    assert!(
        out.ask(&[], &format!(
            "ASK {{ <{siege}> <{SCHEMA}startDate> ?s FILTER(STRSTARTS(STR(?s), \"1453-04-01\")) }}"
        )),
        "siege schema:startDate must project the earliest bound (1453-04-01)"
    );
}

/// Twin of `test_ical_vevent_interval_has_start_end_and_location`.
///
/// A crisp-interval event projects to a VEVENT with DTSTART/DTEND + LOCATION:
/// ex:wedding a ical:Vevent, has ical:dtstart, has ical:dtend, ical:location chapel.
#[gmeow_test_batch_macros::batch_test]
fn ical_vevent_interval_has_start_end_and_location() {
    let out = events_projected(ICAL_RQ_REL);
    let wedding = ex_events("wedding");
    let chapel = ex_events("chapel");
    assert!(
        out.ask(&[], &format!("ASK {{ <{wedding}> a <{ICAL}Vevent> }}")),
        "wedding must be a ical:Vevent"
    );
    assert!(
        out.ask(&[], &format!("ASK {{ <{wedding}> <{ICAL}dtstart> ?o }}")),
        "wedding VEVENT must have ical:dtstart"
    );
    assert!(
        out.ask(&[], &format!("ASK {{ <{wedding}> <{ICAL}dtend> ?o }}")),
        "wedding VEVENT must have ical:dtend"
    );
    assert!(
        out.ask(
            &[],
            &format!("ASK {{ <{wedding}> <{ICAL}location> <{chapel}> }}")
        ),
        "wedding VEVENT must have ical:location chapel"
    );
}

/// Twin of `test_ical_vevent_point_has_start_only`.
///
/// A point-in-time event projects to a VEVENT with a DTSTART but NO DTEND:
/// ex:reception a ical:Vevent, has ical:dtstart, and has no ical:dtend.
#[gmeow_test_batch_macros::batch_test]
fn ical_vevent_point_has_start_only() {
    let out = events_projected(ICAL_RQ_REL);
    let reception = ex_events("reception");
    assert!(
        out.ask(&[], &format!("ASK {{ <{reception}> a <{ICAL}Vevent> }}")),
        "reception must be a ical:Vevent"
    );
    assert!(
        out.ask(&[], &format!("ASK {{ <{reception}> <{ICAL}dtstart> ?o }}")),
        "reception VEVENT must have ical:dtstart"
    );
    assert!(
        !out.ask(&[], &format!("ASK {{ <{reception}> <{ICAL}dtend> ?o }}")),
        "a point event must have NO ical:dtend"
    );
}

/// Twin of `test_ical_vevent_fuzzy_spans_the_bounds`.
///
/// A circa-dated event becomes a VEVENT spanning earliestStart→latestEnd:
/// ex:siege ical:dtstart starts "1453-04-01" and ical:dtend starts "1453-05-31".
#[gmeow_test_batch_macros::batch_test]
fn ical_vevent_fuzzy_spans_the_bounds() {
    let out = events_projected(ICAL_RQ_REL);
    let siege = ex_events("siege");
    assert!(
        out.ask(
            &[],
            &format!(
                "ASK {{ <{siege}> <{ICAL}dtstart> ?o FILTER(STRSTARTS(STR(?o), \"1453-04-01\")) }}"
            )
        ),
        "siege ical:dtstart must span from the earliest bound (1453-04-01)"
    );
    assert!(
        out.ask(
            &[],
            &format!(
                "ASK {{ <{siege}> <{ICAL}dtend> ?o FILTER(STRSTARTS(STR(?o), \"1453-05-31\")) }}"
            )
        ),
        "siege ical:dtend must span to the latest bound (1453-05-31)"
    );
}

/// Twin of `test_ical_summary_is_the_event_type_label`.
///
/// The open eventType vocabulary collapses to a human-readable SUMMARY label —
/// ex:wedding ical:summary literal "marriage".
#[gmeow_test_batch_macros::batch_test]
fn ical_summary_is_the_event_type_label() {
    let out = events_projected(ICAL_RQ_REL);
    let wedding = ex_events("wedding");
    // The summary is language-tagged (STRLANG in the projection); the Python twin
    // compared `str(o)`, which drops the tag — mirror that with STR(?o).
    assert!(
        out.ask(
            &[],
            &format!("ASK {{ <{wedding}> <{ICAL}summary> ?o FILTER(STR(?o) = \"marriage\") }}")
        ),
        "wedding ical:summary must be the eventType label \"marriage\""
    );
}

/// Twin of `test_owl_time_projection_emits_pure_interval_relations`.
///
/// The owl-time profile downcasts each Allen relation 1:1 to an OWL-Time interval*
/// relation, and no relation bleeds across (distinct CONSTRUCT variables):
/// ex:dawn time:intervalBefore ex:noon; ex:conference time:intervalContains
/// ex:keynote; and the predicate set from dawn→noon is EXACTLY {intervalBefore}.
#[gmeow_test_batch_macros::batch_test]
fn owl_time_projection_emits_pure_interval_relations() {
    let out = events_projected(OWL_TIME_RQ_REL);
    let dawn = ex_events("dawn");
    let noon = ex_events("noon");
    let conference = ex_events("conference");
    let keynote = ex_events("keynote");
    assert!(
        out.ask(
            &[],
            &format!("ASK {{ <{dawn}> <{TIME}intervalBefore> <{noon}> }}")
        ),
        "dawn must be time:intervalBefore noon"
    );
    assert!(
        out.ask(
            &[],
            &format!("ASK {{ <{conference}> <{TIME}intervalContains> <{keynote}> }}")
        ),
        "conference must be time:intervalContains keynote"
    );
    // dawn→noon carries ONLY intervalBefore (no var aliasing across relations).
    let (_, rows) = out.select(&[], &format!("SELECT ?p WHERE {{ <{dawn}> ?p <{noon}> }}"));
    let preds: BTreeSet<String> = rows
        .into_iter()
        .filter_map(|row| match row.into_iter().next() {
            Some(Some(purrdf::TermValue::Iri(iri))) => Some(iri),
            _ => None,
        })
        .collect();
    let expected: BTreeSet<String> = [format!("{TIME}intervalBefore")].into_iter().collect();
    assert_eq!(
        preds, expected,
        "dawn→noon predicates must be exactly {{time:intervalBefore}}; got {preds:?}"
    );
}
