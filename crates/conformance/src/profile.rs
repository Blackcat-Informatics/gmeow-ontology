// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `profile.json` parsing and validation.
//!
//! A case's `profile.json` declares the semantic profile to evaluate under, plus
//! optional governor / foundation / counterfactual / certification knobs. This
//! module parses it into a typed [`Profile`] with **strict, hard-fail**
//! validation (no-optionality doctrine): an unknown `semantic_profile`, a
//! malformed `budget_params`, or a non-object profile is an error, never a
//! silently-coerced default. Mirrors the validation in the retired Python
//! `logic_runner.run` / `_parse_budget_params`.

use serde_json::{Map, Value};

/// The semantic profiles the native engine recognises (mirrors
/// `logic_runner._VALID_SEMANTIC_PROFILES`). An unknown localname is a hard
/// failure — the case author must declare a real profile.
pub const VALID_SEMANTIC_PROFILES: [&str; 6] = [
    "PositiveHornProfile",
    "StratifiedNAFProfile",
    "WellFoundedProfile",
    "StableModelProfile",
    "ProceduralPrologProfile",
    "ProbabilisticProfile",
];

/// The default semantic profile when `profile.json` omits the key.
pub const DEFAULT_SEMANTIC_PROFILE: &str = "PositiveHornProfile";

/// The default anti-rigidity policy for the foundation-lowering path.
pub const DEFAULT_ANTI_RIGIDITY_POLICY: &str = "witness-obligation";

/// Optional budget governor ceilings (issue #502). Each is an optional positive
/// integer; absence ⇒ unbounded. Mirrors `logic_seam.BudgetParams`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BudgetParams {
    pub time_ms: Option<u64>,
    pub max_rule_firings: Option<u64>,
    pub max_answers: Option<u64>,
}

/// A parsed, validated `profile.json`.
#[derive(Debug, Clone)]
pub struct Profile {
    /// The declared semantic profile (validated ∈ [`VALID_SEMANTIC_PROFILES`]).
    pub semantic_profile: String,
    /// The optional budget governor (`None` ⇒ unbounded). `Some` iff the
    /// `budget_params` key is present — this doubles as the diff-phase
    /// "declares-budget ⇒ require-golden" signal.
    pub budget_params: Option<BudgetParams>,
    /// Whether the case opts into the foundation-lowering chase
    /// (`"foundation_lowering": true`).
    pub foundation_lowering: bool,
    /// The foundation anti-rigidity policy (default
    /// [`DEFAULT_ANTI_RIGIDITY_POLICY`]).
    pub anti_rigidity_policy: String,
    /// The optional query-resolution profile override (#505): when present it is
    /// used for `gmeow_logic.query` instead of `semantic_profile`.
    pub counterfactual_profile: Option<String>,
    /// Whether certification is required/compared (`"certify": true`).
    pub certify: bool,
}

impl Profile {
    /// The profile to resolve backward goals under: the `counterfactual_profile`
    /// override when declared, else the materialization `semantic_profile`.
    pub fn query_profile(&self) -> &str {
        self.counterfactual_profile
            .as_deref()
            .unwrap_or(&self.semantic_profile)
    }
}

/// Parse and validate a `profile.json` value for case `case_id`.
///
/// Returns the typed [`Profile`], or a human-readable error string on the first
/// malformed/unknown field (hard-fail).
pub fn parse_profile(case_id: &str, value: &Value) -> Result<Profile, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| format!("case {case_id}: profile.json must be a JSON object"))?;

    let semantic_profile = match obj.get("semantic_profile") {
        None => DEFAULT_SEMANTIC_PROFILE.to_string(),
        Some(v) => v
            .as_str()
            .ok_or_else(|| {
                format!("case {case_id}: profile.json semantic_profile must be a string")
            })?
            .to_string(),
    };
    if !VALID_SEMANTIC_PROFILES.contains(&semantic_profile.as_str()) {
        return Err(format!(
            "case {case_id}: unknown semantic_profile {semantic_profile:?} in profile.json — \
             must be one of {VALID_SEMANTIC_PROFILES:?}"
        ));
    }

    let budget_params = parse_budget_params(case_id, obj)?;

    // `foundation_lowering` opts in only on a strict boolean `true` (mirrors the
    // Python `... is True` identity check — never auto-gated on stereotype presence).
    let foundation_lowering = obj.get("foundation_lowering").and_then(Value::as_bool) == Some(true);

    let anti_rigidity_policy = obj
        .get("anti_rigidity_policy")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_ANTI_RIGIDITY_POLICY)
        .to_string();

    let counterfactual_profile = obj
        .get("counterfactual_profile")
        .and_then(Value::as_str)
        .map(String::from);

    let certify = obj.get("certify").and_then(Value::as_bool).unwrap_or(false);

    Ok(Profile {
        semantic_profile,
        budget_params,
        foundation_lowering,
        anti_rigidity_policy,
        counterfactual_profile,
        certify,
    })
}

/// Parse the optional `budget_params` object (mirrors
/// `logic_runner._parse_budget_params`).
///
/// Hard-fail (no silent coercion): a non-object `budget_params`, an unknown key,
/// or a non-positive / non-integer / boolean ceiling is an error. Returns `None`
/// when the key is absent (unbounded chase).
fn parse_budget_params(
    case_id: &str,
    obj: &Map<String, Value>,
) -> Result<Option<BudgetParams>, String> {
    let raw = match obj.get("budget_params") {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let raw = raw.as_object().ok_or_else(|| {
        format!("case {case_id}: profile.json budget_params must be a JSON object")
    })?;

    const ALLOWED: [&str; 3] = ["time_ms", "max_rule_firings", "max_answers"];
    let mut unknown: Vec<&str> = raw
        .keys()
        .map(String::as_str)
        .filter(|k| !ALLOWED.contains(k))
        .collect();
    unknown.sort_unstable();
    if !unknown.is_empty() {
        return Err(format!(
            "case {case_id}: profile.json budget_params has unknown key(s) {unknown:?}; \
             allowed keys are {ALLOWED:?}"
        ));
    }

    let ceiling = |key: &str| -> Result<Option<u64>, String> {
        match raw.get(key) {
            None => Ok(None),
            Some(value) => {
                // `bool` must be rejected explicitly: `serde_json` keeps it distinct
                // from numbers, but guard anyway so `true`/`false` cannot pass as a
                // 1/0 ceiling. `as_u64` rejects negatives, floats and non-numbers.
                if value.is_boolean() {
                    return Err(format!(
                        "case {case_id}: profile.json budget_params.{key} must be a \
                         positive integer, got {value}"
                    ));
                }
                let n = value.as_u64().ok_or_else(|| {
                    format!(
                        "case {case_id}: profile.json budget_params.{key} must be a \
                         positive integer, got {value}"
                    )
                })?;
                if n == 0 {
                    return Err(format!(
                        "case {case_id}: profile.json budget_params.{key} must be a \
                         positive integer, got {n}"
                    ));
                }
                Ok(Some(n))
            }
        }
    };

    Ok(Some(BudgetParams {
        time_ms: ceiling("time_ms")?,
        max_rule_firings: ceiling("max_rule_firings")?,
        max_answers: ceiling("max_answers")?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_when_minimal() {
        let p = parse_profile("c", &json!({})).expect("ok");
        assert_eq!(p.semantic_profile, DEFAULT_SEMANTIC_PROFILE);
        assert!(p.budget_params.is_none());
        assert!(!p.foundation_lowering);
        assert_eq!(p.anti_rigidity_policy, DEFAULT_ANTI_RIGIDITY_POLICY);
        assert!(p.counterfactual_profile.is_none());
        assert!(!p.certify);
        assert_eq!(p.query_profile(), DEFAULT_SEMANTIC_PROFILE);
    }

    #[test]
    fn each_valid_semantic_profile_parses() {
        for name in VALID_SEMANTIC_PROFILES {
            let p = parse_profile("c", &json!({ "semantic_profile": name })).expect("ok");
            assert_eq!(p.semantic_profile, name);
        }
    }

    #[test]
    fn unknown_semantic_profile_hard_fails() {
        let err = parse_profile("c", &json!({ "semantic_profile": "NopeProfile" })).unwrap_err();
        assert!(err.contains("unknown semantic_profile"));
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
        assert!(err.contains("unknown key"));
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
    fn counterfactual_profile_overrides_query_profile() {
        let p = parse_profile(
            "c",
            &json!({ "semantic_profile": "PositiveHornProfile",
                     "counterfactual_profile": "LewisCredulousProfile" }),
        )
        .expect("ok");
        assert_eq!(p.query_profile(), "LewisCredulousProfile");
    }
}
