// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native backward-query dispatch.
//!
//! `dispatch_query` is the single production entry point for goal resolution. It
//! applies the profile gates and delegates to the native demand-transformed physical
//! core. A fragment the native core cannot soundly decide is a typed hard failure:
//! there is no secondary engine, silent approximation, or demotion route.

use crate::annotation::{
    AnnotatedAnswerSet, AnnotationContract, AnnotationFactRef, AnnotationRequest,
    TupleAnnotationAlgebra,
};
use crate::external_relation::{
    QueryRelationProviders, RelationContractError, RelationExecution, RelationExecutionError,
    RelationExecutionFailureKind, RelationQueryFailureReceipt, RelationQueryResult,
};
use crate::profile_gate;
use crate::query_ir::{AnswerSet, Budget, QProgram};
use crate::seam::{RdfViewFactSource, WorldFactSource, WorldSourceIdentity, WorldSourceMetrics};
use purrdf::{DatasetView, FallibleDatasetView, ViewOperationStatus};

/// Stable identities under which a view-backed answer was computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryExecutionIdentity {
    /// Immutable RDF source generation and provider contract.
    pub source: WorldSourceIdentity,
    /// Content identity of the compiled GMEOW engine.
    pub engine_descriptor_hash: String,
    /// Content identity of this query's profile and resource contract.
    pub query_contract_hash: String,
}

impl QueryExecutionIdentity {
    fn current(source: WorldSourceIdentity, profile: &str, budget: &Budget) -> Self {
        Self::for_contract(source, query_contract_hash(profile, budget))
    }

    pub(crate) fn for_contract(source: WorldSourceIdentity, query_contract_hash: String) -> Self {
        Self {
            source,
            engine_descriptor_hash: crate::runtime::EngineContract::current().descriptor_hash,
            query_contract_hash,
        }
    }
}

/// Combined backend and logic-source evidence for one view-backed execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryExecutionEvidence<BackendEvidence> {
    /// Backend-specific evidence, such as PurRDF page/byte/generation accounting.
    pub backend: BackendEvidence,
    /// Deterministic number of pushed patterns and RDF rows delivered to logic.
    pub source: WorldSourceMetrics,
}

/// Evidence for an infallible resident or validated succinct-pack view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidentViewEvidence {
    /// View-reported quad cardinality, when known without enumeration.
    pub len_hint: Option<usize>,
    /// View-reported deterministic statistics discriminator.
    pub stats_fingerprint: u64,
}

/// A view-backed answer certified complete under its source and engine identities.
#[derive(Debug, Clone)]
pub struct CompleteViewQuery<BackendEvidence> {
    /// Complete, dataset-independent GMEOW answer set.
    pub answer: AnswerSet,
    /// Backend and source-access evidence captured after result materialization.
    pub evidence: QueryExecutionEvidence<BackendEvidence>,
    /// Source generation plus engine and per-query contract identities.
    pub identity: QueryExecutionIdentity,
}

/// A view-backed annotated answer certified complete under source and engine identities.
#[derive(Debug, Clone)]
pub struct CompleteAnnotatedViewQuery<Element, BackendEvidence> {
    /// Complete score-carrying GMEOW answer set with direct source lineage.
    pub answer: AnnotatedAnswerSet<Element>,
    /// Backend and source-access evidence captured after result materialization.
    pub evidence: QueryExecutionEvidence<BackendEvidence>,
    /// Source generation plus engine and annotated-query contract identities.
    pub identity: QueryExecutionIdentity,
}

/// A view-backed provider-aware answer certified complete under every source identity.
#[derive(Debug, Clone)]
pub struct CompleteRelationViewQuery<Element, BackendEvidence> {
    /// Native annotated answer and provider operational receipt.
    pub result: RelationQueryResult<Element>,
    /// Backend and RDF source-access evidence captured after result materialization.
    pub evidence: QueryExecutionEvidence<BackendEvidence>,
    /// RDF source, engine, and provider-aware query identities.
    pub identity: QueryExecutionIdentity,
}

/// Annotation algebra and immutable provider set for one query execution.
///
/// Grouping the two related inputs makes the query-local provider boundary explicit:
/// the provider set is borrowed by one native evaluation and cannot escape into a
/// process-wide registry or an implicit fallback path.
pub struct RelationAnnotationRequest<'query, 'provider, A, F>
where
    A: TupleAnnotationAlgebra,
{
    /// Algebra, admission contract, and asserted-RDF annotation source.
    pub annotation: AnnotationRequest<'query, A, F>,
    /// Immutable external relations available to this query only.
    pub providers: &'query QueryRelationProviders<'provider, A::Element>,
}

impl<'query, 'provider, A, F> RelationAnnotationRequest<'query, 'provider, A, F>
where
    A: TupleAnnotationAlgebra,
{
    /// Bundle annotation and provider inputs for one native evaluation.
    #[must_use]
    pub const fn new(
        annotation: AnnotationRequest<'query, A, F>,
        providers: &'query QueryRelationProviders<'provider, A::Element>,
    ) -> Self {
        Self {
            annotation,
            providers,
        }
    }
}

/// Typed non-result from provider-aware annotated dispatch.
#[derive(Debug)]
pub enum RelationQueryError {
    /// The immutable provider set and selected annotation algebra disagree.
    Contract(RelationContractError),
    /// Profile or native evaluation refused while retaining provider attempt evidence.
    Query {
        /// Ordinary GMEOW diagnostic.
        diagnostic: gmeow_errors::Diag,
        /// Content-addressed failed-operation receipt.
        receipt: Box<RelationQueryFailureReceipt>,
    },
    /// A provider failed, was incomplete/cancelled, exhausted its governor, or broke contract.
    Provider {
        /// Typed provider/executor terminal state.
        error: RelationExecutionError,
        /// Content-addressed failed-operation receipt.
        receipt: Box<RelationQueryFailureReceipt>,
    },
}

impl RelationQueryError {
    /// Failed-operation evidence, when execution began.
    #[must_use]
    pub fn receipt(&self) -> Option<&RelationQueryFailureReceipt> {
        match self {
            Self::Contract(_) => None,
            Self::Query { receipt, .. } | Self::Provider { receipt, .. } => Some(receipt.as_ref()),
        }
    }
}

impl std::fmt::Display for RelationQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(error) => error.fmt(formatter),
            Self::Query { diagnostic, .. } => diagnostic.fmt(formatter),
            Self::Provider { error, .. } => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RelationQueryError {}

impl<BackendEvidence> CompleteViewQuery<BackendEvidence> {
    /// Decompose the completeness certificate.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        AnswerSet,
        QueryExecutionEvidence<BackendEvidence>,
        QueryExecutionIdentity,
    ) {
        (self.answer, self.evidence, self.identity)
    }
}

/// Public return type for dispatch over an operationally fallible RDF view.
pub type FallibleViewQueryResult<OperationalError, BackendEvidence> = Result<
    CompleteViewQuery<BackendEvidence>,
    Box<FallibleViewQueryError<OperationalError, BackendEvidence>>,
>;

/// Public return type for annotated dispatch over an operationally fallible RDF view.
pub type FallibleAnnotatedViewQueryResult<Element, OperationalError, BackendEvidence> = Result<
    CompleteAnnotatedViewQuery<Element, BackendEvidence>,
    Box<FallibleViewQueryError<OperationalError, BackendEvidence>>,
>;

/// Public return type for provider-aware annotated dispatch over a fallible RDF view.
pub type FallibleRelationViewQueryResult<Element, OperationalError, BackendEvidence> = Result<
    CompleteRelationViewQuery<Element, BackendEvidence>,
    Box<FallibleRelationViewQueryError<OperationalError, BackendEvidence>>,
>;

/// Three-way failure boundary for provider-aware dispatch over a fallible RDF view.
#[derive(Debug)]
pub enum FallibleRelationViewQueryError<OperationalError, BackendEvidence> {
    /// Profile/native/provider evaluation failed while the RDF view remained ready.
    Relation {
        /// Typed query/provider non-result.
        error: RelationQueryError,
        /// Backend and source-access evidence at the final ready checkpoint.
        evidence: QueryExecutionEvidence<BackendEvidence>,
        /// Source and engine identities for the failed attempt.
        identity: QueryExecutionIdentity,
    },
    /// Lazy RDF access failed and takes precedence over any internal partial result.
    Operational {
        /// Sticky typed RDF provider/budget/cancellation/generation error.
        error: OperationalError,
        /// Backend and source-access evidence at the failure boundary.
        evidence: QueryExecutionEvidence<BackendEvidence>,
        /// Source and engine identities for the failed attempt.
        identity: QueryExecutionIdentity,
    },
}

impl<OperationalError, BackendEvidence>
    FallibleRelationViewQueryError<OperationalError, BackendEvidence>
{
    /// Borrow the evidence carried by either terminal variant.
    #[must_use]
    pub const fn evidence(&self) -> &QueryExecutionEvidence<BackendEvidence> {
        match self {
            Self::Relation { evidence, .. } | Self::Operational { evidence, .. } => evidence,
        }
    }

    /// Borrow the RDF operational root cause, when one occurred.
    #[must_use]
    pub const fn operational_error(&self) -> Option<&OperationalError> {
        match self {
            Self::Relation { .. } => None,
            Self::Operational { error, .. } => Some(error),
        }
    }

    /// Borrow the provider-aware query error, when the RDF view remained ready.
    #[must_use]
    pub const fn relation_error(&self) -> Option<&RelationQueryError> {
        match self {
            Self::Relation { error, .. } => Some(error),
            Self::Operational { .. } => None,
        }
    }

    /// Borrow the execution identities carried by either terminal variant.
    #[must_use]
    pub const fn identity(&self) -> &QueryExecutionIdentity {
        match self {
            Self::Relation { identity, .. } | Self::Operational { identity, .. } => identity,
        }
    }
}

impl<OperationalError: std::fmt::Display, BackendEvidence> std::fmt::Display
    for FallibleRelationViewQueryError<OperationalError, BackendEvidence>
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Relation { error, .. } => error.fmt(formatter),
            Self::Operational { error, .. } => {
                write!(formatter, "operational RDF query failure: {error}")
            }
        }
    }
}

/// Failure of GMEOW dispatch over an operationally fallible RDF view.
#[derive(Debug)]
pub enum FallibleViewQueryError<OperationalError, BackendEvidence> {
    /// Query/profile/native evaluation failed while the RDF view remained ready.
    Query {
        /// Ordinary GMEOW diagnostic.
        diagnostic: gmeow_errors::Diag,
        /// Backend and source-access evidence at the final ready checkpoint.
        evidence: QueryExecutionEvidence<BackendEvidence>,
        /// Source and engine identities for the failed attempt.
        identity: QueryExecutionIdentity,
    },
    /// Lazy RDF access failed. This takes precedence over an evaluator diagnostic.
    Operational {
        /// Sticky typed provider, budget, cancellation, deadline, or generation error.
        error: OperationalError,
        /// Backend and source-access evidence at the failure boundary.
        evidence: QueryExecutionEvidence<BackendEvidence>,
        /// Source and engine identities for the failed attempt.
        identity: QueryExecutionIdentity,
    },
}

impl<OperationalError, BackendEvidence> FallibleViewQueryError<OperationalError, BackendEvidence> {
    /// Borrow the evidence carried by either failure variant.
    #[must_use]
    pub const fn evidence(&self) -> &QueryExecutionEvidence<BackendEvidence> {
        match self {
            Self::Query { evidence, .. } | Self::Operational { evidence, .. } => evidence,
        }
    }

    /// Borrow the operational root cause, when lazy RDF access failed.
    #[must_use]
    pub const fn operational_error(&self) -> Option<&OperationalError> {
        match self {
            Self::Query { .. } => None,
            Self::Operational { error, .. } => Some(error),
        }
    }

    /// Borrow the ordinary query diagnostic, when the view remained ready.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&gmeow_errors::Diag> {
        match self {
            Self::Query { diagnostic, .. } => Some(diagnostic),
            Self::Operational { .. } => None,
        }
    }

    /// Borrow the source and engine identities carried by the failed attempt.
    #[must_use]
    pub const fn identity(&self) -> &QueryExecutionIdentity {
        match self {
            Self::Query { identity, .. } | Self::Operational { identity, .. } => identity,
        }
    }
}

impl<OperationalError: std::fmt::Display, BackendEvidence> std::fmt::Display
    for FallibleViewQueryError<OperationalError, BackendEvidence>
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Query { diagnostic, .. } => diagnostic.fmt(formatter),
            Self::Operational { error, .. } => {
                write!(formatter, "operational RDF query failure: {error}")
            }
        }
    }
}

/// Content identity of the backward execution contract available at this boundary.
///
/// The rule program has its own canonical digest in the physical plan key. This digest
/// covers the remaining semantics/resource inputs that can change dispatch behavior,
/// with explicit option tags so `None` cannot alias a numeric zero.
///
/// `pub(crate)` so the stable runtime façade ([`crate::runtime::EngineContract`]) can
/// surface the per-query contract from THIS single source — a runtime consumer that
/// records "answer minted under contract Y" reproduces the same Y, and there is never a
/// second copy of this hash to drift.
pub(crate) fn query_contract_hash(profile: &str, budget: &Budget) -> String {
    fn frame(hasher: &mut blake3::Hasher, value: &[u8]) {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value);
    }

    let mut hasher = blake3::Hasher::new();
    frame(&mut hasher, b"gmeow-backward-query-contract-v1");
    let canonical_profile = profile_gate::canonical_profile_identity(profile);
    frame(&mut hasher, canonical_profile.as_bytes());
    match budget.max_answers {
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(&(value as u64).to_le_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
    match budget.max_steps {
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(&value.to_le_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
    hasher.finalize().to_hex().to_string()
}

pub(crate) fn annotated_query_contract_hash(
    profile: &str,
    budget: &Budget,
    annotation: &AnnotationContract,
    algebra_identity: &str,
) -> String {
    let base_contract = query_contract_hash(profile, budget);
    let annotation_frame = annotation.canonical_key();
    let mut hasher = blake3::Hasher::new();
    for value in [
        "gmeow-annotated-query-contract-v2",
        base_contract.as_str(),
        annotation_frame.as_str(),
        algebra_identity,
    ] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

pub(crate) fn external_relation_query_contract_hash(
    profile: &str,
    budget: &Budget,
    annotation: &AnnotationContract,
    algebra_identity: &str,
    provider_manifest_hash: &str,
) -> String {
    let annotated = annotated_query_contract_hash(profile, budget, annotation, algebra_identity);
    let mut hasher = blake3::Hasher::new();
    for value in [
        "gmeow-external-relation-query-contract-v1",
        annotated.as_str(),
        provider_manifest_hash,
    ] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Resolve `program` against `world` with the native physical core.
///
/// # Errors
///
/// Returns `Err` from a profile gate, from the native engine, or when the native
/// engine reports an unsupported fragment. Unsupported never means an empty answer.
pub fn dispatch_query(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
) -> gmeow_errors::Result<AnswerSet> {
    profile_gate::reject_cut(program)?;
    profile_gate::check_builtin_profile(program, profile)?;

    let contract_hash = query_contract_hash(profile, budget);
    // A parsed production program is flat (the parser never mints a `Struct` term), so the
    // structured routing inside `resolve_native_under` is not taken and this fresh arena is
    // unused; it is present so the structured entry has a valid owning arena to thread.
    let mut dag = gmeow_term_arena::engine::TermDag::new();
    match crate::physical::resolve_native_under(
        &contract_hash,
        foreign,
        world,
        program,
        budget,
        &mut dag,
    )? {
        crate::physical::NativeOutcome::Decided(answer) => Ok(answer),
        crate::physical::NativeOutcome::Unsupported(kind) => Err(refuse_native_gap(&kind)),
    }
}

/// Build the typed refusal diagnostic for a native declared gap.
///
/// A moded-builtin gap ([`UnsupportedKind::Arithmetic`] carrying captured
/// [`BuiltinGap`](crate::physical::BuiltinGap)s) is NOT an anonymous "does not support
/// Arithmetic": it is minted into a [`gmeow_errors::DiagLedger`] of ledgered per-kind
/// findings (distinct `finding_iri`/`anchor_iri` per `math:` failure class) via the
/// single shared [`crate::reason::builtin_gap`] helper, and the returned diagnostic
/// NAMES each gap's `math:` class + operation, with the ledgered findings' identity
/// hung off it as antecedent quad IRIs. A structural refusal (empty gaps) or any other
/// kind keeps the plain typed-refusal message.
fn refuse_native_gap(kind: &crate::physical::UnsupportedKind) -> gmeow_errors::Diag {
    if let crate::physical::UnsupportedKind::Arithmetic(gaps) = kind
        && !gaps.is_empty()
    {
        let ledger = crate::reason::builtin_gap::builtin_gap_ledger(gaps);
        // The ledgered per-kind finding IRIs, carried as explain-skeleton citations so the
        // refusal is joinable to the distinct findings the ledger projected.
        let finding_iris: Vec<String> = ledger
            .findings("reason")
            .into_iter()
            .filter_map(|f| f.finding_iri)
            .collect();
        return gmeow_errors::Diag::of_kind(crate::error::Reason {
            detail: crate::reason::builtin_gap::builtin_gap_refusal_detail(gaps),
        })
        .with_derived_from_quads(finding_iris);
    }
    // A structural refusal (an empty-gap `Arithmetic`, or any other declared kind) keeps
    // the plain typed-refusal message. `Arithmetic` renders WITHOUT its (empty) gap
    // payload so the label stays the stable `Arithmetic`, never `Arithmetic([])`.
    let label = match kind {
        crate::physical::UnsupportedKind::Arithmetic(_) => "Arithmetic".to_owned(),
        other => format!("{other:?}"),
    };
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: format!(
            "native backward engine does not support {label}; query refused because \
             no fallback engine remains"
        ),
    })
}

/// Resolve directly over an infallible resident or validated succinct-pack RDF view.
///
/// Only rows selected by the compiled query's source patterns are converted to
/// [`crate::seam::DerivedQuad`]; the complete dataset is never copied into a
/// [`crate::store::WorldStore`] or [`crate::seam::WorldFactSnapshot`].
///
/// # Errors
///
/// Returns an ordinary GMEOW diagnostic for profile, source-contract, or native
/// evaluation failure. Unsupported source/evaluator capabilities are refused rather
/// than emulated by whole-view materialization.
pub fn dispatch_query_view<V: DatasetView>(
    view: &V,
    source_identity: WorldSourceIdentity,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
) -> gmeow_errors::Result<CompleteViewQuery<ResidentViewEvidence>> {
    let identity = QueryExecutionIdentity::current(source_identity, profile, budget);
    let source = RdfViewFactSource::new(view, profile, identity.source.clone());
    let answer = dispatch_query(&source, world, program, profile, budget)?;
    Ok(CompleteViewQuery {
        answer,
        evidence: QueryExecutionEvidence {
            backend: ResidentViewEvidence {
                len_hint: view.len_hint(),
                stats_fingerprint: view.stats_fingerprint(),
            },
            source: source.metrics(),
        },
        identity,
    })
}

/// Resolve directly over an operationally fallible RDF view.
///
/// The view is checked before execution and after all answer materialization. A
/// provider failure is sticky and takes precedence over any diagnostic produced from
/// the now-incomplete internal rows; partial answers never cross this boundary.
/// PurRDF's typed provider/budget/cancellation/deadline/stale-generation error and
/// exact request evidence remain intact in the generic result.
pub fn dispatch_query_fallible_view<V>(
    view: &V,
    source_identity: WorldSourceIdentity,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
) -> FallibleViewQueryResult<V::Error, V::Evidence>
where
    V: FallibleDatasetView,
{
    let identity = QueryExecutionIdentity::current(source_identity, profile, budget);
    if let ViewOperationStatus::Failed { error, evidence } = view.operation_status() {
        return Err(Box::new(FallibleViewQueryError::Operational {
            error,
            evidence: QueryExecutionEvidence {
                backend: evidence,
                source: WorldSourceMetrics::default(),
            },
            identity,
        }));
    }

    let source = RdfViewFactSource::new(view, profile, identity.source.clone());
    let evaluation = dispatch_query(&source, world, program, profile, budget);
    let source_metrics = source.metrics();
    match view.operation_status() {
        ViewOperationStatus::Failed { error, evidence } => {
            Err(Box::new(FallibleViewQueryError::Operational {
                error,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            }))
        }
        ViewOperationStatus::Ready { evidence } => match evaluation {
            Ok(answer) => Ok(CompleteViewQuery {
                answer,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            }),
            Err(diagnostic) => Err(Box::new(FallibleViewQueryError::Query {
                diagnostic,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            })),
        },
    }
}

/// Resolve `program` while carrying opaque annotations through native derivations.
///
/// `annotation_for` is consulted for asserted world facts. Returning `None` assigns
/// `algebra.one()`. Body conjunction uses `multiply`; alternative derivations use
/// `add`; each answer exposes the combined value plus its direct derivation lineage.
/// The annotation contract is independently content-framed into the plan identity, so
/// an exact-semiring call cannot alias a declared approximation call.
///
/// # Errors
///
/// Returns `Err` for a profile/fragment refusal, an annotation contract mismatch, an
/// algebra failure, or a non-convergent annotation fixed point. Annotation dispatch is
/// currently the native binary positive-Datalog seam; unsupported n-ary or negated
/// programs hard-fail rather than silently losing scores.
pub fn dispatch_query_annotated<A, F>(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    annotation: AnnotationRequest<'_, A, F>,
) -> gmeow_errors::Result<AnnotatedAnswerSet<A::Element>>
where
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    profile_gate::reject_cut(program)?;
    profile_gate::check_builtin_profile(program, profile)?;

    let contract_hash = annotated_query_contract_hash(
        profile,
        budget,
        annotation.contract,
        annotation.algebra.identity(),
    );
    match crate::physical::resolve_native_annotated_under(
        &contract_hash,
        foreign,
        world,
        program,
        budget,
        &annotation,
    )? {
        crate::physical::NativeOutcome::Decided(answer) => Ok(answer),
        crate::physical::NativeOutcome::Unsupported(kind) => {
            Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!(
                    "native annotated backward engine does not support {kind:?}; query refused because annotations cannot be demoted to post-hoc scoring"
                ),
            }))
        }
    }
}

fn provider_terminal_wire(kind: &RelationExecutionFailureKind) -> &'static str {
    match kind {
        RelationExecutionFailureKind::ProviderFailure(_) => "provider-failure",
        RelationExecutionFailureKind::ProviderIncomplete(_) => "provider-incomplete",
        RelationExecutionFailureKind::BudgetExhausted => "provider-budget-exhausted",
        RelationExecutionFailureKind::Cancelled => "provider-cancelled",
        RelationExecutionFailureKind::ContractViolation => "provider-contract-violation",
    }
}

/// Resolve an annotated query with immutable operation-scoped external relations.
///
/// Provider atoms execute as typed EDB operators inside the native arity-generic
/// fixpoint. Returned tuples are not inserted into the RDF source and never cross into a
/// scratch world. Every non-result retains typed attempt evidence; provider failure or
/// incompleteness is never presented as a complete empty relation.
pub fn dispatch_query_annotated_with_relations<A, F>(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    request: RelationAnnotationRequest<'_, '_, A, F>,
) -> Result<RelationQueryResult<A::Element>, RelationQueryError>
where
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    let RelationAnnotationRequest {
        annotation,
        providers,
    } = request;
    let contract_hash = external_relation_query_contract_hash(
        profile,
        budget,
        annotation.contract,
        annotation.algebra.identity(),
        providers.manifest_hash(),
    );
    let mut execution = RelationExecution::new(providers, annotation.algebra, &contract_hash)
        .map_err(RelationQueryError::Contract)?;
    let source = foreign.identity().clone();
    let engine_descriptor_hash = crate::runtime::EngineContract::current().descriptor_hash;

    for gate in [
        profile_gate::reject_cut(program),
        profile_gate::check_builtin_profile(program, profile),
    ] {
        if let Err(diagnostic) = gate {
            let receipt = execution.failure_receipt(
                source.clone(),
                engine_descriptor_hash.clone(),
                "query-gate",
                diagnostic.to_string(),
            );
            return Err(RelationQueryError::Query {
                diagnostic,
                receipt: Box::new(receipt),
            });
        }
    }

    match crate::physical::resolve_native_annotated_with_relations_under(
        foreign,
        world,
        program,
        budget,
        &annotation,
        &mut execution,
    ) {
        Ok(crate::physical::NativeOutcome::Decided(answer)) => {
            Ok(execution.finish(answer, source, engine_descriptor_hash))
        }
        Ok(crate::physical::NativeOutcome::Unsupported(kind)) => {
            let diagnostic = gmeow_errors::Diag::of_kind(crate::error::Reason {
                detail: format!(
                    "native provider-aware annotated engine does not support {kind:?}; query refused because provider tuples cannot be materialized or scores demoted"
                ),
            });
            let receipt = execution.failure_receipt(
                source,
                engine_descriptor_hash,
                "query-unsupported",
                diagnostic.to_string(),
            );
            Err(RelationQueryError::Query {
                diagnostic,
                receipt: Box::new(receipt),
            })
        }
        Err(crate::physical::ExternalRelationEvaluationError::Query(diagnostic)) => {
            let receipt = execution.failure_receipt(
                source,
                engine_descriptor_hash,
                "query-diagnostic",
                diagnostic.to_string(),
            );
            Err(RelationQueryError::Query {
                diagnostic,
                receipt: Box::new(receipt),
            })
        }
        Err(crate::physical::ExternalRelationEvaluationError::Provider(error)) => {
            let receipt = execution.failure_receipt(
                source,
                engine_descriptor_hash,
                provider_terminal_wire(&error.kind),
                error.to_string(),
            );
            Err(RelationQueryError::Provider {
                error,
                receipt: Box::new(receipt),
            })
        }
    }
}

/// Resolve an annotated query directly over a resident or succinct-pack RDF view.
///
/// The asserted-fact callback observes the owned RDF 1.2 values admitted through
/// the view source. Derived answers retain their direct source tuple keys; no
/// snapshot or post-hoc score join is introduced.
///
/// # Errors
///
/// Returns an ordinary GMEOW diagnostic for profile, annotation-contract,
/// source-contract, or native evaluation failure.
pub fn dispatch_query_annotated_view<V, A, F>(
    view: &V,
    source_identity: WorldSourceIdentity,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    annotation: AnnotationRequest<'_, A, F>,
) -> gmeow_errors::Result<CompleteAnnotatedViewQuery<A::Element, ResidentViewEvidence>>
where
    V: DatasetView,
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    let identity = QueryExecutionIdentity::for_contract(
        source_identity,
        annotated_query_contract_hash(
            profile,
            budget,
            annotation.contract,
            annotation.algebra.identity(),
        ),
    );
    let source = RdfViewFactSource::new(view, profile, identity.source.clone());
    let answer = dispatch_query_annotated(&source, world, program, profile, budget, annotation)?;
    Ok(CompleteAnnotatedViewQuery {
        answer,
        evidence: QueryExecutionEvidence {
            backend: ResidentViewEvidence {
                len_hint: view.len_hint(),
                stats_fingerprint: view.stats_fingerprint(),
            },
            source: source.metrics(),
        },
        identity,
    })
}

/// Resolve an annotated query directly over an operationally fallible RDF view.
///
/// The same preflight/final completeness checkpoints as ordinary fallible dispatch
/// apply. A provider failure wins over an internal annotation diagnostic and no
/// partial answer or partial annotation crosses the boundary.
pub fn dispatch_query_annotated_fallible_view<V, A, F>(
    view: &V,
    source_identity: WorldSourceIdentity,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    annotation: AnnotationRequest<'_, A, F>,
) -> FallibleAnnotatedViewQueryResult<A::Element, V::Error, V::Evidence>
where
    V: FallibleDatasetView,
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    let identity = QueryExecutionIdentity::for_contract(
        source_identity,
        annotated_query_contract_hash(
            profile,
            budget,
            annotation.contract,
            annotation.algebra.identity(),
        ),
    );
    if let ViewOperationStatus::Failed { error, evidence } = view.operation_status() {
        return Err(Box::new(FallibleViewQueryError::Operational {
            error,
            evidence: QueryExecutionEvidence {
                backend: evidence,
                source: WorldSourceMetrics::default(),
            },
            identity,
        }));
    }

    let source = RdfViewFactSource::new(view, profile, identity.source.clone());
    let evaluation = dispatch_query_annotated(&source, world, program, profile, budget, annotation);
    let source_metrics = source.metrics();
    match view.operation_status() {
        ViewOperationStatus::Failed { error, evidence } => {
            Err(Box::new(FallibleViewQueryError::Operational {
                error,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            }))
        }
        ViewOperationStatus::Ready { evidence } => match evaluation {
            Ok(answer) => Ok(CompleteAnnotatedViewQuery {
                answer,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            }),
            Err(diagnostic) => Err(Box::new(FallibleViewQueryError::Query {
                diagnostic,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            })),
        },
    }
}

/// Resolve a provider-aware annotated query directly over a resident RDF 1.2 view.
pub fn dispatch_query_annotated_with_relations_view<V, A, F>(
    view: &V,
    source_identity: WorldSourceIdentity,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    request: RelationAnnotationRequest<'_, '_, A, F>,
) -> Result<CompleteRelationViewQuery<A::Element, ResidentViewEvidence>, RelationQueryError>
where
    V: DatasetView,
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    let identity = QueryExecutionIdentity::for_contract(
        source_identity,
        external_relation_query_contract_hash(
            profile,
            budget,
            request.annotation.contract,
            request.annotation.algebra.identity(),
            request.providers.manifest_hash(),
        ),
    );
    let source = RdfViewFactSource::new(view, profile, identity.source.clone());
    let result =
        dispatch_query_annotated_with_relations(&source, world, program, profile, budget, request)?;
    Ok(CompleteRelationViewQuery {
        result,
        evidence: QueryExecutionEvidence {
            backend: ResidentViewEvidence {
                len_hint: view.len_hint(),
                stats_fingerprint: view.stats_fingerprint(),
            },
            source: source.metrics(),
        },
        identity,
    })
}

/// Resolve a provider-aware annotated query over an operationally fallible RDF view.
///
/// RDF source failure, provider/query failure, and semantic absence remain separate.
/// A sticky RDF failure observed after evaluation takes precedence and discards the
/// internal result exactly as on the ordinary fallible dispatch boundary.
pub fn dispatch_query_annotated_with_relations_fallible_view<V, A, F>(
    view: &V,
    source_identity: WorldSourceIdentity,
    world: &str,
    program: &QProgram,
    profile: &str,
    budget: &Budget,
    request: RelationAnnotationRequest<'_, '_, A, F>,
) -> FallibleRelationViewQueryResult<A::Element, V::Error, V::Evidence>
where
    V: FallibleDatasetView,
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    let identity = QueryExecutionIdentity::for_contract(
        source_identity,
        external_relation_query_contract_hash(
            profile,
            budget,
            request.annotation.contract,
            request.annotation.algebra.identity(),
            request.providers.manifest_hash(),
        ),
    );
    if let ViewOperationStatus::Failed { error, evidence } = view.operation_status() {
        return Err(Box::new(FallibleRelationViewQueryError::Operational {
            error,
            evidence: QueryExecutionEvidence {
                backend: evidence,
                source: WorldSourceMetrics::default(),
            },
            identity,
        }));
    }

    let source = RdfViewFactSource::new(view, profile, identity.source.clone());
    let evaluation =
        dispatch_query_annotated_with_relations(&source, world, program, profile, budget, request);
    let source_metrics = source.metrics();
    match view.operation_status() {
        ViewOperationStatus::Failed { error, evidence } => {
            Err(Box::new(FallibleRelationViewQueryError::Operational {
                error,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            }))
        }
        ViewOperationStatus::Ready { evidence } => match evaluation {
            Ok(result) => Ok(CompleteRelationViewQuery {
                result,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            }),
            Err(error) => Err(Box::new(FallibleRelationViewQueryError::Relation {
                error,
                evidence: QueryExecutionEvidence {
                    backend: evidence,
                    source: source_metrics,
                },
                identity,
            })),
        },
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[path = "dispatch.tests.rs"]
#[cfg(test)]
mod tests;
