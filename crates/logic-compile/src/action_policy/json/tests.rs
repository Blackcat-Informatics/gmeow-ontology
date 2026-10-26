// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored transport rows, without the selected policy document.
use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};
const POLICY: &str = r#"{"schema_version":1,"source_path":"p","source_sha256":"d","source_statements":["a","z"],"rows":[{"subject":"urn:s","predicate":"urn:p","object":{"Iri":"urn:o"}},{"subject":"urn:t","predicate":"urn:q","object":{"Literal":{"lexical":"中\n\"\\","datatype":null,"language":"ar","direction":true}}}],"omitted_annotations":["a","b"],"nquads":"original\nbytes"}"#;
#[test]
fn json_prepared_policy_complete_literal_preserves_boolean_direction_and_lazy_state() {
    let policy = PreparedActionPolicy::decode_json(POLICY.as_bytes()).unwrap();
    assert_eq!(to_vec(&policy), POLICY.as_bytes());
    assert!(policy.dataset.get().is_none());
    assert!(
        policy.validate().is_err(),
        "transport admission does not assert policy identity"
    );
    assert!(policy.dataset.get().is_none());
    let PolicyObject::Literal {
        lexical,
        language,
        direction,
        ..
    } = &policy.rows[1].object
    else {
        panic!("literal")
    };
    assert_eq!(lexical, "中\n\"\\");
    assert_eq!(language.as_deref(), Some("ar"));
    assert_eq!(*direction, Some(true));
    for (wire, expected) in [
        (
            r#"{"Literal":{"lexical":"x"}}"#,
            r#"{"Literal":{"lexical":"x","datatype":null,"language":null,"direction":null}}"#,
        ),
        (
            r#"{"Literal":{"lexical":"x","datatype":null,"language":null,"direction":false}}"#,
            r#"{"Literal":{"lexical":"x","datatype":null,"language":null,"direction":false}}"#,
        ),
    ] {
        let value: PolicyObject = from_slice(wire.as_bytes()).unwrap();
        assert_eq!(to_vec(&value), expected.as_bytes());
    }
}
#[test]
fn json_prepared_policy_strict_fields_variants_and_required_coordinates_refuse() {
    for bad in [
        POLICY.replace(
            r#""schema_version":1"#,
            r#""schema_version":1,"schema_version":1"#,
        ),
        POLICY.replace(
            r#""schema_version":1"#,
            r#""extra":null,"schema_version":1"#,
        ),
        POLICY.replace(r#""schema_version":1,"#, ""),
        POLICY.replace(r#""rows":["#, r#""dataset":null,"rows":["#),
        POLICY.replace(r#""subject":"urn:s""#, r#""subject":"urn:s","other":null"#),
        POLICY.replace(r#""direction":true"#, r#""direction":true,"extra":null"#),
        POLICY.replace(
            r#"{"Iri":"urn:o"}"#,
            r#"{"Iri":"urn:o","Iri":"urn:second"}"#,
        ),
        POLICY.replace(r#"{"Iri":"urn:o"}"#, r#"{"Other":"urn:o"}"#),
        format!("{POLICY} []"),
    ] {
        assert!(
            PreparedActionPolicy::decode_json(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad = POLICY.replace(r#""direction":true"#, r#""direction":"rtl""#);
    assert_eq!(
        PreparedActionPolicy::decode_json(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/rows/1/object/Literal/direction"
    );
    assert!(
        from_slice::<Row>(br#"["s","p",{"Iri":"o"}]"#).is_err(),
        "policy records are object-only"
    );
    for bad in [
        r#"{"Iri":null}"#,
        r#"{"Literal":null}"#,
        r#"{"Literal":{"lexical":null}}"#,
        r#"{"Literal":{"lexical":"x","language":4}}"#,
    ] {
        assert!(from_slice::<PolicyObject>(bad.as_bytes()).is_err(), "{bad}");
    }
}
#[test]
fn json_prepared_policy_schema_unsigned_bounds_and_set_order_are_exact() {
    for token in ["-0", "-1", "1.0", "1e0", "4294967296", "null"] {
        let bad = POLICY.replace(
            r#""schema_version":1"#,
            &format!("\"schema_version\":{token}"),
        );
        assert_eq!(
            PreparedActionPolicy::decode_json(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/schema_version",
            "{token}"
        );
    }
    let max = POLICY.replace(r#""schema_version":1"#, r#""schema_version":4294967295"#);
    assert_eq!(
        PreparedActionPolicy::decode_json(max.as_bytes())
            .unwrap()
            .schema_version,
        u32::MAX
    );
    let sets = POLICY.replace(
        r#""source_statements":["a","z"]"#,
        r#""source_statements":["z","a","z"]"#,
    );
    assert_eq!(
        to_vec(&PreparedActionPolicy::decode_json(sets.as_bytes()).unwrap()),
        POLICY.as_bytes()
    );
}
