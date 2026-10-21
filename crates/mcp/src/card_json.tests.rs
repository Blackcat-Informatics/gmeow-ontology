// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{CardFormat, card_envelope};
use gmeow_docs_model::card::{Card, CardDetail, CardFixture};

#[test]
fn json_card_mcp_envelope_preserves_literal_unicode_byte_and_character_costs() {
    let card = Card {
        category: "Class".to_owned(),
        iri: "x".to_owned(),
        label: Some("猫".to_owned()),
        ..Default::default()
    };
    let expected = r#"{"bytes":44,"card":{"category":"Class","iri":"x","label":"猫"},"detail":"standard","format":"json","ok":true,"tokens":11}"#;
    assert_eq!(
        card_envelope("Title", &card, CardDetail::Standard, CardFormat::Json),
        expected
    );
}

#[test]
fn json_card_mcp_envelope_sorts_nested_records_after_measuring_declared_payload() {
    let card = Card {
        category: "Class".to_owned(),
        iri: "x".to_owned(),
        use_when: vec!["u".to_owned()],
        avoid_when: vec!["a".to_owned()],
        fixtures_do: vec![CardFixture {
            title: "T".to_owned(),
            body: "B".to_owned(),
        }],
        ..Default::default()
    };
    let expected = r#"{"bytes":107,"card":{"avoid_when":["a"],"category":"Class","fixtures_do":[{"body":"B","title":"T"}],"iri":"x","use_when":["u"]},"detail":"full","format":"json","ok":true,"tokens":27}"#;
    assert_eq!(
        card_envelope("Title", &card, CardDetail::Full, CardFormat::Json),
        expected
    );
}

#[test]
fn json_card_mcp_markdown_still_uses_the_existing_renderer_and_string_envelope() {
    let card = Card {
        category: "Class".to_owned(),
        iri: "x".to_owned(),
        ..Default::default()
    };
    let expected_payload = "# Title\n\n- category: Class\n- iri: x\n\n";
    let output = card_envelope("Title", &card, CardDetail::Standard, CardFormat::Markdown);
    let document = purrdf_lex::json::record::read_document(output.as_bytes()).unwrap();
    assert_eq!(document["card"].as_str(), Some(expected_payload));
    assert_eq!(document["bytes"].as_u64(), Some(37));
    assert_eq!(document["tokens"].as_u64(), Some(10));
    assert_eq!(document["format"].as_str(), Some("markdown"));
    assert!(!output.ends_with('\n'));
}

#[test]
fn json_card_mcp_envelope_keeps_tier_projection_before_encoding() {
    let card = Card {
        category: "Class".to_owned(),
        iri: "x".to_owned(),
        definition: Some("d".to_owned()),
        use_when: vec!["u".to_owned()],
        fixtures_do: vec![CardFixture {
            title: "T".to_owned(),
            body: "B".to_owned(),
        }],
        ..Default::default()
    };
    let summary = card_envelope("Title", &card, CardDetail::Summary, CardFormat::Json);
    let standard = card_envelope("Title", &card, CardDetail::Standard, CardFormat::Json);
    let full = card_envelope("Title", &card, CardDetail::Full, CardFormat::Json);
    let decode = |text: &str| purrdf_lex::json::record::read_document(text.as_bytes()).unwrap();
    let (summary, standard, full) = (decode(&summary), decode(&standard), decode(&full));
    assert!(summary["card"].get("use_when").is_none());
    assert!(summary["card"].get("fixtures_do").is_none());
    assert!(standard["card"].get("use_when").is_some());
    assert!(standard["card"].get("fixtures_do").is_none());
    assert!(full["card"].get("fixtures_do").is_some());
    assert!(summary["bytes"].as_u64().unwrap() < standard["bytes"].as_u64().unwrap());
    assert!(standard["bytes"].as_u64().unwrap() < full["bytes"].as_u64().unwrap());
}
