// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn hydration_retains_native_source_and_refuses_stale_or_weakened_admission() {
    // The input is an authenticated producer record. No source compiler or
    // materializer runs in this consumer, even for the deliberate mutations.
    let original = serde_json::to_value(fixture()).expect("native preparation value");
    let bytes = serde_json::to_vec(&original).expect("native preparation bytes");
    let hydrated = PreparedOperatorRules::from_bytes(&bytes).expect("selected native preparation");
    assert_eq!(serde_json::to_value(&hydrated).unwrap(), original);
    assert!(!hydrated.means_end_preparation().1.rules.is_empty());
    for field in [
        "joint",
        "reasoning_joint",
        "reasoning_rules",
        "schema_plans",
    ] {
        assert!(
            original["lowering"].get(field).is_none(),
            "runtime cache {field} must not be published"
        );
    }

    let mut stale = original.clone();
    stale["source_digest"] = "wrong-source".into();
    let mut old_schema = original.clone();
    old_schema["schema_version"] = 1.into();
    let mut absent_admission = original.clone();
    absent_admission["lowering"]
        .as_object_mut()
        .unwrap()
        .remove("admission");
    let mut profile = original.clone();
    profile["profile"] = serde_json::to_value(SemanticProfileId::WellFounded).unwrap();
    let mut scoped = original.clone();
    scoped["program"]["rules"][0]["scope"]["standpoint"] =
        "https://example.test/other-context".into();
    let mut missing = original.clone();
    missing["means_end_lowering"]["rules"] = serde_json::json!([]);
    let mut diagnostics = original;
    diagnostics["diagnostics"] = serde_json::to_value(vec![Diagnostic {
        severity: Severity::Error,
        code: "SYNTHETIC_SOURCE_ERROR".to_owned(),
        message: "selected source failed compilation".to_owned(),
        subject: None,
    }])
    .unwrap();
    for invalid in [
        stale,
        old_schema,
        absent_admission,
        profile,
        scoped,
        missing,
        diagnostics,
    ] {
        assert!(PreparedOperatorRules::from_bytes(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}
