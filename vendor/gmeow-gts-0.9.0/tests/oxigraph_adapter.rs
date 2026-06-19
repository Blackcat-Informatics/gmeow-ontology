// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(feature = "oxigraph-adapter")]

use std::error::Error;

use gmeow_gts::model::{Term, TermKind};
use gmeow_gts::oxigraph::{
    graph_into_quads_with_sidecar, graph_to_store, graph_to_store_with_sidecar, store_to_gts_bytes,
};
use gmeow_gts::reader::read;
use gmeow_gts::writer::Writer;
use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term as OxTerm};
use oxigraph::store::Store;

fn iri(value: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn sorted_store(store: &Store) -> Result<Vec<String>, Box<dyn Error>> {
    let mut rows = store
        .iter()
        .map(|quad| quad.map(|quad| quad.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    rows.sort();
    Ok(rows)
}

#[test]
fn graph_to_store_preserves_named_graphs_and_sidecar() -> Result<(), Box<dyn Error>> {
    let mut writer = Writer::new("dist");
    writer.add_terms(&[
        iri("https://example.org/s"),
        iri("https://example.org/p"),
        iri("https://example.org/o"),
        iri("https://example.org/g"),
    ]);
    writer.add_quads(&[(0, 1, 2, Some(3))]);
    writer.add_blob(b"payload", Some("text/plain"), None);

    let graph = read(&writer.to_bytes(), true, None);
    let projected = graph_to_store_with_sidecar(graph)?;
    let rows = projected.store.iter().collect::<Result<Vec<_>, _>>()?;

    assert_eq!(rows.len(), 1);
    match &rows[0].graph_name {
        GraphName::NamedNode(node) => assert_eq!(node.as_str(), "https://example.org/g"),
        other => panic!("expected named graph, got {other:?}"),
    }
    assert_eq!(projected.sidecar.blobs.len(), 1);
    assert_eq!(projected.sidecar.blob_meta.len(), 1);
    assert_eq!(projected.sidecar.segment_heads.len(), 1);
    Ok(())
}

#[test]
fn graph_into_quads_returns_gts_sidecar() -> Result<(), Box<dyn Error>> {
    let mut writer = Writer::new("dist");
    writer.add_terms(&[
        iri("https://example.org/s"),
        iri("https://example.org/p"),
        iri("https://example.org/o"),
    ]);
    writer.add_quads(&[(0, 1, 2, None)]);
    writer.add_meta(ciborium::value::Value::Map(vec![(
        "source".into(),
        "synthetic".into(),
    )]));

    let graph = read(&writer.to_bytes(), true, None);
    let (quads, sidecar) = graph_into_quads_with_sidecar(graph)?;

    assert_eq!(quads.count(), 1);
    assert_eq!(sidecar.meta.len(), 1);
    assert_eq!(sidecar.segment_profiles, vec!["dist".to_string()]);
    Ok(())
}

#[test]
fn store_to_writer_roundtrips_native_quads() -> Result<(), Box<dyn Error>> {
    let store = Store::new()?;
    let graph_name = NamedNode::new("https://example.org/g")?;
    let subject = NamedNode::new("https://example.org/s")?;
    let predicate = NamedNode::new("https://example.org/p")?;
    let object = Literal::new_simple_literal("object");
    store.insert(
        Quad::new(
            subject.clone(),
            predicate.clone(),
            OxTerm::from(object),
            graph_name.clone(),
        )
        .as_ref(),
    )?;

    let bytes = Writer::from_store(&store, "dist")?.to_bytes();
    assert_eq!(bytes, store_to_gts_bytes(&store, "dist")?);
    let graph = read(&bytes, true, None);
    assert!(graph.diagnostics.is_empty());
    let roundtripped = graph_to_store(&graph)?;

    assert_eq!(sorted_store(&roundtripped)?, sorted_store(&store)?);
    Ok(())
}
