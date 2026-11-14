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
//! else is eligible on the completion-driven ready frontier. The reasoning stage requires
//! [`ENGINE_RESOURCE`] (the process-wide reasoning state is exclusive); carrier-scale
//! serializers and dictionary materialization require
//! [`SERIALIZATION_BUFFER_RESOURCE`] (their peak residency is exclusive).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

mod diagnostic_json;

use purrdf::provenance::DatasetProvenance;
use purrdf::{PipelineBundle, RdfDataset};

use crate::bundle::{
    PipelineHandle, bundle_artifact, bundle_artifact_refs, bundle_artifacts, bundle_from_artifacts,
    bundle_from_artifacts_over,
};
/// The GMEOW namespace prefix that every pipeline term lives under.
pub(crate) const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// Observe a producer phase without putting clock values in its product or key.
/// The work's return value, including an original typed refusal, is unchanged.
pub(crate) fn measure_stage_phase<T>(stage: &str, phase: &str, work: impl FnOnce() -> T) -> T {
    let span = ::tracing::info_span!(target: "pipeline_phase", "producer_phase", stage, phase);
    let _entered = span.enter();
    let started = std::time::Instant::now();
    tracing::info!(target: "pipeline_phase", stage, phase, event = "start", "producer phase boundary");
    let value = work();
    tracing::info!(
        target: "pipeline_phase", stage, phase, event = "end",
        elapsed_ms = %started.elapsed().as_millis(), "producer phase boundary"
    );
    value
}

/// The `gmeow:engineResource` IRI: the process-wide reasoning engine, declared as
/// an exclusive [`Stage::resources`] requirement by the sole `Reason` stage. The
/// scheduler serializes any two stages requiring the same resource (the
/// declarative replacement for a hardcoded engine mutex).
pub const ENGINE_RESOURCE: &str = "https://blackcatinformatics.ca/gmeow/engineResource";

/// The `gmeow:serializationBufferResource` IRI: the whole-dataset serialization
/// buffer.
///
/// Unlike [`ENGINE_RESOURCE`] this guards no shared mutable state — the carrier is a
/// read-only `Arc` every consumer shares. It is exclusive because of PEAK RESIDENCY:
/// a stage that declares it materializes a carrier-scale canonical or serialized
/// representation, so its transient footprint is a multiple of the dataset and two
/// such stages running concurrently add their peaks. The two whole-dataset export
/// leaves and the dictionary trainer's canonical selected-graph/training buffers are
/// the measured production holders. Declaring this resource makes them serialize
/// against each other while every cheap leaf keeps full parallelism — the same
/// declarative permit the reasoning engine uses, with no scheduler special case.
///
/// This is a permit, not a fix for the underlying shape: the exporters ask purrdf for a
/// whole-document `String`/`Vec<u8>`
/// (`purrdf::serialize_dataset`, `purrdf::native_codecs::jsonld::serialize_dataset_to_jsonld`
/// / `serialize_dataset_to_yamlld`). Removing the peak rather than scheduling around it
/// needs those codecs to grow an incremental `io::Write` sink so the document never
/// exists in memory at once — a purrdf API, never a second gmeow-side serializer.
pub const SERIALIZATION_BUFFER_RESOURCE: &str =
    "https://blackcatinformatics.ca/gmeow/serializationBufferResource";

/// The `gmeow:sinkCapability` IRI: the single narrow-waist serialization exit. Held
/// (via [`Stage::capabilities`] / RDF `gmeow:hasCapability`) by exactly ONE stage in
/// the DAG — the loader HARD-fails unless precisely one stage holds it.
pub const SINK_CAPABILITY: &str = "https://blackcatinformatics.ca/gmeow/sinkCapability";

/// The `gmeow:sourceOrigin` IRI: the authored-source loader. The stage that holds it
/// stamps its emitted quads with provenance origin `Source`; every other stage stamps
/// `Generated` (the kind-enum replacement — origin is read off a capability, not a tag).
pub const SOURCE_ORIGIN: &str = "https://blackcatinformatics.ca/gmeow/sourceOrigin";

/// Retain the native carrier refusal and require its exact last-run search evidence.
fn carrier_digest_failure(
    stage_id: &str,
    error: purrdf::PipelineBundleError,
    control: &purrdf::CanonicalizationControl<'_>,
) -> gmeow_errors::Diag {
    let detail = if let purrdf::PipelineBundleError::Canonicalization { source, .. } = &error
        && let purrdf::CanonError::BudgetExceeded(budget) = source.as_ref()
    {
        let observation = control.last_rdfc_refusal().filter(|(position, observation)| {
            observation.first_degree_group.is_some()
                && matches!(
                    control.since(*position),
                    Some([receipt])
                        if !receipt.completed
                            && receipt.observed_refusal == Some(purrdf::SearchRefusal::Limit)
                            && receipt.policy == purrdf::SearchPolicy::Finite(budget.limit)
                            && receipt.work == budget.work
                            && matches!(&receipt.configuration, purrdf::SearchConfiguration::Rdfc { .. })
                )
        });
        if let Some((_, observation)) = observation {
            format!(
                "{error}; recursive frames={}, permutation attempts={}; {observation}",
                budget.work.count(purrdf::SearchEvent::RdfcFrame),
                budget.work.count(purrdf::SearchEvent::RdfcPermutation),
            )
        } else {
            format!(
                "{error}; required last-run RDFC refusal observation is absent or does not match the native receipt"
            )
        }
    } else {
        error.to_string()
    };
    gmeow_errors::Diag::of_kind(crate::error::CarrierDigest::caused_by(
        format!("stage {stage_id} failed: cannot digest the stage carrier: {detail}"),
        error,
    ))
}

/// The product of one stage: its id, the hex content digest of the value it
/// produced (the cache-key contribution downstream stages fold in — Merkle
/// composition P2), and the structured [`PipelineBundle`] it emitted.
///
/// # The carrier (C4)
///
/// The carrier is an [`Arc<PipelineBundle<PipelineHandle>>`]: the frozen RDF
/// dataset + lookaside + content-addressed blob store + provenance + typed-handle
/// lane. The pre-C4 named byte artifacts (logical path → bytes) ride the bundle's
/// byte-artifact lane (see [`crate::bundle`]); `stage-snapshot` / `gts_sink` fold the
/// upstream lane into the one bundle (P3/P4). C2/C3/C5 progressively replace
/// the byte reads with dataset/lane reads and retire the lane per stage.
///
/// `digest` is the cache key: for a freshly produced bundle it is
/// the carrier content fold plus complete typed bindings, so `combined()`
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
    /// Crate-private. The accessors below record the access against the declared read
    /// set of the dataflow edge; they are how a stage is meant to reach a lane. Nothing
    /// enforces that inside the crate: every stage lives in it, so a stage body could
    /// read this field or [`Self::raw_bundle`] without a record. No production stage does today.
    pub(crate) bundle: Arc<PipelineBundle<PipelineHandle>>,
    /// Whether this product's CARRIER has been released (drop-after-last-consumer).
    ///
    /// `false` for every product a stage emits and for every product the cache serves.
    /// The scheduler sets it — and replaces `bundle` with
    /// [`release_carrier`](crate::bundle::release_carrier)'s residue (the committed
    /// byte-artifact lane only, plus the internal artifacts a consumer declares) — in two
    /// cases. Under
    /// [`CarrierRetention::DropAfterLastConsumer`](crate::scheduler::CarrierRetention),
    /// once the LAST stage whose dataflow edge from this one declares a carrier read has
    /// run (the edge reads declared in the pipeline slice, see [`crate::reads`]). Under
    /// the enforcing read policy, whatever the retention, when the product is handed to
    /// a consumer whose edge declares no carrier read; the recording policy hands every
    /// product over unreleased.
    /// `digest` is preserved verbatim across the release: it is the identity witness of
    /// the carrier that was released, so `combined()` is byte-identical either way.
    ///
    /// A released product's dataset / handles / blob records / internal `pipeline/`
    /// artifacts are GONE. Anything reaching for them is reaching past the declared
    /// dataflow and MUST hard-fail on this flag rather than read an empty carrier — see
    /// [`crate::stages::carrier::snapshot_dataset`].
    pub carrier_released: bool,
    /// Published typed identities survive payload release and avoid repeated lowering.
    handle_commitments: BTreeMap<String, crate::handle_identity::HandleCommitment>,
    /// The dataflow edge this view of the product was handed out under. `None` for the
    /// product a stage emits and for the scheduler's, the cache's and the reconcile's own
    /// view; `Some` for the view a consuming stage receives, which records each access made
    /// through the lane accessors (`bundle`, `handle`, `rep_blob`, `dataset`,
    /// `dataset_arc`, `artifact`, `artifacts`, `artifact_refs`, `span_index`), to be
    /// compared with the edge's declared read set when the stage returns.
    access: Option<Arc<crate::reads::EdgeAccess>>,
}

/// Encode one producer-owned diagnostic contribution, including an empty one.
/// Stage identity makes empty contributions distinct without synthetic events.
pub(crate) fn encode_diag_nodes(stage: &str, nodes: &[gmeow_errors::DiagNode]) -> Vec<u8> {
    purrdf_lex::json::record::to_vec(&(stage, nodes))
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
            carrier_released: false,
            handle_commitments: BTreeMap::new(),
            access: None,
        }
    }

    /// Construct a product from emitted named byte artifacts; they ride the
    /// bundle's byte-artifact lane and the digest is the bundle's content fold.
    ///
    /// # Errors
    /// As [`Self::from_bundle`].
    pub fn from_artifacts(
        stage_id: impl Into<String>,
        artifacts: BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, gmeow_errors::Diag> {
        let bundle = bundle_from_artifacts(artifacts, DatasetProvenance::new());
        Self::from_bundle(stage_id, Arc::new(bundle))
    }

    /// Construct a product from named byte artifacts riding over an explicit
    /// backing `dataset` (the lane travels alongside the frozen graph).
    ///
    /// # Errors
    /// As [`Self::from_bundle`].
    pub fn from_artifacts_over(
        stage_id: impl Into<String>,
        dataset: Arc<RdfDataset>,
        artifacts: BTreeMap<String, Vec<u8>>,
    ) -> Result<Self, gmeow_errors::Diag> {
        let bundle = bundle_from_artifacts_over(dataset, artifacts, DatasetProvenance::new());
        Self::from_bundle(stage_id, Arc::new(bundle))
    }

    /// Construct a product wrapping an already-assembled carrier; the digest is the
    /// bundle's content fold plus each complete typed handle binding.
    ///
    /// # Errors
    /// [`CarrierDigest`](crate::error::CarrierDigest) naming the stage and retaining
    /// the native cause when purrdf refuses to canonicalize the carrier: the product
    /// would have no content identity to cache or publish.
    pub fn from_bundle(
        stage_id: impl Into<String>,
        bundle: Arc<PipelineBundle<PipelineHandle>>,
    ) -> Result<Self, gmeow_errors::Diag> {
        let stage_id = stage_id.into();
        let handle_commitments =
            measure_stage_phase(&stage_id, "capture-handle-commitments", || {
                crate::handle_identity::handle_commitments(&bundle)
            });
        let mut control = purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None);
        let digest = measure_stage_phase(&stage_id, "digest-stage-product", || {
            crate::handle_identity::product_digest(&bundle, &handle_commitments, &mut control)
        })
        .map_err(|error| carrier_digest_failure(&stage_id, error, &control))?;
        Ok(Self {
            stage_id,
            digest,
            bundle,
            carrier_released: false,
            handle_commitments,
            access: None,
        })
    }

    /// This product with its carrier RELEASED: the frozen dataset, typed handles,
    /// blob records, provenance, and internal `pipeline/` byte artifacts are dropped;
    /// only the COMMITTED byte-artifact lane survives, for the post-run reconcile.
    ///
    /// `stage_id` and `digest` are preserved VERBATIM — the digest is the identity
    /// witness of the released carrier, never a fold over the residue — so the run's
    /// `combined_digest` is byte-identical whether or not the release happened. The
    /// scheduler calls this at the stage's drop-after-last-consumer point; releasing an
    /// already-released product is idempotent.
    ///
    /// # Errors
    /// Propagates [`release_carrier`](crate::bundle::release_carrier)'s hard failure on
    /// a corrupt byte-artifact lane.
    pub fn into_carrier_released(self) -> Result<Self, gmeow_errors::Diag> {
        self.into_carrier_released_keeping(&std::collections::BTreeSet::new())
    }

    /// [`Self::into_carrier_released`], keeping the internal artifacts named in
    /// `kept_internal` that a consumer declares it reads
    /// ([`Stage::internal_artifact_reads`]).
    ///
    /// # Errors
    /// As [`Self::into_carrier_released`].
    pub fn into_carrier_released_keeping(
        self,
        kept_internal: &std::collections::BTreeSet<&str>,
    ) -> Result<Self, gmeow_errors::Diag> {
        let bundle = crate::bundle::release_carrier(&self.bundle, kept_internal)?;
        Ok(Self {
            stage_id: self.stage_id,
            digest: self.digest,
            bundle: Arc::new(bundle),
            carrier_released: true,
            handle_commitments: self.handle_commitments,
            access: self.access,
        })
    }

    /// Immutable typed commitments from publication, shared with action receipts.
    pub(crate) fn handle_commitments(
        &self,
    ) -> &BTreeMap<String, crate::handle_identity::HandleCommitment> {
        &self.handle_commitments
    }

    /// Release a declared payload after its last consumer while preserving the
    /// published product and entity identities used by immutable receipts.
    pub(crate) fn with_released_bundle(&self, bundle: PipelineBundle<PipelineHandle>) -> Self {
        Self {
            bundle: Arc::new(bundle),
            ..self.clone()
        }
    }

    /// This product as `consumer` may read it: the same carrier, recording each access made through
    /// the lane accessors against `access`, the declared read set of the dataflow edge.
    #[must_use]
    pub fn consumer_view(&self, access: Arc<crate::reads::EdgeAccess>) -> Self {
        Self {
            access: Some(access),
            ..self.clone()
        }
    }

    /// Borrow the carrier without recording a read: for the scheduler, the cache and the
    /// post-run reconcile, which handle a product without being its consumer.
    pub(crate) fn raw_bundle(&self) -> &Arc<PipelineBundle<PipelineHandle>> {
        &self.bundle
    }

    /// The dataflow edge this view was handed out under, if it is a consumer view.
    pub(crate) fn edge_access(&self) -> Option<&Arc<crate::reads::EdgeAccess>> {
        self.access.as_ref()
    }

    /// Record one lane access on a consumer view. The owning view records nothing.
    fn record(&self, access: crate::reads::Read) {
        if let Some(edge) = &self.access {
            edge.record(access);
        }
    }

    /// Borrow the whole structured carrier this product emitted. On a consumer view
    /// this is a read of every carrier lane; prefer the lane accessors.
    pub fn bundle(&self) -> &Arc<PipelineBundle<PipelineHandle>> {
        self.record(crate::reads::Read::Carrier);
        &self.bundle
    }

    /// Borrow one typed handle of this product by its key.
    pub fn handle(&self, key: &str) -> Option<&purrdf::HandleEntry<PipelineHandle>> {
        self.record(crate::reads::Read::Handle(key.to_owned()));
        self.bundle.handle(key)
    }

    /// Borrow one representation blob of this product by its lane label.
    pub fn rep_blob(&self, representation: &str) -> Option<&[u8]> {
        self.record(crate::reads::Read::BlobRep(representation.to_owned()));
        crate::bundle::bundle_rep_blob(&self.bundle, representation)
    }

    /// Borrow the frozen RDF dataset this product's bundle carries.
    ///
    /// # Panics
    /// When the carrier has been released. The read is recorded BEFORE the assertion, so
    /// the scheduler, which catches the stage's panic, reports it as an undeclared read of
    /// this producer with the declaration to add.
    pub fn dataset(&self) -> &RdfDataset {
        self.record(crate::reads::Read::Dataset);
        assert!(
            !self.carrier_released,
            "stage `{}` attempted to read a dataset after its carrier was released; its \
             dataflow edge must declare the dataset read",
            self.stage_id
        );
        self.bundle.dataset()
    }

    /// A shared handle to the frozen RDF dataset this product's bundle carries.
    ///
    /// # Panics
    /// As [`Self::dataset`].
    pub fn dataset_arc(&self) -> Arc<RdfDataset> {
        self.record(crate::reads::Read::Dataset);
        assert!(
            !self.carrier_released,
            "stage `{}` attempted to read a dataset after its carrier was released; its \
             dataflow edge must declare the dataset read",
            self.stage_id
        );
        self.bundle.dataset_arc()
    }

    /// The bytes of one named byte-artifact-lane entry by logical path.
    pub fn artifact(&self, logical_path: &str) -> Option<&[u8]> {
        self.record(crate::reads::Read::Artifact(logical_path.to_owned()));
        bundle_artifact(&self.bundle, logical_path)
    }

    /// The full `(logical_path → bytes)` map of this product's byte-artifact lane,
    /// sorted by path. The surface `run_full` writes / compares against committed.
    ///
    /// On a consumer view this is the committed artifacts plus the internal `pipeline/`
    /// artifacts the edge names. An internal artifact this edge does not name is not
    /// returned for this consumer, and it is kept through the release only when some
    /// other consumer in the run names it. Each internal
    /// artifact returned is recorded as a read of that artifact.
    pub fn artifacts(&self) -> BTreeMap<String, Vec<u8>> {
        self.record(crate::reads::Read::AllArtifacts);
        let mut lane = bundle_artifacts(&self.bundle);
        lane.retain(|path, _| self.lane_visible(path));
        lane
    }

    /// Whether the whole-lane accessors return `path` on this view. The owning view sees
    /// every artifact. A consumer view sees the committed ones, and an internal one only
    /// when its edge names it, in which case the read of that exact artifact is recorded.
    fn lane_visible(&self, path: &str) -> bool {
        let Some(edge) = &self.access else {
            return true;
        };
        if !crate::reads::is_internal_artifact(path) {
            return true;
        }
        let named = crate::reads::Read::Artifact(path.to_owned());
        if edge.declared().contains(&named) {
            edge.record(named);
            true
        } else {
            false
        }
    }

    /// Borrow every byte-artifact entry without cloning its payload.
    ///
    /// Selective consumers should filter this view before copying anything. The
    /// owned [`Self::artifacts`] projection remains the reconcile/write boundary. A
    /// consumer view returns the same lane as [`Self::artifacts`]: committed artifacts and
    /// the internal ones its edge names.
    ///
    /// # Errors
    /// The carrier's byte-artifact lane is structurally corrupt.
    pub fn artifact_refs(&self) -> Result<BTreeMap<&str, &[u8]>, gmeow_errors::Diag> {
        self.record(crate::reads::Read::AllArtifacts);
        let mut lane = bundle_artifact_refs(&self.bundle)?;
        lane.retain(|path, _| self.lane_visible(path));
        Ok(lane)
    }

    /// The FORWARD-projected diagnostics nodes this product carries on its
    /// `diagnostics:nodes` blob lane ([`crate::stages::carrier::REP_DIAG_NODES`]), or an
    /// EMPTY vec when the product carries none (every non-producer stage). This is the
    /// lane the scheduler reads on a CACHE HIT to recover a diagnostics producer's run
    /// ledger contribution WITHOUT re-running the stage — the blob round-trips through
    /// the per-stage cache, so the recovered nodes are byte-identical to the fresh run.
    /// A declared producer must supply its blob, including an explicit empty contribution.
    /// A malformed or wrong-producer blob is a corrupt product — a HARD FAIL.
    pub fn diag_nodes(&self) -> Vec<gmeow_errors::DiagNode> {
        match crate::bundle::bundle_rep_blob(&self.bundle, crate::stages::carrier::REP_DIAG_NODES) {
            Some(bytes) => {
                let (stage, nodes) = diagnostic_json::decode_contribution(bytes).expect(
                    "diagnostics:nodes blob is our own JSON; a decode failure is a corrupt cache",
                );
                assert_eq!(
                    stage, self.stage_id,
                    "diagnostic contribution has the wrong producer"
                );
                nodes
            }
            None => {
                assert!(
                    !crate::stages::attach::blob_reps(&self.stage_id)
                        .iter()
                        .any(|rep| rep == crate::stages::carrier::REP_DIAG_NODES),
                    "stage {} is missing its required diagnostics:nodes contribution",
                    self.stage_id
                );
                Vec::new()
            }
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
        self.record(crate::reads::Read::BlobRep(
            crate::stages::carrier::REP_SPAN_TABLE.to_owned(),
        ));
        match crate::bundle::bundle_rep_blob(&self.bundle, crate::stages::carrier::REP_SPAN_TABLE) {
            Some(bytes) => purrdf_lex::json::record::from_slice(bytes).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Decode::new(format!(
                    "source-span table blob JSON: {e}"
                )))
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
    /// `stage-reason`, and `stage-verify-attestation`) populate it from their report.
    /// The scheduler folds
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

/// Whether a stage's output is stable for identical declared inputs.
///
/// This is the executable twin of RDF `gmeow:stageStability`. The loader requires
/// exact agreement before scheduling, so a receipt never claims a stronger stability
/// class than the canonical DAG declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStability {
    /// Identical declared inputs produce byte-identical output across runs.
    StablePrefix,
    /// The output intentionally varies between otherwise identical runs.
    PerTurnVariance,
}

impl StageStability {
    /// Parse one closed-vocabulary RDF individual.
    #[must_use]
    pub fn from_iri(value: &str) -> Option<Self> {
        match value {
            "https://blackcatinformatics.ca/gmeow/stabilityStablePrefix" => {
                Some(Self::StablePrefix)
            }
            "https://blackcatinformatics.ca/gmeow/stabilityPerTurnVariance" => {
                Some(Self::PerTurnVariance)
            }
            _ => None,
        }
    }

    /// The canonical RDF individual IRI serialized into immutable receipts.
    #[must_use]
    pub const fn iri(self) -> &'static str {
        match self {
            Self::StablePrefix => "https://blackcatinformatics.ca/gmeow/stabilityStablePrefix",
            Self::PerTurnVariance => {
                "https://blackcatinformatics.ca/gmeow/stabilityPerTurnVariance"
            }
        }
    }
}

/// The output family of a stage: which output scopes need what it produces.
///
/// Every stage declares one; the trait method has no default, so a stage added without a
/// declaration does not compile. A run selects its stages from these declarations
/// ([`crate::run::spec_for_scope`]): the committed scope runs the stages whose outputs it
/// selects and their transitive producers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputFamily {
    /// Contributes to what every scope selects: the bundle, the committed `generated/`
    /// tree, the codebook and whatever those need.
    Generated,
    /// Produces only gitignored `dist/` runtime projections, which only the scope that
    /// materializes them (`all`) selects, and that no `Generated` stage consumes.
    RuntimeProjection,
}

/// Whether a stage product should use the persistent structural cache.
///
/// This is the executable twin of RDF `gmeow:stageCacheDisposition`: both variants
/// execute the same stage body and the scheduler applies the same attach-drift,
/// provenance, and diagnostics gates. The loader requires exact RDF/Rust agreement.
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

impl CachePolicy {
    /// Parse one closed-vocabulary RDF individual.
    #[must_use]
    pub fn from_iri(value: &str) -> Option<Self> {
        match value {
            "https://blackcatinformatics.ca/gmeow/cachePersistentContribution" => {
                Some(Self::Persistent)
            }
            "https://blackcatinformatics.ca/gmeow/cacheRecomputeAggregate" => Some(Self::Recompute),
            _ => None,
        }
    }

    /// The canonical RDF individual IRI serialized into immutable receipts.
    #[must_use]
    pub const fn iri(self) -> &'static str {
        match self {
            Self::Persistent => "https://blackcatinformatics.ca/gmeow/cachePersistentContribution",
            Self::Recompute => "https://blackcatinformatics.ca/gmeow/cacheRecomputeAggregate",
        }
    }
}

/// A pipeline stage: one node in the build DAG. The Rust impl is the executable
/// twin of a `gmeow:PipelineStage` individual; the loader binds them by
/// `gmeow:stageImpl` and HARD-fails if their `capabilities` / `consumes` /
/// `resources` disagree.
pub trait Stage: Send + Sync {
    /// The stable stage id — matches the `gmeow:PipelineStage` individual. The loader
    /// refuses to bind an implementation to a stage whose id differs from it.
    fn id(&self) -> &str;
    /// The output family this stage belongs to. Required: there is no default family.
    fn output_family(&self) -> OutputFamily;
    /// The ids of the upstream stages this stage consumes, sorted.
    fn consumes(&self) -> &[String];
    /// The subset of [`Self::consumes`] whose in-memory carrier lanes this stage reads.
    ///
    /// Every `consumes()` edge remains the RDF-authored scheduling, cache-key, and
    /// artifact-access authority. This executable transport-use refinement says which
    /// of those already-declared products must still retain its dataset, typed handles,
    /// representation blobs, provenance, or internal `pipeline/` artifacts when this
    /// stage runs. An edge absent here is artifact-only: the committed logical-artifact
    /// lane survives carrier release and remains available through the same dependency.
    ///
    /// The default is every dependency, which is what a hand-written stage (a test fake)
    /// gets. The stages of the production pipeline are run as the loader's bound stage,
    /// which overrides this with the producers whose dataflow edge declares a carrier
    /// read (a carrier lane, a handle, a blob representation, or a named-graph
    /// narrowing). An implementation's own answer is ignored there, and nothing compares
    /// the two. A dataset read of a released carrier panics at the read, through
    /// [`StageProduct::carrier_released`]; a handle or blob read returns nothing.
    fn carrier_consumes(&self) -> &[String] {
        self.consumes()
    }
    /// The internal `pipeline/` artifacts this stage reads, as `(producer, artifact)`
    /// pairs, whether or not the dependency's carrier is released for it.
    ///
    /// Releasing a carrier drops a product's internal artifacts. The default is none,
    /// which is what a hand-written stage gets. The bound stage of the production
    /// pipeline overrides this with every `gmeow:flowReadsArtifact` the pipeline slice
    /// declares under `pipeline/`; an implementation's own answer is ignored there. The
    /// scheduler keeps exactly those bytes through every release of that producer until
    /// the run ends, whether or not the declaring consumer has run.
    fn internal_artifact_reads(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    /// The reads this stage's dataflow edges declare, by producer. A stage bound from
    /// the pipeline description answers with the description's declarations; the default
    /// declares nothing, so any lane access is undeclared.
    fn reads(&self) -> BTreeMap<String, crate::reads::ReadSet> {
        BTreeMap::new()
    }
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
    /// stage declares [`ENGINE_RESOURCE`]; carrier-scale materializers declare
    /// [`SERIALIZATION_BUFFER_RESOURCE`]. The loader HARD-fails if this
    /// disagrees with the RDF `gmeow:requiresResource` declaration.
    fn resources(&self) -> &[String] {
        &[]
    }
    /// The output-stability declaration mirrored by RDF `gmeow:stageStability`.
    /// Deterministic repository stages use the stable-prefix class by default; a
    /// stage with intentional per-run variance must opt in explicitly.
    fn stability(&self) -> StageStability {
        StageStability::StablePrefix
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
    /// [`StageProduct::span_index`]). The bound stage of the production pipeline answers
    /// `true` exactly when its dataflow edge from `stage-source-load` PERMITS reading the
    /// `spans:source-table` blob representation, by naming it or by declaring the whole
    /// carrier; the default, which a
    /// hand-written stage gets, is `false`. The scheduler folds the
    /// exact remaining-consumer count into the drop-after-last-consumer point: after the
    /// final declared reader commits, the span-table blob is stripped from the source-load
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
