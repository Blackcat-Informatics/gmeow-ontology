// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use serde_json::json;

#[test]
fn source_admission_requires_its_exact_contract_and_refuses_evaluation_options() {
    let valid = json!({"verdict_mode":"class-source-admission", "source_admission_contract":
            gmeow_logic::reason::refute::CLASS_EXPRESSION_SOURCE_ADMISSION_ID});
    assert_eq!(
        parse_profile("source", &valid).unwrap().verdict_mode,
        VerdictMode::ClassSourceAdmission
    );
    for value in [Value::Null, json!("another-contract"), json!(3)] {
        let mut bad = valid.clone();
        bad["source_admission_contract"] = value;
        assert!(parse_profile("source", &bad).is_err());
    }
    for (key, value) in [
        ("foundation_lowering", json!(true)),
        ("teleology_lowering", json!(true)),
        ("certify", json!(true)),
        ("expect_unsupported", json!(true)),
        ("budget_params", json!({"max_steps":1})),
        ("counterfactual_profile", json!("PositiveHornProfile")),
        ("shipped_rules", json!(["urn:rule"])),
        ("compositions", json!([["urn:left", "urn:right"]])),
        (
            "reasoning_contract",
            json!({"preset":"PositiveHornProfile"}),
        ),
        ("anti_rigidity_policy", json!("witness-obligation")),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        assert!(
            parse_profile("source", &bad).is_err(),
            "source-only profile ignored {key}"
        );
    }
    let mut bad = valid;
    bad["verdict_mode"] = json!("consistency");
    assert!(parse_profile("source", &bad).is_err());
}

#[test]
fn defaults_when_minimal() {
    let p = parse_profile("c", &json!({})).expect("ok");
    assert_eq!(p.semantic_profile, DEFAULT_SEMANTIC_PROFILE);
    assert!(p.budget_params.is_none());
    assert!(!p.foundation_lowering);
    assert!(!p.teleology_lowering);
    assert_eq!(p.anti_rigidity_policy, DEFAULT_ANTI_RIGIDITY_POLICY);
    assert!(p.counterfactual_profile.is_none());
    assert!(!p.certify);
    assert!(!p.expect_unsupported);
    assert_eq!(p.verdict_mode, VerdictMode::Materialization);
    assert_eq!(p.query_profile(), DEFAULT_SEMANTIC_PROFILE);
    assert!(p.szs_status.is_none());
    assert!(p.documented_antipattern.is_none());
}

#[test]
fn szs_status_provenance_parses_and_validates() {
    // A raw SZS token is preserved verbatim (the fine-grained value, not the
    // 3-bucket projection).
    let p = parse_profile("c", &json!({ "szs_status": "ContradictoryAxioms" })).unwrap();
    assert_eq!(p.szs_status.as_deref(), Some("ContradictoryAxioms"));
    // Absent ⇒ None.
    assert!(parse_profile("c", &json!({})).unwrap().szs_status.is_none());
    // A non-string or empty token is a hard error (no silent coercion).
    assert!(parse_profile("c", &json!({ "szs_status": 7 })).is_err());
    assert!(parse_profile("c", &json!({ "szs_status": "" })).is_err());
    assert!(parse_profile("c", &json!({ "szs_status": "  " })).is_err());
}

#[test]
fn documented_antipattern_provenance_parses_and_validates() {
    // The documented anti-pattern label is preserved verbatim (the specific
    // community-decided verdict, not a pass/gap projection).
    let p = parse_profile(
        "c",
        &json!({ "documented_antipattern": "RelatorMediatesOne" }),
    )
    .unwrap();
    assert_eq!(
        p.documented_antipattern.as_deref(),
        Some("RelatorMediatesOne")
    );
    // Absent ⇒ None.
    assert!(
        parse_profile("c", &json!({}))
            .unwrap()
            .documented_antipattern
            .is_none()
    );
    // A non-string or empty label is a hard error (no silent coercion).
    assert!(parse_profile("c", &json!({ "documented_antipattern": 7 })).is_err());
    assert!(parse_profile("c", &json!({ "documented_antipattern": "" })).is_err());
    assert!(parse_profile("c", &json!({ "documented_antipattern": "  " })).is_err());
}

#[test]
fn verdict_mode_parses_and_defaults() {
    // Absent ⇒ materialization (the default behavior).
    assert_eq!(
        parse_profile("c", &json!({})).unwrap().verdict_mode,
        VerdictMode::Materialization
    );
    assert_eq!(
        parse_profile("c", &json!({ "verdict_mode": "materialization" }))
            .unwrap()
            .verdict_mode,
        VerdictMode::Materialization
    );
    assert_eq!(
        parse_profile("c", &json!({ "verdict_mode": "consistency" }))
            .unwrap()
            .verdict_mode,
        VerdictMode::Consistency
    );
    assert_eq!(
        parse_profile("c", &json!({ "verdict_mode": "cl-roundtrip" }))
            .unwrap()
            .verdict_mode,
        VerdictMode::CommonLogic
    );
}

#[test]
fn verdict_mode_unknown_value_hard_fails() {
    // No silent coercion to the default (no-optionality doctrine).
    let err = parse_profile("c", &json!({ "verdict_mode": "satisfiable" })).unwrap_err();
    assert!(err.message().contains("verdict_mode must be"), "{err}");
    // A non-string is equally a hard error.
    assert!(parse_profile("c", &json!({ "verdict_mode": 1 })).is_err());
}

#[test]
fn expect_unsupported_round_trips() {
    // An `expect_unsupported: true` case opts into the unsupported
    // short-circuit; a strict bool `true` is required (parallel to
    // foundation_lowering).
    assert!(
        parse_profile("c", &json!({ "expect_unsupported": true }))
            .unwrap()
            .expect_unsupported
    );
    // Absent ⇒ false.
    assert!(!parse_profile("c", &json!({})).unwrap().expect_unsupported);
    // A non-true value (string, 1) does NOT opt in.
    assert!(
        !parse_profile("c", &json!({ "expect_unsupported": "true" }))
            .unwrap()
            .expect_unsupported
    );
    assert!(
        !parse_profile("c", &json!({ "expect_unsupported": 1 }))
            .unwrap()
            .expect_unsupported
    );
}

#[test]
fn each_valid_preset_parses() {
    for name in VALID_SEMANTIC_PROFILES {
        let p =
            parse_profile("c", &json!({ "reasoning_contract": { "preset": name } })).expect("ok");
        assert_eq!(p.semantic_profile, name);
    }
}

#[test]
fn unknown_preset_hard_fails() {
    let err = parse_profile(
        "c",
        &json!({ "reasoning_contract": { "preset": "NopeProfile" } }),
    )
    .unwrap_err();
    assert!(
        err.message().contains("unknown reasoning_contract.preset"),
        "{err}"
    );
}

#[test]
fn legacy_top_level_semantic_profile_is_a_hard_error() {
    // Greenfield (no shim): the retired top-level key is rejected outright,
    // even when its value would otherwise be a valid preset.
    let err =
        parse_profile("c", &json!({ "semantic_profile": "PositiveHornProfile" })).unwrap_err();
    assert!(
        err.message()
            .contains("retired top-level semantic_profile key"),
        "{err}"
    );
    assert!(err.message().contains("reasoning_contract.preset"), "{err}");
}

#[test]
fn reasoning_contract_missing_preset_is_a_hard_error() {
    let err = parse_profile("c", &json!({ "reasoning_contract": {} })).unwrap_err();
    assert!(
        err.message().contains("missing the required preset key"),
        "{err}"
    );
}

#[test]
fn reasoning_contract_non_string_preset_is_a_hard_error() {
    let err = parse_profile("c", &json!({ "reasoning_contract": { "preset": 7 } })).unwrap_err();
    assert!(err.message().contains("preset must be a string"), "{err}");
}

#[test]
fn reasoning_contract_non_object_is_a_hard_error() {
    let err =
        parse_profile("c", &json!({ "reasoning_contract": "PositiveHornProfile" })).unwrap_err();
    assert!(err.message().contains("must be a JSON object"), "{err}");
}

#[test]
fn reasoning_contract_unknown_key_is_a_hard_error() {
    let err = parse_profile(
        "c",
        &json!({ "reasoning_contract": { "preset": "PositiveHornProfile", "nope": 1 } }),
    )
    .unwrap_err();
    assert!(err.message().contains("unknown key"), "{err}");
}

#[test]
fn non_object_profile_hard_fails() {
    assert!(parse_profile("c", &json!([1, 2, 3])).is_err());
    assert!(parse_profile("c", &json!("nope")).is_err());
}

#[test]
fn budget_params_parsed_and_present() {
    let p = parse_profile(
        "c",
        &json!({ "budget_params": { "time_ms": 50, "max_answers": 7 } }),
    )
    .expect("ok");
    let b = p.budget_params.expect("some");
    assert_eq!(b.time_ms, Some(50));
    assert_eq!(b.max_answers, Some(7));
    assert_eq!(b.max_rule_firings, None);
    assert_eq!(b.max_steps, None);
}

#[test]
fn budget_params_parses_max_steps() {
    // The backward step/derivation budget for a native goal query.
    let p = parse_profile("c", &json!({ "budget_params": { "max_steps": 2 } })).expect("ok");
    let b = p.budget_params.expect("some");
    assert_eq!(b.max_steps, Some(2));
    assert_eq!(b.max_answers, None);
}

#[test]
fn empty_budget_params_object_is_some_but_unbounded() {
    // Presence (even empty) signals "declares budget" for the diff phase.
    let p = parse_profile("c", &json!({ "budget_params": {} })).expect("ok");
    assert_eq!(p.budget_params, Some(BudgetParams::default()));
}

#[test]
fn budget_params_unknown_key_hard_fails() {
    let err = parse_profile("c", &json!({ "budget_params": { "nope": 1 } })).unwrap_err();
    assert!(err.message().contains("unknown key"));
}

#[test]
fn budget_params_non_object_hard_fails() {
    assert!(parse_profile("c", &json!({ "budget_params": 5 })).is_err());
}

#[test]
fn budget_params_rejects_bool_zero_negative_and_float() {
    for bad in [json!(true), json!(0), json!(-3), json!(1.5)] {
        let v = json!({ "budget_params": { "time_ms": bad } });
        assert!(
            parse_profile("c", &v).is_err(),
            "expected hard-fail for time_ms = {v}"
        );
    }
}

#[test]
fn foundation_lowering_requires_strict_true() {
    assert!(
        parse_profile("c", &json!({ "foundation_lowering": true }))
            .unwrap()
            .foundation_lowering
    );
    // A non-true value (string, 1) does NOT opt in.
    assert!(
        !parse_profile("c", &json!({ "foundation_lowering": "true" }))
            .unwrap()
            .foundation_lowering
    );
}

#[test]
fn teleology_lowering_requires_strict_true() {
    assert!(
        parse_profile("c", &json!({ "teleology_lowering": true }))
            .unwrap()
            .teleology_lowering
    );
    // A non-true value (string, 1) does NOT opt in (parallel to foundation_lowering).
    assert!(
        !parse_profile("c", &json!({ "teleology_lowering": "true" }))
            .unwrap()
            .teleology_lowering
    );
    assert!(
        !parse_profile("c", &json!({ "teleology_lowering": 1 }))
            .unwrap()
            .teleology_lowering
    );
    // Absent ⇒ false.
    assert!(!parse_profile("c", &json!({})).unwrap().teleology_lowering);
}

#[test]
fn counterfactual_profile_overrides_query_profile() {
    let p = parse_profile(
        "c",
        &json!({ "reasoning_contract": { "preset": "PositiveHornProfile" },
                     "counterfactual_profile": "LewisCredulousProfile" }),
    )
    .expect("ok");
    assert_eq!(p.query_profile(), "LewisCredulousProfile");
}
