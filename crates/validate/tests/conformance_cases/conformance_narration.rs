// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_narration.py
//!
//! Migrated tests (SHACL fixture-based):
//!   - `test_wellformed_narration_fixture_conforms`  → `wellformed_narration_fixture_conforms`
//!   - `test_malformed_narration_fixture_is_flagged` → `malformed_narration_fixture_is_flagged`
//!
//! Retained in Python (not migrated):
//!   - `test_seam_links_specialize_one_ancestor`: pure `_graph()` TBox membership checks.
//!   - `test_orientations_are_not_inverse_axioms`: pure `_graph()` TBox membership checks.
//!   - `test_narration_usage_is_a_reified_relator_with_open_subject`: `_graph()` TBox checks.
//!   - `test_narration_mode_vocab_seeds`: `_graph()` subject iteration.
//!   - `test_no_truth_bridge_from_unreliable_mode`: `_graph()` object iteration.
//!   - `test_fixture_obeys_the_efficiency_budget`: iterates fixture quads, no `run_shacl`.
//!   - `test_competency_cooccurrence_query_over_fixture`: SPARQL SELECT over fixture.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

#[batch_cases]
#[case::wellformed_narration_fixture_conforms(Case::file("shapes", "narration-wellformed"))]
#[case::malformed_narration_fixture_is_flagged(
    Case::file("shapes", "narration-malformed")
        .fails()
        .violations(&[
            "at least one gmeow:narrationMode",
            "exactly one gmeow:narrationSubject",
            "exactly one gmeow:narrationSegment",
        ])
)]
fn narration(#[case] case: Case) {
    case.run();
}

// ── SPARQL / GraphStore twins migrated from tests/test_narration.py ────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_SHAPES: &str = "https://example.org/shapes/";

const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn ex_shapes(local: &str) -> String {
    format!("{EX_SHAPES}{local}")
}

/// Twin of `test_seam_links_specialize_one_ancestor`: narrates/narratedIn both
/// specialise the domain- and range-free `gmeow:narrationLink` ancestor.
#[gmeow_test_batch_macros::batch_test]
fn seam_links_specialize_one_ancestor() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gm("narrationLink")),
        Some(RDF_TYPE),
        Some(OWL_OBJECT_PROPERTY)
    ));
    assert!(g.has(
        Some(&gm("narrates")),
        Some(RDFS_SUBPROPERTY_OF),
        Some(&gm("narrationLink"))
    ));
    assert!(g.has(
        Some(&gm("narratedIn")),
        Some(RDFS_SUBPROPERTY_OF),
        Some(&gm("narrationLink"))
    ));
    assert!(g.objects(&gm("narrationLink"), RDFS_DOMAIN).is_empty());
    assert!(g.objects(&gm("narrationLink"), RDFS_RANGE).is_empty());
}

/// Twin of `test_orientations_are_not_inverse_axioms`: no `owl:inverseOf` between
/// the two orientations (EL-clean), each anchored on the ContentSegment side and
/// open on the content side.
#[gmeow_test_batch_macros::batch_test]
fn orientations_are_not_inverse_axioms() {
    let g = GraphStore::ontology();
    assert!(g.objects(&gm("narrates"), OWL_INVERSE_OF).is_empty());
    assert!(g.objects(&gm("narratedIn"), OWL_INVERSE_OF).is_empty());
    assert!(g.has(
        Some(&gm("narrates")),
        Some(RDFS_DOMAIN),
        Some(&gm("ContentSegment"))
    ));
    assert!(g.has(
        Some(&gm("narratedIn")),
        Some(RDFS_RANGE),
        Some(&gm("ContentSegment"))
    ));
    assert!(g.objects(&gm("narrates"), RDFS_RANGE).is_empty());
    assert!(g.objects(&gm("narratedIn"), RDFS_DOMAIN).is_empty());
}

/// Twin of `test_narration_mode_vocab_seeds`: the six narration-mode individuals
/// are declared `gmeow:NarrationMode` members (open vocabulary — subset check).
#[gmeow_test_batch_macros::batch_test]
fn narration_mode_vocab_seeds() {
    let g = GraphStore::ontology();
    let members = g.subjects_of_type(&gm("NarrationMode"));
    for seed in [
        "narrationDirect",
        "narrationMentioned",
        "narrationFlashback",
        "narrationDream",
        "narrationHypothetical",
        "narrationUnreliable",
    ] {
        assert!(
            members.contains(&gm(seed)),
            "missing NarrationMode seed gmeow:{seed}"
        );
    }
}

/// Twin of `test_no_truth_bridge_from_unreliable_mode`: narrationUnreliable is a
/// plain vocabulary individual — its only rdf:type is NarrationMode (no axiom
/// bridges it to the deception module).
#[gmeow_test_batch_macros::batch_test]
fn no_truth_bridge_from_unreliable_mode() {
    let g = GraphStore::ontology();
    let types = g.objects(&gm("narrationUnreliable"), RDF_TYPE);
    let expected: std::collections::BTreeSet<String> = [gm("NarrationMode")].into_iter().collect();
    assert_eq!(types, expected);
}

/// Twin of `test_fixture_obeys_the_efficiency_budget`: the chapter-scale fixture
/// carries many flat seam links, exactly one promoted NarrationUsage, and does
/// NOT duplicate the promoted link as a flat quad.
#[gmeow_test_batch_macros::batch_test]
fn fixture_obeys_the_efficiency_budget() {
    let g = GraphStore::parse_ttl_file(
        &repo_root().join("tests/fixtures/shapes/narration-wellformed.ttl"),
    );
    // Flat seam links: every (subject, object) pair under narrates OR narratedIn.
    let (_vars, flat) = g.select(
        &[],
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
         SELECT ?s ?o WHERE {\n\
           { ?s gmeow:narrates ?o } UNION { ?s gmeow:narratedIn ?o }\n\
         }",
    );
    assert!(
        flat.len() >= 14,
        "expected >= 14 flat links, got {}",
        flat.len()
    );

    let reified = g.subjects_of_type(&gm("NarrationUsage"));
    assert_eq!(reified.len(), 1, "exactly one promoted NarrationUsage");
    let usage = reified.iter().next().expect("one NarrationUsage");

    // The promoted link is not duplicated as a flat quad.
    let promoted = g.objects(usage, &gm("narrationSubject"));
    let subject = promoted.iter().next().expect("promoted subject present");
    assert!(!g.has(
        Some(&ex_shapes("chapter31")),
        Some(&gm("narrates")),
        Some(subject)
    ));
    assert!(!g.has(
        Some(subject),
        Some(&gm("narratedIn")),
        Some(&ex_shapes("chapter31"))
    ));
}

/// Twin of `test_competency_cooccurrence_query_over_fixture`: the DraCor
/// co-occurrence primitive (`narrative-narration-cooccurrence.rq`, a 3-way UNION)
/// pairs diegetic things sharing a segment reachable through all three seam forms.
#[gmeow_test_batch_macros::batch_test]
fn competency_cooccurrence_query_over_fixture() {
    QueryCase::new("narrative/narration-cooccurrence", &[Feature::Union])
        .over_ttl_file("tests/fixtures/shapes/narration-wellformed.ttl")
        .query_file("narrative-narration-cooccurrence.rq")
        .select_contains_rows(vec![
            // Guy entered via narratedIn; still pairs with flat-linked Phèdre.
            vec![
                iri(&ex_shapes("chapter31")),
                iri(&ex_shapes("guy")),
                iri(&ex_shapes("phedre")),
            ],
            // The oath event entered via the promoted NarrationUsage.
            vec![
                iri(&ex_shapes("chapter31")),
                iri(&ex_shapes("evtOath")),
                iri(&ex_shapes("phedre")),
            ],
        ])
        .run();
}
