// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The typed `logic:ReasoningResult` (ME2): the single shared result model
//! every reasoning surface produces and every consumer reads.
//!
//! This module is **pure data** — no I/O, no graph parsing. It is the Rust
//! authority (Principle 17); `slices/grounding/logic/module.ttl` carries the lossy
//! ontology projection of these types.
//!
//! # Two orthogonal status axes, five compositional fields
//!
//! The canonical specification is `slices/grounding/logic/design/LOGIC-SEMANTICS.md`
//! §"The reasoning result" (authoritative over the issue body). A result carries
//! **five orthogonal fields**, each ranging over its own values; several can hold
//! at once:
//!
//! 1. [`InputStatus`] — was the request well-formed?
//! 2. [`EvaluationStatus`] — what the engine was *able* to do (the computation axis).
//! 3. [`CompletenessStatus`] — relative to what is the answer complete?
//! 4. [`PreservationClaim`] — what lowering did (a *set* of polarities + unsupported constructs).
//! 5. [`InformationState`] — the four-valued Belnap verdict plus two explicit non-results
//!    (the information axis).
//!
//! plus the [`ResultProvenance`] bundle (contract hash, query/conclusion,
//! proof/counterproof, world/standpoint/time/path, engine+version, consumed
//! budget, fragment certification, projection class, contradiction witnesses,
//! assumptions).
//!
//! # The conclusiveness invariant
//!
//! [`InformationState::Neither`] is legal **only** after a *conclusive*
//! evaluation (SEMANTICS:294-318). `neither` means "the engine looked,
//! conclusively, and found no proof and no counterproof"; [`InformationState::Undetermined`]
//! means "the engine has not (yet) reached a verdict"; [`InformationState::NotEvaluated`]
//! means "the engine could not look". The three are never interchangeable. The
//! invariant is enforced at construction by [`ReasoningResult::validate`].

use std::collections::BTreeSet;
use std::fmt;

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, PreservationKind};

use crate::probabilistic::ProbBinding;
use crate::query_ir::Binding;
use crate::reason::el::InferredAxiom;
use crate::reason::{DlVerdict, InconsistencyWitness};

/// Wrap a reasoning-result condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn result_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Result { detail })
}

// --------------------------------------------------------------------------- //
// Field enums — single source of truth.
//
// Each enum exposes two string surfaces:
//   * `wire()`       — the SEMANTICS canonical hyphenated-lowercase value, used in
//                      every JSON/text projection (PyO3, conformance, certs).
//   * `local_name()` — the `module.ttl` named-individual local name (PascalCase),
//                      tied 1:1 by the Rust↔TTL cross-check.
// --------------------------------------------------------------------------- //

/// `input` — was the request well-formed? (SEMANTICS:246-251)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputStatus {
    /// The request and its sources parsed and type-checked; reasoning was attempted.
    Valid,
    /// The request or its sources were ill-formed; no reasoning was attempted.
    Invalid,
}

impl InputStatus {
    /// The SEMANTICS canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
        }
    }
    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Valid => "InputValid",
            Self::Invalid => "InputInvalid",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// Parse the wire value (inverse of [`Self::wire`]).
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "valid" => Self::Valid,
            "invalid" => Self::Invalid,
            _ => return None,
        })
    }
    /// Parse the local name (inverse of [`Self::local_name`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "InputValid" => Self::Valid,
            "InputInvalid" => Self::Invalid,
            _ => return None,
        })
    }
    /// Every variant, for the Rust↔TTL cross-check.
    pub const ALL: &'static [Self] = &[Self::Valid, Self::Invalid];
}

impl fmt::Display for InputStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

/// `evaluation` — what the engine was able to do (the computation axis). (SEMANTICS:252-260)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvaluationStatus {
    /// The engine ran to its natural end on this request.
    Completed,
    /// A declared resource budget or bound was hit before the engine finished.
    BudgetExhausted,
    /// The engine has no defined procedure for this request (unsupported contract).
    Unsupported,
}

impl EvaluationStatus {
    /// The SEMANTICS canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::BudgetExhausted => "budget-exhausted",
            Self::Unsupported => "unsupported",
        }
    }
    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Completed => "EvaluationCompleted",
            Self::BudgetExhausted => "BudgetExhausted",
            Self::Unsupported => "EvaluationUnsupported",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// Parse the wire value (inverse of [`Self::wire`]).
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "completed" => Self::Completed,
            "budget-exhausted" => Self::BudgetExhausted,
            "unsupported" => Self::Unsupported,
            _ => return None,
        })
    }
    /// Parse the local name (inverse of [`Self::local_name`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "EvaluationCompleted" => Self::Completed,
            "BudgetExhausted" => Self::BudgetExhausted,
            "EvaluationUnsupported" => Self::Unsupported,
            _ => return None,
        })
    }
    /// Every variant, for the Rust↔TTL cross-check.
    pub const ALL: &'static [Self] = &[Self::Completed, Self::BudgetExhausted, Self::Unsupported];
}

impl fmt::Display for EvaluationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

/// `completeness` — relative to what is the answer complete? (SEMANTICS:261-266)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompletenessStatus {
    /// Complete for the certified fragment the contract pins.
    CompleteForFragment,
    /// The search did not exhaust the space (budget/cap/depth).
    Incomplete,
    /// Completeness is not a defined question for this request (e.g. a revision tie).
    Unknown,
}

impl CompletenessStatus {
    /// The SEMANTICS canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::CompleteForFragment => "complete-for-fragment",
            Self::Incomplete => "incomplete",
            Self::Unknown => "unknown",
        }
    }
    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::CompleteForFragment => "CompleteForFragment",
            Self::Incomplete => "Incomplete",
            Self::Unknown => "CompletenessUnknown",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// Parse the wire value (inverse of [`Self::wire`]).
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "complete-for-fragment" => Self::CompleteForFragment,
            "incomplete" => Self::Incomplete,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
    /// Parse the local name (inverse of [`Self::local_name`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "CompleteForFragment" => Self::CompleteForFragment,
            "Incomplete" => Self::Incomplete,
            "CompletenessUnknown" => Self::Unknown,
            _ => return None,
        })
    }
    /// Every variant, for the Rust↔TTL cross-check.
    pub const ALL: &'static [Self] = &[Self::CompleteForFragment, Self::Incomplete, Self::Unknown];
}

impl fmt::Display for CompletenessStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

/// `information` — the four-valued Belnap verdict about the queried proposition,
/// plus two explicit non-results. (SEMANTICS:287-322)
///
/// The Belnap four ([`Self::Supported`], [`Self::Opposed`], [`Self::Both`],
/// [`Self::Neither`]) answer "is there a proof / a counterproof?". The two
/// non-results are **never** interchangeable with `Neither`:
/// [`Self::Undetermined`] = the engine did not reach a verdict;
/// [`Self::NotEvaluated`] = the engine could not look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InformationState {
    /// A proof exists and no counterproof.
    Supported,
    /// A counterproof exists and no proof.
    Opposed,
    /// A proof *and* a counterproof exist (a witnessed contradiction in one context).
    Both,
    /// Neither proof nor counterproof, established by a **conclusive** evaluation.
    Neither,
    /// The evaluation did not reach a conclusive verdict (budget, tie, no discretization).
    Undetermined,
    /// No information semantics were available (unsupported contract, missing model).
    NotEvaluated,
}

impl InformationState {
    /// The SEMANTICS canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Opposed => "opposed",
            Self::Both => "both",
            Self::Neither => "neither",
            Self::Undetermined => "undetermined",
            Self::NotEvaluated => "not-evaluated",
        }
    }
    /// The `module.ttl` named-individual local name.
    pub fn local_name(self) -> &'static str {
        match self {
            Self::Supported => "InfoSupported",
            Self::Opposed => "InfoOpposed",
            Self::Both => "InfoBoth",
            Self::Neither => "InfoNeither",
            Self::Undetermined => "InfoUndetermined",
            Self::NotEvaluated => "InfoNotEvaluated",
        }
    }
    /// The full IRI of the `module.ttl` individual.
    pub fn iri(self) -> String {
        format!("{LOGIC_NAMESPACE}{}", self.local_name())
    }
    /// Parse the wire value (inverse of [`Self::wire`]).
    pub fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "supported" => Self::Supported,
            "opposed" => Self::Opposed,
            "both" => Self::Both,
            "neither" => Self::Neither,
            "undetermined" => Self::Undetermined,
            "not-evaluated" => Self::NotEvaluated,
            _ => return None,
        })
    }
    /// Parse the local name (inverse of [`Self::local_name`]).
    pub fn from_local(name: &str) -> Option<Self> {
        Some(match name {
            "InfoSupported" => Self::Supported,
            "InfoOpposed" => Self::Opposed,
            "InfoBoth" => Self::Both,
            "InfoNeither" => Self::Neither,
            "InfoUndetermined" => Self::Undetermined,
            "InfoNotEvaluated" => Self::NotEvaluated,
            _ => return None,
        })
    }
    /// Every variant, for the Rust↔TTL cross-check.
    pub const ALL: &'static [Self] = &[
        Self::Supported,
        Self::Opposed,
        Self::Both,
        Self::Neither,
        Self::Undetermined,
        Self::NotEvaluated,
    ];

    /// Classify the information state from witness presence under the run's
    /// conclusiveness — the single chokepoint that enforces the conclusiveness
    /// invariant (SEMANTICS:294-318).
    ///
    /// * `semantics_available` — `false` ⇒ [`Self::NotEvaluated`] (the engine could not look).
    /// * `(proof, counterproof)` ⇒ the Belnap quadrant, except that the empty
    ///   quadrant resolves to [`Self::Neither`] only when `conclusive`, otherwise
    ///   [`Self::Undetermined`].
    ///
    /// `conclusive` is `evaluation == Completed || completeness == CompleteForFragment`
    /// (see [`ReasoningResult::is_conclusive`]).
    pub fn classify(
        has_proof: bool,
        has_counterproof: bool,
        conclusive: bool,
        semantics_available: bool,
    ) -> Self {
        if !semantics_available {
            return Self::NotEvaluated;
        }
        match (has_proof, has_counterproof) {
            (true, true) => Self::Both,
            (true, false) => Self::Supported,
            (false, true) => Self::Opposed,
            (false, false) => {
                if conclusive {
                    Self::Neither
                } else {
                    Self::Undetermined
                }
            }
        }
    }
}

impl fmt::Display for InformationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

// --------------------------------------------------------------------------- //
// Preservation claim — a structured field, not a single choice.
// --------------------------------------------------------------------------- //

/// The `preservation` field: the set of answer-preservation polarities that
/// co-hold for this result, plus the set of constructs the lowering could not
/// carry at all (SEMANTICS:268-285).
///
/// The polarity set draws from [`PreservationKind`] but **excludes**
/// [`PreservationKind::ValidationOnly`]: that individual describes a *lowering's
/// purpose*, not an answer-preservation polarity, so it is not a legal member of
/// a result's polarity set ([`Self::insert`] / [`Self::validate`] reject it).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreservationClaim {
    /// Co-holding answer-preservation polarities (the five doc polarities).
    pub polarities: BTreeSet<PreservationKind>,
    /// Constructs the lowering could not carry at all (bare/qualified IRIs).
    pub unsupported_constructs: BTreeSet<String>,
}

impl PreservationClaim {
    /// `{exact}` with no unsupported constructs: passed through no lowering
    /// (SEMANTICS:283).
    pub fn exact() -> Self {
        let mut polarities = BTreeSet::new();
        polarities.insert(PreservationKind::Exact);
        Self {
            polarities,
            unsupported_constructs: BTreeSet::new(),
        }
    }

    /// Build a claim from the set of constructs a lowering could not carry — the
    /// single polarity-derivation rule the `reason`, `query`, and `materialize`
    /// lanes share so they cannot diverge: `{exact}` with an empty set when nothing
    /// was dropped, else `{sound-under}` carrying the dropped constructs (a lane
    /// that drops a derivation it cannot evaluate produces a sound *under*-
    /// approximation of the full answer). This is the one place the
    /// unsupported-set → polarity mapping lives.
    pub fn for_unsupported<I, S>(constructs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let unsupported_constructs: BTreeSet<String> =
            constructs.into_iter().map(Into::into).collect();
        let mut polarities = BTreeSet::new();
        polarities.insert(if unsupported_constructs.is_empty() {
            PreservationKind::Exact
        } else {
            PreservationKind::SoundUnder
        });
        Self {
            polarities,
            unsupported_constructs,
        }
    }

    /// `{unsupported}` — the legalization floor: the program was refused as
    /// unsupported and never evaluated, so none of the answer-preservation polarities
    /// (`exact` / `sound-under` / `complete-over`) applies. A refused case carries
    /// this instead of a false `{exact}`, so a consumer reading the claim sees the
    /// case was not evaluated rather than assuming its (empty) answer was faithful.
    pub fn unsupported() -> Self {
        let mut polarities = BTreeSet::new();
        polarities.insert(PreservationKind::Unsupported);
        Self {
            polarities,
            unsupported_constructs: BTreeSet::new(),
        }
    }

    /// `{unsupported}` carrying the `constructs` the lowering refused — the
    /// legalization floor with disclosure. Like [`Self::unsupported`] (the program
    /// was refused and never evaluated, so no answer-preservation polarity applies),
    /// but the refused constructs are NAMED in `unsupported_constructs` so a consumer
    /// sees exactly what could not be carried (never silently truncated). The single
    /// place the unsupported-with-disclosure shape is built (One-Path), used by the
    /// DAG-workflow certifier for the offending cycle members.
    pub fn unsupported_with<I, S>(constructs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut polarities = BTreeSet::new();
        polarities.insert(PreservationKind::Unsupported);
        Self {
            polarities,
            unsupported_constructs: constructs.into_iter().map(Into::into).collect(),
        }
    }

    /// Insert a polarity, rejecting [`PreservationKind::ValidationOnly`] (which is
    /// not an answer-preservation polarity).
    ///
    /// # Errors
    /// Returns `Err` if `kind` is [`PreservationKind::ValidationOnly`].
    pub fn insert(&mut self, kind: PreservationKind) -> gmeow_errors::Result<()> {
        if kind == PreservationKind::ValidationOnly {
            return Err(result_err(
                "PreservationClaim.polarities must not contain ValidationOnly: it names a \
                 lowering's purpose, not an answer-preservation polarity"
                    .to_owned(),
            ));
        }
        self.polarities.insert(kind);
        Ok(())
    }

    /// The issue's `loss-affected` reading — a **derived diagnostic**, never a
    /// stored field (SEMANTICS:280-285): some unsupported construct is relevant to
    /// the query.
    pub fn is_loss_affected(&self, query_constructs: &BTreeSet<String>) -> bool {
        !self.unsupported_constructs.is_disjoint(query_constructs)
    }

    /// Validate the polarity-set invariant (no `ValidationOnly`).
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        if self.polarities.contains(&PreservationKind::ValidationOnly) {
            return Err(result_err(
                "PreservationClaim.polarities must not contain ValidationOnly".to_owned(),
            ));
        }
        Ok(())
    }
}

// --------------------------------------------------------------------------- //
// Provenance bundle.
// --------------------------------------------------------------------------- //

/// Which declared budget or bound a [`BudgetExhausted`](EvaluationStatus::BudgetExhausted)
/// / [`Incomplete`](CompletenessStatus::Incomplete) run tripped.
///
/// This discriminator keeps the unified model **lossless** with respect to the
/// historical per-engine status strings: answer-cap (`partial`), inference-budget
/// (`exhausted`), and nested-depth (`incomplete`) are all budget exhaustion of
/// *different* budgets, indistinguishable from `(evaluation, completeness)` alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BudgetLimit {
    /// The answer cap was hit during resolution (historical `partial`).
    Answers,
    /// The inference budget was exhausted (historical `exhausted`).
    Inference,
    /// The nested-construction depth budget was exhausted (historical `incomplete`).
    Depth,
}

impl BudgetLimit {
    /// The canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Answers => "answers",
            Self::Inference => "inference",
            Self::Depth => "depth",
        }
    }
}

impl fmt::Display for BudgetLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

/// Budget consumed by a run, against the contract's declared allowance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BudgetUsage {
    /// Inference steps / units consumed.
    pub consumed: u64,
    /// The declared allowance, when the contract pinned one.
    pub allowance: Option<u64>,
    /// Which budget tripped, when the run did not complete naturally.
    pub limit: Option<BudgetLimit>,
}

/// A content-addressed handle into the [`crate::explain`] proof tree: a proof or
/// counterproof, identified by its derivation IRI and the set of IRIs it cites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationRef {
    /// The `derivation_id` of the explained conclusion.
    pub derivation_id: String,
    /// The complete cited-IRI set of the derivation tree (sorted).
    pub cited_iris: BTreeSet<String>,
}

impl DerivationRef {
    /// Build a [`DerivationRef`] from a reconstructed [`crate::explain::Explanation`]
    /// proof tree. A *proof* references the conclusion quad's explanation; a
    /// *counterproof* references the explanation of the conclusion's explicit
    /// negation (the FDE/paraconsistent `opposed` evidence). The same machinery
    /// builds both.
    pub fn from_explanation(explanation: &crate::explain::Explanation) -> Self {
        Self {
            derivation_id: explanation.target_derivation_id.clone(),
            cited_iris: explanation.cited_iris.clone(),
        }
    }
}

/// The context an answer is true in: a world (always present — "an answer is
/// always somewhere", SEMANTICS:362) and the optional standpoint/time/path axes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResultContext {
    /// The world (named-graph) IRI the answer holds in.
    pub world: String,
    /// The standpoint IRI, when scoped.
    pub standpoint: Option<String>,
    /// The time expression (ISO 8601 / EDTF), when scoped.
    pub time: Option<String>,
    /// The predicate-path IRI, when the answer rode a named path.
    pub path: Option<String>,
}

/// The engine identity that produced a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineId {
    /// Engine name (e.g. `gmeow-logic`).
    pub name: String,
    /// Engine version string (e.g. [`crate::counterfactual::SOLVER_VERSION`]).
    pub version: String,
}

impl EngineId {
    /// The native engine identity, seeded from [`crate::counterfactual::SOLVER_VERSION`].
    pub fn native() -> Self {
        let version = crate::counterfactual::SOLVER_VERSION.to_owned();
        Self {
            name: "gmeow-logic".to_owned(),
            version,
        }
    }
}

/// A within-world contradiction witness justifying [`InformationState::Both`].
///
/// Adapted from [`crate::reason::InconsistencyWitness`]; carries the individual
/// forced into a clash, the world, and the premise triples.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContradictionWitness {
    /// The individual forced into `owl:Nothing` (the clash subject).
    pub individual: String,
    /// The world the contradiction is local to.
    pub world: String,
    /// The premise triples `(subject, predicate, object)` that witness the clash.
    pub premises: Vec<(String, String, String)>,
}

impl From<&InconsistencyWitness> for ContradictionWitness {
    fn from(w: &InconsistencyWitness) -> Self {
        Self {
            individual: w.individual.clone(),
            world: w.world.clone(),
            premises: w.premises.clone(),
        }
    }
}

/// A declared closure / identity / revision / witness-policy assumption the
/// result rests on (SEMANTICS:368-369).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Assumption {
    /// Closed-world closure was applied (NAF over the declared predicates).
    ClosedWorld,
    /// Open-world closure (no NAF).
    OpenWorld,
    /// The unique-name assumption was in force.
    UniqueName,
    /// Entrenchment-ordered belief revision was used.
    EntrenchmentRevision,
    /// Existential obligations were discharged by content-addressed Skolem witnesses.
    SkolemWitness,
}

impl Assumption {
    /// The canonical wire value.
    pub fn wire(self) -> &'static str {
        match self {
            Self::ClosedWorld => "closed-world",
            Self::OpenWorld => "open-world",
            Self::UniqueName => "unique-name",
            Self::EntrenchmentRevision => "entrenchment-revision",
            Self::SkolemWitness => "skolem-witness",
        }
    }
}

impl fmt::Display for Assumption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.wire())
    }
}

/// The full provenance bundle a result carries (SEMANTICS:357-369).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultProvenance {
    /// The reasoning-contract identity (content hash) this result was produced under.
    pub contract_hash: String,
    /// The canonical query text (the question asked).
    pub query: String,
    /// The canonical conclusion text (the proposition the verdict is about).
    pub conclusion: String,
    /// The proof of the conclusion, when one exists.
    pub proof: Option<DerivationRef>,
    /// The counterproof of the conclusion, when one exists.
    pub counterproof: Option<DerivationRef>,
    /// The world / standpoint / time / path the answer holds in.
    pub context: ResultContext,
    /// The engine identity + version.
    pub engine: EngineId,
    /// Budget consumed against the contract's allowance.
    pub consumed_budget: BudgetUsage,
    /// The certified fragment reference backing a `complete-for-fragment` claim.
    pub certified_fragment: Option<String>,
    /// The projection-preservation class (mirrors [`ReasoningResult::preservation`]).
    pub projection_class: PreservationClaim,
    /// The within-world contradiction witnesses justifying [`InformationState::Both`] (sorted).
    pub contradiction_witnesses: Vec<ContradictionWitness>,
    /// The declared closure/identity/revision/witness-policy assumptions (sorted).
    pub assumptions: BTreeSet<Assumption>,
}

impl ResultProvenance {
    /// A minimal provenance bundle for a result in `world`, produced by the native
    /// engine under `contract_hash`, with no proof/counterproof/witnesses yet.
    pub fn native(contract_hash: impl Into<String>, world: impl Into<String>) -> Self {
        Self {
            contract_hash: contract_hash.into(),
            query: String::new(),
            conclusion: String::new(),
            proof: None,
            counterproof: None,
            context: ResultContext {
                world: world.into(),
                ..ResultContext::default()
            },
            engine: EngineId::native(),
            consumed_budget: BudgetUsage::default(),
            certified_fragment: None,
            projection_class: PreservationClaim::default(),
            contradiction_witnesses: Vec::new(),
            assumptions: BTreeSet::new(),
        }
    }
}

// --------------------------------------------------------------------------- //
// Payload — surface-specific answer shapes under one type.
// --------------------------------------------------------------------------- //

/// The surface-specific answer a result carries, so a single [`ReasoningResult`]
/// type serves every reasoning surface (SEMANTICS:235-238) without one surface's
/// fields polluting another.
#[derive(Debug, Clone, PartialEq)]
pub enum ResultPayload {
    /// The asserted + derived closure (the `reason` / DL surface).
    Inferred(Vec<InferredAxiom>),
    /// Goal-variable bindings (the resolution / counterfactual surface).
    Bindings(Vec<Binding>),
    /// Bindings with marginal probabilities (the probabilistic surface).
    Marginals(Vec<ProbBinding>),
    /// No answer payload (a pure verdict, an invalid request).
    Empty,
}

// --------------------------------------------------------------------------- //
// The result type.
// --------------------------------------------------------------------------- //

/// The typed `logic:ReasoningResult` — the single shared result model.
#[derive(Debug, Clone, PartialEq)]
pub struct ReasoningResult {
    /// Was the request well-formed?
    pub input: InputStatus,
    /// What the engine was able to do (computation axis).
    pub evaluation: EvaluationStatus,
    /// Relative to what is the answer complete?
    pub completeness: CompletenessStatus,
    /// What lowering did (a set of polarities + unsupported constructs).
    pub preservation: PreservationClaim,
    /// The four-valued verdict + the two explicit non-results (information axis).
    pub information: InformationState,
    /// The full provenance bundle.
    pub provenance: ResultProvenance,
    /// The surface-specific answer payload.
    pub payload: ResultPayload,
    /// The declared row-schema facet: the typed [`ResultShape`](crate::result_shape::ResultShape)
    /// the result's bindings are contracted to. `None` for surfaces with no row
    /// schema (e.g. a pure DL-consistency verdict). This is the DECLARED contract a
    /// caller attaches via [`Self::with_row_schema`] — never derived from the
    /// result's own bindings (that would be a tautology) — against which a consumer
    /// validates the bindings.
    pub row_schema: Option<crate::result_shape::ResultShape>,
}

impl ReasoningResult {
    /// The inferred closure this result carries, or an empty slice when the
    /// payload is not an [`ResultPayload::Inferred`] surface.
    pub fn inferred(&self) -> &[InferredAxiom] {
        match &self.payload {
            ResultPayload::Inferred(axioms) => axioms,
            _ => &[],
        }
    }

    /// `true` iff the ontology is consistent under this result — i.e. the
    /// information state is NOT the witnessed-contradiction glut
    /// [`InformationState::Both`]. The DL consistency surface's projection of the
    /// four-valued verdict back to the historical boolean.
    pub fn is_consistent(&self) -> bool {
        self.information != InformationState::Both
    }

    /// `true` iff the ontology is *decided* consistent — a genuine, conclusive
    /// proof of satisfiability ([`InformationState::Supported`]).
    ///
    /// This is the predicate a **positive-consistency** consumer must use: unlike
    /// [`Self::is_consistent`] (which merely rules out the witnessed-contradiction
    /// glut [`InformationState::Both`]), it also rules out
    /// [`InformationState::Undetermined`] — the honest *cannot-decide* state the
    /// DL fold emits when the bundle carries out-of-fragment constructs the native
    /// path did not decide. A cannot-decide is NOT a positive consistency verdict:
    /// reporting it as "consistent" would silently ignore the undecided axioms
    /// (an unsound answer, the incomplete-never-wrong violation this guards).
    pub fn is_decided_consistent(&self) -> bool {
        self.information == InformationState::Supported
    }

    /// `true` iff this result rests on a conclusive evaluation — a completed run
    /// OR a complete-for-the-fragment answer (SEMANTICS:294-297). This is the
    /// predicate [`InformationState::Neither`] requires.
    pub fn is_conclusive(&self) -> bool {
        self.evaluation == EvaluationStatus::Completed
            || self.completeness == CompletenessStatus::CompleteForFragment
    }

    /// Validate the result invariants (hard-fail doctrine):
    ///
    /// 1. [`InformationState::Neither`] requires a conclusive evaluation.
    /// 2. The preservation polarity set excludes `ValidationOnly`.
    /// 3. [`InformationState::Both`] must carry at least one proof/counterproof or
    ///    contradiction witness justifying the glut (it is never a bare claim).
    ///
    /// # Errors
    /// Returns `Err` describing the first violated invariant.
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        if self.information == InformationState::Neither && !self.is_conclusive() {
            return Err(result_err(
                "ReasoningResult: information=neither requires a conclusive evaluation \
                 (completed run or complete-for-fragment); a non-conclusive empty verdict is \
                 undetermined, not neither"
                    .to_owned(),
            ));
        }
        self.preservation.validate()?;
        if self.information == InformationState::Both
            && (self.provenance.proof.is_none() || self.provenance.counterproof.is_none())
            && self.provenance.contradiction_witnesses.is_empty()
        {
            return Err(result_err(
                "ReasoningResult: information=both requires either a proof+counterproof pair \
                 or at least one contradiction witness; a lone proof or counterproof without \
                 the other does not justify a glut"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    /// Construct, asserting the invariants in debug builds.
    ///
    /// Prefer the [`Self::invalid`] / [`Self::from_dl_verdict`] smart constructors;
    /// this is the low-level builder the folds funnel through.
    pub fn new(
        input: InputStatus,
        evaluation: EvaluationStatus,
        completeness: CompletenessStatus,
        preservation: PreservationClaim,
        information: InformationState,
        provenance: ResultProvenance,
        payload: ResultPayload,
    ) -> Self {
        let result = Self {
            input,
            evaluation,
            completeness,
            preservation,
            information,
            provenance,
            payload,
            row_schema: None,
        };
        debug_assert!(
            result.validate().is_ok(),
            "ReasoningResult invariant violated: {:?}",
            result.validate()
        );
        result
    }

    /// Attach the declared row-schema facet — the typed
    /// [`ResultShape`](crate::result_shape::ResultShape) the result's bindings are
    /// contracted to. The schema is the caller's *declaration*, validated against
    /// the bindings by the consumer; it is never synthesised from the bindings.
    #[must_use]
    pub fn with_row_schema(mut self, schema: crate::result_shape::ResultShape) -> Self {
        self.row_schema = Some(schema);
        self
    }

    /// Validate the result's bindings against a caller-declared `schema`, then
    /// attach the schema via [`Self::with_row_schema`] on success.
    ///
    /// This is the **production entry point** for the `row_schema` facet: the
    /// caller declares the contract it expects (the schema is their *declaration*,
    /// never derived from the bindings) and this method reads each bound term's
    /// **actual** term-kind (IRI / literal+datatype / blank-node) to CHECK it
    /// against that declaration — validation, never synthesis.
    ///
    /// # Payload handling
    ///
    /// - `Bindings`: each `Binding` row is observed as a `Vec<ObservedBinding>`,
    ///   one entry per variable in the map.
    /// - `Marginals`: same, using `ProbBinding::vars`.
    /// - `Inferred` / `Empty`: zero rows — the schema validates as an empty result.
    ///
    /// # Errors
    ///
    /// Returns the first [`crate::result_shape::ContractViolation`] if the bindings do not conform to
    /// the declared schema (missing required column, undeclared column, wrong
    /// term-kind, wrong datatype, or wrong row count in `Count` mode).
    pub fn with_declared_row_schema(
        self,
        schema: crate::result_shape::ResultShape,
    ) -> Result<Self, crate::result_shape::ContractViolation> {
        use crate::result_shape::ObservedBinding;

        let rows: Vec<Vec<ObservedBinding>> = match &self.payload {
            ResultPayload::Bindings(bindings) => bindings
                .iter()
                .map(|b| {
                    b.iter()
                        .map(|(var, val)| {
                            ObservedBinding::new(var.clone(), observed_term_from_str(val))
                        })
                        .collect()
                })
                .collect(),
            ResultPayload::Marginals(marginals) => marginals
                .iter()
                .map(|pb| {
                    pb.vars
                        .iter()
                        .map(|(var, val)| {
                            ObservedBinding::new(var.clone(), observed_term_from_str(val))
                        })
                        .collect()
                })
                .collect(),
            // Inferred / Empty payloads carry no SELECT-style bindings; validate as zero rows.
            ResultPayload::Inferred(_) | ResultPayload::Empty => vec![],
        };

        schema.validate_bindings(&rows)?;
        Ok(self.with_row_schema(schema))
    }

    /// An ill-formed request: nothing was reasoned (SEMANTICS:249-250). The other
    /// four fields are pinned to their vacuous values.
    pub fn invalid(reason: impl Into<String>, mut provenance: ResultProvenance) -> Self {
        provenance.conclusion = reason.into();
        Self::new(
            InputStatus::Invalid,
            EvaluationStatus::Unsupported,
            CompletenessStatus::Unknown,
            PreservationClaim::default(),
            InformationState::NotEvaluated,
            provenance,
            ResultPayload::Empty,
        )
    }

    /// Build a result for a **queried conclusion** from its proof / counterproof,
    /// classifying the information state at the single [`InformationState::classify`]
    /// chokepoint (so the conclusiveness invariant cannot be violated).
    ///
    /// This is the query-surface entry point: `proof` is present iff
    /// the conclusion is derivable; `counterproof` is present iff its explicit
    /// negation is derivable. The Belnap quadrant follows: proof-only ⇒ supported,
    /// counterproof-only ⇒ opposed, both ⇒ a witnessed contradiction (`both`),
    /// neither ⇒ `neither` when conclusive else `undetermined`;
    /// `semantics_available == false` ⇒ `not-evaluated`. When `both`, the proof and
    /// counterproof themselves are the glut witnesses the [`Self::validate`]
    /// invariant requires.
    #[allow(clippy::too_many_arguments)]
    pub fn from_query(
        payload: ResultPayload,
        proof: Option<DerivationRef>,
        counterproof: Option<DerivationRef>,
        evaluation: EvaluationStatus,
        completeness: CompletenessStatus,
        preservation: PreservationClaim,
        semantics_available: bool,
        mut provenance: ResultProvenance,
    ) -> Self {
        let conclusive = evaluation == EvaluationStatus::Completed
            || completeness == CompletenessStatus::CompleteForFragment;
        let information = InformationState::classify(
            proof.is_some(),
            counterproof.is_some(),
            conclusive,
            semantics_available,
        );
        provenance.proof = proof;
        provenance.counterproof = counterproof;
        provenance.projection_class = preservation.clone();
        Self::new(
            InputStatus::Valid,
            evaluation,
            completeness,
            preservation,
            information,
            provenance,
            payload,
        )
    }

    /// Build a result from a native DL consistency [`DlVerdict`] and its closure.
    ///
    /// This is the `reason` surface's fold (the consistency-verdict reading of the
    /// information axis): an inconsistent verdict is a witnessed contradiction
    /// ([`InformationState::Both`]) carrying its witnesses; a consistent verdict is
    /// [`InformationState::Supported`] when conclusive. Unsupported DL constructs
    /// surface in [`PreservationClaim::unsupported_constructs`] and drop the
    /// completeness to [`CompletenessStatus::Incomplete`].
    pub fn from_dl_verdict(
        inferred: Vec<InferredAxiom>,
        verdict: &DlVerdict,
        provenance: ResultProvenance,
    ) -> Self {
        Self::from_dl_verdict_with_preservation(
            inferred,
            verdict,
            &PreservationClaim::exact(),
            provenance,
        )
    }

    /// Like [`Self::from_dl_verdict`], but UNIONS an additional lowering claim's
    /// unsupported constructs into the preservation set before deriving polarity.
    ///
    /// Used by the program-carrying `reason` lane: the formula → relational-core lowering
    /// may carry first-order constructs (disjunctive heads, `∃`-functions, sequence
    /// markers, …) that did not lower to the evaluable Horn fragment. The answer is
    /// complete only for the fragment BOTH the DL construct coverage and the lowering
    /// cover; the residue is disclosed (`{sound-under}` + `unsupported_constructs`), never
    /// silently absent. Passing [`PreservationClaim::exact`] recovers [`Self::from_dl_verdict`].
    pub fn from_dl_verdict_with_preservation(
        inferred: Vec<InferredAxiom>,
        verdict: &DlVerdict,
        extra: &PreservationClaim,
        mut provenance: ResultProvenance,
    ) -> Self {
        // The shared unsupported-set → polarity rule (One-Path): `{exact}` when the
        // fragment is fully covered, `{sound-under}` carrying the uncovered constructs
        // otherwise — over the UNION of the DL coverage gap and the lowering residue.
        let mut merged: BTreeSet<String> = verdict
            .coverage
            .unsupported
            .iter()
            .map(ToString::to_string)
            .collect();
        merged.extend(extra.unsupported_constructs.iter().cloned());
        let preservation = PreservationClaim::for_unsupported(merged);
        let unsupported = !preservation.unsupported_constructs.is_empty();
        // The native DL path runs to its end; an unsupported construct does not stop
        // the run, it bounds the fragment the answer is complete for.
        let evaluation = EvaluationStatus::Completed;
        let completeness = if unsupported {
            CompletenessStatus::Incomplete
        } else {
            CompletenessStatus::CompleteForFragment
        };

        let conclusive = evaluation == EvaluationStatus::Completed
            || completeness == CompletenessStatus::CompleteForFragment;

        let information = if !verdict.consistent {
            provenance.contradiction_witnesses = verdict
                .inconsistencies
                .iter()
                .map(ContradictionWitness::from)
                .collect();
            provenance.contradiction_witnesses.sort();
            InformationState::Both
        } else if unsupported {
            // No inconsistency witness was derived, but the native path carries
            // constructs it does NOT decide (the coverage gap). It reached that
            // "no clash" state by IGNORING those out-of-fragment axioms, any of
            // which could have forced a contradiction — so there is genuinely no
            // proof of satisfiability. The consistency verdict is UNDETERMINED
            // (cannot-decide), never a wrong `supported`/`consistent`. This is
            // the incomplete-never-wrong soundness floor; it does NOT collapse the
            // Belnap lattice — a real witnessed contradiction still yields `Both`.
            InformationState::Undetermined
        } else {
            // The consistency claim is supported (a proof of satisfiability) with no
            // counterproof, conclusively when the fragment is fully covered.
            InformationState::classify(true, false, conclusive, true)
        };

        provenance.projection_class = preservation.clone();
        Self::new(
            InputStatus::Valid,
            evaluation,
            completeness,
            preservation,
            information,
            provenance,
            ResultPayload::Inferred(inferred),
        )
    }
}

/// Map a canonical binding value string (as produced by `provenance::term_n3` and
/// stored in [`crate::query_ir::Binding`] / [`crate::probabilistic::ProbBinding`])
/// to the [`crate::result_shape::ObservedTerm`] surface used by
/// [`crate::result_shape::ResultShape::validate_bindings`].
///
/// Canonical forms (from `provenance::term_n3` / `encode::literal_n3`):
/// - `<iri>` → `ObservedTerm::Iri`
/// - `_:id` → `ObservedTerm::BlankNode`
/// - `"lex"` → `ObservedTerm::Literal { datatype: xsd:string }`
/// - `"lex"@lang` → `ObservedTerm::Literal { datatype: rdf:langString }`
/// - `"lex"^^<dtype>` → `ObservedTerm::Literal { datatype: dtype }`
pub(crate) fn observed_term_from_str(val: &str) -> crate::result_shape::ObservedTerm {
    use crate::result_shape::ObservedTerm;

    if let Some(iri) = val.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        let _ = iri; // the IRI content is not needed for term-kind validation
        return ObservedTerm::Iri;
    }
    if val.starts_with("_:") {
        return ObservedTerm::BlankNode;
    }
    // All remaining forms start with `"`.
    // Determine the datatype:
    //   `"lex"^^<dtype>`  — typed literal
    //   `"lex"@lang`      — language-tagged → rdf:langString
    //   `"lex"`           — plain → xsd:string
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
    const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

    // Find the closing `"` of the lexical form (the last `"` that is NOT the
    // opening `"`, accounting for backslash escapes in the lexical form).
    let datatype = if let Some(rest) = find_literal_suffix(val) {
        if let Some(dtype) = rest.strip_prefix("^^<").and_then(|s| s.strip_suffix('>')) {
            dtype.to_owned()
        } else if rest.starts_with('@') {
            RDF_LANG_STRING.to_owned()
        } else {
            // Bare closing `"` — plain xsd:string.
            XSD_STRING.to_owned()
        }
    } else {
        // Malformed — treat as plain string defensively.
        XSD_STRING.to_owned()
    };
    ObservedTerm::Literal { datatype }
}

/// Find the suffix after the closing `"` of a quoted literal canonical string.
///
/// Input: the full canonical string starting with `"`. Walks past the lexical
/// form (handling `\\`, `\"`, `\n`, `\r`, `\t` escapes) and returns everything
/// after the closing `"`.  Returns `None` if the string is malformed.
fn find_literal_suffix(s: &str) -> Option<&str> {
    // s starts with `"` — skip it.
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                // Escape sequence: skip the next byte.
                i += 2;
            }
            b'"' => {
                // Closing quote found; return everything after it.
                return Some(&s[i + 1..]);
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
