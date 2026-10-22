// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Plain advisory records only: no shapes, ontology, artifact selector or producer.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const ADVISORY: &str = r#"{"code":"advice.z","message":"中\n\"\\","severity":"note","suggestions":["z","a"],"subject_iri":null,"claim_code":"claim.z","claim_subject_iri":"urn:claim","modality_iri":"urn:modality","standpoint_iri":"urn:standpoint","confidence":1.0,"verdict_iri":"urn:verdict","advised_proposition":"proposition"}"#;
const FINDING: &str = r#"{"focus":"urn:focus","component":"component","source_shape":"shape","severity":"severity","failure_class":null}"#;

/// Independent data model with deliberately unsorted map/set insertion and vectors.
fn authored_observation(confidence: f64) -> Observation {
    Observation {
        profile: "profile".to_owned(),
        shapes_digest: "shapes".to_owned(),
        input_digest: "input".to_owned(),
        shape_sources: [
            ("z".to_owned(), Default::default()),
            (
                "a".to_owned(),
                [(
                    "p".to_owned(),
                    ["<urn:z>".to_owned(), "<urn:a>".to_owned()].into(),
                )]
                .into(),
            ),
        ]
        .into(),
        retained_conforms: false,
        retained_findings: vec![RetainedFinding {
            focus: "urn:focus".to_owned(),
            component: "component".to_owned(),
            source_shape: "shape".to_owned(),
            severity: "severity".to_owned(),
            failure_class: None,
        }],
        advisories: vec![Advisory {
            code: "advice.z".to_owned(),
            message: "中\n\"\\".to_owned(),
            severity: gmeow_errors::Severity::Note,
            suggestions: vec!["z".to_owned(), "a".to_owned()],
            subject_iri: None,
            claim_code: "claim.z".to_owned(),
            claim_subject_iri: Some("urn:claim".to_owned()),
            modality_iri: "urn:modality".to_owned(),
            standpoint_iri: "urn:standpoint".to_owned(),
            confidence,
            verdict_iri: "urn:verdict".to_owned(),
            advised_proposition: "proposition".to_owned(),
        }],
        claims_nquads: "<urn:s> <urn:p> <urn:o> .\n".to_owned(),
    }
}

#[test]
fn json_advice_wing_literal_envelope_pins_all_records_order_unicode_and_collections() {
    let observation = authored_observation(1.0);
    let expected = format!(
        r#"{{"profile":"profile","shapes_digest":"shapes","input_digest":"input","shape_sources":{{"a":{{"p":["<urn:a>","<urn:z>"]}},"z":{{}}}},"retained_conforms":false,"retained_findings":[{FINDING}],"advisories":[{ADVISORY}],"claims_nquads":"<urn:s> <urn:p> <urn:o> .\n"}}"#
    );
    assert_eq!(
        to_vec(&observation.retained_findings[0]),
        FINDING.as_bytes()
    );
    assert_eq!(observation.to_json_bytes().unwrap(), expected.as_bytes());
    let restored: Observation = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(restored.to_json_bytes().unwrap(), expected.as_bytes());
    assert_eq!(
        restored.advisories[0].confidence.to_bits(),
        1.0_f64.to_bits()
    );
    assert_eq!(restored.advisories[0].suggestions, ["z", "a"]);
}

#[test]
fn json_advice_wing_finite_output_keeps_native_binary64_layout_and_signed_zero() {
    for (confidence, lexical) in [
        (1.0, "1.0"),
        (-0.0, "-0.0"),
        (0.1, "0.1"),
        (1e-5, "0.00001"),
        (1e-6, "1e-6"),
        (1e15, "1000000000000000.0"),
        (1e16, "1e+16"),
        (f64::MAX, "1.7976931348623157e+308"),
        (f64::from_bits(1), "5e-324"),
        (-2.5, "-2.5"),
    ] {
        let observation = authored_observation(confidence);
        let expected = ADVISORY.replace("\"confidence\":1.0", &format!("\"confidence\":{lexical}"));
        let bytes = observation.to_json_bytes().unwrap();
        assert!(
            String::from_utf8(bytes.clone())
                .unwrap()
                .contains(&expected),
            "{lexical}"
        );
        let restored: Observation = from_slice(&bytes).unwrap();
        assert_eq!(
            restored.advisories[0].confidence.to_bits(),
            confidence.to_bits()
        );
    }
}

#[test]
fn json_advice_wing_manually_constructed_nonfinite_values_fail_before_returning_bytes() {
    for confidence in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut observation = authored_observation(1.0);
        let mut invalid: Advisory = from_slice(ADVISORY.as_bytes()).unwrap();
        invalid.code = "advice.invalid".to_owned();
        invalid.confidence = confidence;
        observation.advisories.push(invalid);
        let error = observation.to_json_bytes().unwrap_err();
        let primary = error.to_string();
        assert!(primary.contains("advisory 1"), "{primary}");
        assert!(primary.contains("advice.invalid"), "{primary}");
        assert!(primary.contains("/advisories/1/confidence"), "{primary}");
        assert!(primary.contains("finite f64"), "{primary}");
    }
}

#[test]
fn json_advice_wing_decoding_requires_finite_numbers_and_keeps_nested_pointer() {
    for token in ["1e400", "-1e400", "null", "\"1.0\"", "false"] {
        let row = ADVISORY.replace("\"confidence\":1.0", &format!("\"confidence\":{token}"));
        let input = format!(
            r#"{{"profile":"p","shapes_digest":"s","input_digest":"i","shape_sources":{{}},"retained_conforms":true,"retained_findings":[],"advisories":[{row}],"claims_nquads":""}}"#
        );
        assert_eq!(
            from_slice::<Observation>(input.as_bytes())
                .unwrap_err()
                .pointer(),
            "/advisories/0/confidence"
        );
    }
    for (token, expected) in [
        ("1", 1.0_f64),
        ("1e0", 1.0),
        ("-0", -0.0),
        ("1e-400", 0.0),
        ("-1e-400", -0.0),
        ("-2.5", -2.5),
        ("2", 2.0),
    ] {
        let input = ADVISORY.replace("\"confidence\":1.0", &format!("\"confidence\":{token}"));
        assert_eq!(
            from_slice::<Advisory>(input.as_bytes())
                .unwrap()
                .confidence
                .to_bits(),
            expected.to_bits()
        );
    }
}

#[test]
fn json_advice_wing_optional_null_missing_empty_and_severity_forms_remain_distinct() {
    let absent = ADVISORY.replace("\"subject_iri\":null,", "");
    let row: Advisory = from_slice(absent.as_bytes()).unwrap();
    assert_eq!(row.subject_iri, None);
    let empty = ADVISORY.replace("\"subject_iri\":null", "\"subject_iri\":\"\"");
    assert_eq!(
        from_slice::<Advisory>(empty.as_bytes())
            .unwrap()
            .subject_iri
            .as_deref(),
        Some("")
    );
    let absent = FINDING.replace(",\"failure_class\":null", "");
    assert_eq!(
        to_vec(&from_slice::<RetainedFinding>(absent.as_bytes()).unwrap()),
        FINDING.as_bytes()
    );
    for (tag, expected) in [
        ("note", gmeow_errors::Severity::Note),
        ("info", gmeow_errors::Severity::Info),
        ("warning", gmeow_errors::Severity::Warning),
        ("error", gmeow_errors::Severity::Error),
    ] {
        let string = ADVISORY.replace("\"severity\":\"note\"", &format!("\"severity\":\"{tag}\""));
        let object = ADVISORY.replace(
            "\"severity\":\"note\"",
            &format!("\"severity\":{{\"{tag}\":null}}"),
        );
        assert_eq!(
            from_slice::<Advisory>(string.as_bytes()).unwrap().severity,
            expected
        );
        assert_eq!(
            from_slice::<Advisory>(object.as_bytes()).unwrap().severity,
            expected
        );
    }
}

#[test]
fn json_advice_wing_record_policies_refuse_required_null_duplicates_and_positional_arrays() {
    for bad in [
        "{}".to_owned(),
        "[]".to_owned(),
        "null".to_owned(),
        ADVISORY.replace("\"code\":\"advice.z\",", ""),
        ADVISORY.replace("\"suggestions\":[\"z\",\"a\"]", "\"suggestions\":null"),
        ADVISORY.replace(
            "\"confidence\":1.0",
            "\"confidence\":1.0,\"confidence\":1.0",
        ),
        ADVISORY.replace(
            "\"subject_iri\":null",
            "\"subject_iri\":null,\"subject_iri\":null",
        ),
        ADVISORY.replace("\"severity\":\"note\"", "\"severity\":{\"note\":false}"),
    ] {
        assert!(from_slice::<Advisory>(bad.as_bytes()).is_err(), "{bad}");
    }
    let unknown = ADVISORY.replacen('{', "{\"unknown\":1e400,", 1);
    assert_eq!(
        from_slice::<Advisory>(unknown.as_bytes()).unwrap().code,
        "advice.z"
    );
    assert!(from_slice::<Advisory>(format!("{ADVISORY} false").as_bytes()).is_err());
    assert!(from_slice::<Observation>(b"{").is_err());
    assert!(from_slice::<RetainedFinding>(br#"{"focus":"f","component":"c","source_shape":"s","severity":"v","failure_class":null,"failure_class":null}"#).is_err());
    assert!(
        from_slice::<RetainedFinding>(FINDING.replace("\"focus\":\"urn:focus\",", "").as_bytes())
            .is_err()
    );
    let unknown_finding = FINDING.replacen('{', "{\"unknown\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<RetainedFinding>(unknown_finding.as_bytes()).unwrap()),
        FINDING.as_bytes()
    );
    let empty = r#"{"profile":"p","shapes_digest":"s","input_digest":"i","shape_sources":{},"retained_conforms":true,"retained_findings":[],"advisories":[],"claims_nquads":""}"#;
    for invalid in [
        "[]".to_owned(),
        "{}".to_owned(),
        empty.replace("\"profile\":\"p\",", ""),
        empty.replace("\"advisories\":[]", "\"advisories\":null"),
        empty.replace("\"profile\":\"p\"", "\"profile\":\"p\",\"profile\":\"p\""),
    ] {
        assert!(
            from_slice::<Observation>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let unknown_observation = empty.replacen('{', "{\"unknown\":1e400,", 1);
    assert_eq!(
        from_slice::<Observation>(unknown_observation.as_bytes())
            .unwrap()
            .to_json_bytes()
            .unwrap(),
        empty.as_bytes()
    );
}

#[test]
fn json_advice_wing_vectors_preserve_order_and_map_values_admit_before_last_wins() {
    let mut observation = authored_observation(1.0);
    let second: Advisory = from_slice(ADVISORY.replace("advice.z", "advice.a").as_bytes()).unwrap();
    observation.advisories.push(second);
    let bytes = observation.to_json_bytes().unwrap();
    let restored: Observation = from_slice(&bytes).unwrap();
    assert_eq!(
        restored
            .advisories
            .iter()
            .map(|row| row.code.as_str())
            .collect::<Vec<_>>(),
        ["advice.z", "advice.a"]
    );
    let valid = br#"{"profile":"p","shapes_digest":"s","input_digest":"i","shape_sources":{"x":{},"x":{"p":["z","a","z"]}},"retained_conforms":true,"retained_findings":[],"advisories":[],"claims_nquads":""}"#;
    let restored: Observation = from_slice(valid).unwrap();
    assert_eq!(
        restored.shape_sources["x"]["p"]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    let invalid = String::from_utf8(valid.to_vec())
        .unwrap()
        .replace("\"x\":{}", "\"x\":null");
    assert!(from_slice::<Observation>(invalid.as_bytes()).is_err());
    let empty = br#"{"profile":"p","shapes_digest":"s","input_digest":"i","shape_sources":{},"retained_conforms":true,"retained_findings":[],"advisories":[],"claims_nquads":""}"#;
    assert_eq!(
        from_slice::<Observation>(empty)
            .unwrap()
            .to_json_bytes()
            .unwrap(),
        empty
    );
}
