// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `profile.json` parsing and validation.
//!
//! A case's `profile.json` declares the reasoning preset to evaluate under
//! (nested under `reasoning_contract.preset`), plus optional governor /
//! foundation / counterfactual / certification knobs. This module parses it into a
//! typed [`Profile`] with **strict, hard-fail** validation (no-optionality
//! doctrine): an unknown preset, a malformed `budget_params`, a non-object profile,
//! or a surviving retired top-level `semantic_profile` key is an error, never a
//! silently-coerced default.

use gmeow_errors::Diag;
use serde_json::{Map, Value};

use crate::error::ProfileInvalid;

/// The semantic profiles the native engine recognises. An unknown localname is
/// a hard failure — the case author must declare a real profile.
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

/// The verdict-production mode.
///
/// `Materialization` (the default) runs the profile-routed chase and counts the
/// materialized worlds — the original behavior. `Consistency` reasons over the
/// case's RDF EDB (world-scoped N-Quads in `input.nq`) through the native DL
/// consistency path ([`gmeow_logic::reason::reason_all`]) and emits a per-world
/// `consistent`/`inconsistent` verdict. This is **modal-by-test-intent** —
/// materialization and consistency are genuinely different engine operations,
/// exactly like the existing `foundation_lowering` / `expect_unsupported` modal
/// fields — NOT a quality knob; an unknown value is a hard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerdictMode {
    #[default]
    Materialization,
    Consistency,
    /// The Common Logic round-trip mode: gates the CLIF/CGIF/XCL Exact projections
    /// (IR round-trip isomorphism + cross-dialect equivalence) and pins their canonical
    /// rendering. Like `Consistency` it is modal-by-test-intent — a genuinely different
    /// engine operation (project/parse/isomorphism, no materialize chase), not a quality
    /// knob.
    CommonLogic,
}

/// Optional budget governor ceilings. Each is an optional positive
/// integer; absence ⇒ unbounded. This struct is the sole authority for the
/// budget ceilings (the former Python `logic_seam.BudgetParams` has since been removed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BudgetParams {
    pub time_ms: Option<u64>,
    pub max_rule_firings: Option<u64>,
    pub max_answers: Option<u64>,
    /// The step/derivation budget for a backward goal query — the native governor's
    /// `max_steps` (honoured by the magic-sets engine). Absence ⇒ unbounded.
    pub max_steps: Option<u64>,
}

/// A parsed, validated `profile.json`.
#[derive(Debug, Clone)]
pub struct Profile {
    /// The declared reasoning preset, read from `reasoning_contract.preset`
    /// (validated ∈ [`VALID_SEMANTIC_PROFILES`]). The field keeps its historical
    /// name because the runner uses it as the profile/preset name downstream.
    pub semantic_profile: String,
    /// The optional budget governor (`None` ⇒ unbounded). `Some` iff the
    /// `budget_params` key is present — this doubles as the diff-phase
    /// "declares-budget ⇒ require-golden" signal.
    pub budget_params: Option<BudgetParams>,
    /// Whether the case opts into the foundation-lowering chase
    /// (`"foundation_lowering": true`).
    pub foundation_lowering: bool,
    /// Whether the case opts into the teleology-lowering materialization
    /// (`"teleology_lowering": true`). Like `foundation_lowering`, the
    /// teleology evaluator has no budget governor and needs no rule-program input; a declared
    /// `budget_params` is a hard failure (enforced in the runner).
    pub teleology_lowering: bool,
    /// The foundation anti-rigidity policy (default
    /// [`DEFAULT_ANTI_RIGIDITY_POLICY`]).
    pub anti_rigidity_policy: String,
    /// The optional query-resolution profile override: when present it is
    /// used for `gmeow_logic.query` instead of `semantic_profile`.
    pub counterfactual_profile: Option<String>,
    /// Whether certification is required/compared (`"certify": true`).
    pub certify: bool,
    /// Whether the case asserts the contract is an UNSUPPORTED facet combination
    /// (`"expect_unsupported": true`). When set, the runner requires
    /// the compile to emit an `UNSUPPORTED_CONTRACT` `Severity::Error` diagnostic
    /// and short-circuits BEFORE evaluating/certifying/materializing — the
    /// "unsupported is a hard stop" guarantee, pinned at the corpus level.
    pub expect_unsupported: bool,
    /// The verdict-production mode (default [`VerdictMode::Materialization`]).
    pub verdict_mode: VerdictMode,
    /// Declared correspondence compositions to gate (`(left, right, optional composite)`):
    /// each is a 2- or 3-element array of correspondence IRIs in `profile.json`'s
    /// `"compositions"`. Empty when the case declares none. Fed to the Composition gate.
    pub compositions: Vec<(String, String, Option<String>)>,
    /// The raw external-source status token this case was declared with, preserved
    /// verbatim as provenance so the fine-grained value (e.g. `ContradictoryAxioms`
    /// vs `Unsatisfiable`, `CounterSatisfiable` vs `Satisfiable`) is NOT collapsed to
    /// the 3-bucket runner verdict at ingest — the lossy projection is applied only at
    /// the gate. `None` for cases with no external status (endogenous / W3C-manifest);
    /// the soundness gate requires it for a TPTP `source/problem.p` case and pins it
    /// against the source's `% SZS status` line.
    pub szs_status: Option<String>,
    /// The documented OntoUML anti-pattern label this catalog case carries,
    /// preserved verbatim as provenance so the specific community-decided verdict
    /// is NOT collapsed to a pass/gap at ingest — the lossy projection is applied
    /// only at the comparison gate. `None` for endogenous/clean-control cases.
    pub documented_antipattern: Option<String>,
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
/// Build a profile-invalid diagnostic from a preserved message.
fn invalid(detail: String) -> Diag {
    Diag::of_kind(ProfileInvalid { detail })
}

pub fn parse_profile(case_id: &str, value: &Value) -> gmeow_errors::Result<Profile> {
    let obj = value.as_object().ok_or_else(|| {
        invalid(format!(
            "case {case_id}: profile.json must be a JSON object"
        ))
    })?;

    // Greenfield (no shim): the preset is carried under the nested
    // `reasoning_contract` object as `preset`. A surviving top-level
    // `semantic_profile` key is the retired surface and is a HARD failure — never
    // a silent fallback or dual-read.
    if obj.contains_key("semantic_profile") {
        return Err(invalid(format!(
            "case {case_id}: profile.json uses the retired top-level semantic_profile key; \
             migrate to reasoning_contract.preset"
        )));
    }

    let semantic_profile = match obj.get("reasoning_contract") {
        // Absent contract ⇒ default preset (the minimal-profile path).
        None => DEFAULT_SEMANTIC_PROFILE.to_string(),
        Some(rc) => parse_reasoning_contract(case_id, rc)?,
    };
    if !VALID_SEMANTIC_PROFILES.contains(&semantic_profile.as_str()) {
        return Err(invalid(format!(
            "case {case_id}: unknown reasoning_contract.preset {semantic_profile:?} in \
             profile.json — must be one of {VALID_SEMANTIC_PROFILES:?}"
        )));
    }

    let budget_params = parse_budget_params(case_id, obj)?;

    // `foundation_lowering` opts in only on a strict boolean `true` (mirrors the
    // Python `... is True` identity check — never auto-gated on stereotype presence).
    let foundation_lowering = obj.get("foundation_lowering").and_then(Value::as_bool) == Some(true);

    // `teleology_lowering` opts in only on a strict boolean `true` (mirrors
    // `foundation_lowering` — never auto-gated, never silently coerced from a truthy
    // non-bool).
    let teleology_lowering = obj.get("teleology_lowering").and_then(Value::as_bool) == Some(true);

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

    // `expect_unsupported` opts in only on a strict boolean `true` (parallel to
    // `foundation_lowering` — never auto-gated, never silently coerced from a
    // truthy non-bool).
    let expect_unsupported = obj.get("expect_unsupported").and_then(Value::as_bool) == Some(true);

    let verdict_mode = parse_verdict_mode(case_id, obj)?;
    let compositions = parse_compositions(case_id, obj)?;
    let szs_status = parse_szs_status_field(case_id, obj)?;
    let documented_antipattern = parse_documented_antipattern_field(case_id, obj)?;

    Ok(Profile {
        semantic_profile,
        budget_params,
        foundation_lowering,
        teleology_lowering,
        anti_rigidity_policy,
        counterfactual_profile,
        certify,
        expect_unsupported,
        verdict_mode,
        compositions,
        szs_status,
        documented_antipattern,
    })
}

/// Parse the optional `szs_status` provenance field.
///
/// Absent ⇒ `None`. When present it MUST be a non-empty string (the raw TPTP SZS
/// token, e.g. `"ContradictoryAxioms"`); a non-string or empty value is a hard
/// error (no silent coercion). The *conditional-required* rule — a TPTP
/// `source/problem.p` case MUST carry it — is enforced at the soundness gate,
/// where the source is available to cross-check.
fn parse_szs_status_field(
    case_id: &str,
    obj: &Map<String, Value>,
) -> gmeow_errors::Result<Option<String>> {
    match obj.get("szs_status") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(Some(s.clone())),
        Some(other) => Err(invalid(format!(
            "case {case_id}: profile.json szs_status must be a non-empty string (the raw \
             TPTP SZS token), got {other}"
        ))),
    }
}

/// Parse the optional `documented_antipattern` provenance field.
///
/// Absent ⇒ `None`. When present it MUST be a non-empty string (the documented
/// OntoUML anti-pattern label, e.g. `"RelatorMediatesOne"`); a non-string or
/// empty value is a hard error (no silent coercion). The lossy projection onto a
/// pass/gap verdict is applied only at the comparison gate, never here.
fn parse_documented_antipattern_field(
    case_id: &str,
    obj: &Map<String, Value>,
) -> gmeow_errors::Result<Option<String>> {
    match obj.get("documented_antipattern") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(Some(s.clone())),
        Some(other) => Err(invalid(format!(
            "case {case_id}: profile.json documented_antipattern must be a non-empty string \
             (the documented OntoUML anti-pattern label), got {other}"
        ))),
    }
}

/// Parse the optional `compositions` array: each element is a 2- or 3-string array
/// `[left, right]` or `[left, right, composite]` of correspondence IRIs. Hard-fail (no
/// silent coercion) on any non-array element, wrong length, or non-string member.
fn parse_compositions(
    case_id: &str,
    obj: &Map<String, Value>,
) -> gmeow_errors::Result<Vec<(String, String, Option<String>)>> {
    let raw = match obj.get("compositions") {
        None => return Ok(Vec::new()),
        Some(v) => v.as_array().ok_or_else(|| {
            invalid(format!(
                "case {case_id}: profile.json compositions must be a JSON array"
            ))
        })?,
    };
    let mut out = Vec::with_capacity(raw.len());
    for (i, entry) in raw.iter().enumerate() {
        let arr = entry.as_array().ok_or_else(|| {
            invalid(format!(
                "case {case_id}: profile.json compositions[{i}] must be an array"
            ))
        })?;
        if arr.len() != 2 && arr.len() != 3 {
            return Err(invalid(format!(
                "case {case_id}: profile.json compositions[{i}] must have 2 or 3 elements \
                 ([left, right] or [left, right, composite]), got {}",
                arr.len()
            )));
        }
        let s = |j: usize| -> gmeow_errors::Result<String> {
            arr[j].as_str().map(String::from).ok_or_else(|| {
                invalid(format!(
                    "case {case_id}: profile.json compositions[{i}][{j}] must be a string IRI"
                ))
            })
        };
        out.push((
            s(0)?,
            s(1)?,
            if arr.len() == 3 { Some(s(2)?) } else { None },
        ));
    }
    Ok(out)
}

/// Parse the optional `verdict_mode` field.
///
/// Absent ⇒ [`VerdictMode::Materialization`]. The only accepted values are the
/// strings `"materialization"`, `"consistency"`, and `"cl-roundtrip"`; any other
/// value (including a non-string) is a hard error — no silent coercion to the default.
fn parse_verdict_mode(
    case_id: &str,
    obj: &Map<String, Value>,
) -> gmeow_errors::Result<VerdictMode> {
    match obj.get("verdict_mode") {
        None | Some(Value::Null) => Ok(VerdictMode::Materialization),
        Some(Value::String(s)) if s == "materialization" => Ok(VerdictMode::Materialization),
        Some(Value::String(s)) if s == "consistency" => Ok(VerdictMode::Consistency),
        Some(Value::String(s)) if s == "cl-roundtrip" => Ok(VerdictMode::CommonLogic),
        Some(other) => Err(invalid(format!(
            "case {case_id}: profile.json verdict_mode must be \"materialization\", \
             \"consistency\", or \"cl-roundtrip\", got {other}"
        ))),
    }
}

/// Parse and validate the nested `reasoning_contract` object, returning its
/// `preset` local name (the value the runner uses as the profile name).
///
/// Hard-fail (no-optionality discipline): `reasoning_contract` MUST be a JSON object
/// carrying a string `preset` and no other keys. A non-object contract, a missing or
/// non-string `preset`, or any unknown key is an error (the `preset` value is itself
/// range-checked against [`VALID_SEMANTIC_PROFILES`] by the caller).
fn parse_reasoning_contract(case_id: &str, value: &Value) -> gmeow_errors::Result<String> {
    let obj = value.as_object().ok_or_else(|| {
        invalid(format!(
            "case {case_id}: profile.json reasoning_contract must be a JSON object"
        ))
    })?;

    // Only `preset` is allowed for now (keeps the surface closed; new facets are an
    // explicit extension, never a silently-tolerated key).
    const ALLOWED: [&str; 1] = ["preset"];
    let mut unknown: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|k| !ALLOWED.contains(k))
        .collect();
    unknown.sort_unstable();
    if !unknown.is_empty() {
        return Err(invalid(format!(
            "case {case_id}: profile.json reasoning_contract has unknown key(s) {unknown:?}; \
             allowed keys are {ALLOWED:?}"
        )));
    }

    let preset = obj.get("preset").ok_or_else(|| {
        invalid(format!(
            "case {case_id}: profile.json reasoning_contract is missing the required preset key"
        ))
    })?;
    let preset = preset.as_str().ok_or_else(|| {
        invalid(format!(
            "case {case_id}: profile.json reasoning_contract.preset must be a string"
        ))
    })?;
    Ok(preset.to_string())
}

/// Parse the optional `budget_params` object.
///
/// Hard-fail (no silent coercion): a non-object `budget_params`, an unknown key,
/// or a non-positive / non-integer / boolean ceiling is an error. Returns `None`
/// when the key is absent (unbounded chase).
fn parse_budget_params(
    case_id: &str,
    obj: &Map<String, Value>,
) -> gmeow_errors::Result<Option<BudgetParams>> {
    let raw = match obj.get("budget_params") {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let raw = raw.as_object().ok_or_else(|| {
        invalid(format!(
            "case {case_id}: profile.json budget_params must be a JSON object"
        ))
    })?;

    const ALLOWED: [&str; 4] = ["time_ms", "max_rule_firings", "max_answers", "max_steps"];
    let mut unknown: Vec<&str> = raw
        .keys()
        .map(String::as_str)
        .filter(|k| !ALLOWED.contains(k))
        .collect();
    unknown.sort_unstable();
    if !unknown.is_empty() {
        return Err(invalid(format!(
            "case {case_id}: profile.json budget_params has unknown key(s) {unknown:?}; \
             allowed keys are {ALLOWED:?}"
        )));
    }

    let ceiling = |key: &str| -> gmeow_errors::Result<Option<u64>> {
        match raw.get(key) {
            None => Ok(None),
            Some(value) => {
                // `bool` must be rejected explicitly: `serde_json` keeps it distinct
                // from numbers, but guard anyway so `true`/`false` cannot pass as a
                // 1/0 ceiling. `as_u64` rejects negatives, floats and non-numbers.
                if value.is_boolean() {
                    return Err(invalid(format!(
                        "case {case_id}: profile.json budget_params.{key} must be a \
                         positive integer, got {value}"
                    )));
                }
                let n = value.as_u64().ok_or_else(|| {
                    invalid(format!(
                        "case {case_id}: profile.json budget_params.{key} must be a \
                         positive integer, got {value}"
                    ))
                })?;
                if n == 0 {
                    return Err(invalid(format!(
                        "case {case_id}: profile.json budget_params.{key} must be a \
                         positive integer, got {n}"
                    )));
                }
                Ok(Some(n))
            }
        }
    };

    Ok(Some(BudgetParams {
        time_ms: ceiling("time_ms")?,
        max_rule_firings: ceiling("max_rule_firings")?,
        max_answers: ceiling("max_answers")?,
        max_steps: ceiling("max_steps")?,
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
            let p = parse_profile("c", &json!({ "reasoning_contract": { "preset": name } }))
                .expect("ok");
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
        let err =
            parse_profile("c", &json!({ "reasoning_contract": { "preset": 7 } })).unwrap_err();
        assert!(err.message().contains("preset must be a string"), "{err}");
    }

    #[test]
    fn reasoning_contract_non_object_is_a_hard_error() {
        let err = parse_profile("c", &json!({ "reasoning_contract": "PositiveHornProfile" }))
            .unwrap_err();
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
}
