// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(feature = "rdf")]

use std::collections::HashSet;
use std::error::Error;

use gmeow_gts::model::{Graph, Term, TermKind};
use gmeow_gts::rdf::{from_oxrdf_dataset, to_oxrdf_dataset, to_oxrdf_dataset_lossy};
use gmeow_gts::reader::read;
use gmeow_gts::writer::Writer;
use oxrdf::{
    BlankNode, Dataset, GraphName, Literal, NamedNode, NamedOrBlankNodeRef, Quad as OxQuad,
    Term as OxTerm, TermRef as OxTermRef, Triple as OxTriple,
};

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

fn sorted_dataset(dataset: &Dataset) -> Vec<String> {
    let mut lines: Vec<String> = dataset.iter().map(|quad| quad.to_string()).collect();
    lines.sort();
    lines
}

#[test]
fn oxrdf_dataset_roundtrips_through_gts() -> Result<(), Box<dyn Error>> {
    let mut dataset = Dataset::new();
    let graph = NamedNode::new("https://example.org/graph")?;
    let cat = NamedNode::new("https://example.org/Cat")?;
    let friend = BlankNode::new("friend")?;
    let label = NamedNode::new(RDFS_LABEL)?;
    let lives = NamedNode::new("https://example.org/lives")?;
    let integer = NamedNode::new(XSD_INTEGER)?;

    dataset.insert(
        OxQuad::new(
            cat.clone(),
            label.clone(),
            Literal::new_language_tagged_literal("Cat", "en")?,
            graph.clone(),
        )
        .as_ref(),
    );
    dataset
        .insert(OxQuad::new(cat, lives, Literal::new_typed_literal("9", integer), graph).as_ref());
    dataset.insert(
        OxQuad::new(
            friend,
            label,
            Literal::new_simple_literal("Friend"),
            GraphName::DefaultGraph,
        )
        .as_ref(),
    );

    let bytes = from_oxrdf_dataset(&dataset)?;
    let folded = read(&bytes, true, None);
    assert!(folded.diagnostics.is_empty());
    let back = to_oxrdf_dataset(&folded)?;
    assert_eq!(sorted_dataset(&back), sorted_dataset(&dataset));
    Ok(())
}

#[test]
fn gts_reifier_projection_uses_oxrdf_rdf12_triple_terms() -> Result<(), Box<dyn Error>> {
    let mut writer = Writer::new("dist");
    writer.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/claim".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/subject".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/predicate".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some("object".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    writer.add_reifies(&[(0, (1, 2, 3))]);
    let folded = read(&writer.to_bytes(), true, None);
    let dataset = to_oxrdf_dataset(&folded)?;

    let has_reifier_projection = dataset.iter().any(|quad| {
        if !quad.graph_name.is_default_graph() || quad.predicate.as_str() != RDF_REIFIES {
            return false;
        }
        let OxTermRef::Triple(triple) = quad.object else {
            return false;
        };
        let triple = triple.as_ref();
        matches!(
            triple.subject,
            NamedOrBlankNodeRef::NamedNode(node)
                if node.as_str() == "https://example.org/subject"
        ) && triple.predicate.as_str() == "https://example.org/predicate"
            && matches!(triple.object, OxTermRef::Literal(literal) if literal.value() == "object")
    });
    assert!(has_reifier_projection, "reifier projection is exported");
    Ok(())
}

#[test]
fn oxrdf_rdf12_triple_terms_import_as_gts_reifiers() -> Result<(), Box<dyn Error>> {
    let mut dataset = Dataset::new();
    let claim = NamedNode::new("https://example.org/claim")?;
    let subject = NamedNode::new("https://example.org/subject")?;
    let predicate = NamedNode::new("https://example.org/predicate")?;
    let object = Literal::new_simple_literal("object");
    let rdf_reifies = NamedNode::new(RDF_REIFIES)?;
    let quoted = OxTriple::new(subject, predicate, object);
    dataset.insert(
        OxQuad::new(
            claim,
            rdf_reifies,
            OxTerm::Triple(Box::new(quoted)),
            GraphName::DefaultGraph,
        )
        .as_ref(),
    );

    let bytes = from_oxrdf_dataset(&dataset)?;
    let folded = read(&bytes, true, None);
    assert_eq!(folded.reifiers.len(), 1);
    let back = to_oxrdf_dataset(&folded)?;
    assert_eq!(sorted_dataset(&back), sorted_dataset(&dataset));
    Ok(())
}

#[test]
fn strict_export_refuses_unrepresentable_quoted_triple_positions() {
    let graph = Graph {
        terms: vec![
            Term {
                kind: TermKind::Iri,
                value: Some("https://example.org/subject".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            Term {
                kind: TermKind::Iri,
                value: Some("https://example.org/predicate".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            Term {
                kind: TermKind::Literal,
                value: Some("object".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            Term {
                kind: TermKind::Triple,
                value: None,
                datatype: None,
                lang: None,
                reifier: Some(3),
            },
        ],
        quads: vec![(0, 1, 2, None), (3, 1, 2, None)],
        reifiers: vec![(3, (0, 1, 2))],
        ..Graph::default()
    };

    let err = to_oxrdf_dataset(&graph).expect_err("strict mode rejects triple subjects");
    assert!(err.detail().contains("quoted triple"));

    let lossy = to_oxrdf_dataset_lossy(&graph).expect("lossy mode drops unsupported row");
    assert_eq!(lossy.len(), 1);
    assert!(lossy
        .iter()
        .all(|quad| quad.predicate.as_str() != RDF_REIFIES));
}

#[test]
fn generated_blank_node_labels_do_not_collide_with_explicit_labels() {
    let graph = Graph {
        terms: vec![
            Term {
                kind: TermKind::Bnode,
                value: Some("b1".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            Term {
                kind: TermKind::Bnode,
                value: None,
                datatype: None,
                lang: None,
                reifier: None,
            },
            Term {
                kind: TermKind::Iri,
                value: Some("https://example.org/predicate".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            Term {
                kind: TermKind::Iri,
                value: Some("https://example.org/object".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
        ],
        quads: vec![(0, 2, 3, None), (1, 2, 3, None)],
        ..Graph::default()
    };

    let dataset = to_oxrdf_dataset(&graph).expect("missing blank node label is generated");
    assert_eq!(dataset.len(), 2);
    let subjects: HashSet<String> = dataset
        .iter()
        .map(|quad| quad.subject.to_string())
        .collect();
    assert_eq!(subjects.len(), 2);
}

#[test]
fn oxrdf_import_rejects_conflicting_reifier_bindings() -> Result<(), Box<dyn Error>> {
    let mut dataset = Dataset::new();
    let claim = NamedNode::new("https://example.org/claim")?;
    let rdf_reifies = NamedNode::new(RDF_REIFIES)?;
    let subject = NamedNode::new("https://example.org/subject")?;
    let predicate = NamedNode::new("https://example.org/predicate")?;

    dataset.insert(
        OxQuad::new(
            claim.clone(),
            rdf_reifies.clone(),
            OxTerm::Triple(Box::new(OxTriple::new(
                subject.clone(),
                predicate.clone(),
                Literal::new_simple_literal("first"),
            ))),
            GraphName::DefaultGraph,
        )
        .as_ref(),
    );
    dataset.insert(
        OxQuad::new(
            claim,
            rdf_reifies,
            OxTerm::Triple(Box::new(OxTriple::new(
                subject,
                predicate,
                Literal::new_simple_literal("second"),
            ))),
            GraphName::DefaultGraph,
        )
        .as_ref(),
    );

    let err = from_oxrdf_dataset(&dataset).expect_err("conflicting reifier binding is rejected");
    assert!(err.detail().contains("conflicting rdf:reifies"));
    Ok(())
}
