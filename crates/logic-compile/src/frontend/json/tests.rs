// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn json_compiler_diagnostic_mixed_case_severity_and_complete_literal_bytes() {
    for (severity, name) in [
        (Severity::Error, "Error"),
        (Severity::Warning, "Warning"),
        (Severity::Info, "Info"),
    ] {
        let text = format!("\"{name}\"");
        assert_eq!(to_vec(&severity), text.as_bytes());
        assert_eq!(from_slice::<Severity>(text.as_bytes()).unwrap(), severity);
        let object = format!("{{\"{name}\":null}}");
        assert_eq!(from_slice::<Severity>(object.as_bytes()).unwrap(), severity);
    }
    let observed = Diagnostic {
        severity: Severity::Warning,
        code: "AUTHORED".into(),
        message: "中\n\"\\".into(),
        subject: Some("_:source".into()),
    };
    let expected =
        r#"{"severity":"Warning","code":"AUTHORED","message":"中\n\"\\","subject":"_:source"}"#;
    assert_eq!(to_vec(&observed), expected.as_bytes());
    assert_eq!(serde_json::to_vec(&observed).unwrap(), expected.as_bytes());
    assert_eq!(
        from_slice::<Diagnostic>(expected.as_bytes()).unwrap(),
        observed
    );
    assert_eq!(Severity::Warning.as_str(), "WARNING");
}
#[test]
fn json_compiler_diagnostic_optional_subject_and_record_variant_refusals() {
    let expected = r#"{"severity":"Info","code":"C","message":"m","subject":null}"#;
    let missing = expected.replace(",\"subject\":null", "");
    assert_eq!(
        to_vec(&from_slice::<Diagnostic>(missing.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    let unknown = expected.replacen('{', "{\"unknown\":false,", 1);
    assert_eq!(
        to_vec(&from_slice::<Diagnostic>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    for bad in [
        "{}".to_owned(),
        "[]".into(),
        expected.replace("\"code\":\"C\",", ""),
        expected.replace("\"message\":\"m\"", "\"message\":null"),
        expected.replace("\"subject\":null", "\"subject\":null,\"subject\":null"),
        format!("{expected} null"),
    ] {
        assert!(from_slice::<Diagnostic>(bad.as_bytes()).is_err(), "{bad}");
    }
    for bad in [
        r#""WARNING""#,
        r#""warning""#,
        r#"{"Warning":false}"#,
        r#"{"Info":null,"Info":null}"#,
        r#"{"Info":null,"Error":null}"#,
    ] {
        assert!(from_slice::<Severity>(bad.as_bytes()).is_err(), "{bad}");
    }
    let wrong = expected.replace("\"severity\":\"Info\"", "\"severity\":null");
    assert_eq!(
        from_slice::<Diagnostic>(wrong.as_bytes())
            .unwrap_err()
            .pointer(),
        "/severity"
    );
}
