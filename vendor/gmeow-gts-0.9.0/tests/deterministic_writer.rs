// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs;
use std::path::Path;

use ciborium::value::Value;
use gmeow_gts::model::{Graph, Suppression, Term, TermKind};
use gmeow_gts::writer::{digest_string, Writer};

const CAT: &str = "https://example.org/Cat";
const LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

fn iri(value: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn literal(value: &str, lang: Option<&str>) -> Term {
    Term {
        kind: TermKind::Literal,
        value: Some(value.to_string()),
        datatype: None,
        lang: lang.map(str::to_string),
        reifier: None,
    }
}

fn blob_meta() -> Value {
    Value::Map(vec![("mt".into(), "text/plain".into())])
}

fn term_target(id: usize) -> Value {
    Value::Map(vec![
        ("kind".into(), "term".into()),
        ("id".into(), Value::from(id as u64)),
    ])
}

fn deterministic_graphs() -> (Graph, Graph) {
    let payload = b"deterministic payload";
    let digest = digest_string(payload);

    let mut a = Graph {
        terms: vec![
            iri(LABEL),
            literal("Cat", Some("en")),
            iri(CAT),
            iri("https://example.org/graph"),
            iri("https://example.org/stmt1"),
            iri("https://example.org/confidence"),
            literal("0.9", None),
        ],
        quads: vec![(2, 0, 1, Some(3))],
        reifiers: vec![(4, (2, 0, 1))],
        annotations: vec![(4, 5, 6)],
        suppressions: vec![Suppression {
            targets: vec![term_target(6)],
            reason: Some("example suppression".to_string()),
            by: Some(4),
        }],
        ..Graph::default()
    };
    a.set_meta("generator".to_string(), "deterministic-writer".into());
    a.set_blob(digest.clone(), payload.to_vec());
    a.set_blob_meta(digest.clone(), blob_meta());

    let mut b = Graph {
        terms: vec![
            literal("0.9", None),
            iri("https://example.org/confidence"),
            iri("https://example.org/stmt1"),
            iri("https://example.org/graph"),
            iri(CAT),
            literal("Cat", Some("en")),
            iri(LABEL),
        ],
        quads: vec![(4, 6, 5, Some(3))],
        reifiers: vec![(2, (4, 6, 5))],
        annotations: vec![(2, 1, 0)],
        suppressions: vec![Suppression {
            targets: vec![term_target(0)],
            reason: Some("example suppression".to_string()),
            by: Some(2),
        }],
        ..Graph::default()
    };
    b.set_meta("generator".to_string(), "deterministic-writer".into());
    b.set_blob(digest.clone(), payload.to_vec());
    b.set_blob_meta(digest, blob_meta());

    (a, b)
}

#[test]
fn deterministic_writer_reorders_equivalent_graphs() {
    let (a, b) = deterministic_graphs();
    let first = Writer::deterministic(&a, "dist")
        .expect("graph writes")
        .to_bytes();
    let second = Writer::deterministic(&b, "dist")
        .expect("graph writes")
        .to_bytes();
    assert_eq!(first, second);
}

#[test]
fn deterministic_writer_matches_frozen_vector() {
    let (graph, _) = deterministic_graphs();
    let actual = Writer::deterministic(&graph, "dist")
        .expect("graph writes")
        .to_bytes();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors");
    let frozen = fs::read(dir.join("29-deterministic-writer.gts")).expect("vector bytes");
    assert_eq!(actual, frozen);
}
