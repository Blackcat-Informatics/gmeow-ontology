// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Query-scoped annotated external relations.
//!
//! External relations are operation-local EDB sources for the native relational
//! evaluator. They are not RDF facts, do not mutate a world, and are never resolved
//! through ambient process state. A registration binds one relation IRI to one provider,
//! immutable artifact/model identities, a typed RDF 1.2 schema, an annotation dimension,
//! an annotation algebra, a preservation claim, and deterministic request policy.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use gmeow_logic_compile::result_shape::ColumnKind;
use purrdf::TermValue;

use crate::annotation::{AnnotatedAnswerSet, TupleAnnotationAlgebra};
use crate::provenance::term_display;
use crate::result::PreservationClaim;
use crate::seam::WorldSourceIdentity;

fn push_frame(out: &mut String, value: &str) {
    out.push_str(&value.len().to_string());
    out.push(':');
    out.push_str(value);
}

fn require_absolute_iri(field: &str, value: &str) -> Result<(), RelationContractError> {
    match purrdf::iri::parse(value) {
        Ok(iri) if iri.has_scheme() => Ok(()),
        Ok(_) => Err(RelationContractError::new(format!(
            "external relation {field} must be an absolute IRI, got {value:?}"
        ))),
        Err(error) => Err(RelationContractError::new(format!(
            "external relation {field} is not a valid RDF 1.2 IRI {value:?}: {error}"
        ))),
    }
}

fn column_kind_key(kind: &ColumnKind) -> String {
    match kind {
        ColumnKind::Iri => "iri".to_owned(),
        ColumnKind::BlankNode => "blank-node".to_owned(),
        ColumnKind::Literal { datatype: None } => "literal:*".to_owned(),
        ColumnKind::Literal {
            datatype: Some(datatype),
        } => format!("literal:{datatype}"),
        ColumnKind::TripleTerm => "triple-term".to_owned(),
    }
}

/// A malformed external-relation declaration or registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationContractError {
    /// Precise hard-failure detail.
    pub detail: String,
}

impl RelationContractError {
    fn new(detail: String) -> Self {
        Self { detail }
    }
}

impl fmt::Display for RelationContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for RelationContractError {}

/// Semantic dimension carried by a provider tuple's opaque annotation.
///
/// These values are deliberately distinct. In particular, a similarity, rank,
/// distance, or persistence value is never silently reported as epistemic confidence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationAnnotationDimension {
    /// Lexical, vector, or other similarity.
    Similarity,
    /// A provider-defined ordinal rank.
    Rank,
    /// Metric or non-metric distance.
    Distance,
    /// Topological persistence.
    Persistence,
    /// Epistemic confidence, only when this is genuinely the provider's claim.
    EpistemicConfidence,
    /// A named dimension whose IRI remains explicit in receipts.
    Named(String),
}

impl RelationAnnotationDimension {
    /// Stable IRI identity of this annotation dimension.
    #[must_use]
    pub fn iri(&self) -> &str {
        match self {
            Self::Similarity => "https://blackcatinformatics.ca/logic/SimilarityAnnotation",
            Self::Rank => "https://blackcatinformatics.ca/logic/RankAnnotation",
            Self::Distance => "https://blackcatinformatics.ca/logic/DistanceAnnotation",
            Self::Persistence => "https://blackcatinformatics.ca/logic/PersistenceAnnotation",
            Self::EpistemicConfidence => {
                "https://blackcatinformatics.ca/logic/EpistemicConfidenceAnnotation"
            }
            Self::Named(iri) => iri,
        }
    }

    fn validate(&self) -> Result<(), RelationContractError> {
        require_absolute_iri("annotation dimension", self.iri())
    }
}

/// Direction of the provider's declared total order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationOrderDirection {
    /// Smaller provider order keys precede larger keys.
    Ascending,
    /// Larger provider order keys precede smaller keys.
    Descending,
}

impl RelationOrderDirection {
    /// Stable wire identity.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }
}

/// Total ordering pushed into every call to one provider.
///
/// The provider emits an opaque, lexically ordered `order_key` for each row under
/// `criterion_iri`. Equal keys are broken by the canonical RDF tuple order, so the
/// relation has a total order independent of map/hash iteration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelationOrdering {
    /// IRI naming the ordering criterion (for example lexical rank or vector distance).
    pub criterion_iri: String,
    /// Direction applied to provider order keys.
    pub direction: RelationOrderDirection,
}

impl RelationOrdering {
    /// Construct and validate a total ordering contract.
    pub fn new(
        criterion_iri: impl Into<String>,
        direction: RelationOrderDirection,
    ) -> Result<Self, RelationContractError> {
        let value = Self {
            criterion_iri: criterion_iri.into(),
            direction,
        };
        require_absolute_iri("ordering criterion", &value.criterion_iri)?;
        Ok(value)
    }

    /// Compare two returned rows under this total ordering contract.
    #[must_use]
    pub fn compare_rows<E>(&self, left: &RelationTuple<E>, right: &RelationTuple<E>) -> Ordering {
        let primary = left.order_key.cmp(&right.order_key);
        let primary = match self.direction {
            RelationOrderDirection::Ascending => primary,
            RelationOrderDirection::Descending => primary.reverse(),
        };
        primary.then_with(|| left.arguments.cmp(&right.arguments))
    }
}

/// Immutable declaration of one query-scoped external relation source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationProviderDescriptor {
    /// Stable provider IRI.
    pub provider_iri: String,
    /// Immutable index/artifact generation IRI.
    pub artifact_generation: String,
    /// Model or deterministic algorithm identity used by the provider.
    pub model_iri: String,
    /// Predicate/relation IRI exposed to the query program.
    pub relation_iri: String,
    /// Positional RDF 1.2 argument schema; its length is the relation arity.
    pub argument_schema: Vec<ColumnKind>,
    /// Meaning of the opaque tuple annotation.
    pub annotation_dimension: RelationAnnotationDimension,
    /// Stable identity required from the selected annotation algebra.
    pub annotation_algebra: String,
    /// Provider relation versus source-universe preservation disclosure.
    pub preservation: PreservationClaim,
    /// Total row order pushed into provider requests.
    pub ordering: RelationOrdering,
}

impl RelationProviderDescriptor {
    /// Construct a fully validated provider descriptor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_iri: impl Into<String>,
        artifact_generation: impl Into<String>,
        model_iri: impl Into<String>,
        relation_iri: impl Into<String>,
        argument_schema: Vec<ColumnKind>,
        annotation_dimension: RelationAnnotationDimension,
        annotation_algebra: impl Into<String>,
        preservation: PreservationClaim,
        ordering: RelationOrdering,
    ) -> Result<Self, RelationContractError> {
        let value = Self {
            provider_iri: provider_iri.into(),
            artifact_generation: artifact_generation.into(),
            model_iri: model_iri.into(),
            relation_iri: relation_iri.into(),
            argument_schema,
            annotation_dimension,
            annotation_algebra: annotation_algebra.into(),
            preservation,
            ordering,
        };
        value.validate()?;
        Ok(value)
    }

    /// Declared relation arity.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.argument_schema.len()
    }

    /// Canonical, versioned descriptor identity input.
    #[must_use]
    pub fn canonical_key(&self) -> String {
        let mut out = "gmeow-external-relation-descriptor-v1".to_owned();
        for value in [
            self.provider_iri.as_str(),
            self.artifact_generation.as_str(),
            self.model_iri.as_str(),
            self.relation_iri.as_str(),
            self.annotation_dimension.iri(),
            self.annotation_algebra.as_str(),
            self.ordering.criterion_iri.as_str(),
            self.ordering.direction.wire(),
        ] {
            push_frame(&mut out, value);
        }
        for kind in &self.argument_schema {
            push_frame(&mut out, &column_kind_key(kind));
        }
        for polarity in &self.preservation.polarities {
            push_frame(&mut out, polarity.as_str());
        }
        for unsupported in &self.preservation.unsupported_constructs {
            push_frame(&mut out, unsupported);
        }
        out
    }

    fn validate(&self) -> Result<(), RelationContractError> {
        require_absolute_iri("provider identity", &self.provider_iri)?;
        require_absolute_iri("artifact generation", &self.artifact_generation)?;
        require_absolute_iri("model identity", &self.model_iri)?;
        require_absolute_iri("relation identity", &self.relation_iri)?;
        require_absolute_iri("annotation algebra identity", &self.annotation_algebra)?;
        self.annotation_dimension.validate()?;
        if self.argument_schema.is_empty() {
            return Err(RelationContractError::new(format!(
                "external relation <{}> must declare at least one argument",
                self.relation_iri
            )));
        }
        for (position, kind) in self.argument_schema.iter().enumerate() {
            if let ColumnKind::Literal {
                datatype: Some(datatype),
            } = kind
            {
                require_absolute_iri(&format!("argument {position} literal datatype"), datatype)?;
            }
        }
        self.preservation.validate().map_err(|error| {
            RelationContractError::new(format!(
                "external relation <{}> has an invalid preservation claim: {error}",
                self.relation_iri
            ))
        })
    }
}

/// One positional, demand-pushed call to an external relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationCall {
    /// Engine-minted, content-addressed request IRI.
    pub request_iri: String,
    /// Governing annotated query contract hash.
    pub query_contract_hash: String,
    /// Relation being requested.
    pub relation_iri: String,
    /// One bound value or an unbound slot per relation argument.
    pub bounds: Vec<Option<TermValue>>,
    /// Ordered-prefix limit pushed into the provider.
    pub limit: usize,
    /// Total ordering the provider must apply before limiting.
    pub ordering: RelationOrdering,
}

/// One complete provider tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationTuple<E> {
    /// Positional RDF 1.2 arguments.
    pub arguments: Vec<TermValue>,
    /// Opaque value in the selected annotation algebra.
    pub annotation: E,
    /// Provider-authored lexical key under the request's named ordering criterion.
    pub order_key: String,
}

/// A complete response to one ordered-prefix provider call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationBatch<E> {
    /// Artifact generation actually read by the provider.
    pub artifact_generation: String,
    /// Complete rows for the requested bound pattern and ordered prefix.
    pub rows: Vec<RelationTuple<E>>,
}

/// Provider-side failure class. None of these is semantic absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationProviderFailureKind {
    /// Provider or backing index is unavailable.
    Unavailable,
    /// Provider rejected a well-formed request under its declared contract.
    Rejected,
    /// Provider observed cancellation while executing the call.
    Cancelled,
    /// Provider detected an internal computation failure.
    Internal,
}

/// Provider-side incompleteness class. No partial row batch accompanies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationProviderIncompletenessKind {
    /// Provider could not exhaust the requested ordered prefix under its own budget.
    ProviderBudget,
    /// Provider detected that its artifact generation changed during the call.
    StaleGeneration,
    /// Provider cannot certify the requested candidate universe as complete.
    UncertifiedUniverse,
}

/// Typed provider call failure; incomplete and failed executions remain distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationProviderError {
    /// Operational provider failure.
    Failure {
        /// Stable failure class.
        kind: RelationProviderFailureKind,
        /// Provider-authored diagnostic detail.
        detail: String,
    },
    /// Provider could not return a complete requested prefix.
    Incomplete {
        /// Stable incompleteness class.
        kind: RelationProviderIncompletenessKind,
        /// Provider-authored diagnostic detail.
        detail: String,
    },
}

impl fmt::Display for RelationProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failure { kind, detail } => {
                write!(
                    formatter,
                    "external relation provider failure ({kind:?}): {detail}"
                )
            }
            Self::Incomplete { kind, detail } => write!(
                formatter,
                "external relation provider incomplete ({kind:?}): {detail}"
            ),
        }
    }
}

impl std::error::Error for RelationProviderError {}

/// Operation-scoped cancellation observed at deterministic provider-call boundaries.
pub trait RelationCancellation: Send + Sync {
    /// Whether cancellation has been requested.
    fn is_cancelled(&self) -> bool;
}

/// Cancellation source for an execution that remains live for its entire duration.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeverCancelled;

impl RelationCancellation for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// Object-safe external relation implementation.
pub trait ExternalRelationProvider<E>: Send + Sync {
    /// Return the complete ordered prefix requested by `call`.
    ///
    /// A provider must not encode failure or incompleteness as an empty successful batch.
    fn call(
        &self,
        call: &RelationCall,
        cancellation: &dyn RelationCancellation,
    ) -> Result<RelationBatch<E>, RelationProviderError>;
}

/// Deterministic operation-wide provider governor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelationProviderBudget {
    /// Maximum number of distinct provider calls.
    pub max_calls: u64,
    /// Maximum number of unique provider rows admitted across complete batches.
    pub max_rows: u64,
}

impl RelationProviderBudget {
    /// Construct a non-zero deterministic budget.
    pub fn new(max_calls: u64, max_rows: u64) -> Result<Self, RelationContractError> {
        if max_calls == 0 || max_rows == 0 {
            return Err(RelationContractError::new(
                "external relation call and row budgets must both be non-zero".to_owned(),
            ));
        }
        Ok(Self {
            max_calls,
            max_rows,
        })
    }
}

/// One query-local binding between a descriptor and its implementation.
pub struct RelationProviderRegistration<'provider, E> {
    /// Immutable semantic/operational descriptor.
    pub descriptor: RelationProviderDescriptor,
    /// Ordered-prefix limit pushed on every call.
    pub per_call_limit: usize,
    /// Provider implementation borrowed for this query execution only.
    pub provider: &'provider dyn ExternalRelationProvider<E>,
}

impl<'provider, E> RelationProviderRegistration<'provider, E> {
    /// Construct a registration with a non-zero pushed limit.
    pub fn new(
        descriptor: RelationProviderDescriptor,
        per_call_limit: usize,
        provider: &'provider dyn ExternalRelationProvider<E>,
    ) -> Result<Self, RelationContractError> {
        descriptor.validate()?;
        if per_call_limit == 0 {
            return Err(RelationContractError::new(format!(
                "external relation <{}> per-call limit must be non-zero",
                descriptor.relation_iri
            )));
        }
        Ok(Self {
            descriptor,
            per_call_limit,
            provider,
        })
    }
}

/// Immutable provider set borrowed by exactly one query execution.
pub struct QueryRelationProviders<'provider, E> {
    registrations: Vec<RelationProviderRegistration<'provider, E>>,
    /// Deterministic operation-wide call/row budget.
    pub budget: RelationProviderBudget,
    /// Operation-scoped cancellation source.
    pub cancellation: &'provider dyn RelationCancellation,
    manifest_hash: String,
}

impl<'provider, E> QueryRelationProviders<'provider, E> {
    /// Validate, sort, and seal one query-local provider set.
    pub fn new(
        mut registrations: Vec<RelationProviderRegistration<'provider, E>>,
        budget: RelationProviderBudget,
        cancellation: &'provider dyn RelationCancellation,
    ) -> Result<Self, RelationContractError> {
        if budget.max_calls == 0 || budget.max_rows == 0 {
            return Err(RelationContractError::new(
                "external relation call and row budgets must both be non-zero".to_owned(),
            ));
        }
        if registrations.is_empty() {
            return Err(RelationContractError::new(
                "an external relation query must register at least one provider".to_owned(),
            ));
        }
        registrations.sort_by(|left, right| {
            left.descriptor
                .relation_iri
                .cmp(&right.descriptor.relation_iri)
        });
        for registration in &registrations {
            registration.descriptor.validate()?;
            if registration.per_call_limit == 0 {
                return Err(RelationContractError::new(format!(
                    "external relation <{}> per-call limit must be non-zero",
                    registration.descriptor.relation_iri
                )));
            }
        }
        for pair in registrations.windows(2) {
            if pair[0].descriptor.relation_iri == pair[1].descriptor.relation_iri {
                return Err(RelationContractError::new(format!(
                    "external relation <{}> is registered more than once in one query",
                    pair[0].descriptor.relation_iri
                )));
            }
        }
        let mut canonical = "gmeow-query-relation-provider-set-v1".to_owned();
        push_frame(&mut canonical, &budget.max_calls.to_string());
        push_frame(&mut canonical, &budget.max_rows.to_string());
        for registration in &registrations {
            push_frame(&mut canonical, &registration.descriptor.canonical_key());
            push_frame(&mut canonical, &registration.per_call_limit.to_string());
        }
        let manifest_hash = blake3::hash(canonical.as_bytes()).to_hex().to_string();
        Ok(Self {
            registrations,
            budget,
            cancellation,
            manifest_hash,
        })
    }

    /// Content identity of every registered provider, policy, and deterministic budget.
    #[must_use]
    pub fn manifest_hash(&self) -> &str {
        &self.manifest_hash
    }

    /// Registration for `relation_iri`, when this operation owns it.
    #[must_use]
    pub fn registration(
        &self,
        relation_iri: &str,
    ) -> Option<&RelationProviderRegistration<'provider, E>> {
        self.registrations
            .binary_search_by(|candidate| {
                candidate.descriptor.relation_iri.as_str().cmp(relation_iri)
            })
            .ok()
            .map(|index| &self.registrations[index])
    }

    /// Registered relation names in canonical order.
    pub fn relation_names(&self) -> impl Iterator<Item = &str> {
        self.registrations
            .iter()
            .map(|registration| registration.descriptor.relation_iri.as_str())
    }

    /// Hard-fail if any descriptor expects a different annotation algebra.
    pub fn validate_algebra<A>(&self, algebra: &A) -> Result<(), RelationContractError>
    where
        A: TupleAnnotationAlgebra<Element = E>,
    {
        for registration in &self.registrations {
            if registration.descriptor.annotation_algebra != algebra.identity() {
                return Err(RelationContractError::new(format!(
                    "external relation <{}> requires annotation algebra <{}> but the query selected <{}>",
                    registration.descriptor.relation_iri,
                    registration.descriptor.annotation_algebra,
                    algebra.identity()
                )));
            }
        }
        Ok(())
    }
}

/// Stable provider provenance carried by an answer derivation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderTupleSource {
    /// Provider identity.
    pub provider_iri: String,
    /// Artifact generation that produced the tuple.
    pub artifact_generation: String,
    /// Model/algorithm identity.
    pub model_iri: String,
    /// Engine-minted request identity.
    pub request_iri: String,
    /// External relation identity.
    pub relation_iri: String,
    /// Canonical RDF 1.2 tuple arguments.
    pub arguments: Vec<String>,
    /// Explicit annotation dimension.
    pub annotation_dimension_iri: String,
}

/// Terminal status of one provider invocation receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationInvocationStatus {
    /// Provider returned a complete, validated batch.
    Complete,
    /// An identical complete batch was reused from the query-local cache.
    CacheHit,
    /// Provider returned a typed operational failure.
    Failed,
    /// Provider returned typed incompleteness.
    Incomplete,
    /// Operation-wide call or row budget was exhausted.
    BudgetExhausted,
    /// Cancellation was observed at a deterministic call boundary.
    Cancelled,
    /// Provider output violated its descriptor/request contract.
    ContractViolation,
}

/// Deterministic evidence for one provider request or cache reuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationInvocationReceipt {
    /// Engine-minted request identity.
    pub request_iri: String,
    /// Provider identity.
    pub provider_iri: String,
    /// Pinned artifact generation.
    pub artifact_generation: String,
    /// Model/algorithm identity.
    pub model_iri: String,
    /// Relation identity.
    pub relation_iri: String,
    /// Positional bounds pushed into the provider.
    pub bounds: Vec<Option<TermValue>>,
    /// Ordered-prefix limit pushed into the provider.
    pub limit: usize,
    /// Ordering contract pushed into the provider.
    pub ordering: RelationOrdering,
    /// Explicit annotation dimension.
    pub annotation_dimension_iri: String,
    /// Terminal invocation status.
    pub status: RelationInvocationStatus,
    /// Content hash of a complete validated response, empty on non-complete attempts.
    pub response_hash: Option<String>,
    /// Rows delivered by the provider before validation.
    pub delivered_rows: u64,
    /// Unique rows admitted to this query execution.
    pub admitted_rows: u64,
    /// Whether at least one returned tuple contributed to a final answer derivation.
    pub contributed: bool,
    /// Typed terminal detail for non-complete attempts.
    pub detail: Option<String>,
}

/// Structural provider-access evidence; no wall-clock inference is encoded here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RelationAccessMetrics {
    /// Distinct provider calls executed.
    pub provider_calls: u64,
    /// Complete batches reused without another provider call.
    pub cache_hits: u64,
    /// Rows delivered before contract validation.
    pub delivered_rows: u64,
    /// Unique validated rows admitted to evaluation.
    pub admitted_rows: u64,
    /// Calls carrying at least one bound argument.
    pub bound_calls: u64,
}

/// Content-addressed operational receipt for a complete provider-aware query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationQueryReceipt {
    /// Governing provider-aware annotated query contract.
    pub query_contract_hash: String,
    /// Query-local provider manifest/budget identity.
    pub provider_manifest_hash: String,
    /// Immutable RDF source generation and source contract.
    pub source: WorldSourceIdentity,
    /// Content identity of the native engine that executed the query.
    pub engine_descriptor_hash: String,
    /// Invocation evidence in deterministic execution order.
    pub invocations: Vec<RelationInvocationReceipt>,
    /// Provider/artifact pairs that contributed to returned answers.
    pub contributing_providers: BTreeSet<(String, String)>,
    /// Structural demand evidence.
    pub metrics: RelationAccessMetrics,
    /// Hash over every field above plus source and engine identities at dispatch.
    pub receipt_hash: String,
}

/// Complete annotated answer plus its external-relation operational receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationQueryResult<E> {
    /// Native annotated answer set.
    pub answer: AnnotatedAnswerSet<E>,
    /// Complete operational receipt.
    pub receipt: RelationQueryReceipt,
}

/// Content-addressed evidence retained when a provider-aware query does not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationQueryFailureReceipt {
    /// Governing provider-aware annotated query contract.
    pub query_contract_hash: String,
    /// Query-local provider manifest/budget identity.
    pub provider_manifest_hash: String,
    /// Immutable RDF source generation and source contract.
    pub source: WorldSourceIdentity,
    /// Content identity of the native engine that attempted the query.
    pub engine_descriptor_hash: String,
    /// Every complete, cached, or failed provider attempt before termination.
    pub invocations: Vec<RelationInvocationReceipt>,
    /// Structural access evidence at termination.
    pub metrics: RelationAccessMetrics,
    /// Stable terminal class supplied by dispatch.
    pub terminal_kind: String,
    /// Deterministic diagnostic detail.
    pub detail: String,
    /// Content hash over every field above.
    pub receipt_hash: String,
}

/// Engine-side terminal class for a provider invocation that cannot produce rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationExecutionFailureKind {
    /// The provider reported a typed operational failure.
    ProviderFailure(RelationProviderFailureKind),
    /// The provider reported that it could not certify a complete ordered prefix.
    ProviderIncomplete(RelationProviderIncompletenessKind),
    /// The deterministic operation-wide provider governor was exhausted.
    BudgetExhausted,
    /// Operation cancellation was observed at a deterministic boundary.
    Cancelled,
    /// A returned batch violated its pinned descriptor or request.
    ContractViolation,
}

/// One typed failed provider attempt, including the receipt retained for diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationExecutionError {
    /// Stable terminal class.
    pub kind: RelationExecutionFailureKind,
    /// Complete evidence for the failed attempt; no rows from it were admitted.
    pub invocation: Box<RelationInvocationReceipt>,
}

impl fmt::Display for RelationExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "external relation invocation <{}> ended as {:?}: {}",
            self.invocation.request_iri,
            self.kind,
            self.invocation.detail.as_deref().unwrap_or("no detail")
        )
    }
}

impl std::error::Error for RelationExecutionError {}

/// One validated provider tuple admitted to the native evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedRelationTuple<E> {
    pub(crate) arguments: Vec<TermValue>,
    pub(crate) annotation: E,
    pub(crate) source: ProviderTupleSource,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RelationCallCacheKey {
    descriptor_key: String,
    bounds: Vec<Option<String>>,
    limit: usize,
    ordering_criterion: String,
    ordering_direction: RelationOrderDirection,
}

#[derive(Debug, Clone)]
struct CachedRelationBatch<E> {
    rows: Vec<ResolvedRelationTuple<E>>,
    response_hash: String,
}

fn hash_canonical(canonical: &str) -> String {
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

fn request_iri(
    query_contract_hash: &str,
    descriptor: &RelationProviderDescriptor,
    bounds: &[Option<TermValue>],
    limit: usize,
) -> String {
    let mut canonical = "gmeow-external-relation-request-v1".to_owned();
    push_frame(&mut canonical, query_contract_hash);
    push_frame(&mut canonical, &descriptor.canonical_key());
    push_frame(&mut canonical, &limit.to_string());
    for bound in bounds {
        match bound {
            Some(value) => {
                push_frame(&mut canonical, "bound");
                push_frame(&mut canonical, &term_display(value));
            }
            None => push_frame(&mut canonical, "unbound"),
        }
    }
    format!(
        "https://blackcatinformatics.ca/.well-known/genid/external-request/{}",
        hash_canonical(&canonical)
    )
}

fn term_conforms_to_column(term: &TermValue, column: &ColumnKind) -> bool {
    match (term, column) {
        (TermValue::Iri(_), ColumnKind::Iri)
        | (TermValue::Blank { .. }, ColumnKind::BlankNode)
        | (TermValue::Triple { .. }, ColumnKind::TripleTerm)
        | (TermValue::Literal { .. }, ColumnKind::Literal { datatype: None }) => true,
        (
            TermValue::Literal { datatype, .. },
            ColumnKind::Literal {
                datatype: Some(expected),
            },
        ) => datatype == expected,
        _ => false,
    }
}

fn invocation_status_wire(status: RelationInvocationStatus) -> &'static str {
    match status {
        RelationInvocationStatus::Complete => "complete",
        RelationInvocationStatus::CacheHit => "cache-hit",
        RelationInvocationStatus::Failed => "failed",
        RelationInvocationStatus::Incomplete => "incomplete",
        RelationInvocationStatus::BudgetExhausted => "budget-exhausted",
        RelationInvocationStatus::Cancelled => "cancelled",
        RelationInvocationStatus::ContractViolation => "contract-violation",
    }
}

fn canonical_invocation(out: &mut String, invocation: &RelationInvocationReceipt) {
    for value in [
        invocation.request_iri.as_str(),
        invocation.provider_iri.as_str(),
        invocation.artifact_generation.as_str(),
        invocation.model_iri.as_str(),
        invocation.relation_iri.as_str(),
        invocation.ordering.criterion_iri.as_str(),
        invocation.ordering.direction.wire(),
        invocation.annotation_dimension_iri.as_str(),
        invocation_status_wire(invocation.status),
    ] {
        push_frame(out, value);
    }
    push_frame(out, &invocation.limit.to_string());
    for bound in &invocation.bounds {
        match bound {
            Some(value) => {
                push_frame(out, "bound");
                push_frame(out, &term_display(value));
            }
            None => push_frame(out, "unbound"),
        }
    }
    push_frame(out, invocation.response_hash.as_deref().unwrap_or(""));
    push_frame(out, &invocation.delivered_rows.to_string());
    push_frame(out, &invocation.admitted_rows.to_string());
    push_frame(
        out,
        if invocation.contributed {
            "true"
        } else {
            "false"
        },
    );
    push_frame(out, invocation.detail.as_deref().unwrap_or(""));
}

/// Stateful, operation-local executor for an immutable provider set.
///
/// The state is deliberately borrowed by one query and has no global registration or
/// cache. Only validated complete batches enter `cache`; cache reuse precedes budget
/// charging, and row accounting is by unique relation tuple across the whole operation.
pub(crate) struct RelationExecution<'set, 'provider, 'algebra, A>
where
    A: TupleAnnotationAlgebra,
{
    providers: &'set QueryRelationProviders<'provider, A::Element>,
    algebra: &'algebra A,
    query_contract_hash: String,
    cache: BTreeMap<RelationCallCacheKey, CachedRelationBatch<A::Element>>,
    admitted_annotations: BTreeMap<(String, Vec<String>), String>,
    invocations: Vec<RelationInvocationReceipt>,
    metrics: RelationAccessMetrics,
}

impl<'set, 'provider, 'algebra, A> RelationExecution<'set, 'provider, 'algebra, A>
where
    A: TupleAnnotationAlgebra,
{
    pub(crate) fn new(
        providers: &'set QueryRelationProviders<'provider, A::Element>,
        algebra: &'algebra A,
        query_contract_hash: impl Into<String>,
    ) -> Result<Self, RelationContractError> {
        providers.validate_algebra(algebra)?;
        Ok(Self {
            providers,
            algebra,
            query_contract_hash: query_contract_hash.into(),
            cache: BTreeMap::new(),
            admitted_annotations: BTreeMap::new(),
            invocations: Vec::new(),
            metrics: RelationAccessMetrics::default(),
        })
    }

    fn receipt(
        descriptor: &RelationProviderDescriptor,
        call: &RelationCall,
        status: RelationInvocationStatus,
        response_hash: Option<String>,
        delivered_rows: u64,
        admitted_rows: u64,
        detail: Option<String>,
    ) -> RelationInvocationReceipt {
        RelationInvocationReceipt {
            request_iri: call.request_iri.clone(),
            provider_iri: descriptor.provider_iri.clone(),
            artifact_generation: descriptor.artifact_generation.clone(),
            model_iri: descriptor.model_iri.clone(),
            relation_iri: descriptor.relation_iri.clone(),
            bounds: call.bounds.clone(),
            limit: call.limit,
            ordering: call.ordering.clone(),
            annotation_dimension_iri: descriptor.annotation_dimension.iri().to_owned(),
            status,
            response_hash,
            delivered_rows,
            admitted_rows,
            contributed: false,
            detail,
        }
    }

    fn fail(
        &mut self,
        kind: RelationExecutionFailureKind,
        receipt: RelationInvocationReceipt,
    ) -> RelationExecutionError {
        self.invocations.push(receipt.clone());
        RelationExecutionError {
            kind,
            invocation: Box::new(receipt),
        }
    }

    fn validate_batch(
        &self,
        descriptor: &RelationProviderDescriptor,
        call: &RelationCall,
        batch: &RelationBatch<A::Element>,
    ) -> Result<(), String> {
        if batch.artifact_generation != descriptor.artifact_generation {
            return Err(format!(
                "provider returned artifact generation <{}>, expected <{}>",
                batch.artifact_generation, descriptor.artifact_generation
            ));
        }
        if batch.rows.len() > call.limit {
            return Err(format!(
                "provider returned {} rows beyond pushed limit {}",
                batch.rows.len(),
                call.limit
            ));
        }
        let mut unique = BTreeSet::new();
        for (row_index, row) in batch.rows.iter().enumerate() {
            if row.arguments.len() != descriptor.arity() {
                return Err(format!(
                    "provider row {row_index} has arity {}, expected {}",
                    row.arguments.len(),
                    descriptor.arity()
                ));
            }
            for (position, ((argument, column), bound)) in row
                .arguments
                .iter()
                .zip(&descriptor.argument_schema)
                .zip(&call.bounds)
                .enumerate()
            {
                if !term_conforms_to_column(argument, column) {
                    return Err(format!(
                        "provider row {row_index} argument {position} does not conform to {}",
                        column_kind_key(column)
                    ));
                }
                if let Some(expected) = bound
                    && term_display(argument) != term_display(expected)
                {
                    return Err(format!(
                        "provider row {row_index} argument {position} violates pushed bound {}",
                        term_display(expected)
                    ));
                }
            }
            let key = row.arguments.iter().map(term_display).collect::<Vec<_>>();
            if !unique.insert(key) {
                return Err(format!(
                    "provider row {row_index} duplicates an earlier tuple"
                ));
            }
        }
        for (position, pair) in batch.rows.windows(2).enumerate() {
            if call.ordering.compare_rows(&pair[0], &pair[1]) == Ordering::Greater {
                return Err(format!(
                    "provider rows {} and {} violate the declared total order",
                    position,
                    position + 1
                ));
            }
        }
        Ok(())
    }

    fn response_hash(
        &self,
        descriptor: &RelationProviderDescriptor,
        batch: &RelationBatch<A::Element>,
    ) -> String {
        let mut canonical = "gmeow-external-relation-response-v1".to_owned();
        push_frame(&mut canonical, &descriptor.canonical_key());
        push_frame(&mut canonical, &batch.artifact_generation);
        for row in &batch.rows {
            push_frame(&mut canonical, &row.order_key);
            for argument in &row.arguments {
                push_frame(&mut canonical, &term_display(argument));
            }
            push_frame(
                &mut canonical,
                &self.algebra.canonical_element(&row.annotation),
            );
        }
        hash_canonical(&canonical)
    }

    pub(crate) fn resolve(
        &mut self,
        relation_iri: &str,
        bounds: Vec<Option<TermValue>>,
    ) -> Result<Vec<ResolvedRelationTuple<A::Element>>, RelationExecutionError> {
        let (descriptor, limit, provider) = {
            let registration = self
                .providers
                .registration(relation_iri)
                .expect("provider resolution is called only for a registered relation");
            (
                registration.descriptor.clone(),
                registration.per_call_limit,
                registration.provider,
            )
        };
        let call = RelationCall {
            request_iri: request_iri(&self.query_contract_hash, &descriptor, &bounds, limit),
            query_contract_hash: self.query_contract_hash.clone(),
            relation_iri: descriptor.relation_iri.clone(),
            bounds,
            limit,
            ordering: descriptor.ordering.clone(),
        };
        if call.bounds.len() != descriptor.arity() {
            let receipt = Self::receipt(
                &descriptor,
                &call,
                RelationInvocationStatus::ContractViolation,
                None,
                0,
                0,
                Some(format!(
                    "request has {} bound slots for arity {}",
                    call.bounds.len(),
                    descriptor.arity()
                )),
            );
            return Err(self.fail(RelationExecutionFailureKind::ContractViolation, receipt));
        }
        let cache_key = RelationCallCacheKey {
            descriptor_key: descriptor.canonical_key(),
            bounds: call
                .bounds
                .iter()
                .map(|bound| bound.as_ref().map(term_display))
                .collect(),
            limit: call.limit,
            ordering_criterion: call.ordering.criterion_iri.clone(),
            ordering_direction: call.ordering.direction,
        };

        if self.providers.cancellation.is_cancelled() {
            let receipt = Self::receipt(
                &descriptor,
                &call,
                RelationInvocationStatus::Cancelled,
                None,
                0,
                0,
                Some("cancellation observed before provider call".to_owned()),
            );
            return Err(self.fail(RelationExecutionFailureKind::Cancelled, receipt));
        }
        if let Some(cached) = self.cache.get(&cache_key) {
            let rows = cached.rows.clone();
            let response_hash = cached.response_hash.clone();
            self.metrics.cache_hits = self.metrics.cache_hits.saturating_add(1);
            self.invocations.push(Self::receipt(
                &descriptor,
                &call,
                RelationInvocationStatus::CacheHit,
                Some(response_hash),
                0,
                0,
                None,
            ));
            return Ok(rows);
        }
        if self.metrics.provider_calls >= self.providers.budget.max_calls {
            let receipt = Self::receipt(
                &descriptor,
                &call,
                RelationInvocationStatus::BudgetExhausted,
                None,
                0,
                0,
                Some(format!(
                    "provider call budget {} exhausted",
                    self.providers.budget.max_calls
                )),
            );
            return Err(self.fail(RelationExecutionFailureKind::BudgetExhausted, receipt));
        }

        self.metrics.provider_calls = self.metrics.provider_calls.saturating_add(1);
        if call.bounds.iter().any(Option::is_some) {
            self.metrics.bound_calls = self.metrics.bound_calls.saturating_add(1);
        }
        let batch = match provider.call(&call, self.providers.cancellation) {
            Ok(batch) => batch,
            Err(RelationProviderError::Failure { kind, detail }) => {
                let receipt = Self::receipt(
                    &descriptor,
                    &call,
                    RelationInvocationStatus::Failed,
                    None,
                    0,
                    0,
                    Some(detail),
                );
                return Err(self.fail(RelationExecutionFailureKind::ProviderFailure(kind), receipt));
            }
            Err(RelationProviderError::Incomplete { kind, detail }) => {
                let receipt = Self::receipt(
                    &descriptor,
                    &call,
                    RelationInvocationStatus::Incomplete,
                    None,
                    0,
                    0,
                    Some(detail),
                );
                return Err(self.fail(
                    RelationExecutionFailureKind::ProviderIncomplete(kind),
                    receipt,
                ));
            }
        };
        let delivered = u64::try_from(batch.rows.len()).unwrap_or(u64::MAX);
        self.metrics.delivered_rows = self.metrics.delivered_rows.saturating_add(delivered);
        if self.providers.cancellation.is_cancelled() {
            let receipt = Self::receipt(
                &descriptor,
                &call,
                RelationInvocationStatus::Cancelled,
                None,
                delivered,
                0,
                Some("cancellation observed after provider call".to_owned()),
            );
            return Err(self.fail(RelationExecutionFailureKind::Cancelled, receipt));
        }
        if let Err(detail) = self.validate_batch(&descriptor, &call, &batch) {
            let receipt = Self::receipt(
                &descriptor,
                &call,
                RelationInvocationStatus::ContractViolation,
                None,
                delivered,
                0,
                Some(detail),
            );
            return Err(self.fail(RelationExecutionFailureKind::ContractViolation, receipt));
        }

        let mut new_keys = Vec::new();
        for row in &batch.rows {
            let arguments = row.arguments.iter().map(term_display).collect::<Vec<_>>();
            let key = (descriptor.relation_iri.clone(), arguments);
            let annotation = self.algebra.canonical_element(&row.annotation);
            match self.admitted_annotations.get(&key) {
                Some(existing) if existing != &annotation => {
                    let receipt = Self::receipt(
                        &descriptor,
                        &call,
                        RelationInvocationStatus::ContractViolation,
                        None,
                        delivered,
                        0,
                        Some(
                            "the same provider tuple was returned with two annotation values"
                                .to_owned(),
                        ),
                    );
                    return Err(self.fail(RelationExecutionFailureKind::ContractViolation, receipt));
                }
                Some(_) => {}
                None => new_keys.push((key, annotation)),
            }
        }
        let new_count = u64::try_from(new_keys.len()).unwrap_or(u64::MAX);
        if self.metrics.admitted_rows.saturating_add(new_count) > self.providers.budget.max_rows {
            let receipt = Self::receipt(
                &descriptor,
                &call,
                RelationInvocationStatus::BudgetExhausted,
                None,
                delivered,
                0,
                Some(format!(
                    "admitting {new_count} new rows would exceed provider row budget {}",
                    self.providers.budget.max_rows
                )),
            );
            return Err(self.fail(RelationExecutionFailureKind::BudgetExhausted, receipt));
        }
        for (key, annotation) in new_keys {
            self.admitted_annotations.insert(key, annotation);
        }
        self.metrics.admitted_rows = self.metrics.admitted_rows.saturating_add(new_count);
        let response_hash = self.response_hash(&descriptor, &batch);
        let rows = batch
            .rows
            .into_iter()
            .map(|row| ResolvedRelationTuple {
                source: ProviderTupleSource {
                    provider_iri: descriptor.provider_iri.clone(),
                    artifact_generation: descriptor.artifact_generation.clone(),
                    model_iri: descriptor.model_iri.clone(),
                    request_iri: call.request_iri.clone(),
                    relation_iri: descriptor.relation_iri.clone(),
                    arguments: row.arguments.iter().map(term_display).collect(),
                    annotation_dimension_iri: descriptor.annotation_dimension.iri().to_owned(),
                },
                arguments: row.arguments,
                annotation: row.annotation,
            })
            .collect::<Vec<_>>();
        self.invocations.push(Self::receipt(
            &descriptor,
            &call,
            RelationInvocationStatus::Complete,
            Some(response_hash.clone()),
            delivered,
            new_count,
            None,
        ));
        self.cache.insert(
            cache_key,
            CachedRelationBatch {
                rows: rows.clone(),
                response_hash,
            },
        );
        Ok(rows)
    }

    pub(crate) fn is_provider_relation(&self, relation_iri: &str) -> bool {
        self.providers.registration(relation_iri).is_some()
    }

    pub(crate) fn relation_names(&self) -> impl Iterator<Item = &str> {
        self.providers.relation_names()
    }

    pub(crate) fn relation_arity(&self, relation_iri: &str) -> Option<usize> {
        self.providers
            .registration(relation_iri)
            .map(|registration| registration.descriptor.arity())
    }

    pub(crate) fn merge_preservation(
        &self,
        target: &mut PreservationClaim,
    ) -> gmeow_errors::Result<()> {
        let invoked = self
            .invocations
            .iter()
            .filter(|invocation| {
                matches!(
                    invocation.status,
                    RelationInvocationStatus::Complete | RelationInvocationStatus::CacheHit
                )
            })
            .map(|invocation| invocation.relation_iri.as_str())
            .collect::<BTreeSet<_>>();
        for relation in invoked {
            let descriptor = &self
                .providers
                .registration(relation)
                .expect("an invocation always names a registered relation")
                .descriptor;
            target
                .polarities
                .extend(descriptor.preservation.polarities.iter().copied());
            target.unsupported_constructs.extend(
                descriptor
                    .preservation
                    .unsupported_constructs
                    .iter()
                    .cloned(),
            );
        }
        let widened = target.polarities.iter().any(|polarity| {
            matches!(
                polarity,
                gmeow_logic_compile::ir::PreservationKind::SoundUnder
                    | gmeow_logic_compile::ir::PreservationKind::CompleteOver
                    | gmeow_logic_compile::ir::PreservationKind::Unsupported
            )
        });
        if widened {
            target
                .polarities
                .remove(&gmeow_logic_compile::ir::PreservationKind::Exact);
        }
        target.validate()
    }

    pub(crate) fn finish(
        mut self,
        answer: AnnotatedAnswerSet<A::Element>,
        source: WorldSourceIdentity,
        engine_descriptor_hash: String,
    ) -> RelationQueryResult<A::Element> {
        let contributing_requests = answer
            .answers
            .iter()
            .flat_map(|answer| &answer.derivations)
            .flat_map(|derivation| &derivation.provider_sources)
            .map(|source| source.request_iri.clone())
            .collect::<BTreeSet<_>>();
        let mut contributing_providers = BTreeSet::new();
        for invocation in &mut self.invocations {
            invocation.contributed = contributing_requests.contains(&invocation.request_iri);
            if invocation.contributed {
                contributing_providers.insert((
                    invocation.provider_iri.clone(),
                    invocation.artifact_generation.clone(),
                ));
            }
        }
        let mut canonical = "gmeow-external-relation-query-receipt-v1".to_owned();
        for value in [
            self.query_contract_hash.as_str(),
            self.providers.manifest_hash(),
            source.generation.as_str(),
            source.source_contract.as_str(),
            engine_descriptor_hash.as_str(),
        ] {
            push_frame(&mut canonical, value);
        }
        for invocation in &self.invocations {
            canonical_invocation(&mut canonical, invocation);
        }
        for (provider, generation) in &contributing_providers {
            push_frame(&mut canonical, provider);
            push_frame(&mut canonical, generation);
        }
        for value in [
            self.metrics.provider_calls,
            self.metrics.cache_hits,
            self.metrics.delivered_rows,
            self.metrics.admitted_rows,
            self.metrics.bound_calls,
        ] {
            push_frame(&mut canonical, &value.to_string());
        }
        for row in &answer.answers {
            for (variable, value) in &row.binding {
                push_frame(&mut canonical, variable);
                push_frame(&mut canonical, value);
            }
            push_frame(
                &mut canonical,
                &self.algebra.canonical_element(&row.annotation),
            );
        }
        let receipt_hash = hash_canonical(&canonical);
        RelationQueryResult {
            answer,
            receipt: RelationQueryReceipt {
                query_contract_hash: self.query_contract_hash,
                provider_manifest_hash: self.providers.manifest_hash().to_owned(),
                source,
                engine_descriptor_hash,
                invocations: self.invocations,
                contributing_providers,
                metrics: self.metrics,
                receipt_hash,
            },
        }
    }

    pub(crate) fn failure_receipt(
        &self,
        source: WorldSourceIdentity,
        engine_descriptor_hash: String,
        terminal_kind: impl Into<String>,
        detail: impl Into<String>,
    ) -> RelationQueryFailureReceipt {
        let terminal_kind = terminal_kind.into();
        let detail = detail.into();
        let mut canonical = "gmeow-external-relation-query-failure-v1".to_owned();
        for value in [
            self.query_contract_hash.as_str(),
            self.providers.manifest_hash(),
            source.generation.as_str(),
            source.source_contract.as_str(),
            engine_descriptor_hash.as_str(),
            terminal_kind.as_str(),
            detail.as_str(),
        ] {
            push_frame(&mut canonical, value);
        }
        for invocation in &self.invocations {
            canonical_invocation(&mut canonical, invocation);
        }
        for value in [
            self.metrics.provider_calls,
            self.metrics.cache_hits,
            self.metrics.delivered_rows,
            self.metrics.admitted_rows,
            self.metrics.bound_calls,
        ] {
            push_frame(&mut canonical, &value.to_string());
        }
        RelationQueryFailureReceipt {
            query_contract_hash: self.query_contract_hash.clone(),
            provider_manifest_hash: self.providers.manifest_hash().to_owned(),
            source,
            engine_descriptor_hash,
            invocations: self.invocations.clone(),
            metrics: self.metrics,
            terminal_kind,
            detail,
            receipt_hash: hash_canonical(&canonical),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use gmeow_logic_compile::ir::PreservationKind;

    use crate::provenance::ZWeightSemiring;

    struct NoopProvider;

    impl ExternalRelationProvider<i64> for NoopProvider {
        fn call(
            &self,
            call: &RelationCall,
            _cancellation: &dyn RelationCancellation,
        ) -> Result<RelationBatch<i64>, RelationProviderError> {
            Ok(RelationBatch {
                artifact_generation: "https://example.org/index/generation/1".to_owned(),
                rows: Vec::with_capacity(call.limit),
            })
        }
    }

    struct StaticProvider {
        response: Result<RelationBatch<i64>, RelationProviderError>,
        calls: Mutex<Vec<RelationCall>>,
    }

    impl StaticProvider {
        fn complete(rows: Vec<RelationTuple<i64>>) -> Self {
            Self {
                response: Ok(RelationBatch {
                    artifact_generation: "https://example.org/index/generation/1".to_owned(),
                    rows,
                }),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl ExternalRelationProvider<i64> for StaticProvider {
        fn call(
            &self,
            call: &RelationCall,
            _cancellation: &dyn RelationCancellation,
        ) -> Result<RelationBatch<i64>, RelationProviderError> {
            self.calls.lock().unwrap().push(call.clone());
            self.response.clone()
        }
    }

    fn descriptor(relation: &str) -> RelationProviderDescriptor {
        RelationProviderDescriptor::new(
            "https://example.org/provider/lexical",
            "https://example.org/index/generation/1",
            "https://example.org/model/bm25-v1",
            relation,
            vec![ColumnKind::Literal { datatype: None }, ColumnKind::Iri],
            RelationAnnotationDimension::Similarity,
            "https://blackcatinformatics.ca/logic/algebra/z-weight-v1",
            PreservationClaim::exact(),
            RelationOrdering::new(
                "https://example.org/order/lexical-rank",
                RelationOrderDirection::Ascending,
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn descriptor_is_typed_content_addressable_and_rdf12_complete() {
        let mut with_triple = descriptor("https://example.org/relation/mentions");
        with_triple.argument_schema.push(ColumnKind::TripleTerm);
        let key = with_triple.canonical_key();
        assert!(key.contains("triple-term"));
        assert_eq!(with_triple.arity(), 3);
        assert!(
            with_triple
                .preservation
                .polarities
                .contains(&PreservationKind::Exact)
        );
    }

    #[test]
    fn query_set_rejects_duplicate_relation_ownership() {
        let provider = NoopProvider;
        let first = RelationProviderRegistration::new(
            descriptor("https://example.org/relation/name-like"),
            8,
            &provider,
        )
        .unwrap();
        let second = RelationProviderRegistration::new(
            descriptor("https://example.org/relation/name-like"),
            8,
            &provider,
        )
        .unwrap();
        let error = QueryRelationProviders::new(
            vec![first, second],
            RelationProviderBudget::new(4, 32).unwrap(),
            &NeverCancelled,
        )
        .err()
        .expect("duplicate registration must fail");
        assert!(error.detail.contains("registered more than once"));
    }

    #[test]
    fn dimension_identities_never_collapse_to_confidence() {
        let distinct = [
            RelationAnnotationDimension::Similarity,
            RelationAnnotationDimension::Rank,
            RelationAnnotationDimension::Distance,
            RelationAnnotationDimension::Persistence,
            RelationAnnotationDimension::EpistemicConfidence,
        ]
        .into_iter()
        .map(|dimension| dimension.iri().to_owned())
        .collect::<BTreeSet<_>>();
        assert_eq!(distinct.len(), 5);
    }

    #[test]
    fn malformed_iris_empty_schema_and_zero_budgets_are_rejected() {
        let bad_provider = RelationProviderDescriptor::new(
            "relative-provider",
            "https://example.org/index/generation/1",
            "https://example.org/model/bm25-v1",
            "https://example.org/relation/name-like",
            vec![ColumnKind::Iri],
            RelationAnnotationDimension::Similarity,
            "https://blackcatinformatics.ca/logic/algebra/z-weight-v1",
            PreservationClaim::exact(),
            RelationOrdering::new(
                "https://example.org/order/lexical-rank",
                RelationOrderDirection::Ascending,
            )
            .unwrap(),
        )
        .expect_err("relative provider identity must fail");
        assert!(bad_provider.detail.contains("absolute IRI"));

        let empty_schema = RelationProviderDescriptor::new(
            "https://example.org/provider/lexical",
            "https://example.org/index/generation/1",
            "https://example.org/model/bm25-v1",
            "https://example.org/relation/name-like",
            Vec::new(),
            RelationAnnotationDimension::Similarity,
            "https://blackcatinformatics.ca/logic/algebra/z-weight-v1",
            PreservationClaim::exact(),
            RelationOrdering::new(
                "https://example.org/order/lexical-rank",
                RelationOrderDirection::Ascending,
            )
            .unwrap(),
        )
        .expect_err("zero-arity provider relation must fail");
        assert!(empty_schema.detail.contains("at least one argument"));

        assert!(RelationProviderBudget::new(0, 1).is_err());
        assert!(RelationProviderBudget::new(1, 0).is_err());
    }

    fn relation_row(query: &str, document: &str, score: i64, order: &str) -> RelationTuple<i64> {
        RelationTuple {
            arguments: vec![
                TermValue::simple_literal(query),
                TermValue::iri(format!("https://example.org/document/{document}")),
            ],
            annotation: score,
            order_key: order.to_owned(),
        }
    }

    #[test]
    fn complete_batches_are_validated_hashed_and_cached_before_budget_charge() {
        let provider = StaticProvider::complete(vec![
            relation_row("cat", "one", 7, "001"),
            relation_row("cat", "two", 5, "002"),
        ]);
        let registration = RelationProviderRegistration::new(
            descriptor("https://example.org/relation/name-like"),
            4,
            &provider,
        )
        .unwrap();
        let providers = QueryRelationProviders::new(
            vec![registration],
            RelationProviderBudget::new(1, 2).unwrap(),
            &NeverCancelled,
        )
        .unwrap();
        let mut execution =
            RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
        let bounds = vec![Some(TermValue::simple_literal("cat")), None];
        let first = execution
            .resolve("https://example.org/relation/name-like", bounds.clone())
            .unwrap();
        let second = execution
            .resolve("https://example.org/relation/name-like", bounds)
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(provider.calls.lock().unwrap().len(), 1);
        assert_eq!(execution.metrics.provider_calls, 1);
        assert_eq!(execution.metrics.cache_hits, 1);
        assert_eq!(execution.metrics.delivered_rows, 2);
        assert_eq!(execution.metrics.admitted_rows, 2);
        assert_eq!(execution.metrics.bound_calls, 1);
        assert_eq!(execution.invocations.len(), 2);
        assert_eq!(
            execution.invocations[0].status,
            RelationInvocationStatus::Complete
        );
        assert_eq!(
            execution.invocations[1].status,
            RelationInvocationStatus::CacheHit
        );
        assert_eq!(
            execution.invocations[0].response_hash,
            execution.invocations[1].response_hash
        );
        assert!(
            execution.invocations[0]
                .response_hash
                .as_ref()
                .is_some_and(|hash| hash.len() == 64)
        );
    }

    #[test]
    fn malformed_provider_rows_are_typed_non_results_and_never_cached() {
        let provider = StaticProvider::complete(vec![
            relation_row("cat", "two", 5, "002"),
            relation_row("cat", "one", 7, "001"),
        ]);
        let registration = RelationProviderRegistration::new(
            descriptor("https://example.org/relation/name-like"),
            4,
            &provider,
        )
        .unwrap();
        let providers = QueryRelationProviders::new(
            vec![registration],
            RelationProviderBudget::new(2, 8).unwrap(),
            &NeverCancelled,
        )
        .unwrap();
        let mut execution =
            RelationExecution::new(&providers, &ZWeightSemiring, "query-contract").unwrap();
        let error = execution
            .resolve(
                "https://example.org/relation/name-like",
                vec![Some(TermValue::simple_literal("cat")), None],
            )
            .expect_err("unordered rows must fail");
        assert_eq!(error.kind, RelationExecutionFailureKind::ContractViolation);
        assert_eq!(
            error.invocation.status,
            RelationInvocationStatus::ContractViolation
        );
        assert_eq!(execution.metrics.admitted_rows, 0);
        assert_eq!(execution.metrics.cache_hits, 0);
    }

    #[test]
    fn provider_failure_and_row_budget_exhaustion_are_not_empty_complete_relations() {
        let failed = StaticProvider {
            response: Err(RelationProviderError::Failure {
                kind: RelationProviderFailureKind::Unavailable,
                detail: "lexical index offline".to_owned(),
            }),
            calls: Mutex::new(Vec::new()),
        };
        let failed_registration = RelationProviderRegistration::new(
            descriptor("https://example.org/relation/name-like"),
            4,
            &failed,
        )
        .unwrap();
        let failed_set = QueryRelationProviders::new(
            vec![failed_registration],
            RelationProviderBudget::new(1, 8).unwrap(),
            &NeverCancelled,
        )
        .unwrap();
        let mut execution =
            RelationExecution::new(&failed_set, &ZWeightSemiring, "query-contract").unwrap();
        let error = execution
            .resolve("https://example.org/relation/name-like", vec![None, None])
            .expect_err("provider failure must cross the boundary");
        assert_eq!(
            error.kind,
            RelationExecutionFailureKind::ProviderFailure(RelationProviderFailureKind::Unavailable)
        );
        assert_eq!(error.invocation.status, RelationInvocationStatus::Failed);

        let oversized = StaticProvider::complete(vec![
            relation_row("cat", "one", 7, "001"),
            relation_row("cat", "two", 5, "002"),
        ]);
        let oversized_registration = RelationProviderRegistration::new(
            descriptor("https://example.org/relation/name-like"),
            4,
            &oversized,
        )
        .unwrap();
        let oversized_set = QueryRelationProviders::new(
            vec![oversized_registration],
            RelationProviderBudget::new(1, 1).unwrap(),
            &NeverCancelled,
        )
        .unwrap();
        let mut execution =
            RelationExecution::new(&oversized_set, &ZWeightSemiring, "query-contract").unwrap();
        let error = execution
            .resolve(
                "https://example.org/relation/name-like",
                vec![Some(TermValue::simple_literal("cat")), None],
            )
            .expect_err("row governor must reject the complete batch atomically");
        assert_eq!(error.kind, RelationExecutionFailureKind::BudgetExhausted);
        assert_eq!(
            error.invocation.status,
            RelationInvocationStatus::BudgetExhausted
        );
        assert_eq!(execution.metrics.admitted_rows, 0);
    }
}
