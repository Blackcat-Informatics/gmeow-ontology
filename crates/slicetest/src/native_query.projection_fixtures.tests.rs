// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Standalone authored examples; these controls neither load nor produce the corpus.

use super::*;
use purrdf::DatasetView;

#[test]
fn language_status_query_retains_its_bitemporal_answer_with_native_reifiers() {
    let data = dataset_from_turtle(include_str!(
        "../../../slices/extensions/languages/tests/fixtures/language-state-bitemporal.ttl"
    ))
    .unwrap();
    let specification = dataset_from_turtle(include_str!(
        "../../../slices/extensions/languages/tests/competency.ttl"
    ))
    .unwrap();
    let view = gmeow_logic_compile::ingest::DslView::new(&specification);
    let query = view.object_literal(
        "https://blackcatinformatics.ca/gmeow/examples/languages/tests/cqLanguageStateBitemporal",
        "https://blackcatinformatics.ca/gmeow/cqQuery",
    ).expect("the language-status competency question declares its query");
    let rows = select(&data, &query).unwrap();
    assert_eq!(
        rows.rows,
        vec![vec![Some(TermValue::Iri(
            "https://example.org/lang/middleEnglishState".into()
        ))]]
    );
}

#[test]
fn similarity_query_rejects_canonical_and_projected_identity_overclaims() {
    let query = include_str!(
        "../../../slices/extensions/embedding-projection/queries/competency/similarity-not-equivalence.rq"
    );
    for namespace in [
        "https://blackcatinformatics.ca/logic/",
        "http://www.w3.org/2002/07/owl#",
    ] {
        let data = dataset_from_turtle(&format!(
            "@prefix ex: <https://example.org/> .
             @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
             @prefix v: <{namespace}> .
             ex:clean a gmeow:SimilarityObservation .
             ex:identity a gmeow:SimilarityObservation ; v:sameAs ex:x .
             ex:x v:equivalentClass ex:identity ."
        ))
        .unwrap();
        let rows = select(&data, query).unwrap();
        assert_eq!(
            rows.rows,
            vec![vec![Some(TermValue::Iri(
                "https://example.org/identity".into()
            ))]],
            "{namespace}"
        );
    }
}

#[test]
fn grounded_claim_example_does_not_assert_the_hallucinated_closing_time() {
    let data = dataset_from_turtle(include_str!(
        "../../../slices/core/ai/examples/grounded-claim.ttl"
    ))
    .unwrap();
    let view = gmeow_logic_compile::ingest::DslView::new(&data);
    let closing_claim = view.reified_statements().find(|statement| {
        matches!(statement.reifier(), purrdf::TermRef::Iri(iri) if iri == "https://blackcatinformatics.ca/gmeow/examples/ai/stmt-2300")
    }).expect("the hallucinated proposition retains its named reifier");
    let (subject, predicate, object) = closing_claim.triple().unwrap();
    let purrdf::TermRef::Iri(subject) = subject else {
        panic!("named observed subject")
    };
    let purrdf::TermRef::Iri(predicate) = predicate else {
        panic!("named observed predicate")
    };
    let assertions: Vec<_> = data
        .quads_for_pattern(
            data.term_id_by_iri(subject),
            data.term_id_by_iri(predicate),
            None,
            purrdf::GraphMatch::Any,
        )
        .collect();
    assert_eq!(
        assertions.len(),
        1,
        "only the grounded closing time is asserted"
    );
    assert!(
        !assertions
            .iter()
            .any(|quad| data.as_ref().resolve(quad.o) == object),
        "reifying the hallucination must not assert its truth"
    );
}
