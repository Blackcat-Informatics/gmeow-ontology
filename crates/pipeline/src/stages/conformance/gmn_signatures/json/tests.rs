// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const CASE: &str = r#"{"query_path":"中\n\"\\.rq","query_digest":"digest","injection_digest":null,"rows":{"Ok":0}}"#;
#[test]
fn json_conformance_signatures_literal_zero_and_optional_policy() {
    let case = CaseObservation {
        query_path: "中\n\"\\.rq".into(),
        query_digest: "digest".into(),
        injection_digest: None,
        rows: Ok(0),
    };
    assert_eq!(to_vec(&case), CASE.as_bytes());
    let expected =
        format!("{{\"modules\":{{\"a\":\"A\",\"z\":\"Z\"}},\"cases\":{{\"zero\":{CASE}}}}}");
    let observed = Observations {
        modules: [("z".into(), "Z".into()), ("a".into(), "A".into())].into(),
        cases: [("zero".into(), case)].into(),
    };
    assert_eq!(to_vec(&observed), expected.as_bytes());
    let restored: Observations = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(restored.cases["zero"].rows.as_ref().unwrap(), &0);
    let absent = CASE.replace("\"injection_digest\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<CaseObservation>(absent.as_bytes()).unwrap()),
        CASE.as_bytes()
    );
    let unknown = CASE.replacen('{', "{\"unknown\":false,", 1);
    assert_eq!(
        to_vec(&from_slice::<CaseObservation>(unknown.as_bytes()).unwrap()),
        CASE.as_bytes()
    );
}
#[test]
fn json_conformance_signatures_counts_records_and_duplicate_map_values() {
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "18446744073709551616",
        "null",
        "\"1\"",
    ] {
        let bad = CASE.replace("\"Ok\":0", &format!("\"Ok\":{token}"));
        assert_eq!(
            from_slice::<CaseObservation>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/rows/Ok"
        );
        assert!(from_slice::<RowCount>(token.as_bytes()).is_err());
    }
    let max = usize::MAX.to_string();
    assert_eq!(
        from_slice::<RowCount>(max.as_bytes()).unwrap().0,
        usize::MAX
    );
    for bad in [
        "{}".into(),
        "[]".into(),
        CASE.replace("\"query_digest\":\"digest\",", ""),
        CASE.replace("\"query_digest\":\"digest\"", "\"query_digest\":null"),
        CASE.replace(
            "\"query_path\":",
            "\"query_path\":\"first\",\"query_path\":",
        ),
        CASE.replace("{\"Ok\":0}", "{\"Ok\":0,\"Ok\":1}"),
        format!("{CASE} null"),
    ] {
        assert!(
            from_slice::<CaseObservation>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad = CASE.replace("\"Ok\":0", "\"Ok\":-0");
    let map = format!("{{\"modules\":{{}},\"cases\":{{\"x\":{bad},\"x\":{CASE}}}}}");
    assert_eq!(
        from_slice::<Observations>(map.as_bytes())
            .unwrap_err()
            .pointer(),
        "/cases/x/rows/Ok"
    );
    let map = format!("{{\"modules\":{{}},\"cases\":{{\"x\":{CASE},\"x\":{CASE}}}}}");
    assert_eq!(
        from_slice::<Observations>(map.as_bytes())
            .unwrap()
            .cases
            .len(),
        1
    );
}
