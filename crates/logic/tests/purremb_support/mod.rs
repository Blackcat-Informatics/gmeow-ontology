// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic PURREMB fixtures for the external-relation provider matrix.
//!
//! Every artifact here is built with PurRDF's public writer/view APIs — a
//! certified source pack, a typed [`purrdf::CanonicalMetadataInput`], and an
//! [`purrdf::EmbeddingBuilder`] matrix — never by hand-authoring `.purremb`
//! bytes. After the builder emits the bytes, the fixture opens a throwaway
//! verified view to read the artifact's *actual* identities and digests, so a
//! test can perturb a true [`ProfileSurfaceDigests`] to drive a mismatch class.
//!
//! This module is `#[path]`-included into more than one integration-test binary,
//! and each binary exercises a different subset of the fixtures, so each include
//! site carries its own `#[allow(dead_code)]` for the helpers it does not use.

use gmeow_logic::purremb_relation::{ProfileSurfaceDigests, PurrembSelection, RetrievalPolicy};
use purrdf::{
    AppliedStage, ArtifactIdentity, ArtifactIdentityKind, CanonicalMetadataInput,
    CertifiedPurrpckSource, ContentDigest, DimensionalityPolicy, DistanceMetric, EffectivePrefix,
    EmbeddingBuilder, EmbeddingFamilyContract, EmbeddingTarget, EmbeddingView, FamilyId, MatrixId,
    MatrixInput, MatrixRow, PrefixPostprocessing, ProjectionId, ProjectionSpec, RdfDatasetBuilder,
    RdfTermTarget, StageImplementation, TargetSet, TargetSetId, VectorDtype, VectorSpaceId,
    verify_embedding,
};

/// One reconstructable target and its stored vector, in canonical matrix-row order.
#[derive(Debug, Clone)]
pub struct RowInfo {
    /// Reconstructable RDF IRI, when the target is an `RdfTerm(Iri)`; `None` for a
    /// deliberately non-mappable candidate (a digest-only target).
    pub iri: Option<String>,
    /// Stable target identity bytes.
    pub target: [u8; 32],
    /// Exact stored scalars as `f64` (the `f32` cast is folded in for `F32` matrices),
    /// so an in-test reference scorer reproduces the provider's arithmetic bit-for-bit.
    pub vector: Vec<f64>,
}

/// A declared effective leading-prefix (the Matryoshka coarse space).
#[derive(Debug, Clone, Copy)]
pub struct PrefixSelection {
    /// Effective vector-space identity.
    pub space: VectorSpaceId,
    /// Effective projection identity.
    pub projection: ProjectionId,
    /// Leading-prefix dimension.
    pub dimension: u32,
    /// Prefix postprocessing.
    pub postprocessing: PrefixPostprocessing,
}

/// A fully built, verified PURREMB fixture plus every identity a selection needs.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Caller-owned artifact bytes (heap buffer).
    pub artifact_bytes: Vec<u8>,
    /// Caller-owned certified source-pack bytes.
    pub source_bytes: Vec<u8>,
    /// Embedding family identity.
    pub family: FamilyId,
    /// Target row set identity.
    pub target_set: TargetSetId,
    /// Authoritative stored matrix identity.
    pub matrix: MatrixId,
    /// Declared distance metric.
    pub metric: DistanceMetric,
    /// Stored scalar type.
    pub dtype: VectorDtype,
    /// Authoritative stored dimension.
    pub stored_dimension: u32,
    /// Full effective vector-space identity (the highest stored prefix).
    pub full_space: VectorSpaceId,
    /// Full-space postprocessing.
    pub full_postprocessing: PrefixPostprocessing,
    /// Matryoshka coarse prefix, when the family declares one.
    pub prefix: Option<PrefixSelection>,
    /// The artifact's true published profile-surface digests.
    pub digests: ProfileSurfaceDigests,
    /// Rows in canonical matrix-row order.
    pub rows: Vec<RowInfo>,
}

impl Fixture {
    /// Build a validated [`PurrembSelection`] for the given retrieval policy against
    /// this fixture's real identities.
    #[must_use]
    pub fn selection(&self, policy: RetrievalPolicy) -> PurrembSelection {
        match policy {
            RetrievalPolicy::ExactFullSpace => PurrembSelection {
                target_set: self.target_set,
                family: self.family,
                vector_space: self.full_space,
                matrix: self.matrix,
                projection: None,
                metric: self.metric.clone(),
                effective_dimension: self.stored_dimension,
                postprocessing: self.full_postprocessing,
                dtype: self.dtype,
                policy,
            },
            RetrievalPolicy::MatryoshkaPrefixThenRerank => {
                let prefix = self
                    .prefix
                    .expect("a Matryoshka selection requires a declared coarse prefix");
                PurrembSelection {
                    target_set: self.target_set,
                    family: self.family,
                    vector_space: prefix.space,
                    matrix: self.matrix,
                    projection: Some(prefix.projection),
                    metric: self.metric.clone(),
                    effective_dimension: prefix.dimension,
                    postprocessing: prefix.postprocessing,
                    dtype: self.dtype,
                    policy,
                }
            }
        }
    }
}

/// A matrix-row target plus its stored vector and optional reconstructable IRI.
pub struct MatrixTargetSpec {
    target: EmbeddingTarget,
    iri: Option<String>,
    vector: Vec<f64>,
}

fn artifact_identity(name: &str) -> ArtifactIdentity {
    ArtifactIdentity::new(
        format!("https://example.org/purremb/{name}"),
        "application/octet-stream",
        ContentDigest::of(name.as_bytes()),
        None,
        ArtifactIdentityKind::Single,
    )
    .expect("artifact identity")
}

fn applied_stage(name: &str) -> AppliedStage {
    AppliedStage::Applied(
        StageImplementation::new(
            format!("https://example.org/purremb/{name}"),
            ContentDigest::of(name.as_bytes()),
            "application/octet-stream",
            vec![1],
        )
        .expect("stage implementation"),
    )
}

/// Build one embedding-family contract for the given scalar/metric/dimensionality.
fn family_contract(
    tag: &str,
    dtype: VectorDtype,
    metric: DistanceMetric,
    dimensionality: DimensionalityPolicy,
) -> EmbeddingFamilyContract {
    EmbeddingFamilyContract {
        model: artifact_identity(&format!("model-{tag}")),
        engine: artifact_identity(&format!("engine-{tag}")),
        tokenizer: artifact_identity(&format!("tokenizer-{tag}")),
        execution: applied_stage(&format!("execution-{tag}")),
        subject_projection: applied_stage(&format!("subject-{tag}")),
        preprocessing: AppliedStage::NotApplied,
        chunking: AppliedStage::NotApplied,
        pooling: applied_stage(&format!("pooling-{tag}")),
        normalization: AppliedStage::NotApplied,
        truncation: AppliedStage::NotApplied,
        dtype,
        metric,
        dimensionality,
        extensions: Vec::new(),
    }
}

/// The complete lowered build inputs.
struct BuildSpec {
    tag: String,
    dtype: VectorDtype,
    metric: DistanceMetric,
    dimensionality: DimensionalityPolicy,
    /// The certified source pack the artifact binds to.
    source: CertifiedPurrpckSource,
    /// Exact source-pack bytes handed back to the caller.
    source_bytes: Vec<u8>,
    /// The certified-dataset target (referenced by RDF component targets/relations).
    dataset_target: EmbeddingTarget,
    matrix_targets: Vec<MatrixTargetSpec>,
    aux_targets: Vec<EmbeddingTarget>,
    relations: Vec<purrdf::TargetRelation>,
    token_spans: Vec<purrdf::TokenSpan>,
}

/// A fresh certified source pack over an empty RDF dataset, plus its dataset target.
fn empty_source() -> (CertifiedPurrpckSource, Vec<u8>, EmbeddingTarget) {
    let dataset = RdfDatasetBuilder::new()
        .freeze()
        .expect("empty certified RDF source dataset");
    let (source, source_bytes) =
        CertifiedPurrpckSource::from_dataset(&dataset).expect("certified source pack");
    let dataset_target = source.dataset_target(true).expect("dataset target");
    (source, source_bytes, dataset_target)
}

/// Lower a build spec into a verified fixture.
fn build(spec: BuildSpec) -> Fixture {
    let source = spec.source;
    let source_bytes = spec.source_bytes;
    let dataset_target = spec.dataset_target;

    let contract = family_contract(
        &spec.tag,
        spec.dtype,
        spec.metric.clone(),
        spec.dimensionality.clone(),
    );
    let family = contract.derive().expect("derived family");
    let stored_dimension = family.stored_dimension;

    let target_set = TargetSet::new(
        spec.matrix_targets
            .iter()
            .map(|row| row.target.id)
            .collect(),
    )
    .expect("nonempty target set");

    let projections = spec
        .dimensionality
        .prefixes()
        .iter()
        .map(|prefix| ProjectionSpec::derive(family.id, prefix.dimension, prefix.postprocessing))
        .collect::<Vec<_>>();

    let mut targets = vec![dataset_target];
    targets.extend(spec.aux_targets);
    targets.extend(spec.matrix_targets.iter().map(|row| row.target.clone()));

    let metadata = CanonicalMetadataInput {
        source,
        family_contracts: vec![contract],
        targets,
        target_sets: vec![target_set.clone()],
        relations: spec.relations,
        token_spans: spec.token_spans,
        external_bindings: Vec::new(),
        indexes: Vec::new(),
        extensions: Vec::new(),
    };

    let mut builder = EmbeddingBuilder::from_typed_metadata(metadata);
    match spec.dtype {
        VectorDtype::F32 => {
            builder.add_f32_matrix(MatrixInput {
                family_id: family.id,
                target_set_id: target_set.id,
                stored_dimension,
                rows: spec
                    .matrix_targets
                    .iter()
                    .map(|row| {
                        MatrixRow::new(
                            row.target.id,
                            row.vector.iter().map(|&value| value as f32).collect(),
                        )
                    })
                    .collect(),
                projections: projections.clone(),
            });
        }
        VectorDtype::F64 => {
            builder.add_f64_matrix(MatrixInput {
                family_id: family.id,
                target_set_id: target_set.id,
                stored_dimension,
                rows: spec
                    .matrix_targets
                    .iter()
                    .map(|row| MatrixRow::new(row.target.id, row.vector.clone()))
                    .collect(),
                projections: projections.clone(),
            });
        }
    }
    let artifact_bytes = builder.build().expect("PURREMB artifact bytes").bytes;

    // Read the artifact's real identities and digests from a throwaway verified view.
    let mut view = EmbeddingView::from_bytes(&artifact_bytes).expect("structural view");
    verify_embedding(&mut view).expect("verified view");

    let matrix_view = view.matrices().next().expect("one authoritative matrix");
    let matrix = matrix_view.id();
    let matrix_content_digest = *matrix_view.content_digest().as_bytes();

    let full_prefix = *spec
        .dimensionality
        .prefixes()
        .last()
        .expect("at least one effective prefix");
    let full_space =
        ProjectionSpec::derive(family.id, full_prefix.dimension, full_prefix.postprocessing)
            .vector_space_id;

    let prefix = if spec.dimensionality.prefixes().len() >= 2 {
        let coarse = spec.dimensionality.prefixes()[0];
        let space = ProjectionSpec::derive(family.id, coarse.dimension, coarse.postprocessing)
            .vector_space_id;
        let projection = view
            .projections()
            .find(|projection| projection.vector_space_id() == space)
            .expect("coarse effective projection present")
            .id();
        Some(PrefixSelection {
            space,
            projection,
            dimension: coarse.dimension,
            postprocessing: coarse.postprocessing,
        })
    } else {
        None
    };

    let source_view = view.source();
    let digests = ProfileSurfaceDigests {
        artifact_root: view.artifact_root().into_bytes(),
        source_exact_digest: *source_view.source_exact_digest().as_bytes(),
        matrix_content_digest,
        target_set_id: target_set.id.into_bytes(),
        vector_space_id: full_space.into_bytes(),
        certified_rdf_digest: Some(source_view.certified_rdf_digest()),
    };

    // Reassemble rows in canonical (sorted-by-target-id) matrix order.
    let rows = target_set
        .targets
        .iter()
        .map(|target| {
            let spec_row = spec
                .matrix_targets
                .iter()
                .find(|row| row.target.id == *target)
                .expect("every matrix row target is a spec target");
            let stored = match spec.dtype {
                VectorDtype::F32 => spec_row
                    .vector
                    .iter()
                    .map(|&value| f64::from(value as f32))
                    .collect(),
                VectorDtype::F64 => spec_row.vector.clone(),
            };
            RowInfo {
                iri: spec_row.iri.clone(),
                target: target.into_bytes(),
                vector: stored,
            }
        })
        .collect();

    Fixture {
        artifact_bytes,
        source_bytes,
        family: family.id,
        target_set: target_set.id,
        matrix,
        metric: spec.metric,
        dtype: spec.dtype,
        stored_dimension,
        full_space,
        full_postprocessing: full_prefix.postprocessing,
        prefix,
        digests,
        rows,
    }
}

/// The canonical example base IRI a corpus IRI shares with the query goals.
pub const EX: &str = "https://example.org/";

fn iri_target(local: &str) -> (EmbeddingTarget, String) {
    let iri = format!("{EX}{local}");
    let target = RdfTermTarget::Iri(iri.clone())
        .into_target(true, None)
        .expect("absolute-IRI RDF term target");
    (target, iri)
}

/// A fixed-dimension corpus of `RdfTerm(Iri)` targets over an `f32` matrix.
///
/// `rows` are `(local-name, vector)`; every vector must have the same width, which
/// becomes the family's fixed stored dimension.
#[must_use]
pub fn iri_corpus_f32(tag: &str, metric: DistanceMetric, rows: &[(&str, &[f64])]) -> Fixture {
    let dimension = rows[0].1.len() as u32;
    let matrix_targets = rows
        .iter()
        .map(|(local, vector)| {
            let (target, iri) = iri_target(local);
            MatrixTargetSpec {
                target,
                iri: Some(iri),
                vector: vector.to_vec(),
            }
        })
        .collect();
    let (source, source_bytes, dataset_target) = empty_source();
    build(BuildSpec {
        tag: tag.to_owned(),
        source,
        source_bytes,
        dataset_target,
        dtype: VectorDtype::F32,
        metric,
        dimensionality: DimensionalityPolicy::fixed(dimension, PrefixPostprocessing::None)
            .expect("fixed dimensionality"),
        matrix_targets,
        aux_targets: Vec::new(),
        relations: Vec::new(),
        token_spans: Vec::new(),
    })
}

/// A fixed-dimension corpus of `RdfTerm(Iri)` targets over an `f64` matrix.
#[must_use]
pub fn iri_corpus_f64(tag: &str, metric: DistanceMetric, rows: &[(&str, &[f64])]) -> Fixture {
    let dimension = rows[0].1.len() as u32;
    let matrix_targets = rows
        .iter()
        .map(|(local, vector)| {
            let (target, iri) = iri_target(local);
            MatrixTargetSpec {
                target,
                iri: Some(iri),
                vector: vector.to_vec(),
            }
        })
        .collect();
    let (source, source_bytes, dataset_target) = empty_source();
    build(BuildSpec {
        tag: tag.to_owned(),
        source,
        source_bytes,
        dataset_target,
        dtype: VectorDtype::F64,
        metric,
        dimensionality: DimensionalityPolicy::fixed(dimension, PrefixPostprocessing::None)
            .expect("fixed dimensionality"),
        matrix_targets,
        aux_targets: Vec::new(),
        relations: Vec::new(),
        token_spans: Vec::new(),
    })
}

/// The deterministic local name of row `index` in a [`iri_corpus_f32_large`] corpus.
#[must_use]
pub fn large_corpus_local(index: usize) -> String {
    format!("row{index:06}")
}

/// A large, deterministic fixed-dimension `f32` corpus of `RdfTerm(Iri)` targets for the
/// performance & cost lane.
///
/// Builds `count` rows of width `dimension`; row `i`'s local name is
/// [`large_corpus_local`] and its stored vector is a fixed function of `(i, component)`,
/// so the artifact is byte-reproducible across runs. The authoritative stored matrix is
/// `count * dimension * 4` bytes — far larger than the `O(k + dimension)` working set one
/// bounded retrieval scan is permitted to touch, which is exactly what the cost lane
/// measures against.
#[must_use]
pub fn iri_corpus_f32_large(
    tag: &str,
    metric: DistanceMetric,
    count: usize,
    dimension: u32,
) -> Fixture {
    let matrix_targets = (0..count)
        .map(|index| {
            let (target, iri) = iri_target(&large_corpus_local(index));
            let vector = (0..dimension)
                .map(|component| {
                    // A fixed integer mix folded into [0, 1): distinct across rows and
                    // components, never zero-magnitude, no wall-clock or RNG input.
                    let mixed = (index as u64)
                        .wrapping_mul(2_654_435_761)
                        .wrapping_add(u64::from(component).wrapping_mul(40_503))
                        .wrapping_add(1);
                    ((mixed % 1009) as f64 + 1.0) / 1010.0
                })
                .collect();
            MatrixTargetSpec {
                target,
                iri: Some(iri),
                vector,
            }
        })
        .collect();
    let (source, source_bytes, dataset_target) = empty_source();
    build(BuildSpec {
        tag: tag.to_owned(),
        source,
        source_bytes,
        dataset_target,
        dtype: VectorDtype::F32,
        metric,
        dimensionality: DimensionalityPolicy::fixed(dimension, PrefixPostprocessing::None)
            .expect("fixed dimensionality"),
        matrix_targets,
        aux_targets: Vec::new(),
        relations: Vec::new(),
        token_spans: Vec::new(),
    })
}

/// A Matryoshka corpus of `RdfTerm(Iri)` targets: a coarse leading `prefix_dim` space
/// plus the full stored space, over an `f32` matrix.
#[must_use]
pub fn iri_corpus_matryoshka_f32(
    tag: &str,
    metric: DistanceMetric,
    prefix_dim: u32,
    rows: &[(&str, &[f64])],
) -> Fixture {
    let full = rows[0].1.len() as u32;
    assert!(prefix_dim < full, "coarse prefix must be shorter than full");
    let matrix_targets = rows
        .iter()
        .map(|(local, vector)| {
            let (target, iri) = iri_target(local);
            MatrixTargetSpec {
                target,
                iri: Some(iri),
                vector: vector.to_vec(),
            }
        })
        .collect();
    let dimensionality = DimensionalityPolicy::matryoshka(vec![
        EffectivePrefix {
            dimension: prefix_dim,
            postprocessing: PrefixPostprocessing::None,
        },
        EffectivePrefix {
            dimension: full,
            postprocessing: PrefixPostprocessing::None,
        },
    ])
    .expect("matryoshka dimensionality");
    let (source, source_bytes, dataset_target) = empty_source();
    build(BuildSpec {
        tag: tag.to_owned(),
        source,
        source_bytes,
        dataset_target,
        dtype: VectorDtype::F32,
        metric,
        dimensionality,
        matrix_targets,
        aux_targets: Vec::new(),
        relations: Vec::new(),
        token_spans: Vec::new(),
    })
}

/// An `f32` corpus mixing reconstructable `RdfTerm(Iri)` rows with one deliberately
/// non-mappable digest-only `RdfTerm` row (its RDF identity was never disclosed), to
/// prove the provider rejects rather than fabricating an IRI.
#[must_use]
pub fn iri_corpus_with_digest_only_f32(
    tag: &str,
    metric: DistanceMetric,
    rows: &[(&str, &[f64])],
    digest_only_vector: &[f64],
) -> Fixture {
    use purrdf::{TargetIdentityDigest, TargetKind};

    let dimension = rows[0].1.len() as u32;
    let mut matrix_targets: Vec<MatrixTargetSpec> = rows
        .iter()
        .map(|(local, vector)| {
            let (target, iri) = iri_target(local);
            MatrixTargetSpec {
                target,
                iri: Some(iri),
                vector: vector.to_vec(),
            }
        })
        .collect();

    let digest_only = EmbeddingTarget::from_digest(
        TargetKind::RdfTerm,
        TargetIdentityDigest::from_raw([0x5a; 32]),
        None,
    )
    .expect("digest-only RDF term target");
    matrix_targets.push(MatrixTargetSpec {
        target: digest_only,
        iri: None,
        vector: digest_only_vector.to_vec(),
    });

    let (source, source_bytes, dataset_target) = empty_source();
    build(BuildSpec {
        tag: tag.to_owned(),
        source,
        source_bytes,
        dataset_target,
        dtype: VectorDtype::F32,
        metric,
        dimensionality: DimensionalityPolicy::fixed(dimension, PrefixPostprocessing::None)
            .expect("fixed dimensionality"),
        matrix_targets,
        aux_targets: Vec::new(),
        relations: Vec::new(),
        token_spans: Vec::new(),
    })
}

/// A statement/document fixture: the target set holds two RDF 1.2 statement (quoted
/// triple) rows plus one external `Document` row, so `map_row` can be driven directly
/// for the triple-term reconstruction and the unsupported-kind rejection. The bound
/// triple-term query cannot be expressed in the query-goal grammar, so this fixture is
/// consumed by focused `map_row` assertions rather than end-to-end dispatch.
pub struct StatementFixture {
    /// The verified artifact bytes.
    pub artifact_bytes: Vec<u8>,
    /// Selection identities (full-space, `f32`, squared-Euclidean).
    pub fixture: Fixture,
    /// Row index of the first statement (quoted triple) in matrix-row order.
    pub statement_rows: Vec<usize>,
    /// Row index of the external `Document` candidate.
    pub document_row: usize,
    /// The reconstructed subject/predicate/object IRIs of each statement, by row.
    pub statement_terms: Vec<(String, String, String)>,
}

/// Build the statement/document fixture described by [`StatementFixture`].
#[must_use]
pub fn statement_and_document_fixture() -> StatementFixture {
    use purrdf::{
        CorpusTarget, DocumentTarget, RdfGraphTarget, RdfStatementTarget, RelationKind,
        TargetRelation,
    };

    let (source, source_bytes, dataset_target) = empty_source();

    // Default graph over the certified dataset.
    let graph = RdfGraphTarget {
        dataset_id: dataset_target.id,
        graph_name: None,
    }
    .into_target(true)
    .expect("default graph target");

    // Two statements sharing a predicate; distinct subjects and objects.
    let predicate_iri = format!("{EX}p");
    let (predicate, _) = iri_target("p");
    let mut statement_terms = Vec::new();
    let mut statement_targets = Vec::new();
    let mut component_targets = vec![predicate.clone()];
    let mut relations = Vec::new();
    for index in 0..2u8 {
        let subject_iri = format!("{EX}s{index}");
        let object_iri = format!("{EX}o{index}");
        let (subject, _) = iri_target(&format!("s{index}"));
        let (object, _) = iri_target(&format!("o{index}"));
        let statement = RdfStatementTarget {
            graph: graph.id,
            subject: subject.id,
            predicate: predicate.id,
            object: object.id,
        }
        .into_target(true, None)
        .expect("statement target");
        relations.push(TargetRelation::builtin(
            graph.id,
            RelationKind::GraphStatement,
            statement.id,
        ));
        relations.push(TargetRelation::builtin(
            statement.id,
            RelationKind::StatementSubject,
            subject.id,
        ));
        relations.push(TargetRelation::builtin(
            statement.id,
            RelationKind::StatementPredicate,
            predicate.id,
        ));
        relations.push(TargetRelation::builtin(
            statement.id,
            RelationKind::StatementObject,
            object.id,
        ));
        component_targets.push(subject);
        component_targets.push(object);
        statement_targets.push(statement);
        statement_terms.push((subject_iri, predicate_iri.clone(), object_iri));
    }
    relations.push(TargetRelation::builtin(
        dataset_target.id,
        RelationKind::DatasetGraph,
        graph.id,
    ));

    // One external corpus/document pair as the unsupported-kind candidate.
    let corpus = CorpusTarget {
        manifest_digest: ContentDigest::of(b"purremb-statement-corpus"),
        manifest_media_type: "application/vnd.example.corpus+cbor".to_owned(),
        logical_id_digest: ContentDigest::of(b"corpus:statement-fixture"),
    }
    .into_target(true)
    .expect("corpus target");
    let document_bytes = "alpha document".as_bytes();
    let document = DocumentTarget::from_content(
        corpus.id,
        ContentDigest::of(b"document:0001"),
        "text/plain;charset=utf-8",
        document_bytes,
    )
    .expect("document metadata")
    .into_target(true)
    .expect("document target");
    relations.push(TargetRelation::builtin(
        corpus.id,
        RelationKind::CorpusDocument,
        document.id,
    ));

    // Matrix rows: the two statements and the document (each a 2-vector).
    let mut matrix_targets = vec![
        MatrixTargetSpec {
            target: statement_targets[0].clone(),
            iri: None,
            vector: vec![1.0, 0.0],
        },
        MatrixTargetSpec {
            target: statement_targets[1].clone(),
            iri: None,
            vector: vec![0.0, 1.0],
        },
        MatrixTargetSpec {
            target: document.clone(),
            iri: None,
            vector: vec![0.5, 0.5],
        },
    ];
    matrix_targets.sort_by_key(|row| row.target.id);

    let mut aux_targets = vec![graph, corpus];
    aux_targets.extend(component_targets);

    let statement_ids: Vec<[u8; 32]> = statement_targets
        .iter()
        .map(|target| target.id.into_bytes())
        .collect();
    let document_id = document.id.into_bytes();

    // A text-subject matrix row (the document) requires a family-scoped token span; the
    // family identity is deterministic in the contract, so it can be derived up front.
    let dimensionality =
        DimensionalityPolicy::fixed(2, PrefixPostprocessing::None).expect("fixed dimensionality");
    let family_id = family_contract(
        "statement",
        VectorDtype::F32,
        DistanceMetric::SquaredEuclidean,
        dimensionality.clone(),
    )
    .derive()
    .expect("statement family")
    .id;
    let token_spans = vec![
        purrdf::TokenSpan {
            family_id,
            target_id: document.id,
            token_start: 0,
            token_end: 2,
            model_input_token_count: 2,
            left_truncated: false,
            right_truncated: false,
            includes_special_tokens: false,
        }
        .validate()
        .expect("valid document token span"),
    ];

    let fixture = build(BuildSpec {
        tag: "statement".to_owned(),
        source,
        source_bytes,
        dataset_target,
        dtype: VectorDtype::F32,
        metric: DistanceMetric::SquaredEuclidean,
        dimensionality,
        matrix_targets,
        aux_targets,
        relations,
        token_spans,
    });

    let statement_rows = fixture
        .rows
        .iter()
        .enumerate()
        .filter_map(|(row, info)| statement_ids.contains(&info.target).then_some(row))
        .collect();
    let document_row = fixture
        .rows
        .iter()
        .position(|info| info.target == document_id)
        .expect("document row present");

    // Reorder statement_terms to match matrix-row order of the statements.
    let statement_terms_by_row = {
        let mut ordered = Vec::new();
        for (row, info) in fixture.rows.iter().enumerate() {
            if let Some(index) = statement_ids.iter().position(|id| *id == info.target) {
                ordered.push((row, statement_terms[index].clone()));
            }
        }
        ordered.sort_by_key(|(row, _)| *row);
        ordered.into_iter().map(|(_, terms)| terms).collect()
    };

    StatementFixture {
        artifact_bytes: fixture.artifact_bytes.clone(),
        fixture,
        statement_rows,
        document_row,
        statement_terms: statement_terms_by_row,
    }
}
