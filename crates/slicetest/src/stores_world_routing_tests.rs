// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{BlankScope, RdfDatasetBuilder, RdfTerm};

use super::world_scoped;
use gmeow_logic::reason::rl::DEFAULT_WORLD;

#[test]
fn competency_world_keeps_attributed_claims_and_their_shared_native_subject() {
    let mut builder = RdfDatasetBuilder::new();
    let member = builder.intern_blank("member", BlankScope(7));
    let object = builder.intern_blank("member", BlankScope(9));
    let relation = builder.intern_iri("https://blackcatinformatics.ca/math/hasPart");
    let claim = builder.intern_blank("claim", BlankScope(7));
    let old = builder.intern_iri("urn:competency:source-world");
    let according_to = builder.intern_iri("https://blackcatinformatics.ca/gmeow/accordingTo");
    builder.push_quad(member, relation, object, None);
    let triple = builder.intern_triple(member, relation, object);
    builder.push_reifier_in_graph(claim, triple, Some(old));
    builder.push_annotation_in_graph(claim, according_to, old, Some(old));
    let empty = builder.intern_iri("urn:competency:empty-source-world");
    builder.declare_named_graph(empty);
    let source = builder.freeze().unwrap();
    let original = source.owned_quads().next().unwrap();
    let output = world_scoped(&source).unwrap();
    assert_eq!(output.owned_named_graphs().count(), 0);
    assert_eq!(output.rdf_row_count(), 3);
    let row = output.owned_quads().next().unwrap();
    assert_eq!(row.subject, original.subject);
    assert_eq!(row.object, original.object);
    assert_ne!(row.subject, row.object);
    assert_eq!(row.graph_name, None);
    let reifier = output.owned_reifiers().next().unwrap();
    assert_eq!(reifier.statement.subject, row.subject);
    assert_eq!(reifier.statement.object, row.object);
    assert_eq!(reifier.graph, None);
    let annotation = output.owned_annotations().next().unwrap();
    assert_eq!(annotation.reifier, reifier.reifier);
    assert_eq!(
        annotation.object,
        RdfTerm::iri("urn:competency:source-world")
    );
    assert_eq!(annotation.graph, None);
    // Exercise the GMEOW competency world's real ingress, without invoking a
    // source sweep or proving the upstream query/parser engine's conformance.
    let input = gmeow_logic::reason::prepare_reasoning_input(&output).unwrap();
    assert_eq!(input.source_assertion_count(DEFAULT_WORLD), Some(3));
    assert!(
        input
            .source_assertion_count("urn:competency:source-world")
            .is_none()
    );
    assert_eq!(source.owned_quads().next().unwrap(), original);
}

#[test]
fn competency_empty_selection_declares_the_reasoner_world_only() {
    let mut builder = RdfDatasetBuilder::new();
    let old = builder.intern_iri("urn:competency:empty-source-world");
    builder.declare_named_graph(old);
    let source = builder.freeze().unwrap();
    let output = world_scoped(&source).unwrap();
    assert_eq!(output.rdf_row_count(), 0);
    let input = gmeow_logic::reason::prepare_reasoning_input(&output).unwrap();
    assert_eq!(input.source_assertion_count(DEFAULT_WORLD), Some(0));
    assert_eq!(input.source_contexts().len(), 1);
    assert_eq!(output.owned_named_graphs().count(), 0);
}
