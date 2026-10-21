// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete authored spec and tiny native data only. The repository-level
//! run_competency_file corpus boundary is deliberately not invoked.

use super::*;

const SPEC: &str = r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
<urn:ask> a g:CompetencyQuestion ; g:cqQuery "ASK { <urn:item> <urn:value> <urn:answer> }" ;
 g:cqExpectAsk true ; g:cqRationale "same", "same"@x-gmeow-english, "same"@fr, "same"@en--ltr, "same"@ar--rtl .
<urn:select> a g:CompetencyQuestion ; g:cqQuery "SELECT ?answer WHERE { <urn:item> <urn:value> ?answer }" ;
 g:cqExactRows true ; g:cqExpectRow <urn:row> ; g:cqRationale "answer must be retained"@en, "la réponse doit être conservée"@fr .
<urn:row> g:rowCell [ g:cellVar "answer" ; g:cellValueIri <urn:answer> ] .
"#;

#[test]
fn cq_rationale_slicetest_preserves_all_terms_without_conflicting_scalar_definitions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authored-spec.ttl");
    std::fs::write(&path, SPEC).unwrap();
    let spec = crate::dsl::load_spec(&path).unwrap();
    assert_eq!(spec.competency.len(), 2);
    let ask = spec
        .competency
        .iter()
        .find(|question| question.iri == "urn:ask")
        .unwrap();
    assert_eq!(ask.rationales.len(), 5);
    assert!(
        ask.rationales
            .iter()
            .any(|literal| literal.language.as_deref() == Some("x-gmeow-english"))
    );
    assert!(
        ask.rationales
            .iter()
            .any(|literal| literal.direction == Some(purrdf::RdfTextDirection::Ltr))
    );
    assert!(
        ask.rationales
            .iter()
            .any(|literal| literal.direction == Some(purrdf::RdfTextDirection::Rtl))
    );
    let reordered = SPEC.replace(
        r#""same", "same"@x-gmeow-english, "same"@fr, "same"@en--ltr, "same"@ar--rtl"#,
        r#""same"@ar--rtl, "same"@en--ltr, "same"@fr, "same"@x-gmeow-english, "same""#,
    );
    std::fs::write(&path, reordered).unwrap();
    assert_eq!(
        crate::dsl::load_spec(&path).unwrap().competency,
        spec.competency
    );
}

#[test]
fn cq_rationale_slicetest_actual_ask_and_select_results_are_unchanged_by_rationale_multiplicity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authored-spec.ttl");
    std::fs::write(&path, SPEC).unwrap();
    let spec = crate::dsl::load_spec(&path).unwrap();
    let dataset = purrdf::parse_dataset(
        b"<urn:item> <urn:value> <urn:answer> .",
        "text/turtle",
        None,
    )
    .unwrap();
    for question in &spec.competency {
        run_competency_cell(&dataset, question, directory.path()).unwrap();
        let mut without_prose = question.clone();
        without_prose.rationales.clear();
        run_competency_cell(&dataset, &without_prose, directory.path()).unwrap();
    }
    assert_eq!(
        spec.competency
            .iter()
            .find(|question| question.iri == "urn:select")
            .unwrap()
            .expected_rows[0]
            .cells[0]
            .value,
        purrdf::TermValue::iri("urn:answer")
    );
}

#[test]
fn cq_rationale_slicetest_nonliteral_values_refuse_and_query_conflicts_remain_errors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authored-spec.ttl");
    for source in [
        SPEC.replace(r#""answer must be retained"@en"#, "<urn:not-literal>"),
        SPEC.replace(r#"g:cqExpectAsk true"#, "g:cqExpectAsk true, false"),
    ] {
        std::fs::write(&path, source).unwrap();
        assert!(crate::dsl::load_spec(&path).is_err());
    }
}
