// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored tiny documentation graphs, no bundle or corpus loader.

use crate::McpView;
use serde_json::json;
use std::sync::Arc;

fn view(body: &str) -> McpView {
    let source = format!(
        r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
<https://blackcatinformatics.ca/gmeow> a <http://www.w3.org/2002/07/owl#Ontology> ;
 <http://purl.org/dc/terms/title> "Tiny" ;
 <http://www.w3.org/2002/07/owl#versionInfo> "test" .
GRAPH <{documentation}> {{ {body} }}
"#,
        documentation = gmeow_bundle_view::graph_iris::GRAPH_DOCUMENTATION
    );
    let dataset = purrdf::parse_dataset(source.as_bytes(), "application/trig", None).unwrap();
    McpView::from_dataset(dataset, Arc::from(Vec::<u8>::new())).unwrap()
}

const QUESTION: &str = r#"
<urn:cq> a g:DocumentedCompetency ; g:documents <urn:term> ;
 g:cqQueryText "SELECT ?answer WHERE { <urn:s> <urn:p> ?answer }" ;
 g:cqExpectRowCount 1 ; g:cqExactRows true ;
 g:cqRationale "same", "same"@en, "same"@fr, "same"@en--ltr, "same"@en--rtl .
"#;

#[test]
fn cq_rationale_mcp_actual_caller_retains_all_native_literal_terms_and_expectations() {
    let view = view(QUESTION);
    let records = view.competency_questions(None).unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["expected_row_count"], 1);
    assert_eq!(record["exact_rows"], true);
    assert!(record.get("rationale").is_none());
    let rationales = record["rationales"].as_array().unwrap();
    assert_eq!(rationales.len(), 5);
    assert!(rationales.contains(&json!({"lexical_form":"same", "datatype":"http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString", "language":"en", "direction":"ltr"})));
    assert!(rationales.contains(&json!({"lexical_form":"same", "datatype":"http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString", "language":"en", "direction":"rtl"})));
    assert!(rationales.contains(&json!({"lexical_form":"same", "datatype":"http://www.w3.org/1999/02/22-rdf-syntax-ns#langString", "language":"fr", "direction":null})));
    assert_eq!(
        records,
        view.competency_questions(Some("urn:term")).unwrap()
    );
    assert!(
        view.competency_questions(Some("urn:absent"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(records, view.competency_questions(None).unwrap());
    let dataset =
        purrdf::parse_dataset(b"<urn:s> <urn:p> <urn:answer> .", "text/turtle", None).unwrap();
    let result =
        gmeow_bundle_view::native_query::query(&dataset, record["query_text"].as_str().unwrap())
            .unwrap();
    let purrdf::SparqlResult::Solutions { rows, .. } = result else {
        panic!("SELECT required")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], Some(purrdf::TermValue::iri("urn:answer")));
}

#[test]
fn cq_rationale_mcp_actual_caller_refuses_malformed_and_conflicting_fields() {
    for source in [
        QUESTION.replace(r#""same"@fr"#, "<urn:not-literal>"),
        QUESTION.replace("g:cqExpectRowCount 1", "g:cqExpectRowCount \"one\""),
        QUESTION.replace("g:cqExactRows true", "g:cqExactRows \"true\""),
        QUESTION.replace("g:cqExactRows true", "g:cqExactRows true, false"),
        QUESTION.replace("g:cqExpectRowCount 1", "g:cqExpectRowCount 1, 2"),
        QUESTION.replace(
            r#"g:cqQueryText "SELECT ?answer WHERE { <urn:s> <urn:p> ?answer }""#,
            "g:cqQueryText <urn:not-query-text>",
        ),
        QUESTION.replace(
            r#"g:cqQueryText "SELECT ?answer WHERE { <urn:s> <urn:p> ?answer }""#,
            r#"g:cqQueryText "SELECT ?answer WHERE { <urn:s> <urn:p> ?answer }", "ASK {}""#,
        ),
    ] {
        let error = view(&source).competency_questions(None).unwrap_err();
        assert!(
            error.to_string().contains("competency_questions"),
            "{error}"
        );
    }
}

#[test]
fn cq_rationale_mcp_absence_and_reordered_duplicate_terms_are_deterministic() {
    let without = QUESTION.replace(
        r#" ;
 g:cqRationale "same", "same"@en, "same"@fr, "same"@en--ltr, "same"@en--rtl"#,
        "",
    );
    assert_eq!(
        view(&without).competency_questions(None).unwrap()[0]["rationales"],
        json!([])
    );
    let no_expectations = without.replace(
        "g:cqExpectRowCount 1 ; g:cqExactRows true",
        "g:cqRationale \"prose\"",
    );
    let record = view(&no_expectations)
        .competency_questions(None)
        .unwrap()
        .remove(0);
    assert!(record.get("expected_row_count").is_none());
    assert!(record.get("exact_rows").is_none());
    let reordered = QUESTION.replace(
        r#""same", "same"@en, "same"@fr, "same"@en--ltr, "same"@en--rtl"#,
        r#""same"@en--rtl, "same"@en--ltr, "same"@fr, "same"@en, "same", "same"@en"#,
    );
    assert_eq!(
        view(QUESTION).competency_questions(None).unwrap(),
        view(&reordered).competency_questions(None).unwrap()
    );
}
