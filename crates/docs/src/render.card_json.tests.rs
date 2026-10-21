// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn json_card_docs_single_term_adapter_preserves_literal_standard_bytes() {
    let term = DocTerm {
        iri: "https://example.test/X".to_owned(),
        curie: "ex:X".to_owned(),
        label: Some("猫".to_owned()),
        owner_slice: "https://example.test/slices/demo".to_owned(),
        category: DocTermCategory::Class,
        definition: Some("A\ncard.".to_owned()),
        use_when: vec!["Use".to_owned()],
        ..Default::default()
    };
    let model = DocsModel {
        terms: vec![term.clone()],
        ..Default::default()
    };
    let expected = r#"{"category":"Class","iri":"https://example.test/X","label":"猫","slice":"demo","definition":"A card.","use_when":["Use"]}"#;
    assert_eq!(term_card_json(&model, &term), expected.as_bytes());
}
