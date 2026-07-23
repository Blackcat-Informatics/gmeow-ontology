// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Verified PURREMB external-relation provider for the native `logic:` evaluator.
//!
//! A PURREMB artifact carries model-specific vector projections bound to an exact
//! source pack and an independently certified RDF identity. This module exposes those
//! projections to the native annotated relational evaluator as one
//! [`crate::external_relation::ExternalRelationProvider`]: a query-scoped, ordered-prefix
//! nearest-neighbour relation whose rows are RDF 1.2 identities retrieved by a
//! metric-correct vector scan GMEOW owns end-to-end (PurRDF ships the container and the
//! verified reader; it ships no retrieval engine).
//!
//! # Proof boundary
//!
//! Retrieved tuples are **derived query inputs**, never asserted facts, equivalences, or
//! entailments. A similarity, distance, or rank value is carried in its own annotation
//! dimension ([`crate::external_relation::RelationAnnotationDimension::Similarity`] /
//! `Distance` / `Rank`) and is **never** reported as
//! [`crate::external_relation::RelationAnnotationDimension::EpistemicConfidence`]. The
//! provider's descriptor declares a non-exact preservation claim — a vector similarity is
//! `logic:Vague`, not an equivalence.
//!
//! # No-optionality
//!
//! Absence, incompatibility, or corruption is a typed hard failure, never a silent
//! fallback: a selection that does not resolve, a dtype that does not match, a
//! profile-surface digest that disagrees, a non-losslessly-mappable target, a
//! zero-magnitude vector under cosine, or a non-finite score all surface as
//! [`RelationProviderError::Failure`] with kind `Rejected`. A generation that changed
//! under the pinned certificate surfaces as [`RelationProviderError::Incomplete`] with
//! kind `StaleGeneration`. An empty batch means genuine absence for the pushed bounds.
//!
//! # Borrow-stability safety contract
//!
//! [`PurrembBinding::open`] borrows caller-owned artifact and source bytes for the whole
//! query lifetime. The caller **must** guarantee that byte slice — especially a
//! [`memmap2::Mmap`](https://docs.rs/memmap2) — is stable, private, and immutable for the
//! entire lifetime of the binding and every provider call made through it. A truncation
//! or mutation of the mapped file mid-scan is SIGBUS/undefined behaviour that no typed
//! return value in this module can catch; only the caller's exclusive ownership of the
//! mapping can prevent it.

use std::collections::{BTreeSet, BinaryHeap};
use std::fmt;

use gmeow_logic_compile::result_shape::ColumnKind;
use purrdf::{
    ArtifactRoot, BlankScope, DistanceMetric, EmbeddingError, EmbeddingView, FamilyId, MatrixId,
    MatrixView, PrefixPostprocessing, ProjectionId, RdfTextDirection, ResidentEmbeddingCertificate,
    SourceVerificationMode, TargetId, TargetKind, TargetSetId, TargetSetView, TermValue,
    TlvWireType, VectorDtype, VectorSpaceId, canonical_tlv, reopen_prevalidated,
    verify_embedding, verify_embedding_source,
};

use crate::external_relation::{
    ExternalRelationProvider, RelationAnnotationDimension, RelationBatch, RelationCall,
    RelationCancellation, RelationContractError, RelationOrderDirection, RelationOrdering,
    RelationProviderDescriptor, RelationProviderError, RelationProviderFailureKind,
    RelationProviderIncompletenessKind, RelationTuple,
};
use crate::result::PreservationClaim;

/// Maximum recursive depth honoured when reconstructing an RDF 1.2 triple term from its
/// component target identities. Target identities are content digests and therefore
/// acyclic by construction; the guard bounds pathological nesting regardless.
const MAX_TERM_DEPTH: u32 = 32;

/// Candidate rows scanned between two cooperative cancellation polls.
const CANCEL_POLL_BLOCK: usize = 4_096;

/// Multiplier applied to the requested prefix when sizing the Matryoshka coarse-pass
/// candidate pool that the full-space rerank narrows to the final ordered prefix.
const MATRYOSHKA_RERANK_FACTOR: usize = 8;

// --------------------------------------------------------------------------- //
// Selection and retrieval policy — explicit, first-class DAG selection.
// --------------------------------------------------------------------------- //

/// The explicit retrieval branch a query selects. Each branch is a first-class,
/// deterministic, cache-keyed strategy; once selected it is mandatory (a missing
/// prerequisite is a hard fail, never a downgrade to the other branch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RetrievalPolicy {
    /// Score every stored full-space row against the bound query vector.
    ExactFullSpace,
    /// Rank a coarse Matryoshka leading-prefix pass, then rerank the pooled candidates
    /// against the full stored space. Permitted only where the family declares a
    /// Matryoshka dimensionality policy.
    MatryoshkaPrefixThenRerank,
}

impl RetrievalPolicy {
    /// Stable wire identity, folded into descriptor identity for cache distinctness.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::ExactFullSpace => "exact-full-space",
            Self::MatryoshkaPrefixThenRerank => "matryoshka-prefix-then-rerank",
        }
    }
}

/// Stable wire identity of a source-verification mode, folded into descriptor identity.
#[must_use]
fn source_mode_wire(mode: SourceVerificationMode) -> &'static str {
    match mode {
        SourceVerificationMode::Exact => "exact",
        SourceVerificationMode::Certified => "certified",
    }
}

/// The complete, declared selection of one PURREMB retrieval surface.
///
/// Every field is a first-class part of the query contract; the binding validates each
/// against the opened artifact and fails closed on any disagreement.
#[derive(Debug, Clone)]
pub struct PurrembSelection {
    /// Target row set the matrix is built over.
    pub target_set: TargetSetId,
    /// Embedding family whose contract governs dtype and metric.
    pub family: FamilyId,
    /// Effective vector space (dimension + postprocessing) scored against.
    pub vector_space: VectorSpaceId,
    /// Authoritative stored matrix.
    pub matrix: MatrixId,
    /// Effective projection, when a Matryoshka coarse pass is used.
    pub projection: Option<ProjectionId>,
    /// Declared distance metric, cross-checked against the family contract.
    pub metric: DistanceMetric,
    /// Effective (leading-prefix) dimension.
    pub effective_dimension: u32,
    /// Prefix postprocessing for the effective space.
    pub postprocessing: PrefixPostprocessing,
    /// Declared stored scalar type, cross-checked against the matrix.
    pub dtype: VectorDtype,
    /// Selected retrieval branch.
    pub policy: RetrievalPolicy,
}

/// The expected profile-surface digests a query declares, sourced from the published
/// profile surfaces. Every field is a 32-byte SHA-256 identity compared byte-for-byte
/// against the opened artifact; any disagreement is a hard fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileSurfaceDigests {
    /// Expected whole-artifact integrity root.
    pub artifact_root: [u8; 32],
    /// Expected exact source-pack SHA-256.
    pub source_exact_digest: [u8; 32],
    /// Expected stored-matrix content digest.
    pub matrix_content_digest: [u8; 32],
    /// Expected target-set identity.
    pub target_set_id: [u8; 32],
    /// Expected effective vector-space identity.
    pub vector_space_id: [u8; 32],
    /// Expected independently certified RDF digest, when the profile pins one.
    pub certified_rdf_digest: Option<[u8; 32]>,
}

// --------------------------------------------------------------------------- //
// Typed error surfaces.
// --------------------------------------------------------------------------- //

/// A PURREMB binding, selection, or profile-surface validation failure. Every variant is
/// a fail-closed rejection of a well-formed request under the provider's declared
/// contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurrembBindingError {
    /// The artifact failed to open, verify, or its source could not be certified.
    Verification(String),
    /// The declared selection does not resolve against the opened artifact.
    Selection(String),
    /// A declared profile-surface digest disagreed with the opened artifact.
    ProfileMismatch(String),
}

impl fmt::Display for PurrembBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Verification(detail) => write!(formatter, "PURREMB verification failed: {detail}"),
            Self::Selection(detail) => write!(formatter, "PURREMB selection rejected: {detail}"),
            Self::ProfileMismatch(detail) => {
                write!(formatter, "PURREMB profile-surface mismatch: {detail}")
            }
        }
    }
}

impl std::error::Error for PurrembBindingError {}

/// A stale-generation signal from a cheap re-pin: the artifact bytes no longer match the
/// certificate retained at binding open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StalePin {
    /// Diagnostic detail, free of any process tag.
    pub detail: String,
}

impl fmt::Display for StalePin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for StalePin {}

/// A target-to-RDF reconstruction failure. Absence of retained identity bytes, a
/// digest-only disclosure, or a kind whose identity cannot be reconstructed losslessly
/// is a rejection: the provider never fabricates an IRI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PurrembMapError {
    /// The row index has no target in the target set.
    MissingRow(usize),
    /// The target id names no target record.
    MissingTarget(String),
    /// The target discloses only a digest, so its RDF identity cannot be reconstructed.
    DigestOnly(String),
    /// The target kind is not losslessly reconstructable into an RDF 1.2 term here.
    UnsupportedKind(String),
    /// The retained identity block is malformed or violates its schema.
    MalformedIdentity(String),
    /// The reconstructed term does not conform to the declared column kind.
    ColumnMismatch(String),
}

impl fmt::Display for PurrembMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRow(row) => write!(formatter, "target set has no row {row}"),
            Self::MissingTarget(id) => write!(formatter, "no target record for id {id}"),
            Self::DigestOnly(id) => {
                write!(formatter, "target {id} discloses only a digest identity")
            }
            Self::UnsupportedKind(detail) => {
                write!(formatter, "target kind not reconstructable: {detail}")
            }
            Self::MalformedIdentity(detail) => {
                write!(formatter, "malformed target identity: {detail}")
            }
            Self::ColumnMismatch(detail) => {
                write!(formatter, "reconstructed term violates column: {detail}")
            }
        }
    }
}

impl std::error::Error for PurrembMapError {}

/// A metric-scoring failure. A dimension mismatch, an unsupported metric, a zero-magnitude
/// vector under cosine, or a non-finite result is a rejection; a NaN is never propagated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreError {
    /// The query and candidate vectors have different lengths.
    DimensionMismatch,
    /// A zero-magnitude vector makes cosine similarity undefined (`0/0`).
    ZeroMagnitude,
    /// The computed score is not finite.
    NonFinite,
    /// The metric has no implemented scoring rule here.
    UnsupportedMetric,
}

impl fmt::Display for ScoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DimensionMismatch => "query and candidate vector dimensions differ",
            Self::ZeroMagnitude => "zero-magnitude vector makes cosine score undefined",
            Self::NonFinite => "metric produced a non-finite score",
            Self::UnsupportedMetric => "metric has no implemented scoring rule",
        })
    }
}

impl std::error::Error for ScoreError {}

// --------------------------------------------------------------------------- //
// Verified binding.
// --------------------------------------------------------------------------- //

/// A fully verified, borrow-stable binding to one PURREMB artifact and its source pack.
///
/// [`Self::open`] opens the container, verifies every contained digest, identity, scalar,
/// and projection once, verifies the attached source pack under the requested mode, and
/// validates the declared selection against the artifact. It retains the resident
/// verification certificate so [`Self::re_pin`] can cheaply detect a generation that
/// changed under the pinned bytes.
pub struct PurrembBinding<'a> {
    bytes: &'a [u8],
    view: EmbeddingView<'a>,
    certificate: ResidentEmbeddingCertificate<'a>,
    artifact_root: ArtifactRoot,
    artifact_root_hex: String,
    selection: PurrembSelection,
    source_mode: SourceVerificationMode,
}

impl<'a> PurrembBinding<'a> {
    /// Open, fully verify, and validate a PURREMB binding.
    ///
    /// The caller owns `artifact_bytes` and `source_bytes` and must keep them stable and
    /// immutable for the entire lifetime of the binding (see the module-level
    /// borrow-stability safety contract).
    ///
    /// # Errors
    ///
    /// Returns [`PurrembBindingError::Verification`] if the artifact does not open, does
    /// not fully verify, or its source pack does not certify under `source_mode`; returns
    /// [`PurrembBindingError::Selection`] if any declared selection component does not
    /// resolve against the artifact or is mutually inconsistent (a cross-dtype selection
    /// is rejected, never reinterpreted).
    pub fn open(
        artifact_bytes: &'a [u8],
        source_bytes: &'a [u8],
        selection: PurrembSelection,
        source_mode: SourceVerificationMode,
    ) -> Result<Self, PurrembBindingError> {
        let mut view = EmbeddingView::from_bytes(artifact_bytes).map_err(embedding_verification)?;
        let certificate = verify_embedding(&mut view)
            .map_err(embedding_verification)?
            .into_certificate();
        verify_embedding_source(&view, source_bytes, source_mode)
            .map_err(embedding_verification)?;

        let artifact_root = view.artifact_root();
        let artifact_root_hex = artifact_root.to_hex();

        validate_selection(&view, &selection)?;

        Ok(Self {
            bytes: artifact_bytes,
            view,
            certificate,
            artifact_root,
            artifact_root_hex,
            selection,
            source_mode,
        })
    }

    /// The fully verified borrowed view.
    #[must_use]
    pub fn view(&self) -> &EmbeddingView<'a> {
        &self.view
    }

    /// The declared, validated selection.
    #[must_use]
    pub fn selection(&self) -> &PurrembSelection {
        &self.selection
    }

    /// The source-verification mode the binding certified under.
    #[must_use]
    pub fn source_mode(&self) -> SourceVerificationMode {
        self.source_mode
    }

    /// Lowercase hex of the pinned whole-artifact integrity root.
    #[must_use]
    pub fn artifact_root_hex(&self) -> &str {
        &self.artifact_root_hex
    }

    /// Cheaply re-pin the artifact against the retained verification certificate.
    ///
    /// # Errors
    ///
    /// Returns [`StalePin`] when the bytes no longer match the certificate (a certificate
    /// mismatch or any structural reopen failure): the generation changed under the pin.
    pub fn re_pin(&self) -> Result<(), StalePin> {
        match reopen_prevalidated(self.bytes, &self.certificate) {
            Ok(_) => Ok(()),
            Err(EmbeddingError::CertificateMismatch) => Err(StalePin {
                detail: "artifact bytes no longer match the pinned verification certificate"
                    .to_owned(),
            }),
            Err(other) => Err(StalePin {
                detail: format!("artifact could not be re-pinned against its certificate: {other}"),
            }),
        }
    }

    /// Cross-check the artifact's container SHA-256 surfaces against a query's declared
    /// profile-surface digests, failing closed on any disagreement.
    ///
    /// # Errors
    ///
    /// Returns [`PurrembBindingError::ProfileMismatch`] on the first surface that
    /// disagrees (artifact root, source, matrix content, target set, vector space, or
    /// certified RDF digest).
    pub fn cross_check_profile(
        &self,
        expected: &ProfileSurfaceDigests,
    ) -> Result<(), PurrembBindingError> {
        if self.artifact_root.as_bytes() != &expected.artifact_root {
            return Err(PurrembBindingError::ProfileMismatch(
                "artifact integrity root".to_owned(),
            ));
        }
        if self.view.source().source_exact_digest().as_bytes() != &expected.source_exact_digest {
            return Err(PurrembBindingError::ProfileMismatch(
                "exact source-pack digest".to_owned(),
            ));
        }
        let matrix = self
            .view
            .matrix(self.selection.matrix)
            .ok_or_else(|| PurrembBindingError::Selection("selected matrix absent".to_owned()))?;
        if matrix.content_digest().as_bytes() != &expected.matrix_content_digest {
            return Err(PurrembBindingError::ProfileMismatch(
                "stored-matrix content digest".to_owned(),
            ));
        }
        if self.selection.target_set.as_bytes() != &expected.target_set_id {
            return Err(PurrembBindingError::ProfileMismatch(
                "target-set identity".to_owned(),
            ));
        }
        if self.selection.vector_space.as_bytes() != &expected.vector_space_id {
            return Err(PurrembBindingError::ProfileMismatch(
                "vector-space identity".to_owned(),
            ));
        }
        if let Some(expected_rdf) = expected.certified_rdf_digest
            && self.view.source().certified_rdf_digest() != expected_rdf
        {
            return Err(PurrembBindingError::ProfileMismatch(
                "certified RDF digest".to_owned(),
            ));
        }
        Ok(())
    }

    /// Build the provider-identity receipt naming every contributing PURREMB identity.
    #[must_use]
    pub fn receipt(&self) -> PurrembRetrievalReceipt {
        let source = self.view.source();
        let (recall, loss) = match self.selection.policy {
            RetrievalPolicy::ExactFullSpace => (Some(1.0_f64), "none".to_owned()),
            RetrievalPolicy::MatryoshkaPrefixThenRerank => {
                (None, "matryoshka-prefix-rerank-approximation".to_owned())
            }
        };
        let index_guard = self
            .view
            .index_guards()
            .find(|guard| guard.matrix_id() == self.selection.matrix)
            .map(|guard| guard.id().to_hex());
        PurrembRetrievalReceipt {
            artifact_root: self.artifact_root_hex.clone(),
            source_exact_digest: source.source_exact_digest().to_hex(),
            certified_rdf_digest: hex32(&source.certified_rdf_digest()),
            source_verification_mode: source_mode_wire(self.source_mode).to_owned(),
            target_set: self.selection.target_set.to_hex(),
            matrix: self.selection.matrix.to_hex(),
            projection: self.selection.projection.map(|id| id.to_hex()),
            vector_space: self.selection.vector_space.to_hex(),
            family: self.selection.family.to_hex(),
            metric_code: self.selection.metric.code(),
            metric_name: metric_wire(&self.selection.metric).to_owned(),
            effective_dimension: self.selection.effective_dimension,
            postprocessing: postprocessing_wire(self.selection.postprocessing).to_owned(),
            retrieval_policy: self.selection.policy.wire().to_owned(),
            recall,
            loss,
            index_guard,
        }
    }
}

/// Map an embedding-verification / source-certification failure to a binding error.
fn embedding_verification(error: EmbeddingError) -> PurrembBindingError {
    PurrembBindingError::Verification(error.to_string())
}

/// Validate a declared selection against the opened artifact, failing closed.
fn validate_selection(
    view: &EmbeddingView<'_>,
    selection: &PurrembSelection,
) -> Result<(), PurrembBindingError> {
    let family = view
        .family(selection.family)
        .ok_or_else(|| PurrembBindingError::Selection("family absent".to_owned()))?;
    let space = view
        .vector_space(selection.vector_space)
        .ok_or_else(|| PurrembBindingError::Selection("vector space absent".to_owned()))?;
    let target_set = view
        .target_set(selection.target_set)
        .ok_or_else(|| PurrembBindingError::Selection("target set absent".to_owned()))?;
    let matrix = view
        .matrix(selection.matrix)
        .ok_or_else(|| PurrembBindingError::Selection("matrix absent".to_owned()))?;

    if space.family_id() != selection.family {
        return Err(PurrembBindingError::Selection(
            "vector space belongs to another family".to_owned(),
        ));
    }
    if matrix.family_id() != selection.family {
        return Err(PurrembBindingError::Selection(
            "matrix belongs to another family".to_owned(),
        ));
    }
    if matrix.target_set_id() != selection.target_set {
        return Err(PurrembBindingError::Selection(
            "matrix is built over another target set".to_owned(),
        ));
    }
    if matrix.row_count() != target_set.row_count() as u64 {
        return Err(PurrembBindingError::Selection(
            "matrix row count disagrees with target-set row count".to_owned(),
        ));
    }

    // Effective/prefix dimension must fit inside the stored family dimension.
    if selection.effective_dimension == 0 || selection.effective_dimension > family.stored_dimension()
    {
        return Err(PurrembBindingError::Selection(
            "effective dimension is zero or exceeds the stored family dimension".to_owned(),
        ));
    }
    if space.dimension() != selection.effective_dimension {
        return Err(PurrembBindingError::Selection(
            "vector-space dimension disagrees with the declared effective dimension".to_owned(),
        ));
    }
    let space_postprocessing = space
        .postprocessing()
        .map_err(|error| PurrembBindingError::Selection(error.to_string()))?;
    if space_postprocessing != selection.postprocessing {
        return Err(PurrembBindingError::Selection(
            "vector-space postprocessing disagrees with the declared postprocessing".to_owned(),
        ));
    }

    // Declared dtype must equal the matrix dtype: a cross-dtype selection is rejected,
    // never reinterpreted.
    let matrix_dtype = matrix
        .dtype()
        .map_err(|error| PurrembBindingError::Selection(error.to_string()))?;
    if matrix_dtype != selection.dtype {
        return Err(PurrembBindingError::Selection(
            "declared dtype disagrees with the stored-matrix dtype".to_owned(),
        ));
    }
    let family_dtype = family
        .dtype()
        .map_err(|error| PurrembBindingError::Selection(error.to_string()))?;
    if family_dtype != selection.dtype {
        return Err(PurrembBindingError::Selection(
            "declared dtype disagrees with the family contract dtype".to_owned(),
        ));
    }

    // The distance metric lives in the family-contract TLV (it is not a `FamilyView`
    // accessor); cross-check the declared metric's stable code against the contract.
    let contract_metric_code = family_contract_metric_code(family.contract_bytes())
        .map_err(|error| PurrembBindingError::Selection(error.to_string()))?;
    if contract_metric_code != selection.metric.code() {
        return Err(PurrembBindingError::Selection(
            "declared distance metric disagrees with the family contract metric".to_owned(),
        ));
    }

    if let Some(projection_id) = selection.projection {
        let projection = view
            .projection(projection_id)
            .ok_or_else(|| PurrembBindingError::Selection("projection absent".to_owned()))?;
        if projection.matrix_id() != selection.matrix
            || projection.vector_space_id() != selection.vector_space
        {
            return Err(PurrembBindingError::Selection(
                "projection is bound to another matrix or vector space".to_owned(),
            ));
        }
    }

    if selection.policy == RetrievalPolicy::MatryoshkaPrefixThenRerank
        && family.dimensionality_policy() != 2
    {
        return Err(PurrembBindingError::Selection(
            "Matryoshka retrieval requires a Matryoshka family dimensionality policy".to_owned(),
        ));
    }

    Ok(())
}

/// Read the stable distance-metric code from a canonical family-contract block.
///
/// The metric is a nested TLV block at contract tag 14 whose tag 1 carries the `u32`
/// metric code. `FamilyView` exposes no parsed metric accessor, so the code is read
/// directly and cross-checked against the declared selection.
fn family_contract_metric_code(contract_bytes: &[u8]) -> Result<u32, EmbeddingError> {
    for entry in canonical_tlv(contract_bytes)? {
        if entry.tag == 14 && entry.wire_type == TlvWireType::Block {
            for inner in canonical_tlv(entry.value)? {
                if inner.tag == 1 && inner.wire_type == TlvWireType::U32 {
                    return tlv_u32(inner.value);
                }
            }
            return Err(EmbeddingError::Missing("family contract metric code"));
        }
    }
    Err(EmbeddingError::Missing("family contract metric block"))
}

// --------------------------------------------------------------------------- //
// RDF 1.2 target mapping.
// --------------------------------------------------------------------------- //

/// Reconstructs canonical RDF 1.2 identities from PURREMB targets.
///
/// One PURREMB target reconstructs to exactly one RDF 1.2 term (a triple term recurses
/// through its component targets), so [`Self::map_row`] returns a single [`TermValue`]
/// validated against the declared candidate column. `RdfTerm` (IRI / literal / blank /
/// triple) and `RdfStatement` (quoted triple) are reconstructed; a digest-only target,
/// or any kind whose identity cannot be reconstructed losslessly here, is rejected — no
/// IRI is fabricated. A `source_local_ordinal` hint is never used as identity.
#[derive(Debug, Default, Clone, Copy)]
pub struct PurrembTargetMapper;

impl PurrembTargetMapper {
    /// Reconstruct the RDF 1.2 term for one target-set row and validate it against the
    /// declared candidate column kind.
    ///
    /// # Errors
    ///
    /// Returns [`PurrembMapError`] when the row is absent, the target is digest-only or
    /// of a non-reconstructable kind, the retained identity block is malformed, or the
    /// reconstructed term does not conform to `candidate_kind`.
    pub fn map_row(
        &self,
        view: &EmbeddingView<'_>,
        target_set: &TargetSetView<'_>,
        row: usize,
        candidate_kind: &ColumnKind,
    ) -> Result<TermValue, PurrembMapError> {
        let target_id = target_set
            .target(row)
            .ok_or(PurrembMapError::MissingRow(row))?;
        let term = reconstruct_target(view, target_id, 0)?;
        if !term_conforms(&term, candidate_kind) {
            return Err(PurrembMapError::ColumnMismatch(format!(
                "term does not conform to {}",
                column_kind_wire(candidate_kind)
            )));
        }
        Ok(term)
    }
}

/// Reconstruct the RDF 1.2 term for one target id, recursing through triple components.
fn reconstruct_target(
    view: &EmbeddingView<'_>,
    target_id: TargetId,
    depth: u32,
) -> Result<TermValue, PurrembMapError> {
    if depth > MAX_TERM_DEPTH {
        return Err(PurrembMapError::MalformedIdentity(
            "triple-term nesting exceeds the reconstruction depth guard".to_owned(),
        ));
    }
    let target = view
        .target(target_id)
        .ok_or_else(|| PurrembMapError::MissingTarget(target_id.to_hex()))?;
    let kind = target
        .kind()
        .map_err(|error| PurrembMapError::MalformedIdentity(error.to_string()))?;
    let identity = target
        .identity_bytes()
        .ok_or_else(|| PurrembMapError::DigestOnly(target_id.to_hex()))?;
    match kind {
        TargetKind::RdfTerm => reconstruct_rdf_term(view, identity, depth),
        TargetKind::RdfStatement => reconstruct_rdf_statement(view, identity, depth),
        other => Err(PurrembMapError::UnsupportedKind(format!(
            "target kind {other:?} has no lossless RDF term reconstruction here",
        ))),
    }
}

/// Reconstruct an `RdfTerm` target from its canonical identity TLV block.
fn reconstruct_rdf_term(
    view: &EmbeddingView<'_>,
    identity: &[u8],
    depth: u32,
) -> Result<TermValue, PurrembMapError> {
    let fields = TargetIdentityFields::parse(identity)?;
    let term_kind = fields.u32(1)?;
    match term_kind {
        1 => {
            // IRI.
            let iri = fields.utf8(2)?;
            Ok(TermValue::iri(iri.to_owned()))
        }
        2 => {
            // Blank node: dataset-scoped canonical label without the `_:` prefix.
            let label = fields.utf8(3)?;
            Ok(TermValue::Blank {
                label: label.to_owned(),
                scope: BlankScope::DEFAULT,
            })
        }
        3 => {
            // Literal.
            let lexical = fields.utf8(2)?;
            let datatype = fields.utf8(3)?;
            let language = fields.utf8_optional(4)?;
            let direction = match fields.u32_optional(5)? {
                None | Some(0) => None,
                Some(1) => Some(RdfTextDirection::Ltr),
                Some(2) => Some(RdfTextDirection::Rtl),
                Some(other) => {
                    return Err(PurrembMapError::MalformedIdentity(format!(
                        "literal direction code {other} is not canonical",
                    )));
                }
            };
            Ok(TermValue::Literal {
                lexical_form: lexical.to_owned(),
                datatype: datatype.to_owned(),
                language: language.map(str::to_owned),
                direction,
            })
        }
        4 => {
            // Recursive triple term by component target ids.
            let subject = reconstruct_target(view, fields.target(2)?, depth + 1)?;
            let predicate = reconstruct_target(view, fields.target(3)?, depth + 1)?;
            let object = reconstruct_target(view, fields.target(4)?, depth + 1)?;
            require_iri_predicate(&predicate)?;
            Ok(TermValue::Triple {
                s: Box::new(subject),
                p: Box::new(predicate),
                o: Box::new(object),
            })
        }
        other => Err(PurrembMapError::MalformedIdentity(format!(
            "RDF term kind code {other} is not canonical",
        ))),
    }
}

/// Reconstruct an `RdfStatement` target as an RDF 1.2 quoted-triple term over its
/// subject / predicate / object component targets (its graph scope is not part of the
/// triple-term identity).
fn reconstruct_rdf_statement(
    view: &EmbeddingView<'_>,
    identity: &[u8],
    depth: u32,
) -> Result<TermValue, PurrembMapError> {
    let fields = TargetIdentityFields::parse(identity)?;
    // Canonical four-digest block: tag 1 graph, tag 2 subject, tag 3 predicate, tag 4 object.
    let subject = reconstruct_target(view, fields.target(2)?, depth + 1)?;
    let predicate = reconstruct_target(view, fields.target(3)?, depth + 1)?;
    let object = reconstruct_target(view, fields.target(4)?, depth + 1)?;
    require_iri_predicate(&predicate)?;
    Ok(TermValue::Triple {
        s: Box::new(subject),
        p: Box::new(predicate),
        o: Box::new(object),
    })
}

/// Reject a triple term whose predicate is not an IRI (RDF 1.2 requires an IRI predicate).
fn require_iri_predicate(predicate: &TermValue) -> Result<(), PurrembMapError> {
    if predicate.is_iri() {
        Ok(())
    } else {
        Err(PurrembMapError::MalformedIdentity(
            "triple-term predicate is not an IRI".to_owned(),
        ))
    }
}

/// A parsed, tag-indexed canonical target-identity TLV block.
struct TargetIdentityFields<'a> {
    entries: Vec<(u16, TlvWireType, &'a [u8])>,
}

impl<'a> TargetIdentityFields<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, PurrembMapError> {
        let iterator =
            canonical_tlv(bytes).map_err(|error| PurrembMapError::MalformedIdentity(error.to_string()))?;
        let entries = iterator
            .map(|entry| (entry.tag, entry.wire_type, entry.value))
            .collect();
        Ok(Self { entries })
    }

    fn find(&self, tag: u16) -> Option<(TlvWireType, &'a [u8])> {
        self.entries
            .iter()
            .find(|(candidate, _, _)| *candidate == tag)
            .map(|(_, wire, value)| (*wire, *value))
    }

    fn u32(&self, tag: u16) -> Result<u32, PurrembMapError> {
        self.u32_optional(tag)?
            .ok_or_else(|| PurrembMapError::MalformedIdentity(format!("missing u32 field {tag}")))
    }

    fn u32_optional(&self, tag: u16) -> Result<Option<u32>, PurrembMapError> {
        match self.find(tag) {
            None => Ok(None),
            Some((TlvWireType::U32, value)) => tlv_u32(value)
                .map(Some)
                .map_err(|error| PurrembMapError::MalformedIdentity(error.to_string())),
            Some(_) => Err(PurrembMapError::MalformedIdentity(format!(
                "field {tag} is not a u32",
            ))),
        }
    }

    fn utf8(&self, tag: u16) -> Result<&'a str, PurrembMapError> {
        self.utf8_optional(tag)?
            .ok_or_else(|| PurrembMapError::MalformedIdentity(format!("missing utf8 field {tag}")))
    }

    fn utf8_optional(&self, tag: u16) -> Result<Option<&'a str>, PurrembMapError> {
        match self.find(tag) {
            None => Ok(None),
            Some((TlvWireType::Utf8, value)) => std::str::from_utf8(value)
                .map(Some)
                .map_err(|error| PurrembMapError::MalformedIdentity(error.to_string())),
            Some(_) => Err(PurrembMapError::MalformedIdentity(format!(
                "field {tag} is not utf8",
            ))),
        }
    }

    fn target(&self, tag: u16) -> Result<TargetId, PurrembMapError> {
        match self.find(tag) {
            Some((TlvWireType::Digest32, value)) => {
                let bytes: [u8; 32] = value.try_into().map_err(|_| {
                    PurrembMapError::MalformedIdentity(format!("field {tag} is not 32 bytes"))
                })?;
                Ok(TargetId::from_raw(bytes))
            }
            _ => Err(PurrembMapError::MalformedIdentity(format!(
                "missing digest32 field {tag}",
            ))),
        }
    }
}

/// Read a little-endian `u32` from an exact 4-byte TLV value.
fn tlv_u32(value: &[u8]) -> Result<u32, EmbeddingError> {
    let bytes: [u8; 4] = value
        .try_into()
        .map_err(|_| EmbeddingError::MalformedTlv("u32 field length"))?;
    Ok(u32::from_le_bytes(bytes))
}

/// Whether a reconstructed term conforms to a declared column kind.
fn term_conforms(term: &TermValue, column: &ColumnKind) -> bool {
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

/// Stable wire label of a column kind, for diagnostics.
fn column_kind_wire(kind: &ColumnKind) -> String {
    match kind {
        ColumnKind::Iri => "iri".to_owned(),
        ColumnKind::BlankNode => "blank-node".to_owned(),
        ColumnKind::TripleTerm => "triple-term".to_owned(),
        ColumnKind::Literal { datatype: None } => "literal:*".to_owned(),
        ColumnKind::Literal {
            datatype: Some(datatype),
        } => format!("literal:{datatype}"),
    }
}

// --------------------------------------------------------------------------- //
// Metric scoring and the IEEE-754 total-order key.
// --------------------------------------------------------------------------- //

/// Compute a metric-correct distance (smaller is closer) between two `f32` vectors.
///
/// Cosine returns `1 − cos`; negative-dot returns `−(a·b)`; squared-Euclidean returns
/// `‖a − b‖²`. A zero-magnitude vector under cosine (`0/0`) or any non-finite result is a
/// hard error — a NaN is never propagated. An extension metric has no canonical scoring
/// rule here and is rejected.
///
/// # Errors
///
/// Returns [`ScoreError`] on a dimension mismatch, an unsupported metric, a zero-magnitude
/// cosine vector, or a non-finite score.
pub fn score(query: &[f32], candidate: &[f32], metric: &DistanceMetric) -> Result<f64, ScoreError> {
    if query.len() != candidate.len() {
        return Err(ScoreError::DimensionMismatch);
    }
    let value = match metric {
        DistanceMetric::Cosine => {
            let mut dot = 0.0_f64;
            let mut norm_query = 0.0_f64;
            let mut norm_candidate = 0.0_f64;
            for (&left, &right) in query.iter().zip(candidate) {
                let left = f64::from(left);
                let right = f64::from(right);
                dot += left * right;
                norm_query += left * left;
                norm_candidate += right * right;
            }
            let denominator = norm_query.sqrt() * norm_candidate.sqrt();
            if denominator == 0.0 {
                return Err(ScoreError::ZeroMagnitude);
            }
            1.0 - (dot / denominator)
        }
        DistanceMetric::NegativeDot => {
            let mut dot = 0.0_f64;
            for (&left, &right) in query.iter().zip(candidate) {
                dot += f64::from(left) * f64::from(right);
            }
            -dot
        }
        DistanceMetric::SquaredEuclidean => {
            let mut sum = 0.0_f64;
            for (&left, &right) in query.iter().zip(candidate) {
                let delta = f64::from(left) - f64::from(right);
                sum += delta * delta;
            }
            sum
        }
        DistanceMetric::Extension { .. } => return Err(ScoreError::UnsupportedMetric),
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ScoreError::NonFinite)
    }
}

/// The IEEE-754 total-order bit transform of an `f64`: for `bits = value.to_bits()`,
/// `bits ^ (((bits >> 63) as u64).wrapping_neg() | 0x8000_0000_0000_0000)`. The result's
/// unsigned integer order matches the numeric order of the input (negatives included), so
/// a fixed-width hex encoding sorts lexically exactly as the scores sort numerically.
#[must_use]
pub fn total_order_bits(value: f64) -> u64 {
    let bits = value.to_bits();
    bits ^ ((bits >> 63).wrapping_neg() | 0x8000_0000_0000_0000)
}

/// Inverse of [`total_order_bits`].
#[must_use]
pub fn from_total_order_bits(key: u64) -> f64 {
    let mask = if key & 0x8000_0000_0000_0000 != 0 {
        0x8000_0000_0000_0000
    } else {
        u64::MAX
    };
    f64::from_bits(key ^ mask)
}

/// Fixed-width lowercase-hex order key from total-order bits.
#[must_use]
fn order_key_hex(bits: u64) -> String {
    format!("{bits:016x}")
}

// --------------------------------------------------------------------------- //
// Bounded top-k retrieval engine.
// --------------------------------------------------------------------------- //

/// One scanned candidate awaiting mapping into a tuple.
#[derive(Debug, Clone, Copy)]
struct ScannedCandidate {
    /// Matrix / target-set row.
    row: usize,
    /// Metric distance (smaller is closer).
    distance: f64,
    /// Total-order bits of the distance (the ordered prefix key).
    bits: u64,
    /// Candidate target identity, the deterministic in-heap tie-break.
    target: [u8; 32],
}

/// A fixed-capacity bounded top-k selector.
///
/// The comparator mirrors [`RelationOrdering::compare_rows`]: the primary key is the
/// total-order distance bits, reversed for a descending order; ties are broken by the
/// candidate target identity (a deterministic, allocation-free proxy for the emitted RDF
/// argument order, which the provider applies when it finally sorts the selected prefix).
/// It keeps at most `k` candidates in `O(k)` memory and `O(N log k)` time, evicting the
/// current worst per row rather than allocating or sorting a tuple for every corpus row.
struct BoundedTopK {
    capacity: usize,
    direction: RelationOrderDirection,
    heap: BinaryHeap<HeapEntry>,
}

impl BoundedTopK {
    fn new(capacity: usize, direction: RelationOrderDirection) -> Self {
        Self {
            capacity,
            direction,
            heap: BinaryHeap::new(),
        }
    }

    fn offer(&mut self, candidate: ScannedCandidate) {
        if self.capacity == 0 {
            return;
        }
        let entry = HeapEntry {
            direction: self.direction,
            candidate,
        };
        if self.heap.len() < self.capacity {
            self.heap.push(entry);
        } else if let Some(worst) = self.heap.peek()
            && entry < *worst
        {
            self.heap.pop();
            self.heap.push(entry);
        }
    }

    /// Drain the selected candidates in best-first order (primary key, then target).
    fn into_ranked(self) -> Vec<ScannedCandidate> {
        let mut selected: Vec<HeapEntry> = self.heap.into_vec();
        selected.sort_unstable();
        selected.into_iter().map(|entry| entry.candidate).collect()
    }
}

/// A heap element whose ordering is the retrieval "badness" (greater is worse) so a
/// max-heap keeps the current worst on top for eviction.
#[derive(Debug, Clone, Copy)]
struct HeapEntry {
    direction: RelationOrderDirection,
    candidate: ScannedCandidate,
}

impl HeapEntry {
    fn ordering(&self, other: &Self) -> std::cmp::Ordering {
        let primary = self.candidate.bits.cmp(&other.candidate.bits);
        let primary = match self.direction {
            RelationOrderDirection::Ascending => primary,
            RelationOrderDirection::Descending => primary.reverse(),
        };
        // The tuple tie-break is always ascending, regardless of the primary direction.
        primary.then_with(|| self.candidate.target.cmp(&other.candidate.target))
    }
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.ordering(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for HeapEntry {}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ordering(other)
    }
}

/// The deterministic PURREMB retrieval engine.
///
/// Supports the moded call `retrieve(bound query target, unbound candidate)`: a single
/// bound in-corpus query slot and a single unbound candidate slot. Any other mode shape
/// is rejected. Cancellation is polled at row-block boundaries; an observed cancellation
/// returns a typed cancellation (the engine maps it to `Cancelled`).
struct PurrembScanner;

impl PurrembScanner {
    /// Scan for the `limit` nearest candidates to `query_row` under `selection.policy`.
    fn scan(
        binding: &PurrembBinding<'_>,
        query_row: usize,
        limit: usize,
        direction: RelationOrderDirection,
        cancellation: &dyn RelationCancellation,
    ) -> Result<Vec<ScannedCandidate>, ScanError> {
        match binding.selection.policy {
            RetrievalPolicy::ExactFullSpace => {
                Self::scan_full(binding, query_row, limit, direction, cancellation)
            }
            RetrievalPolicy::MatryoshkaPrefixThenRerank => {
                Self::scan_matryoshka(binding, query_row, limit, direction, cancellation)
            }
        }
    }

    /// Exact full-space scan over every stored row.
    fn scan_full(
        binding: &PurrembBinding<'_>,
        query_row: usize,
        limit: usize,
        direction: RelationOrderDirection,
        cancellation: &dyn RelationCancellation,
    ) -> Result<Vec<ScannedCandidate>, ScanError> {
        let view = &binding.view;
        let selection = &binding.selection;
        let matrix = view
            .matrix(selection.matrix)
            .ok_or_else(|| ScanError::Rejected("selected matrix absent".to_owned()))?;
        let target_set = view
            .target_set(selection.target_set)
            .ok_or_else(|| ScanError::Rejected("selected target set absent".to_owned()))?;
        let query = collect_full_row(matrix, query_row as u64, selection.dtype)?;

        let mut heap = BoundedTopK::new(limit, direction);
        let row_count = matrix.row_count();
        for row in 0..row_count {
            if (row as usize).is_multiple_of(CANCEL_POLL_BLOCK) && cancellation.is_cancelled() {
                return Err(ScanError::Cancelled);
            }
            let candidate = collect_full_row(matrix, row, selection.dtype)?;
            let distance = score_f64(&query, &candidate, &selection.metric)?;
            heap.offer(scanned(&target_set, row, distance)?);
        }
        Ok(heap.into_ranked())
    }

    /// Matryoshka coarse leading-prefix pass, then a full-space rerank of the pool.
    fn scan_matryoshka(
        binding: &PurrembBinding<'_>,
        query_row: usize,
        limit: usize,
        direction: RelationOrderDirection,
        cancellation: &dyn RelationCancellation,
    ) -> Result<Vec<ScannedCandidate>, ScanError> {
        let view = &binding.view;
        let selection = &binding.selection;
        let effective = view
            .effective_matrix(selection.target_set, selection.vector_space)
            .map_err(|error| ScanError::Rejected(error.to_string()))?
            .ok_or_else(|| {
                ScanError::Rejected("no effective matrix for the target set and space".to_owned())
            })?;
        let matrix = view
            .matrix(selection.matrix)
            .ok_or_else(|| ScanError::Rejected("selected matrix absent".to_owned()))?;
        let target_set = view
            .target_set(selection.target_set)
            .ok_or_else(|| ScanError::Rejected("selected target set absent".to_owned()))?;

        // Coarse pass over the deterministically-projected leading prefix.
        let query_prefix = collect_effective_row(effective, query_row as u64, selection.dtype)?;
        let pool_capacity = limit.saturating_mul(MATRYOSHKA_RERANK_FACTOR);
        let mut coarse = BoundedTopK::new(pool_capacity, direction);
        let row_count = matrix.row_count();
        for row in 0..row_count {
            if (row as usize).is_multiple_of(CANCEL_POLL_BLOCK) && cancellation.is_cancelled() {
                return Err(ScanError::Cancelled);
            }
            let candidate = collect_effective_row(effective, row, selection.dtype)?;
            let distance = score_f64(&query_prefix, &candidate, &selection.metric)?;
            coarse.offer(scanned(&target_set, row, distance)?);
        }

        // Full-space rerank over the pooled candidates only.
        let query_full = collect_full_row(matrix, query_row as u64, selection.dtype)?;
        let mut rerank = BoundedTopK::new(limit, direction);
        for pooled in coarse.into_ranked() {
            if cancellation.is_cancelled() {
                return Err(ScanError::Cancelled);
            }
            let candidate = collect_full_row(matrix, pooled.row as u64, selection.dtype)?;
            let distance = score_f64(&query_full, &candidate, &selection.metric)?;
            rerank.offer(scanned(&target_set, pooled.row as u64, distance)?);
        }
        Ok(rerank.into_ranked())
    }
}

/// Assemble a scanned candidate, folding the distance into the ordered-prefix key.
fn scanned(
    target_set: &TargetSetView<'_>,
    row: u64,
    distance: f64,
) -> Result<ScannedCandidate, ScanError> {
    let target = target_set
        .target(row as usize)
        .ok_or_else(|| ScanError::Rejected(format!("target set has no row {row}")))?;
    Ok(ScannedCandidate {
        row: row as usize,
        distance,
        bits: total_order_bits(distance),
        target: target.into_bytes(),
    })
}

/// A scanner-level failure.
enum ScanError {
    /// A well-formed request rejected under the provider contract.
    Rejected(String),
    /// Cancellation observed at a row-block boundary.
    Cancelled,
}

impl From<ScoreError> for ScanError {
    fn from(error: ScoreError) -> Self {
        Self::Rejected(error.to_string())
    }
}

impl From<EmbeddingError> for ScanError {
    fn from(error: EmbeddingError) -> Self {
        Self::Rejected(error.to_string())
    }
}

/// Score two `f64` vectors under a metric, sharing the `f32` scoring rule's semantics.
fn score_f64(query: &[f64], candidate: &[f64], metric: &DistanceMetric) -> Result<f64, ScoreError> {
    if query.len() != candidate.len() {
        return Err(ScoreError::DimensionMismatch);
    }
    let value = match metric {
        DistanceMetric::Cosine => {
            let mut dot = 0.0_f64;
            let mut norm_query = 0.0_f64;
            let mut norm_candidate = 0.0_f64;
            for (&left, &right) in query.iter().zip(candidate) {
                dot += left * right;
                norm_query += left * left;
                norm_candidate += right * right;
            }
            let denominator = norm_query.sqrt() * norm_candidate.sqrt();
            if denominator == 0.0 {
                return Err(ScoreError::ZeroMagnitude);
            }
            1.0 - (dot / denominator)
        }
        DistanceMetric::NegativeDot => {
            let mut dot = 0.0_f64;
            for (&left, &right) in query.iter().zip(candidate) {
                dot += left * right;
            }
            -dot
        }
        DistanceMetric::SquaredEuclidean => {
            let mut sum = 0.0_f64;
            for (&left, &right) in query.iter().zip(candidate) {
                let delta = left - right;
                sum += delta * delta;
            }
            sum
        }
        DistanceMetric::Extension { .. } => return Err(ScoreError::UnsupportedMetric),
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ScoreError::NonFinite)
    }
}

/// Collect one full stored matrix row as finite `f64` scalars, preferring the aligned
/// native slice unlocked by full verification.
fn collect_full_row(
    matrix: MatrixView<'_>,
    row: u64,
    dtype: VectorDtype,
) -> Result<Vec<f64>, EmbeddingError> {
    match dtype {
        VectorDtype::F32 => {
            if let Some(native) = matrix.native_f32_row(row) {
                Ok(native.iter().map(|&value| f64::from(value)).collect())
            } else {
                matrix
                    .f32_row(row)?
                    .map(|value| value.map(f64::from))
                    .collect()
            }
        }
        VectorDtype::F64 => {
            if let Some(native) = matrix.native_f64_row(row) {
                Ok(native.to_vec())
            } else {
                matrix.f64_row(row)?.collect()
            }
        }
    }
}

/// Collect one deterministically-projected effective (leading-prefix) row as finite
/// `f64` scalars.
fn collect_effective_row(
    effective: purrdf::EffectiveMatrixView<'_>,
    row: u64,
    dtype: VectorDtype,
) -> Result<Vec<f64>, EmbeddingError> {
    match dtype {
        VectorDtype::F32 => effective
            .f32_row(row)?
            .map(|value| value.map(f64::from))
            .collect(),
        VectorDtype::F64 => effective.f64_row(row)?.collect(),
    }
}

// --------------------------------------------------------------------------- //
// Vector-space-scoped annotation algebra.
// --------------------------------------------------------------------------- //

/// A total-order-encoded score carried in the annotation algebra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderedScoreBits(pub u64);

impl OrderedScoreBits {
    /// Encode a distance into total-order bits.
    #[must_use]
    pub fn from_distance(distance: f64) -> Self {
        Self(total_order_bits(distance))
    }

    /// Decode the carried distance.
    #[must_use]
    pub fn distance(self) -> f64 {
        from_total_order_bits(self.0)
    }
}

/// One space-tagged retrieval score: the ordered distance, the contributing vector-space
/// identities, and the metric it was computed under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceTaggedScore {
    /// Total-order-encoded distance (smaller distance is closer).
    pub score: OrderedScoreBits,
    /// Contributing effective vector-space identities.
    pub spaces: BTreeSet<[u8; 32]>,
    /// Stable metric code the score was computed under.
    pub metric_code: u32,
}

impl SpaceTaggedScore {
    /// A single-space score for one contributing vector space.
    #[must_use]
    pub fn single(distance: f64, vector_space: VectorSpaceId, metric_code: u32) -> Self {
        let mut spaces = BTreeSet::new();
        spaces.insert(vector_space.into_bytes());
        Self {
            score: OrderedScoreBits::from_distance(distance),
            spaces,
            metric_code,
        }
    }

    /// Whether this element is an algebra identity (no contributing space).
    fn is_identity(&self) -> bool {
        self.spaces.is_empty()
    }
}

/// A reusable annotation algebra over [`SpaceTaggedScore`] elements that moves cross-space
/// refusal into the evaluator.
///
/// `add` (`⊕`) chooses the better alternative derivation (the smaller distance under the
/// tropical min) and unions contributing spaces. `multiply` (`⊗`) combines conjunctive
/// premises by summing distances and unioning spaces — but **hard-fails** when the two
/// elements carry incompatible vector spaces (distinct `VectorSpaceId`s with no licensing
/// correspondence). A query-scoped licensing set, constructed from `logic:Correspondence`
/// facts, permits named cross-space pairs. The refusal makes `⊗` a partial operation, so
/// the algebra declares a `SemiringLaw` deviation through
/// [`AnnotationContract::complete_over`](crate::annotation::AnnotationContract::complete_over).
pub struct VectorSpaceScopedAlgebra {
    identity: String,
    deviations: BTreeSet<crate::annotation::SemiringLaw>,
    licensing: BTreeSet<([u8; 32], [u8; 32])>,
}

impl VectorSpaceScopedAlgebra {
    /// Base algebra identity IRI, extended by the declared deviation set for stability
    /// and cache distinctness.
    const BASE_IRI: &'static str = "https://blackcatinformatics.ca/logic/VectorSpaceScopedAlgebra";

    /// Construct the algebra with an explicit `SemiringLaw` deviation set and a
    /// query-scoped cross-space licensing set.
    #[must_use]
    pub fn new(
        deviations: BTreeSet<crate::annotation::SemiringLaw>,
        licensing: BTreeSet<([u8; 32], [u8; 32])>,
    ) -> Self {
        let mut identity = format!("{}#deviations=", Self::BASE_IRI);
        let mut first = true;
        for law in &deviations {
            if !first {
                identity.push(',');
            }
            identity.push_str(law.wire());
            first = false;
        }
        Self {
            identity,
            deviations,
            licensing,
        }
    }

    /// The default cross-space refusal deviation: the space refusal makes `⊗` partial, so
    /// total multiplicative closure — and therefore multiplicative associativity — no
    /// longer holds universally.
    #[must_use]
    pub fn with_cross_space_refusal(licensing: BTreeSet<([u8; 32], [u8; 32])>) -> Self {
        let mut deviations = BTreeSet::new();
        deviations.insert(crate::annotation::SemiringLaw::MultiplyAssociative);
        Self::new(deviations, licensing)
    }

    /// The annotation admission contract disclosing the declared deviation, scoped to the
    /// certified query classes.
    #[must_use]
    pub fn annotation_contract(
        &self,
        certified_for: impl IntoIterator<Item = crate::annotation::AnnotationQueryClass>,
    ) -> crate::annotation::AnnotationContract {
        crate::annotation::AnnotationContract::complete_over(
            self.deviations.iter().copied(),
            certified_for,
        )
    }

    /// Whether two vector-space identities may be combined under `⊗` (same space, or a
    /// licensed correspondence in either direction).
    fn licensed(&self, left: &[u8; 32], right: &[u8; 32]) -> bool {
        left == right
            || self.licensing.contains(&(*left, *right))
            || self.licensing.contains(&(*right, *left))
    }

    /// Whether every cross pair of contributing spaces is licensed.
    fn spaces_compatible(&self, left: &SpaceTaggedScore, right: &SpaceTaggedScore) -> bool {
        left.spaces
            .iter()
            .all(|a| right.spaces.iter().all(|b| self.licensed(a, b)))
    }
}

impl crate::annotation::TupleAnnotationAlgebra for VectorSpaceScopedAlgebra {
    type Element = SpaceTaggedScore;

    fn identity(&self) -> &str {
        &self.identity
    }

    fn canonical_element(&self, element: &Self::Element) -> String {
        let mut out = format!("{:016x}:", element.score.0);
        for space in &element.spaces {
            out.push_str(&hex32(space));
            out.push(',');
        }
        out.push(':');
        out.push_str(&format!("{:08x}", element.metric_code));
        out
    }

    fn zero(&self) -> Self::Element {
        // No derivation: the tropical additive identity is the worst (infinite) distance.
        SpaceTaggedScore {
            score: OrderedScoreBits::from_distance(f64::INFINITY),
            spaces: BTreeSet::new(),
            metric_code: 0,
        }
    }

    fn one(&self) -> Self::Element {
        // Unit evidence: the tropical multiplicative identity is zero distance.
        SpaceTaggedScore {
            score: OrderedScoreBits::from_distance(0.0),
            spaces: BTreeSet::new(),
            metric_code: 0,
        }
    }

    fn add(
        &self,
        left: &Self::Element,
        right: &Self::Element,
    ) -> gmeow_errors::Result<Self::Element> {
        if left.is_identity() {
            return Ok(right.clone());
        }
        if right.is_identity() {
            return Ok(left.clone());
        }
        if left.metric_code != right.metric_code {
            return Err(space_algebra_error(
                "cannot combine alternative derivations computed under different metrics",
            ));
        }
        // Tropical ⊕ = min distance; union the contributing spaces.
        let score = if left.score.distance() <= right.score.distance() {
            left.score
        } else {
            right.score
        };
        let mut spaces = left.spaces.clone();
        spaces.extend(right.spaces.iter().copied());
        Ok(SpaceTaggedScore {
            score,
            spaces,
            metric_code: left.metric_code,
        })
    }

    fn multiply(
        &self,
        left: &Self::Element,
        right: &Self::Element,
    ) -> gmeow_errors::Result<Self::Element> {
        if left.is_identity() {
            return Ok(right.clone());
        }
        if right.is_identity() {
            return Ok(left.clone());
        }
        if left.metric_code != right.metric_code {
            return Err(space_algebra_error(
                "cannot combine conjunctive premises computed under different metrics",
            ));
        }
        if !self.spaces_compatible(left, right) {
            return Err(space_algebra_error(
                "conjunction of retrieval premises across incompatible vector spaces without a \
                 licensing correspondence",
            ));
        }
        // Tropical ⊗ = distance sum; union the contributing spaces.
        let distance = left.score.distance() + right.score.distance();
        if !distance.is_finite() {
            return Err(space_algebra_error(
                "conjunction of retrieval premises produced a non-finite distance",
            ));
        }
        let mut spaces = left.spaces.clone();
        spaces.extend(right.spaces.iter().copied());
        Ok(SpaceTaggedScore {
            score: OrderedScoreBits::from_distance(distance),
            spaces,
            metric_code: left.metric_code,
        })
    }
}

/// A vector-space-scoped algebra failure on the shared diagnostic substrate.
fn space_algebra_error(detail: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical {
        detail: detail.to_owned(),
    })
}

// --------------------------------------------------------------------------- //
// Retrieval receipt.
// --------------------------------------------------------------------------- //

/// A retrieval receipt naming every contributing PURREMB identity, riding alongside the
/// standard provider lineage the dispatch layer builds.
#[derive(Debug, Clone, PartialEq)]
pub struct PurrembRetrievalReceipt {
    /// Pinned whole-artifact integrity root (hex).
    pub artifact_root: String,
    /// Exact source-pack digest (hex).
    pub source_exact_digest: String,
    /// Independently certified RDF digest (hex).
    pub certified_rdf_digest: String,
    /// Source-verification mode the binding certified under.
    pub source_verification_mode: String,
    /// Target-set identity (hex).
    pub target_set: String,
    /// Stored-matrix identity (hex).
    pub matrix: String,
    /// Effective-projection identity (hex), when a Matryoshka pass is used.
    pub projection: Option<String>,
    /// Effective vector-space identity (hex).
    pub vector_space: String,
    /// Family identity (hex).
    pub family: String,
    /// Stable distance-metric code.
    pub metric_code: u32,
    /// Distance-metric wire name.
    pub metric_name: String,
    /// Effective (leading-prefix) dimension.
    pub effective_dimension: u32,
    /// Prefix postprocessing wire name.
    pub postprocessing: String,
    /// Selected retrieval policy wire name.
    pub retrieval_policy: String,
    /// Declared recall (`1.0` for an exact full-space scan; `None` for the approximate
    /// Matryoshka rerank).
    pub recall: Option<f64>,
    /// Declared loss disclosure (`none` for an exact scan).
    pub loss: String,
    /// Guarding derived-index identity (hex), when one covers the selected matrix.
    pub index_guard: Option<String>,
}

// --------------------------------------------------------------------------- //
// The orchestrating provider.
// --------------------------------------------------------------------------- //

/// A computed retrieval score handed to the query's annotation mapper.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetrievalScore {
    /// Metric distance (smaller is closer).
    pub distance: f64,
    /// Stable metric code the distance was computed under.
    pub metric_code: u32,
    /// Zero-based rank within the returned ordered prefix.
    pub rank: u64,
    /// Effective vector-space identity the distance was computed in.
    pub vector_space: [u8; 32],
}

/// The PURREMB retrieval provider: a thin orchestrator over a verified binding, the
/// retrieval engine, and the target mapper, generic over the query's annotation element.
///
/// `call()` re-pins the artifact (a changed generation is `StaleGeneration`), resolves the
/// bound in-corpus query target, scans the top-`limit` candidates, maps each to its RDF
/// 1.2 identity, and returns a complete ordered-prefix batch pinned to the descriptor's
/// artifact generation. The injected `annotate` closure maps each computed
/// [`RetrievalScore`] into the query's algebra element for the descriptor's selected
/// annotation dimension, so the provider works with [`VectorSpaceScopedAlgebra`]
/// (`E = SpaceTaggedScore`) or a plain scalar algebra alike.
pub struct PurrembRetrievalProvider<'a, E> {
    binding: PurrembBinding<'a>,
    descriptor: RelationProviderDescriptor,
    mapper: PurrembTargetMapper,
    annotate: Box<dyn Fn(RetrievalScore) -> E + Send + Sync>,
    profile: Option<ProfileSurfaceDigests>,
}

impl<'a, E> PurrembRetrievalProvider<'a, E> {
    /// Build a retrieval provider.
    ///
    /// The `descriptor` names the relation, its RDF 1.2 argument schema, the annotation
    /// dimension, and the ordering pushed on every call; its `artifact_generation` must
    /// name this binding's pinned artifact root (see [`purremb_generation_iri`]) so a
    /// returned batch's generation matches the descriptor. `annotate` maps a computed
    /// score into the query's annotation element.
    ///
    /// # Errors
    ///
    /// Returns [`RelationContractError`] if the descriptor's argument schema is not the
    /// binary `(query, candidate)` retrieval shape, if the annotation dimension is
    /// [`RelationAnnotationDimension::EpistemicConfidence`] (a similarity/distance/rank
    /// value is never epistemic confidence), or if the descriptor's artifact generation
    /// does not name this binding's pinned artifact root.
    pub fn new(
        binding: PurrembBinding<'a>,
        descriptor: RelationProviderDescriptor,
        annotate: Box<dyn Fn(RetrievalScore) -> E + Send + Sync>,
    ) -> Result<Self, RelationContractError> {
        Self::with_profile(binding, descriptor, annotate, None)
    }

    /// Build a retrieval provider that additionally cross-checks a declared profile
    /// surface against the binding on every call.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn with_profile(
        binding: PurrembBinding<'a>,
        descriptor: RelationProviderDescriptor,
        annotate: Box<dyn Fn(RetrievalScore) -> E + Send + Sync>,
        profile: Option<ProfileSurfaceDigests>,
    ) -> Result<Self, RelationContractError> {
        if descriptor.arity() != 2 {
            return Err(RelationContractError {
                detail: format!(
                    "PURREMB retrieval relation <{}> must declare a binary (query, candidate) schema",
                    descriptor.relation_iri
                ),
            });
        }
        if descriptor.annotation_dimension == RelationAnnotationDimension::EpistemicConfidence {
            return Err(RelationContractError {
                detail: format!(
                    "PURREMB retrieval relation <{}> must not report a similarity/distance/rank \
                     value as epistemic confidence",
                    descriptor.relation_iri
                ),
            });
        }
        if !descriptor
            .artifact_generation
            .contains(binding.artifact_root_hex())
        {
            return Err(RelationContractError {
                detail: format!(
                    "PURREMB retrieval relation <{}> artifact generation does not name the pinned \
                     artifact root",
                    descriptor.relation_iri
                ),
            });
        }
        Ok(Self {
            binding,
            descriptor,
            mapper: PurrembTargetMapper,
            annotate,
            profile,
        })
    }

    /// The verified binding backing this provider.
    #[must_use]
    pub fn binding(&self) -> &PurrembBinding<'a> {
        &self.binding
    }

    /// The immutable descriptor this provider serves.
    #[must_use]
    pub fn descriptor(&self) -> &RelationProviderDescriptor {
        &self.descriptor
    }

    /// The provider-identity retrieval receipt.
    #[must_use]
    pub fn retrieval_receipt(&self) -> PurrembRetrievalReceipt {
        self.binding.receipt()
    }

    /// Resolve the single bound query slot and single unbound candidate slot of a moded
    /// call, rejecting any other mode shape.
    fn resolve_mode(call: &RelationCall) -> Result<(usize, usize, &TermValue), RelationProviderError> {
        if call.bounds.len() != 2 {
            return Err(rejected("retrieval call is not the binary retrieval shape"));
        }
        let mut query_slot = None;
        let mut candidate_slot = None;
        for (index, bound) in call.bounds.iter().enumerate() {
            match bound {
                Some(_) => {
                    if query_slot.replace(index).is_some() {
                        return Err(rejected(
                            "retrieval call binds more than one slot; exactly the query is bound",
                        ));
                    }
                }
                None => {
                    if candidate_slot.replace(index).is_some() {
                        return Err(rejected(
                            "retrieval call leaves more than one slot unbound",
                        ));
                    }
                }
            }
        }
        match (query_slot, candidate_slot) {
            (Some(query), Some(candidate)) => {
                let term = call.bounds[query]
                    .as_ref()
                    .expect("the query slot is bound");
                Ok((query, candidate, term))
            }
            _ => Err(rejected(
                "retrieval call must bind the query slot and leave the candidate slot unbound",
            )),
        }
    }

    /// Find the target-set row whose reconstructed RDF identity equals the bound query
    /// term. The bound query must name an in-corpus target; if none matches, the moded
    /// retrieval cannot run and the call is rejected.
    fn resolve_query_row(
        &self,
        target_set: &TargetSetView<'_>,
        query_term: &TermValue,
    ) -> Result<usize, RelationProviderError> {
        let row_count = target_set.row_count();
        for row in 0..row_count {
            match reconstruct_target(
                &self.binding.view,
                target_set.target(row).expect("row is in range"),
                0,
            ) {
                Ok(term) if &term == query_term => return Ok(row),
                // A row that is not losslessly reconstructable cannot be the query; skip.
                Ok(_) | Err(_) => {}
            }
        }
        Err(rejected(
            "the bound query term names no in-corpus target in the selected target set",
        ))
    }
}

impl<E> ExternalRelationProvider<E> for PurrembRetrievalProvider<'_, E>
where
    E: Send + Sync,
{
    fn call(
        &self,
        call: &RelationCall,
        cancellation: &dyn RelationCancellation,
    ) -> Result<RelationBatch<E>, RelationProviderError> {
        // A generation that changed under the pinned certificate is incomplete, never a
        // silent stale read.
        if let Err(stale) = self.binding.re_pin() {
            return Err(RelationProviderError::Incomplete {
                kind: RelationProviderIncompletenessKind::StaleGeneration,
                detail: stale.detail,
            });
        }

        // A declared profile surface is part of the query's selection: fail closed.
        if let Some(profile) = &self.profile {
            self.binding
                .cross_check_profile(profile)
                .map_err(|error| rejected(&error.to_string()))?;
        }

        if cancellation.is_cancelled() {
            return Err(RelationProviderError::Failure {
                kind: RelationProviderFailureKind::Cancelled,
                detail: "cancellation observed before the retrieval scan".to_owned(),
            });
        }

        let (query_slot, candidate_slot, query_term) = Self::resolve_mode(call)?;
        let candidate_kind = &self.descriptor.argument_schema[candidate_slot];

        let target_set = self
            .binding
            .view
            .target_set(self.binding.selection.target_set)
            .ok_or_else(|| rejected("selected target set absent"))?;
        let query_row = self.resolve_query_row(&target_set, query_term)?;

        let selected = PurrembScanner::scan(
            &self.binding,
            query_row,
            call.limit,
            call.ordering.direction,
            cancellation,
        )
        .map_err(|error| match error {
            ScanError::Rejected(detail) => rejected(&detail),
            ScanError::Cancelled => RelationProviderError::Failure {
                kind: RelationProviderFailureKind::Cancelled,
                detail: "cancellation observed during the retrieval scan".to_owned(),
            },
        })?;

        let metric_code = self.binding.selection.metric.code();
        let vector_space = self.binding.selection.vector_space.into_bytes();
        let mut rows = Vec::with_capacity(selected.len());
        for (rank, candidate) in selected.into_iter().enumerate() {
            let candidate_term = self
                .mapper
                .map_row(&self.binding.view, &target_set, candidate.row, candidate_kind)
                .map_err(|error| rejected(&error.to_string()))?;
            let mut arguments = vec![TermValue::iri(String::new()); 2];
            arguments[query_slot] = query_term.clone();
            arguments[candidate_slot] = candidate_term;
            let annotation = (self.annotate)(RetrievalScore {
                distance: candidate.distance,
                metric_code,
                rank: rank as u64,
                vector_space,
            });
            rows.push(RelationTuple {
                arguments,
                annotation,
                order_key: order_key_hex(candidate.bits),
            });
        }

        // The bounded scan already selected the top prefix; sort the emitted rows by the
        // call's total order so ties resolve on the RDF arguments, exactly as the engine
        // validates.
        rows.sort_by(|left, right| call.ordering.compare_rows(left, right));

        Ok(RelationBatch {
            artifact_generation: self.descriptor.artifact_generation.clone(),
            rows,
        })
    }
}

/// A rejected well-formed request under the provider contract.
fn rejected(detail: &str) -> RelationProviderError {
    RelationProviderError::Failure {
        kind: RelationProviderFailureKind::Rejected,
        detail: detail.to_owned(),
    }
}

// --------------------------------------------------------------------------- //
// Descriptor helper: fold the explicit selection into the descriptor identity.
// --------------------------------------------------------------------------- //

/// Build the artifact-generation IRI that folds the pinned artifact root and the explicit
/// retrieval policy + source-verification mode into one identity.
///
/// Two providers differing only in an explicit selection (policy or source mode) get
/// distinct generation IRIs and therefore distinct
/// [`RelationProviderDescriptor::canonical_key`] cache keys — the selection is never
/// aliased. `base` must be an absolute IRI; the fragment carries the selection.
#[must_use]
pub fn purremb_generation_iri(
    base: &str,
    artifact_root_hex: &str,
    policy: RetrievalPolicy,
    source_mode: SourceVerificationMode,
) -> String {
    format!(
        "{base}/{artifact_root_hex}#retrieval={}&source={}",
        policy.wire(),
        source_mode_wire(source_mode),
    )
}

/// Build a fully validated retrieval descriptor whose identity folds the explicit
/// selection (via [`purremb_generation_iri`]) and whose preservation claim is non-exact —
/// a vector similarity is `logic:Vague`, not an equivalence.
///
/// # Errors
///
/// Returns [`RelationContractError`] if any identity is not an absolute IRI, the schema is
/// empty, or the preservation claim is invalid.
#[allow(clippy::too_many_arguments)]
pub fn purremb_descriptor(
    provider_iri: impl Into<String>,
    generation_iri: impl Into<String>,
    model_iri: impl Into<String>,
    relation_iri: impl Into<String>,
    argument_schema: Vec<ColumnKind>,
    annotation_dimension: RelationAnnotationDimension,
    annotation_algebra: impl Into<String>,
    ordering: RelationOrdering,
) -> Result<RelationProviderDescriptor, RelationContractError> {
    RelationProviderDescriptor::new(
        provider_iri,
        generation_iri,
        model_iri,
        relation_iri,
        argument_schema,
        annotation_dimension,
        annotation_algebra,
        PreservationClaim::for_unsupported([
            "https://blackcatinformatics.ca/logic/VagueVectorSimilarity".to_owned(),
        ]),
        ordering,
    )
}

// --------------------------------------------------------------------------- //
// Shared small helpers.
// --------------------------------------------------------------------------- //

/// Lowercase hex of a 32-byte digest.
fn hex32(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Stable wire name of a distance metric.
fn metric_wire(metric: &DistanceMetric) -> &'static str {
    match metric {
        DistanceMetric::Cosine => "cosine",
        DistanceMetric::NegativeDot => "negative-dot",
        DistanceMetric::SquaredEuclidean => "squared-euclidean",
        DistanceMetric::Extension { .. } => "extension",
    }
}

/// Stable wire name of a prefix postprocessing policy.
fn postprocessing_wire(postprocessing: PrefixPostprocessing) -> &'static str {
    match postprocessing {
        PrefixPostprocessing::None => "none",
        PrefixPostprocessing::DeterministicL2 => "deterministic-l2",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation::TupleAnnotationAlgebra;

    #[test]
    fn purremb_cosine_zero_vector_is_rejected() {
        let zero = [0.0_f32, 0.0, 0.0];
        let unit = [1.0_f32, 0.0, 0.0];
        assert_eq!(
            score(&zero, &unit, &DistanceMetric::Cosine),
            Err(ScoreError::ZeroMagnitude)
        );
    }

    #[test]
    fn purremb_cosine_orthogonal_is_unit_distance() {
        let left = [1.0_f32, 0.0];
        let right = [0.0_f32, 1.0];
        let distance = score(&left, &right, &DistanceMetric::Cosine).expect("finite");
        assert!((distance - 1.0).abs() < 1e-9);
    }

    #[test]
    fn purremb_cosine_identical_is_zero_distance() {
        let vector = [0.3_f32, -0.7, 0.1];
        let distance = score(&vector, &vector, &DistanceMetric::Cosine).expect("finite");
        assert!(distance.abs() < 1e-9);
    }

    #[test]
    fn purremb_squared_euclidean_matches_manual() {
        let left = [1.0_f32, 2.0, 3.0];
        let right = [0.0_f32, 0.0, 0.0];
        let distance = score(&left, &right, &DistanceMetric::SquaredEuclidean).expect("finite");
        assert!((distance - 14.0).abs() < 1e-9);
    }

    #[test]
    fn purremb_dimension_mismatch_is_rejected() {
        assert_eq!(
            score(&[1.0_f32], &[1.0_f32, 2.0], &DistanceMetric::NegativeDot),
            Err(ScoreError::DimensionMismatch)
        );
    }

    #[test]
    fn purremb_extension_metric_is_rejected() {
        let metric = DistanceMetric::Extension {
            identifier: "example".to_owned(),
            parameter_encoding: "raw".to_owned(),
            parameters: vec![],
        };
        assert_eq!(
            score(&[1.0_f32], &[1.0_f32], &metric),
            Err(ScoreError::UnsupportedMetric)
        );
    }

    #[test]
    fn purremb_order_key_is_monotonic_including_negatives() {
        let ordered = [
            f64::NEG_INFINITY,
            -1000.0,
            -1.5,
            -0.0,
            0.0,
            0.25,
            1.5,
            1000.0,
            f64::INFINITY,
        ];
        for window in ordered.windows(2) {
            let left = total_order_bits(window[0]);
            let right = total_order_bits(window[1]);
            assert!(
                left <= right,
                "order-key monotonicity broke at {:?} -> {:?}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn purremb_order_key_round_trips() {
        for value in [-1234.5_f64, -1.0, -0.0, 0.0, 3.14159, 1e12] {
            let restored = from_total_order_bits(total_order_bits(value));
            assert_eq!(restored.to_bits(), value.to_bits());
        }
    }

    #[test]
    fn purremb_bounded_topk_keeps_the_best_prefix() {
        // Ascending: smaller distance is better. Insert out of order; expect the three
        // smallest, best-first.
        let mut heap = BoundedTopK::new(3, RelationOrderDirection::Ascending);
        for (row, distance) in [
            (0, 5.0_f64),
            (1, 1.0),
            (2, 9.0),
            (3, 2.0),
            (4, 0.5),
            (5, 7.0),
        ] {
            heap.offer(ScannedCandidate {
                row,
                distance,
                bits: total_order_bits(distance),
                target: [row as u8; 32],
            });
        }
        let ranked = heap.into_ranked();
        let rows: Vec<usize> = ranked.iter().map(|candidate| candidate.row).collect();
        assert_eq!(rows, vec![4, 1, 3]);
    }

    #[test]
    fn purremb_bounded_topk_descending_keeps_the_largest() {
        let mut heap = BoundedTopK::new(2, RelationOrderDirection::Descending);
        for (row, distance) in [(0, 1.0_f64), (1, 8.0), (2, 3.0), (3, 9.0)] {
            heap.offer(ScannedCandidate {
                row,
                distance,
                bits: total_order_bits(distance),
                target: [row as u8; 32],
            });
        }
        let rows: Vec<usize> = heap
            .into_ranked()
            .iter()
            .map(|candidate| candidate.row)
            .collect();
        assert_eq!(rows, vec![3, 1]);
    }

    #[test]
    fn purremb_bounded_topk_breaks_ties_by_target() {
        // Two rows tie on distance; the smaller target sorts first (ascending tie-break).
        let mut heap = BoundedTopK::new(2, RelationOrderDirection::Ascending);
        heap.offer(ScannedCandidate {
            row: 0,
            distance: 1.0,
            bits: total_order_bits(1.0),
            target: [9; 32],
        });
        heap.offer(ScannedCandidate {
            row: 1,
            distance: 1.0,
            bits: total_order_bits(1.0),
            target: [2; 32],
        });
        let rows: Vec<usize> = heap
            .into_ranked()
            .iter()
            .map(|candidate| candidate.row)
            .collect();
        assert_eq!(rows, vec![1, 0]);
    }

    fn space_id(byte: u8) -> VectorSpaceId {
        VectorSpaceId::from_raw([byte; 32])
    }

    #[test]
    fn purremb_multiply_same_space_combines() {
        let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
        let left = SpaceTaggedScore::single(1.0, space_id(1), 1);
        let right = SpaceTaggedScore::single(2.0, space_id(1), 1);
        let product = algebra.multiply(&left, &right).expect("same space combines");
        assert!((product.score.distance() - 3.0).abs() < 1e-9);
        assert_eq!(product.spaces.len(), 1);
    }

    #[test]
    fn purremb_multiply_cross_space_refused_without_licensing() {
        let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
        let left = SpaceTaggedScore::single(1.0, space_id(1), 1);
        let right = SpaceTaggedScore::single(2.0, space_id(2), 1);
        assert!(algebra.multiply(&left, &right).is_err());
    }

    #[test]
    fn purremb_multiply_cross_space_licensed_combines() {
        let mut licensing = BTreeSet::new();
        licensing.insert(([1_u8; 32], [2_u8; 32]));
        let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(licensing);
        let left = SpaceTaggedScore::single(1.0, space_id(1), 1);
        let right = SpaceTaggedScore::single(2.0, space_id(2), 1);
        let product = algebra
            .multiply(&left, &right)
            .expect("licensed cross-space combines");
        assert!((product.score.distance() - 3.0).abs() < 1e-9);
        assert_eq!(product.spaces.len(), 2);
    }

    #[test]
    fn purremb_multiply_identity_is_neutral() {
        let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
        let element = SpaceTaggedScore::single(4.0, space_id(3), 1);
        let one = algebra.one();
        assert_eq!(algebra.multiply(&one, &element).expect("neutral"), element);
        assert_eq!(algebra.multiply(&element, &one).expect("neutral"), element);
    }

    #[test]
    fn purremb_add_chooses_the_better_alternative() {
        let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
        let near = SpaceTaggedScore::single(0.5, space_id(1), 1);
        let far = SpaceTaggedScore::single(5.0, space_id(2), 1);
        let sum = algebra.add(&near, &far).expect("alternatives combine");
        assert!((sum.score.distance() - 0.5).abs() < 1e-9);
        assert_eq!(sum.spaces.len(), 2);
    }

    #[test]
    fn purremb_algebra_identity_folds_declared_deviations() {
        let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
        assert!(algebra.identity().contains("multiply-associative"));
        let plain = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
        assert_ne!(algebra.identity(), plain.identity());
    }

    #[test]
    fn purremb_generation_iri_distinguishes_selection() {
        let exact = purremb_generation_iri(
            "https://example.org/gen",
            "abcd",
            RetrievalPolicy::ExactFullSpace,
            SourceVerificationMode::Certified,
        );
        let matryoshka = purremb_generation_iri(
            "https://example.org/gen",
            "abcd",
            RetrievalPolicy::MatryoshkaPrefixThenRerank,
            SourceVerificationMode::Certified,
        );
        assert_ne!(exact, matryoshka);
        assert!(exact.contains("abcd"));
    }
}
