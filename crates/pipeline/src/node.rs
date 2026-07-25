// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The pipeline node model: the [`Stage`] trait and the in-memory
//! [`StageInput`] / [`StageOutput`] / [`StageProduct`] handles a stage exchanges
//! .
//!
//! A stage is re-cut for in-memory dataflow: it consumes the products of its
//! upstream stages (live handles, not re-parsed files) and emits one product.
//! Each resource a stage [`Stage::resources`] declares is held exclusively while
//! it runs — two stages competing for the same resource serialize; everything
//! else is parallel within its topological level. The reasoning stage requires
//! [`ENGINE_RESOURCE`] (the process-wide reasoning state is exclusive).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use purrdf::provenance::DatasetProvenance;
use purrdf::{PipelineBundle, RdfDataset};

use crate::bundle::{
    PipelineHandle, bundle_artifact, bundle_artifacts, bundle_from_artifacts,
    bundle_from_artifacts_over,
};
/// The GMEOW namespace prefix that every pipeline term lives under.
pub(crate) const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// The `gmeow:engineResource` IRI: the process-wide reasoning engine, declared as
/// an exclusive [`Stage::resources`] requirement by the sole `Reason` stage. The
/// scheduler serializes any two stages requiring the same resource (the
/// declarative replacement for a hardcoded engine mutex).
pub const ENGINE_RESOURCE: &str = "https://blackcatinformatics.ca/gmeow/engineResource";

/// The `gmeow:sinkCapability` IRI: the single narrow-waist serialization exit. Held
/// (via [`Stage::capabilities`] / RDF `gmeow:hasCapability`) by exactly ONE stage in
/// the DAG — the loader HARD-fails unless precisely one stage holds it.
pub const SINK_CAPABILITY: &str = "https://blackcatinformatics.ca/gmeow/sinkCapability";

/// The `gmeow:sourceOrigin` IRI: the authored-source loader. The stage that holds it
/// stamps its emitted quads with provenance origin `Source`; every other stage stamps
/// `Generated` (the kind-enum replacement — origin is read off a capability, not a tag).
pub const SOURCE_ORIGIN: &str = "https://blackcatinformatics.ca/gmeow/sourceOrigin";

/// The product of one stage: its id, the hex content digest of the value it
/// produced (the cache-key contribution downstream stages fold in — Merkle
/// composition P2), and the structured [`PipelineBundle`] it emitted.
///
/// # The carrier (C4)
///
/// The carrier is an [`Arc<PipelineBundle<PipelineHandle>>`]: the frozen RDF
/// dataset + lookaside + content-addressed blob store + provenance + typed-handle
/// lane. The pre-C4 named byte artifacts (logical path → bytes) ride the bundle's
/// byte-artifact lane (see [`crate::bundle`]); `gts_compose` / `gts_sink` fold the
/// upstream lane into the one bundle (P3/P4). C2/C3/C5 progressively replace
/// the byte reads with dataset/lane reads and retire the lane per stage.
///
/// `digest` is the cache key: for a freshly produced bundle it is
/// `bundle.digest().to_hex()` (the handle-excluded content fold), so `combined()`
/// over stages stays an order-independent Merkle fold; abstract/test products may
/// carry an explicit digest decoupled from the (empty) carrier.
#[derive(Debug, Clone)]
pub struct StageProduct {
    /// The id of the stage that produced this.
    pub stage_id: String,
    /// The hex SHA-256 digest of the produced value (content-addressed cache key).
    pub digest: String,
    /// The structured carrier this stage emitted: the frozen dataset, lookaside
    /// (including the byte-artifact lane), blob store, provenance, and handle lane.
    pub bundle: Arc<PipelineBundle<PipelineHandle>>,
}

impl StageProduct {
    /// Construct an artifact-free product with an explicit digest (abstract
    /// stages / tests). Real transform stages use [`Self::from_artifacts`].
    ///
    /// The carrier is an empty bundle; the explicit `digest` is the cache key
    /// (decoupled from the empty carrier so existing abstract stages keep their
    /// declared digest).
    pub fn new(stage_id: impl Into<String>, digest: impl Into<String>) -> Self {
        Self {
            stage_id: stage_id.into(),
            digest: digest.into(),
            bundle: Arc::new(bundle_from_artifacts(
                BTreeMap::new(),
                DatasetProvenance::new(),
            )),
        }
    }

    /// Construct a product from emitted named byte artifacts; they ride the
    /// bundle's byte-artifact lane and the digest is the bundle's content fold.
    pub fn from_artifacts(
        stage_id: impl Into<String>,
        artifacts: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        let bundle = bundle_from_artifacts(artifacts, DatasetProvenance::new());
        Self::from_bundle(stage_id, Arc::new(bundle))
    }

    /// Construct a product from named byte artifacts riding over an explicit
    /// backing `dataset` (the lane travels alongside the frozen graph).
    pub fn from_artifacts_over(
        stage_id: impl Into<String>,
        dataset: Arc<RdfDataset>,
        artifacts: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        let bundle = bundle_from_artifacts_over(dataset, artifacts, DatasetProvenance::new());
        Self::from_bundle(stage_id, Arc::new(bundle))
    }

    /// Construct a product wrapping an already-assembled carrier; the digest is the
    /// bundle's content fold (handle lane excluded).
    pub fn from_bundle(
        stage_id: impl Into<String>,
        bundle: Arc<PipelineBundle<PipelineHandle>>,
    ) -> Self {
        let digest = bundle.digest().to_hex();
        Self {
            stage_id: stage_id.into(),
            digest,
            bundle,
        }
    }

    /// Borrow the structured carrier this product emitted.
    pub fn bundle(&self) -> &Arc<PipelineBundle<PipelineHandle>> {
        &self.bundle
    }

    /// Borrow the frozen RDF dataset this product's bundle carries.
    pub fn dataset(&self) -> &RdfDataset {
        self.bundle.dataset()
    }

    /// The bytes of one named byte-artifact-lane entry by logical path.
    pub fn artifact(&self, logical_path: &str) -> Option<&[u8]> {
        bundle_artifact(&self.bundle, logical_path)
    }

    /// The full `(logical_path → bytes)` map of this product's byte-artifact lane,
    /// sorted by path. The surface `run_full` writes / compares against committed.
    pub fn artifacts(&self) -> BTreeMap<String, Vec<u8>> {
        bundle_artifacts(&self.bundle)
    }

    /// The FORWARD-projected diagnostics nodes this product carries on its
    /// `diagnostics:nodes` blob lane ([`crate::stages::carrier::REP_DIAG_NODES`]), or an
    /// EMPTY vec when the product carries none (every non-producer stage). This is the
    /// lane the scheduler reads on a CACHE HIT to recover a diagnostics producer's run
    /// ledger contribution WITHOUT re-running the stage — the blob round-trips through
    /// the per-stage cache, so the recovered nodes are byte-identical to the fresh run.
    /// A present-but-malformed blob is a corrupt product — a HARD FAIL (no-optionality).
    pub fn diag_nodes(&self) -> Vec<gmeow_errors::DiagNode> {
        match crate::bundle::bundle_rep_blob(&self.bundle, crate::stages::carrier::REP_DIAG_NODES) {
            Some(bytes) => serde_json::from_slice(bytes).expect(
                "diagnostics:nodes blob is our own JSON; a decode failure is a corrupt cache",
            ),
            None => Vec::new(),
        }
    }

    /// The authored subject→source-position [`SpanIndex`](crate::ingest::SpanIndex) this
    /// product carries on its `spans:source-table` blob lane
    /// ([`crate::stages::carrier::REP_SPAN_TABLE`]), deserialized. Read by the span-table
    /// consumers (`stage-validate` / `stage-compile-logic`) off the `stage-source-load`
    /// product to lift spans onto their findings.
    ///
    /// The blob being ABSENT is a HARD FAIL ([`crate::error::SpanTableConsumedAfterDrop`]):
    /// the span table is stripped from the source-load product once the last consumer has
    /// run (drop-after-last-consumer), so a reader finding it absent is a stage reaching
    /// for it AFTER the drop — never a legitimate read (the drop level is computed as the
    /// max consumer level, so real consumers always run before it). A present-but-malformed
    /// blob is likewise a HARD FAIL (no-optionality).
    pub fn span_index(&self) -> gmeow_errors::Result<crate::ingest::SpanIndex> {
        match crate::bundle::bundle_rep_blob(&self.bundle, crate::stages::carrier::REP_SPAN_TABLE) {
            Some(bytes) => serde_json::from_slice(bytes).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Decode {
                    message: format!("source-span table blob JSON: {e}"),
                })
            }),
            None => Err(gmeow_errors::Diag::of_kind(
                crate::error::SpanTableConsumedAfterDrop {
                    detail: format!(
                        "product `{}` carries no {} blob",
                        self.stage_id,
                        crate::stages::carrier::REP_SPAN_TABLE
                    ),
                },
            )),
        }
    }
}

/// The input handed to a stage's `run`: the repo root and the products of every
/// stage it `consumes` (live, in-memory — never re-parsed from disk).
pub struct StageInput<'a> {
    /// The repository root the build operates over.
    pub root: &'a Path,
    /// Upstream products keyed by producing-stage id. A stage reads only the
    /// ids it declared in `consumes()`.
    pub upstream: &'a BTreeMap<String, StageProduct>,
}

/// The output a stage's `run` returns.
pub struct StageOutput {
    /// The single product this stage produced.
    pub product: StageProduct,
    /// The pre-lowered diagnostic nodes this stage emits (the FORWARD projection of
    /// its `gmeow_errors::Report` findings). Empty for every stage that produces no
    /// findings; the diagnostics producers (`stage-validate`, `stage-compile-logic`,
    /// and `stage-reason`) populate it from their report. The scheduler folds
    /// these into the run-level `DiagLedger` (fresh run) or reads them back from the
    /// product's `diagnostics:nodes` blob (cache hit), so the ledger is a projection
    /// of the SAME producer findings whether the stage ran or replayed.
    pub diags: Vec<gmeow_errors::DiagNode>,
    /// Optional internal phase timings from a freshly executed stage. These are
    /// observational telemetry only: they are returned to the runner, never folded
    /// into the product digest or persisted in the stage cache.
    pub timings: Vec<StageRunTiming>,
}

/// One internal phase timing emitted by a freshly executed stage.
///
/// The scheduler qualifies `phase` with the producing stage id before exposing it
/// through the run report. Cache hits emit no internal timings because no stage body
/// ran; the enclosing stage timing still records cache hydration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRunTiming {
    /// Stable phase name local to the stage.
    pub phase: String,
    /// Observed wall-clock duration in milliseconds.
    pub elapsed_ms: u128,
    /// Optional stable work metadata; elapsed time itself is never deterministic.
    pub metadata: Option<String>,
}

impl StageRunTiming {
    /// Construct a phase timing without metadata.
    pub fn new(phase: impl Into<String>, elapsed_ms: u128) -> Self {
        Self {
            phase: phase.into(),
            elapsed_ms,
            metadata: None,
        }
    }
}

impl StageOutput {
    /// A stage output carrying `product` and NO diagnostic nodes — the default for
    /// every stage that emits no findings. Diagnostics producers build the
    /// struct literal directly, threading their forward `diags` in.
    pub fn new(product: StageProduct) -> Self {
        Self {
            product,
            diags: Vec::new(),
            timings: Vec::new(),
        }
    }
}

/// Whether a stage product should use the persistent structural cache.
///
/// This is a performance policy only: both variants execute the same stage body and
/// the scheduler applies the same attach-drift, provenance, and diagnostics gates.
/// [`Self::Recompute`] is for very large aggregate products whose canonical-byte
/// reparse and typed-handle reconstruction costs more than rebuilding them from their
/// already-live upstream products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    /// Read an unchanged product from the persistent cache, or compute and persist it
    /// on a miss.
    Persistent,
    /// Always compute the product and do not persist it.
    Recompute,
}

/// A pipeline stage: one node in the build DAG. The Rust impl is the executable
/// twin of a `gmeow:PipelineStage` individual; the loader binds them by
/// `gmeow:stageImpl` and HARD-fails if their `capabilities` / `consumes` /
/// `resources` disagree.
pub trait Stage: Send + Sync {
    /// The stable stage id — matches the `gmeow:PipelineStage` individual.
    fn id(&self) -> &str;
    /// The ids of the upstream stages this stage consumes, sorted.
    fn consumes(&self) -> &[String];
    /// The capability IRIs (`gmeow:hasCapability`) this stage holds, sorted. The
    /// executor reads these declarations in place of a kind enum: [`SINK_CAPABILITY`]
    /// marks the sole serialization exit (the gts narrow waist — the loader HARD-fails
    /// unless exactly one stage holds it), and [`SOURCE_ORIGIN`] marks the
    /// authored-source loader (its emitted quads' provenance origin is `Source`, every
    /// other stage's is `Generated`). The default is none — a stage holding no
    /// recognized capability is provenance-`Generated` and non-sink. The loader
    /// HARD-fails if this disagrees with the RDF `gmeow:hasCapability` declaration.
    fn capabilities(&self) -> &[String] {
        &[]
    }
    /// The IRIs of the shared resources this stage must hold exclusively while it
    /// runs (`gmeow:requiresResource`), sorted. Two stages declaring the same
    /// resource serialize; the default is none (parallel-eligible). The reasoning
    /// stage declares [`ENGINE_RESOURCE`]. The loader HARD-fails if this disagrees
    /// with the RDF `gmeow:requiresResource` declaration.
    fn resources(&self) -> &[String] {
        &[]
    }
    /// The persistent-cache policy for this stage's product. Most stages are cheap to
    /// hydrate and use [`CachePolicy::Persistent`]. A stage may opt into
    /// [`CachePolicy::Recompute`] when reconstructing its whole aggregate carrier is
    /// measurably slower than recomputing it from live upstream products.
    fn cache_policy(&self) -> CachePolicy {
        CachePolicy::Persistent
    }
    /// The typed dataflow (`gmeow:BuildDataFlow` reified edges): for each upstream producer
    /// the stage reads only SPECIFIC named-graph entities from, a
    /// `(producer_id, sorted entity-graph IRIs)` pair, the whole list sorted by
    /// producer id. A producer ABSENT here (the default for every producer) is a
    /// WHOLE-PRODUCT dependency — the cache key folds that producer's entire bundle
    /// digest, so any change re-runs this stage. A producer PRESENT narrows the
    /// dependency to those named graphs' content digests, so a change to a graph this
    /// stage does NOT read no longer re-runs it (artifact-level incremental rebuild).
    ///
    /// Narrowing is a CORRECTNESS ASSERTION: declare an entity set only when the stage
    /// provably reads nothing else from that producer's product — a too-small set would
    /// serve a stale build. The loader HARD-fails if this disagrees with the RDF
    /// `gmeow:BuildDataFlow` declaration (single source of truth).
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &[]
    }
    /// Whether this stage READS `stage-source-load`'s source-span table (via
    /// [`StageProduct::span_index`]). Overridden to `true` by the two diagnostics
    /// consumers (`stage-validate` / `stage-compile-logic`) ONLY. The scheduler folds the
    /// max topological level holding a span-table consumer into the drop-after-last-consumer
    /// point: after that level commits, the span-table blob is stripped from the source-load
    /// product, so every later stage that reaches for it HARD-fails and the shipped bundle
    /// never carries it. The default is `false` — a stage that does not read spans.
    fn consumes_span_table(&self) -> bool {
        false
    }
    /// The named-graph IRIs this stage ATTACHES to the carrier — the graphs
    /// present in its output product bundle but NOT in its effective input (its
    /// attach DELTA), sorted and deduplicated. For producers narrowed by
    /// [`Self::consumed_entities`], only the declared named graphs are effective
    /// inputs; a different graph merely carried by that upstream product does not
    /// conceal this stage's attachment. Mirrors the RDF `gmeow:attachesGraph`
    /// declarations on the stage individual; the loader HARD-fails if the two
    /// disagree (Rust/RDF agreement) and the scheduler HARD-fails
    /// ([`crate::error::AttachDrift`]) if the stage's actual attach delta at run
    /// time diverges from this set (in either direction). The default is empty — a
    /// stage that attaches no NEW named graph (e.g. a leaf that only rides the
    /// byte-artifact lane, or a projection whose graphs already ride an upstream).
    fn attaches_graphs(&self) -> &[String] {
        &[]
    }
    /// The blob-representation lane labels this stage ATTACHES to the carrier — the
    /// `representation`-keyed blob records (e.g.
    /// [`crate::bundle_blobs::REP_AXIOMS`],
    /// [`crate::bundle_blobs::REP_DIAG_NODES`]) whose `(representation, content
    /// digest)` identity is present in its output product but NOT in its assembled
    /// input (its attach DELTA), sorted and deduplicated. Distinct producers may
    /// therefore each attach different content under the same shared lane label.
    /// Mirrors the RDF `gmeow:attachesBlobRep` declarations; verified against the RDF
    /// at load (Rust/RDF agreement) and against the actual run-time delta by the
    /// scheduler ([`crate::error::AttachDrift`]). NOT the byte-artifact lane (logical
    /// paths) — only the representation-keyed by-reference blob lane. The default is
    /// empty.
    fn attaches_blob_reps(&self) -> &[String] {
        &[]
    }
    /// A version string folded into the cache key; bump to invalidate this
    /// stage's cached products when its logic changes.
    fn impl_version(&self) -> &str;
    /// The RAW source files this stage reads directly from disk that are NOT
    /// reflected in any upstream product digest (e.g. an export leaf that reads
    /// `metadata/references.ttl`, the eval corpus, or the slice manifests rather
    /// than consuming the composed fold). Their CONTENT is folded into the cache
    /// key so a source change busts the cache — the cache-soundness guarantee for
    /// non-`SourceLoad` stages that legitimately consume nothing.
    ///
    /// The default is empty: a stage whose every input is an upstream product
    /// (Merkle-composed) or whose file reads are already covered by a consumed
    /// `SourceLoad`/`stage-snapshot` product declares nothing here. Paths are
    /// resolved relative to the repo root; the scheduler reads each file's bytes
    /// and folds a content digest into the key (a missing file HARD-fails).
    fn input_files(&self, _root: &Path) -> gmeow_errors::Result<Vec<std::path::PathBuf>> {
        Ok(Vec::new())
    }
    /// Execute the stage over its upstream products.
    fn run(&self, input: StageInput<'_>) -> gmeow_errors::Result<StageOutput>;
}
