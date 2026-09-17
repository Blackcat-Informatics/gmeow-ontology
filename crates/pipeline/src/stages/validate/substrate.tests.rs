// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn validation_substrate_keeps_native_evidence_inside_its_exact_role() {
    let mut builder = RdfDatasetBuilder::new();
    let selected = builder.intern_iri(&format!("{SUBSTRATE_IRI_PREFIX}selected"));
    let unrelated = builder.intern_iri("https://example.org/unrelated");
    let predicate = builder.intern_iri("https://example.org/evidence");
    let graph = builder.intern_iri(GRAPH_PROVENANCE);
    let outside = builder.intern_iri("https://example.org/outside");
    let literal = builder.intern_literal(purrdf::RdfLiteral {
        lexical_form: "witness".into(),
        datatype: None,
        language: Some("ar".into()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    });
    let quote = builder.intern_triple(unrelated, predicate, literal);
    builder.push_quad(selected, predicate, literal, Some(graph));
    builder.push_reifier_in_graph(selected, quote, Some(graph));
    builder.push_annotation_in_graph(selected, predicate, quote, Some(graph));
    builder.push_quad(unrelated, predicate, literal, Some(graph));
    builder.push_quad(selected, predicate, outside, Some(outside));
    builder.push_annotation(selected, predicate, outside);
    let source = builder.freeze().unwrap();
    let projected = project(&source).unwrap();
    assert_eq!(projected.quad_count(), 1);
    assert_eq!(projected.reifier_quads().count(), 1);
    assert_eq!(projected.annotation_quads().count(), 1);
    assert!(projected.named_graphs().next().is_none());
    assert!(
        projected
            .term_id_by_iri("https://example.org/outside")
            .is_none()
    );
    let actual = projected.quads().next().unwrap();
    assert_eq!(projected.term_value(actual.o), source.term_value(literal));
    let actual_quote = projected.reifier_quads().next().unwrap().o;
    assert_eq!(projected.term_value(actual_quote), source.term_value(quote));
    assert!(
        projected
            .quads()
            .all(|quad| matches!(projected.resolve(quad.s),
            TermRef::Iri(iri) if iri.starts_with(SUBSTRATE_IRI_PREFIX)))
    );
}
