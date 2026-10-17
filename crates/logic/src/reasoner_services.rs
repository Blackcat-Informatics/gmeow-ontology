// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW diagnostic adaptation of PurRDF's native DL services.
//!
//! Every service returns the upstream [`Certified`] value unchanged: its answer,
//! complete [`purrdf::entail::DlCertificate`] and recorded
//! [`purrdf::entail::ServiceProof`] remain together. GMEOW owns no copied
//! certificate or decision engine at this boundary.
//!
//! Refusals become [`crate::error::DlService`] diagnostics without flattening
//! original causes. No-model, malformed-input and execution failures stay distinct.
//! Consistency reports no-model as [`Verdict::False`]; exhausted or stopped work
//! remains [`Verdict::Unknown`] with the original native certificate.
//!
//! Construction explicitly selects recording through [`DlReasoner::with_proofs`]
//! or unrecorded services through [`DlReasoner::new`]. An absent proof is never
//! fabricated as an empty trace. All upstream limits and observations remain on
//! the native result; the step-cap constructor only narrows the native ceiling.

use gmeow_errors::Result;
use purrdf::entail::{
    CertainAnswers, Certified, ClassHierarchy, CombinedMaterialization, DlAxiom, EntailError,
    ImportMap, ModuleExtraction, ModuleMethod, ProfileCertificate, QTriple, Question, Realization,
    Reasoner, Regime, Verdict,
};
use purrdf::{RdfDataset, TermValue};

/// Stable disposition of a refused OWL Direct-Semantics service operation.
///
/// The category is deliberately separate from rendered diagnostics. In particular,
/// an unsatisfiable ontology is a decided negative result for classification and
/// realization, while malformed reverse-mapping input and an engine failure are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DlServiceFailureKind {
    /// The ontology has no model, so the requested model-relative product is undefined.
    NoModel,
    /// The RDF graph cannot be admitted as a well-formed OWL Direct-Semantics input.
    Malformed,
    /// The selected engine failed to execute or exhausted a construction resource.
    Execution,
}

/// Attach an original native entailment refusal to the sole diagnostic substrate.
/// Logical no-model, malformed source and physical execution remain distinct.
fn map_entail_err(error: EntailError) -> gmeow_errors::Diag {
    let kind = match &error {
        EntailError::Unsatisfiable => DlServiceFailureKind::NoModel,
        EntailError::Parse(_) | EntailError::MalformedList(_) => DlServiceFailureKind::Malformed,
        EntailError::RoleHierarchy(role) => {
            use purrdf::entail::RoleHierarchyError;
            match role.classification() {
                RoleHierarchyError::Storage(_)
                | RoleHierarchyError::Graph(_)
                | RoleHierarchyError::Stopped => DlServiceFailureKind::Execution,
                _ => DlServiceFailureKind::Malformed,
            }
        }
        _ => DlServiceFailureKind::Execution,
    };
    gmeow_errors::Diag::of_kind(crate::error::DlService {
        kind,
        cause: std::sync::Arc::new(error),
    })
}

/// The OWL 2 Direct-Semantics reasoning services over one [`RdfDataset`], each
/// answering with the unchanged native [`Certified`] value.
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
    /// A typed [`crate::error::DlService`] if reverse mapping sees malformed input, execution
    /// fails, or an `owl:hasKey` axiom proves the ontology has no model.
    pub fn new(dataset: &RdfDataset) -> Result<Self> {
        Reasoner::new(dataset)
            .map(|inner| Self { inner })
            .map_err(map_entail_err)
    }

    /// Record native proofs while retaining the complete upstream certificate.
    ///
    /// # Errors
    /// Original source, execution or canonicalization refusals, as typed diagnostics.
    pub fn with_proofs(dataset: &RdfDataset) -> Result<Self> {
        Reasoner::with_proofs(dataset)
            .map(|inner| Self { inner })
            .map_err(map_entail_err)
    }

    /// Prepare the exact checking question without performing another solver decision.
    ///
    /// # Errors
    /// Original question preparation refusals, retained as typed diagnostics.
    pub fn prepare(&mut self, question: &Question) -> Result<()> {
        self.inner.prepare(question).map_err(map_entail_err)
    }

    /// Consume this session into the native context for checking its selected question.
    /// Call [`Self::prepare`] with that question before consuming the session.
    ///
    /// # Errors
    /// A typed execution refusal if the session did not record proofs.
    pub fn proof_context(self) -> Result<purrdf::entail::DlProofContext> {
        self.inner.proof_context().map_err(map_entail_err)
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
    /// [`Verdict::Unknown`], which the [`Certified`] carries rather than
    /// collapsing to `False`.
    #[must_use]
    pub fn consistency(&self) -> Certified<Verdict> {
        self.inner.consistency()
    }

    /// Whether `class` can have an instance in some model of the ontology.
    ///
    /// # Errors
    ///
    /// [`crate::error::DlService`] with [`DlServiceFailureKind::NoModel`] if the ontology has
    /// no model at all — every class is then vacuously unsatisfiable and the answer
    /// would say nothing.
    pub fn class_satisfiability(&mut self, class: &TermValue) -> Result<Certified<Verdict>> {
        self.inner
            .class_satisfiability(class)
            .map_err(map_entail_err)
    }

    /// The subsumption hierarchy over the ontology's named classes.
    ///
    /// # Errors
    ///
    /// [`crate::error::DlService`] with [`DlServiceFailureKind::NoModel`] if the ontology has
    /// no model.
    pub fn classify(&self) -> Result<Certified<ClassHierarchy>> {
        self.inner.classify().map_err(map_entail_err)
    }

    /// The entailed types of the ontology's named individuals, and the most specific
    /// of them.
    ///
    /// # Errors
    ///
    /// [`crate::error::DlService`] with [`DlServiceFailureKind::NoModel`] if the ontology has
    /// no model.
    pub fn realize(&self) -> Result<Certified<Realization>> {
        self.inner.realize().map_err(map_entail_err)
    }

    /// The named individuals entailed to be instances of `class`, sorted.
    ///
    /// # Errors
    ///
    /// [`crate::error::DlService`] with [`DlServiceFailureKind::NoModel`] if the ontology has
    /// no model.
    pub fn instances(&mut self, class: &TermValue) -> Result<Certified<Vec<TermValue>>> {
        self.inner.instances(class).map_err(map_entail_err)
    }

    /// Whether the ontology entails `axiom`, decided by refutation.
    ///
    /// # Errors
    ///
    /// [`crate::error::DlService`] with [`DlServiceFailureKind::NoModel`] if the ontology has
    /// no model, in which case every axiom is entailed and the answer would be
    /// worthless.
    pub fn entails(&mut self, axiom: &DlAxiom) -> Result<Certified<Verdict>> {
        self.inner.entails(axiom).map_err(map_entail_err)
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
/// A [`DlService`](crate::error::DlService) diagnostic if the extracted module cannot be
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
/// A [`DlService`](crate::error::DlService) diagnostic propagating any purrdf refusal from
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
/// A [`DlService`](crate::error::DlService) diagnostic for a regime the service is not
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

#[path = "reasoner_services.tests.rs"]
#[cfg(test)]
mod tests;
