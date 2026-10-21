// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::{DocLinkage, DocMarkdownDocument};

fn record(kind: &'static str, id: &str, url: &str) -> SearchRecord {
    SearchRecord {
        kind,
        id: id.to_owned(),
        label: "é 猫".to_owned(),
        definition: None,
        url: url.to_owned(),
        advice: Vec::new(),
        alignments: Vec::new(),
        missing_coverage: Vec::new(),
    }
}

#[test]
fn json_search_index_record_keeps_literal_order_null_and_empty_facet_omission() {
    let output = search_records_json(vec![record("term", "ex:One", "terms/one/index.html")]);
    let expected = "[\n  {\n    \"kind\": \"term\",\n    \"id\": \"ex:One\",\n    \"label\": \"é 猫\",\n    \"definition\": null,\n    \"url\": \"terms/one/index.html\"\n  }\n]";
    assert_eq!(output, expected);
    assert!(!output.ends_with('\n'));
}

#[test]
fn json_search_index_populated_facets_keep_key_vector_and_escape_order() {
    let mut value = record("term", "ex:One", "terms/one/index.html");
    value.label = "é 猫\n\t\"\\\u{0001}".to_owned();
    value.definition = Some("line 1\nline 2".to_owned());
    value.advice = vec!["second".to_owned(), "first".to_owned()];
    value.alignments = vec!["z:猫".to_owned(), "a:é".to_owned()];
    value.missing_coverage = vec!["z", "a"];
    let output = search_records_json(vec![value]);
    let expected = "[\n  {\n    \"kind\": \"term\",\n    \"id\": \"ex:One\",\n    \"label\": \"é 猫\\n\\t\\\"\\\\\\u0001\",\n    \"definition\": \"line 1\\nline 2\",\n    \"url\": \"terms/one/index.html\",\n    \"advice\": [\n      \"second\",\n      \"first\"\n    ],\n    \"alignments\": [\n      \"z:猫\",\n      \"a:é\"\n    ],\n    \"missing_coverage\": [\n      \"z\",\n      \"a\"\n    ]\n  }\n]";
    assert_eq!(output, expected);
}

#[test]
fn json_search_index_only_empty_vectors_are_omitted_not_empty_string_members() {
    let mut value = record("term", "ex:Empty", "terms/empty/index.html");
    value.definition = Some(String::new());
    value.advice = vec![String::new()];
    value.alignments = vec![String::new()];
    value.missing_coverage = vec![""];
    let object = value.to_json();
    assert_eq!(object["definition"].as_str(), Some(""));
    for field in ["advice", "alignments", "missing_coverage"] {
        assert_eq!(object[field].as_array().unwrap().len(), 1);
        assert_eq!(object[field][0].as_str(), Some(""));
    }
}

#[test]
fn json_search_index_actual_adapter_sorts_url_then_kind_then_id() {
    let output = search_records_json(vec![
        record("term", "0", "z/index.html"),
        record("term", "z", "same/index.html"),
        record("slice", "q", "same/index.html"),
        record("term", "a", "same/index.html"),
    ]);
    let document = json::record::read_document(output.as_bytes()).unwrap();
    let identities: Vec<_> = document
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            (
                value["url"].as_str().unwrap(),
                value["kind"].as_str().unwrap(),
                value["id"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        identities,
        vec![
            ("same/index.html", "slice", "q"),
            ("same/index.html", "term", "a"),
            ("same/index.html", "term", "z"),
            ("z/index.html", "term", "0"),
        ]
    );
}

#[test]
fn json_search_index_empty_adapter_and_public_model_keep_literal_empty_array() {
    assert_eq!(search_records_json(Vec::new()), "[]");
    assert_eq!(search_index_json(&DocsModel::default()), "[]");
}

#[test]
fn json_search_index_public_concern_model_keeps_literal_wire_bytes() {
    let model = DocsModel {
        concerns: vec![DocConcern {
            iri: "https://example.test/concerns/search".to_owned(),
            curie: "ex:Search".to_owned(),
            label: Some("é 猫".to_owned()),
            definition: None,
            terms: Vec::new(),
            slices: Vec::new(),
        }],
        ..Default::default()
    };
    let expected = "[\n  {\n    \"kind\": \"concern\",\n    \"id\": \"ex:Search\",\n    \"label\": \"é 猫\",\n    \"definition\": null,\n    \"url\": \"concerns/search/index.html\"\n  }\n]";
    assert_eq!(search_index_json(&model), expected);
}

#[test]
fn json_search_index_public_document_model_retains_child_url_and_full_prose() {
    let slice_iri = "https://example.test/slices/search";
    let documents = [
        (
            "docs.md",
            "Slice thesis",
            "The top-level prose is part of the slice page.",
        ),
        (
            "design/ARCHITECTURE.md",
            "Architecture 猫",
            "# Architecture 猫\n\nFull child prose.\n",
        ),
    ]
    .into_iter()
    .map(|(path, title, text)| DocMarkdownDocument {
        slice_iri: slice_iri.to_owned(),
        slice_slug: "search".to_owned(),
        source_path: path.to_owned(),
        title: title.to_owned(),
        source_text: text.to_owned(),
        raw_digest: "independently-authored-model-input".to_owned(),
    })
    .collect();
    let model = DocsModel {
        slices: vec![DocSlice {
            iri: slice_iri.to_owned(),
            label: None,
            title: Some("Search slice".to_owned()),
            tier: None,
            identifier: None,
            creators: Vec::new(),
            consumers: Vec::new(),
            profiles: Vec::new(),
            depends_on: Vec::new(),
            artifacts: Vec::new(),
            documents,
            has_thesis_sentence: true,
            realized_state_complete: false,
        }],
        ..Default::default()
    };
    let output = search_index_json(&model);
    let document = json::record::read_document(output.as_bytes()).unwrap();
    let rows = document.as_array().unwrap();
    assert_eq!(
        rows.len(),
        2,
        "docs.md must not create a separate child record"
    );
    assert_eq!(rows[0]["kind"].as_str(), Some("document"));
    assert_eq!(rows[0]["id"].as_str(), Some("design/ARCHITECTURE.md"));
    assert_eq!(
        rows[0]["url"].as_str(),
        Some("slices/search/documents/design/ARCHITECTURE/index.html")
    );
    assert_eq!(rows[0]["label"].as_str(), Some("Architecture 猫"));
    assert_eq!(
        rows[0]["definition"].as_str(),
        Some("# Architecture 猫\n\nFull child prose.\n")
    );
    assert_eq!(rows[1]["kind"].as_str(), Some("slice"));
    assert_eq!(rows[1]["url"].as_str(), Some("slices/search/index.html"));
}

#[test]
fn json_search_index_public_term_model_retains_advice_and_sorted_alignment_facets() {
    let iri = "https://example.test/Term";
    let term = DocTerm {
        iri: iri.to_owned(),
        curie: "ex:Term".to_owned(),
        label: Some("Term".to_owned()),
        category: DocTermCategory::Class,
        scope_notes: vec!["scope".to_owned()],
        use_when: vec!["use".to_owned()],
        avoid_when: vec!["avoid".to_owned()],
        how_to_use: vec!["how".to_owned()],
        ..Default::default()
    };
    let linkages = [
        "https://example.test/Z",
        "https://example.test/A",
        "https://example.test/Z",
    ]
    .into_iter()
    .map(|object| DocLinkage {
        mapping_set: None,
        subject: iri.to_owned(),
        subject_curie: "ex:Term".to_owned(),
        predicate: "https://example.test/match".to_owned(),
        object: object.to_owned(),
        justification: None,
        confidence: None,
        owner_slice: "https://example.test/slices/search".to_owned(),
    })
    .collect();
    let model = DocsModel {
        terms: vec![term],
        linkages,
        ..Default::default()
    };
    let output = search_index_json(&model);
    let document = json::record::read_document(output.as_bytes()).unwrap();
    let rows = document.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["id"].as_str(), Some("ex:Term"));
    assert_eq!(
        row["advice"],
        vec!["scope", "use", "avoid", "how"].to_json()
    );
    let alignments: Vec<_> = row["alignments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(alignments.len(), 2);
    assert!(alignments[0].ends_with(":A"));
    assert!(alignments[1].ends_with(":Z"));
    assert!(!row["missing_coverage"].as_array().unwrap().is_empty());
}
