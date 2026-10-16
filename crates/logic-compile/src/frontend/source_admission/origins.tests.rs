// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::frontend::{SourceCarrier, SourceDocument, SourceNodeBinding, SourceOccurrencePosition};

fn receipt(
    path: &str,
    nodes: &[SourceNode],
    statements: &[SourceStatementBinding],
) -> SourceDocumentOccurrences {
    let mut statements = statements.to_vec();
    statements.sort();
    SourceDocumentOccurrences {
        document: SourceDocument {
            path: path.into(),
            content_digest: "synthetic occurrence receipt".into(),
            role: "synthetic-test".into(),
            base: None,
        },
        bindings: nodes
            .iter()
            .flat_map(|&node| {
                [
                    SourceOccurrencePosition::Subject,
                    SourceOccurrencePosition::Object,
                ]
                .map(|position| SourceNodeBinding {
                    original: node,
                    canonical: node,
                    position,
                })
            })
            .collect(),
        statements,
    }
}

#[test]
/// Original contribution semantics survive shared rows, carriers, graph placement,
/// repeated positions and repeated portable paths; absent/anonymous sources stay empty.
fn origin_index_matches_document_scans_without_merging_graphs_or_carriers() {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let s = builder.intern_iri("urn:subject").unwrap();
    let p = builder.intern_iri("urn:predicate").unwrap();
    let o = builder.intern_iri("urn:object").unwrap();
    let graph = builder.intern_iri("urn:graph").unwrap();
    let missing = builder.intern_iri("urn:missing").unwrap();
    let node = SourceNode {
        term: s,
        graph: None,
    };
    let named_node = SourceNode {
        term: s,
        graph: Some(graph),
    };
    let statement = SourceStatementBinding {
        canonical: purrdf::QuadIds { s, p, o, g: None },
        carrier: SourceCarrier::Statement,
    };
    let annotation = SourceStatementBinding {
        carrier: SourceCarrier::Annotation,
        ..statement
    };
    let named = SourceStatementBinding {
        canonical: purrdf::QuadIds {
            g: Some(graph),
            ..statement.canonical
        },
        ..statement
    };
    let origins = vec![
        receipt("z.ttl", &[node], &[statement, annotation]),
        receipt("a.ttl", &[node], &[statement, statement]),
        receipt("named.ttl", &[named_node], &[named]),
        receipt("a.ttl", &[node], &[statement]),
    ];
    let index = OriginIndex::new(&origins);
    for query in [
        node,
        named_node,
        SourceNode {
            term: missing,
            graph: None,
        },
    ] {
        let scanned: BTreeSet<_> = origins
            .iter()
            .filter(|origin| origin.bindings.iter().any(|b| b.canonical == query))
            .map(|origin| origin.document.path.clone())
            .collect();
        assert_eq!(index.node_documents(query), scanned);
    }
    for query in [
        statement,
        annotation,
        named,
        SourceStatementBinding {
            carrier: SourceCarrier::Reifier,
            ..statement
        },
    ] {
        let scanned: BTreeSet<_> = origins
            .iter()
            .filter(|origin| origin.statements.binary_search(&query).is_ok())
            .map(|origin| origin.document.path.clone())
            .collect();
        assert_eq!(index.statement_documents(query), scanned);
    }
    assert_eq!(index.nodes[&node], vec![0, 1, 3]);
    assert_eq!(index.statements[&statement], vec![0, 1, 3]);
    assert_eq!(
        index.statement_documents(annotation),
        BTreeSet::from(["z.ttl".into()])
    );
    assert_eq!(
        index.node_documents(named_node),
        BTreeSet::from(["named.ttl".into()])
    );
    let anonymous = OriginIndex::new(&[]);
    assert!(anonymous.node_documents(node).is_empty());
    assert!(anonymous.statement_documents(statement).is_empty());
}
