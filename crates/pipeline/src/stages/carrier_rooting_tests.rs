// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic contracts for GMEOW carrier placement and compile graph selection.

use std::collections::BTreeMap;
use std::sync::Arc;

use purrdf::{
    BlankScope, ContentStore, DatasetProvenance, PipelineBundle, RdfDataset, RdfDatasetBuilder,
    RdfLocation, RdfLookaside, RdfQuad, RdfTerm,
};

use super::{
    GRAPH_DIAGNOSTICS, compile_logic_carrier_graphs, compile_logic_object_graphs, rooted_in_graph,
};
use crate::node::StageProduct;
use crate::stages::compile_logic::{
    CARRIER_GRAPHS, GRAPH_CORRESPONDENCE, GRAPH_LOGIC, GRAPH_RELATIONAL_CORE,
};

const RELATION: &str = "urn:carrier:relation";
const STANDPOINT: &str = "https://blackcatinformatics.ca/logic/standpoint";

fn assert_graph_owner(dataset: &RdfDataset, graph: &str) {
    let expected = RdfTerm::iri(graph);
    assert_eq!(
        dataset.owned_named_graphs().collect::<Vec<_>>(),
        [expected.clone()]
    );
    assert!(
        dataset
            .owned_quads()
            .all(|row| row.graph_name.as_ref() == Some(&expected))
    );
    assert!(
        dataset
            .owned_reifiers()
            .all(|row| row.graph.as_ref() == Some(&expected))
    );
    assert!(
        dataset
            .owned_annotations()
            .all(|row| row.graph.as_ref() == Some(&expected))
    );
}

/// The same claim resource has separate logic and diagnostic ownership. The
/// selected relational-core graph exists without rows; correspondence has only
/// statement-layer rows and is carried only by the complete shipping selection.
fn compile_source(logic_standpoint: &str, diagnostic_standpoint: &str) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri("urn:carrier:subject");
    let relation = builder.intern_iri(RELATION);
    let object = builder.intern_iri("urn:carrier:object");
    let claim = builder.intern_iri("urn:carrier:claim");
    let statement = builder.intern_triple(subject, relation, object);
    let standpoint = builder.intern_iri(STANDPOINT);
    for (graph, context) in [
        (GRAPH_LOGIC, logic_standpoint),
        (GRAPH_DIAGNOSTICS, diagnostic_standpoint),
    ] {
        let graph = builder.intern_iri(graph);
        let context = builder.intern_iri(context);
        builder.push_quad(subject, relation, object, Some(graph));
        builder.push_reifier_in_graph(claim, statement, Some(graph));
        builder.push_annotation_in_graph(claim, standpoint, context, Some(graph));
    }
    let relational = builder.intern_iri(GRAPH_RELATIONAL_CORE);
    builder.declare_named_graph(relational);
    let correspondence = builder.intern_iri(GRAPH_CORRESPONDENCE);
    let correspondence_context = builder.intern_iri("urn:standpoint:correspondence");
    builder.push_reifier_in_graph(claim, statement, Some(correspondence));
    builder.push_annotation_in_graph(
        claim,
        standpoint,
        correspondence_context,
        Some(correspondence),
    );
    builder.freeze().expect("freeze synthetic compile carrier")
}

fn compile_inputs(source: Arc<RdfDataset>) -> BTreeMap<String, StageProduct> {
    BTreeMap::from([(
        "stage-compile-logic".to_owned(),
        StageProduct::from_artifacts_over("stage-compile-logic", source, BTreeMap::new()),
    )])
}

fn commitment_bundle(dataset: Arc<RdfDataset>) -> PipelineBundle<()> {
    PipelineBundle::new(
        dataset,
        RdfLookaside::default(),
        Arc::new(ContentStore::new()),
        DatasetProvenance::new(),
    )
}

#[test]
fn rooting_moves_statement_metadata_without_changing_within_input_identity() {
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_blank("member", BlankScope(7));
    let object = builder.intern_blank("member", BlankScope(9));
    let relation = builder.intern_iri(RELATION);
    let claim = builder.intern_blank("claim", BlankScope(7));
    let statement = builder.intern_triple(subject, relation, object);
    let old_graph = builder.intern_iri("urn:carrier:previous-owner");
    let standpoint = builder.intern_iri(STANDPOINT);
    let provenance = builder.intern_iri("http://www.w3.org/ns/prov#wasDerivedFrom");
    let location = RdfLocation::logical("synthetic compile projection");
    let row = builder.push_quad_with_handle(subject, relation, object, None);
    builder.attach_location(row, location.clone());
    builder.push_reifier_in_graph(claim, statement, Some(old_graph));
    builder.push_annotation_in_graph(claim, standpoint, subject, None);
    builder.push_annotation_in_graph(claim, provenance, old_graph, Some(old_graph));
    let source = builder.freeze().expect("freeze selected RDF input");
    let original = source.owned_quads().next().expect("ordinary source row");
    assert_ne!(
        original.subject, original.object,
        "source scopes are distinct"
    );

    let rooted = rooted_in_graph(&source, GRAPH_LOGIC).expect("root complete logic carrier");
    assert_graph_owner(&rooted, GRAPH_LOGIC);
    let ordinary: Vec<_> = rooted.owned_quads().collect();
    assert_eq!(
        ordinary,
        [
            RdfQuad::new(original.subject.clone(), RELATION, original.object.clone())
                .in_graph(RdfTerm::iri(GRAPH_LOGIC))
                .with_location(location)
        ]
    );
    let reifiers: Vec<_> = rooted.owned_reifiers().collect();
    let annotations: Vec<_> = rooted.owned_annotations().collect();
    assert_eq!(reifiers.len(), 1);
    assert_eq!(annotations.len(), 2);
    assert_eq!(reifiers[0].statement.subject, original.subject);
    assert_eq!(reifiers[0].statement.predicate, RELATION);
    assert_eq!(reifiers[0].statement.object, original.object);
    for annotation in &annotations {
        assert_eq!(annotation.reifier, reifiers[0].reifier);
    }
    let context = annotations
        .iter()
        .find(|row| row.predicate == STANDPOINT)
        .unwrap();
    assert_eq!(context.object, original.subject);
    let origin = annotations
        .iter()
        .find(|row| row.predicate == "http://www.w3.org/ns/prov#wasDerivedFrom")
        .unwrap();
    assert_eq!(origin.object, RdfTerm::iri("urn:carrier:previous-owner"));
}

#[test]
fn compile_project_reinsert_keeps_all_selected_graph_layers_and_world_boundaries() {
    let source = compile_source("urn:standpoint:logic", "urn:standpoint:diagnostic");
    let upstream = compile_inputs(Arc::clone(&source));
    let shipped = compile_logic_carrier_graphs(&upstream).expect("select shipping carrier");
    assert_eq!(shipped.len(), CARRIER_GRAPHS.len());
    for (graph, dataset) in CARRIER_GRAPHS.iter().zip(&shipped) {
        assert_graph_owner(dataset, graph);
        let expected = source.project_named_graph(graph);
        let recovered = dataset.project_named_graph(graph);
        assert_eq!(
            recovered.owned_quads().collect::<Vec<_>>(),
            expected.owned_quads().collect::<Vec<_>>()
        );
        assert_eq!(
            recovered.owned_reifiers().collect::<Vec<_>>(),
            expected.owned_reifiers().collect::<Vec<_>>()
        );
        assert_eq!(
            recovered.owned_annotations().collect::<Vec<_>>(),
            expected.owned_annotations().collect::<Vec<_>>()
        );
    }
    let logic = &shipped[0];
    assert_eq!(logic.owned_reifiers().count(), 1);
    assert_eq!(logic.owned_annotations().count(), 1);
    assert_eq!(
        logic
            .owned_annotations()
            .next()
            .expect("logic context")
            .object,
        RdfTerm::iri("urn:standpoint:logic")
    );
    let correspondence = &shipped[2];
    assert_eq!(correspondence.quad_count(), 0);
    assert_eq!(correspondence.owned_reifiers().count(), 1);
    assert_eq!(correspondence.owned_annotations().count(), 1);
    let reasoning = compile_logic_object_graphs(&upstream).expect("select reasoning carrier");
    assert_eq!(reasoning.len(), 2);
    for (dataset, graph) in reasoning.iter().zip([GRAPH_LOGIC, GRAPH_RELATIONAL_CORE]) {
        assert_graph_owner(dataset, graph);
    }
}

#[test]
fn logic_handle_commitment_tracks_reinserted_metadata_and_excludes_diagnostics() {
    let source = compile_source("urn:standpoint:logic", "urn:standpoint:diagnostic");
    let expected = commitment_bundle(Arc::clone(&source)).graph_digest(GRAPH_LOGIC);
    let mut selected = compile_logic_carrier_graphs(&compile_inputs(source)).unwrap();
    let mut bundle = commitment_bundle(selected.remove(0));
    bundle
        .pin_handle(GRAPH_LOGIC, (), expected)
        .expect("reinsertion preserves the logic pin");
    assert_eq!(bundle.handle(GRAPH_LOGIC).unwrap().content_digest, expected);

    let mut changed_metadata = compile_logic_carrier_graphs(&compile_inputs(compile_source(
        "urn:standpoint:changed-logic",
        "urn:standpoint:diagnostic",
    )))
    .unwrap();
    let mut changed = commitment_bundle(changed_metadata.remove(0));
    assert_ne!(changed.graph_digest(GRAPH_LOGIC), expected);
    assert!(matches!(
        changed.pin_handle(GRAPH_LOGIC, (), expected),
        Err(purrdf::PipelineBundleError::HandleDigestMismatch { .. })
    ));

    let mut changed_diagnostics = compile_logic_carrier_graphs(&compile_inputs(compile_source(
        "urn:standpoint:logic",
        "urn:standpoint:changed-diagnostic",
    )))
    .unwrap();
    let diagnostic_only = commitment_bundle(changed_diagnostics.remove(0));
    assert_eq!(diagnostic_only.graph_digest(GRAPH_LOGIC), expected);
}

#[test]
fn empty_selected_input_and_source_declarations_become_the_destination_declaration() {
    for source_declaration in [None, Some("urn:carrier:previous-empty-owner")] {
        let mut builder = RdfDatasetBuilder::new();
        if let Some(graph) = source_declaration {
            let graph = builder.intern_iri(graph);
            builder.declare_named_graph(graph);
        }
        let source = builder.freeze().expect("freeze empty selection");
        let rooted = rooted_in_graph(&source, GRAPH_RELATIONAL_CORE).expect("root empty carrier");
        assert_eq!(rooted.rdf_row_count(), 0);
        assert_graph_owner(&rooted, GRAPH_RELATIONAL_CORE);
    }
}

#[test]
fn ordinary_projection_reinsert_preserves_content_and_source_location() {
    let ordinary = RdfQuad::new(
        RdfTerm::iri("urn:carrier:subject"),
        RELATION,
        RdfTerm::iri("urn:carrier:object"),
    )
    .with_location(RdfLocation::logical("synthetic ordinary projection"));
    let mut builder = RdfDatasetBuilder::new();
    builder.push_owned_quad(&ordinary);
    let source = builder.freeze().unwrap();
    let rooted = rooted_in_graph(&source, GRAPH_LOGIC).unwrap();
    assert_graph_owner(&rooted, GRAPH_LOGIC);
    let recovered = rooted.project_named_graph(GRAPH_LOGIC);
    assert_eq!(recovered.owned_quads().collect::<Vec<_>>(), [ordinary]);
    assert_eq!(recovered.owned_reifiers().count(), 0);
    assert_eq!(recovered.owned_annotations().count(), 0);
}
