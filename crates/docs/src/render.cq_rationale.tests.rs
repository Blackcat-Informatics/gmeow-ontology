// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored in-memory documentation only; no site discovery or corpus rendering.

use super::*;

#[test]
fn cq_rationale_actual_index_and_term_display_every_lexical_value_without_preference() {
    let term = DocTerm {
        iri: "urn:term".to_owned(),
        slug: "authored".to_owned(),
        curie: "authored:term".to_owned(),
        label: Some("Authored term".to_owned()),
        ..DocTerm::default()
    };
    let question = crate::model::DocCompetency {
        iri: "urn:cq".to_owned(),
        rationales: vec![
            purrdf::RdfLiteral::simple("untagged reason"),
            purrdf::RdfLiteral::language_tagged("justification française", "fr"),
            purrdf::RdfLiteral {
                lexical_form: "سبب عربي".to_owned(),
                datatype: None,
                language: Some("ar".to_owned()),
                direction: Some(purrdf::RdfTextDirection::Rtl),
            },
        ],
        query_file: Some("queries/competency/authored.rq".to_owned()),
        query_text: Some("ASK { }".to_owned()),
        exercises: vec![term.iri.clone()],
        ..Default::default()
    };
    let model = DocsModel {
        terms: vec![term],
        competencies: vec![question],
        ..DocsModel::default()
    };
    for markdown in [
        md_competency_index(&model),
        md_term(&model, "authored", &ExecutableDocsData::default()),
    ] {
        for text in ["untagged reason", "justification française", "سبب عربي"] {
            assert_eq!(markdown.matches(text).count(), 1, "{markdown}");
        }
        assert!(markdown.contains("queries/competency/authored.rq"));
    }
    assert_eq!(
        model.competencies[0].rationales[2].direction,
        Some(purrdf::RdfTextDirection::Rtl)
    );
}
