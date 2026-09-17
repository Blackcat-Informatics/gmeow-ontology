// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{BlankScope, RdfDatasetBuilder, RdfLocation, RdfTerm};

use super::{GRAPH_DIAGNOSTICS, GRAPH_DOCUMENTATION, playground_dataset_from_bundle};
use gmeow_logic::result_rdf::GRAPH_REASONING;

#[test]
fn playground_selects_complete_graph_roles_in_one_identity_space() {
    let mut builder = RdfDatasetBuilder::new();
    let member = builder.intern_blank("member", BlankScope(7));
    let claim = builder.intern_blank("claim", BlankScope(9));
    let predicate = builder.intern_iri("urn:playground:relation");
    let source_graph = builder.intern_iri("urn:playground:original-source");
    let statement = builder.intern_triple(member, predicate, source_graph);
    let standpoint = builder.intern_iri("https://blackcatinformatics.ca/logic/standpoint");
    let location = RdfLocation::logical("selected playground source");
    for role in [GRAPH_DOCUMENTATION, GRAPH_REASONING, GRAPH_DIAGNOSTICS] {
        let graph = builder.intern_iri(role);
        let row = builder.push_quad_with_handle(member, predicate, source_graph, Some(graph));
        builder.attach_location(row, location.clone());
        builder.push_reifier_in_graph(claim, statement, Some(graph));
        builder.push_annotation_in_graph(claim, standpoint, graph, Some(graph));
    }
    builder.declare_named_graph(source_graph);
    let empty = builder.intern_iri("urn:playground:unused-empty");
    builder.declare_named_graph(empty);
    let source = builder.freeze().unwrap();
    let original_rows = source.owned_quads().collect::<Vec<_>>();
    let output = playground_dataset_from_bundle(&source).unwrap();
    assert_eq!(output.quad_count(), 2);
    assert_eq!(output.reifiers().count(), 2);
    assert_eq!(output.annotations().count(), 2);
    let graphs = output.owned_named_graphs().collect::<Vec<_>>();
    assert_eq!(graphs.len(), 2);
    assert!(graphs.contains(&RdfTerm::iri(GRAPH_DOCUMENTATION)));
    assert!(graphs.contains(&RdfTerm::iri(GRAPH_REASONING)));
    let rows = output.owned_quads().collect::<Vec<_>>();
    assert_eq!(rows[0].subject, rows[1].subject);
    for role in [GRAPH_DOCUMENTATION, GRAPH_REASONING] {
        let graph = Some(RdfTerm::iri(role));
        let row = rows.iter().find(|q| q.graph_name == graph).unwrap();
        assert_eq!(row.object, RdfTerm::iri("urn:playground:original-source"));
        assert_eq!(row.location.as_ref(), Some(&location));
        let reifier = output.owned_reifiers().find(|r| r.graph == graph).unwrap();
        let annotation = output
            .owned_annotations()
            .find(|a| a.graph == graph)
            .unwrap();
        assert_eq!(reifier.statement.subject, row.subject);
        assert_eq!(reifier.statement.object, row.object);
        assert_eq!(annotation.reifier, reifier.reifier);
        assert_eq!(annotation.object, RdfTerm::iri(role));
    }
    let reifiers = output.owned_reifiers().collect::<Vec<_>>();
    assert_eq!(reifiers[0].reifier, reifiers[1].reifier);
    assert_eq!(source.owned_quads().collect::<Vec<_>>(), original_rows);
}

#[test]
fn playground_keeps_required_empty_graph_roles_without_stale_declarations() {
    let mut builder = RdfDatasetBuilder::new();
    let old = builder.intern_iri("urn:playground:unused-empty");
    builder.declare_named_graph(old);
    let output = playground_dataset_from_bundle(&builder.freeze().unwrap()).unwrap();
    assert_eq!(output.quad_count(), 0);
    assert_eq!(output.reifiers().count(), 0);
    assert_eq!(output.annotations().count(), 0);
    let graphs = output.owned_named_graphs().collect::<Vec<_>>();
    assert_eq!(graphs.len(), 2);
    assert!(graphs.contains(&RdfTerm::iri(GRAPH_DOCUMENTATION)));
    assert!(graphs.contains(&RdfTerm::iri(GRAPH_REASONING)));
}

#[test]
fn playground_lifts_only_witness_derivation_out_of_diagnostics() {
    let mut builder = RdfDatasetBuilder::new();
    let graph = builder.intern_iri(GRAPH_DIAGNOSTICS);
    let witness = builder.intern_iri("urn:playground:invented");
    let reifier = builder.intern_iri("urn:playground:minting-head");
    let finding = builder.intern_iri("urn:playground:finding");
    let rdf_type = builder.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let rdf_object = builder.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#object");
    let invented = builder.intern_iri(&format!("{}InventedWitness", gmeow_ns::GMEOW_NS));
    let finding_type = builder.intern_iri(&format!("{}Finding", gmeow_ns::GMEOW_NS));
    builder.push_quad(witness, rdf_type, invented, Some(graph));
    builder.push_quad(reifier, rdf_object, witness, Some(graph));
    builder.push_quad(finding, rdf_type, finding_type, Some(graph));
    let source = builder.freeze().unwrap();
    let output = playground_dataset_from_bundle(&source).unwrap();
    let rows = output.owned_quads().collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|q| q.graph_name == Some(RdfTerm::iri(GRAPH_REASONING)))
    );
    assert!(
        rows.iter()
            .any(|q| q.subject == RdfTerm::iri("urn:playground:invented"))
    );
    assert!(
        rows.iter()
            .any(|q| q.subject == RdfTerm::iri("urn:playground:minting-head"))
    );
    assert!(
        !rows
            .iter()
            .any(|q| q.subject == RdfTerm::iri("urn:playground:finding"))
    );
    assert_eq!(source.quad_count(), 3);
}
