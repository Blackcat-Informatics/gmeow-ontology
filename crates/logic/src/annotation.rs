// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Opaque semiring annotations carried through native tuple evaluation.
//!
//! This is the public contract for score-carrying evaluation.  A caller supplies an
//! algebra whose `multiply` combines conjunctive premises and whose `add` combines
//! alternative derivations.  The physical core never interprets the element: it keeps
//! the value beside the tuple while the rule fires, then exposes both the combined value
//! and the direct derivation contributions on each answer.
//!
//! An algebra that is not a semiring must say so.  [`AnnotationContract`] scopes the
//! declared law deviation to structural query classes, and the native classifier refuses
//! a program outside that declaration rather than silently applying semiring reasoning.

use std::collections::BTreeSet;

use purrdf::TermValue;

use crate::query_ir::{Binding, CompletionFrontier};
use crate::seam::{BudgetStatus, DerivedQuad};

/// Caller-supplied algebra for one opaque tuple annotation.
///
/// `add` is alternative derivation (`⊕`); `multiply` is body conjunction (`⊗`).
/// Operations are fallible so overflow, an invalid score domain, or another algebraic
/// failure is a typed engine error rather than saturation or wrapping.
pub trait TupleAnnotationAlgebra {
    /// The opaque value carried beside each tuple.
    type Element: Clone + PartialEq + std::fmt::Debug;

    /// Stable semantic identity of this algebra, included in query/provider contracts.
    fn identity(&self) -> &str;

    /// Canonical, deterministic encoding of one element for content-addressed receipts.
    fn canonical_element(&self, element: &Self::Element) -> String;

    /// No derivation.
    fn zero(&self) -> Self::Element;
    /// Asserted/unit evidence.
    fn one(&self) -> Self::Element;
    /// Combine alternative derivations.
    fn add(
        &self,
        left: &Self::Element,
        right: &Self::Element,
    ) -> gmeow_errors::Result<Self::Element>;
    /// Combine conjunctive premises.
    fn multiply(
        &self,
        left: &Self::Element,
        right: &Self::Element,
    ) -> gmeow_errors::Result<Self::Element>;
}

/// A semiring law a caller may explicitly declare its annotation algebra violates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemiringLaw {
    /// `a ⊕ b = b ⊕ a`.
    AddCommutative,
    /// `(a ⊕ b) ⊕ c = a ⊕ (b ⊕ c)`.
    AddAssociative,
    /// `0 ⊕ a = a`.
    AdditiveIdentity,
    /// `(a ⊗ b) ⊗ c = a ⊗ (b ⊗ c)`.
    MultiplyAssociative,
    /// `1 ⊗ a = a`.
    MultiplicativeIdentity,
    /// Multiplication distributes over addition.
    Distributive,
    /// `0 ⊗ a = a ⊗ 0 = 0`.
    ZeroAnnihilates,
}

impl SemiringLaw {
    /// Stable wire identity used by annotation-contract hashing.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::AddCommutative => "add-commutative",
            Self::AddAssociative => "add-associative",
            Self::AdditiveIdentity => "additive-identity",
            Self::MultiplyAssociative => "multiply-associative",
            Self::MultiplicativeIdentity => "multiplicative-identity",
            Self::Distributive => "distributive",
            Self::ZeroAnnihilates => "zero-annihilates",
        }
    }
}

/// Structural class certified for one annotated run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnnotationQueryClass {
    /// Positive, finite Datalog with an acyclic IDB dependency graph.
    PositiveAcyclic,
    /// Positive, finite Datalog with at least one recursive IDB dependency.
    PositiveRecursive,
    /// Positive arity-generic Datalog with an acyclic IDB dependency graph.
    PositiveNaryAcyclic,
    /// Positive arity-generic Datalog with at least one recursive IDB dependency.
    PositiveNaryRecursive,
    /// Stratified negation-as-failure. Negative literals are membership guards:
    /// a satisfied absence contributes `one()` and is not a lineage source.
    StratifiedNaf,
    /// Well-founded non-monotone evaluation. Scores follow the selected positive
    /// support of rows in the well-founded result; negative guards contribute `one()`.
    WellFounded,
    /// Cautious stable-model evaluation. Scores follow positive support common to
    /// the selected cautious result; negative guards contribute `one()`.
    StableModel,
    /// Restricted existential chase. Every conjunctive head row receives the body
    /// product of its firing; invented witnesses are identities, not scored premises.
    ExistentialChase,
}

impl AnnotationQueryClass {
    /// Stable wire identity used by annotation-contract hashing.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::PositiveAcyclic => "positive-acyclic",
            Self::PositiveRecursive => "positive-recursive",
            Self::PositiveNaryAcyclic => "positive-nary-acyclic",
            Self::PositiveNaryRecursive => "positive-nary-recursive",
            Self::StratifiedNaf => "stratified-naf",
            Self::WellFounded => "well-founded",
            Self::StableModel => "stable-model",
            Self::ExistentialChase => "existential-chase",
        }
    }
}

/// Which physical lineage surface justified an annotation certificate.
///
/// Positive binary/n-ary and stratified evaluation enumerate every admitted rule
/// grounding during the one closure pass. Non-monotone solvers and the restricted
/// chase already select deterministic physical proof rows, so their annotation fold
/// follows those selected rows without re-running a second closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationLineageContract {
    /// Every direct positive grounding contributes to `add` (`oplus`).
    AllPhysicalDerivations,
    /// The native solver's deterministic selected proof carrier contributes.
    SelectedPhysicalDerivation,
}

/// An explicit non-semiring declaration and the query classes for which the caller
/// warrants a complete over-approximation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAnnotationApproximation {
    /// The semiring laws the supplied algebra does not satisfy.
    pub deviates_from: BTreeSet<SemiringLaw>,
    /// Structural query classes for which the caller warrants the deviation as a
    /// complete over-approximation.
    pub certified_for: BTreeSet<AnnotationQueryClass>,
}

/// Admission contract for annotation evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationContract {
    /// `None` means a genuine semiring and therefore exact annotation preservation.
    pub approximation: Option<DeclaredAnnotationApproximation>,
    /// Deterministic convergence guard for the annotation fixed point.
    ///
    /// Reaching this limit is an error; the engine never returns a silently truncated
    /// annotation.  The default is intentionally generous for bounded absorptive
    /// carriers while still making a non-convergent recursive algebra finite to diagnose.
    pub max_fixpoint_rounds: usize,
}

impl AnnotationContract {
    /// Exact semiring evaluation with the default convergence guard.
    #[must_use]
    pub fn exact() -> Self {
        Self {
            approximation: None,
            max_fixpoint_rounds: 1_024,
        }
    }

    /// A declared complete over-approximation, scoped to explicit query classes.
    #[must_use]
    pub fn complete_over(
        deviates_from: impl IntoIterator<Item = SemiringLaw>,
        certified_for: impl IntoIterator<Item = AnnotationQueryClass>,
    ) -> Self {
        Self {
            approximation: Some(DeclaredAnnotationApproximation {
                deviates_from: deviates_from.into_iter().collect(),
                certified_for: certified_for.into_iter().collect(),
            }),
            max_fixpoint_rounds: 1_024,
        }
    }

    /// Override the deterministic convergence guard.
    #[must_use]
    pub fn with_max_fixpoint_rounds(mut self, rounds: usize) -> Self {
        self.max_fixpoint_rounds = rounds;
        self
    }

    /// Canonical, versioned identity of the algebraic admission contract.
    #[must_use]
    pub fn canonical_key(&self) -> String {
        let mut out = format!(
            "gmeow-annotation-contract-v1\0rounds={}\0",
            self.max_fixpoint_rounds
        );
        match &self.approximation {
            None => out.push_str("exact"),
            Some(declaration) => {
                out.push_str("complete-over\0laws=");
                for law in &declaration.deviates_from {
                    out.push_str(law.wire());
                    out.push(',');
                }
                out.push_str("\0classes=");
                for class in &declaration.certified_for {
                    out.push_str(class.wire());
                    out.push(',');
                }
            }
        }
        out
    }
}

impl Default for AnnotationContract {
    fn default() -> Self {
        Self::exact()
    }
}

/// Algebra, admission contract, and asserted-fact annotation source for one run.
///
/// Grouping these cohesive inputs keeps the public dispatch/materialization calls small
/// and makes it impossible to pass an algebra without its preservation declaration.
pub struct AnnotationRequest<'a, A, F> {
    /// Caller-supplied `⊕`/`⊗` implementation.
    pub algebra: &'a A,
    /// Exact or explicitly approximating admission contract.
    pub contract: &'a AnnotationContract,
    /// Asserted-fact lookup; `None` means the multiplicative identity.
    pub annotation_for: F,
}

impl<'a, A, F> AnnotationRequest<'a, A, F> {
    /// Bundle the annotation inputs for a native evaluation.
    #[must_use]
    pub fn new(algebra: &'a A, contract: &'a AnnotationContract, annotation_for: F) -> Self {
        Self {
            algebra,
            contract,
            annotation_for,
        }
    }
}

/// Engine-certified scope and preservation polarity for one annotated run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationCertification {
    /// The structural class inspected from the actual rule program.
    pub query_class: AnnotationQueryClass,
    /// Exact for a declared semiring; complete-over for an admitted law deviation.
    pub preservation: crate::result::PreservationClaim,
    /// Empty for an exact semiring; otherwise the explicitly declared deviations.
    pub declared_deviations: BTreeSet<SemiringLaw>,
    /// Whether the physical class exposes every grounding or a solver-selected
    /// support carrier. This is explicit so a consumer never infers lineage strength.
    pub lineage_contract: AnnotationLineageContract,
}

/// Stable public identity of one world-scoped tuple in annotation lineage.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnnotatedFactKey {
    /// Named graph/world IRI.
    pub graph: String,
    /// Canonical N3 subject surface.
    pub subject: String,
    /// Predicate IRI.
    pub predicate: String,
    /// Canonical N3 object surface.
    pub object: String,
}

/// Stable identity of an arity-generic world-scoped tuple in annotation lineage.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnnotatedTupleKey {
    /// Named graph/world IRI.
    pub graph: String,
    /// Relation name.
    pub relation: String,
    /// Canonical N3 argument surfaces in positional order.
    pub arguments: Vec<String>,
}

/// One direct derivation's contribution to a tuple annotation.
///
/// This is a one-hop lineage edge, not an eagerly-expanded proof tree.  Recursive
/// consumers follow `sources` by key, preserving the repository's bounded-provenance
/// doctrine while retaining the score attached to each alternative firing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationDerivation<E> {
    /// Rule that produced this contribution (`logic:assert` for an input tuple).
    pub rule_iri: String,
    /// Direct positive premises in authored body order.
    pub sources: Vec<AnnotatedFactKey>,
    /// Direct arity-generic premises in authored body order. Binary derivations
    /// keep using `sources`; n-ary derivations use this lossless positional carrier.
    pub tuple_sources: Vec<AnnotatedTupleKey>,
    /// Query-scoped external tuples that contribute to this derivation, including
    /// provider/artifact/model/request identity and the explicit annotation dimension.
    pub provider_sources: Vec<crate::external_relation::ProviderTupleSource>,
    /// The premise product contributed by this firing.
    pub annotation: E,
}

/// One query answer with its combined annotation and direct score lineage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotatedAnswer<E> {
    /// Goal-variable bindings.
    pub binding: Binding,
    /// `⊕` of every admitted direct derivation contribution.
    pub annotation: E,
    /// Direct derivation contributions whose `⊕` produced `annotation`.
    pub derivations: Vec<AnnotationDerivation<E>>,
}

/// Score-carrying counterpart of [`crate::query_ir::AnswerSet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotatedAnswerSet<E> {
    /// Deterministically sorted answer rows.
    pub answers: Vec<AnnotatedAnswer<E>>,
    /// Whether fact evaluation completed within its declared budget.
    pub status: BudgetStatus,
    /// Logical preservation disclosure of the underlying answer evaluation.
    pub preservation: crate::result::PreservationClaim,
    /// Completion frontier of the underlying native fact evaluation.
    pub frontier: CompletionFrontier,
    /// Structural and algebraic admission certificate for the annotation layer.
    pub certification: AnnotationCertification,
}

/// One materialized quad plus its combined annotation and direct score lineage.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotatedQuad<E> {
    /// Existing world-scoped fact/proof carrier.
    pub quad: DerivedQuad,
    /// `⊕` of every admitted direct derivation contribution.
    pub annotation: E,
    /// Direct derivation contributions whose `⊕` produced `annotation`.
    pub derivations: Vec<AnnotationDerivation<E>>,
}

/// Public annotation source signature used by materialization.
///
/// Returning `None` assigns the algebra's multiplicative identity.  This makes an
/// unscored RDF fact neutral while scored extensional relations opt in tuple by tuple.
#[derive(Debug, Clone, Copy)]
pub struct AnnotationFactRef<'a> {
    /// Named graph/world IRI.
    pub world: &'a str,
    /// Native RDF subject term.
    pub subject: &'a TermValue,
    /// Predicate IRI.
    pub predicate: &'a str,
    /// Native RDF object term.
    pub object: &'a TermValue,
}

/// Asserted-fact annotation lookup signature.
pub type QuadAnnotationSource<'a, E> =
    dyn for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<E> + 'a;
