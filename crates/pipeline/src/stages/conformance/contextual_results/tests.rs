// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny native controls for GMEOW's observation selection and quote boundary.

use purrdf::RdfDatasetBuilder;

use super::*;

/// A duplicate selected result must remain visible to the consumer's cardinality
/// assertion; a verdict in another graph cannot replace the selected verdict.
#[test]
fn observation_keeps_duplicate_results_and_exact_reasoning_graph() {
    // gmeow-test-input: synthetic-only
    let mut builder = RdfDatasetBuilder::new();
    let graph = builder.intern_iri(gmeow_logic::result_rdf::GRAPH_REASONING);
    let foreign = builder.intern_iri("urn:contextual-observation:foreign-world");
    let request_iri = format!("{CONTEXTUAL}proposalAssessment");
    let request = builder.intern_iri(&request_iri);
    let relation = builder.intern_iri(&format!("{LOGIC}contextualResult"));
    let result = builder.intern_iri("urn:contextual-observation:result");
    let duplicate = builder.intern_iri("urn:contextual-observation:duplicate");
    let absent = builder.intern_iri("urn:contextual-observation:foreign-result");
    let information = builder.intern_iri(&format!("{LOGIC}resultInformation"));
    let supported = builder.intern_iri("urn:contextual-observation:supported");
    let opposed = builder.intern_iri("urn:contextual-observation:opposed");
    let quoted = builder.intern_triple(result, information, supported);
    builder.push_reifier_in_graph(request, quoted, Some(graph));
    builder.push_quad(request, relation, result, Some(graph));
    builder.push_annotation_in_graph(request, relation, duplicate, Some(graph));
    builder.push_annotation_in_graph(request, relation, absent, Some(foreign));
    builder.push_quad(result, information, supported, Some(graph));
    builder.push_quad(result, information, opposed, Some(foreign));
    let observed = observe(&builder.freeze().expect("tiny selected-result rows"));
    assert_eq!(
        observed.nodes[&request_iri].properties[&format!("{LOGIC}contextualResult")],
        BTreeSet::from([
            "<urn:contextual-observation:result>".to_owned(),
            "<urn:contextual-observation:duplicate>".to_owned()
        ])
    );
    assert_eq!(
        observed.nodes["urn:contextual-observation:result"].properties
            [&format!("{LOGIC}resultInformation")],
        BTreeSet::from(["<urn:contextual-observation:supported>".to_owned()])
    );
    assert!(
        !observed
            .nodes
            .contains_key("urn:contextual-observation:foreign-result")
    );
    assert!(
        observed.nodes[&format!("{CONTEXTUAL}reviewAssessment")]
            .properties
            .is_empty(),
        "absent requests stay absent evidence; the observer cannot fabricate results"
    );
}

/// The non-assertion contract must detect ordinary and annotation assertions in
/// default, named and blank-node worlds, while excluding a reifier's quotation.
#[test]
fn readiness_observation_distinguishes_quoted_evidence_from_asserted_annotations() {
    // gmeow-test-input: synthetic-only
    for asserted in [false, true] {
        let mut builder = RdfDatasetBuilder::new();
        let subject = builder.intern_iri(&format!("{CONTEXTUAL}procedure"));
        let predicate = builder.intern_iri(&format!("{CONTEXTUAL}readyFor"));
        let object = builder.intern_iri(&format!("{CONTEXTUAL}execution"));
        let named = builder.intern_iri("urn:contextual-observation:world");
        let blank = builder.intern_blank("contextual-world", purrdf::BlankScope::DEFAULT);
        let quoted = builder.intern_triple(subject, predicate, object);
        builder.push_reifier_in_graph(subject, quoted, Some(named));
        if asserted {
            builder.push_quad(subject, predicate, object, None);
            builder.push_annotation_in_graph(subject, predicate, object, Some(named));
            builder.push_annotation_in_graph(subject, predicate, object, Some(blank));
        }
        let dataset = builder.freeze().expect("tiny readiness control");
        let observed = observe(&dataset);
        let expected = if asserted {
            BTreeSet::from([
                None,
                Some(dataset.to_owned_term(named).to_string()),
                Some(dataset.to_owned_term(blank).to_string()),
            ])
        } else {
            BTreeSet::new()
        };
        assert_eq!(observed.readiness_graphs[CONTEXTUAL], expected);
    }
}
