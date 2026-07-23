// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Performance & cost lane for the verified PURREMB external-relation provider.
//!
//! These are **correctness gates**, not wall-clock timers: a process-wide counting
//! [`std::alloc::GlobalAlloc`] proves the *shape* of the retrieval provider's memory cost.
//! The provider borrows the verified matrix and scores it row-by-row through the aligned
//! `native_*_row` slices, so one retrieval call touches an `O(k + dimension)` scoring
//! working set — never a whole-matrix (`O(rows * dimension)`) copy — the once-verified view
//! is retained across calls (a per-call `re_pin` is a structural `reopen_prevalidated`,
//! never a re-verification), and the bounded top-`k` selector keeps `O(k)` live. A separate
//! query-local cache assertion drives the real dispatch fixpoint.
//!
//! # What is measured, and why not through full dispatch
//!
//! Assertions 1–4 invoke [`PurrembRetrievalProvider::call`] — *the scan itself* — directly.
//! A full `dispatch_query_annotated_with_relations` run wraps the scan in the annotated
//! evaluator's own working set (fixpoint frames, per-answer lineage, provider receipts),
//! whose peak swamps the scan's allocation signal (it measures ~1.5 MB regardless of the
//! matrix). Calling `call` isolates exactly `re_pin` + query-row resolution + the metric
//! scan + top-`k` mapping, which is the surface these invariants are about. The full
//! dispatch integration path (recursion, caching, lineage) is exercised by assertion 5 here
//! and by the acceptance matrix in `purremb_relations.rs`.
//!
//! # Fixture geometry: wide and short
//!
//! The cost fixtures are **wide** (large `dimension`) and **short** (modest `rows`). The
//! authoritative stored matrix is `rows * dimension * 4` bytes, dominated by the scalar
//! payload; the structural view that `re_pin` reopens scales with the (small) target count,
//! not the scalar payload. So a whole-matrix copy would be plainly visible as a peak on the
//! order of the matrix, while streaming borrowed rows keeps the peak far below it. A
//! `dimension`-doubling discriminator makes this airtight: doubling the matrix must not
//! double the scan peak.
//!
//! # The honest bound
//!
//! The "no whole-matrix copy" invariant is on the **peak working set** (high-water of *live*
//! bytes), which is `O(k + dimension)`. The *cumulative requested* bytes of one scan are
//! `O(rows * dimension)` because `collect_full_row` allocates one bounded `O(dimension)`
//! conversion buffer per row that is immediately dropped — a transient per-row buffer, never
//! a retained whole-matrix copy. Peak (not cumulative-requested) is therefore the metric
//! that separates "streams borrowed rows" from "copies the matrix", and it is what the
//! bounds below assert.

#[allow(
    dead_code,
    reason = "the shared PURREMB fixture support module also exposes builders used only by the acceptance-matrix test binary"
)]
#[path = "purremb_support/mod.rs"]
mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

use gmeow_logic::annotation::{
    AnnotationContract, AnnotationFactRef, AnnotationRequest, TupleAnnotationAlgebra,
};
use gmeow_logic::dispatch::{RelationAnnotationRequest, dispatch_query_annotated_with_relations};
use gmeow_logic::external_relation::{
    ExternalRelationProvider, NeverCancelled, QueryRelationProviders, RelationAnnotationDimension,
    RelationBatch, RelationCall, RelationCancellation, RelationOrderDirection, RelationOrdering,
    RelationProviderBudget, RelationProviderDescriptor, RelationProviderError,
    RelationProviderRegistration,
};
use gmeow_logic::purremb_relation::{
    PurrembBinding, PurrembRetrievalProvider, RetrievalPolicy, RetrievalScore, SpaceTaggedScore,
    VectorSpaceScopedAlgebra, purremb_descriptor, purremb_generation_iri,
};
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::seam::WorldFactSnapshot;
use gmeow_logic::store::WorldStore;
use gmeow_logic_compile::result_shape::ColumnKind;
use purrdf::{DistanceMetric, SourceVerificationMode, TermValue, VectorSpaceId};

use support::Fixture;

// --------------------------------------------------------------------------- //
// Process-wide counting allocator (mirrors PurRDF's `purremb_alloc` probe).
// --------------------------------------------------------------------------- //

static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOCATION_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicI64 = AtomicI64::new(0);
static PEAK_BYTES: AtomicI64 = AtomicI64::new(0);

struct CountingAllocator;

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn record_allocation(size: usize) {
    ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    ALLOCATION_BYTES.fetch_add(usize_to_u64(size), Ordering::Relaxed);
    let size = usize_to_i64(size);
    let live = LIVE_BYTES
        .fetch_add(size, Ordering::Relaxed)
        .saturating_add(size);
    let mut peak = PEAK_BYTES.load(Ordering::Relaxed);
    while live > peak {
        match PEAK_BYTES.compare_exchange_weak(peak, live, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => peak = observed,
        }
    }
}

fn record_deallocation(size: usize) {
    LIVE_BYTES.fetch_sub(usize_to_i64(size), Ordering::Relaxed);
}

// SAFETY: every operation delegates to `System` with the exact incoming pointer/layout.
// The atomic accounting does not affect allocator ownership.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation(layout.size());
        // SAFETY: delegated with the exact layout supplied by the caller.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record_deallocation(layout.size());
        // SAFETY: delegated with the exact pointer/layout supplied by the caller.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_deallocation(layout.size());
        record_allocation(new_size);
        // SAFETY: delegated with the exact pointer/layout and requested size.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[derive(Clone, Copy)]
struct AllocationSnapshot {
    count: u64,
    requested: u64,
    live: i64,
    peak: i64,
}

/// Rebase the peak high-water at the current live level and return the baseline snapshot.
fn reset_peak() -> AllocationSnapshot {
    let live = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_BYTES.store(live, Ordering::Relaxed);
    AllocationSnapshot {
        count: ALLOCATION_COUNT.load(Ordering::Relaxed),
        requested: ALLOCATION_BYTES.load(Ordering::Relaxed),
        live,
        peak: live,
    }
}

fn snapshot() -> AllocationSnapshot {
    AllocationSnapshot {
        count: ALLOCATION_COUNT.load(Ordering::Relaxed),
        requested: ALLOCATION_BYTES.load(Ordering::Relaxed),
        live: LIVE_BYTES.load(Ordering::Relaxed),
        peak: PEAK_BYTES.load(Ordering::Relaxed),
    }
}

/// The allocation cost of one measured window.
#[derive(Clone, Copy, Debug)]
struct Cost {
    /// Distinct allocations recorded in the window.
    allocations: u64,
    /// Cumulative requested bytes (includes transient per-row conversion buffers).
    requested: u64,
    /// Net bytes still live at the end of the window (retained working set).
    retained: i64,
    /// Peak live bytes above the window's baseline (the true working-set high-water).
    peak: i64,
}

/// Run `body` inside a measured allocation window rebased at entry.
fn measure<T>(body: impl FnOnce() -> T) -> (T, Cost) {
    let before = reset_peak();
    let value = body();
    let after = snapshot();
    let cost = Cost {
        allocations: after.count.saturating_sub(before.count),
        requested: after.requested.saturating_sub(before.requested),
        retained: after.live.saturating_sub(before.live),
        peak: after.peak.saturating_sub(before.live),
    };
    (value, cost)
}

fn peak_usize(cost: Cost) -> usize {
    usize::try_from(cost.peak.max(0)).unwrap_or(usize::MAX)
}

fn retained_usize(cost: Cost) -> usize {
    usize::try_from(cost.retained.max(0)).unwrap_or(usize::MAX)
}

// --------------------------------------------------------------------------- //
// Fixture geometry and shared wiring.
// --------------------------------------------------------------------------- //

/// Corpus row count: short enough that the reopened structural view is far below the scalar
/// payload, long enough that `k` stays decisively under `N`.
const ROWS: usize = 1_024;
/// Base stored family dimension (wide, so the scalar matrix dominates the artifact).
const DIM: u32 = 512;
/// Doubled dimension for the matrix-copy discriminator (twice the scalar payload).
const DIM_WIDE: u32 = 1_024;

/// Authoritative stored-matrix byte size (`f32`) for a given dimension.
const fn matrix_bytes(dimension: u32) -> usize {
    ROWS * dimension as usize * 4
}

const WORLD: &str = "https://example.org/world/purremb-perf";
const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const RELATION: &str = "https://example.org/relation/vector";
const GEN_BASE: &str = "https://example.org/index/purremb-perf";
const ORDER_CRITERION: &str = "https://blackcatinformatics.ca/logic/VectorDistanceOrder";

static NEVER_CANCELLED: NeverCancelled = NeverCancelled;

fn ex(local: &str) -> String {
    format!("https://example.org/{local}")
}

/// A minimal single-anchor world; PURREMB retrieval needs no RDF facts, but the annotated
/// evaluator still runs over a real world snapshot.
fn anchor_store() -> WorldStore {
    let store = WorldStore::new();
    store.insert_quad(WORLD, &ex("anchor"), &ex("present"), &ex("yes"));
    store
}

fn snapshot_of(store: &WorldStore) -> WorldFactSnapshot {
    WorldFactSnapshot::from_world(store, WORLD, PROFILE).expect("world snapshot")
}

/// Fold a computed retrieval score into the space-tagged algebra element.
fn annotate(score: RetrievalScore) -> SpaceTaggedScore {
    SpaceTaggedScore::single(
        score.distance,
        VectorSpaceId::from_raw(score.vector_space),
        score.metric_code,
    )
}

/// No RDF-asserted fact is scored.
fn no_fact_score(_: AnnotationFactRef<'_>) -> Option<SpaceTaggedScore> {
    None
}

fn ascending_order() -> RelationOrdering {
    RelationOrdering::new(ORDER_CRITERION, RelationOrderDirection::Ascending).expect("ordering")
}

fn open_binding(fixture: &Fixture) -> PurrembBinding<'_> {
    PurrembBinding::open(
        &fixture.artifact_bytes,
        &fixture.source_bytes,
        fixture.selection(RetrievalPolicy::ExactFullSpace),
        SourceVerificationMode::Exact,
    )
    .expect("verified PURREMB binding")
}

fn descriptor_for(relation: &str, artifact_root_hex: &str) -> RelationProviderDescriptor {
    let generation = purremb_generation_iri(
        GEN_BASE,
        artifact_root_hex,
        RetrievalPolicy::ExactFullSpace,
        SourceVerificationMode::Exact,
    );
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    purremb_descriptor(
        "https://example.org/provider/purremb",
        generation,
        "https://example.org/model/purremb-embedding-v1",
        relation,
        vec![ColumnKind::Iri, ColumnKind::Iri],
        RelationAnnotationDimension::Distance,
        algebra.identity().to_owned(),
        ascending_order(),
    )
    .expect("valid PURREMB descriptor")
}

fn provider_for(fixture: &Fixture) -> PurrembRetrievalProvider<'_, SpaceTaggedScore> {
    let binding = open_binding(fixture);
    let descriptor = descriptor_for(RELATION, binding.artifact_root_hex());
    PurrembRetrievalProvider::new(binding, descriptor, Box::new(annotate))
        .expect("valid provider contract")
}

fn providers_for<'a>(
    provider: &'a dyn ExternalRelationProvider<SpaceTaggedScore>,
    descriptor: RelationProviderDescriptor,
    per_call_limit: usize,
) -> QueryRelationProviders<'a, SpaceTaggedScore> {
    QueryRelationProviders::new(
        vec![
            RelationProviderRegistration::new(descriptor, per_call_limit, provider)
                .expect("registration"),
        ],
        RelationProviderBudget::new(64, ROWS as u64).expect("budget"),
        &NEVER_CANCELLED,
    )
    .expect("sealed provider set")
}

/// Build a moded retrieval call: the query slot bound to an in-corpus IRI, candidate slot
/// unbound, requesting the top `limit` under the ascending distance order.
fn build_call(query_local: &str, limit: usize) -> RelationCall {
    RelationCall {
        request_iri: "https://example.org/request/purremb-perf".to_owned(),
        query_contract_hash: "purremb-perf-contract".to_owned(),
        relation_iri: RELATION.to_owned(),
        bounds: vec![Some(TermValue::iri(ex(query_local))), None],
        limit,
        ordering: ascending_order(),
    }
}

/// Invoke the provider scan directly and require a complete `limit`-row batch.
fn scan(
    provider: &PurrembRetrievalProvider<'_, SpaceTaggedScore>,
    call: &RelationCall,
) -> RelationBatch<SpaceTaggedScore> {
    provider
        .call(call, &NEVER_CANCELLED)
        .expect("complete retrieval batch")
}

// --------------------------------------------------------------------------- //
// Assertion 5 support: a counting delegate over the real provider.
// --------------------------------------------------------------------------- //

/// A thin delegate that counts `call` invocations then forwards to the inner provider, so a
/// query-local cache hit is observable as an inner call that never fires.
struct CountingProvider<'a> {
    inner: PurrembRetrievalProvider<'a, SpaceTaggedScore>,
    calls: AtomicUsize,
}

impl<'a> CountingProvider<'a> {
    fn new(inner: PurrembRetrievalProvider<'a, SpaceTaggedScore>) -> Self {
        Self {
            inner,
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ExternalRelationProvider<SpaceTaggedScore> for CountingProvider<'_> {
    fn call(
        &self,
        call: &RelationCall,
        cancellation: &dyn RelationCancellation,
    ) -> Result<RelationBatch<SpaceTaggedScore>, RelationProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.call(call, cancellation)
    }
}

// --------------------------------------------------------------------------- //
// The lane.
// --------------------------------------------------------------------------- //

#[test]
fn purremb_performance_and_cost_lane() {
    // Cost fixture: 1024 rows × 512-dim f32, squared-Euclidean, fixed vector-space contract.
    // Built once, entirely outside every measured window.
    let fixture =
        support::iri_corpus_f32_large("perf", DistanceMetric::SquaredEuclidean, ROWS, DIM);
    assert_eq!(fixture.rows.len(), ROWS);
    assert_eq!(fixture.stored_dimension, DIM);
    let query_local = support::large_corpus_local(0);
    let matrix = matrix_bytes(DIM);

    // Construct the provider ONCE — full open + verify happens here, outside every window.
    let provider = provider_for(&fixture);

    // ----------------------------------------------------------------------- //
    // Assertion 1 — no whole-matrix copy.
    //
    // One scan reads borrowed `native_f32_row`s and keeps only a bounded working set. Peak
    // live bytes are O(k + dimension) ≪ the stored matrix.
    // ----------------------------------------------------------------------- //
    const K1: usize = 8;
    let call = build_call(&query_local, K1);
    let (batch, one_scan) = measure(|| scan(&provider, &call));
    assert_eq!(batch.rows.len(), K1, "top-k prefix delivered");
    eprintln!(
        "[purremb_perf] one_scan k={K1} dim={DIM}: peak={} retained={} requested={} allocations={} matrix={matrix}",
        one_scan.peak, one_scan.retained, one_scan.requested, one_scan.allocations
    );
    assert!(
        peak_usize(one_scan) < matrix / 4,
        "a single scan must not copy the matrix: peak {} vs matrix {matrix}",
        one_scan.peak
    );
    assert!(
        retained_usize(one_scan) < matrix / 8,
        "a single scan must retain no matrix-scale buffer: retained {}",
        one_scan.retained
    );

    // Discriminator: doubling the dimension DOUBLES the stored matrix. A whole-matrix copy
    // would roughly double the scan peak; streaming borrowed rows adds only O(Δdimension).
    let wide = support::iri_corpus_f32_large(
        "perf-wide",
        DistanceMetric::SquaredEuclidean,
        ROWS,
        DIM_WIDE,
    );
    let wide_provider = provider_for(&wide);
    let wide_call = build_call(&support::large_corpus_local(0), K1);
    let (wide_batch, wide_scan) = measure(|| scan(&wide_provider, &wide_call));
    assert_eq!(wide_batch.rows.len(), K1);
    let matrix_growth = matrix_bytes(DIM_WIDE) - matrix;
    let peak_growth = peak_usize(wide_scan).saturating_sub(peak_usize(one_scan));
    eprintln!(
        "[purremb_perf] wide_scan dim={DIM_WIDE}: peak={} | peak_growth={peak_growth} matrix_growth={matrix_growth}",
        wide_scan.peak
    );
    assert!(
        peak_growth < matrix_growth / 8,
        "doubling the matrix must not scale the scan peak: peak grew {peak_growth}, matrix grew {matrix_growth} — proves borrowed-row streaming, not a copy"
    );

    // ----------------------------------------------------------------------- //
    // Assertion 2 — no reopen/reverification per provider call.
    //
    // Reuse the already-verified provider. Run 20 distinct-query scans. Each `call` re-pins
    // via structural `reopen_prevalidated`, never re-runs `verify_embedding`/`from_bytes`'s
    // full verification (which would re-derive every target identity and re-check every
    // scalar). Peak across all 20 stays near a single scan; nothing accumulates.
    // ----------------------------------------------------------------------- //
    const REPEATS: usize = 20;
    let ((), many_scans) = measure(|| {
        for index in 0..REPEATS {
            let local = support::large_corpus_local(index);
            let call = build_call(&local, K1);
            let batch = scan(&provider, &call);
            assert_eq!(batch.rows.len(), K1);
        }
    });
    eprintln!(
        "[purremb_perf] {REPEATS}_scans: peak={} retained={} requested={} allocations={}",
        many_scans.peak, many_scans.retained, many_scans.requested, many_scans.allocations
    );
    assert!(
        peak_usize(many_scans) < matrix / 4,
        "no call re-verifies the matrix: {REPEATS}-scan peak {} vs matrix {matrix}",
        many_scans.peak
    );
    // The 20-scan peak is within a small constant of one scan — it does NOT accumulate a
    // per-call re-verification cost.
    assert!(
        peak_usize(many_scans) < peak_usize(one_scan) * 4 + 64 * 1024,
        "{REPEATS}-scan peak {} must stay near the single-scan peak {}",
        many_scans.peak,
        one_scan.peak
    );
    assert!(
        retained_usize(many_scans) < matrix / 8,
        "repeated scans must accumulate nothing: retained {}",
        many_scans.retained
    );

    // ----------------------------------------------------------------------- //
    // Assertion 3 — bounded top-k memory proportional to k.
    //
    // Same corpus, k=4 vs k=64. The scan peak scales with k (bounded), never with the
    // corpus, and k stays far below N.
    // ----------------------------------------------------------------------- //
    const K_SMALL: usize = 4;
    const K_LARGE: usize = 64;
    // Compile-time: the large k stays far below the corpus size (k ≪ N).
    const { assert!(K_LARGE < ROWS / 8, "k must stay far below the corpus size") };

    let call_small = build_call(&query_local, K_SMALL);
    let (small_batch, small_scan) = measure(|| scan(&provider, &call_small));
    assert_eq!(small_batch.rows.len(), K_SMALL);

    let call_large = build_call(&query_local, K_LARGE);
    let (large_batch, large_scan) = measure(|| scan(&provider, &call_large));
    assert_eq!(large_batch.rows.len(), K_LARGE);

    eprintln!(
        "[purremb_perf] k={K_SMALL}: peak={} | k={K_LARGE}: peak={}",
        small_scan.peak, large_scan.peak
    );
    assert!(
        peak_usize(small_scan) < matrix / 4 && peak_usize(large_scan) < matrix / 4,
        "top-k memory is bounded by k, not the corpus: k4 peak {}, k64 peak {}, matrix {matrix}",
        small_scan.peak,
        large_scan.peak
    );
    // The k=64 → k=4 gap is bounded by Δk (extra heap entries + emitted rows), far below a
    // corpus-scale buffer.
    let gap = peak_usize(large_scan).saturating_sub(peak_usize(small_scan));
    assert!(
        gap < matrix / 8,
        "the k-scaling increment {gap} must be O(Δk), not O(corpus)"
    );

    // ----------------------------------------------------------------------- //
    // Assertion 4 — no materialized scratch-world candidate graph.
    //
    // A scan's working set is proportional to the ANSWER (k) and dimension, not the corpus:
    // assertion 1's dimension discriminator shows the peak does not track the scalar payload,
    // and assertion 3 shows it tracks k. Structurally, `PurrembRetrievalProvider::call`
    // receives a `RelationCall` and returns a `RelationBatch` over the SHARED verified matrix
    // view; it constructs no second `WorldFactSnapshot`/`WorldStore`/graph (those types are
    // not reachable from the provider). A pure structural assertion is not expressible
    // through the public API, so the corpus-independent peak bound is the honest proof,
    // recorded here explicitly.
    // ----------------------------------------------------------------------- //
    assert!(
        peak_usize(large_scan) < matrix / 4,
        "no candidate graph proportional to the corpus is materialized: peak {} vs matrix {matrix}",
        large_scan.peak
    );

    // ----------------------------------------------------------------------- //
    // Assertion 5 — identical-call cache reuse (through the real dispatch fixpoint).
    //
    // A small a..d corpus wrapped in a counting delegate. A recursion re-derives the SAME
    // provider literal (same relation, bound query ex:a, limit, ordering) across fixpoint
    // rounds; the query-local cache serves every repeat, so the inner provider runs exactly
    // once.
    // ----------------------------------------------------------------------- //
    let small_corpus = support::iri_corpus_f32(
        "perf-cache",
        DistanceMetric::SquaredEuclidean,
        &[
            ("a", &[0.0, 0.0]),
            ("b", &[1.0, 0.0]),
            ("c", &[3.0, 0.0]),
            ("d", &[6.0, 0.0]),
        ],
    );
    let counting = CountingProvider::new(provider_for(&small_corpus));
    let cache_providers = providers_for(&counting, counting.inner.descriptor().clone(), 8);
    let cache_store = anchor_store();
    let program = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
         ex:reach(Q, D) :- ex:relation/vector(Q, D).\n\
         ex:reach(Q, D) :- ex:reach(Q, M), ex:relation/vector(Q, D).\n\
         ?- ex:reach(ex:a, D).\n",
    )
    .expect("recursive provider-seeded program");
    let algebra = VectorSpaceScopedAlgebra::new(BTreeSet::new(), BTreeSet::new());
    let contract = AnnotationContract::exact();
    let cache_result = dispatch_query_annotated_with_relations(
        &snapshot_of(&cache_store),
        WORLD,
        &program,
        PROFILE,
        &Budget::default(),
        RelationAnnotationRequest::new(
            AnnotationRequest::new(&algebra, &contract, no_fact_score),
            &cache_providers,
        ),
    )
    .expect("recursive retrieval query");
    assert!(
        !cache_result.answer.answers.is_empty(),
        "the recursion seeds at least the provider candidates"
    );
    assert_eq!(
        counting.calls(),
        1,
        "identical (relation, bounds, limit, ordering) rounds must hit the query-local cache"
    );
}
