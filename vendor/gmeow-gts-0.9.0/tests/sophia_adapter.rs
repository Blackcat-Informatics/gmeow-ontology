// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg(feature = "sophia-adapter")]

use std::error::Error;

use gmeow_gts::model::{Graph, Term as GtsTerm, TermKind};
use gmeow_gts::reader::read;
use gmeow_gts::sophia::{from_sophia_dataset, to_sophia_dataset};
use gmeow_gts::writer::Writer;
use sophia_api::dataset::{Dataset, MutableDataset};
use sophia_api::ns::Namespace;
use sophia_api::serializer::{QuadSerializer, Stringifier};
use sophia_api::term::{BnodeId, LanguageTag, SimpleTerm, Term as SophiaTerm};
use sophia_inmem::dataset::LightDataset;
use sophia_turtle::serializer::nq::NQuadsSerializer;

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

fn sorted_dataset<D: Dataset>(dataset: &D) -> Result<Vec<String>, Box<dyn Error>> {
    let mut serializer = NQuadsSerializer::new_stringifier();
    serializer.serialize_dataset(dataset)?;
    let mut rows = serializer
        .as_str()
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    rows.sort();
    Ok(rows)
}

#[test]
fn sophia_dataset_roundtrips_iris_bnodes_literals_and_named_graphs() -> Result<(), Box<dyn Error>> {
    let mut dataset = LightDataset::new();
    let ex = Namespace::new("https://example.org/")?;
    let rdfs = Namespace::new("http://www.w3.org/2000/01/rdf-schema#")?;
    let xsd = Namespace::new("http://www.w3.org/2001/XMLSchema#")?;
    let graph = ex.get("graph")?;
    let cat = ex.get("Cat")?;
    let friend = BnodeId::new_unchecked("friend");
    let label = rdfs.get("label")?;
    let lives = ex.get("lives")?;
    let integer = xsd.get("integer")?;
    let language = LanguageTag::new_unchecked("en".into());

    MutableDataset::insert(
        &mut dataset,
        cat,
        label,
        SimpleTerm::LiteralLanguage("Cat".into(), language, None),
        Some(&graph),
    )?;
    MutableDataset::insert(
        &mut dataset,
        cat,
        lives,
        SimpleTerm::LiteralDatatype(
            "9".into(),
            integer.iri().expect("xsd:integer is an IRI term"),
        ),
        Some(&graph),
    )?;
    MutableDataset::insert(
        &mut dataset,
        friend,
        label,
        "Friend",
        None as Option<&SimpleTerm<'static>>,
    )?;

    let bytes = from_sophia_dataset(&dataset)?;
    let folded = read(&bytes, true, None);
    assert!(folded.diagnostics.is_empty());
    let back = to_sophia_dataset(&folded)?;

    assert_eq!(sorted_dataset(&back)?, sorted_dataset(&dataset)?);
    Ok(())
}

#[test]
fn gts_reifier_projection_uses_sophia_rdf12_triple_terms() -> Result<(), Box<dyn Error>> {
    let mut writer = Writer::new("dist");
    writer.add_terms(&[
        GtsTerm {
            kind: TermKind::Iri,
            value: Some("https://example.org/claim".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        GtsTerm {
            kind: TermKind::Iri,
            value: Some("https://example.org/subject".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        GtsTerm {
            kind: TermKind::Iri,
            value: Some("https://example.org/predicate".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        GtsTerm {
            kind: TermKind::Literal,
            value: Some("object".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    writer.add_reifies(&[(0, (1, 2, 3))]);

    let folded = read(&writer.to_bytes(), true, None);
    let dataset = to_sophia_dataset(&folded)?;
    let rows = sorted_dataset(&dataset)?;

    assert!(rows.iter().any(|row| row.contains(RDF_REIFIES)));
    assert!(rows.iter().any(|row| row.contains("<<(")));

    let bytes = from_sophia_dataset(&dataset)?;
    let back = read(&bytes, true, None);
    assert_eq!(back.reifiers.len(), 1);
    assert_eq!(sorted_dataset(&to_sophia_dataset(&back)?)?, rows);
    Ok(())
}

#[test]
fn strict_sophia_export_rejects_unrepresentable_quoted_triple_graph_names() {
    let graph = Graph {
        terms: vec![
            GtsTerm {
                kind: TermKind::Iri,
                value: Some("https://example.org/subject".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            GtsTerm {
                kind: TermKind::Iri,
                value: Some("https://example.org/predicate".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            GtsTerm {
                kind: TermKind::Literal,
                value: Some("object".to_string()),
                datatype: None,
                lang: None,
                reifier: None,
            },
            GtsTerm {
                kind: TermKind::Triple,
                value: None,
                datatype: None,
                lang: None,
                reifier: Some(3),
            },
        ],
        quads: vec![(0, 1, 2, Some(3))],
        reifiers: vec![(3, (0, 1, 2))],
        ..Graph::default()
    };

    let err = to_sophia_dataset(&graph)
        .expect_err("RDF 1.2 N-Quads forbids quoted triples as graph names");
    assert!(err.detail().contains("Sophia N-Quads parse failed"));
}
