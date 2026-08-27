// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_calendar.py
//!
//! Migrated tests:
//! - `test_calendar_temporal_datatypes_are_datetime_or_duration` → `calendar_temporal_datatypes_are_datetime_or_duration`
//! - `test_calendar_axes_are_independent` → `calendar_axes_are_independent`
//!
//! The datatypes test walks a blank-node union range: `gmeow:reminderTrigger`
//! `rdfs:range` points at a blank `rdfs:Datatype` node whose `owl:unionOf` is a
//! blank `rdf:List` head, so the walk goes named `reminderTrigger` → blank range
//! node → `owl:unionOf` head (blank) → `rdf:List` members, mirroring the Python
//! `g.value(range_node, OWL.unionOf)` + `g.items(union_head)` walk. The axes test
//! sweeps every unordered pair of ten orthogonal properties asserting no
//! `rdfs:subPropertyOf` / `owl:equivalentProperty` bridge (all IRI triples).

use crate::conformance_support::*;
use purrdf::slice::rdf_query::{Object, Subject};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
const XSD_DURATION: &str = "http://www.w3.org/2001/XMLSchema#duration";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

// --------------------------------------------------------------------------- #
// DL-clean datatypes (Principle 3)
// Complex blank-node union + cardinality check on gmeow:reminderTrigger.
// --------------------------------------------------------------------------- #

#[gmeow_test_batch_macros::batch_test]
fn calendar_temporal_datatypes_are_datetime_or_duration() {
    let g = GraphStore::ontology();

    for (prop, rng) in [
        ("exceptionOriginalDate", XSD_DATE_TIME),
        ("taskDueDate", XSD_DATE_TIME),
    ] {
        assert!(
            g.has(Some(&gmeow(prop)), Some(RDFS_RANGE), Some(rng)),
            "gmeow:{prop} must range over {rng}"
        );
    }

    // reminderTrigger ranges over a union datatype (xsd:duration OR xsd:dateTime).
    let range_nodes = g.objects_h(&Subject::Named(gmeow("reminderTrigger")), RDFS_RANGE);
    assert_eq!(
        range_nodes.len(),
        1,
        "reminderTrigger must have exactly one range"
    );
    let range_node = GraphStore::object_as_subject(&range_nodes[0])
        .expect("reminderTrigger range node is a named or blank subject");

    assert!(
        g.objects_h(&range_node, RDF_TYPE)
            .contains(&Object::Named(RDFS_DATATYPE.to_owned())),
        "reminderTrigger range node must be an rdfs:Datatype"
    );

    let union_head_obj = g
        .value_h(&range_node, OWL_UNION_OF)
        .expect("reminderTrigger range must carry an owl:unionOf");
    let union_head = GraphStore::object_as_subject(&union_head_obj)
        .expect("owl:unionOf head is a named or blank list node");
    let union_members = g.rdf_list_h(&union_head);

    assert!(
        union_members.contains(&Object::Named(XSD_DURATION.to_owned())),
        "reminderTrigger range must include duration; got {union_members:?}"
    );
    assert!(
        union_members.contains(&Object::Named(XSD_DATE_TIME.to_owned())),
        "reminderTrigger range must include dateTime; got {union_members:?}"
    );

    // taskPriority is integer (0-9).
    assert!(
        g.has(
            Some(&gmeow("taskPriority")),
            Some(RDFS_RANGE),
            Some(XSD_INTEGER)
        ),
        "gmeow:taskPriority must range over xsd:integer"
    );
}

// --------------------------------------------------------------------------- #
// Orthogonality — schedule, invitation, availability, reminder, task axes are
// independent. No inferential bridge between them.
// combinations sweep (45 pairs x 4 assertions).
// --------------------------------------------------------------------------- #

const ORTHOGONAL_PROPS: &[&str] = &[
    "scheduleTemplateEvent",
    "scheduleRecurrenceRule",
    "invitationEvent",
    "invitationStatus",
    "availabilitySlot",
    "availabilityStatus",
    "reminderTrigger",
    "reminderAction",
    "taskDueDate",
    "taskStatus",
];

#[gmeow_test_batch_macros::batch_test]
fn calendar_axes_are_independent() {
    let g = GraphStore::ontology();

    for (i, &a) in ORTHOGONAL_PROPS.iter().enumerate() {
        for &b in ORTHOGONAL_PROPS.iter().skip(i + 1) {
            let (na, nb) = (gmeow(a), gmeow(b));
            assert!(
                !g.has(Some(&na), Some(RDFS_SUB_PROPERTY_OF), Some(&nb)),
                "{a} ⊑ {b} forbidden"
            );
            assert!(
                !g.has(Some(&nb), Some(RDFS_SUB_PROPERTY_OF), Some(&na)),
                "{b} ⊑ {a} forbidden"
            );
            assert!(
                !g.has(Some(&na), Some(OWL_EQUIVALENT_PROPERTY), Some(&nb)),
                "{a} ≡ {b} forbidden"
            );
            assert!(
                !g.has(Some(&nb), Some(OWL_EQUIVALENT_PROPERTY), Some(&na)),
                "{b} ≡ {a} forbidden"
            );
        }
    }
}
