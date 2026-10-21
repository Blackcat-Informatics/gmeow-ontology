// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Decoder contracts over independently authored JSON bytes, without bundles or corpus.

use super::*;

#[test]
fn json_bundled_schema_keys_preserve_unicode_escapes_and_sorted_identity() {
    let keys = modeled_def_keys_from_json(
        br#"{"$defs":{"Z":{},"\u732b":null,"a\"b":false,"":[],"line\nbreak":{}}}"#,
    )
    .unwrap();
    assert_eq!(
        keys,
        BTreeSet::from([
            "".to_owned(),
            "Z".to_owned(),
            "a\"b".to_owned(),
            "line\nbreak".to_owned(),
            "猫".to_owned(),
        ])
    );
}

#[test]
fn json_bundled_schema_duplicate_defs_selects_the_last_object_or_empty_shape() {
    let keys = modeled_def_keys_from_json(
        br#"{"$defs":{"old":{}},"$defs":{"new":{"nested":1,"nested":2},"new":null,"z":{}}}"#,
    )
    .unwrap();
    assert_eq!(keys, BTreeSet::from(["new".to_owned(), "z".to_owned()]));
    for text in [
        r#"{"$defs":{"old":{}},"$defs":null}"#,
        r#"{"$defs":{"old":{}},"$defs":[]}"#,
    ] {
        assert!(
            modeled_def_keys_from_json(text.as_bytes())
                .unwrap()
                .is_empty(),
            "{text}"
        );
    }
    let keys = modeled_def_keys_from_json(br#"{"$defs":null,"$defs":{"last":{}}}"#).unwrap();
    assert_eq!(keys, BTreeSet::from(["last".to_owned()]));
}

#[test]
fn json_bundled_schema_extraction_preserves_existing_empty_shape_policy() {
    for text in [
        "null",
        "[]",
        "[{}]",
        "true",
        "false",
        "0",
        "\"schema\"",
        "{}",
        r#"{"other":{}}"#,
        r#"{"$defs":null}"#,
        r#"{"$defs":[]}"#,
        r#"{"$defs":false}"#,
        r#"{"$defs":0}"#,
        r#"{"$defs":"not an object"}"#,
        r#"{"$defs":{}}"#,
    ] {
        assert!(
            modeled_def_keys_from_json(text.as_bytes())
                .unwrap()
                .is_empty(),
            "{text}"
        );
    }
}

#[test]
fn json_bundled_schema_ignored_numbers_use_native_rfc_admission_without_key_rounding() {
    let keys = modeled_def_keys_from_json(
        br#"{"unused":1e400,"$defs":{"1e400":{"maximum":-1e400},"9007199254740993":{"unused":18446744073709551616},"-0":null,"0":null}}"#,
    ).unwrap();
    assert_eq!(
        keys,
        BTreeSet::from([
            "-0".to_owned(),
            "0".to_owned(),
            "1e400".to_owned(),
            "9007199254740993".to_owned(),
        ])
    );
    // The native parser's wider numeric admission does not coerce denied members to strings.
    assert!(denied_cells_from_json(br#"[[1e400,"p","o"]]"#).is_err());
}

#[test]
fn json_bundled_schema_malformed_bytes_fail_with_schema_context() {
    for raw in [
        &b""[..],
        &b"{"[..],
        &b"{} true"[..],
        &b"{}{}"[..],
        &b"{\"$defs\":{\"x\":NaN}}"[..],
        &b"{\"x\":Infinity}"[..],
        &b"\xff"[..],
        &b"\xef\xbb\xbf{}"[..],
        &br#"{"$defs":{"\uD800":{}}}"#[..],
    ] {
        let error = modeled_def_keys_from_json(raw).unwrap_err();
        assert!(error.downcast_ref::<BundleJson>().is_some(), "{error}");
        assert!(error.to_string().contains("gmeow.schema.json"), "{error}");
    }
}

#[test]
fn json_bundled_denied_rows_keep_strings_order_duplicates_and_empty_members() {
    let raw = br#"[["z","p","o"],["\u732b","p\n\"",""],["z","p","o"]]"#;
    assert_eq!(
        denied_cells_from_json(raw).unwrap(),
        vec![
            ("z".to_owned(), "p".to_owned(), "o".to_owned()),
            ("猫".to_owned(), "p\n\"".to_owned(), String::new()),
            ("z".to_owned(), "p".to_owned(), "o".to_owned()),
        ]
    );
    assert!(denied_cells_from_json(b"[]").unwrap().is_empty());
    assert_eq!(
        denied_cells_from_json(br#"[["-0","1e400","9007199254740993"]]"#).unwrap(),
        vec![(
            "-0".to_owned(),
            "1e400".to_owned(),
            "9007199254740993".to_owned()
        )]
    );
}

#[test]
fn json_bundled_denied_rows_require_arrays_of_strings_without_coercion() {
    for text in [
        "null",
        "{}",
        "true",
        "1",
        "\"rows\"",
        "[null]",
        "[{}]",
        "[true]",
        "[1]",
        "[\"row\"]",
        r#"[["s",null,"o"]]"#,
        r#"[["s",false,"o"]]"#,
        r#"[["s",1,"o"]]"#,
        r#"[["s",[],"o"]]"#,
        r#"[["s",{},"o"]]"#,
    ] {
        let error = denied_cells_from_json(text.as_bytes()).unwrap_err();
        assert!(
            error.downcast_ref::<BundleJson>().is_some(),
            "{text}: {error}"
        );
    }
    let error = denied_cells_from_json(br#"[["s",1,"o"]]"#).unwrap_err();
    assert!(error.to_string().contains("/0/1"), "{error}");
}

#[test]
fn json_bundled_denied_rows_keep_exact_tuple_width_refusal() {
    for text in [
        "[[]]",
        r#"[["s"]]"#,
        r#"[["s","p"]]"#,
        r#"[["s","p","o","extra"]]"#,
    ] {
        let error = denied_cells_from_json(text.as_bytes()).unwrap_err();
        assert!(error.downcast_ref::<BundleJson>().is_some(), "{error}");
        assert!(
            error
                .to_string()
                .contains("denied-cells row is not a 3-tuple"),
            "{text}: {error}"
        );
    }
}

#[test]
fn json_bundled_denied_malformed_or_trailing_json_never_yields_partial_rows() {
    for raw in [
        &b""[..],
        &b"["[..],
        &br#"[["s","p","o"],]"#[..],
        &br#"[["s","p","o"]] null"#[..],
        &b"[][]"[..],
        &b"\xff"[..],
        &br#"[["s","p","\uD800"]]"#[..],
    ] {
        let error = denied_cells_from_json(raw).unwrap_err();
        assert!(error.downcast_ref::<BundleJson>().is_some(), "{error}");
    }
}
