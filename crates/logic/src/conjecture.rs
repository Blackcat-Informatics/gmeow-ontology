// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Conjecture-and-refutation: test a candidate first-order formula against a KB.
//!
//! `conjecture_test` is the runtime for `logic:Conjecture` (design/LOGIC-FOUNDATION.md,
//! the conjecture-and-refutation section). It **generalizes** the counterfactual
//! [`crate::counterfactual::construct_and_resolve`] from ground assume-atoms to a full
//! candidate `Formula` — crucially including a universally-quantified Horn implication
//! `∀x. body → head`, which is lowered to an evaluable rule and RESOLVED, never refused.
//!
//! # The symmetric test — two independent legs, `φ` and `¬φ`
//!
//! The candidate `φ` is tested inside a fresh, ISOLATED scenario world built as
//! `KB ∪ assume_context` (the input KB is borrowed `&` and never mutated — isolation is
//! inherent because [`crate::reason::reason_program`] / [`crate::reason::reason_all`] take
//! `&RdfDataset` and load into their own store). Two closures are computed over the *same*
//! scenario EDB, both carrying the fixed DL consistency calculus:
//!
//! * `base` — the closure with NO candidate.
//! * `with_phi` — the closure WITH `φ` asserted (a ground fact added to the EDB, or the
//!   candidate program `P_phi` evaluated over the EDB).
//!
//! The verdict comes from two GENUINELY INDEPENDENT legs — support for `φ` and support for
//! its constructed strong negation `¬φ` (`negate_candidate`) — so
//! `InformationState::classify` can land in any Belnap quadrant, `Both` included:
//!
//! * **`φ` leg (proof)** — `with_phi` added NOTHING NEW versus `base` (the candidate is
//!   redundant given the KB ⟹ the KB's canonical model already satisfies `φ` ⟹ `KB ⊨ φ`).
//!   Redundancy compares the derived triple SETS. This leg does NOT depend on the
//!   consistency of `with_phi`, so it stays true even when the `¬φ` leg also fires.
//! * **`¬φ` leg (counterproof)** — `KB ⊨ ¬φ`, decided SOUNDLY AND COMPLETELY over the
//!   supported fragment by the inconsistency of asserting `φ`: `KB ∪ {φ} ⊨ ⊥ ⟺ KB ⊨ ¬φ`
//!   (`kb_entails_negation`). For a ground literal `¬φ` is the direct clash; for a
//!   `∀x. body → head` the negation `∃x. body ∧ ¬head` is EXISTENTIAL and chase-inexpressible
//!   to *lower*, yet is decided WITHOUT lowering it — the clash on the asserted rule
//!   materializes the body-instance witness that forces the head false. The first
//!   [`ContradictionWitness`](crate::result::ContradictionWitness) is surfaced.
//!
//! Because the two legs are independent, a KB that both entails `φ` (redundant) AND refutes
//! it (asserting `φ` clashes) yields the Belnap glut `Both`. That co-support is exactly a
//! within-standpoint contradiction ABOUT `φ`: the base is a glut LOCALIZED to the candidate
//! proposition — it entails `φ` while its disjointness/negative-assertion axioms refute `φ`.
//! A base whose inconsistency is UNRELATED to the candidate (it entails neither `φ` nor a
//! genuine `φ`-refutation) is still a hard error: a conjecture cannot be tested against a
//! world contradictory for foreign reasons (ex falso would make every candidate both
//! entailed and refuted). See the base-consistency guard in `conjecture_test`.
//!
//! A candidate whose lowering produced ZERO evaluable content (fully beyond the
//! Horn-expressible fragment — a disjunctive head, an existential-as-goal, strong
//! negation, an unbounded sequence marker) was never evaluated: the run is
//! `EvaluationStatus::Unsupported` and classifies to
//! `InformationState::NotEvaluated`, with the residue disclosed (never a false proof
//! from a vacuous "added nothing"). A PARTIALLY-supported formula still evaluates its
//! Horn part and discloses the residue in the preservation claim.
//!
//! # The budget seam
//!
//! A ground IRI-or-literal candidate keeps the reasoning contract and rule set fixed,
//! so it is a signed `+1` transaction on the native incremental session. The shared
//! `StepGovernor` charges genuinely new derivations inline at the deterministic sorted
//! commit boundary; cached closure facts and the asserted candidate are not recharged.
//! A cut returns a sound partial closure and `BudgetExhausted`, never a post-hoc fiction.
//!
//! A candidate that changes the rule program (a non-trivial formula) cannot reuse that
//! fixed-contract session.  It remains on the complete native program evaluator and its
//! declared ceiling is applied after closure construction until rule-program sessions are
//! separately incrementalized.  The two cases are explicit in `conjecture_test`; neither
//! is routed through a secondary reasoner.
//!
//! # Lifecycle projection
//!
//! `lifecycle_of` is the single chokepoint mapping the Belnap verdict to the epistemic
//! `ConjectureLifecycleState`: Supported → Corroborated; Opposed | Both →
//! RefutedInStandpoint; Neither-conclusive → Open (discharge Discharged); Undetermined |
//! NotEvaluated | BudgetExhausted → Open (discharge Unknown). `Withdrawn` is an author
//! action, NEVER engine-produced.

use std::collections::BTreeSet;

use gmeow_logic_compile::ir::{Formula, LOGIC_NAMESPACE, LogicProgram, Term};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

use crate::query_ir::Budget;
use crate::reason::InferredAxiom;
use crate::reason::reason_all;
use crate::relational_core::lower_formulas;
use crate::result::{
    CompletenessStatus, ContradictionWitness, EvaluationStatus, InformationState, InputStatus,
    PreservationClaim, ReasoningResult, ResultProvenance,
};

// --------------------------------------------------------------------------- //
// Lifecycle + discharge enums (mirror the `InformationState` enum idiom).
// --------------------------------------------------------------------------- //

/// The epistemic `logic:ConjectureLifecycleState` a completed conjecture test lands in —
/// ORTHOGONAL to the governance candidate lifecycle. The closed four-member value class of
/// `module.ttl`; `Withdrawn` is an author action, never engine-produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConjectureLifecycleState {
    /// Neither corroborated nor refuted (the discharge verdict tells the two origins apart).
    Open,
    /// The formula was Supported (Belnap true) from its standpoint.
    Corroborated,
    /// The formula was Opposed or Both (a concrete counterexample), scoped to the standpoint.
    RefutedInStandpoint,
    /// Author-retired — NEVER produced by [`conjecture_test`].
    Withdrawn,
}

impl ConjectureLifecycleState {
    /// The canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Corroborated => "corroborated",
            Self::RefutedInStandpoint => "refuted-in-standpoint",
            Self::Withdrawn => "withdrawn",
        }
    }
    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Open => "ConjectureOpen",
            Self::Corroborated => "ConjectureCorroborated",
            Self::RefutedInStandpoint => "ConjectureRefutedInStandpoint",
            Self::Withdrawn => "ConjectureWithdrawn",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// Parse the wire value (inverse of [`Self::wire`]).
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "open" => Self::Open,
            "corroborated" => Self::Corroborated,
            "refuted-in-standpoint" => Self::RefutedInStandpoint,
            "withdrawn" => Self::Withdrawn,
            _ => return None,
        })
    }
    /// Parse the local name (inverse of [`Self::local_name`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "ConjectureOpen" => Self::Open,
            "ConjectureCorroborated" => Self::Corroborated,
            "ConjectureRefutedInStandpoint" => Self::RefutedInStandpoint,
            "ConjectureWithdrawn" => Self::Withdrawn,
            _ => return None,
        })
    }
    /// Every variant, for the Rust↔TTL cross-check.
    pub const ALL: &'static [Self] = &[
        Self::Open,
        Self::Corroborated,
        Self::RefutedInStandpoint,
        Self::Withdrawn,
    ];
}

/// The conclusiveness carrier of a conjecture test — the `logic:conjectureDischargeVerdict`
/// facet, reusing the two engine-producible members of the `logic:DischargeVerdict` value
/// class (a conjecture test never emits `ObligationViolated`). A conclusive verdict (a
/// Belnap quadrant or a conclusive independence) is [`Self::Discharged`]; a budget-exhausted
/// or fragment-gap run is [`Self::Unknown`] (carried forward, never "proved absent").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConjectureDischarge {
    /// `logic:ObligationDischarged` — conclusively checked within the fragment.
    Discharged,
    /// `logic:ObligationUnknown` — not-yet-discharged, carried forward.
    Unknown,
}

impl ConjectureDischarge {
    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Discharged => "ObligationDischarged",
            Self::Unknown => "ObligationUnknown",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// Parse the local name (inverse of [`Self::local_name`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "ObligationDischarged" => Self::Discharged,
            "ObligationUnknown" => Self::Unknown,
            _ => return None,
        })
    }
}

// --------------------------------------------------------------------------- //
// The public answer.
// --------------------------------------------------------------------------- //

/// The result of a [`conjecture_test`] run.
#[derive(Debug, Clone, PartialEq)]
pub struct ConjectureAnswer {
    /// The canonical five-field verdict (the classified information state, the evaluation /
    /// completeness axes, the preservation claim disclosing any residue, and provenance).
    pub verdict: ReasoningResult,
    /// The first contradiction witness, present exactly when the conjecture was
    /// [`ConjectureLifecycleState::RefutedInStandpoint`].
    pub witness: Option<ContradictionWitness>,
    /// The epistemic lifecycle state.
    pub lifecycle: ConjectureLifecycleState,
    /// The conclusiveness carrier (Discharged | Unknown).
    pub discharge: ConjectureDischarge,
    /// The isolated scenario world the test ran in.
    pub scenario_world: String,
}

/// The single chokepoint projecting the Belnap information state to the epistemic
/// [`ConjectureLifecycleState`]. See the module doc for the full table.
fn lifecycle_of(
    info: InformationState,
    evaluation: EvaluationStatus,
    discharge: ConjectureDischarge,
) -> ConjectureLifecycleState {
    let state = match info {
        InformationState::Supported => ConjectureLifecycleState::Corroborated,
        InformationState::Opposed | InformationState::Both => {
            ConjectureLifecycleState::RefutedInStandpoint
        }
        InformationState::Neither
        | InformationState::Undetermined
        | InformationState::NotEvaluated => ConjectureLifecycleState::Open,
    };
    // A budget-exhausted run is always inconclusive → Open carrying Unknown; a corroboration
    // or refutation is always a discharged verdict. These invariants are the table.
    debug_assert!(
        evaluation != EvaluationStatus::BudgetExhausted
            || (state == ConjectureLifecycleState::Open
                && discharge == ConjectureDischarge::Unknown),
        "a budget-exhausted conjecture run must be Open/Unknown, got {state:?}/{discharge:?}"
    );
    debug_assert!(
        !matches!(
            state,
            ConjectureLifecycleState::Corroborated | ConjectureLifecycleState::RefutedInStandpoint
        ) || discharge == ConjectureDischarge::Discharged,
        "a corroborated/refuted verdict must be Discharged, got {discharge:?}"
    );
    state
}

/// The discharge verdict a Belnap information state carries: a conclusive quadrant
/// (Supported/Opposed/Both) or a conclusive independence (Neither) is discharged; the two
/// non-verdicts (Undetermined/NotEvaluated) are carried forward as Unknown.
fn discharge_of(info: InformationState) -> ConjectureDischarge {
    match info {
        InformationState::Supported
        | InformationState::Opposed
        | InformationState::Both
        | InformationState::Neither => ConjectureDischarge::Discharged,
        InformationState::Undetermined | InformationState::NotEvaluated => {
            ConjectureDischarge::Unknown
        }
    }
}

// --------------------------------------------------------------------------- //
// The runtime.
// --------------------------------------------------------------------------- //

/// Test the candidate first-order formula `candidate` against `kb` in the ISOLATED
/// scenario world `scenario_world`, scoped to `standpoint` (REQUIRED — Principle 9 refuses
/// a global-false verdict), with `assume_context` ground `(subject, predicate, object)`
/// IRI triples layered onto the scenario EDB, honoring `budget` inline for ground-fact
/// candidates and as the declared post-hoc ceiling for rule-program-changing formulas
/// (see the module doc's budget-seam note).
///
/// `kb` is borrowed `&` and NEVER mutated: the scenario EDB is a fresh dataset built from a
/// copy of `kb` plus the assume-context and (for a ground candidate) `φ`.
///
/// # Errors
///
/// Returns `Err(String)` if `standpoint` is empty, if a ground candidate atom is not
/// ground (a variable/sequence-marker subject or object), if the scenario KB is ALREADY
/// inconsistent before the candidate is asserted (a conjecture cannot be tested against a
/// contradictory world), if the native chase fails, or if the assembled result violates a
/// [`ReasoningResult::validate`] invariant.
pub fn conjecture_test(
    kb: &RdfDataset,
    scenario_world: &str,
    candidate: &Formula,
    standpoint: &str,
    assume_context: &[(String, String, String)],
    budget: &Budget,
) -> gmeow_errors::Result<ConjectureAnswer> {
    if standpoint.trim().is_empty() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: "conjecture_test requires a non-empty standpoint (Principle 9: a conjecture \
                     verdict is always standpoint-scoped, never global)"
                .to_owned(),
        }));
    }

    // (1) The scenario EDB = KB ∪ assume_context. The input KB is copied in, never mutated.
    let base_edb = build_scenario_edb(kb, scenario_world, assume_context, None)?;

    // (3a) `base` — the closure with NO candidate, carrying the DL consistency verdict. The
    //      up-front consistency check is deferred until the two legs are computed: a base that
    //      is inconsistent SPECIFICALLY about the candidate (it entails both `φ` and `¬φ`) is a
    //      genuine, testable within-standpoint glut, whereas a base inconsistent for FOREIGN
    //      reasons is a hard error (see the guard below).
    let base = reason_all(&base_edb)?;

    // (2) Route the candidate: a trivially-Horn ground atom is a fact in the EDB; every
    //     other formula is a program `P_phi` reason_program lowers and evaluates.
    let (with_phi, semantics_available, inline_budget) = match as_ground_fact(candidate)? {
        Some((subject, predicate, object)) => {
            // The "asserted φ": a ground fact in the scenario world.
            let phi_edb = build_scenario_edb(
                kb,
                scenario_world,
                assume_context,
                Some((subject.clone(), predicate.clone(), object.clone())),
            )?;
            let adjusted = crate::reason::reason_ground_fact_insert_incremental(
                crate::reason::GroundFactIncrementalRequest {
                    base_edb: &base_edb,
                    with_candidate_edb: &phi_edb,
                    base: &base,
                    scenario_world,
                    subject: &subject,
                    predicate: &predicate,
                    object: &object,
                    max_steps: budget.max_steps,
                },
            )?;
            (
                adjusted.result,
                true,
                (adjusted.status, adjusted.consumed_steps),
            )
        }
        None => {
            // A full candidate formula. `with_formulas` hard-fails on a trivially-Horn leaf,
            // so only a non-trivial formula reaches here.
            let p_phi = LogicProgram::new(vec![], vec![], vec![], None)
                .with_formulas(vec![candidate.clone()]);
            // The candidate contributed EVALUABLE content iff its lowering produced any Horn
            // rule or n-ary head rule. A fully beyond-fragment candidate (empty lowering) was
            // never evaluated, so its "added nothing" is vacuous, not a proof.
            let lowering = lower_formulas(&p_phi);
            let evaluable = !lowering.rules.is_empty() || !lowering.nary_head_rules.is_empty();
            // The candidate program is evaluated through the GOVERNED forward chase
            // ([`reason_program_budgeted`]): `budget.max_steps` cuts the semi-naive fixpoint
            // mid-flight and reports a real `BudgetStatus` + committed step count, so a
            // step-exhausted rule-program candidate is chase-bounded exactly like the ground
            // path — never a full run relabeled after the fact.
            let (result, status, consumed) =
                crate::reason::reason_program_budgeted(&p_phi, &base_edb, budget.max_steps)?;
            (result, evaluable, (status, consumed))
        }
    };

    let derived_closure_size = with_phi.inferred().iter().filter(|a| !a.is_edb).count() as u64;
    let answer_ceiling_tripped = budget
        .max_answers
        .is_some_and(|n| derived_closure_size > n as u64);
    // The step ceiling is now decided SOLELY by the forward-chase governor's status on both
    // routes (ground-fact incremental session and rule-program forward chase): the chase was
    // genuinely cut mid-flight, never inferred post-hoc from `derived_closure_size` after a
    // full run.
    let step_ceiling_tripped = inline_budget.0 == crate::seam::BudgetStatus::Exhausted;
    let budget_tripped = answer_ceiling_tripped || step_ceiling_tripped;

    // (4) The Belnap inputs — two INDEPENDENT legs (φ and ¬φ).
    // ¬φ leg (counterproof): KB ⊨ ¬φ, decided soundly & completely by the inconsistency of
    // asserting φ (KB ∪ {φ} ⊨ ⊥ ⟺ KB ⊨ ¬φ). `negate_candidate` names the leg being decided.
    let neg_phi = negate_candidate(candidate);
    let has_counterproof = kb_entails_negation(&neg_phi, &with_phi);
    // φ leg (proof): φ is redundant (KB ⊨ φ) iff its closure added NO new derived triple
    // versus `base` — a proof whenever φ contributed evaluable content. This leg is
    // DELIBERATELY independent of `with_phi`'s consistency: over a consistent base a redundant
    // φ can never clash (so proof and counterproof stay mutually exclusive there), but over a
    // base that ALREADY entails φ while refuting it, both legs fire and the verdict is `Both`.
    let redundant = triple_set(with_phi.inferred()) == triple_set(base.inferred());
    let has_proof = semantics_available && redundant;
    let conclusive = with_phi.is_conclusive() && !budget_tripped;

    // (4a) The base-consistency guard, now leg-aware. A base inconsistent SPECIFICALLY about
    //      the candidate (it entails φ AND refutes φ) is a genuine within-standpoint glut and
    //      is testable → falls through to a `Both` verdict. A base inconsistent for reasons
    //      UNRELATED to the candidate (it does not entail φ, or does not genuinely refute it)
    //      cannot host a meaningful test — ex falso would make every proposition both entailed
    //      and refuted — so it stays a hard error.
    if !base.is_consistent() && !(has_proof && has_counterproof) {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: format!(
                "conjecture_test: the scenario KB in world <{scenario_world}> is ALREADY \
                 inconsistent for a reason UNRELATED to the candidate (it neither entails the \
                 candidate nor genuinely refutes it), so the candidate cannot be tested against \
                 it — ex falso would make every proposition both entailed and refuted. \
                 witnesses: {:?}",
                base.provenance.contradiction_witnesses
            ),
        }));
    }

    // (5) Classify, then apply the budget ceiling and the beyond-fragment status.
    let information = if budget_tripped {
        // A truncated run is inconclusive: never Supported/Opposed/Both/Neither.
        InformationState::Undetermined
    } else {
        InformationState::classify(has_proof, has_counterproof, conclusive, semantics_available)
    };

    let evaluation = if budget_tripped {
        EvaluationStatus::BudgetExhausted
    } else if !semantics_available {
        // Fully beyond-fragment: the candidate was never evaluated.
        EvaluationStatus::Unsupported
    } else {
        EvaluationStatus::Completed
    };

    let completeness = if budget_tripped || !semantics_available {
        CompletenessStatus::Incomplete
    } else {
        with_phi.completeness
    };

    // The preservation claim: a fully beyond-fragment candidate surfaces as the
    // `{unsupported}` floor WITH disclosure; every other run carries `with_phi`'s claim
    // (`{exact}`, or `{sound-under}` disclosing a partially-supported formula's residue).
    let preservation = if !semantics_available {
        PreservationClaim::unsupported_with(
            with_phi.preservation.unsupported_constructs.iter().cloned(),
        )
    } else {
        with_phi.preservation.clone()
    };

    // The witness is present exactly for a refutation (Opposed | Both).
    let witness = if matches!(
        information,
        InformationState::Opposed | InformationState::Both
    ) {
        with_phi.provenance.contradiction_witnesses.first().cloned()
    } else {
        None
    };

    let discharge = discharge_of(information);
    let lifecycle = lifecycle_of(information, evaluation, discharge);

    // (5) Assemble the returned result. Provenance carries the witnesses and the standpoint
    //     scope; the payload is the with-φ closure for inspection.
    let mut provenance =
        ResultProvenance::native(with_phi.provenance.contract_hash.clone(), scenario_world);
    provenance.context.standpoint = Some(standpoint.to_owned());
    provenance.consumed_budget.allowance =
        budget.max_steps.or(budget.max_answers.map(|n| n as u64));
    // The consumed budget is the forward-chase governor's real committed-step count on both
    // routes — never the post-hoc `derived_closure_size` fiction.
    provenance.consumed_budget.consumed = inline_budget.1;
    if budget_tripped {
        provenance.consumed_budget.limit = Some(crate::result::BudgetLimit::Inference);
    }
    provenance.contradiction_witnesses = with_phi.provenance.contradiction_witnesses.clone();
    provenance.projection_class = preservation.clone();

    let verdict = ReasoningResult::new(
        InputStatus::Valid,
        evaluation,
        completeness,
        preservation,
        information,
        provenance,
        with_phi.payload.clone(),
    );
    // Hard-fail on any invariant violation (a `Both` without a witness would be one), rather
    // than lean on the debug-only assertion in `ReasoningResult::new`.
    verdict.validate()?;

    Ok(ConjectureAnswer {
        verdict,
        witness,
        lifecycle,
        discharge,
        scenario_world: scenario_world.to_owned(),
    })
}

/// Build the scenario EDB `KB ∪ assume_context` (∪ `φ` when `phi` is `Some`) as a FRESH
/// [`RdfDataset`]: the input `kb` is copied in via [`RdfDatasetBuilder::push_dataset`] and
/// never mutated. The assume-context facts and the ground `φ` fact are asserted in
/// `scenario_world`, so the world-scoped DL calculus joins them with the KB facts a caller
/// placed there.
fn build_scenario_edb(
    kb: &RdfDataset,
    scenario_world: &str,
    assume_context: &[(String, String, String)],
    phi: Option<(String, String, RdfTerm)>,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(kb);
    for (s, p, o) in assume_context {
        let quad = RdfQuad::new(RdfTerm::iri(s.clone()), p.clone(), RdfTerm::iri(o.clone()))
            .in_graph(RdfTerm::iri(scenario_world.to_owned()));
        builder.push_owned_quad(&quad);
    }
    if let Some((s, p, object)) = phi {
        let quad = RdfQuad::new(RdfTerm::iri(s), p, object)
            .in_graph(RdfTerm::iri(scenario_world.to_owned()));
        builder.push_owned_quad(&quad);
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: e.to_string(),
        })
    })
}

/// Route the candidate: `Some((subject, predicate, object))` when it is a trivially-Horn
/// GROUND fact (a binary [`Formula::Atom`] over an IRI relation with a ground subject and a
/// ground object) that must be asserted as an EDB fact rather than entered into
/// [`LogicProgram::formulas`] (which hard-fails on a trivially-Horn leaf); `None` when it is
/// a full formula the program path evaluates.
///
/// # Errors
///
/// Returns `Err` when the candidate has the trivially-Horn binary shape but is NOT ground
/// (a variable / sequence-marker subject or object), which cannot be asserted as a fact.
#[allow(clippy::type_complexity)]
fn as_ground_fact(candidate: &Formula) -> gmeow_errors::Result<Option<(String, String, RdfTerm)>> {
    let Formula::Atom { relation, args } = candidate else {
        return Ok(None);
    };
    let Term::Iri(predicate) = relation else {
        return Ok(None);
    };
    // Only a binary atom with no sequence marker is trivially Horn; anything else is a
    // genuine formula (unary or n-ary reified, quantified, connective) → the program path.
    if args.len() != 2 || args.iter().any(|t| matches!(t, Term::SequenceMarker(_))) {
        return Ok(None);
    }
    let subject = match &args[0] {
        Term::Iri(s) => s.clone(),
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!(
                    "conjecture_test: a ground candidate atom's subject must be an IRI, got \
                     {other:?} — a non-ground binary atom cannot be asserted as a fact"
                ),
            }));
        }
    };
    let object = match &args[1] {
        Term::Iri(o) => RdfTerm::iri(o.clone()),
        Term::Literal { lexical, datatype } => {
            let literal = match datatype {
                Some(dt) => RdfLiteral::typed(lexical.clone(), dt.clone()),
                None => RdfLiteral::simple(lexical.clone()),
            };
            RdfTerm::literal(literal)
        }
        other => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!(
                    "conjecture_test: a ground candidate atom's object must be an IRI or literal, \
                     got {other:?} — a non-ground binary atom cannot be asserted as a fact"
                ),
            }));
        }
    };
    Ok(Some((subject, predicate.clone(), object)))
}

/// Construct the strong (explicit) negation `¬φ` of the candidate — the second leg of the
/// symmetric test. This is `logic:` strong negation ([`Formula::Not`]), DISTINCT from
/// negation-as-failure: it names the proposition the counterproof leg decides. The candidate
/// AST is cloned untouched, so `¬φ`'s content identity is the alpha-normalized negation of
/// `φ`'s.
fn negate_candidate(candidate: &Formula) -> Formula {
    Formula::Not(Box::new(candidate.clone()))
}

/// Decide the `¬φ` leg: does the KB entail `¬φ`? Over the supported fragment (ground literals
/// and the `∀`-Horn implication) this is decided SOUNDLY AND COMPLETELY by the inconsistency
/// of asserting `φ` — `KB ∪ {φ} ⊨ ⊥ ⟺ KB ⊨ ¬φ` — so the existential negation of a `∀`-Horn is
/// witnessed by the clash on the asserted rule WITHOUT lowering the (chase-inexpressible)
/// `∃x. body ∧ ¬head`. `neg_phi` is the constructed negation this leg tests; `with_phi` is its
/// sound evaluator (the closure with `φ` asserted).
fn kb_entails_negation(neg_phi: &Formula, with_phi: &ReasoningResult) -> bool {
    debug_assert!(
        matches!(neg_phi, Formula::Not(_)),
        "the ¬φ leg must test a strong negation, got {neg_phi:?}"
    );
    // The clash on the asserted φ (already materialized in `with_phi`) IS the refutation
    // witness for `neg_phi`; an inconsistent `with_phi` means the KB entails ¬φ.
    !with_phi.is_consistent()
}

/// The closure's derived+asserted triple identities `(subject, predicate, object, world)` —
/// the IS-there-a-new-fact projection redundancy compares. Keyed on the triple SHAPE only
/// (not the `is_edb` flag / firing rule / premises), so a fact that was DERIVED in `base`
/// and ASSERTED in `with_phi` counts as the same triple (no false "new fact").
fn triple_set(inferred: &[InferredAxiom]) -> BTreeSet<(String, String, String, String)> {
    inferred
        .iter()
        .map(|a| {
            (
                a.subject.clone(),
                a.predicate.clone(),
                a.object.clone(),
                a.world.clone(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
