// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Thin, HONEST wrappers over the purrdf `entail` Description-Logic services.
//!
//! The reasoning core takes its OWL 2 Direct-Semantics answers from the purrdf
//! `entail` crate: consistency, class satisfiability, classification, realization,
//! instance retrieval and axiom entailment through [`purrdf::entail::Reasoner`],
//! plus the two services that need no tableau at all — profile certification
//! ([`purrdf::entail::profile`]) and syntactic-locality module extraction
//! ([`purrdf::entail::extract_module`]) — and the query-directed combined and
//! certain-answer surfaces.
//!
//! These wrappers add exactly two things and remove nothing:
//!
//! * they map [`purrdf::entail::EntailError`] onto the shared
//!   [`gmeow_errors`](gmeow_errors) substrate through the reasoning-core
//!   [`Reason`](crate::error::Reason) diagnostic, so a caller inside this workspace
//!   sees a typed [`Diag`](gmeow_errors::Diag) rather than a foreign error type; and
//! * they carry every [`purrdf::entail::Certified`] answer WITH its
//!   [`purrdf::entail::DlCertificate`] completeness verdict and construct boundaries
//!   as one value, [`CertifiedAnswer`], so a service's answer is never read apart
//!   from how complete it is.
//!
//! Nothing is flattened: a [`Verdict::Unknown`] stays `Unknown`, a boundary residue
//! stays on the answer, and an unsatisfiable ontology stays the typed error every
//! service but [`DlReasoner::consistency`] returns for it. The dataset type is
//! purrdf's own [`RdfDataset`], which is already the reasoning core's carrier — no
//! conversion sits between the caller and the service.
//!
//! # Evaluation ceilings are external, and exhaustion is never a fabricated answer
//!
//! Every service here runs under a purrdf ceiling this repository cannot raise: the
//! datalog engine's `pub const` budgets (`MAX_JOIN_STEPS`, `MAX_STORED_FACTS`,
//! `MAX_TERM_ARENA_BYTES`) for the chase-backed surfaces ([`certain_answers`],
//! [`materialize_combined`]), and the size-derived per-decision hypertableau step cap
//! for the tableau surfaces on [`DlReasoner`]. Raising any of them is an
//! upstream-purrdf change, never an in-repo tune. What this module guarantees is that
//! exhaustion is HONEST, taking exactly one of two shapes and never a third:
//!
//! * a service that answers through a [`Certified`](purrdf::entail::Certified) value
//!   carries the exhaustion in its certificate — an exhausted hypertableau run reports
//!   [`DlCompleteness::BudgetExhausted`], its [`CertifiedAnswer::is_decided`] and
//!   [`CertifiedAnswer::is_exact`] both read `false`, and a boolean service returns
//!   [`Verdict::Unknown`] rather than guessing `True`/`False`; and
//! * a service that returns a bare [`Result`] maps every [`EntailError`] — including
//!   the budget refusals — onto a hard [`Diag`](gmeow_errors::Diag) through
//!   [`map_entail_err`], so an exhausted chase becomes a refusal, never a partial
//!   answer passed off as complete.
//!
//! Neither shape truncates silently. The per-decision step cap is the one ceiling that
//! CAN be narrowed in-repo — only downward, through [`DlReasoner::with_step_cap`] —
//! which exists so the exhaustion path is reachable and testable rather than a branch
//! nothing exercises.

use gmeow_errors::Result;
use purrdf::entail::{
    Boundary, CertainAnswers, ClassHierarchy, CombinedMaterialization, DlAxiom, DlCertificate,
    DlCompleteness, EntailError, ImportMap, ModuleExtraction, ModuleMethod, ProfileCertificate,
    QTriple, Realization, Reasoner, Regime, Verdict,
};
use purrdf::{RdfDataset, TermValue};

/// Lower a purrdf [`EntailError`] onto the shared reasoning-core diagnostic
/// substrate, preserving its rendered text verbatim.
///
/// The reasoning core admits no degraded fallback, so every purrdf entail refusal —
/// an unsatisfiable ontology, a malformed class-expression graph, a budget the
/// datalog evaluator passed, an inconsistency witness — becomes a hard
/// [`Reason`](crate::error::Reason) diagnostic rather than a bare string or a
/// silently swallowed `None`.
fn map_entail_err(error: EntailError) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: format!("purrdf entail service refused: {error}"),
    })
}

/// A DL reasoning answer together with the completeness verdict and construct
/// boundaries the purrdf reasoner measured while producing it.
///
/// Neither the answer's own three-valued content (a [`Verdict::Unknown`]) nor the
/// certificate's residue is discarded: [`completeness`](Self::completeness) says how
/// complete the answer is w.r.t. the DL-clause set that was actually decided, and
/// [`boundaries`](Self::boundaries) names every construct the reverse mapping could
/// not turn into a DL clause. A caller that wants "the Direct-Semantics answer for
/// the ontology as supplied" checks [`is_exact`](Self::is_exact); a caller that only
/// needs "the search finished" checks [`is_decided`](Self::is_decided).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertifiedAnswer<T> {
    /// The service's answer, carried exactly as the service produced it.
    pub answer: T,
    /// How complete the answer is w.r.t. the DL-clause set the hypertableau decided.
    pub completeness: DlCompleteness,
    /// The constructs the reverse mapping could not turn into DL clauses, in
    /// [`Construct`](purrdf::entail::Construct) declaration order.
    pub boundaries: Vec<Boundary>,
    /// Derivation rounds consumed, summed over every hypertableau run the service
    /// made — a measurement in the round cap's own units.
    pub steps: u64,
    /// The per-decision round cap every run of this service ran under.
    pub budget: u64,
    /// How many hypertableau runs the service made.
    pub decisions: u64,
}

impl<T> CertifiedAnswer<T> {
    /// Pair `answer` with the completeness and boundaries the certificate measured.
    fn from_parts(answer: T, certificate: &DlCertificate) -> Self {
        Self {
            answer,
            completeness: certificate.completeness(),
            boundaries: certificate.boundaries().to_vec(),
            steps: certificate.steps(),
            budget: certificate.budget(),
            decisions: certificate.decisions(),
        }
    }

    /// Split a purrdf [`Certified`](purrdf::entail::Certified) answer into its answer
    /// and its measured certificate, keeping both.
    fn from_certified(certified: purrdf::entail::Certified<T>) -> Self {
        let (answer, certificate) = certified.into_parts();
        Self::from_parts(answer, &certificate)
    }

    /// Whether every hypertableau run the service made decided its question — i.e.
    /// no run reached its step cap. `True` for both [`DlCompleteness::Decided`] and
    /// [`DlCompleteness::DecidedWithinBoundaries`]; the second still carries
    /// [`boundaries`](Self::boundaries), so a caller wanting the answer for the WHOLE
    /// ontology must check [`is_exact`](Self::is_exact) as well.
    #[must_use]
    pub fn is_decided(&self) -> bool {
        self.completeness.is_decided()
    }

    /// Whether the answer is the OWL 2 Direct-Semantics answer for the ontology
    /// exactly as supplied: every run finished AND no construct was left out.
    #[must_use]
    pub fn is_exact(&self) -> bool {
        matches!(self.completeness, DlCompleteness::Decided)
    }
}

/// The OWL 2 Direct-Semantics reasoning services over one [`RdfDataset`], each
/// answering a [`CertifiedAnswer`].
///
/// Owns a purrdf [`Reasoner`], whose three question-taking services
/// ([`class_satisfiability`](Self::class_satisfiability),
/// [`instances`](Self::instances), [`entails`](Self::entails)) intern the term they
/// are handed and therefore take `&mut self`, while the three that range over the
/// ontology's own vocabulary ([`consistency`](Self::consistency),
/// [`classify`](Self::classify), [`realize`](Self::realize)) take `&self` — the same
/// split the underlying reasoner makes, surfaced rather than hidden.
#[derive(Debug)]
pub struct DlReasoner {
    /// The purrdf reasoner this façade delegates every question to.
    inner: Reasoner,
}

impl DlReasoner {
    /// Reverse-map `dataset` into a knowledge base and open the reasoning services
    /// over it.
    ///
    /// # Errors
    ///
    /// A [`Reason`](crate::error::Reason) diagnostic if the reverse mapping refuses —
    /// a malformed OWL class-expression graph, or an `owl:hasKey` axiom that
    /// exhausts the tableau or finds the ontology already unsatisfiable.
    pub fn new(dataset: &RdfDataset) -> Result<Self> {
        Reasoner::new(dataset)
            .map(|inner| Self { inner })
            .map_err(map_entail_err)
    }

    /// Open the reasoning services over `dataset` with the per-decision step cap
    /// narrowed to `cap`.
    ///
    /// The cap is clamped to the reasoner's size-derived ceiling and can only lower
    /// it, never raise it — this exists so the budget-exhausted path is reachable
    /// deterministically rather than as a branch nobody exercises.
    ///
    /// # Errors
    ///
    /// As [`new`](Self::new).
    pub fn with_step_cap(dataset: &RdfDataset, cap: u64) -> Result<Self> {
        Reasoner::new(dataset)
            .map(|reasoner| Self {
                inner: reasoner.with_step_cap(cap),
            })
            .map_err(map_entail_err)
    }

    /// The per-decision step cap every tableau run of this reasoner runs under.
    #[must_use]
    pub const fn step_cap(&self) -> u64 {
        self.inner.step_cap()
    }

    /// The named classes this reasoner ranges over, in visit order.
    #[must_use]
    pub fn signature(&self) -> Vec<TermValue> {
        self.inner.signature()
    }

    /// The named individuals this reasoner ranges over, in visit order.
    #[must_use]
    pub fn named_individuals(&self) -> Vec<TermValue> {
        self.inner.named_individuals()
    }

    /// Whether the ontology has a model.
    ///
    /// The one service that never errors on an unsatisfiable ontology, because it is
    /// the service that DETECTS one: it answers [`Verdict::False`] where every other
    /// service returns the unsatisfiable error. An exhausted search answers
    /// [`Verdict::Unknown`], which the [`CertifiedAnswer`] carries rather than
    /// collapsing to `False`.
    #[must_use]
    pub fn consistency(&self) -> CertifiedAnswer<Verdict> {
        CertifiedAnswer::from_certified(self.inner.consistency())
    }

    /// Whether `class` can have an instance in some model of the ontology.
    ///
    /// # Errors
    ///
    /// A [`Reason`](crate::error::Reason) diagnostic if the ontology has no model at
    /// all — every class is then vacuously unsatisfiable and the answer would say
    /// nothing.
    pub fn class_satisfiability(&mut self, class: &TermValue) -> Result<CertifiedAnswer<Verdict>> {
        self.inner
            .class_satisfiability(class)
            .map(CertifiedAnswer::from_certified)
            .map_err(map_entail_err)
    }

    /// The subsumption hierarchy over the ontology's named classes.
    ///
    /// # Errors
    ///
    /// A [`Reason`](crate::error::Reason) diagnostic if the ontology has no model.
    pub fn classify(&self) -> Result<CertifiedAnswer<ClassHierarchy>> {
        self.inner
            .classify()
            .map(CertifiedAnswer::from_certified)
            .map_err(map_entail_err)
    }

    /// The entailed types of the ontology's named individuals, and the most specific
    /// of them.
    ///
    /// # Errors
    ///
    /// A [`Reason`](crate::error::Reason) diagnostic if the ontology has no model.
    pub fn realize(&self) -> Result<CertifiedAnswer<Realization>> {
        self.inner
            .realize()
            .map(CertifiedAnswer::from_certified)
            .map_err(map_entail_err)
    }

    /// The named individuals entailed to be instances of `class`, sorted.
    ///
    /// # Errors
    ///
    /// A [`Reason`](crate::error::Reason) diagnostic if the ontology has no model.
    pub fn instances(&mut self, class: &TermValue) -> Result<CertifiedAnswer<Vec<TermValue>>> {
        self.inner
            .instances(class)
            .map(CertifiedAnswer::from_certified)
            .map_err(map_entail_err)
    }

    /// Whether the ontology entails `axiom`, decided by refutation.
    ///
    /// # Errors
    ///
    /// A [`Reason`](crate::error::Reason) diagnostic if the ontology has no model, in
    /// which case every axiom is entailed and the answer would be worthless.
    pub fn entails(&mut self, axiom: &DlAxiom) -> Result<CertifiedAnswer<Verdict>> {
        self.inner
            .entails(axiom)
            .map(CertifiedAnswer::from_certified)
            .map_err(map_entail_err)
    }
}

/// Certify `dataset` against the OWL 2 profiles (EL, QL, RL, DL, Full).
///
/// Purely syntactic and infallible — no tableau, no closure, no budget — so it is
/// returned directly rather than wrapped in a `Result`: there is no
/// [`EntailError`] for it to map, and manufacturing an always-`Ok` result would be
/// exactly the silent optionality the reasoning core forbids. A clean certification
/// PROVES membership; a violation proves only that the cheap structural check failed.
#[must_use]
pub fn certify_profiles(dataset: &RdfDataset) -> ProfileCertificate {
    purrdf::entail::profile(dataset)
}

/// Extract the syntactic-locality module of `dataset` for the seed `signature`
/// under `method` (`⊥`, `⊤`, or the nested `⊥⊤*`).
///
/// # Errors
///
/// A [`Reason`](crate::error::Reason) diagnostic if the extracted module cannot be
/// frozen into a dataset.
pub fn extract_module(
    dataset: &RdfDataset,
    signature: &[TermValue],
    method: ModuleMethod,
) -> Result<ModuleExtraction> {
    purrdf::entail::extract_module(dataset, signature, method).map_err(map_entail_err)
}

/// Attempt the combined (Lutz/Toman/Wolter, Stefanoni/Motik/Horrocks) approach for
/// `dataset`'s basic graph pattern `query_bgp`.
///
/// `Ok(None)` is the HONEST "not applicable" answer the purrdf surface returns when
/// the ontology's TBox is outside the Horn fragment the combined approach can lower
/// and chase — the caller then falls back to the whole-vocabulary augmentation. It
/// is not an error and not a silent empty result: the distinction between "does not
/// apply" and "applies and found nothing" is preserved.
///
/// # Errors
///
/// A [`Reason`](crate::error::Reason) diagnostic propagating any purrdf refusal from
/// the reverse mapping or the restricted chase (e.g. an inconsistent knowledge base).
pub fn materialize_combined(
    dataset: &RdfDataset,
    query_bgp: &[QTriple],
) -> Result<Option<CombinedMaterialization>> {
    purrdf::entail::materialize_combined(dataset, query_bgp).map_err(map_entail_err)
}

/// Compute the certain answers to `bgp` over `premise` under `regime`, resolving
/// `owl:imports` through `imports`.
///
/// The returned [`CertainAnswers`] carries its own completeness signal
/// ([`CertainAnswers::is_complete`] / [`CertainAnswers::limits`]) and the reasoning
/// report the rows were drawn from, so an empty row set that means "none found" is
/// never confused with one that means "none exists".
///
/// # Errors
///
/// A [`Reason`](crate::error::Reason) diagnostic for a regime the service is not
/// total over, an unresolved import, an inconsistent premise, an exhausted match
/// budget, or any refusal the underlying materialization returns.
pub fn certain_answers(
    premise: &RdfDataset,
    bgp: &[QTriple],
    regime: Regime,
    imports: &ImportMap,
) -> Result<CertainAnswers> {
    purrdf::entail::certain_answers(premise, bgp, regime, imports).map_err(map_entail_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::RdfDatasetBuilder;
    use purrdf::entail::OwlProfile;

    /// Reserved vocabulary the fixtures below assert over.
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

    const CAT: &str = "http://example.org/Cat";
    const DOG: &str = "http://example.org/Dog";
    const ANIMAL: &str = "http://example.org/Animal";
    const TOM: &str = "http://example.org/tom";

    /// A tiny, consistent A-Box: `Cat ⊑ Animal`, `tom a Cat`.
    fn consistent_dataset() -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        let cat = builder.intern_iri(CAT);
        let animal = builder.intern_iri(ANIMAL);
        let tom = builder.intern_iri(TOM);
        let sub = builder.intern_iri(RDFS_SUBCLASSOF);
        let ty = builder.intern_iri(RDF_TYPE);
        builder.push_quad(cat, sub, animal, None);
        builder.push_quad(tom, ty, cat, None);
        builder.freeze().expect("freeze the consistent fixture")
    }

    /// A tiny, INCONSISTENT A-Box: `Cat` and `Dog` are disjoint, yet `tom` is both.
    fn inconsistent_dataset() -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        let cat = builder.intern_iri(CAT);
        let dog = builder.intern_iri(DOG);
        let tom = builder.intern_iri(TOM);
        let disjoint = builder.intern_iri(OWL_DISJOINT_WITH);
        let ty = builder.intern_iri(RDF_TYPE);
        builder.push_quad(cat, disjoint, dog, None);
        builder.push_quad(tom, ty, cat, None);
        builder.push_quad(tom, ty, dog, None);
        builder.freeze().expect("freeze the inconsistent fixture")
    }

    #[test]
    fn consistency_true_on_a_satisfiable_abox() {
        let dataset = consistent_dataset();
        let reasoner = DlReasoner::new(&dataset).expect("reverse-map the consistent ontology");
        let certified = reasoner.consistency();
        assert_eq!(certified.answer, Verdict::True);
        // The whole ontology was read and every search finished: an exact answer.
        assert!(certified.is_decided());
        assert!(certified.is_exact());
        assert!(certified.boundaries.is_empty());
    }

    #[test]
    fn consistency_false_on_a_disjointness_violation() {
        let dataset = inconsistent_dataset();
        let reasoner = DlReasoner::new(&dataset).expect("reverse-map the inconsistent ontology");
        let certified = reasoner.consistency();
        // Detected, not errored: consistency is the one service that reports an
        // unsatisfiable ontology as a verdict rather than as a refusal.
        assert_eq!(certified.answer, Verdict::False);
        assert!(certified.is_decided());
    }

    #[test]
    fn entailed_subsumption_is_certified_true() {
        let dataset = consistent_dataset();
        let mut reasoner = DlReasoner::new(&dataset).expect("reverse-map");
        let axiom = DlAxiom::ClassAssertion {
            individual: TermValue::iri(TOM),
            class: TermValue::iri(ANIMAL),
        };
        // `tom a Animal` is not asserted but IS entailed through `Cat ⊑ Animal`.
        let certified = reasoner.entails(&axiom).expect("consistent ontology");
        assert_eq!(certified.answer, Verdict::True);
        assert!(certified.is_exact());
    }

    #[test]
    fn class_satisfiability_on_the_unsatisfiable_ontology_is_an_error() {
        let dataset = inconsistent_dataset();
        let mut reasoner = DlReasoner::new(&dataset).expect("reverse-map");
        // Every class is vacuously unsatisfiable in an ontology with no model, so
        // the service refuses rather than answering — and the refusal is a typed
        // reasoning-core diagnostic, not a foreign error.
        let error = reasoner
            .class_satisfiability(&TermValue::iri(CAT))
            .expect_err("an unsatisfiable ontology has no meaningful class answer");
        assert!(
            error.to_string().contains("purrdf entail service refused"),
            "unexpected diagnostic text: {error}"
        );
    }

    #[test]
    fn a_narrowed_step_cap_reports_unknown_rather_than_a_fabricated_verdict() {
        let dataset = consistent_dataset();

        // Under the size-derived ceiling the same ontology is DECIDED exactly: this is
        // the control that makes the exhausted arm below falsifiable — the ontology is
        // trivially consistent, so a non-conclusion can only come from the ceiling, not
        // from the input being genuinely undecidable.
        let decided = DlReasoner::new(&dataset)
            .expect("reverse-map the consistent ontology")
            .consistency();
        assert_eq!(decided.answer, Verdict::True);
        assert!(decided.is_exact());
        assert_eq!(decided.completeness, DlCompleteness::Decided);

        // Now narrow the per-decision step cap to one round — the one ceiling this
        // repository can move, and only downward. One round decides nothing, so the
        // hypertableau search must exhaust.
        let starved =
            DlReasoner::with_step_cap(&dataset, 1).expect("reverse-map under a narrowed step cap");
        assert_eq!(starved.step_cap(), 1, "the cap was narrowed to one round");

        let answer = starved.consistency();

        // The honest contract: exhaustion is a NON-CONCLUSION, never a fabricated
        // verdict and never a panic. A boolean service reports `Unknown`, the
        // certificate reports `BudgetExhausted`, and both completeness predicates read
        // `false` — the answer is not presented as if it were decided or exact.
        assert_eq!(
            answer.answer,
            Verdict::Unknown,
            "an exhausted search is Unknown, never True/False as if decided"
        );
        assert_ne!(answer.answer, Verdict::True);
        assert_ne!(answer.answer, Verdict::False);
        assert_eq!(answer.completeness, DlCompleteness::BudgetExhausted);
        assert!(!answer.is_decided());
        assert!(!answer.is_exact());
    }

    #[test]
    fn profile_certifies_a_bare_subclass_ontology_everywhere() {
        let dataset = consistent_dataset();
        let certificate = certify_profiles(&dataset);
        // A bare sub-class axiom plus a class assertion is in every OWL 2 profile.
        assert_eq!(certificate.certified(), OwlProfile::ALL.to_vec());
        assert!(certificate.violations().is_empty());
        // Full is certified unconditionally: every RDF graph is an OWL 2 Full ontology.
        assert!(certificate.certifies(OwlProfile::Full));
    }
}
