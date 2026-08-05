// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production acceptance matrix for the verified PURREMB external-relation provider.
//!
//! Every case builds a real PURREMB artifact through PurRDF's public writer/view APIs
//! (see the `support` module), opens a fully verified [`PurrembBinding`], registers a
//! [`PurrembRetrievalProvider`] as a query-scoped external relation, and drives a real
//! `dispatch_query_annotated_with_relations*` entry point — asserting on the returned
//! answer rows, the annotation dimension carried in lineage, the operational receipt, or
//! the distinct typed failure surfaced at the dispatch boundary.

#[allow(
    dead_code,
    reason = "the shared PURREMB fixture support module also exposes builders used only by other test binaries"
)]
#[path = "purremb_support/mod.rs"]
mod support;

use std::collections::BTreeSet;

use gmeow_logic::annotation::{
    AnnotationContract, AnnotationQueryClass, AnnotationRequest, SemiringLaw,
    TupleAnnotationAlgebra,
};
use gmeow_logic::dispatch::{
    RelationAnnotationRequest, RelationQueryError, dispatch_query_annotated_with_relations,
};
use gmeow_logic::external_relation::{
    NeverCancelled, QueryRelationProviders, RelationAnnotationDimension, RelationCancellation,
    RelationExecutionFailureKind, RelationInvocationStatus, RelationOrderDirection,
    RelationOrdering, RelationProviderBudget, RelationProviderFailureKind,
    RelationProviderRegistration,
};
use gmeow_logic::purremb_relation::{
    ProfileSurfaceDigests, PurrembBinding, PurrembBindingError, PurrembRetrievalProvider,
    PurrembTargetMapper, RetrievalPolicy, RetrievalScore, SpaceTaggedScore,
    VectorSpaceScopedAlgebra, purremb_descriptor, purremb_generation_iri, total_order_bits,
};
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::seam::{BudgetStatus, WorldFactSnapshot};
use gmeow_logic::store::WorldStore;
use gmeow_logic_compile::result_shape::ColumnKind;
use purrdf::{DistanceMetric, SourceVerificationMode, TermValue, VectorSpaceId};

use support::Fixture;

const WORLD: &str = "https://example.org/world/purremb";
const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const RELATION: &str = "https://example.org/relation/vector";
const GEN_BASE: &str = "https://example.org/index/purremb";
const ORDER_CRITERION: &str = "https://blackcatinformatics.ca/logic/VectorDistanceOrder";

static NEVER_CANCELLED: NeverCancelled = NeverCancelled;

/// A cancellation source that flips to cancelled after a fixed number of polls.
struct CancelAfter {
    remaining: std::sync::atomic::AtomicUsize,
}

impl CancelAfter {
    fn new(polls: usize) -> Self {
        Self {
            remaining: std::sync::atomic::AtomicUsize::new(polls),
        }
    }
}

impl RelationCancellation for CancelAfter {
    fn is_cancelled(&self) -> bool {
        use std::sync::atomic::Ordering::SeqCst;
        let current = self.remaining.load(SeqCst);
        if current == 0 {
            return true;
        }
        self.remaining.store(current - 1, SeqCst);
        false
    }
}

fn ex(local: &str) -> String {
    format!("https://example.org/{local}")
}

/// A minimal single-anchor world snapshot; PURREMB retrieval never needs RDF facts,
/// but the annotated evaluator still runs over a real world.
fn anchor_store() -> WorldStore {
    let store = WorldStore::new();
    store.insert_quad(WORLD, &ex("anchor"), &ex("present"), &ex("yes"));
    store
}

fn snapshot(store: &WorldStore) -> WorldFactSnapshot {
    WorldFactSnapshot::from_world(store, WORLD, PROFILE).expect("world snapshot")
}

/// The provider's annotation closure: fold a computed retrieval score into the
/// space-tagged algebra element for its effective vector space.
fn annotate(score: RetrievalScore) -> SpaceTaggedScore {
    SpaceTaggedScore::single(
        score.distance,
        VectorSpaceId::from_raw(score.vector_space),
        score.metric_code,
    )
}

/// No RDF-asserted fact is scored: the multiplicative identity everywhere.
fn no_fact_score(_: gmeow_logic::annotation::AnnotationFactRef<'_>) -> Option<SpaceTaggedScore> {
    None
}

fn ascending_order() -> RelationOrdering {
    RelationOrdering::new(ORDER_CRITERION, RelationOrderDirection::Ascending).expect("ordering")
}

/// Build the descriptor for a fixture's binding under a policy, folding the pinned
/// artifact root and explicit selection into the generation IRI.
fn descriptor_for(
    fixture: &Fixture,
    relation: &str,
    artifact_root_hex: &str,
    policy: RetrievalPolicy,
    source_mode: SourceVerificationMode,
    algebra_identity: &str,
    dimension: RelationAnnotationDimension,
) -> gmeow_logic::external_relation::RelationProviderDescriptor {
    descriptor_for_kind(
        fixture,
        relation,
        artifact_root_hex,
        policy,
        source_mode,
        algebra_identity,
        dimension,
        vec![ColumnKind::Iri, ColumnKind::Iri],
    )
}

/// A descriptor whose binary (query, candidate) argument schema uses the given column
/// kinds — e.g. `[ColumnKind::TripleTerm, ColumnKind::TripleTerm]` for a triple-term corpus.
#[allow(clippy::too_many_arguments)]
fn descriptor_for_kind(
    fixture: &Fixture,
    relation: &str,
    artifact_root_hex: &str,
    policy: RetrievalPolicy,
    source_mode: SourceVerificationMode,
    algebra_identity: &str,
    dimension: RelationAnnotationDimension,
    columns: Vec<ColumnKind>,
) -> gmeow_logic::external_relation::RelationProviderDescriptor {
    let _ = fixture;
    let generation = purremb_generation_iri(GEN_BASE, artifact_root_hex, policy, source_mode);
    purremb_descriptor(
        "https://example.org/provider/purremb",
        generation,
        "https://example.org/model/purremb-embedding-v1",
        relation,
        columns,
        dimension,
        algebra_identity.to_owned(),
        ascending_order(),
    )
    .expect("valid PURREMB descriptor")
}

/// Open a verified binding for a fixture under a policy/source mode.
fn open_binding<'a>(
    fixture: &'a Fixture,
    policy: RetrievalPolicy,
    source_mode: SourceVerificationMode,
) -> PurrembBinding<'a> {
    PurrembBinding::open(
        &fixture.artifact_bytes,
        &fixture.source_bytes,
        fixture.selection(policy),
        source_mode,
    )
    .expect("verified PURREMB binding")
}

/// Run one direct retrieval query `?- relation(query, D).` against a single provider,
/// returning the typed result.
#[allow(clippy::too_many_arguments)]
fn run_direct<'p>(
    snapshot: &WorldFactSnapshot,
    providers: &QueryRelationProviders<'p, SpaceTaggedScore>,
    algebra: &VectorSpaceScopedAlgebra,
    contract: &AnnotationContract,
    query_local: &str,
) -> Result<gmeow_logic::external_relation::RelationQueryResult<SpaceTaggedScore>, RelationQueryError>
{
    let program = parse_query_program(&format!(
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:relation/vector(ex:{query_local}, D).\n"
    ))
    .expect("direct retrieval goal");
    dispatch_query_annotated_with_relations(
        snapshot,
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        RelationAnnotationRequest::new(
            AnnotationRequest::new(algebra, contract, no_fact_score),
            providers,
        ),
    )
}

/// A small 2-dimensional squared-Euclidean corpus with distinct pairwise distances.
fn distinct_corpus() -> Fixture {
    support::iri_corpus_f32(
        "distinct",
        DistanceMetric::SquaredEuclidean,
        &[
            ("a", &[0.0, 0.0]),
            ("b", &[1.0, 0.0]),
            ("c", &[3.0, 0.0]),
            ("d", &[6.0, 0.0]),
        ],
    )
}

/// The provider, its binding, and descriptor bundled for one query.
struct Wired<'a> {
    provider: PurrembRetrievalProvider<'a, SpaceTaggedScore>,
}

fn wire<'a>(
    fixture: &'a Fixture,
    relation: &str,
    policy: RetrievalPolicy,
    source_mode: SourceVerificationMode,
    algebra: &VectorSpaceScopedAlgebra,
    dimension: RelationAnnotationDimension,
    profile: Option<ProfileSurfaceDigests>,
) -> Wired<'a> {
    let binding = open_binding(fixture, policy, source_mode);
    let descriptor = descriptor_for(
        fixture,
        relation,
        binding.artifact_root_hex(),
        policy,
        source_mode,
        algebra.identity(),
        dimension,
    );
    let provider =
        PurrembRetrievalProvider::with_profile(binding, descriptor, Box::new(annotate), profile)
            .expect("valid provider contract");
    Wired { provider }
}

fn providers_for<'a>(
    provider: &'a PurrembRetrievalProvider<'a, SpaceTaggedScore>,
    per_call_limit: usize,
    budget: RelationProviderBudget,
) -> QueryRelationProviders<'a, SpaceTaggedScore> {
    QueryRelationProviders::new(
        vec![
            RelationProviderRegistration::new(
                provider.descriptor().clone(),
                per_call_limit,
                provider,
            )
            .expect("registration"),
        ],
        budget,
        &NEVER_CANCELLED,
    )
    .expect("sealed provider set")
}

// --------------------------------------------------------------------------- //
// Baseline: a real dispatch returns the metric-ordered nearest RDF identities.
// --------------------------------------------------------------------------- //

#[test]
fn exact_full_space_returns_metric_ordered_iri_rows() {
    let fixture = distinct_corpus();
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let providers = providers_for(
        &wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let store = anchor_store();
    let result = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a")
        .expect("complete retrieval");

    assert_eq!(result.answer.status, BudgetStatus::Ok);
    // Query row `a=[0,0]` is its own nearest; then b,c,d by ascending distance.
    let ordered: Vec<String> = result
        .answer
        .answers
        .iter()
        .map(|answer| answer.binding["D"].clone())
        .collect();
    assert_eq!(
        ordered,
        vec![
            format!("<{}>", ex("a")),
            format!("<{}>", ex("b")),
            format!("<{}>", ex("c")),
            format!("<{}>", ex("d")),
        ]
    );
    // Distance annotation is carried, never epistemic confidence.
    let dimensions: BTreeSet<&str> = result
        .answer
        .answers
        .iter()
        .flat_map(|answer| &answer.derivations)
        .flat_map(|derivation| &derivation.provider_sources)
        .map(|source| source.annotation_dimension_iri.as_str())
        .collect();
    assert!(dimensions.contains(RelationAnnotationDimension::Distance.iri()));
    assert!(!dimensions.contains(RelationAnnotationDimension::EpistemicConfidence.iri()));
}

// --------------------------------------------------------------------------- //
// RDF 1.2 triple-term targets: a quoted-triple query drives retrieval end-to-end
// through the query-goal grammar, and the candidates round-trip as triple terms.
// --------------------------------------------------------------------------- //

#[test]
fn triple_term_query_returns_metric_ordered_statement_candidates() {
    let (fixture, terms) = support::statement_corpus_f32(
        "triple-e2e",
        DistanceMetric::SquaredEuclidean,
        &[
            ("s0", "o0", &[0.0, 0.0]),
            ("s1", "o1", &[1.0, 0.0]),
            ("s2", "o2", &[3.0, 0.0]),
        ],
    );
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let binding = open_binding(
        &fixture,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
    );
    let descriptor = descriptor_for_kind(
        &fixture,
        RELATION,
        binding.artifact_root_hex(),
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        algebra.identity(),
        RelationAnnotationDimension::Distance,
        vec![ColumnKind::TripleTerm, ColumnKind::TripleTerm],
    );
    let provider =
        PurrembRetrievalProvider::with_profile(binding, descriptor, Box::new(annotate), None)
            .expect("valid provider contract");
    let providers = providers_for(&provider, 8, RelationProviderBudget::new(8, 64).unwrap());
    let store = anchor_store();

    // The bound query term is a quoted triple naming an in-corpus statement row.
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:relation/vector(<<( ex:s0 ex:p ex:o0 )>>, D).\n",
    )
    .expect("triple-term retrieval goal");
    let result = dispatch_query_annotated_with_relations(
        &snapshot(&store),
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        RelationAnnotationRequest::new(
            AnnotationRequest::new(&algebra, &contract, no_fact_score),
            &providers,
        ),
    )
    .expect("complete triple-term retrieval");

    assert_eq!(result.answer.status, BudgetStatus::Ok);
    // Each candidate D binds to a statement's quoted-triple surface, metric-ordered
    // (s0 at distance 0 is its own nearest, then s1, then s2).
    let ordered: Vec<String> = result
        .answer
        .answers
        .iter()
        .map(|answer| answer.binding["D"].clone())
        .collect();
    let expect = |index: usize| {
        let (subject, predicate, object) = &terms[index];
        format!("<<( <{subject}> <{predicate}> <{object}> )>>")
    };
    assert_eq!(ordered, vec![expect(0), expect(1), expect(2)]);

    // The annotation stays a distance dimension, never epistemic confidence.
    let dimensions: BTreeSet<&str> = result
        .answer
        .answers
        .iter()
        .flat_map(|answer| &answer.derivations)
        .flat_map(|derivation| &derivation.provider_sources)
        .map(|source| source.annotation_dimension_iri.as_str())
        .collect();
    assert!(dimensions.contains(RelationAnnotationDimension::Distance.iri()));
    assert!(!dimensions.contains(RelationAnnotationDimension::EpistemicConfidence.iri()));
}

// --------------------------------------------------------------------------- //
// Helpers shared by the remaining matrix.
// --------------------------------------------------------------------------- //

/// The ordered candidate IRI bindings returned by a completed direct retrieval.
fn ordered_iris(
    result: &gmeow_logic::external_relation::RelationQueryResult<SpaceTaggedScore>,
) -> Vec<String> {
    result
        .answer
        .answers
        .iter()
        .map(|answer| answer.binding["D"].clone())
        .collect()
}

/// Assert a direct retrieval failed as a typed provider `Rejected` at the dispatch boundary.
fn expect_rejected(error: RelationQueryError) {
    match error {
        RelationQueryError::Provider { error, .. } => {
            assert_eq!(
                error.kind,
                RelationExecutionFailureKind::ProviderFailure(
                    RelationProviderFailureKind::Rejected
                ),
                "expected a typed provider rejection"
            );
            assert_eq!(error.invocation.status, RelationInvocationStatus::Failed);
        }
        other => panic!("expected ProviderFailure(Rejected), got {other:?}"),
    }
}

// --------------------------------------------------------------------------- //
// Heap vs memory-mapped artifact bytes: identical logical answers.
// --------------------------------------------------------------------------- //

#[test]
fn heap_and_mmap_backed_bindings_yield_identical_answers() {
    let fixture = distinct_corpus();
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();

    // Heap-backed run.
    let heap_wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let heap_providers = providers_for(
        &heap_wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let heap = run_direct(&snapshot(&store), &heap_providers, &algebra, &contract, "a")
        .expect("heap retrieval");

    // Memory-mapped run over identical bytes written to a private temp file.
    let temp = tempfile::NamedTempFile::new().expect("temp artifact file");
    std::fs::write(temp.path(), &fixture.artifact_bytes).expect("write artifact");
    let file = std::fs::File::open(temp.path()).expect("reopen artifact");
    // SAFETY: the file is a private, freshly written temp file this test owns
    // exclusively and never mutates for the mapping's lifetime.
    let mmap = unsafe { memmap2::Mmap::map(&file).expect("map artifact") };
    let mmap_binding = PurrembBinding::open(
        &mmap[..],
        &fixture.source_bytes,
        fixture.selection(RetrievalPolicy::ExactFullSpace),
        SourceVerificationMode::Exact,
    )
    .expect("mmap binding");
    let descriptor = descriptor_for(
        &fixture,
        RELATION,
        mmap_binding.artifact_root_hex(),
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        algebra.identity(),
        RelationAnnotationDimension::Distance,
    );
    let mmap_provider =
        PurrembRetrievalProvider::with_profile(mmap_binding, descriptor, Box::new(annotate), None)
            .expect("mmap provider");
    let mmap_providers = providers_for(
        &mmap_provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let mapped = run_direct(&snapshot(&store), &mmap_providers, &algebra, &contract, "a")
        .expect("mmap retrieval");

    assert_eq!(
        heap.answer.answers, mapped.answer.answers,
        "heap and mmap bindings must yield byte-identical answer rows"
    );
    assert_eq!(ordered_iris(&heap), ordered_iris(&mapped));
}

// --------------------------------------------------------------------------- //
// f32 and f64 matrices agree on the metric-ordered answer.
// --------------------------------------------------------------------------- //

#[test]
fn f32_and_f64_matrices_produce_the_same_ordering() {
    let rows: &[(&str, &[f64])] = &[
        ("a", &[0.0, 0.0]),
        ("b", &[1.0, 0.0]),
        ("c", &[3.0, 0.0]),
        ("d", &[6.0, 0.0]),
    ];
    let f32_fixture = support::iri_corpus_f32("dual32", DistanceMetric::SquaredEuclidean, rows);
    let f64_fixture = support::iri_corpus_f64("dual64", DistanceMetric::SquaredEuclidean, rows);
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();

    let run = |fixture: &Fixture| {
        let wired = wire(
            fixture,
            RELATION,
            RetrievalPolicy::ExactFullSpace,
            SourceVerificationMode::Exact,
            &algebra,
            RelationAnnotationDimension::Distance,
            None,
        );
        let providers = providers_for(
            &wired.provider,
            8,
            RelationProviderBudget::new(8, 64).unwrap(),
        );
        ordered_iris(&run_direct(&snapshot(&store), &providers, &algebra, &contract, "a").unwrap())
    };

    assert_eq!(run(&f32_fixture), run(&f64_fixture));
}

// --------------------------------------------------------------------------- //
// Matryoshka prefix + full-space rerank matches the exact full-space scan.
// --------------------------------------------------------------------------- //

#[test]
fn matryoshka_prefix_then_rerank_matches_exact_full_space() {
    let rows: &[(&str, &[f64])] = &[
        ("a", &[0.0, 0.0, 0.0, 0.0]),
        ("b", &[1.0, 0.0, 0.5, 0.0]),
        ("c", &[3.0, 0.0, 0.0, 1.0]),
        ("d", &[6.0, 0.0, 1.0, 0.0]),
    ];
    let matryoshka =
        support::iri_corpus_matryoshka_f32("matry", DistanceMetric::SquaredEuclidean, 2, rows);
    let exact = support::iri_corpus_f32("matry-exact", DistanceMetric::SquaredEuclidean, rows);
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();

    let matry_wired = wire(
        &matryoshka,
        RELATION,
        RetrievalPolicy::MatryoshkaPrefixThenRerank,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let matry_providers = providers_for(
        &matry_wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let matry = run_direct(
        &snapshot(&store),
        &matry_providers,
        &algebra,
        &contract,
        "a",
    )
    .unwrap();

    let exact_wired = wire(
        &exact,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let exact_providers = providers_for(
        &exact_wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let exact_res = run_direct(
        &snapshot(&store),
        &exact_providers,
        &algebra,
        &contract,
        "a",
    )
    .unwrap();

    assert_eq!(ordered_iris(&matry), ordered_iris(&exact_res));
    // The Matryoshka receipt discloses an approximate recall; exact discloses recall 1.0.
    assert_eq!(matry_wired.provider.retrieval_receipt().recall, None);
    assert_eq!(exact_wired.provider.retrieval_receipt().recall, Some(1.0));
    assert!(
        matry_wired
            .provider
            .retrieval_receipt()
            .projection
            .is_some()
    );
}

// --------------------------------------------------------------------------- //
// Profile-surface agreement passes; each mismatch class is a typed rejection.
// --------------------------------------------------------------------------- //

fn run_with_profile(fixture: &Fixture, profile: ProfileSurfaceDigests) -> Result<(), ()> {
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        Some(profile),
    );
    let providers = providers_for(
        &wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    match run_direct(&snapshot(&store), &providers, &algebra, &contract, "a") {
        Ok(_) => Ok(()),
        Err(error) => {
            expect_rejected(error);
            Err(())
        }
    }
}

#[test]
fn profile_surface_agreement_passes_and_every_mismatch_class_is_rejected() {
    let fixture = distinct_corpus();
    // Exact agreement passes.
    assert!(run_with_profile(&fixture, fixture.digests).is_ok());

    // Each perturbed surface fails closed as a typed rejection.
    let mut wrong_root = fixture.digests;
    wrong_root.artifact_root[0] ^= 0xff;
    assert!(run_with_profile(&fixture, wrong_root).is_err());

    let mut wrong_source = fixture.digests;
    wrong_source.source_exact_digest[0] ^= 0xff;
    assert!(run_with_profile(&fixture, wrong_source).is_err());

    let mut wrong_matrix = fixture.digests;
    wrong_matrix.matrix_content_digest[0] ^= 0xff;
    assert!(run_with_profile(&fixture, wrong_matrix).is_err());

    let mut wrong_target_set = fixture.digests;
    wrong_target_set.target_set_id[0] ^= 0xff;
    assert!(run_with_profile(&fixture, wrong_target_set).is_err());

    let mut wrong_space = fixture.digests;
    wrong_space.vector_space_id[0] ^= 0xff;
    assert!(run_with_profile(&fixture, wrong_space).is_err());

    let mut wrong_rdf = fixture.digests;
    wrong_rdf.certified_rdf_digest = Some([0x11; 32]);
    assert!(run_with_profile(&fixture, wrong_rdf).is_err());
}

// --------------------------------------------------------------------------- //
// Cross-dtype and over-wide selections are rejected at binding open.
// --------------------------------------------------------------------------- //

#[test]
fn cross_dtype_selection_is_rejected_at_binding_open() {
    let fixture = distinct_corpus(); // an f32 matrix.
    let mut selection = fixture.selection(RetrievalPolicy::ExactFullSpace);
    selection.dtype = purrdf::VectorDtype::F64; // declare F64 for the F32 matrix.
    let Err(error) = PurrembBinding::open(
        &fixture.artifact_bytes,
        &fixture.source_bytes,
        selection,
        SourceVerificationMode::Exact,
    ) else {
        panic!("a cross-dtype selection must be rejected, never reinterpreted");
    };
    assert!(matches!(error, PurrembBindingError::Selection(_)));
}

#[test]
fn effective_dimension_exceeding_stored_dimension_is_rejected_at_binding_open() {
    let fixture = distinct_corpus();
    let mut selection = fixture.selection(RetrievalPolicy::ExactFullSpace);
    selection.effective_dimension = fixture.stored_dimension + 5;
    let Err(error) = PurrembBinding::open(
        &fixture.artifact_bytes,
        &fixture.source_bytes,
        selection,
        SourceVerificationMode::Exact,
    ) else {
        panic!("an over-wide effective dimension must be rejected");
    };
    assert!(matches!(error, PurrembBindingError::Selection(_)));
}

// --------------------------------------------------------------------------- //
// Deterministic ordering + negative-score (NegativeDot) total-order key.
// --------------------------------------------------------------------------- //

#[test]
fn negative_dot_scores_order_deterministically_across_repeats() {
    // NegativeDot yields negative distances for aligned vectors, exercising the
    // IEEE-754 total-order key across the sign boundary.
    let fixture = support::iri_corpus_f32(
        "negdot",
        DistanceMetric::NegativeDot,
        &[
            ("a", &[1.0, 1.0]),
            ("b", &[0.9, 0.8]),
            ("c", &[-1.0, -1.0]),
            ("d", &[0.2, -0.5]),
        ],
    );
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();

    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    // A top-2 prefix exercises the score-based selection (not just the presentation
    // order): NegativeDot ranks the two most-aligned vectors, `a` and `b`, ahead of the
    // anti-aligned `c`, and the total-order key must place the negative distances first.
    let providers = providers_for(
        &wired.provider,
        2,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let first = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a").unwrap();
    let second = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a").unwrap();
    assert_eq!(
        first.answer.answers, second.answer.answers,
        "repeats are byte-identical"
    );

    let selected: BTreeSet<String> = first
        .answer
        .answers
        .iter()
        .map(|answer| answer.binding["D"].clone())
        .collect();
    assert_eq!(
        selected,
        BTreeSet::from([format!("<{}>", ex("a")), format!("<{}>", ex("b"))]),
        "the two most-aligned candidates are selected under the negative-dot total order"
    );
}

// --------------------------------------------------------------------------- //
// k > corpus yields a COMPLETE answer of every row, not an incompleteness error.
// --------------------------------------------------------------------------- //

#[test]
fn limit_greater_than_corpus_returns_a_complete_full_answer() {
    // A PURREMB target set is nonempty by construction (PurRDF rejects an empty set),
    // so genuine zero-row absence is unreachable; the boundary case a caller can reach
    // is k > N, which must return all N rows as a COMPLETE answer.
    let fixture = distinct_corpus(); // four rows.
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let providers = providers_for(
        &wired.provider,
        100,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let result = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a").unwrap();
    assert_eq!(result.answer.status, BudgetStatus::Ok);
    assert_eq!(
        result.answer.answers.len(),
        4,
        "every row is a complete candidate"
    );
}

// --------------------------------------------------------------------------- //
// Zero-magnitude vector under cosine is a typed rejection.
// --------------------------------------------------------------------------- //

#[test]
fn zero_vector_under_cosine_is_rejected() {
    let fixture = support::iri_corpus_f32(
        "cosine-zero",
        DistanceMetric::Cosine,
        &[("a", &[1.0, 0.0]), ("z", &[0.0, 0.0]), ("b", &[0.0, 1.0])],
    );
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let providers = providers_for(
        &wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let error = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a")
        .expect_err("scoring the zero candidate under cosine is undefined");
    expect_rejected(error);
}

// --------------------------------------------------------------------------- //
// A bound query naming no in-corpus target is rejected (pushdown correctness).
// --------------------------------------------------------------------------- //

#[test]
fn bound_query_naming_no_in_corpus_target_is_rejected() {
    let fixture = distinct_corpus();
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let providers = providers_for(
        &wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    // `missing` is not one of a,b,c,d.
    let error = run_direct(
        &snapshot(&store),
        &providers,
        &algebra,
        &contract,
        "missing",
    )
    .expect_err("a bound query outside the corpus cannot resolve a query row");
    expect_rejected(error);
}

// --------------------------------------------------------------------------- //
// A non-mappable digest-only candidate is rejected, never given a fabricated IRI.
// --------------------------------------------------------------------------- //

#[test]
fn digest_only_candidate_is_rejected_without_fabricating_an_iri() {
    let fixture = support::iri_corpus_with_digest_only_f32(
        "digest-only",
        DistanceMetric::SquaredEuclidean,
        &[("a", &[0.0, 0.0]), ("b", &[1.0, 0.0]), ("c", &[2.0, 0.0])],
        &[0.5, 0.0],
    );
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    // A limit spanning the whole corpus forces the digest-only row into the candidate set.
    let providers = providers_for(
        &wired.provider,
        16,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let error = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a")
        .expect_err("a digest-only target discloses no reconstructable RDF identity");
    expect_rejected(error);
}

// --------------------------------------------------------------------------- //
// Cancellation observed at a provider-call boundary maps to the Cancelled path.
// --------------------------------------------------------------------------- //

#[test]
fn cancellation_at_a_call_boundary_surfaces_the_cancelled_path() {
    let fixture = distinct_corpus();
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    // A source cancelled before the first scan poll.
    let cancel = CancelAfter::new(0);
    let providers = QueryRelationProviders::new(
        vec![
            RelationProviderRegistration::new(
                wired.provider.descriptor().clone(),
                8,
                &wired.provider,
            )
            .unwrap(),
        ],
        RelationProviderBudget::new(8, 64).unwrap(),
        &cancel,
    )
    .unwrap();
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:relation/vector(ex:a, D).\n",
    )
    .unwrap();
    let error = dispatch_query_annotated_with_relations(
        &snapshot(&store),
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        RelationAnnotationRequest::new(
            AnnotationRequest::new(&algebra, &contract, no_fact_score),
            &providers,
        ),
    )
    .expect_err("a cancelled operation cannot complete");
    match error {
        RelationQueryError::Provider { error, .. } => {
            assert!(
                matches!(
                    error.kind,
                    RelationExecutionFailureKind::Cancelled
                        | RelationExecutionFailureKind::ProviderFailure(
                            RelationProviderFailureKind::Cancelled
                        )
                ),
                "expected a cancellation terminal, got {:?}",
                error.kind
            );
        }
        other => panic!("expected a provider cancellation terminal, got {other:?}"),
    }
}

// --------------------------------------------------------------------------- //
// The operation-wide row governor is exhausted deterministically.
// --------------------------------------------------------------------------- //

#[test]
fn row_budget_exhaustion_surfaces_the_budget_path() {
    let fixture = distinct_corpus(); // four candidate rows.
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    // Deliver four rows against a two-row governor.
    let providers = providers_for(
        &wired.provider,
        8,
        RelationProviderBudget::new(8, 2).unwrap(),
    );
    let error = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a")
        .expect_err("four delivered rows exceed the two-row governor");
    match error {
        RelationQueryError::Provider { error, .. } => {
            assert_eq!(error.kind, RelationExecutionFailureKind::BudgetExhausted);
        }
        other => panic!("expected the budget terminal, got {other:?}"),
    }
}

// --------------------------------------------------------------------------- //
// StaleGeneration: a stable immutable buffer always re-pins; forcing a certificate
// mismatch would require mutating the pinned bytes mid-life, which the module's
// borrow-stability contract forbids (it is UB, not a typed return). The honest,
// public-API assertion is that a stable binding re-pins successfully, keeping the
// provider on the live-generation path.
// --------------------------------------------------------------------------- //

#[test]
fn stable_binding_repins_against_its_retained_certificate() {
    let fixture = distinct_corpus();
    let binding = open_binding(
        &fixture,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
    );
    assert!(
        binding.re_pin().is_ok(),
        "a stable, immutable artifact buffer must re-pin against its retained certificate"
    );
}

// --------------------------------------------------------------------------- //
// A provider seed participates in positive recursion with transitive lineage.
// --------------------------------------------------------------------------- //

#[test]
fn provider_seed_feeds_recursion_with_transitive_provider_lineage() {
    let fixture = distinct_corpus();
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    store.insert_quad(WORLD, &ex("d"), &ex("link"), &ex("e"));
    store.insert_quad(WORLD, &ex("e"), &ex("link"), &ex("f"));

    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let providers = providers_for(
        &wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ex:reach(Q, D) :- ex:relation/vector(Q, D).\n\
         ex:reach(Q, D) :- ex:reach(Q, M), ex:link(M, D).\n\
         ?- ex:reach(ex:a, D).\n",
    )
    .expect("recursive provider-seeded program");
    let result = dispatch_query_annotated_with_relations(
        &snapshot(&store),
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        RelationAnnotationRequest::new(
            AnnotationRequest::new(&algebra, &contract, no_fact_score),
            &providers,
        ),
    )
    .expect("recursive retrieval query");

    let reached: BTreeSet<String> = result
        .answer
        .answers
        .iter()
        .map(|answer| answer.binding["D"].clone())
        .collect();
    // The provider seeds a..d; the RDF link edges transitively reach e and f.
    assert!(reached.contains(&format!("<{}>", ex("e"))));
    assert!(reached.contains(&format!("<{}>", ex("f"))));

    let deepest = result
        .answer
        .answers
        .iter()
        .find(|answer| answer.binding["D"] == format!("<{}>", ex("f")))
        .expect("f is reachable");
    assert!(
        deepest.derivations.iter().any(|derivation| {
            derivation
                .provider_sources
                .iter()
                .any(|source| source.provider_iri == "https://example.org/provider/purremb")
        }),
        "the deepest recursive answer must cite the PURREMB provider in its lineage"
    );
    assert!(
        result
            .receipt
            .contributing_providers
            .iter()
            .any(|(provider, _)| provider == "https://example.org/provider/purremb")
    );
}

// --------------------------------------------------------------------------- //
// In-fixpoint cross-space refusal: a conjunction of two incompatible vector spaces
// hard-fails without a licensing correspondence, and succeeds with one.
// --------------------------------------------------------------------------- //

const RELATION_A: &str = "https://example.org/relation/veca";
const RELATION_B: &str = "https://example.org/relation/vecb";

#[test]
fn cross_space_conjunction_refused_without_license_and_allowed_with_one() {
    let rows: &[(&str, &[f64])] = &[("a", &[0.0, 0.0]), ("b", &[1.0, 0.0]), ("c", &[2.0, 0.0])];
    let fixture_a = support::iri_corpus_f32("spaceA", DistanceMetric::SquaredEuclidean, rows);
    let fixture_b = support::iri_corpus_f32("spaceB", DistanceMetric::SquaredEuclidean, rows);
    assert_ne!(
        fixture_a.full_space.into_bytes(),
        fixture_b.full_space.into_bytes(),
        "distinct families must occupy distinct vector spaces"
    );

    // The refusal deviation is identical for both licensed and unlicensed algebras, so
    // one set of descriptors serves both runs.
    let refusal_identity = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
    let dimension = RelationAnnotationDimension::Distance;

    let wired_a = wire(
        &fixture_a,
        RELATION_A,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &refusal_identity,
        dimension.clone(),
        None,
    );
    let wired_b = wire(
        &fixture_b,
        RELATION_B,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &refusal_identity,
        dimension,
        None,
    );
    let providers = QueryRelationProviders::new(
        vec![
            RelationProviderRegistration::new(
                wired_a.provider.descriptor().clone(),
                8,
                &wired_a.provider,
            )
            .unwrap(),
            RelationProviderRegistration::new(
                wired_b.provider.descriptor().clone(),
                8,
                &wired_b.provider,
            )
            .unwrap(),
        ],
        RelationProviderBudget::new(16, 64).unwrap(),
        &NEVER_CANCELLED,
    )
    .unwrap();
    // Conjoin both similarity literals over the shared bound query but independent
    // candidate slots, so each provider is invoked in its moded (bound query, unbound
    // candidate) shape while the annotation fold must still `multiply` a space-A score by
    // a space-B score — the cross-space combination the algebra governs.
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ex:pair(Q, D1, D2) :- ex:relation/veca(Q, D1), ex:relation/vecb(Q, D2).\n\
         ?- ex:pair(ex:a, D1, D2).\n",
    )
    .expect("cross-space conjunction program");
    let store = anchor_store();

    let certified = [
        AnnotationQueryClass::PositiveAcyclic,
        AnnotationQueryClass::PositiveRecursive,
        AnnotationQueryClass::PositiveNaryAcyclic,
        AnnotationQueryClass::PositiveNaryRecursive,
    ];

    // Unlicensed: the algebra's ⊗ hard-fails on the incompatible spaces.
    let unlicensed = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
    let unlicensed_contract = unlicensed.annotation_contract(certified);
    let refused = dispatch_query_annotated_with_relations(
        &snapshot(&store),
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        RelationAnnotationRequest::new(
            AnnotationRequest::new(&unlicensed, &unlicensed_contract, no_fact_score),
            &providers,
        ),
    );
    assert!(
        matches!(refused, Err(RelationQueryError::Query { .. })),
        "an unlicensed cross-space conjunction must fail in the annotation fold, got {refused:?}"
    );

    // The declared SemiringLaw deviation is disclosed by the algebra's contract.
    let approximation = unlicensed_contract
        .approximation
        .as_ref()
        .expect("refusal algebra declares an approximation");
    assert!(
        approximation
            .deviates_from
            .contains(&SemiringLaw::MultiplyAssociative)
    );

    // Licensed: naming the space pair permits the conjunction.
    let mut licensing = BTreeSet::new();
    licensing.insert((
        fixture_a.full_space.into_bytes(),
        fixture_b.full_space.into_bytes(),
    ));
    let licensed = VectorSpaceScopedAlgebra::with_cross_space_refusal(licensing);
    let licensed_contract = licensed.annotation_contract(certified);
    let allowed = dispatch_query_annotated_with_relations(
        &snapshot(&store),
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        RelationAnnotationRequest::new(
            AnnotationRequest::new(&licensed, &licensed_contract, no_fact_score),
            &providers,
        ),
    )
    .expect("a licensed cross-space conjunction completes");
    assert!(!allowed.answer.answers.is_empty());
    assert!(
        allowed
            .answer
            .answers
            .iter()
            .all(|answer| answer.annotation.spaces.len() == 2),
        "a licensed conjunction unions both contributing vector spaces"
    );
}

// --------------------------------------------------------------------------- //
// The receipt names every contributing PURREMB identity and the invocation lineage.
// --------------------------------------------------------------------------- //

#[test]
fn receipt_names_every_contributing_purremb_identity() {
    let fixture = distinct_corpus();
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let receipt = wired.provider.retrieval_receipt();
    assert_eq!(receipt.artifact_root.len(), 64);
    assert_eq!(receipt.matrix.len(), 64);
    assert_eq!(receipt.target_set.len(), 64);
    assert_eq!(receipt.vector_space.len(), 64);
    assert_eq!(receipt.family.len(), 64);
    assert_eq!(receipt.metric_name, "squared-euclidean");
    assert_eq!(receipt.retrieval_policy, "exact-full-space");
    assert_eq!(receipt.source_verification_mode, "exact");
    assert_eq!(receipt.recall, Some(1.0));

    let providers = providers_for(
        &wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let result = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a").unwrap();
    let invocation = result
        .receipt
        .invocations
        .iter()
        .find(|invocation| invocation.status == RelationInvocationStatus::Complete)
        .expect("a complete invocation receipt");
    assert_eq!(invocation.relation_iri, RELATION);
    assert_eq!(
        invocation.model_iri,
        "https://example.org/model/purremb-embedding-v1"
    );
    assert!(
        invocation
            .artifact_generation
            .contains(receipt.artifact_root.as_str())
    );
    assert_eq!(
        invocation.annotation_dimension_iri,
        RelationAnnotationDimension::Distance.iri()
    );
    assert!(
        result
            .receipt
            .contributing_providers
            .iter()
            .any(|(provider, _)| provider == "https://example.org/provider/purremb")
    );
}

// --------------------------------------------------------------------------- //
// The provider's dispatch answer equals a naive in-test reference scorer.
// --------------------------------------------------------------------------- //

#[test]
fn dispatch_answer_matches_a_naive_reference_scorer() {
    let fixture = distinct_corpus();
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    // A top-2 prefix so the reference must reproduce the provider's score-based
    // k-boundary selection, not merely the full corpus.
    const K: usize = 2;
    let providers = providers_for(
        &wired.provider,
        K,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let result = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a").unwrap();

    // Reference: score every row against the query vector, sort by the total-order
    // distance key then candidate target identity (the provider's documented k-boundary
    // tie-break), and take the top K.
    let query = fixture
        .rows
        .iter()
        .find(|row| row.iri.as_deref() == Some(ex("a").as_str()))
        .expect("query row present")
        .vector
        .clone();
    let mut reference: Vec<(u64, [u8; 32], String)> = fixture
        .rows
        .iter()
        .map(|row| {
            let distance: f64 = query
                .iter()
                .zip(&row.vector)
                .map(|(q, c)| (q - c) * (q - c))
                .sum();
            (
                total_order_bits(distance),
                row.target,
                format!("<{}>", row.iri.clone().unwrap()),
            )
        })
        .collect();
    reference.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    let reference_topk: BTreeSet<String> = reference
        .into_iter()
        .take(K)
        .map(|(_, _, iri)| iri)
        .collect();

    // The dispatch answer set (presented in a stable order) must equal the reference
    // top-K selection.
    let dispatched: BTreeSet<String> = ordered_iris(&result).into_iter().collect();
    assert_eq!(dispatched, reference_topk);
}

// --------------------------------------------------------------------------- //
// The non-exact (Vague) preservation claim is carried into the answer.
// --------------------------------------------------------------------------- //

#[test]
fn vague_similarity_preservation_is_carried_into_the_answer() {
    use gmeow_logic_compile::ir::PreservationKind;

    let fixture = distinct_corpus();
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let store = anchor_store();
    let wired = wire(
        &fixture,
        RELATION,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
        &algebra,
        RelationAnnotationDimension::Distance,
        None,
    );
    let providers = providers_for(
        &wired.provider,
        8,
        RelationProviderBudget::new(8, 64).unwrap(),
    );
    let result = run_direct(&snapshot(&store), &providers, &algebra, &contract, "a").unwrap();
    assert!(
        result
            .answer
            .preservation
            .unsupported_constructs
            .contains("https://blackcatinformatics.ca/logic/VagueVectorSimilarity"),
        "a vector similarity is logic:Vague, not an equivalence"
    );
    assert!(
        result
            .answer
            .preservation
            .polarities
            .contains(&PreservationKind::SoundUnder)
    );
}

// --------------------------------------------------------------------------- //
// Focused map_row assertions over a heterogeneous statement/document artifact.
//
// Triple-term retrieval itself is exercised end-to-end through the query-goal grammar's
// quoted-triple term (see `triple_term_query_returns_metric_ordered_statement_candidates`).
// This test isolates the reconstruction fidelity and the unsupported-kind (`Document`)
// rejection at the `PurrembTargetMapper` boundary, where a single row's mapping can be
// asserted in isolation against a real verified statement/document artifact.
// --------------------------------------------------------------------------- //

#[test]
fn statement_rows_reconstruct_as_triple_terms_and_documents_are_rejected() {
    use gmeow_logic::purremb_relation::PurrembMapError;
    use purrdf::{EmbeddingView, verify_embedding};

    let statement = support::statement_and_document_fixture();
    let mut view = EmbeddingView::from_bytes(&statement.artifact_bytes).expect("view");
    verify_embedding(&mut view).expect("verified statement view");
    let target_set = view
        .target_set(statement.fixture.target_set)
        .expect("target set present");
    let mapper = PurrembTargetMapper;

    // Each statement row reconstructs losslessly into an RDF 1.2 triple term.
    for (position, &row) in statement.statement_rows.iter().enumerate() {
        let term = mapper
            .map_row(&view, &target_set, row, &ColumnKind::TripleTerm)
            .expect("statement reconstructs as a triple term");
        let (subject, predicate, object) = &statement.statement_terms[position];
        match term {
            TermValue::Triple { s, p, o } => {
                assert!(matches!(&*s, TermValue::Iri(iri) if iri == subject));
                assert!(matches!(&*p, TermValue::Iri(iri) if iri == predicate));
                assert!(matches!(&*o, TermValue::Iri(iri) if iri == object));
            }
            other => panic!("expected a triple term, got {other:?}"),
        }
        // A triple term does not conform to an IRI column.
        assert!(matches!(
            mapper.map_row(&view, &target_set, row, &ColumnKind::Iri),
            Err(PurrembMapError::ColumnMismatch(_))
        ));
    }

    // The external Document row has no lossless RDF term reconstruction: rejected,
    // never fabricated.
    assert!(matches!(
        mapper.map_row(&view, &target_set, statement.document_row, &ColumnKind::Iri),
        Err(PurrembMapError::UnsupportedKind(_))
    ));
}
