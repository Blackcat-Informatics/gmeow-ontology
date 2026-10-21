// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn inventory_payload_keeps_literal_legacy_bytes_and_refuses_duplicates() {
    let xml = r#"<testsuite><testcase classname="c" name="a" time="0.001"/></testsuite>"#;
    let cases = parse_junit(xml, Path::new("a.xml")).unwrap();
    let digest = digest_inventory(&cases);
    let payload = inventory_payload(cases.clone(), Vec::new()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("inventory.json");
    write_json_atomic(&output, &payload).unwrap();
    let expected = concat!(
        "{\n  \"command\": \"junit-inventory\",\n  \"deterministic_work\": {\n",
        "    \"identities\": [\n      {\n        \"classname\": \"c\",\n        \"name\": \"a\"\n      }\n    ],\n",
        "    \"inventory_sha256\": \"DIGEST\",\n    \"test_count\": 1\n  },\n",
        "  \"observations\": {\n    \"duration_micros\": 1000,\n    \"failed\": 0,\n    \"inputs\": [],\n    \"passed\": 1,\n    \"skipped\": 0,\n",
        "    \"testcases\": [\n      {\n        \"classname\": \"c\",\n        \"duration_micros\": 1000,\n        \"name\": \"a\",\n        \"status\": \"passed\"\n      }\n    ]\n  },\n",
        "  \"schema_version\": 1\n}\n",
    ).replace("DIGEST", &digest);
    assert_eq!(fs::read_to_string(&output).unwrap(), expected);
    assert!(
        inventory_payload(vec![cases[0].clone(), cases[0].clone()], Vec::new())
            .unwrap_err()
            .to_string()
            .contains("duplicate JUnit testcase identity")
    );
}

#[test]
fn inventory_is_order_independent_and_duration_is_observational() {
    let a = parse_junit(
        r#"<testsuite><testcase classname="c" name="b" time="2.5"/><testcase classname="c" name="a" time="1"><skipped/></testcase></testsuite>"#,
        Path::new("a.xml"),
    )
    .unwrap();
    let b = parse_junit(
        r#"<testsuite><testcase classname="c" name="a" time="99"><skipped/></testcase><testcase classname="c" name="b" time="0"/></testsuite>"#,
        Path::new("b.xml"),
    )
    .unwrap();
    let mut a = a;
    let mut b = b;
    a.sort();
    b.sort();
    assert_eq!(digest_inventory(&a), digest_inventory(&b));
    assert_ne!(
        a.iter().map(|case| case.duration_micros).sum::<u64>(),
        b.iter().map(|case| case.duration_micros).sum::<u64>()
    );
}

#[test]
fn malformed_and_negative_durations_fail_closed() {
    assert!(parse_duration_micros("-1").is_err());
    assert!(parse_duration_micros("NaN").is_err());
    assert!(parse_duration_micros("forever").is_err());
}
