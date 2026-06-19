// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Folded graph quad iterator APIs.

use gmeow_gts::model::{Graph, Quad, Term, TermKind};

fn iri(value: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        reifier: None,
    }
}

fn sample_graph() -> Graph {
    Graph {
        terms: vec![
            iri("https://example.org/s"),
            iri("https://example.org/p"),
            iri("https://example.org/o"),
            iri("https://example.org/g"),
        ],
        quads: vec![(0, 1, 2, None), (0, 1, 2, Some(3))],
        ..Graph::default()
    }
}

#[test]
fn into_quads_yields_existing_quad_rows() {
    let graph = sample_graph();
    let expected: Vec<Quad> = graph.quads.clone();

    assert_eq!(graph.into_quads().collect::<Vec<_>>(), expected);

    let graph = sample_graph();
    assert_eq!(graph.into_iter().collect::<Vec<_>>(), expected);
}

#[test]
fn quad_terms_resolves_term_references_lazily() {
    let graph = sample_graph();
    let mut rows = graph.quad_terms();

    let default = rows.next().unwrap();
    assert_eq!(
        default.subject.value.as_deref(),
        Some("https://example.org/s")
    );
    assert_eq!(
        default.predicate.value.as_deref(),
        Some("https://example.org/p")
    );
    assert_eq!(
        default.object.value.as_deref(),
        Some("https://example.org/o")
    );
    assert!(default.graph_name.is_none());

    let named = rows.next().unwrap();
    assert_eq!(
        named.graph_name.unwrap().value.as_deref(),
        Some("https://example.org/g")
    );
    assert!(rows.next().is_none());
}
