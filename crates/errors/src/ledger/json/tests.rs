// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored JSON and in-memory diagnostics only. No corpus,
//! repository source, fixture selector, renderer or producer is loaded.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const MINIMAL: &str = r#"{"fingerprint":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"stage":"tiny","grade":{"severity":"error","category":"data-shape-violation","standpoint":"binding"},"code":"authored.code","observations":[],"frames":[],"antecedents":[],"source_ctx":{"location":{},"focus":null,"term_role":null},"attributions":[],"advice":[],"labels":[],"tags":[],"knowledge":"supported","emitted_at":{"file":"authored.rs","line":1,"column":2}}"#;

const COMPLETE: &str = r#"{"fingerprint":[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15],"stage":"étape\n😀","grade":{"severity":"warning","category":"projection-loss","standpoint":"perspectival"},"code":"authored.code","observations":[{"message":"nul\u0000\t\"\\中","observed":{"lexical":"01","datatype":"urn:datatype"},"expected":{"lexical":"1","datatype":null}}],"frames":[{"message":"frame","at":{"file":"authored.rs","line":4294967295,"column":2}}],"antecedents":[[15,14,13,12,11,10,9,8,7,6,5,4,3,2,1,0]],"source_ctx":{"location":{"path":"a.ttl","line":18446744073709551615,"column":4294967295,"logical":"urn:logical","gts_term_id":1,"gts_quad_index":2,"gts_reifier_id":3,"gts_frame_index":4,"gts_segment_index":5},"graph":"https://example.org/graph","focus":"urn:focus","term_role":"object"},"attributions":[{"slice_iri":"urn:slice","role":"evaluation-scope","evidence":"original"}],"advice":[{"standpoint":"advisory","text":"advice","help_uri":null}],"remediation":[{"text":"repair","standpoint":"binding","help_uri":"https://example.org/help","artifact_change":{"artifact_uri":"a.ttl","region":{"start_line":1,"start_column":2,"end_line":3,"end_column":4},"replacement":"x\n"}}],"guidance":[{"modality":"HowToUse","source":"DocumentedTerm","term_iri":"urn:term","text":"usage","standpoint":"advisory","help_uri":"https://example.org/term"}],"derived_from_quads":["urn:quad"],"cited_iris":["urn:citation"],"labels":[{"location":{"path":"other.ttl"},"text":"other"}],"tags":["tag"],"documented_terms":["urn:term"],"failure_class":"urn:failure","knowledge":"both","emitted_at":{"file":"authored.rs","line":1,"column":2},"locus_stage":"original-stage"}"#;

#[test]
fn json_diag_complete_literal_round_trips_every_payload_and_exact_bytes() {
    let node: DiagNode = from_slice(COMPLETE.as_bytes()).unwrap();
    assert_eq!(to_vec(&node), COMPLETE.as_bytes());
    assert_eq!(
        node.fingerprint.0,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    assert_eq!(node.antecedents.len(), 1);
    assert_eq!(node.observations[0].message, "nul\0\t\"\\中");
    assert_eq!(node.source_ctx.location.line, Some(u64::MAX));
    assert_eq!(node.guidance[0].source, GuidanceSource::DocumentedTerm);
}

#[test]
fn json_diag_minimal_literal_preserves_required_empty_and_explicit_null_fields() {
    let node: DiagNode = from_slice(MINIMAL.as_bytes()).unwrap();
    assert_eq!(to_vec(&node), MINIMAL.as_bytes());
    assert!(node.remediation.is_empty());
    assert!(node.failure_class.is_none());
    assert_eq!(node.source_ctx.graph, None);
    assert!(node.locus_stage.is_none());
}

#[test]
fn json_diag_defaults_distinguish_missing_vectors_from_explicit_null() {
    for field in [
        "remediation",
        "guidance",
        "derived_from_quads",
        "cited_iris",
        "documented_terms",
    ] {
        let null = MINIMAL.replacen('{', &format!("{{\"{field}\":null,"), 1);
        assert!(from_slice::<DiagNode>(null.as_bytes()).is_err(), "{field}");
        let empty = MINIMAL.replacen('{', &format!("{{\"{field}\":[],"), 1);
        assert_eq!(
            to_vec(&from_slice::<DiagNode>(empty.as_bytes()).unwrap()),
            MINIMAL.as_bytes()
        );
    }
    let absent = MINIMAL.replace("\"observations\":[],", "");
    assert!(from_slice::<DiagNode>(absent.as_bytes()).is_err());
    let absent = MINIMAL
        .replace("\"focus\":null,", "")
        .replace(",\"term_role\":null", "");
    assert_eq!(
        to_vec(&from_slice::<DiagNode>(absent.as_bytes()).unwrap()),
        MINIMAL.as_bytes()
    );
}

#[test]
fn json_diag_duplicate_known_fields_refuse_while_unknown_members_remain_ignored() {
    for (old, new) in [
        (
            "\"code\":\"authored.code\"",
            "\"code\":\"authored.code\",\"code\":\"duplicate\"",
        ),
        ("\"focus\":null", "\"focus\":null,\"focus\":null"),
        ("\"line\":1", "\"line\":1,\"line\":2"),
    ] {
        assert!(from_slice::<DiagNode>(MINIMAL.replace(old, new).as_bytes()).is_err());
    }
    let unknown = MINIMAL.replacen('{', "{\"unrecognized\":{\"x\":null},", 1);
    assert_eq!(
        to_vec(&from_slice::<DiagNode>(unknown.as_bytes()).unwrap()),
        MINIMAL.as_bytes()
    );
    let unknown = MINIMAL.replacen('{', "{\"unrecognized\":1e400,", 1);
    assert_eq!(
        to_vec(&from_slice::<DiagNode>(unknown.as_bytes()).unwrap()),
        MINIMAL.as_bytes()
    );
}

#[test]
fn json_diag_unsigned_fields_refuse_negative_zero_fraction_exponent_and_overflow() {
    for token in ["-0", "-1", "1.0", "1e0", "4294967296", "\"1\"", "null"] {
        let value = format!("{{\"file\":\"a\",\"line\":{token},\"column\":1}}");
        assert!(
            from_slice::<SerLocation>(value.as_bytes()).is_err(),
            "{token}"
        );
    }
    for token in ["-0", "-1", "1.0", "1e0", "18446744073709551616", "\"1\""] {
        let value = format!("{{\"line\":{token}}}");
        assert!(from_slice::<Location>(value.as_bytes()).is_err(), "{token}");
    }
    assert_eq!(
        from_slice::<Location>(br#"{"line":18446744073709551615}"#)
            .unwrap()
            .line,
        Some(u64::MAX)
    );
}

#[test]
fn json_diag_fingerprint_requires_exactly_sixteen_strict_unsigned_bytes() {
    for value in [
        "[]",
        "[0]",
        "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]",
        "[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]",
    ] {
        assert!(from_slice::<DiagFingerprint>(value.as_bytes()).is_err());
    }
    for token in ["-0", "-1", "256", "1.0", "1e0", "\"0\""] {
        let value = format!("[{token},0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]");
        assert!(
            from_slice::<DiagFingerprint>(value.as_bytes()).is_err(),
            "{token}"
        );
    }
}

fn enum_forms<T: FromJson + ToJson>(name: &str) {
    let string = format!("\"{name}\"");
    assert_eq!(
        to_vec(&from_slice::<T>(string.as_bytes()).unwrap()),
        string.as_bytes()
    );
    let object = format!("{{\"{name}\":null}}");
    assert_eq!(
        to_vec(&from_slice::<T>(object.as_bytes()).unwrap()),
        string.as_bytes()
    );
    for value in [
        format!("{{\"{name}\":false}}"),
        format!("{{\"{name}\":null,\"{name}\":null}}"),
        format!("{{\"{name}\":null,\"unknown\":null}}"),
        "{}".to_owned(),
        "\"unknown\"".to_owned(),
        "{\"unknown\":null}".to_owned(),
        "null".to_owned(),
    ] {
        assert!(from_slice::<T>(value.as_bytes()).is_err(), "{value}");
    }
}

#[test]
fn json_diag_enum_forms_preserve_typed_spellings_and_exact_unit_object_policy() {
    enum_forms::<Severity>("error");
    for category in FindingCategory::ALL {
        enum_forms::<FindingCategory>(category.as_str());
        assert_eq!(
            from_slice::<FindingCategory>(&to_vec(&category)).unwrap(),
            category
        );
    }
    assert_eq!(
        to_vec(&FindingCategory::Transient),
        br#""transient-chatter""#
    );
    assert_eq!(
        from_slice::<FindingCategory>(br#""transient-chatter""#).unwrap(),
        FindingCategory::Transient
    );
    assert_eq!(
        from_slice::<FindingCategory>(br#"{"transient-chatter":null}"#).unwrap(),
        FindingCategory::Transient
    );
    let unknown = from_slice::<FindingCategory>(br#""transient""#)
        .unwrap_err()
        .to_string();
    assert!(unknown.contains("transient-chatter"), "{unknown}");
    enum_forms::<Standpoint>("perspectival");
    enum_forms::<Belnap>("neither");
    enum_forms::<TermRole>("graph");
    enum_forms::<GuidanceModality>("AvoidWhen");
    enum_forms::<GuidanceSource>("RuleGoverningTerm");
}

#[test]
fn json_diag_graph_identity_uses_native_absolute_iri_admission() {
    for graph in ["relative", "", "https://example.org/with space"] {
        let source = MINIMAL.replace(
            "\"location\":{}",
            &format!("\"location\":{{}},\"graph\":\"{graph}\""),
        );
        assert!(
            from_slice::<DiagNode>(source.as_bytes()).is_err(),
            "{graph}"
        );
    }
    let source = MINIMAL.replace("\"location\":{}", "\"location\":{},\"graph\":null");
    assert_eq!(
        to_vec(&from_slice::<DiagNode>(source.as_bytes()).unwrap()),
        MINIMAL.as_bytes()
    );
}

#[test]
fn json_diag_records_refuse_positional_struct_arrays_and_wrong_newtype_kinds() {
    assert!(from_slice::<SerLocation>(br#"["a",1,2]"#).is_err());
    assert!(from_slice::<StageId>(b"[\"a\"]").is_err());
    assert!(from_slice::<Focus>(b"null").is_err());
    assert!(from_slice::<DiagNode>(b"[]").is_err());
}

#[test]
fn json_diag_native_replay_preserves_causal_closure_and_original_identity() {
    let mut ledger = crate::DiagLedger::new();
    let cause = ledger.attach(
        crate::Diag::from(std::io::Error::other("original read")).with_focus("urn:cause"),
        StageId::new("read"),
    );
    ledger.attach(
        crate::Diag::from(std::io::Error::other("selected operation"))
            .with_focus("urn:result")
            .with_antecedents([cause]),
        StageId::new("operation"),
    );
    let bytes = to_vec(&ledger.emit_sorted());
    let nodes: Vec<DiagNode> = from_slice(&bytes).unwrap();
    assert!(nodes.iter().any(|node| !node.antecedents.is_empty()));
    let mut replay = crate::DiagLedger::new();
    replay.replay(nodes);
    assert_eq!(to_vec(&replay.emit_sorted()), bytes);
    assert_eq!(replay.verdict(), ledger.verdict());
}

#[test]
#[should_panic(expected = "pinned digest")]
fn json_diag_supplied_fingerprint_tampering_is_not_recomputed_or_hidden() {
    let mut ledger = crate::DiagLedger::new();
    ledger.replay([from_slice::<DiagNode>(MINIMAL.as_bytes()).unwrap()]);
}
