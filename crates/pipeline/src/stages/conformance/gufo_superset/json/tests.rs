// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn json_conformance_gufo_verbatim_terms_exact_pairs_and_sorted_containers() {
    let expected = r#"{"source_path":"source.ttl","default_quad_count":4,"subjects":["a","z"],"pairs":{"p":[["z",{"Iri":"urn:中"}],["a",{"Literal":"\"not reparsed\"@xx"}],["a",{"BlankNode":"b0"}],["z",{"Triple":"<< <s> <p> <o> >>"}]]}}"#;
    let observed = SourceObservation {
        source_path: "source.ttl".into(),
        default_quad_count: 4,
        subjects: ["z".into(), "a".into()].into(),
        pairs: [(
            "p".into(),
            vec![
                ("z".into(), ObservedTerm::Iri("urn:中".into())),
                (
                    "a".into(),
                    ObservedTerm::Literal("\"not reparsed\"@xx".into()),
                ),
                ("a".into(), ObservedTerm::BlankNode("b0".into())),
                ("z".into(), ObservedTerm::Triple("<< <s> <p> <o> >>".into())),
            ],
        )]
        .into(),
    };
    assert_eq!(to_vec(&observed), expected.as_bytes());
    let restored: SourceObservation = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(restored.pairs, observed.pairs);
    assert_eq!(to_vec(&restored), expected.as_bytes());
    let unknown = expected.replacen('{', "{\"extra\":false,", 1);
    assert_eq!(
        to_vec(&from_slice::<SourceObservation>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_conformance_gufo_refuses_bad_counts_pairs_tags_and_map_values() {
    let base = r#"{"source_path":"s","default_quad_count":0,"subjects":[],"pairs":{}}"#;
    for token in ["-0", "-1", "1e0", "1.0", "18446744073709551616", "null"] {
        let bad = base.replace(
            "\"default_quad_count\":0",
            &format!("\"default_quad_count\":{token}"),
        );
        assert_eq!(
            from_slice::<SourceObservation>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/default_quad_count"
        );
    }
    let max = base.replace(
        "\"default_quad_count\":0",
        &format!("\"default_quad_count\":{}", usize::MAX),
    );
    assert_eq!(
        from_slice::<SourceObservation>(max.as_bytes())
            .unwrap()
            .default_quad_count,
        usize::MAX
    );
    for pair in [
        "[]",
        r#"["s"]"#,
        r#"["s",{"Iri":"x"},null]"#,
        r#"[null,{"Iri":"x"}]"#,
        r#"["s",{"Iri":null}]"#,
        r#"["s",{"Iri":"x","Iri":"y"}]"#,
        r#"["s",{"Other":"x"}]"#,
    ] {
        let bad = base.replace("\"pairs\":{}", &format!("\"pairs\":{{\"p\":[{pair}]}}"));
        assert!(
            from_slice::<SourceObservation>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad = base.replace(
        "\"pairs\":{}",
        r#""pairs":{"p":[["s",{"Iri":null}]],"p":[]}"#,
    );
    assert_eq!(
        from_slice::<SourceObservation>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/pairs/p/0/1/Iri"
    );
    for bad in [
        "{}".into(),
        "[]".into(),
        base.replace("\"subjects\":[],", ""),
        base.replace("\"subjects\":[]", "\"subjects\":null"),
        base.replace(
            "\"source_path\":",
            "\"source_path\":\"first\",\"source_path\":",
        ),
        format!("{base} true"),
    ] {
        assert!(
            from_slice::<SourceObservation>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
}
