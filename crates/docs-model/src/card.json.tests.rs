// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn full_card() -> Card {
    Card {
        category: "Class".to_owned(),
        iri: "urn:card".to_owned(),
        label: Some("猫\n\"é".to_owned()),
        slice: Some("slice".to_owned()),
        box_roles: vec!["box".to_owned()],
        definition: Some("definition".to_owned()),
        parents: vec!["parent".to_owned()],
        domain: vec!["domain".to_owned()],
        range: vec!["range".to_owned()],
        use_when: vec!["use".to_owned()],
        avoid_when: vec!["avoid".to_owned()],
        how_to_use: vec!["how".to_owned()],
        scope_notes: vec!["scope".to_owned()],
        examples: vec!["example".to_owned()],
        logic_stereotypes: vec!["kind".to_owned()],
        related_terms: vec!["related".to_owned()],
        use_for_consumer: vec!["consumer".to_owned()],
        avoid_for_consumer: vec!["excluded".to_owned()],
        aligns: vec!["match".to_owned()],
        python_model: Some("package.Model".to_owned()),
        python_snippet: Some("Model()".to_owned()),
        entailments: vec![CardEntailment {
            rule: "r".to_owned(),
            conclusion: "c".to_owned(),
            premises: vec!["z".to_owned(), "a".to_owned()],
        }],
        fixtures_do: vec![CardFixture {
            title: "do".to_owned(),
            body: "yes".to_owned(),
        }],
        fixtures_dont: vec![CardFixture {
            title: "don't".to_owned(),
            body: "no".to_owned(),
        }],
        diagnostics: vec![CardDiagnostic {
            code: "code".to_owned(),
            note: "note".to_owned(),
        }],
        loss: vec![CardLoss {
            target: "target".to_owned(),
            preservation: "preservation".to_owned(),
        }],
    }
}

#[test]
fn json_card_all_five_records_keep_complete_literal_compact_order() {
    let actual = json::write_compact(&full_card().to_json());
    let expected = r#"{"category":"Class","iri":"urn:card","label":"猫\n\"é","slice":"slice","box_roles":["box"],"definition":"definition","parents":["parent"],"domain":["domain"],"range":["range"],"use_when":["use"],"avoid_when":["avoid"],"how_to_use":["how"],"scope_notes":["scope"],"examples":["example"],"logic_stereotypes":["kind"],"related_terms":["related"],"use_for_consumer":["consumer"],"avoid_for_consumer":["excluded"],"aligns":["match"],"python_model":"package.Model","python_snippet":"Model()","entailments":[{"rule":"r","conclusion":"c","premises":["z","a"]}],"fixtures_do":[{"title":"do","body":"yes"}],"fixtures_dont":[{"title":"don't","body":"no"}],"diagnostics":[{"code":"code","note":"note"}],"loss":[{"target":"target","preservation":"preservation"}]}"#;
    assert_eq!(actual, expected);
}

#[test]
fn json_card_pretty_bytes_and_required_empty_strings_keep_no_terminal_newline() {
    let card = Card {
        category: "Class".to_owned(),
        iri: "urn:猫".to_owned(),
        definition: Some("a\n\t\"\\\u{0001}".to_owned()),
        ..Default::default()
    };
    assert_eq!(
        card_json(&card),
        "{\n  \"category\": \"Class\",\n  \"iri\": \"urn:猫\",\n  \"definition\": \"a\\n\\t\\\"\\\\\\u0001\"\n}"
    );
    assert_eq!(
        json::write_compact(&Card::default().to_json()),
        r#"{"category":"","iri":""}"#
    );
    assert!(!card_json(&card).ends_with('\n'));
}

#[test]
fn json_card_present_empty_optionals_and_nonempty_panels_are_never_omitted() {
    let card = Card {
        label: Some(String::new()),
        slice: Some(String::new()),
        definition: Some(String::new()),
        python_model: Some(String::new()),
        python_snippet: Some(String::new()),
        parents: vec![String::new()],
        entailments: vec![CardEntailment::default()],
        fixtures_do: vec![CardFixture::default()],
        diagnostics: vec![CardDiagnostic::default()],
        loss: vec![CardLoss::default()],
        ..Default::default()
    };
    let value = card.to_json();
    for name in [
        "label",
        "slice",
        "definition",
        "python_model",
        "python_snippet",
    ] {
        assert_eq!(value[name].as_str(), Some(""), "{name}");
    }
    assert_eq!(value["parents"][0].as_str(), Some(""));
    assert!(value.get("domain").is_none());
    assert_eq!(
        json::write_compact(&value["entailments"]),
        r#"[{"rule":"","conclusion":"","premises":[]}]"#
    );
    assert_eq!(
        json::write_compact(&value["fixtures_do"]),
        r#"[{"title":"","body":""}]"#
    );
    assert_eq!(
        json::write_compact(&value["diagnostics"]),
        r#"[{"code":"","note":""}]"#
    );
    assert_eq!(
        json::write_compact(&value["loss"]),
        r#"[{"target":"","preservation":""}]"#
    );
}

#[test]
fn json_card_tiers_preserve_literal_identity_advisory_and_panel_boundaries() {
    let card = Card {
        category: "Class".to_owned(),
        iri: "x".to_owned(),
        label: Some("label".to_owned()),
        definition: Some("d".to_owned()),
        use_when: vec!["u".to_owned()],
        fixtures_do: vec![CardFixture {
            title: "t".to_owned(),
            body: "b".to_owned(),
        }],
        ..Default::default()
    };
    for (detail, expected) in [
        (
            CardDetail::Summary,
            r#"{"category":"Class","iri":"x","label":"label","definition":"d"}"#,
        ),
        (
            CardDetail::Standard,
            r#"{"category":"Class","iri":"x","label":"label","definition":"d","use_when":["u"]}"#,
        ),
        (
            CardDetail::Full,
            r#"{"category":"Class","iri":"x","label":"label","definition":"d","use_when":["u"],"fixtures_do":[{"title":"t","body":"b"}]}"#,
        ),
    ] {
        assert_eq!(
            json::write_compact(&card.projected(detail).to_json()),
            expected
        );
    }
}

#[test]
fn json_card_toon_retains_sorted_root_and_table_columns_with_string_quoting() {
    let card = Card {
        category: "Class".to_owned(),
        iri: "x".to_owned(),
        label: Some("猫".to_owned()),
        parents: vec!["NaN".to_owned(), "1e400".to_owned()],
        fixtures_do: vec![CardFixture {
            title: "Title".to_owned(),
            body: "Body".to_owned(),
        }],
        ..Default::default()
    };
    assert_eq!(
        card_toon(&card),
        "category: Class\nfixtures_do[1]{body,title}:\n  Body,Title\niri: x\nlabel: 猫\nparents[2]: \"NaN\",\"1e400\"\n"
    );
}

#[test]
fn json_card_schema_validates_complete_tiers_and_refuses_malformed_records() {
    let schema = json::write_compact(&card_json_schema()).into_bytes();
    let card = full_card();
    for detail in [CardDetail::Summary, CardDetail::Standard, CardDetail::Full] {
        let instance = json::write_compact(&card.projected(detail).to_json());
        assert!(
            gmeow_validate::instance::validate_instance(
                instance.as_bytes(),
                gmeow_validate::instance::InstanceFormat::Json,
                &schema
            )
            .unwrap()
            .is_empty()
        );
    }
    for instance in [
        r#"{"iri":"x"}"#,
        r#"{"category":"Class","iri":"x","label":null}"#,
        r#"{"category":"Class","iri":"x","parents":[1]}"#,
        r#"{"category":"Class","iri":"x","fixtures_do":[{"title":"t"}]}"#,
        r#"{"category":"Class","iri":"x","entailments":[{"rule":"r","conclusion":"c","premises":null}]}"#,
        r#"{"category":"Class","iri":"x","unknown":true}"#,
    ] {
        assert!(
            !gmeow_validate::instance::validate_instance(
                instance.as_bytes(),
                gmeow_validate::instance::InstanceFormat::Json,
                &schema
            )
            .unwrap()
            .is_empty(),
            "{instance}"
        );
    }
}

#[test]
fn json_card_schema_object_names_are_recursively_sorted_without_reordering_required_arrays() {
    let schema = card_json_schema();
    let mut pending = vec![&schema];
    while let Some(value) = pending.pop() {
        if let Some(object) = value.as_object() {
            let keys: Vec<_> = object.keys().map(String::as_str).collect();
            assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
            pending.extend(object.values());
        } else if let Some(array) = value.as_array() {
            pending.extend(array);
        }
    }
    assert_eq!(schema["required"], vec!["category", "iri"].to_json());
    assert_eq!(
        schema["$defs"]["fixture"]["required"],
        vec!["title", "body"].to_json()
    );
}
