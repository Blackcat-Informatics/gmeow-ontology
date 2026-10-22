// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny authored graphs/results only. No bundle loader, corpus or producer.

use std::sync::Arc;

use purrdf::{BlankScope, RdfDatasetBuilder, SparqlResult, TermBox, TermValue};
use purrdf_lex::json::{Value, record::read_document};
use serde_json::json;

use super::envelope;
use crate::{Extension, McpServer, McpView, QueryScope, SegmentSet};

fn view(body: &str) -> McpView {
    let source = format!(
        r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
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

fn server(body: &str) -> McpServer {
    McpServer::from_admitted_view(
        Arc::new(view(body)),
        SegmentSet::linked(),
        Extension::default(),
        None,
    )
    .unwrap()
}

fn parse(text: &str) -> Value {
    read_document(text.as_bytes()).unwrap()
}

fn solutions(variables: &[&str], rows: Vec<Vec<Option<TermValue>>>) -> SparqlResult {
    SparqlResult::Solutions {
        variables: variables.iter().map(|name| (*name).to_owned()).collect(),
        rows,
        aux: RdfDatasetBuilder::new().freeze().unwrap().into(),
    }
}

#[test]
fn query_transport_docs_and_local_retain_language_direction_and_literal_lexemes() {
    let body = r#"<urn:s> <urn:p> "same", "same"@fr, "same"@en--ltr, "same"@ar--rtl, "01"^^<http://www.w3.org/2001/XMLSchema#integer> ."#;
    let view = view(body);
    let query = "SELECT ?o WHERE { <urn:s> <urn:p> ?o } ORDER BY ?o";
    let docs = view.run_docs_query(query).unwrap();
    let local = view
        .run_local_query(body, "turtle", query, QueryScope::InputOnly)
        .unwrap();
    assert_eq!(docs, local);
    assert!(
        docs.get("purrdf").is_none(),
        "the query API supplies no result provenance carrier"
    );
    let bindings = docs["results"]["bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 5);
    let terms: Vec<_> = bindings.iter().map(|row| row["o"].clone()).collect();
    for expected in [
        r#"{"type":"literal","value":"same"}"#,
        r#"{"type":"literal","value":"same","xml:lang":"fr"}"#,
        r#"{"type":"literal","value":"same","xml:lang":"en","its:dir":"ltr"}"#,
        r#"{"type":"literal","value":"same","xml:lang":"ar","its:dir":"rtl"}"#,
        r#"{"type":"literal","value":"01","datatype":"http://www.w3.org/2001/XMLSchema#integer"}"#,
    ] {
        assert!(
            terms.contains(&parse(expected)),
            "missing {expected}: {docs}"
        );
    }
}

#[test]
fn query_transport_docs_and_local_preserve_recursive_triple_terms() {
    let body = r#"<urn:s> <urn:p> <<( <urn:a> <urn:b> <<( <urn:c> <urn:d> "نص"@ar--rtl )>> )>> ."#;
    let view = view(body);
    let query = "SELECT ?o WHERE { <urn:s> <urn:p> ?o }";
    let docs = view.run_docs_query(query).unwrap();
    assert_eq!(
        docs,
        view.run_local_query(body, "turtle", query, QueryScope::InputOnly)
            .unwrap()
    );
    let term = &docs["results"]["bindings"][0]["o"];
    assert_eq!(term["type"].as_str(), Some("triple"));
    assert_eq!(term["value"]["subject"]["value"].as_str(), Some("urn:a"));
    let nested = &term["value"]["object"];
    assert_eq!(nested["type"].as_str(), Some("triple"));
    assert_eq!(nested["value"]["object"]["its:dir"].as_str(), Some("rtl"));
    assert_eq!(nested["value"]["object"]["value"].as_str(), Some("نص"));
}

#[test]
fn query_transport_scoped_blanks_remain_distinct_through_actual_docs_query() {
    let mut builder = RdfDatasetBuilder::new();
    let graph = builder
        .intern_iri(gmeow_bundle_view::graph_iris::GRAPH_DOCUMENTATION)
        .unwrap();
    let predicate = builder.intern_iri("urn:p").unwrap();
    for scope in [BlankScope(1), BlankScope(9)] {
        let subject = builder.intern_blank("same", scope).unwrap();
        builder
            .push_quad(subject, predicate, subject, Some(graph))
            .unwrap();
    }
    let header = view("");
    let authored = builder.freeze().unwrap();
    let dataset = purrdf::RdfDataset::union(&[header.dataset.as_ref(), &authored]).unwrap();
    let view = McpView::from_dataset(Arc::new(dataset), Arc::from(Vec::<u8>::new())).unwrap();
    let value = view
        .run_docs_query("SELECT ?s ?o WHERE { ?s <urn:p> ?o } ORDER BY ?s")
        .unwrap();
    let rows = value["results"]["bindings"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0]["s"]["value"], rows[1]["s"]["value"]);
    for row in rows {
        assert_eq!(row["s"]["type"].as_str(), Some("bnode"));
        assert_eq!(row["s"], row["o"]);
    }
}

#[test]
fn query_transport_empty_unbound_and_boolean_envelopes_keep_declared_shapes() {
    let view = view("<urn:s> <urn:p> <urn:o> .");
    let empty = view
        .run_docs_query("SELECT ?missing WHERE { <urn:absent> <urn:p> ?missing }")
        .unwrap();
    assert_eq!(
        empty,
        parse(
            r#"{"ok":true,"form":"bindings","head":{"vars":["missing"]},"results":{"bindings":[]}}"#
        )
    );
    let unbound = view
        .run_docs_query(
            "SELECT ?s ?missing WHERE { ?s <urn:p> <urn:o> OPTIONAL { ?s <urn:absent> ?missing } }",
        )
        .unwrap();
    assert_eq!(unbound["head"]["vars"], parse(r#"["s","missing"]"#));
    assert!(unbound["results"]["bindings"][0].get("missing").is_none());
    assert_eq!(
        view.query_docs_json("ASK { <urn:s> <urn:p> <urn:o> }"),
        r#"{"boolean":true,"form":"boolean","ok":true}"#
    );
    assert_eq!(
        view.query_local_json("", "turtle", "ASK { ?s ?p ?o }", QueryScope::InputOnly),
        r#"{"boolean":false,"form":"boolean","ok":true}"#
    );
}

#[test]
fn query_transport_graph_forms_keep_native_nquads_and_quad_count() {
    let body = r#"<urn:s> <urn:p> "texte"@fr--ltr ."#;
    let view = view(body);
    let default = r#"<urn:s> <urn:p> "texte"@fr--ltr ."#;
    let external = r#"<urn:s> <urn:p> "texte"@fr--ltr <urn:gmeow:mcp:overlay:external> ."#;
    for (query, expected_local) in [
        ("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }", vec![default]),
        ("DESCRIBE <urn:s>", vec![default, external]),
    ] {
        let docs = view.run_docs_query(query).unwrap();
        let local = view
            .run_local_query(body, "turtle", query, QueryScope::InputOnly)
            .unwrap();
        // Documentation is projected into the default graph. Local input keeps
        // both default and external-origin copies; native DESCRIBE retains both,
        // whereas this CONSTRUCT template emits only default-graph triples.
        if expected_local.len() == 1 {
            assert_eq!(docs, local);
        }
        assert_eq!(docs["form"].as_str(), Some("graph"));
        assert_eq!(docs["quad_count"].as_u64(), Some(1));
        assert_eq!(docs["ok"].as_bool(), Some(true));
        assert!(
            docs["graph_nquads"]
                .as_str()
                .unwrap()
                .contains(r#""texte"@fr--ltr"#)
        );
        assert_eq!(
            docs["graph_nquads"]
                .as_str()
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            vec![default]
        );
        assert_eq!(local["form"].as_str(), Some("graph"));
        assert_eq!(local["ok"].as_bool(), Some(true));
        assert_eq!(
            local["quad_count"].as_u64(),
            Some(expected_local.len() as u64)
        );
        assert_eq!(
            local["graph_nquads"]
                .as_str()
                .unwrap()
                .lines()
                .collect::<std::collections::BTreeSet<_>>(),
            expected_local
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
        );
    }
}

#[test]
fn query_transport_refuses_malformed_native_results_without_silent_cells() {
    let bad = TermValue::Triple {
        s: TermBox::new(TermValue::iri("urn:s")),
        p: TermBox::new(TermValue::typed_literal(
            "not a predicate",
            "http://www.w3.org/2001/XMLSchema#string",
        )),
        o: TermBox::new(TermValue::iri("urn:o")),
    };
    let error = envelope(solutions(&["term"], vec![vec![Some(bad)]])).unwrap_err();
    assert!(error.to_string().contains("predicate"), "{error:#}");
    assert!(error.downcast_ref::<purrdf::sparql::Error>().is_some());
    for result in [
        solutions(&["same", "same"], vec![vec![None, None]]),
        solutions(&["term"], vec![vec![]]),
        solutions(&[], vec![vec![Some(TermValue::iri("urn:extra"))]]),
    ] {
        let error = envelope(result).unwrap_err();
        assert!(
            error.to_string().contains("native SPARQL result"),
            "{error}"
        );
    }
}

#[test]
fn query_transport_tool_and_rpc_return_one_unquoted_json_text_payload() {
    let server = server(r#"<urn:s> <urn:p> "نص"@ar--rtl ."#);
    let args = json!({"query":"SELECT ?o WHERE { <urn:s> <urn:p> ?o }"});
    let direct = server.call_tool_result("query_docs", &args);
    assert_eq!(direct["isError"], false);
    let text = direct["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with('{'));
    let envelope = parse(text);
    assert_eq!(
        envelope["results"]["bindings"][0]["o"]["its:dir"].as_str(),
        Some("rtl")
    );
    let request = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"query_docs","arguments":args}});
    let response = parse(&server.handle_message(&request.to_string()));
    assert_eq!(
        response["result"]["content"][0]["text"].as_str(),
        Some(text)
    );
    let local = server.call_tool_result("query_local", &json!({"data":"<urn:s> <urn:p> \"نص\"@ar--rtl .", "format":"turtle", "scope":"input", "query":"SELECT ?o WHERE { <urn:s> <urn:p> ?o }"}));
    assert_eq!(local["content"][0]["text"].as_str(), Some(text));
}

#[test]
fn query_transport_argument_and_query_errors_retain_existing_tool_policy() {
    let server = server("");
    let missing = server.call_tool_result("query_docs", &json!({}));
    assert_eq!(missing["isError"], true);
    for (tool, args) in [
        ("query_docs", json!({"query":"not a query"})),
        (
            "query_local",
            json!({"data":"", "format":"not-a-format", "scope":"input", "query":"ASK {}"}),
        ),
    ] {
        let result = server.call_tool_result(tool, &args);
        assert_eq!(
            result["isError"], false,
            "query-level failures remain selected error envelopes"
        );
        let payload = parse(result["content"][0]["text"].as_str().unwrap());
        assert_eq!(payload["ok"].as_bool(), Some(false));
        assert!(
            payload["error"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        );
    }
}

#[test]
fn query_transport_lexical_documentation_adapter_remains_an_explicit_display_exit() {
    let view = view(r#"<urn:s> <urn:p> "texte"@fr--ltr ."#);
    let rows = view
        .docs_select_rows("SELECT ?o WHERE { <urn:s> <urn:p> ?o }")
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["o"], "texte");
    let complete = view
        .run_docs_query("SELECT ?o WHERE { <urn:s> <urn:p> ?o }")
        .unwrap();
    assert_eq!(
        complete["results"]["bindings"][0]["o"]["its:dir"].as_str(),
        Some("ltr")
    );
}

#[test]
fn query_transport_search_keeps_lexical_facets_and_deterministic_ranking() {
    let view = view(
        r#"
<urn:doc> a g:DocumentedTerm ; g:documents <urn:term> ;
 g:docSearchLabel "chat"@fr--ltr ; g:docSearchDefinition "Un petit félin."@fr ;
 g:docSearchAdvice "Consulter les illustrations."@fr ; g:docUrl "term/chat.html" .
"#,
    );
    let first = crate::search_documentation(view.documentation().unwrap(), "chat", 10).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].label, "chat");
    assert_eq!(first[0].id, "urn:term");
    assert_eq!(first[0].definition.as_deref(), Some("Un petit félin."));
    assert_eq!(first[0].advice, vec!["Consulter les illustrations."]);
    let repeated = crate::search_documentation(view.documentation().unwrap(), "chat", 10).unwrap();
    assert_eq!(first[0].to_json(), repeated[0].to_json());
}
