// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_narrative_time.py
//!
//! Two SHACL-conformance tests from the narrative-time test module are ported
//! here. The remaining tests (TBox membership, dynamic sweep, SPARQL competency
//! query, fixture graph-walk) are retained in Python because they either call
//! `load_merged_graph`, iterate subjects dynamically, or run SPARQL queries.
//!
//! Retained in Python (not migrated):
//!   - `test_narrative_time_frame_is_a_reference_frame`: TBox `(triple) in g` membership.
//!   - `test_axis_vocab_spans_exactly_fabula_and_syuzhet`: dynamic sweep of `g.subjects(...)`.
//!   - `test_frame_properties_are_functional_with_correct_anchors`: TBox membership loop.
//!   - `test_position_is_an_object_with_frame_ordinal_label`: TBox membership checks.
//!   - `test_at_narrative_position_is_domain_free_and_not_functional`: TBox membership.
//!   - `test_flashback_fixture_carries_coexisting_orders`: fixture graph-walk (`g.objects()`).
//!   - `test_competency_narrative_time_axes_query`: SPARQL competency query.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use purrdf::slice::rdf_query::{Object, Subject};

#[batch_cases]
#[case::wellformed_narrative_time_fixture_conforms(Case::file(
    "shapes",
    "narrative-time-wellformed"
))]
#[case::malformed_narrative_time_fixture_is_flagged(
    Case::file("shapes", "narrative-time-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:narrativeTimeAxis",
            "never the other anchor",
            "exactly one reference frame (gmeow:positionFrame)",
        ])
)]
fn narrative_time(#[case] case: Case) {
    case.run();
}

// ── SPARQL / GraphStore twins migrated from tests/test_narrative_time.py ───────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_SHAPES: &str = "https://example.org/shapes/";

const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn ex_shapes(local: &str) -> String {
    format!("{EX_SHAPES}{local}")
}

/// Twin of `test_frame_properties_are_functional_with_correct_anchors`: the four
/// frame properties are OWL-functional and range-anchored to their correct class.
#[gmeow_test_batch_macros::batch_test]
fn frame_properties_are_functional_with_correct_anchors() {
    let g = GraphStore::ontology();
    for (prop, range) in [
        ("narrativeTimeAxis", "NarrativeTimeAxis"),
        ("discourseTimeOf", "CreativeWork"),
        ("storyTimeOf", "NarrativeReferenceFrame"),
        ("positionFrame", "NarrativeTimeFrame"),
    ] {
        assert!(
            g.is_functional_carrier(&gm(prop)),
            "gmeow:{prop} must carry a logic: functionalProperty characteristic"
        );
        assert!(
            g.has(Some(&gm(prop)), Some(RDFS_RANGE), Some(&gm(range))),
            "gmeow:{prop} must have range gmeow:{range}"
        );
    }
}

/// Twin of `test_at_narrative_position_is_domain_free_and_not_functional`: the
/// shared anchor is a domain-free, non-functional ObjectProperty ranged on
/// NarrativePosition — coexisting positions are the flashback (P9).
#[gmeow_test_batch_macros::batch_test]
fn at_narrative_position_is_domain_free_and_not_functional() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gm("atNarrativePosition")),
        Some(RDF_TYPE),
        Some(OWL_OBJECT_PROPERTY)
    ));
    assert!(
        g.objects(&gm("atNarrativePosition"), RDFS_DOMAIN)
            .is_empty()
    );
    assert!(!g.is_functional_carrier(&gm("atNarrativePosition")));
    assert!(g.has(
        Some(&gm("atNarrativePosition")),
        Some(RDFS_RANGE),
        Some(&gm("NarrativePosition"))
    ));
}

/// Twin of `test_flashback_fixture_carries_coexisting_orders`: the betrayal event
/// holds two coexisting positions — discourse ordinal 31 and story ordinal 1 —
/// both standing, no contradiction (P9).
#[gmeow_test_batch_macros::batch_test]
fn flashback_fixture_carries_coexisting_orders() {
    let g = GraphStore::parse_ttl_file(
        &repo_root().join("tests/fixtures/shapes/narrative-time-wellformed.ttl"),
    );
    let positions = g.objects_h(
        &Subject::Named(ex_shapes("betrayalEvent")),
        &gm("atNarrativePosition"),
    );
    assert_eq!(positions.len(), 2, "two coexisting positions");

    let mut by_frame: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for p in &positions {
        let pos = GraphStore::object_as_subject(p).expect("position is a named/blank subject");
        let ordinal = match g.value_h(&pos, &gm("positionOrdinal")) {
            Some(Object::Literal { value, .. }) => value.parse::<i64>().expect("integer ordinal"),
            other => panic!("expected a literal positionOrdinal, got {other:?}"),
        };
        let frame = match g.value_h(&pos, &gm("positionFrame")) {
            Some(Object::Named(iri)) => iri,
            other => panic!("expected a named positionFrame, got {other:?}"),
        };
        by_frame.insert(frame, ordinal);
    }
    assert_eq!(by_frame.get(&ex_shapes("discourseFrame")), Some(&31));
    assert_eq!(by_frame.get(&ex_shapes("storyFrame")), Some(&1));
}

/// Twin of `test_competency_narrative_time_axes_query`: the
/// `narrative-time-axes.rq` competency query returns exactly the two axes —
/// fabula and syuzhet, neither privileged.
#[gmeow_test_batch_macros::batch_test]
fn competency_narrative_time_axes_query() {
    QueryCase::new("narrative/time-axes", &[])
        .over_ontology()
        .query_file("narrative-time-axes.rq")
        .select_row_set(vec![
            vec![iri(&gm("axisDiscourseTime"))],
            vec![iri(&gm("axisStoryTime"))],
        ])
        .run();
}
