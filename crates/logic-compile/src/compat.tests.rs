// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::ReasoningContract;

fn unsupported_reasons(c: &ReasoningContract) -> Vec<String> {
    match check(c) {
        ContractVerdict::Unsupported(reasons) => reasons,
        ContractVerdict::Supported => panic!("expected Unsupported, got Supported"),
    }
}

fn dataset_from(ttl: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    let header = "\
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";
    purrdf::parse_dataset(format!("{header}{ttl}").as_bytes(), "text/turtle", None)
        .expect("parse contract turtle")
}

#[test]
fn resolve_from_dataset_reads_the_declared_valuation() {
    let ds = dataset_from(
        "logic:c rdf:type logic:ReasoningContract ; logic:admissibleValuation logic:ForbidGap .",
    );
    assert_eq!(
        ContradictionPolicy::resolve_from_dataset(ds.as_ref()).unwrap(),
        ContradictionPolicy::ForbidGap
    );
}

#[test]
fn resolve_from_dataset_defaults_to_classical_when_none_declared() {
    // No contract / no valuation ⇒ the conservative classical DEFAULT.
    let ds = dataset_from("logic:x rdf:type logic:Anything .");
    assert_eq!(
        ContradictionPolicy::resolve_from_dataset(ds.as_ref()).unwrap(),
        ContradictionPolicy::DEFAULT
    );
}

#[test]
fn resolve_from_dataset_picks_the_most_conservative_of_conflicting_contracts() {
    // Two contracts: one admits a glut (ForbidGap), one forbids both
    // (ForbidGapAndGlut). The MOST CONSERVATIVE (most-forbidding) governs.
    let ds = dataset_from(
        "\
logic:permissive rdf:type logic:ReasoningContract ; logic:admissibleValuation logic:ForbidGap .
logic:strict rdf:type logic:ReasoningContract ; logic:admissibleValuation logic:ForbidGapAndGlut .",
    );
    assert_eq!(
        ContradictionPolicy::resolve_from_dataset(ds.as_ref()).unwrap(),
        ContradictionPolicy::ForbidGapAndGlut
    );
}

#[test]
fn resolve_from_dataset_hard_fails_on_a_garbled_valuation() {
    let ds = dataset_from(
        "logic:c rdf:type logic:ReasoningContract ; logic:admissibleValuation logic:Nonsense .",
    );
    assert!(ContradictionPolicy::resolve_from_dataset(ds.as_ref()).is_err());
}

// ── Forbidden combinations each fire with the right reason ───────────────

#[test]
fn probabilistic_stable_model_is_unsupported() {
    let mut c = ReasoningContract::new();
    c.uncertainty_measures
        .insert("ProbabilisticMeasure".to_owned());
    c.model_semantics = Some("StableModelSemantics".to_owned());
    let reasons = unsupported_reasons(&c);
    assert_eq!(reasons.len(), 1);
    // Each reason names its rule id so the diagnostic identifies which rule fired.
    assert!(reasons[0].contains("[RuleNoProbabilisticStableModel]"));
    assert!(reasons[0].contains("ProbabilisticMeasure"));
    assert!(reasons[0].contains("StableModelSemantics"));
}

#[test]
fn probabilistic_without_stable_model_is_supported() {
    // The measure alone (e.g. over least-model semantics) is fine here; the
    // model-declaration requirement is graph-dependent and enforced elsewhere.
    let mut c = ReasoningContract::new();
    c.uncertainty_measures
        .insert("ProbabilisticMeasure".to_owned());
    c.model_semantics = Some("LeastModelSemantics".to_owned());
    assert!(check(&c).is_supported());
}

#[test]
fn paraconsistent_valuation_under_counterfactual_revision_is_unsupported() {
    for valuation in ["AdmitAllFour", "ForbidGap", "ForbidGlut"] {
        let mut c = ReasoningContract::new();
        c.admissible_valuation = Some(valuation.to_owned());
        c.revision = Some("EntrenchmentRevision".to_owned());
        let reasons = unsupported_reasons(&c);
        assert_eq!(reasons.len(), 1, "valuation {valuation}");
        assert!(reasons[0].contains("EntrenchmentRevision"));
        assert!(reasons[0].contains("paraconsistent"));
    }
}

#[test]
fn belnap_algebra_under_counterfactual_revision_is_unsupported() {
    let mut c = ReasoningContract::new();
    c.truth_algebra = Some("BelnapBilattice".to_owned());
    c.revision = Some("EntrenchmentRevision".to_owned());
    let reasons = unsupported_reasons(&c);
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].contains("BelnapBilattice"));
}

#[test]
fn classical_valuation_under_counterfactual_revision_is_supported() {
    // ForbidGapAndGlut admits neither a gap nor a glut ⇒ not paraconsistent.
    let mut c = ReasoningContract::new();
    c.admissible_valuation = Some("ForbidGapAndGlut".to_owned());
    c.revision = Some("EntrenchmentRevision".to_owned());
    assert!(check(&c).is_supported());
}

#[test]
fn closed_world_default_in_counterfactual_is_unsupported() {
    let mut c = ReasoningContract::new();
    c.default_closure = Some("ClosedWorldClosure".to_owned());
    c.revision = Some("EntrenchmentRevision".to_owned());
    let reasons = unsupported_reasons(&c);
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].contains("ClosedWorldClosure"));
}

#[test]
fn closed_world_per_key_entry_in_counterfactual_is_unsupported() {
    let mut c = ReasoningContract::new();
    c.closure_entries
        .insert("ex:pred".to_owned(), "ClosedWorldClosure".to_owned());
    c.revision = Some("EntrenchmentRevision".to_owned());
    let reasons = unsupported_reasons(&c);
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].contains("ClosedWorldClosure"));
}

#[test]
fn closed_world_without_counterfactual_revision_is_supported() {
    let mut c = ReasoningContract::new();
    c.default_closure = Some("ClosedWorldClosure".to_owned());
    c.revision = Some("MonotonicRevision".to_owned());
    assert!(check(&c).is_supported());
}

#[test]
fn multiple_violations_collect_all_reasons() {
    // Both the probabilistic-stable-model rule and the closed-world-in-
    // counterfactual rule fire on one contract.
    let mut c = ReasoningContract::new();
    c.uncertainty_measures
        .insert("ProbabilisticMeasure".to_owned());
    c.model_semantics = Some("StableModelSemantics".to_owned());
    c.default_closure = Some("ClosedWorldClosure".to_owned());
    c.revision = Some("EntrenchmentRevision".to_owned());
    let reasons = unsupported_reasons(&c);
    assert_eq!(reasons.len(), 2);
}

// ── Clean / preset contracts are supported ───────────────────────────────

#[test]
fn empty_contract_is_supported() {
    assert!(check(&ReasoningContract::new()).is_supported());
}

// ── Rule-id catalog integrity ────────────────────────────────────────────

#[test]
fn every_table_rule_id_is_in_all_rule_ids() {
    // The contract-internal table rules are a subset of the full catalog; the
    // one remaining id is the graph-dependent front-end rule.
    let table = table_rule_ids();
    let all: std::collections::BTreeSet<&str> = ALL_RULE_IDS.iter().copied().collect();
    assert!(table.is_subset(&all));
    // Exactly one catalog id is NOT a table rule (the graph-dependent rule).
    assert_eq!(all.difference(&table).count(), 1);
    assert!(all.contains("RuleProbabilisticRequiresModel"));
}

#[test]
fn all_rule_ids_are_unique() {
    let unique: std::collections::BTreeSet<&str> = ALL_RULE_IDS.iter().copied().collect();
    assert_eq!(unique.len(), ALL_RULE_IDS.len());
}

#[test]
fn every_preset_contract_is_supported() {
    for contract in preset_contracts() {
        assert!(
            check(&contract).is_supported(),
            "preset {:?} should be supported: {:?}",
            contract.preset,
            check(&contract),
        );
    }
}

#[test]
fn contradiction_policy_glut_permitted_truth_table() {
    // The glut-permitting policies admit a glut; the glut-forbidding ones do not.
    // NARROWER than admits_gap_or_glut: ForbidGlut admits a gap but FORBIDS a glut.
    assert!(ContradictionPolicy::AdmitAllFour.glut_permitted());
    assert!(ContradictionPolicy::ForbidGap.glut_permitted());
    assert!(!ContradictionPolicy::ForbidGlut.glut_permitted());
    assert!(!ContradictionPolicy::ForbidGapAndGlut.glut_permitted());
}

#[test]
fn contradiction_policy_local_name_round_trips_and_hard_fails() {
    for &policy in ContradictionPolicy::ALL {
        assert_eq!(
            ContradictionPolicy::from_local(policy.local_name()).unwrap(),
            policy
        );
        assert!(policy.iri().ends_with(policy.local_name()));
    }
    // A garbled value is a HARD FAIL, never a silent permissive default.
    assert!(ContradictionPolicy::from_local("Permissive").is_err());
    assert!(ContradictionPolicy::from_local("").is_err());
}

#[test]
fn contradiction_policy_for_contract_defaults_to_classical() {
    // No explicit admissible_valuation ⇒ conservative classical default (gluts
    // forbidden), so the native DL path keeps treating a glut as a violation.
    let bare = ReasoningContract::default();
    assert_eq!(bare.admissible_valuation, None);
    assert_eq!(
        ContradictionPolicy::for_contract(&bare).unwrap(),
        ContradictionPolicy::ForbidGapAndGlut
    );
    // An explicit glut-admitting policy relaxes it to permitted.
    let paraconsistent = ReasoningContract {
        admissible_valuation: Some("ForbidGap".to_owned()),
        ..ReasoningContract::default()
    };
    assert_eq!(
        ContradictionPolicy::for_contract(&paraconsistent).unwrap(),
        ContradictionPolicy::ForbidGap
    );
    // A garbled explicit value propagates the hard failure.
    let garbled = ReasoningContract {
        admissible_valuation: Some("nonsense".to_owned()),
        ..ReasoningContract::default()
    };
    assert!(ContradictionPolicy::for_contract(&garbled).is_err());
}

// ── Facet-combination completeness sweep (ME1 watch-item) ────────────
//
// The unit tests above each pin ONE forbidden combination. They do not answer
// the meta-epic's standing question: as facets multiply, is the feature model
// *complete* — does `check` fire EXACTLY the right rules over the whole space of
// facet combinations, never silently approximating a forbidden contract to a
// supported one (a false-supported) and never rejecting a sound one (a
// false-unsupported)? This block enumerates the full cross-product of the
// rule-participating facet domains and checks every contract against an
// INDEPENDENT oracle — a second, hand-written statement of the three documented
// forbidden combinations that does NOT call [`check`], [`RULES`], [`FacetRef`],
// or [`admits_gap_or_glut`]. Agreement across the whole product is the
// completeness witness; a divergence is a real soundness bug, not flakiness.

/// The value domains the sweep varies, one tuple field per rule-participating
/// facet (plus `None`/absent where the facet is optional). Deliberately
/// INDEPENDENT of the production value lists — if a new facet value gains a rule,
/// it must be added here too, and the completeness guard
/// (`sweep_reaches_every_table_rule`) fails loudly until it is.
const SWEEP_MODEL_SEMANTICS: &[Option<&str>] = &[
    None,
    Some("LeastModelSemantics"),
    Some("StratifiedSemantics"),
    Some("WellFoundedSemantics"),
    Some("StableModelSemantics"),
];
const SWEEP_TRUTH_ALGEBRA: &[Option<&str>] =
    &[None, Some("BelnapBilattice"), Some("TwoValuedBoolean")];
const SWEEP_REVISION: &[Option<&str>] = &[
    None,
    Some("MonotonicRevision"),
    Some("EntrenchmentRevision"),
];
const SWEEP_DEFAULT_CLOSURE: &[Option<&str>] =
    &[None, Some("OpenWorldClosure"), Some("ClosedWorldClosure")];
const SWEEP_ADMISSIBLE_VALUATION: &[Option<&str>] = &[
    None,
    Some("AdmitAllFour"),
    Some("ForbidGap"),
    Some("ForbidGlut"),
    Some("ForbidGapAndGlut"),
];

/// The independent oracle: the set of rule-ids that SHOULD fire for `c`, derived
/// by re-stating the three documented forbidden combinations by hand
/// (LOGIC-CONTRACT.md), with NO reference to the production
/// `RULES`/`FacetRef`/`admits_gap_or_glut`. This is intentionally a separate
/// implementation so the sweep tests intent, not a tautology against the table.
fn oracle_fired_ids(c: &ReasoningContract) -> std::collections::BTreeSet<&'static str> {
    let mut ids = std::collections::BTreeSet::new();

    // Rule 1: a probabilistic uncertainty measure cannot coexist with stable-model
    // semantics (no calibrated mass over incomparable answer sets).
    let probabilistic = c.uncertainty_measures.contains("ProbabilisticMeasure");
    let stable_model = c.model_semantics.as_deref() == Some("StableModelSemantics");
    if probabilistic && stable_model {
        ids.insert("RuleNoProbabilisticStableModel");
    }

    // Rule 2: a paraconsistent valuation — a gap/glut-admitting admissibleValuation
    // (anything but the classical gap-and-glut-forbidding policy) OR the Belnap
    // bilattice truth algebra — cannot coexist with counterfactual entrenchment
    // revision.
    let counterfactual = c.revision.as_deref() == Some("EntrenchmentRevision");
    let valuation_admits_gap_or_glut = matches!(
        c.admissible_valuation.as_deref(),
        Some("AdmitAllFour") | Some("ForbidGap") | Some("ForbidGlut")
    );
    let belnap = c.truth_algebra.as_deref() == Some("BelnapBilattice");
    if (valuation_admits_gap_or_glut || belnap) && counterfactual {
        ids.insert("RuleNoParaconsistentCounterfactualRevision");
    }

    // Rule 3: closed-world closure — the default closure OR any per-key closure
    // entry — cannot coexist with counterfactual entrenchment revision (its
    // generated states are open-ended).
    let closed_world_anywhere = c.default_closure.as_deref() == Some("ClosedWorldClosure")
        || c.closure_entries
            .values()
            .any(|v| v == "ClosedWorldClosure");
    if closed_world_anywhere && counterfactual {
        ids.insert("RuleNoClosedWorldInCounterfactual");
    }

    ids
}

/// Extract the rule-ids `check` actually fired from its verdict, by parsing the
/// `[{id}] {reason}` prefix each reason carries (the format `check` builds).
///
/// A rule may fire AT MOST ONCE per contract: each id appears once in [`RULES`], so a
/// verdict carrying the same id twice would be a real diagnostic defect. We assert
/// that invariant here (parse to a `Vec` and reject duplicates) BEFORE collapsing to
/// a set — otherwise a `BTreeSet` would silently dedupe a double-firing and weaken the
/// sweep's "fires EXACTLY the right rules" guarantee.
fn fired_ids_from_verdict(verdict: &ContractVerdict) -> std::collections::BTreeSet<String> {
    let reasons = match verdict {
        ContractVerdict::Supported => return std::collections::BTreeSet::new(),
        ContractVerdict::Unsupported(reasons) => reasons,
    };
    let ids: Vec<String> = reasons
        .iter()
        .map(|r| {
            let id = r
                .strip_prefix('[')
                .and_then(|s| s.split_once(']'))
                .map(|(id, _)| id)
                .unwrap_or_else(|| panic!("reason not in `[id] ...` form: {r}"));
            id.to_owned()
        })
        .collect();
    let set: std::collections::BTreeSet<String> = ids.iter().cloned().collect();
    assert_eq!(
        set.len(),
        ids.len(),
        "a rule fired more than once in one verdict (each rule must fire at most once): {ids:?}"
    );
    set
}

/// Build every contract in the swept cross-product, invoking `body` on each.
/// 5 × 3 × 3 × 3 × 5 (singletons) × 2 (probabilistic measure) × 2 (per-key
/// closed-world entry) = 8100 contracts; each `check` is µs-scale.
fn for_each_swept_contract(mut body: impl FnMut(&ReasoningContract)) {
    for &model in SWEEP_MODEL_SEMANTICS {
        for &algebra in SWEEP_TRUTH_ALGEBRA {
            for &revision in SWEEP_REVISION {
                for &closure in SWEEP_DEFAULT_CLOSURE {
                    for &valuation in SWEEP_ADMISSIBLE_VALUATION {
                        for &probabilistic in &[false, true] {
                            for &per_key_closed_world in &[false, true] {
                                let mut c = ReasoningContract::new();
                                c.model_semantics = model.map(str::to_owned);
                                c.truth_algebra = algebra.map(str::to_owned);
                                c.revision = revision.map(str::to_owned);
                                c.default_closure = closure.map(str::to_owned);
                                c.admissible_valuation = valuation.map(str::to_owned);
                                if probabilistic {
                                    c.uncertainty_measures
                                        .insert("ProbabilisticMeasure".to_owned());
                                }
                                if per_key_closed_world {
                                    c.closure_entries.insert(
                                        "ex:pred".to_owned(),
                                        "ClosedWorldClosure".to_owned(),
                                    );
                                }
                                body(&c);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn sweep_check_agrees_with_the_independent_oracle() {
    // The completeness assertion: over the WHOLE facet cross-product, `check`
    // fires exactly the rule set an independent oracle says it must. This pins
    // BOTH directions at once — no false-supported (a forbidden combo silently
    // approximated to Supported) and no false-unsupported (a sound combo
    // rejected) — for every combination, not just the hand-picked unit cases.
    for_each_swept_contract(|c| {
        let verdict = check(c);
        let actual = fired_ids_from_verdict(&verdict);
        let expected: std::collections::BTreeSet<String> =
            oracle_fired_ids(c).into_iter().map(str::to_owned).collect();
        assert_eq!(
            actual, expected,
            "check/oracle disagree for contract {c:?}: check fired {actual:?}, oracle expected {expected:?}"
        );
        // The verdict's Supported flag must agree with the oracle's emptiness.
        assert_eq!(
            verdict.is_supported(),
            expected.is_empty(),
            "Supported flag disagrees with oracle for contract {c:?}"
        );
    });
}

#[test]
fn sweep_reaches_every_table_rule() {
    // OCP completeness guard: every table rule must FIRE at least once somewhere
    // in the swept domain. If a future rule keys on a facet/value the sweep does
    // not vary, that rule never fires here and this guard fails — forcing the
    // swept domains above to be extended in lockstep with the rule table, so
    // coverage can never silently fall behind the feature model.
    let mut observed: std::collections::BTreeSet<&'static str> = std::collections::BTreeSet::new();
    for_each_swept_contract(|c| {
        if let ContractVerdict::Unsupported(reasons) = check(c) {
            for r in &reasons {
                let id = r
                    .strip_prefix('[')
                    .and_then(|s| s.split_once(']'))
                    .map(|(id, _)| id)
                    .expect("reason in `[id] ...` form");
                // Re-map the parsed &str to a &'static rule id from the table so
                // the observed set is comparable to `table_rule_ids()`.
                if let Some(stable) = RULES.iter().map(|rule| rule.id).find(|sid| *sid == id) {
                    observed.insert(stable);
                }
            }
        }
    });
    assert_eq!(
        observed,
        table_rule_ids(),
        "the sweep does not reach every table rule — extend the swept facet domains \
             to cover the rule(s) in the symmetric difference"
    );
}

#[test]
fn sweep_determinism_check_is_stable() {
    // `check` is a pure function of the contract: evaluating the same contract
    // twice yields the identical verdict, across the whole swept domain.
    for_each_swept_contract(|c| {
        assert_eq!(check(c), check(c), "check is non-deterministic for {c:?}");
    });
}
