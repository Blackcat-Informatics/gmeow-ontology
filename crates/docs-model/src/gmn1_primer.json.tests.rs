// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Plain authored primer records: no graph, dictionary, producer or corpus loading.

use super::teachability::{HeldoutTask, Observations};
use super::*;
use purrdf_lex::json::record::from_slice;

const COMPLETE: &str = r#"{"primer":{"rows":[{"sort_curie":"z","body":"@c — 猫","source":{"Sigil":{"curie":"g:Claim"}}},{"sort_curie":"a","body":"⊑ (infix, alias sub) — g:Sub","source":{"Operator":{"curie":"g:Sub","glyph":"⊑"}}},{"sort_curie":"r","body":"repair","source":{"Repair":{"curie":"g:Err"}}}],"intro":"Intro \"猫\"\nline","elided_operators":1,"total_operators":2},"tasks":[{"label":"猫","document":"@c{\\x}\n","sigils":["@a","@z"],"operator_glyphs":["⊑","⊔"],"ast_error":null},{"label":"","document":"","sigils":[],"operator_glyphs":[],"ast_error":"bad \"row\"\t"}]}"#;

/// Build only independently authored in-memory records in deliberately unsorted row order.
fn authored() -> Observations {
    Observations {
        primer: Gmn1Primer {
            rows: vec![
                PrimerRow {
                    sort_curie: "z".into(),
                    body: "@c — 猫".into(),
                    source: PrimerRowSource::Sigil {
                        curie: "g:Claim".into(),
                    },
                },
                PrimerRow {
                    sort_curie: "a".into(),
                    body: "⊑ (infix, alias sub) — g:Sub".into(),
                    source: PrimerRowSource::Operator {
                        curie: "g:Sub".into(),
                        glyph: "⊑".into(),
                    },
                },
                PrimerRow {
                    sort_curie: "r".into(),
                    body: "repair".into(),
                    source: PrimerRowSource::Repair {
                        curie: "g:Err".into(),
                    },
                },
            ],
            intro: "Intro \"猫\"\nline".into(),
            elided_operators: 1,
            total_operators: 2,
        },
        tasks: vec![
            HeldoutTask {
                label: "猫".into(),
                document: "@c{\\x}\n".into(),
                sigils: ["@z".into(), "@a".into()].into_iter().collect(),
                operator_glyphs: ["⊔".into(), "⊑".into()].into_iter().collect(),
                ast_error: None,
            },
            HeldoutTask {
                label: String::new(),
                document: String::new(),
                sigils: Default::default(),
                operator_glyphs: Default::default(),
                ast_error: Some("bad \"row\"\t".into()),
            },
        ],
    }
}

#[test]
fn json_gmn_primer_complete_literal_bytes_preserve_all_five_records() {
    assert_eq!(
        purrdf_lex::json::write_compact(&authored().to_json()),
        COMPLETE
    );
    assert!(!COMPLETE.ends_with('\n'));
}

#[test]
fn json_gmn_primer_roundtrip_preserves_rendering_and_witness_accessors() {
    let original = authored();
    let decoded: Observations = from_slice(COMPLETE.as_bytes()).unwrap();
    assert_eq!(
        purrdf_lex::json::write_compact(&decoded.to_json()),
        COMPLETE
    );
    assert_eq!(decoded.primer.rendered(), original.primer.rendered());
    assert_eq!(
        decoded.primer.resource_text(),
        original.primer.resource_text()
    );
    assert_eq!(decoded.primer.section().heading, PRIMER_HEADING);
    assert_eq!(decoded.primer.token_count(), original.primer.token_count());
    assert_eq!(decoded.primer.fits_budget(), original.primer.fits_budget());
    assert_eq!(
        decoded.primer.sigil_glyphs(),
        ["@c".to_string()].into_iter().collect()
    );
    assert_eq!(
        decoded.primer.operator_index().get("⊑"),
        Some(&("infix".into(), "sub".into()))
    );
    assert_eq!(
        decoded.primer.cited_curies(),
        ["g:Claim".into(), "g:Err".into(), "g:Sub".into()]
            .into_iter()
            .collect()
    );
    assert!(decoded.primer.graph_line_bodies().contains("repair"));
    assert!(
        decoded
            .primer
            .elision_line()
            .starts_with("1 of 2 operator glyphs elided")
    );
}

#[test]
fn json_gmn_primer_nullable_error_and_set_normalization_keep_contract() {
    let text =
        r#"{"label":"","document":"","sigils":["@z","@a","@z"],"operator_glyphs":["⊔","⊑","⊔"]}"#;
    let task: HeldoutTask = from_slice(text.as_bytes()).unwrap();
    assert!(task.ast_error.is_none());
    assert_eq!(
        purrdf_lex::json::write_compact(&task.to_json()),
        r#"{"label":"","document":"","sigils":["@a","@z"],"operator_glyphs":["⊑","⊔"],"ast_error":null}"#
    );
    let null = text.replace("}", r#","ast_error":null}"#);
    assert!(
        from_slice::<HeldoutTask>(null.as_bytes())
            .unwrap()
            .ast_error
            .is_none()
    );
}

#[test]
fn json_gmn_primer_unknown_record_members_are_ignored_without_reordering() {
    let text = COMPLETE
        .replacen(
            r#"{"primer":{"rows":"#,
            r#"{"unknown":42,"primer":{"extra":true,"rows":"#,
            1,
        )
        .replace(r#""sort_curie":"z""#, r#""unknown":null,"sort_curie":"z""#)
        .replace(r#""curie":"g:Claim""#, r#""extra":[],"curie":"g:Claim""#)
        .replace(r#""label":"猫""#, r#""extra":{},"label":"猫""#);
    let decoded: Observations = from_slice(text.as_bytes()).unwrap();
    assert_eq!(
        purrdf_lex::json::write_compact(&decoded.to_json()),
        COMPLETE
    );
}

#[test]
fn json_gmn_primer_duplicate_known_members_are_refused_at_each_record_depth() {
    for text in [
        COMPLETE.replacen(r#""tasks":"#, r#""tasks":[],"tasks":"#, 1),
        COMPLETE.replacen(r#""intro":"#, r#""intro":"","intro":"#, 1),
        COMPLETE.replacen(r#""body":"#, r#""body":"","body":"#, 1),
        COMPLETE.replacen(r#""curie":"#, r#""curie":"","curie":"#, 1),
        COMPLETE.replacen(r#""label":"#, r#""label":"","label":"#, 1),
        COMPLETE.replacen(
            r#""ast_error":null"#,
            r#""ast_error":null,"ast_error":null"#,
            1,
        ),
    ] {
        assert!(
            from_slice::<Observations>(text.as_bytes()).is_err(),
            "{text}"
        );
    }
    assert!(
        from_slice::<PrimerRowSource>(br#"{"Sigil":{"curie":"x"},"Sigil":{"curie":"y"}}"#).is_err()
    );
}

#[test]
fn json_gmn_primer_external_tags_require_one_known_object_variant() {
    for text in [
        r#"{}"#,
        r#"{"Other":{"curie":"x"}}"#,
        r#"{"Sigil":{"curie":"x"},"Repair":{"curie":"y"}}"#,
        r#"{"Sigil":null}"#,
        r#"{"Sigil":["x"]}"#,
        r#"{"Operator":{"curie":"x"}}"#,
        r#"{"Repair":{"curie":1}}"#,
        r#""Sigil""#,
        r#"["Sigil",{"curie":"x"}]"#,
    ] {
        assert!(
            from_slice::<PrimerRowSource>(text.as_bytes()).is_err(),
            "{text}"
        );
    }
}

#[test]
fn json_gmn_primer_missing_wrong_and_positional_records_are_refused() {
    for text in [
        COMPLETE.replacen(r#""tasks":"#, r#""other_tasks":"#, 1),
        COMPLETE.replacen(r#""rows":"#, r#""other_rows":"#, 1),
        COMPLETE.replacen(r#""sort_curie":"#, r#""other_curie":"#, 1),
        COMPLETE.replacen(r#""label":"#, r#""other_label":"#, 1),
        COMPLETE.replacen(r#""tasks":["#, r#""tasks":null,"ignored":["#, 1),
        COMPLETE.replacen(r#""sigils":["@a","@z"]"#, r#""sigils":[1]"#, 1),
        COMPLETE.replacen(r#""ast_error":null"#, r#""ast_error":false"#, 1),
        "[]".into(),
        "null".into(),
    ] {
        assert!(
            from_slice::<Observations>(text.as_bytes()).is_err(),
            "{text}"
        );
    }
}

#[test]
fn json_gmn_primer_unsigned_counters_require_exact_bounded_integer_tokens() {
    let maximum = format!(
        r#"{{"rows":[],"intro":"","elided_operators":{},"total_operators":0}}"#,
        usize::MAX
    );
    let primer: Gmn1Primer = from_slice(maximum.as_bytes()).unwrap();
    assert_eq!(primer.elided_operators, usize::MAX);
    let overflow = (usize::MAX as u128 + 1).to_string();
    for token in [
        "-1", "-0", "1.0", "1e0", "1e400", "null", "true", "\"1\"", &overflow,
    ] {
        for field in ["elided_operators", "total_operators"] {
            let text = r#"{"rows":[],"intro":"","elided_operators":0,"total_operators":0}"#
                .replace(&format!(r#""{field}":0"#), &format!(r#""{field}":{token}"#));
            assert!(from_slice::<Gmn1Primer>(text.as_bytes()).is_err(), "{text}");
        }
    }
}

#[test]
fn json_gmn_primer_malformed_or_trailing_documents_are_refused() {
    for text in ["", "{", "[", "NaN", "{}", "null", "false"] {
        assert!(
            from_slice::<Observations>(text.as_bytes()).is_err(),
            "{text}"
        );
    }
    for suffix in [" {}", " null", "x", ","] {
        assert!(from_slice::<Observations>(format!("{COMPLETE}{suffix}").as_bytes()).is_err());
    }
    assert!(from_slice::<Observations>(&[0xff]).is_err());
}
