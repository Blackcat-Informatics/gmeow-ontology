// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Plain authored records; no registry, source loader or corpus is constructed.
use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};
const MEDIUM: &str = r#"{"source_path":"中\n\"\\","source_digest":"digest","dictionaries":[{"name":{"Iri":"urn:dictionary"},"values":{"a":[],"z":[{"Literal":"lexical"},{"Other":"_:b"},{"Literal":"lexical"}]}}],"corpora":[],"splits":[],"schemas":[],"classes":{"a":null},"gmn_terms":["a","z"],"shacl_terms":[]}"#;
#[test]
fn json_medium_observation_complete_literal_keeps_order_and_explicit_absence() {
    let observed: Observations = from_slice(MEDIUM.as_bytes()).unwrap();
    assert_eq!(to_vec(&observed), MEDIUM.as_bytes());
    assert_eq!(observed.source_path, "中\n\"\\");
    assert_eq!(observed.dictionaries[0].values["a"].len(), 0);
    assert!(!observed.dictionaries[0].values.contains_key("missing"));
    assert!(observed.classes["a"].is_none());
    assert!(!observed.classes.contains_key("missing"));
    let changed = MEDIUM.replace(r#""classes":{"a":null}"#, r#""classes":{}"#);
    assert_eq!(
        to_vec(&from_slice::<Observations>(changed.as_bytes()).unwrap()),
        changed.as_bytes()
    );
    let unordered = MEDIUM.replace(r#""gmn_terms":["a","z"]"#, r#""gmn_terms":["z","a","z"]"#);
    assert_eq!(
        to_vec(&from_slice::<Observations>(unordered.as_bytes()).unwrap()),
        MEDIUM.as_bytes()
    );
    let unknown = MEDIUM.replace(r#""source_path":"#, r#""extra":null,"source_path":"#);
    assert_eq!(
        to_vec(&from_slice::<Observations>(unknown.as_bytes()).unwrap()),
        MEDIUM.as_bytes()
    );
}
#[test]
fn json_medium_observation_record_fields_tags_and_nested_pointers_refuse_bad_data() {
    for bad in [
        MEDIUM.replace(r#""classes":{"a":null},"#, ""),
        MEDIUM.replace(r#""classes":{"a":null}"#, r#""classes":null"#),
        MEDIUM.replace(
            r#""source_digest":"digest""#,
            r#""source_digest":"digest","source_digest":"other""#,
        ),
        MEDIUM.replace(
            r#"{"Iri":"urn:dictionary"}"#,
            r#"{"Iri":"urn:dictionary","Iri":"second"}"#,
        ),
        MEDIUM.replace(r#"{"Iri":"urn:dictionary"}"#, r#"{"Unknown":"x"}"#),
        format!("{MEDIUM} null"),
    ] {
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    let bad = MEDIUM.replace(r#"{"Other":"_:b"}"#, r#"{"Other":7}"#);
    assert_eq!(
        from_slice::<Observations>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/dictionaries/0/values/z/1/Other"
    );
    assert!(
        from_slice::<Node>(br#"[{"Iri":"a"},{}]"#).is_err(),
        "records are object-only"
    );
}
#[test]
fn json_medium_observation_maps_admit_every_duplicate_value_then_keep_last() {
    let duplicate = MEDIUM.replace(
        r#""classes":{"a":null}"#,
        r#""classes":{"a":{"name":{"Iri":"first"},"values":{}},"a":null}"#,
    );
    assert_eq!(
        to_vec(&from_slice::<Observations>(duplicate.as_bytes()).unwrap()),
        MEDIUM.as_bytes()
    );
    let invalid = MEDIUM.replace(r#""classes":{"a":null}"#, r#""classes":{"a":3,"a":null}"#);
    assert_eq!(
        from_slice::<Observations>(invalid.as_bytes())
            .unwrap_err()
            .pointer(),
        "/classes/a"
    );
    let values = MEDIUM.replace(r#""a":[],"z":"#, r#""a":[{"Literal":"first"}],"a":[],"z":"#);
    assert_eq!(
        to_vec(&from_slice::<Observations>(values.as_bytes()).unwrap()),
        MEDIUM.as_bytes()
    );
}
