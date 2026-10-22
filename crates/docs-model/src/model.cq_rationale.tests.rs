// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny authored CQ source through the actual extractor, model and RDF writer.
//! No discovery, authenticated fixture, source corpus or producer is invoked.

use super::*;

const CQ: &str = r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
<urn:authored:cq> a g:CompetencyQuestion ;
 g:cqRationale "same", "same"@x-gmeow-english, "same"@fr, "same"@en--ltr, "same"@ar--rtl ;
 g:cqQuery "SELECT ?item WHERE { ?item <urn:p> <urn:o> }" ; g:cqExactRows true ;
 g:cqExpectRow <urn:row> .
<urn:row> g:rowCell [ g:cellVar "item" ; g:cellValueIri <urn:term> ] .
"#;

fn competencies(source: &str) -> Vec<DocCompetency> {
    extract_competency(
        &Store::parse_turtle(source.as_bytes()).unwrap(),
        "urn:owner",
    )
    .unwrap()
}

#[test]
fn cq_rationale_model_retains_coexisting_term_identities_and_query_result_metadata() {
    let questions = competencies(CQ);
    assert_eq!(questions.len(), 1);
    let question = &questions[0];
    assert_eq!(question.rationales.len(), 5);
    assert!(
        question
            .rationales
            .iter()
            .all(|value| value.lexical_form == "same")
    );
    assert!(
        question
            .rationales
            .iter()
            .any(|value| value.language.as_deref() == Some("x-gmeow-english"))
    );
    assert!(
        question
            .rationales
            .iter()
            .any(|value| value.language.as_deref() == Some("fr"))
    );
    assert!(
        question
            .rationales
            .iter()
            .any(|value| value.direction == Some(purrdf::RdfTextDirection::Ltr))
    );
    assert!(
        question
            .rationales
            .iter()
            .any(|value| value.direction == Some(purrdf::RdfTextDirection::Rtl))
    );
    assert_eq!(
        question.query_text.as_deref(),
        Some("SELECT ?item WHERE { ?item <urn:p> <urn:o> }")
    );
    assert_eq!(question.expected_row_count, None);
    assert_eq!(question.exact_rows, Some(true));
    assert_eq!(
        question.expected_rows[0].cells[0].var.as_deref(),
        Some("item")
    );
    assert_eq!(
        question.expected_rows[0].cells[0].value_iri.as_deref(),
        Some("urn:term")
    );
    assert_eq!(question.exercises, ["urn:term"]);
    let reordered = CQ.replace(
        r#""same", "same"@x-gmeow-english, "same"@fr, "same"@en--ltr, "same"@ar--rtl"#,
        r#""same"@ar--rtl, "same"@en--ltr, "same"@fr, "same"@x-gmeow-english, "same", "same"@fr"#,
    );
    assert_eq!(competencies(&reordered), questions);
}

#[test]
fn cq_rationale_model_cache_record_round_trip_retains_language_datatype_and_direction() {
    let question = competencies(CQ).remove(0);
    let wire = serde_json::to_vec(&question).unwrap();
    let restored: DocCompetency = serde_json::from_slice(&wire).unwrap();
    assert_eq!(restored, question);
    assert_eq!(serde_json::to_vec(&restored).unwrap(), wire);
    let model = DocsModel {
        competencies: vec![question],
        ..DocsModel::default()
    };
    let wire = serde_json::to_vec(&model.competencies).unwrap();
    let restored: Vec<DocCompetency> = serde_json::from_slice(&wire).unwrap();
    assert_eq!(restored, model.competencies);
}

#[test]
fn cq_rationale_model_rdf_uses_complete_native_literal_syntax_without_retagging() {
    let model = DocsModel {
        competencies: competencies(CQ),
        ..DocsModel::default()
    };
    let rdf = crate::rdf::to_gmeow_rdf(&model, &BTreeMap::new());
    let dataset = purrdf::parse_dataset(rdf.as_bytes(), "application/n-quads", None).unwrap();
    let mut literals: Vec<_> = dataset
        .owned_quads()
        .filter(|quad| quad.predicate == GMEOW_CQ_RATIONALE)
        .map(|quad| {
            let purrdf::RdfTerm::Literal(literal) = quad.object else {
                panic!("rationale must remain a literal")
            };
            let mut term = String::new();
            purrdf_core::write_term_value(
                &purrdf::TermValue::from_rdf_term(&purrdf::RdfTerm::Literal(literal)),
                &mut term,
            );
            term
        })
        .collect();
    literals.sort();
    assert_eq!(
        literals,
        [
            r#""same""#,
            r#""same"@ar--rtl"#,
            r#""same"@en--ltr"#,
            r#""same"@fr"#,
            r#""same"@x-gmeow-english"#
        ]
    );
}

#[test]
fn cq_rationale_model_distinguishes_absence_from_empty_prose_and_refuses_nonliteral_loss() {
    let absent = r#"<urn:cq> a <https://blackcatinformatics.ca/gmeow/CompetencyQuestion> ."#;
    assert!(competencies(absent)[0].rationales.is_empty());
    let empty = format!("{absent} <urn:cq> <{GMEOW_CQ_RATIONALE}> \"\" .");
    let question = competencies(&empty).remove(0);
    assert_eq!(question.rationales, [purrdf::RdfLiteral::simple("")]);
    let wrong = format!("{absent} <urn:cq> <{GMEOW_CQ_RATIONALE}> <urn:not-literal> .");
    let store = Store::parse_turtle(wrong.as_bytes()).unwrap();
    let error = extract_competency(&store, "urn:owner")
        .unwrap_err()
        .to_string();
    assert!(error.contains("urn:cq"));
    assert!(error.contains("cqRationale"));
}

#[test]
fn cq_rationale_coverage_reads_all_values_instead_of_a_single_lexical_winner() {
    let term = DocTerm {
        iri: "urn:term".to_owned(),
        ..DocTerm::default()
    };
    let model = DocsModel {
        competencies: vec![DocCompetency {
            rationales: vec![
                purrdf::RdfLiteral::simple(""),
                purrdf::RdfLiteral::language_tagged("A genuine modelling reason", "fr"),
                purrdf::RdfLiteral::simple("Mirrors test_internal"),
            ],
            exercises: vec![term.iri.clone()],
            ..DocCompetency::default()
        }],
        ..DocsModel::default()
    };
    let context = crate::coverage::CoverageContext::new(&model);
    let coverage = crate::coverage::term_coverage(&term, &context);
    assert!(coverage.has_competency_rationale);
    assert!(!coverage.has_provenance_honesty);
}
