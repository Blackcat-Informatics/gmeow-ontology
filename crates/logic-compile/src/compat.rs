// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoning-contract **compatibility feature model**.
//!
//! A [`ReasoningContract`] selects values across orthogonal reasoning facets.  Not
//! every combination is soundly evaluable: some facet pairs name semantics that
//! cannot coexist (the LOGIC-CONTRACT.md "forbidden combination" examples).  This
//! module is the **authority** for which contracts are supported; the ontology
//! surface (`logic:CompatibilityRule` individuals in `slices/grounding/logic/module.ttl`)
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
//! # Hard verdict, no silent approximation
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

use gmeow_errors::Diag;
use purrdf::{RdfDataset, TermRef};

use super::ir::{LOGIC_NAMESPACE, ReasoningContract};

/// The `rdf:type` IRI, for recognising a `logic:ReasoningContract` subject in a
/// dataset.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The contradiction-handling policy a reasoning contract selects — the typed form
/// of the `admissible_valuation` facet (the four `logic:AdmissibleValuationPolicy`
/// individuals). It is the pivot that classifies a within-world glut as a permitted,
/// disclosed conflict or a forbidden integrity violation for the scoped coherence
/// certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContradictionPolicy {
    /// Belnap's four-valued algebra: both truth-value gaps AND gluts are admitted.
    AdmitAllFour,
    /// Gaps forbidden, gluts admitted — the paraconsistent policy.
    ForbidGap,
    /// Gluts forbidden, gaps admitted — the paracomplete policy.
    ForbidGlut,
    /// Both gaps and gluts forbidden — the classical two-valued policy. The default
    /// when a contract pins no explicit `admissible_valuation` (and the implicit
    /// policy of native DL reasoning, where an inconsistency IS `owl:Nothing`).
    ForbidGapAndGlut,
}

impl ContradictionPolicy {
    /// The conservative default a contract carries when it pins no explicit
    /// `admissible_valuation`: classical, gluts forbidden. Choosing the *forbidding*
    /// default can never mask a forbidden glut (the SAFE direction); only an explicit
    /// glut-admitting policy relaxes a contradiction to a permitted disclosed conflict.
    pub const DEFAULT: Self = Self::ForbidGapAndGlut;

    /// Parse the `module.ttl` local value name. HARD-FAILS on an unrecognised name —
    /// the coherence certificate must never silently default a *garbled* policy to a
    /// permissive one (that would mask a forbidden glut and turn the gate green).
    pub fn from_local(name: &str) -> gmeow_errors::Result<Self> {
        Ok(match name {
            "AdmitAllFour" => Self::AdmitAllFour,
            "ForbidGap" => Self::ForbidGap,
            "ForbidGlut" => Self::ForbidGlut,
            "ForbidGapAndGlut" => Self::ForbidGapAndGlut,
            other => {
                return Err(Diag::of_kind(crate::error::Compat {
                    detail: format!(
                        "unknown admissible-valuation policy `{other}`; expected one of \
                     AdmitAllFour, ForbidGap, ForbidGlut, ForbidGapAndGlut"
                    ),
                }));
            }
        })
    }

    /// The policy a contract carries: its explicit `admissible_valuation` (HARD-FAIL
    /// on a garbled value), or the conservative classical [`Self::DEFAULT`] when none
    /// is pinned.
    pub fn for_contract(contract: &ReasoningContract) -> gmeow_errors::Result<Self> {
        match contract.admissible_valuation.as_deref() {
            Some(name) => Self::from_local(name),
            None => Ok(Self::DEFAULT),
        }
    }

    /// How FORBIDDING this policy is, for the conservative-resolution tie-break: a
    /// HIGHER rank forbids strictly more. `ForbidGapAndGlut` (classical, forbids
    /// both) is the most conservative; `AdmitAllFour` (admits both) the least. When
    /// several contracts in one dataset declare conflicting valuations, the most
    /// conservative governs, so a permissive contract can never relax a glut that a
    /// stricter sibling forbids.
    fn forbidding_rank(self) -> u8 {
        match self {
            Self::AdmitAllFour => 0,
            // ForbidGap and ForbidGlut each forbid exactly one of {gap, glut}. Only
            // glut-forbidding bears on the coherence certificate, so rank ForbidGlut
            // above ForbidGap; the deterministic order is fixed regardless.
            Self::ForbidGap => 1,
            Self::ForbidGlut => 2,
            Self::ForbidGapAndGlut => 3,
        }
    }

    /// Resolve the governing contradiction policy from a dataset by reading the
    /// `logic:admissibleValuation` facet on every `logic:ReasoningContract` (or
    /// `logic:ReasoningPreset`) subject the bundle declares.
    ///
    /// Resolution rule:
    /// * NO contract / NO `admissibleValuation` declared ⇒ the conservative
    ///   classical [`Self::DEFAULT`] (the `for_contract` None branch). A bundle that
    ///   pins nothing is checked under the SAFE, most-forbidding policy.
    /// * EXACTLY ONE valuation ⇒ that policy (HARD-FAIL on a garbled value name —
    ///   [`Self::from_local`] never silently defaults a garbled policy to a
    ///   permissive one).
    /// * MULTIPLE contracts declaring CONFLICTING valuations ⇒ the MOST CONSERVATIVE
    ///   (most-forbidding) of them governs, picked deterministically by
    ///   [`Self::forbidding_rank`]. A permissive contract can never overrule a
    ///   stricter sibling's forbiddance.
    ///
    /// # Errors
    /// Propagates the [`Self::from_local`] error if ANY declared valuation is a
    /// garbled / unknown policy name — a HARD FAIL, never a silent fallback.
    pub fn resolve_from_dataset(dataset: &RdfDataset) -> gmeow_errors::Result<Self> {
        let admissible_valuation = format!("{LOGIC_NAMESPACE}admissibleValuation");
        let contract_type = format!("{LOGIC_NAMESPACE}ReasoningContract");
        let preset_type = format!("{LOGIC_NAMESPACE}ReasoningPreset");

        // First pass: collect the subjects typed as a reasoning contract / preset.
        let mut contract_subjects: std::collections::BTreeSet<String> =
            std::collections::BTreeSet::new();
        for q in dataset.quads() {
            let (TermRef::Iri(p), TermRef::Iri(o)) = (dataset.resolve(q.p), dataset.resolve(q.o))
            else {
                continue;
            };
            if p == RDF_TYPE
                && (o == contract_type || o == preset_type)
                && let TermRef::Iri(s) = dataset.resolve(q.s)
            {
                contract_subjects.insert(s.to_string());
            }
        }

        // Second pass: read each contract subject's admissibleValuation. A
        // valuation triple on a non-contract subject is ignored — only a declared
        // contract governs.
        let mut governing: Option<Self> = None;
        for q in dataset.quads() {
            let (TermRef::Iri(s), TermRef::Iri(p), TermRef::Iri(o)) = (
                dataset.resolve(q.s),
                dataset.resolve(q.p),
                dataset.resolve(q.o),
            ) else {
                continue;
            };
            if p != admissible_valuation || !contract_subjects.contains(s) {
                continue;
            }
            // The object is the facet-value IRI `logic:<Policy>`; strip the prefix.
            let Some(local) = o.strip_prefix(LOGIC_NAMESPACE) else {
                continue;
            };
            // HARD-FAIL on a garbled value — never silently default to permissive.
            let policy = Self::from_local(local)?;
            governing = Some(match governing {
                None => policy,
                Some(current) => {
                    if policy.forbidding_rank() > current.forbidding_rank() {
                        policy
                    } else {
                        current
                    }
                }
            });
        }

        Ok(governing.unwrap_or(Self::DEFAULT))
    }

    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::AdmitAllFour => "AdmitAllFour",
            Self::ForbidGap => "ForbidGap",
            Self::ForbidGlut => "ForbidGlut",
            Self::ForbidGapAndGlut => "ForbidGapAndGlut",
        }
    }

    /// The full IRI of the `logic:AdmissibleValuationPolicy` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }

    /// Whether a within-world GLUT (a witnessed contradiction) is PERMITTED under
    /// this policy. Wildcard-free `match` so a future policy variant is a COMPILE
    /// error, never a silent default. NARROWER than [`admits_gap_or_glut`]:
    /// `ForbidGlut` admits a gap but FORBIDS a glut, so it is not glut-permitting.
    pub fn glut_permitted(self) -> bool {
        match self {
            Self::AdmitAllFour | Self::ForbidGap => true,
            Self::ForbidGlut | Self::ForbidGapAndGlut => false,
        }
    }

    /// Every variant, for the Rust↔TTL cross-check.
    pub const ALL: &'static [Self] = &[
        Self::AdmitAllFour,
        Self::ForbidGap,
        Self::ForbidGlut,
        Self::ForbidGapAndGlut,
    ];
}

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
/// bundles in `slices/grounding/logic/module.ttl`.
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
        let mut observed: std::collections::BTreeSet<&'static str> =
            std::collections::BTreeSet::new();
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
}
