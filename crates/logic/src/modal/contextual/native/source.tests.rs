// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn premise(subject: &str, predicate: &str, object: &str, graph: &str) -> RefutationPremise {
    RefutationPremise {
        subject: TermValue::iri(subject),
        predicate: predicate.to_owned(),
        object: TermValue::iri(object),
        graph: Some(TermValue::iri(graph)),
    }
}

#[test]
fn indexed_rows_preserve_statement_layer_partitions() {
    let graph = "urn:graph";
    let selected = "urn:selected";
    let other = "urn:other";
    let reifier = "urn:reifier";
    let sources = BTreeMap::from([(
        graph.to_owned(),
        Arc::from(
            vec![
                premise("urn:first", selected, "urn:one", graph),
                premise("urn:second", other, "urn:two", graph),
                premise(reifier, REIFIES, "urn:quoted", graph),
                premise(reifier, "urn:annotation", "urn:evidence", graph),
            ]
            .into_boxed_slice(),
        ),
    )]);
    let view = SourceView::new(&sources).expect("bounded source view");
    let graph = view.id(&TermValue::iri(graph));
    let selected = view.id(&TermValue::iri(selected));

    let rows: Vec<_> = view
        .quads_for_pattern(None, Some(selected), None, GraphMatch::Named(graph))
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(view.resolve(rows[0].s), Ok(TermRef::Iri("urn:first")));
    assert_eq!(view.quads().count(), 2);
    assert_eq!(view.reifier_quads().count(), 1);
    assert_eq!(view.annotation_quads().count(), 1);
    assert_eq!(view.named_graphs().collect::<Vec<_>>(), vec![graph]);
}
