// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{BlankScope, RdfDatasetBuilder, RdfTerm};

use super::{HYBRID_QUERY_WORLD, load_hybrid_query_facts, place_world_dataset};

#[test]
fn command_world_placement_keeps_claim_layers_and_provenance_values() {
    let mut source = RdfDatasetBuilder::new();
    let subject = source.intern_blank("member", BlankScope(7));
    let object = source.intern_blank("member", BlankScope(9));
    let predicate = source.intern_iri("urn:command:relation");
    let claim = source.intern_blank("claim", BlankScope(7));
    let old_world = source.intern_iri("urn:command:old-world");
    let empty_world = source.intern_iri("urn:command:empty-old-world");
    source.declare_named_graph(empty_world);
    source.push_quad(subject, predicate, object, Some(old_world));
    let triple = source.intern_triple(subject, predicate, object);
    source.push_reifier_in_graph(claim, triple, None);
    source.push_annotation_in_graph(claim, predicate, old_world, Some(old_world));
    let source = source.freeze().unwrap();
    let original = source.owned_quads().next().unwrap();

    for world in [HYBRID_QUERY_WORLD, "urn:command:session-world"] {
        let output = place_world_dataset(source.clone(), world).unwrap();
        assert_eq!(
            output.owned_named_graphs().collect::<Vec<_>>(),
            [RdfTerm::iri(world)]
        );
        assert_eq!(output.quad_count(), 1);
        assert_eq!(output.reifiers().count(), 1);
        assert_eq!(output.annotations().count(), 1);
        let row = output.owned_quads().next().unwrap();
        assert_eq!(row.subject, original.subject);
        assert_eq!(row.object, original.object);
        assert_ne!(row.subject, row.object);
        assert_eq!(row.graph_name, Some(RdfTerm::iri(world)));
        let reifier = output.owned_reifiers().next().unwrap();
        assert_eq!(reifier.statement.subject, row.subject);
        assert_eq!(reifier.statement.object, row.object);
        assert_eq!(reifier.graph, Some(RdfTerm::iri(world)));
        let annotation = output.owned_annotations().next().unwrap();
        assert_eq!(annotation.reifier, reifier.reifier);
        assert_eq!(annotation.graph, Some(RdfTerm::iri(world)));
        assert_eq!(annotation.object, RdfTerm::iri("urn:command:old-world"));
    }
    assert_eq!(source.owned_quads().next().unwrap(), original);
}

#[test]
fn command_empty_world_replaces_source_declaration() {
    let mut source = RdfDatasetBuilder::new();
    let old_world = source.intern_iri("urn:command:old-world");
    source.declare_named_graph(old_world);
    let output = place_world_dataset(source.freeze().unwrap(), HYBRID_QUERY_WORLD).unwrap();
    assert_eq!(output.quad_count(), 0);
    assert_eq!(
        output.owned_named_graphs().collect::<Vec<_>>(),
        [RdfTerm::iri(HYBRID_QUERY_WORLD)]
    );
}

#[test]
fn hybrid_facts_loader_keeps_user_standpoint_in_the_query_world() {
    // Tiny caller-owned input; no authored repository source or producer fixture.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.ttl");
    std::fs::write(
        &path,
        r#"
        <urn:command:claim> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies>
            <<( <urn:command:member> <urn:command:relation> <urn:command:value> )>> ;
            <https://blackcatinformatics.ca/logic/standpoint> <urn:command:alice> .
    "#,
    )
    .unwrap();
    let store = load_hybrid_query_facts(&gmeow_cli_core::SilentReporter, &path).unwrap();
    let rows = store.quads_in_world(HYBRID_QUERY_WORLD);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|q| q[3] == HYBRID_QUERY_WORLD));
    assert!(rows.iter().any(|q| q[0] == "<urn:command:claim>"
        && q[1] == "<https://blackcatinformatics.ca/logic/standpoint>"
        && q[2] == "<urn:command:alice>"));
    assert!(rows.iter().any(
        |q| q[1] == "<http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies>"
            && q[2].contains("urn:command:member")
    ));
}
