// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn json_auxiliary_scene_complete_literal_keeps_finding_order_and_duplicate_rows() {
    let finding =
        gmeow_errors::Finding::new(gmeow_errors::Severity::Warning, "authored", "中\n\"\\");
    let observation = AuxiliaryObservation {
        source_path: "x.ttl".into(),
        source_digest: "unchanged".into(),
        findings: vec![finding.clone(), finding],
    };
    let row = r#"{"severity":"warning","code":"authored","message":"中\n\"\\"}"#;
    let expected = format!(
        "{{\"Ok\":{{\"source_path\":\"x.ttl\",\"source_digest\":\"unchanged\",\"findings\":[{row},{row}]}}}}"
    );
    let observed = AuxiliaryOutcome(Ok(observation));
    assert_eq!(to_vec(&observed), expected.as_bytes());
    let restored: AuxiliaryOutcome = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(restored.0.as_ref().unwrap().findings.len(), 2);
    assert_eq!(to_vec(&restored), expected.as_bytes());
    let unknown = expected.replace("\"source_path\":", "\"extra\":null,\"source_path\":");
    assert_eq!(
        to_vec(&from_slice::<AuxiliaryOutcome>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_auxiliary_scene_required_fields_and_outcomes_refuse_malformed_records() {
    for bad in [
        "{}",
        "[]",
        "null",
        r#"{"Ok":null}"#,
        r#"{"Ok":[]}"#,
        r#"{"Ok":{}}"#,
        r#"{"Ok":{"source_path":"p","source_digest":"d","findings":null}}"#,
        r#"{"Ok":{"source_path":"p","source_digest":"d","findings":[],"findings":[]}}"#,
        r#"{"Ok":{"source_path":null,"source_digest":"d","findings":[]}}"#,
        r#"{"Ok":{"source_path":"p","source_digest":"d","findings":[]},"Err":null}"#,
        r#"{"Err":null}"#,
        r#"{"Unknown":null}"#,
    ] {
        assert!(
            from_slice::<AuxiliaryOutcome>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    let bad=br#"{"Ok":{"source_path":"p","source_digest":"d","findings":[{"severity":"other","code":"c","message":"m"}]}}"#;
    assert_eq!(
        from_slice::<AuxiliaryOutcome>(bad).unwrap_err().pointer(),
        "/Ok/findings/0/severity"
    );
    assert!(
        from_slice::<AuxiliaryOutcome>(
            br#"{"Ok":{"source_path":"p","source_digest":"d","findings":[]}} []"#
        )
        .is_err()
    );
}
