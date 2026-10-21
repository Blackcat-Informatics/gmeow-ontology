// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored finding records; no repository/corpus input.

use super::*;
use crate::{FindingCategory, Severity, Standpoint};
use purrdf_lex::json::record::{from_slice, to_vec};

const MINIMAL: &str = r#"{"severity":"error","code":"authored.code","message":"missing evidence"}"#;
const COMPLETE: &str = r#"{"severity":"warning","code":"authored.code","message":"nul\u0000\t\"\\中😀","source_graph":"https://example.org/Graph","source_focus":"urn:focus","tool":"tiny","locations":[{"path":"a.ttl","line":18446744073709551615,"column":4294967295,"logical":"urn:logical","gts_term_id":1,"gts_quad_index":2,"gts_reifier_id":3,"gts_frame_index":4,"gts_segment_index":5}],"related_locations":[{"path":"related.ttl"}],"related_labels":[{"location":{"path":"label.ttl"},"message":"defined here"}],"suggestions":["repair"],"advice":[{"standpoint":"advisory","text":"review","help_uri":null}],"remediation":[{"text":"replace","standpoint":"binding","help_uri":"urn:help","artifact_change":{"artifact_uri":"a.ttl","region":{"start_line":1,"start_column":2,"end_line":3,"end_column":4},"replacement":"x\n"}}],"finding_iri":"urn:finding","anchor_iri":"urn:anchor","anchor_non_trivial":true,"antecedents":["urn:antecedent"],"root_cause":"urn:root","cluster":"urn:cluster","cross_node_glut_with":["urn:opposite"],"tags":["tag"],"detail":"detail","failure_class":"urn:failure","category":"transient-chatter","standpoint":"perspectival","attributions":[{"slice_iri":"urn:slice","role":"definition-owner","evidence":"source"}],"documented_terms":["urn:term"],"guidance":[{"modality":"UseWhen","source":"DocumentedTerm","term_iri":"urn:term","text":"usage","standpoint":"advisory","help_uri":"urn:guide"}],"derived_from_quads":["urn:quad"],"cited_iris":["urn:citation"]}"#;

#[test]
fn json_finding_complete_literal_preserves_every_payload_and_exact_bytes() {
    let finding: Finding = from_slice(COMPLETE.as_bytes()).unwrap();
    assert_eq!(to_vec(&finding), COMPLETE.as_bytes());
    assert_eq!(finding.severity, Severity::Warning);
    assert_eq!(finding.code, "authored.code");
    assert_eq!(finding.message, "nul\0\t\"\\中😀");
    assert_eq!(
        finding.source_graph.as_ref().unwrap().as_str(),
        "https://example.org/Graph"
    );
    assert_eq!(finding.source_focus.as_deref(), Some("urn:focus"));
    assert_eq!(finding.tool.as_deref(), Some("tiny"));
    assert_eq!(finding.locations[0].line, Some(u64::MAX));
    assert_eq!(finding.locations[0].column, Some(u32::MAX));
    assert_eq!(finding.locations[0].gts_segment_index, Some(5));
    assert_eq!(
        finding.related_locations[0].path.as_deref(),
        Some("related.ttl")
    );
    assert_eq!(finding.related_labels[0].message, "defined here");
    assert_eq!(finding.suggestions, ["repair"]);
    assert_eq!(finding.advice[0].text, "review");
    assert_eq!(
        finding.remediation[0]
            .artifact_change
            .as_ref()
            .unwrap()
            .replacement,
        "x\n"
    );
    assert_eq!(finding.finding_iri.as_deref(), Some("urn:finding"));
    assert_eq!(finding.anchor_iri.as_deref(), Some("urn:anchor"));
    assert!(finding.anchor_non_trivial);
    assert_eq!(finding.antecedents, ["urn:antecedent"]);
    assert_eq!(finding.root_cause.as_deref(), Some("urn:root"));
    assert_eq!(finding.cluster.as_deref(), Some("urn:cluster"));
    assert_eq!(finding.cross_node_glut_with, ["urn:opposite"]);
    assert_eq!(finding.tags, ["tag"]);
    assert_eq!(finding.detail.as_deref(), Some("detail"));
    assert_eq!(finding.failure_class.as_deref(), Some("urn:failure"));
    assert_eq!(finding.category, Some(FindingCategory::Transient));
    assert_eq!(finding.standpoint, Some(Standpoint::Perspectival));
    assert_eq!(finding.attributions[0].evidence.as_deref(), Some("source"));
    assert_eq!(finding.documented_terms, ["urn:term"]);
    assert_eq!(finding.guidance[0].text, "usage");
    assert_eq!(finding.derived_from_quads, ["urn:quad"]);
    assert_eq!(finding.cited_iris, ["urn:citation"]);
}

#[test]
fn json_finding_minimal_writer_omits_empty_false_and_absent_fields() {
    assert_eq!(
        to_vec(&Finding::new(
            Severity::Error,
            "authored.code",
            "missing evidence"
        )),
        MINIMAL.as_bytes()
    );
    let finding: Finding = from_slice(MINIMAL.as_bytes()).unwrap();
    assert_eq!(
        finding,
        Finding::new(Severity::Error, "authored.code", "missing evidence")
    );
}

#[test]
fn json_finding_optional_nulls_are_absent_but_present_empty_strings_survive() {
    for field in [
        "source_graph",
        "source_focus",
        "tool",
        "finding_iri",
        "anchor_iri",
        "root_cause",
        "cluster",
        "detail",
        "failure_class",
        "category",
        "standpoint",
    ] {
        let input = MINIMAL.replacen('{', &format!("{{\"{field}\":null,"), 1);
        assert_eq!(
            to_vec(&from_slice::<Finding>(input.as_bytes()).unwrap()),
            MINIMAL.as_bytes(),
            "{field}"
        );
    }
    let input = MINIMAL.replacen('{', "{\"tool\":\"\",\"detail\":\"\",", 1);
    let finding: Finding = from_slice(input.as_bytes()).unwrap();
    assert_eq!(finding.tool.as_deref(), Some(""));
    assert_eq!(finding.detail.as_deref(), Some(""));
    assert_eq!(to_vec(&finding), br#"{"severity":"error","code":"authored.code","message":"missing evidence","tool":"","detail":""}"#);
}

#[test]
fn json_finding_default_vectors_refuse_null_and_wrong_member_kinds() {
    for field in [
        "locations",
        "related_locations",
        "related_labels",
        "suggestions",
        "advice",
        "remediation",
        "antecedents",
        "cross_node_glut_with",
        "tags",
        "attributions",
        "documented_terms",
        "guidance",
        "derived_from_quads",
        "cited_iris",
    ] {
        for value in ["null", "{}", "[false]"] {
            let input = MINIMAL.replacen('{', &format!("{{\"{field}\":{value},"), 1);
            assert!(
                from_slice::<Finding>(input.as_bytes()).is_err(),
                "{field}: {value}"
            );
        }
        let input = MINIMAL.replacen('{', &format!("{{\"{field}\":[],"), 1);
        assert_eq!(
            to_vec(&from_slice::<Finding>(input.as_bytes()).unwrap()),
            MINIMAL.as_bytes()
        );
    }
    for token in ["null", "0", "\"false\""] {
        let input = MINIMAL.replacen('{', &format!("{{\"anchor_non_trivial\":{token},"), 1);
        assert!(from_slice::<Finding>(input.as_bytes()).is_err());
    }
    let input = MINIMAL.replacen('{', "{\"anchor_non_trivial\":false,", 1);
    assert_eq!(
        to_vec(&from_slice::<Finding>(input.as_bytes()).unwrap()),
        MINIMAL.as_bytes()
    );
}

#[test]
fn json_finding_known_duplicates_refuse_and_unknown_members_remain_ignored() {
    for (old, new) in [
        (
            "\"code\":\"authored.code\"",
            "\"code\":\"authored.code\",\"code\":\"other\"",
        ),
        (
            "\"anchor_non_trivial\":true",
            "\"anchor_non_trivial\":true,\"anchor_non_trivial\":false",
        ),
        (
            "\"source_graph\":\"https://example.org/Graph\"",
            "\"source_graph\":null,\"source_graph\":\"https://example.org/Graph\"",
        ),
        (
            "\"related_labels\":[",
            "\"related_labels\":[],\"related_labels\":[",
        ),
        (
            "\"message\":\"defined here\"",
            "\"message\":\"defined here\",\"message\":\"duplicate\"",
        ),
    ] {
        assert!(
            from_slice::<Finding>(COMPLETE.replace(old, new).as_bytes()).is_err(),
            "{old}"
        );
    }
    let input = MINIMAL.replacen('{', "{\"unknown\":{\"number\":1e400},\"unknown\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<Finding>(input.as_bytes()).unwrap()),
        MINIMAL.as_bytes()
    );
}

#[test]
fn json_finding_required_fields_records_and_related_labels_fail_closed() {
    for value in [
        "null",
        "[]",
        "[\"error\",\"code\",\"message\"]",
        "{}",
        r#"{"severity":"error","code":"c"}"#,
        r#"{"severity":"error","message":"m"}"#,
        r#"{"code":"c","message":"m"}"#,
        r#"{"severity":"error","code":null,"message":"m"}"#,
    ] {
        assert!(from_slice::<Finding>(value.as_bytes()).is_err(), "{value}");
    }
    for value in [
        "null",
        "[]",
        "{}",
        r#"{"location":{}}"#,
        r#"{"message":"m"}"#,
        r#"{"location":{},"message":null}"#,
        r#"{"location":{},"message":"m","location":{}}"#,
    ] {
        assert!(
            from_slice::<RelatedLabel>(value.as_bytes()).is_err(),
            "{value}"
        );
    }
    let label: RelatedLabel =
        from_slice(br#"{"location":{},"message":"m","unknown":true}"#).unwrap();
    assert_eq!(to_vec(&label), br#"{"location":{},"message":"m"}"#);
}

#[test]
fn json_finding_graph_identity_is_absolute_and_never_normalized() {
    for graph in ["relative", "", "https://example.org/with space"] {
        let input = MINIMAL.replacen('{', &format!("{{\"source_graph\":\"{graph}\","), 1);
        assert!(from_slice::<Finding>(input.as_bytes()).is_err(), "{graph}");
    }
    let graph = "https://EXAMPLE.org:443/a/../Graph%2f?Q=1";
    let input = MINIMAL.replacen('{', &format!("{{\"source_graph\":\"{graph}\","), 1);
    let finding: Finding = from_slice(input.as_bytes()).unwrap();
    assert_eq!(finding.source_graph.as_ref().unwrap().as_str(), graph);
    assert_eq!(to_vec(&finding), format!(r#"{{"severity":"error","code":"authored.code","message":"missing evidence","source_graph":"{graph}"}}"#).as_bytes());
}

#[test]
fn json_finding_nested_unsigned_widths_retain_strict_lexical_admission() {
    for token in ["-0", "-1", "1.0", "1e0", "18446744073709551616", "\"1\""] {
        let input = COMPLETE.replace(
            "\"line\":18446744073709551615",
            &format!("\"line\":{token}"),
        );
        assert!(from_slice::<Finding>(input.as_bytes()).is_err(), "{token}");
    }
    for token in ["-0", "-1", "1.0", "1e0", "4294967296", "\"1\""] {
        let input = COMPLETE.replace("\"column\":4294967295", &format!("\"column\":{token}"));
        assert!(from_slice::<Finding>(input.as_bytes()).is_err(), "{token}");
    }
}

#[test]
fn json_finding_embedded_enum_forms_retain_every_category_and_refuse_malformed_units() {
    for category in FindingCategory::ALL {
        for form in [
            format!("\"{}\"", category.as_str()),
            format!("{{\"{}\":null}}", category.as_str()),
        ] {
            let input = MINIMAL.replacen('{', &format!("{{\"category\":{form},"), 1);
            let finding: Finding = from_slice(input.as_bytes()).unwrap();
            assert_eq!(finding.category, Some(category));
            assert_eq!(to_vec(&finding), format!(r#"{{"severity":"error","code":"authored.code","message":"missing evidence","category":"{}"}}"#, category.as_str()).as_bytes());
        }
    }
    for form in [
        r#"{"transient-chatter":false}"#,
        r#"{"transient-chatter":null,"transient-chatter":null}"#,
        r#""transient""#,
        "{}",
    ] {
        let input = MINIMAL.replacen('{', &format!("{{\"category\":{form},"), 1);
        assert!(from_slice::<Finding>(input.as_bytes()).is_err(), "{form}");
    }
    let input =
        r#"{"severity":{"warning":null},"code":"c","message":"m","standpoint":{"binding":null}}"#;
    assert_eq!(
        to_vec(&from_slice::<Finding>(input.as_bytes()).unwrap()),
        br#"{"severity":"warning","code":"c","message":"m","standpoint":"binding"}"#
    );
}
