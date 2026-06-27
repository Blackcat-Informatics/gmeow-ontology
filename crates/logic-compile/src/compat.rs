// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoning-contract **compatibility feature model** (#767, Task 3).
//!
//! A [`ReasoningContract`] selects values across orthogonal reasoning facets.  Not
//! every combination is soundly evaluable: some facet pairs name semantics that
//! cannot coexist (the LOGIC-CONTRACT.md "forbidden combination" examples).  This
//! module is the **authority** for which contracts are supported; the ontology
//! surface (`logic:CompatibilityRule` individuals in `slices/core/logic/module.ttl`)
//! is a lossy documentation projection of the [`RULES`] table (Principle 17).
//!
//! # Design — a data table, not a cascade of `if`s
//!
//! The feature model is expressed as a flat array [`RULES`] of [`CompatibilityRule`]
//! entries.  Each rule is `(id, kind, lhs, rhs, reason)`: an `lhs` facet condition
//! and an `rhs` facet condition combined by a [`RuleKind`].  A single generic
//! evaluator ([`check`]) iterates the table once, testing each rule against the
//! contract; the verdict is [`ContractVerdict::Supported`] iff no rule fires.
//! Adding a forbidden combination is one new array entry — no new control flow.
//!
//! # Hard verdict, no silent approximation (reviewer C3)
//!
//! An [`ContractVerdict::Unsupported`] contract is a **hard** condition.  The
//! front-end turns it into a `Severity::Error` diagnostic so the compile `Report`
//! is not ok and the program is never silently approximated to a nearby semantics.
//!
//! # Counterfactual coupling (LOGIC-CONTRACT.md)
//!
//! Two of the rules below couple a facet against *counterfactual-world generation*.
//! At the contract level the indicator of counterfactual-world generation is the
//! `logic:EntrenchmentRevision` revision-policy value: entrenchment revision is the
//! belief-revision operator that constructs the counterfactual (closest-world)
//! states a query ranges over.  The rules therefore key on `revision ==
//! Some("EntrenchmentRevision")` rather than on a modality string — the contract
//! carries no per-world modality, only its revision policy.

use super::ir::ReasoningContract;

/// The verdict of a compatibility [`check`] over a [`ReasoningContract`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractVerdict {
    /// The contract violates no [`CompatibilityRule`]; it is soundly evaluable.
    Supported,
    /// The contract violates one or more rules; the carried strings are the
    /// human-readable reasons (one per violated rule, in [`RULES`] order).
    Unsupported(Vec<String>),
}

impl ContractVerdict {
    /// `true` iff the contract is [`ContractVerdict::Supported`].
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }
}

/// One side of a [`CompatibilityRule`]: a predicate over a [`ReasoningContract`]
/// that names a facet condition by facet + local value name(s).
///
/// Each variant is matched against the contract by [`FacetRef::matches`]; the
/// reference is the *only* place that knows which contract field a facet lives in,
/// so a rule entry stays purely declarative.
#[derive(Debug, Clone, Copy)]
enum FacetRef {
    /// The single-valued `model_semantics` facet equals this local value name.
    ModelSemantics(&'static str),
    /// The single-valued `truth_algebra` facet equals this local value name.
    TruthAlgebra(&'static str),
    /// The single-valued `revision` facet equals this local value name.
    Revision(&'static str),
    /// The set-valued `uncertainty_measures` facet contains this local value name.
    UncertaintyMeasureContains(&'static str),
    /// The single-valued `admissible_valuation` facet admits a gap or a glut —
    /// i.e. it is any recognised paraconsistent/paracomplete policy value
    /// (anything except the gap-and-glut-forbidding classical policy).  Holds for
    /// `AdmitAllFour`, `ForbidGap`, and `ForbidGlut`.
    AdmissibleValuationAdmitsGapOrGlut,
    /// `default_closure` equals this local value name OR any `closure_entries`
    /// map value equals it (the closure value appears anywhere in the contract's
    /// closure map, default or per-key).
    ClosureValueAnywhere(&'static str),
}

impl FacetRef {
    /// Whether this facet condition holds for `contract`.
    fn matches(&self, contract: &ReasoningContract) -> bool {
        match self {
            Self::ModelSemantics(v) => contract.model_semantics.as_deref() == Some(v),
            Self::TruthAlgebra(v) => contract.truth_algebra.as_deref() == Some(v),
            Self::Revision(v) => contract.revision.as_deref() == Some(v),
            Self::UncertaintyMeasureContains(v) => contract.uncertainty_measures.contains(*v),
            Self::AdmissibleValuationAdmitsGapOrGlut => contract
                .admissible_valuation
                .as_deref()
                .is_some_and(admits_gap_or_glut),
            Self::ClosureValueAnywhere(v) => {
                contract.default_closure.as_deref() == Some(v)
                    || contract.closure_entries.values().any(|cv| cv == v)
            }
        }
    }
}

/// Whether an `admissible_valuation` local value name admits a truth-value gap or
/// glut (the paraconsistent/paracomplete policies), i.e. is NOT the classical
/// gap-and-glut-forbidding policy.  Keyed by the named module.ttl value individuals.
fn admits_gap_or_glut(value: &str) -> bool {
    matches!(value, "AdmitAllFour" | "ForbidGap" | "ForbidGlut")
}

/// How a [`CompatibilityRule`]'s two facet conditions combine.
#[derive(Debug, Clone, Copy)]
enum RuleKind {
    /// `lhs` and `rhs` mutually **exclude**: if both hold the contract is
    /// unsupported (the named pair cannot coexist).
    Excludes,
}

/// A single declarative rule of the feature model.
///
/// A rule has an `lhs` facet condition and an `rhs` facet condition combined by
/// `kind`.  For [`RuleKind::Excludes`] the rule fires (the contract is unsupported)
/// iff both conditions hold.  `lhs` may carry several alternative facet conditions
/// (any one satisfies the side) to express "a paraconsistent valuation OR the
/// Belnap algebra" without duplicating rules.
struct CompatibilityRule {
    /// Stable rule id; matches the `logic:CompatibilityRule` individual local name.
    id: &'static str,
    /// How the two sides combine.
    kind: RuleKind,
    /// The left-hand facet condition(s); the side holds if ANY entry matches.
    lhs: &'static [FacetRef],
    /// The right-hand facet condition.
    rhs: FacetRef,
    /// The human-readable reason surfaced when the rule fires.
    reason: &'static str,
}

impl CompatibilityRule {
    /// Whether this rule fires (is violated) against `contract`.
    fn fires(&self, contract: &ReasoningContract) -> bool {
        match self.kind {
            RuleKind::Excludes => {
                let lhs = self.lhs.iter().any(|f| f.matches(contract));
                lhs && self.rhs.matches(contract)
            }
        }
    }
}

/// The feature-model rule table — the sole authority for unsupported contracts.
///
/// Each entry is one forbidden combination from the LOGIC-CONTRACT.md examples; a
/// new forbidden combination is one new entry here (and one mirroring
/// `logic:CompatibilityRule` individual in module.ttl, cross-checked by a test).
/// The `RuleProbabilisticRequiresModel` rule is graph-dependent (it needs a
/// declared `logic:ProbabilityModel`), so it lives in the front-end, not here; it
/// has a `logic:CompatibilityRule` individual but no [`CompatibilityRule`] table
/// entry.
const RULES: &[CompatibilityRule] = &[
    // Probabilistic measures over the (multi-model, non-deterministic) stable-model
    // semantics: a calibrated probability mass cannot be defined over a contract
    // whose model semantics admits several incomparable answer sets.
    CompatibilityRule {
        id: "RuleNoProbabilisticStableModel",
        kind: RuleKind::Excludes,
        lhs: &[FacetRef::UncertaintyMeasureContains("ProbabilisticMeasure")],
        rhs: FacetRef::ModelSemantics("StableModelSemantics"),
        reason: "a probabilistic uncertainty measure (logic:ProbabilisticMeasure) cannot be \
                 combined with stable-model semantics (logic:StableModelSemantics): a calibrated \
                 probability mass is not well-defined over the multiple incomparable answer sets \
                 the stable-model semantics admits",
    },
    // Paraconsistent truth (a gap/glut-admitting valuation OR the Belnap bilattice)
    // under counterfactual (entrenchment) revision: the counterfactual-world
    // generator assumes classical closest-world selection and is not defined over
    // gappy/glutty valuations.
    CompatibilityRule {
        id: "RuleNoParaconsistentCounterfactualRevision",
        kind: RuleKind::Excludes,
        lhs: &[
            FacetRef::AdmissibleValuationAdmitsGapOrGlut,
            FacetRef::TruthAlgebra("BelnapBilattice"),
        ],
        rhs: FacetRef::Revision("EntrenchmentRevision"),
        reason: "a paraconsistent valuation (a gap/glut-admitting logic:admissibleValuation or \
                 the logic:BelnapBilattice truth algebra) cannot be combined with counterfactual \
                 entrenchment revision (logic:EntrenchmentRevision): the closest-world selection \
                 that generates the counterfactual states is not defined over gappy or glutty \
                 valuations",
    },
    // Closed-world closure inside generated counterfactual states: the
    // counterfactual states generated by entrenchment revision are open-ended, so
    // a closed-world (negation-by-absence) reading inside them is unsound.
    CompatibilityRule {
        id: "RuleNoClosedWorldInCounterfactual",
        kind: RuleKind::Excludes,
        lhs: &[FacetRef::ClosureValueAnywhere("ClosedWorldClosure")],
        rhs: FacetRef::Revision("EntrenchmentRevision"),
        reason: "closed-world closure (logic:ClosedWorldClosure, whether the default closure or \
                 any per-key closure entry) cannot be combined with counterfactual entrenchment \
                 revision (logic:EntrenchmentRevision): the counterfactual states it generates \
                 are open-ended, so reading absence as falsehood inside them is unsound",
    },
];

/// The stable ids of every compatibility rule the Rust authority knows — both the
/// table-driven [`RULES`] and the graph-dependent front-end rule
/// (`RuleProbabilisticRequiresModel`).  This is the set the ontology surface
/// (`logic:CompatibilityRule` individuals) must mirror exactly; the cross-check
/// test in `ir/tests.rs` pins the two together.
pub const ALL_RULE_IDS: &[&str] = &[
    "RuleNoClosedWorldInCounterfactual",
    "RuleNoParaconsistentCounterfactualRevision",
    "RuleNoProbabilisticStableModel",
    "RuleProbabilisticRequiresModel",
];

/// Run every [`RULES`] entry against `contract`, collecting the reason of each
/// rule that fires (in table order).  The verdict is [`ContractVerdict::Supported`]
/// iff none fire.
///
/// This covers only the contract-internal feature model; the graph-dependent
/// `RuleProbabilisticRequiresModel` (probabilistic measure requires a declared
/// `logic:ProbabilityModel`) is enforced in the front-end where the source graph
/// is available.
pub fn check(contract: &ReasoningContract) -> ContractVerdict {
    let reasons: Vec<String> = RULES
        .iter()
        .filter(|rule| rule.fires(contract))
        .map(|rule| format!("[{}] {}", rule.id, rule.reason))
        .collect();
    if reasons.is_empty() {
        ContractVerdict::Supported
    } else {
        ContractVerdict::Unsupported(reasons)
    }
}

/// Build the expanded facet contract for each of the six named presets, as the
/// front-end would after expanding `logic:expandsToFacet`.  Used by the tests to
/// assert every preset's contract is supported.  Mirrors the `expandsToFacet`
/// bundles in `slices/core/logic/module.ttl`.
#[cfg(test)]
fn preset_contracts() -> Vec<ReasoningContract> {
    use super::ir::SemanticProfileId;

    // PositiveHorn: HornFragment, LeastModelSemantics, MonotonicRevision,
    // OpenWorldClosure (default), CertifiedFragmentResource.
    let mut positive_horn = ReasoningContract::from_preset(SemanticProfileId::PositiveHorn);
    positive_horn.formula_fragment = Some("HornFragment".to_owned());
    positive_horn.model_semantics = Some("LeastModelSemantics".to_owned());
    positive_horn.revision = Some("MonotonicRevision".to_owned());
    positive_horn.default_closure = Some("OpenWorldClosure".to_owned());
    positive_horn
        .resource_policies
        .insert("CertifiedFragmentResource".to_owned());

    // StratifiedNAF: DefaultNegation, StratifiedSemantics.
    let mut stratified = ReasoningContract::from_preset(SemanticProfileId::StratifiedNaf);
    stratified
        .negation_operators
        .insert("DefaultNegation".to_owned());
    stratified.model_semantics = Some("StratifiedSemantics".to_owned());

    // WellFounded: DefaultNegation, WellFoundedSemantics.
    let mut well_founded = ReasoningContract::from_preset(SemanticProfileId::WellFounded);
    well_founded
        .negation_operators
        .insert("DefaultNegation".to_owned());
    well_founded.model_semantics = Some("WellFoundedSemantics".to_owned());

    // StableModel: DefaultNegation, StableModelSemantics. (No probabilistic
    // measure ⇒ supported.)
    let mut stable = ReasoningContract::from_preset(SemanticProfileId::StableModel);
    stable
        .negation_operators
        .insert("DefaultNegation".to_owned());
    stable.model_semantics = Some("StableModelSemantics".to_owned());

    // ProceduralProlog: HornFragment, BudgetBoundedResource.
    let mut prolog = ReasoningContract::from_preset(SemanticProfileId::ProceduralProlog);
    prolog.formula_fragment = Some("HornFragment".to_owned());
    prolog
        .resource_policies
        .insert("BudgetBoundedResource".to_owned());

    // Probabilistic: ProbabilisticMeasure. (No StableModelSemantics ⇒ supported by
    // the table; the model-declaration requirement is graph-dependent.)
    let mut probabilistic = ReasoningContract::from_preset(SemanticProfileId::Probabilistic);
    probabilistic
        .uncertainty_measures
        .insert("ProbabilisticMeasure".to_owned());

    vec![
        positive_horn,
        stratified,
        well_founded,
        stable,
        prolog,
        probabilistic,
    ]
}

/// The set of ids of every table-driven rule (a subset of [`ALL_RULE_IDS`]; the
/// remaining id is the graph-dependent front-end rule).
#[cfg(test)]
fn table_rule_ids() -> std::collections::BTreeSet<&'static str> {
    RULES.iter().map(|r| r.id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ReasoningContract;

    fn unsupported_reasons(c: &ReasoningContract) -> Vec<String> {
        match check(c) {
            ContractVerdict::Unsupported(reasons) => reasons,
            ContractVerdict::Supported => panic!("expected Unsupported, got Supported"),
        }
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
}
