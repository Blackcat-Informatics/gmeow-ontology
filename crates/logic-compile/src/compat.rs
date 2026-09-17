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

#[cfg(test)]
mod test_support;
#[cfg(test)]
use crate::ir;
#[cfg(test)]
use test_support::{preset_contracts, table_rule_ids};

#[path = "compat.tests.rs"]
#[cfg(test)]
mod tests;
