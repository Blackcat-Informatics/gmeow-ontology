// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact authored cache payloads, without source parsing or action execution.
use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};
#[test]
fn json_grounding_cache_exact_catalog_pair_and_declaration_triple_bytes() {
    for input in [r#"[[],null]"#, r#"[[],{"Ok":0}]"#] {
        let value: Catalog = from_slice(input.as_bytes()).unwrap();
        assert_eq!(to_vec(&value), input.as_bytes());
    }
    let input = br#"[["a","z"],["c"],{"s":["label"]}]"#;
    let value: Declaration = from_slice(input).unwrap();
    assert_eq!(to_vec(&value), input);
    let unordered = br#"[["z","a","z"],["c","c"],{"s":["label","label"]}]"#;
    assert_eq!(
        to_vec(&from_slice::<Declaration>(unordered).unwrap()),
        input
    );
}
#[test]
fn json_grounding_cache_exact_arity_and_strict_nested_coordinate_refusals() {
    for bad in [
        "null",
        "[]",
        "[[]]",
        "[[],null,null]",
        r#"[[],{"Ok":-0}]"#,
        r#"[[],{"Ok":1e0}]"#,
        r#"[[],{"Err":null}]"#,
    ] {
        assert!(from_slice::<Catalog>(bad.as_bytes()).is_err(), "{bad}");
    }
    for bad in ["{}", "[]", "[[],[]]", "[[],[],{},null]", "[[],[],null]"] {
        assert!(from_slice::<Declaration>(bad.as_bytes()).is_err(), "{bad}");
    }
    assert_eq!(
        from_slice::<Catalog>(br#"[[],{"Ok":-0}]"#)
            .unwrap_err()
            .pointer(),
        "/1/Ok"
    );
    assert_eq!(
        from_slice::<Declaration>(br#"[[],[],{"s":[4]}]"#)
            .unwrap_err()
            .pointer(),
        "/2/s/0"
    );
}
