// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The full-build entry point: [`run_full`] runs the WHOLE
//! dogfooded DAG single-pass and either WRITES every produced artifact to disk
//! (update mode) or COMPARES each against the committed bytes and collects
//! drift [`Finding`]s (check mode).
//!
//! # The single-pass property
//!
//! The fold-reading export leaves (lpg, metadata, export, okf) consume
//! the in-memory `stage-snapshot` product — THIS run's freshly-composed
//! `gmeow.gts` — rather than re-reading the committed file from disk. The sole
//! [`crate::stages::gts_sink::GtsSinkStage`] re-emits those snapshot bytes as its
//! product; `run_full` writes them to `generated/dist/gmeow.gts`.
//!
//! # The schemas sink-product tail
//!
//! The native `schemas` leaf consumes the `stage-gts-sink` product because the
//! generated schema surfaces are projections of the exact folded GTS bytes that
//! are shipped. The sink product and schemas tail execute in one scheduled DAG;
//! schemas read those bytes from their in-memory upstream product, with no Python
//! subprocess and no disk-read dependency.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gmeow_cli_core::Reporter;
use gmeow_errors::{
    Diag, DiagLedger, Finding, FindingCategory, GateVerdict, Grade, Severity, StageId, Standpoint,
    register_code,
};
use gmeow_logic::dag_profile::certify_acyclic;
use gmeow_logic::result::ReasoningResult;

use crate::cache::StageReceipt;
use crate::loader::{PipelineSpec, bind};
use crate::node::{SINK_CAPABILITY, StageProduct};
use crate::registry::default_registry;
use crate::scheduler::{
    CarrierRetention, LevelTiming, RunContext, StagePhaseTiming, StageTiming, run,
};

/// The stage id every drift/superset diagnostic is attributed to on the carrier
/// ledger (pin-on-attach).
const PIPELINE_STAGE_ID: &str = "stage-pipeline-reconcile";

// The registered finding codes the reconcile phase emits. Declared here as the
// enumeration authority (the same discipline as `validate::codes`).
const CODE_SUPERSET_MISSING: &str = "pipeline.superset.missing";
const CODE_SUPERSET_MISMATCH: &str = "pipeline.superset.mismatch";
const CODE_SUPERSET_ORPHAN: &str = "pipeline.superset.orphan";
const CODE_MISSING: &str = "pipeline.missing";
const CODE_DRIFT: &str = "pipeline.drift";
/// The GMN-1 construct-coverage-completeness audit's finding code: a codec
/// construct category the real grounding corpus never exercises — distinct from a GMN-1
/// round-trip failure (a quad that failed to round-trip, now interned with typed
/// `lang:`-class DiagLedger identity via `gmeow_lang_bridge::error::attach_gmn_failure`) —
/// this fires on a category with ZERO real occurrences, the completeness gap the round-trip
/// gate alone cannot see.
const CODE_GMN1_CONSTRUCT_COVERAGE_GAP: &str = "pipeline.gmn1.construct-coverage-gap";

/// The native codebook-digest gate's finding code: an envelope declares a
/// `gmeow:gmnCodebookDigest` that does not equal the codebook's recomputed Merkle root
/// (`lang:GmnCodebookDigestMismatch`) — a graph-level cross-node check, distinct from a
/// per-record codec failure.
const CODE_GMN1_CODEBOOK_DIGEST_MISMATCH: &str = "pipeline.gmn1.codebook-digest-mismatch";

/// The native pack-root check's finding code: a shipped conformance pack declares a
/// `gmeow:gmnPackRoot` that does not equal the recomputation over its three parts (the
/// codebook digest, the authored grammar bytes, and the sigil table).
const CODE_GMN1_PACK_ROOT_MISMATCH: &str = "pipeline.gmn1.pack-root-mismatch";

/// Intern a reconcile-phase drift/superset finding into the carrier ledger. The
/// drifting path is the finding's focus, so each path is a distinct content-addressed
/// witness (a shared code alone would hash-cons every drift into one node). All are
/// gate-failing modelling defects (Error / ModelingDisciplineViolation / Binding).
fn attach_pipeline_finding(ledger: &mut DiagLedger, code: &str, focus: &str, message: String) {
    let diag = Diag::new(
        register_code(code),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    )
    .with_focus(focus);
    ledger.attach(diag, StageId::new(PIPELINE_STAGE_ID));
}

/// The file-level result of reconciling one canonical abstract projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanonicalTargetOutcome {
    /// The target is reconciled; `written` distinguishes a rewrite from a fixed point.
    Reproduced { written: bool },
    /// Check mode observed bytes that differ from the canonical field projection.
    Drifted,
}

/// Reconcile one canonical projection without weakening check mode into a write.
fn reconcile_canonical_target(
    root: &Path,
    mode: RunMode,
    target: &crate::canonical_abstract::ProjectedAbstract,
) -> Result<CanonicalTargetOutcome, gmeow_errors::Diag> {
    if mode == RunMode::Update {
        return Ok(CanonicalTargetOutcome::Reproduced {
            written: write_artifact(root, target.path, &target.bytes)?,
        });
    }
    if std::fs::read(root.join(target.path)).is_ok_and(|bytes| bytes == target.bytes) {
        Ok(CanonicalTargetOutcome::Reproduced { written: false })
    } else {
        Ok(CanonicalTargetOutcome::Drifted)
    }
}

/// Attach one visible ledger diagnostic for every canonical projection drift.
fn attach_canonical_drift_findings(ledger: &mut DiagLedger, drifted: &[String]) {
    for path in drifted {
        attach_pipeline_finding(
            ledger,
            CODE_DRIFT,
            path,
            format!("{path} differs from the canonical abstract projection"),
        );
    }
}

/// The sole serialization exit; its product carries the `gmeow.gts` bytes.
pub(crate) const SINK_STAGE: &str = "stage-gts-sink";
/// The committed fold path the sink writes / schemas project / fanout reads.
pub(crate) const GTS_PATH: &str = "generated/dist/gmeow.gts";
/// The DAG-workflow contract identity the build plan executes under — the
/// `gmeow:pipeline-build` `logic:Plan` is certified under `logic:DagWorkflowResource`.
const BUILD_DAG_CONTRACT: &str = "contract:gmeow:pipeline-build:dag-workflow";
/// The world the build plan's certification verdict holds in.
const BUILD_DAG_WORLD: &str = "urn:gmeow:pipeline-build";

/// Whether `run_full` reconciles artifacts to disk (update) or compares them to
/// the committed bytes and reports drift (check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Reconcile every produced artifact, then run the same strict post-build gates.
    Update,
    /// Compare every produced artifact to the committed bytes, collecting drift.
    Check,
}

/// Which output scope a run selects.
///
/// The scope selects the DAG branch: [`spec_for_scope`] derives the stage set from each
/// stage's declared [`crate::node::OutputFamily`]. [`All`](Self::All) runs every stage and
/// writes the gitignored `dist/*` runtime projections. [`Committed`](Self::Committed) runs
/// the stages whose outputs the committed scope selects (the bundle, the `generated/`
/// tree and the codebook) and their transitive producers, so the runtime-only exporters
/// do not execute. Every selected stage and every gate the selected stages feed stays
/// mandatory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutputScope {
    /// Materialize committed outputs and every gitignored runtime projection.
    All,
    /// Materialize only committed/generated outputs.
    Committed,
}

/// Which kind of build a run is: an ordinary build or a release.
///
/// A release is selected EXPLICITLY by its caller (`gmeow-dev sync --profile release`,
/// which the `release` and `full-release` Make targets pass), never inferred from an
/// output scope, a mode, or the environment. It changes one behaviour today: a
/// release refuses committed dictionary sweep evidence measured over a corpus other
/// than the one the build resolved ([`crate::medium::sweep::grade_dictionary`]), where
/// an ordinary build trains at the declared point and records the stale state on the
/// shipped realization. The profile reaches each stage it affects through that
/// stage's constructor and is folded into its cache key, so a product built under one
/// profile is never reused by the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    /// An ordinary development or gate build.
    Ordinary,
    /// A release build.
    Release,
}

impl BuildProfile {
    /// The stable token naming this profile on the command line and in run manifests.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::Release => "release",
        }
    }
}

/// The outcome of a [`run_full`]: how many artifacts were produced / reproduced
/// and any drift findings (check mode) or write errors.
#[derive(Debug, Clone)]
pub struct RunReport {
    /// The run mode.
    pub mode: RunMode,
    /// Total logical output paths the run produced.
    pub produced: usize,
    /// Paths that reproduced byte-for-byte (check) / reconciled cleanly (update).
    pub reproduced: usize,
    /// Artifacts rewritten in update mode because bytes changed or the file was missing.
    pub written: usize,
    /// Artifacts left untouched in update mode because committed bytes already matched.
    pub skipped_writes: usize,
    /// Stale projection-owned files removed while reconciling output trees.
    pub removed: usize,
    /// The carrier-borne diagnostic ledger: the hash-consed witness DAG every
    /// drift/superset finding is interned into (pin-on-attach, stage-attributed,
    /// deterministically ordered). A first-class member of the run's carrier
    /// output — [`findings`](RunReport::findings) is its lossy projection.
    pub ledger: DiagLedger,
    /// The drifted logical paths (check mode), sorted.
    pub drifted: Vec<String>,
    /// Per-phase timing records for profiling the gate without parsing stderr.
    pub timings: Vec<TimingRecord>,
    /// Deterministic root of the topologically ordered stage receipts. Cache outcomes
    /// and timings are excluded, so cold and warm executions share this identity.
    pub stage_receipt_root: String,
    /// Structured per-stage execution/cache observations. These are run telemetry,
    /// deliberately separate from deterministic receipts.
    pub stage_timings: Vec<StageTiming>,
    /// Internal phase observations emitted by freshly executed stages. Stable work
    /// metadata is carried separately from the observed elapsed duration.
    pub stage_phase_timings: Vec<StagePhaseTiming>,
    /// Structured scheduler critical levels used to compute the DAG critical-path floor.
    pub level_timings: Vec<LevelTiming>,
    /// Immutable deterministic receipts for every executed DAG stage. These carry the
    /// exact action context and structural output census but no wall-clock observation.
    pub stage_receipts: Vec<StageReceipt>,
    /// Every non-internal logical output path produced by the run, sorted and
    /// deduplicated. Callers use this to build whole-run output manifests without
    /// rediscovering or re-running the pipeline.
    pub output_paths: Vec<String>,
    /// The build plan's DAG-workflow certification, lowered to the typed
    /// [`ReasoningResult`] a consumer reads — the Rust-struct counterpart of the
    /// RDF `logic:ReasoningResult` `teleology::emit_dag_certification` emits, both
    /// computed from the SAME [`certify_acyclic`] verdict. For the always-acyclic
    /// `gmeow:pipeline-build` plan this is a `Completed` / `CompleteForFragment`
    /// (certified) result; a non-certified verdict at this point HARD-fails the run
    /// (no silent default) — see [`run_full`].
    pub certification: ReasoningResult,
}

impl RunReport {
    /// Every stage and reconciliation finding, projected from the current ledger.
    /// Informational, advisory and coherent findings remain visible on successful
    /// runs; acceptance uses the ledger's typed gate verdict, never this exit view.
    pub fn findings(&self) -> impl Iterator<Item = Finding> + '_ {
        self.ledger.iter_findings("gmeow-pipeline")
    }

    /// Whether every artifact is reconciled and the carrier ledger permits the run.
    /// Collected diagnostics are retained; any fatal witness or drift refuses reuse.
    pub fn is_clean(&self) -> bool {
        self.ledger.verdict() == GateVerdict::Collected && self.drifted.is_empty()
    }

    /// Record only the exact receipts of a successful materializing run.
    /// This candidate cannot replace the fixture producer's finalized runner selector.
    fn record_fixture_candidate(&mut self, root: &Path) -> gmeow_errors::Result<()> {
        if self.mode != RunMode::Update || !self.is_clean() {
            return Ok(());
        }
        let started = Instant::now();
        crate::fixture::record_stage_fixture_candidate(root, &self.stage_receipts)?;
        self.timings.push(TimingRecord {
            phase: "fixture-receipt-candidate".to_string(),
            elapsed_ms: started.elapsed().as_millis(),
            metadata: Some(format!(
                "stages={};receipt-root={}",
                crate::fixture::AUTHENTICATED_TEST_STAGE_IDS.len(),
                self.stage_receipt_root
            )),
        });
        Ok(())
    }
}

/// One wall-clock timing row emitted by the pipeline runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimingRecord {
    /// Phase label, e.g. `stage:stage-reason`, `pipeline-reconcile`, or `superset`.
    pub phase: String,
    /// Elapsed wall-clock in milliseconds.
    pub elapsed_ms: u128,
    /// Optional stable metadata string for deterministic JSON output.
    pub metadata: Option<String>,
}

/// The authored description of the build pipeline: the text of the pipeline slice the
/// producer was compiled from.
const PIPELINE_DESCRIPTION: &str = include_str!("../../../slices/core/pipeline/module.ttl");

/// The pipeline the producer executes, read from the authored pipeline slice.
///
/// `slices/core/pipeline/module.ttl` is the one description of the build graph: its
/// stages, their dependency edges and what each edge reads, their capabilities,
/// resources, cache dispositions and attach sets. What each edge reads also decides
/// retention: which products keep their carrier for a consumer, which internal artifacts
/// survive a release, and which stage reads the source-span table. Those three are
/// stated nowhere in Rust.
///
/// Every stage implementation still declares its own id, consumes, capabilities,
/// resources, stability, cache disposition, entity narrowing and attach sets, and
/// [`crate::bind`] refuses a description that disagrees with the implementation it
/// names. The description is compiled in, so the producer and the description it
/// executes cannot differ, and the producer's own authentication covers the file.
///
/// # Panics
/// When the compiled-in description does not parse as a pipeline. That is a defect of
/// the authored slice, and no run can proceed without a pipeline.
#[must_use]
pub fn full_spec() -> PipelineSpec {
    static SPEC: std::sync::OnceLock<PipelineSpec> = std::sync::OnceLock::new();
    SPEC.get_or_init(|| {
        PipelineSpec::from_turtle(&[PIPELINE_DESCRIPTION]).unwrap_or_else(|error| {
            panic!("slices/core/pipeline/module.ttl does not describe a pipeline: {error}")
        })
    })
    .clone()
}

/// Select the stages one output scope runs, from each stage's declared
/// [`crate::node::OutputFamily`].
///
/// [`RunOutputScope::All`] keeps every stage. [`RunOutputScope::Committed`] keeps the
/// stages of the `Generated` family plus all of their transitive producers, so a
/// runtime-projection stage that a selected stage consumes still runs. A stage whose
/// implementation is not registered is a hard error: a family cannot be read for it.
pub fn spec_for_scope(
    spec: PipelineSpec,
    scope: RunOutputScope,
) -> Result<PipelineSpec, gmeow_errors::Diag> {
    if scope == RunOutputScope::All {
        return Ok(spec);
    }
    let registry = default_registry();
    let mut selected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pending: Vec<String> = Vec::new();
    for stage in &spec.stages {
        let implementation = registry.get(&stage.impl_key).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
                message: format!(
                    "stage {} binds unregistered implementation {}; its output family is unknown",
                    stage.id, stage.impl_key
                ),
            })
        })?;
        if implementation.output_family() == crate::node::OutputFamily::Generated {
            pending.push(stage.id.clone());
        }
    }
    while let Some(id) = pending.pop() {
        if !selected.insert(id.clone()) {
            continue;
        }
        if let Some(stage) = spec.stage(&id) {
            pending.extend(stage.consumes.iter().cloned());
        }
    }
    Ok(PipelineSpec {
        id: spec.id,
        stages: spec
            .stages
            .into_iter()
            .filter(|stage| selected.contains(&stage.id))
            .collect(),
    })
}

/// Run the FULL dogfooded build single-pass and either write every produced
/// artifact (update) or compare it to the committed bytes (check).
///
/// `jobs` is the completion-ready frontier's parallelism budget. Returns a [`RunReport`]; in check
/// mode `report.is_clean()` is the cutover gate (zero drift across every
/// committed artifact). RDF artifacts compare by bytes (they are byte
/// deterministic); the `gmeow.gts` bundle is compared by the production superset
/// fold because CBOR has encoding skew.
pub fn run_full(
    root: &Path,
    jobs: usize,
    mode: RunMode,
    profile: BuildProfile,
) -> Result<RunReport, gmeow_errors::Diag> {
    run_full_scoped(root, jobs, mode, RunOutputScope::All, profile)
}

/// Run one build under the explicitly selected [`BuildProfile`] for the selected output
/// scope. The scope selects the stage set ([`spec_for_scope`]); every selected stage and
/// its gates are mandatory. The profile has no default: a caller that ships its output
/// must name [`BuildProfile::Release`].
pub fn run_full_scoped(
    root: &Path,
    jobs: usize,
    mode: RunMode,
    output_scope: RunOutputScope,
    profile: BuildProfile,
) -> Result<RunReport, gmeow_errors::Diag> {
    run_full_scoped_with_progress(root, jobs, mode, output_scope, profile, None)
}

/// Run the scoped pipeline under an explicitly selected [`BuildProfile`], with an
/// optional live progress sink.
pub fn run_full_scoped_with_progress(
    root: &Path,
    jobs: usize,
    mode: RunMode,
    output_scope: RunOutputScope,
    profile: BuildProfile,
    progress: Option<Arc<dyn Reporter>>,
) -> Result<RunReport, gmeow_errors::Diag> {
    let total_started = Instant::now();

    // The abstract fields are producer-owned inputs to `source_load`, so this
    // field-level projection must reach its fixed point before the DAG reads the
    // ontology/self-description documents. Check mode compares without writing.
    let canonical_started = Instant::now();
    if let Some(progress) = progress.as_ref() {
        progress.stage_start("pipeline:canonical-abstract");
    }
    let canonical_targets = crate::canonical_abstract::render_targets(root)?;
    let mut canonical_drifted = Vec::new();
    let mut canonical_reproduced = 0usize;
    let mut canonical_written = 0usize;
    let mut canonical_skipped_writes = 0usize;
    let canonical_output_paths: Vec<String> = canonical_targets
        .iter()
        .map(|target| target.path.to_owned())
        .collect();
    for target in &canonical_targets {
        match reconcile_canonical_target(root, mode, target)? {
            CanonicalTargetOutcome::Reproduced { written } => {
                canonical_reproduced += 1;
                if written {
                    debug_assert_eq!(mode, RunMode::Update);
                    canonical_written += 1;
                } else if mode == RunMode::Update {
                    canonical_skipped_writes += 1;
                }
            }
            CanonicalTargetOutcome::Drifted => {
                debug_assert_eq!(mode, RunMode::Check);
                canonical_drifted.push(target.path.to_owned());
            }
        }
    }
    let canonical_elapsed = canonical_started.elapsed();
    if let Some(progress) = progress.as_ref() {
        progress.stage_end("pipeline:canonical-abstract", canonical_elapsed);
    }

    let spec = spec_for_scope(full_spec(), output_scope)?;

    // Single-pass: the schemas leaf is now a normal carrier-reading
    // export leaf (it consumes `stage-snapshot`, not the sink bytes), so the whole selected
    // DAG — the terminal gts sink and the schemas tail included — runs in ONE scheduler pass.
    // No producer/serialization re-derivation, no SINK_STAGE-only sub-run.
    let graph = spec.validate()?;
    let registry = crate::registry::registry_for(profile);
    let bound = bind(&spec, &graph, &registry)?;
    // The whole-run manifest remains the zero-work fixed-point boundary. On a manifest
    // miss, the DAG's exact-one cache dispositions admit independently bounded stage
    // contributions to the structural cache while the four cumulative carrier
    // aggregates recompute from the live frontier. Cache misses still execute every
    // selected stage and all attach/receipt/provenance gates remain mandatory.
    let mut ctx = RunContext::open(root, jobs)?;
    // Bound peak residency: release each stage's carrier once its last declared carrier
    // consumer has run (or after production when every later edge is artifact-only).
    // This path is the whole-repository build — roughly 47 contribution and
    // aggregate products — so retaining every product's bundle for the run's life makes
    // peak memory the SUM over the DAG rather than the live frontier, which is what
    // OOM-kills a 16 GB CI runner. Everything the reconcile below reads (each product's
    // COMMITTED byte-artifact lane, every digest) survives the release untouched, so the
    // written/compared bytes and `combined_digest` are byte-identical either way.
    ctx.carrier_retention = CarrierRetention::DropAfterLastConsumer;
    let gmn_observation_key = (
        "stage-conformance".to_owned(),
        crate::stages::conformance::gmn_grounding::CHANNEL.to_owned(),
    );
    ctx.retained_artifacts.insert(gmn_observation_key.clone());
    if let Some(progress) = progress.as_ref() {
        ctx = ctx.with_progress(Arc::clone(progress));
        progress.stage_start("pipeline:dag");
    }
    let scheduler_started = Instant::now();
    let mut result = run(&graph, &bound, &mut ctx)?;
    let scheduler_elapsed = scheduler_started.elapsed().as_millis();
    let gmn_reports = crate::stages::conformance::gmn_grounding::from_bytes(
        &result
            .retained_artifacts
            .remove(&gmn_observation_key)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    cause: None,
                    stage: "pipeline".to_owned(),
                    message: "required producer GMN observations are missing".to_owned(),
                })
            })?,
    )?
    .reports();
    if let Some(progress) = progress.as_ref() {
        progress.stage_end(
            "pipeline:dag",
            std::time::Duration::from_millis(u64::try_from(scheduler_elapsed).unwrap_or(u64::MAX)),
        );
    }
    let mut timings: Vec<TimingRecord> = Vec::new();
    timings.push(TimingRecord {
        phase: "canonical-abstract".to_string(),
        elapsed_ms: canonical_elapsed.as_millis(),
        metadata: Some(format!("targets={}", canonical_targets.len())),
    });
    timings.push(TimingRecord {
        phase: "pipeline-scheduler".to_string(),
        elapsed_ms: scheduler_elapsed,
        metadata: Some(format!(
            "jobs={jobs};stages={};levels={}",
            result.stage_timings.len(),
            result.level_timings.len()
        )),
    });
    for timing in &result.stage_timings {
        timings.push(TimingRecord {
            phase: format!("stage:{}", timing.stage_id),
            elapsed_ms: timing.elapsed_ms,
            metadata: Some(format!(
                "level={};cached={};cache-outcome={};cache-read-bytes={};cache-write-bytes={};cache-hydration-rss-delta-kib={}",
                timing.level,
                timing.cached,
                timing.cache_outcome,
                timing.cache_read_bytes,
                timing.cache_write_bytes,
                timing
                    .cache_hydration_rss_delta_kib
                    .map_or_else(|| "not-applicable".to_string(), |value| value.to_string())
            )),
        });
    }
    for timing in &result.stage_phase_timings {
        timings.push(TimingRecord {
            phase: format!("stage:{}/{}", timing.stage_id, timing.phase),
            elapsed_ms: timing.elapsed_ms,
            metadata: timing.metadata.clone(),
        });
    }
    for timing in &result.level_timings {
        timings.push(TimingRecord {
            phase: "pipeline-level".to_string(),
            elapsed_ms: timing.elapsed_ms,
            metadata: Some(format!(
                "level={};critical={}",
                timing.level, timing.critical_stage
            )),
        });
    }
    let stage_receipt_root = result.receipt_root.clone();
    let stage_timings = result.stage_timings.clone();
    let stage_phase_timings = result.stage_phase_timings.clone();
    let level_timings = result.level_timings.clone();
    let stage_receipts = result.stage_receipts.clone();
    let mut products: BTreeMap<String, StageProduct> = result.products;
    // The run-level ledger is the scheduler's FORWARD fold of every producer's
    // diagnostic nodes (their report findings, projected once — the single source). The
    // reconcile phase below attaches its own run-level drift/superset findings to this
    // SAME ledger (they are forward, run-level diagnostics), so the ledger stays the one
    // source of truth. The backward RDF→ledger read is gone (greenfield).
    let mut ledger = result.ledger;
    attach_canonical_drift_findings(&mut ledger, &canonical_drifted);

    // PIPELINE_SPINE §4/§7: exactly one stage writes the bundle bytes, AND it must be
    // the stage that DECLARED the sink capability. Assert it on the ACTUAL produced
    // artifacts — stronger than the loader's capability-declaration-count gate — so a
    // stage emitting `gmeow.gts` WITHOUT declaring `sinkCapability` (identity mismatch),
    // a stage emitting it in addition to the declared sink, or a second declared sink,
    // is a hard failure in BOTH update and check modes.
    let declared_sink = declared_sink_stage(&spec)?;
    assert_single_gts_writer(&products, declared_sink)?;
    // Reconcile the declared terminal LAST. Check mode imports the freshly emitted GTS
    // for the superset proof; retaining every other stage's committed artifact store
    // beside that import needlessly adds the whole artifact frontier to peak residency.
    // Consuming each non-terminal product first releases its store immediately after its
    // bytes have been compared/written. The terminal's GTS bytes are therefore the only
    // stage product still live when the superset reader opens its independent carrier.
    // Output order is not semantic (`output_paths` is sorted below), and the single-writer
    // assertion above proves this removal cannot hide a second terminal.
    let terminal_product = products.remove(declared_sink).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            cause: None,
            stage: "pipeline".to_string(),
            message: format!(
                "declared sink {declared_sink} produced no product after the single-writer gate"
            ),
        })
    })?;

    let mut drifted = canonical_drifted;
    let mut produced = canonical_targets.len();
    let mut reproduced = canonical_reproduced;
    let mut written = canonical_written;
    let mut skipped_writes = canonical_skipped_writes;
    let mut removed = 0usize;
    let mut output_paths = canonical_output_paths;

    // ── Reconcile every produced artifact against committed / write it. ──
    let reconcile_started = Instant::now();
    if let Some(progress) = progress.as_ref() {
        progress.stage_start("pipeline:reconcile");
    }
    for product in products
        .into_values()
        .chain(std::iter::once(terminal_product))
    {
        for (path, bytes) in product.artifact_refs()? {
            // Internal in-memory dataflow artifacts (under the `pipeline/` logical
            // prefix: base-graph.nq, composed.nq, documentation.nq) are NOT
            // committed outputs — they exist only to pass between stages. Skip.
            // ONE constant with the scheduler's carrier release, which drops exactly
            // these once their last declared consumer has run.
            if path.starts_with(crate::bundle::INTERNAL_ARTIFACT_PREFIX) {
                continue;
            }
            output_paths.push(path.to_string());
            produced += 1;

            // The `gmeow.gts` bundle: in Update mode WRITE the freshly-assembled
            // bundle to disk (the terminal's sole output — without this a stale
            // local bundle survives an `integrate-main` + update, the exact trap
            // CLAUDE.md warns about). The bundle is a git-ignored local product with
            // no committed copy to fall back on, so it must be regenerated, never
            // trusted stale. In Check mode the production superset gate compares its
            // semantic projection (per-named-graph quad set plus declared carried-file
            // inventory) because CBOR has encoding skew, so it is only counted here.
            if path == GTS_PATH {
                if mode == RunMode::Update {
                    if write_artifact(root, path, bytes)? {
                        written += 1;
                    } else {
                        skipped_writes += 1;
                    }
                } else {
                    // Superset gate (PIPELINE_SPINE §7): every committed path under
                    // `generated/` must be byte-reconstructible from the emitted
                    // bundle — RDF as a named-graph fold, opaque as an inline blob.
                    // Reconstruct from THESE bytes (re-imported), so the gate proves
                    // the shipped bundle is a superset, not the in-memory carrier.
                    let superset_started = Instant::now();
                    let report = crate::stages::superset::check_superset(root, bytes)?;
                    timings.push(TimingRecord {
                        phase: "superset".to_string(),
                        elapsed_ms: superset_started.elapsed().as_millis(),
                        metadata: Some("path=generated/dist/gmeow.gts".to_string()),
                    });
                    for path in report.missing {
                        drifted.push(path.clone());
                        attach_pipeline_finding(
                            &mut ledger,
                            CODE_SUPERSET_MISSING,
                            &path,
                            format!("{path} has no carrier representative in gmeow.gts"),
                        );
                    }
                    for path in report.mismatch {
                        drifted.push(path.clone());
                        attach_pipeline_finding(
                            &mut ledger,
                            CODE_SUPERSET_MISMATCH,
                            &path,
                            format!("{path} differs from its gmeow.gts reconstruction"),
                        );
                    }
                    for rep in report.orphan {
                        drifted.push(rep.clone());
                        attach_pipeline_finding(
                            &mut ledger,
                            CODE_SUPERSET_ORPHAN,
                            &rep,
                            format!(
                                "{rep} is carried in gmeow.gts but maps to no committed generated/ path"
                            ),
                        );
                    }

                    // GMN-1 round-trip gate: the executed byte
                    // witness behind `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic
                    // true` claim, total over the grounding slices' GMN-0 (logic, lang,
                    // math — module.ttl + examples/*.ttl). Mirrors the superset gate's
                    // discipline: no skips, a single non-round-tripping source reds the
                    // build. Grades the exact authored-source observations emitted by
                    // stage-conformance; no source loading or codec execution here.
                    let gmn1_started = Instant::now();
                    let gmn1_report = &gmn_reports.roundtrip;
                    timings.push(TimingRecord {
                        phase: "gmn1-roundtrip".to_string(),
                        elapsed_ms: gmn1_started.elapsed().as_millis(),
                        metadata: Some(format!(
                            "source=producer-observation;failures={}",
                            gmn1_report.failures.len()
                        )),
                    });
                    for failure in &gmn1_report.failures {
                        drifted.push(failure.path.clone());
                        // L3 ledger-identity split, driven off the codec's ONE canonical
                        // classifier (`Gmn1Error::failure_class()`): every typed GMN failure
                        // — uncovered term, non-canonical order, malformed number, undeclared
                        // dialect version, non-decodable grammar — is interned through
                        // `attach_gmn_failure`'s DiagLedger identity (finding_iri + anchor +
                        // antecedents), so a reasoner-over-findings meta-fold can join ANY of
                        // them by class (per the diagnostics-producers-must-carry-ledger-
                        // identity rule), never a hand-built Finding and never a second
                        // classifier.
                        gmeow_lang_bridge::error::attach_gmn_failure(
                            &mut ledger,
                            PIPELINE_STAGE_ID,
                            &failure.path,
                            &failure.error,
                        );
                    }

                    // Production shipped-projection lint: read every committed
                    // `generated/projections/lang/gmn1/*.gmn` back through the production
                    // codec and hard-fail (with the same ledger identity) if any shipped
                    // projection fails to read clean — a real production caller of the
                    // canonical `failure_class()` over shipped artifacts.
                    let gmn1_shipped_report =
                        crate::stages::gmn1_gate::check_gmn1_shipped_projections(root)?;
                    for failure in gmn1_shipped_report.failures {
                        drifted.push(failure.path.clone());
                        gmeow_lang_bridge::error::attach_gmn_failure(
                            &mut ledger,
                            PIPELINE_STAGE_ID,
                            &failure.path,
                            &failure.error,
                        );
                    }

                    // GMN-1 construct-coverage-completeness audit: the round-trip
                    // gate above proves every quad IN
                    // the grounding corpus round-trips byte-exact, but says nothing
                    // about whether the corpus actually EXERCISES every codec
                    // construct category — a dispatch branch with zero real occurrences
                    // could carry a latent bug indefinitely without ever failing the
                    // round-trip gate. This closes that gap: it hard-fails if ANY
                    // `gmeow_lang_bridge::Gmn1ConstructCategory` the codec's write-side
                    // dispatch can produce has zero occurrences across the real
                    // grounding sources.
                    let gmn1_coverage_started = Instant::now();
                    let gmn1_coverage_report = &gmn_reports.coverage;
                    timings.push(TimingRecord {
                        phase: "gmn1-construct-coverage".to_string(),
                        elapsed_ms: gmn1_coverage_started.elapsed().as_millis(),
                        metadata: Some(format!(
                            "source=producer-observation;unexercised={}",
                            gmn1_coverage_report.unexercised.len()
                        )),
                    });
                    if !gmn1_coverage_report.is_complete() {
                        let focus = "slices/grounding (gmn1-construct-coverage)";
                        drifted.push(focus.to_string());
                        attach_pipeline_finding(
                            &mut ledger,
                            CODE_GMN1_CONSTRUCT_COVERAGE_GAP,
                            focus,
                            format!(
                                "GMN-1 construct-coverage audit: {} codec construct \
                                 {} never exercised by real grounding content \
                                 (module.ttl + examples/*.ttl across logic/lang/math) — \
                                 the 'total over grounding' claim is unproven for: {:?}",
                                gmn1_coverage_report.unexercised.len(),
                                if gmn1_coverage_report.unexercised.len() == 1 {
                                    "category"
                                } else {
                                    "categories"
                                },
                                gmn1_coverage_report.unexercised
                            ),
                        );
                    }

                    // GMN-1 native codebook-digest gate + pack-root check: recompute the
                    // codebook Merkle root and refuse any shipped envelope declaring a stale
                    // `gmeow:gmnCodebookDigest`, and (when the fanout has projected the pack)
                    // byte-assert the conformance pack's `gmeow:gmnPackRoot` against the
                    // recomputation. Both are graph-level cross-node checks no per-record codec
                    // parse or SHACL shape can see.
                    reconcile_gmn1_digest_gates(root, &mut ledger, &mut drifted)?;
                }
                reproduced += 1;
                continue;
            }

            // `dist/*` artifacts are gitignored runtime outputs with NO committed
            // authority: a fresh checkout (CI strict sync) has no `dist/` tree,
            // so they can never be drift-compared. They are WRITTEN in Update but
            // SKIPPED in Check. Their producer actions and emitted digests are
            // deterministic and content-addressed; no test reruns the DAG merely to
            // compare a second emission.
            if path.starts_with("dist/") {
                if mode == RunMode::Update && output_scope == RunOutputScope::All {
                    if write_artifact(root, path, bytes)? {
                        written += 1;
                    } else {
                        skipped_writes += 1;
                    }
                }
                reproduced += 1;
                continue;
            }

            if mode == RunMode::Update {
                // A stage's only output is its carrier contribution (PIPELINE_SPINE
                // §3.1): a committed `generated/` file is NOT written here — it is
                // projected from the bundle by the post-pipeline fanout phase (§6),
                // which runs after this loop writes `gmeow.gts`. Retiring the direct
                // write leaves the terminal `gmeow.gts` (plus explicitly selected
                // gitignored `dist/*` projections) as the pipeline's only disk output.
                // Paths OUTSIDE `generated/` (e.g. the
                // root OASIS catalog) are out of the superset law's scope (§5 governs
                // `generated/`), so their producing stage still writes them directly.
                if !path.starts_with("generated/") {
                    if write_artifact(root, path, bytes)? {
                        written += 1;
                    } else {
                        skipped_writes += 1;
                    }
                }
                reproduced += 1;
                continue;
            }

            // ── Check mode: compare to the committed bytes. ──
            let committed = match std::fs::read(root.join(path)) {
                Ok(c) => c,
                Err(e) => {
                    drifted.push(path.to_string());
                    attach_pipeline_finding(
                        &mut ledger,
                        CODE_MISSING,
                        path,
                        format!("{path} could not be read for comparison: {e}"),
                    );
                    continue;
                }
            };

            if committed.as_slice() == bytes {
                reproduced += 1;
                continue;
            }

            // RDF/Turtle/N-Triples/N-Quads leaves are validated against committed
            // by GRAPH ISOMORPHISM (their unit-test contract: serialization
            // formatting is immaterial because the committed files were minted by a
            // DIFFERENT serializer — the retired Python build's rdflib). Compare
            // them isomorphically; byte drift that is isomorphic is NOT a finding.
            if is_rdf_artifact(path) && graphs_isomorphic(&committed, bytes) {
                reproduced += 1;
                continue;
            }

            // A genuine drift.
            drifted.push(path.to_string());
            attach_pipeline_finding(
                &mut ledger,
                CODE_DRIFT,
                path,
                format!("{path} differs from the committed artifact"),
            );
        }
    }
    timings.push(TimingRecord {
        phase: "pipeline-reconcile".to_string(),
        elapsed_ms: reconcile_started.elapsed().as_millis(),
        metadata: Some(format!("produced={produced};reproduced={reproduced}")),
    });
    if let Some(progress) = progress.as_ref() {
        progress.stage_end("pipeline:reconcile", reconcile_started.elapsed());
    }

    // The consuming iteration above has released every stage product. Fanout and the
    // post-update gates operate only on the freshly written terminal bundle; update mode
    // deliberately re-reads that one canonical disk artifact below.

    // ── Fanout (PIPELINE_SPINE §6): the separate post-pipeline projection phase. ──
    // The pipeline has now written `gmeow.gts`; project every committed `generated/`
    // file back out of it (pure reconstruction, no compute). This is the only writer
    // of the `generated/` tree — the stages contributed to the carrier, the terminal
    // presented it, and fanout unpacks it. Check mode does NOT fan out: the superset
    // gate above already proved every committed path is reconstructible.
    if mode == RunMode::Update {
        let fanout_started = Instant::now();
        if let Some(progress) = progress.as_ref() {
            progress.stage_start("pipeline:fanout");
        }
        let report = crate::fanout::fanout(root, jobs)?;
        if let Some(progress) = progress.as_ref() {
            progress.stage_end("pipeline:fanout", fanout_started.elapsed());
        }
        timings.push(TimingRecord {
            phase: "fanout".to_string(),
            elapsed_ms: fanout_started.elapsed().as_millis(),
            metadata: Some(format!(
                "produced={};written={};skipped={};removed={}",
                report.produced, report.written, report.skipped, report.removed
            )),
        });
        written += report.written;
        skipped_writes += report.skipped;
        removed += report.removed;

        // Update is check-while-writing, not a weaker generation lane. Run the
        // strict carrier/syntax gates against the freshly fanned-out tree without
        // executing the pipeline a second time.
        // GENERATED-READ-OK: post-update audit re-reads the terminal just emitted by
        // this run's fanout; it never folds committed generated bytes into a stage.
        let gts_path = root.join(GTS_PATH);
        let gts = std::fs::read(&gts_path).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                cause: None,
                stage: "pipeline".to_string(),
                message: format!(
                    "read freshly emitted {} for post-update gates: {error}",
                    gts_path.display()
                ),
            })
        })?;
        let gates_started = Instant::now();
        if let Some(progress) = progress.as_ref() {
            progress.stage_start("pipeline:post-update-gates");
        }
        run_post_update_gates(
            root,
            &gts,
            &gmn_reports,
            &mut ledger,
            &mut drifted,
            &mut timings,
        )?;
        if let Some(progress) = progress.as_ref() {
            progress.stage_end("pipeline:post-update-gates", gates_started.elapsed());
        }
    }

    drifted.sort();
    drifted.dedup();
    output_paths.sort();
    output_paths.dedup();

    // The DAG-workflow certification of the build plan (the build-pipeline executor's typed surface): the
    // SAME verdict the RDF `emit_dag_certification` emits, lowered to the typed
    // ReasoningResult a consumer reads. Hard-fails if the plan is not certified.
    let certification = certify_build_plan(&spec)?;

    let mut report = RunReport {
        mode,
        produced,
        reproduced,
        written,
        skipped_writes,
        removed,
        ledger,
        drifted,
        timings,
        stage_receipt_root,
        stage_timings,
        stage_phase_timings,
        level_timings,
        stage_receipts,
        output_paths,
        certification,
    };
    // Reuse this run's exact receipts without walking nonpersistent dependencies.
    // The same typed verdict governs CLI completion and manifest publication.
    report.record_fixture_candidate(root)?;
    report.timings.push(TimingRecord {
        phase: "pipeline-total".to_string(),
        elapsed_ms: total_started.elapsed().as_millis(),
        metadata: Some(format!(
            "mode={};outputs={}",
            match mode {
                RunMode::Check => "check",
                RunMode::Update => "update",
            },
            match output_scope {
                RunOutputScope::All => "all",
                RunOutputScope::Committed => "committed",
            },
        )),
    });
    Ok(report)
}

/// Run the strict gates that historically lived only on the read-only drift path
/// against the freshly written carrier/tree. Update therefore has the same
/// superset and GMN teeth without executing the pipeline a second time.
fn run_post_update_gates(
    root: &Path,
    gts: &[u8],
    gmn_reports: &crate::stages::conformance::gmn_grounding::Reports,
    ledger: &mut DiagLedger,
    drifted: &mut Vec<String>,
    timings: &mut Vec<TimingRecord>,
) -> Result<(), gmeow_errors::Diag> {
    let started = Instant::now();
    let report = crate::stages::superset::check_superset(root, gts)?;
    timings.push(TimingRecord {
        phase: "superset".to_string(),
        elapsed_ms: started.elapsed().as_millis(),
        metadata: Some("path=generated/dist/gmeow.gts;after=update".to_string()),
    });
    for path in report.missing {
        drifted.push(path.clone());
        attach_pipeline_finding(
            ledger,
            CODE_SUPERSET_MISSING,
            &path,
            format!("{path} has no carrier representative in gmeow.gts"),
        );
    }
    for path in report.mismatch {
        drifted.push(path.clone());
        attach_pipeline_finding(
            ledger,
            CODE_SUPERSET_MISMATCH,
            &path,
            format!("{path} differs from its gmeow.gts reconstruction"),
        );
    }
    for rep in report.orphan {
        drifted.push(rep.clone());
        attach_pipeline_finding(
            ledger,
            CODE_SUPERSET_ORPHAN,
            &rep,
            format!("{rep} is carried in gmeow.gts but maps to no generated path"),
        );
    }

    let started = Instant::now();
    let roundtrip = &gmn_reports.roundtrip;
    timings.push(TimingRecord {
        phase: "gmn1-roundtrip".to_string(),
        elapsed_ms: started.elapsed().as_millis(),
        metadata: Some(format!(
            "source=producer-observation;failures={}",
            roundtrip.failures.len()
        )),
    });
    for failure in &roundtrip.failures {
        drifted.push(failure.path.clone());
        gmeow_lang_bridge::error::attach_gmn_failure(
            ledger,
            PIPELINE_STAGE_ID,
            &failure.path,
            &failure.error,
        );
    }

    for failure in crate::stages::gmn1_gate::check_gmn1_shipped_projections(root)?.failures {
        drifted.push(failure.path.clone());
        gmeow_lang_bridge::error::attach_gmn_failure(
            ledger,
            PIPELINE_STAGE_ID,
            &failure.path,
            &failure.error,
        );
    }

    let started = Instant::now();
    let coverage = &gmn_reports.coverage;
    timings.push(TimingRecord {
        phase: "gmn1-construct-coverage".to_string(),
        elapsed_ms: started.elapsed().as_millis(),
        metadata: Some(format!(
            "source=producer-observation;unexercised={}",
            coverage.unexercised.len()
        )),
    });
    if !coverage.is_complete() {
        let focus = "slices/grounding (gmn1-construct-coverage)";
        drifted.push(focus.to_string());
        attach_pipeline_finding(
            ledger,
            CODE_GMN1_CONSTRUCT_COVERAGE_GAP,
            focus,
            format!(
                "GMN-1 construct-coverage audit found {} unexercised categories: {:?}",
                coverage.unexercised.len(),
                coverage.unexercised
            ),
        );
    }

    reconcile_gmn1_digest_gates(root, ledger, drifted)?;
    Ok(())
}

/// The native GMN-1 codebook-digest gate + pack-root check, shared by both reconcile paths
/// (read-only Check and post-update). Recompute the codebook Merkle root and refuse any shipped
/// `gmeow:GmnEnvelope` declaring a stale `gmeow:gmnCodebookDigest`
/// (`lang:GmnCodebookDigestMismatch`), and — once the fanout has projected the conformance pack —
/// byte-assert its `gmeow:gmnPackRoot` against the recomputation. Both are graph-level cross-node
/// checks no per-record codec parse or SHACL shape can see; a missing pack is a NO-OP the fanout
/// activates, never a faked pass.
fn reconcile_gmn1_digest_gates(
    root: &Path,
    ledger: &mut DiagLedger,
    drifted: &mut Vec<String>,
) -> Result<(), gmeow_errors::Diag> {
    let digest_report = crate::stages::gmn1_gate::check_gmn1_codebook_digest_on_gate(root)?;
    for mismatch in digest_report.mismatches {
        drifted.push(mismatch.source.clone());
        attach_pipeline_finding(
            ledger,
            CODE_GMN1_CODEBOOK_DIGEST_MISMATCH,
            &mismatch.source,
            format!(
                "envelope {} declares codebook digest {} but the codebook recomputes to {} \
                 (lang:GmnCodebookDigestMismatch)",
                mismatch.envelope, mismatch.declared, mismatch.recomputed
            ),
        );
    }

    let pack_report = crate::stages::gmn1_gate::check_gmn1_pack_root(root)?;
    if !pack_report.is_clean() {
        // The finding focus is the version-keyed pack path the check resolved (gmn1/v<major>/…).
        let focus = pack_report.pack_rel.clone();
        drifted.push(focus.clone());
        attach_pipeline_finding(
            ledger,
            CODE_GMN1_PACK_ROOT_MISMATCH,
            &focus,
            format!(
                "conformance pack declares gmeow:gmnPackRoot {:?} but its parts recompute to {}",
                pack_report.declared_root, pack_report.recomputed_root
            ),
        );
    }
    Ok(())
}

/// The stage_id that DECLARES [`SINK_CAPABILITY`] in `spec` — the identity the runtime
/// writer must match. `spec.validate()` (already run before this is called) asserts
/// exactly one such stage exists, but this re-derives it defensively rather than
/// trusting that invariant survives at a distance.
fn declared_sink_stage(spec: &PipelineSpec) -> Result<&str, gmeow_errors::Diag> {
    let sinks: Vec<&str> = spec
        .stages
        .iter()
        .filter(|s| s.capabilities.iter().any(|c| c == SINK_CAPABILITY))
        .map(|s| s.id.as_str())
        .collect();
    match sinks.as_slice() {
        [id] => Ok(id),
        [] => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            cause: None,
            stage: "pipeline".to_string(),
            message:
                "no stage declares sinkCapability; PIPELINE_SPINE §4 requires exactly one terminal writer"
                    .to_string(),
        })),
        _ => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            cause: None,
            stage: "pipeline".to_string(),
            message: format!(
                "{} stages declare sinkCapability ({}); exactly one is allowed (PIPELINE_SPINE §4)",
                sinks.len(),
                sinks.join(", ")
            ),
        })),
    }
}

/// Whether `path` is an RDF text artifact compared by graph isomorphism (its
/// committed bytes were serialized by the retired Python build, so byte parity
/// is not expected; the unit tests assert isomorphism, never bytes).
/// PIPELINE_SPINE §4/§7 — exactly ONE stage writes the bundle bytes, AND it must be the
/// stage that DECLARED `sinkCapability` (`declared_sink`). The loader gate
/// (`loader::validate`) asserts exactly one stage DECLARES `sinkCapability`; this asserts
/// the stronger runtime property: exactly one produced product actually carries the
/// `gmeow.gts` byte artifact, AND its stage_id is the declared sink. A stage emitting the
/// bundle bytes without declaring the capability (identity mismatch — a rogue writer
/// impersonating the terminal), the declared sink NOT emitting it, or a second writer, is
/// a hard failure (no-optionality, fail-closed) — never a silent second terminal, in
/// either update or check mode.
fn assert_single_gts_writer(
    products: &BTreeMap<String, StageProduct>,
    declared_sink: &str,
) -> Result<(), gmeow_errors::Diag> {
    let writers: Vec<&str> = products
        .iter()
        .filter(|(_, p)| p.artifact(GTS_PATH).is_some())
        .map(|(id, _)| id.as_str())
        .collect();
    match writers.len() {
        1 => {
            let writer = writers[0];
            if writer != declared_sink {
                return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    cause: None,
                    stage: "pipeline".to_string(),
                    message: format!(
                        "stage {writer} emits the `{GTS_PATH}` bundle bytes but the declared sink (sinkCapability) is {declared_sink}; PIPELINE_SPINE §4/§7 requires the terminal writer's identity to match the declared sink"
                    ),
                }));
            }
            Ok(())
        }
        0 => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            cause: None,
            stage: "pipeline".to_string(),
            message: format!(
                "no stage emits the `{GTS_PATH}` bundle bytes; PIPELINE_SPINE §4 requires exactly one terminal writer"
            ),
        })),
        n => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            cause: None,
            stage: "pipeline".to_string(),
            message: format!(
                "{n} stages emit the `{GTS_PATH}` bundle bytes ({}); exactly one terminal writer is allowed (PIPELINE_SPINE §4)",
                writers.join(", ")
            ),
        })),
    }
}

fn is_rdf_artifact(path: &str) -> bool {
    path.ends_with(".ttl") || path.ends_with(".nt") || path.ends_with(".nq")
}

/// Whether two RDF documents (Turtle / N-Triples / N-Quads, by `a`'s extension —
/// both committed and produced share a logical path) are isomorphic: the same set
/// of quads after RDFC-1.0 blank-node canonicalization. Returns false on any
/// parse error (treated as drift).
fn graphs_isomorphic(committed: &[u8], produced: &[u8]) -> bool {
    canonical_quad_set(committed)
        .zip(canonical_quad_set(produced))
        .map(|(c, p)| c == p)
        .unwrap_or(false)
}

/// Parse RDF (lenient, Turtle for `.ttl`, N-Quads otherwise) and return the
/// canonicalized quad set as sorted strings. `None` on a parse error or a
/// canonicalization refusal, which the caller reports as drift.
fn canonical_quad_set(bytes: &[u8]) -> Option<std::collections::BTreeSet<String>> {
    // Try Turtle first, then N-Quads — the leaves emit one of these.
    // Native text ingress + native full RDFC-1.0: no oxigraph::io
    // parse, no oxrdf `Dataset::canonicalize`. The parsed IR is FLATTENED back to its
    // un-folded plain-quad stream (`flat_rdf_quads_from_dataset`) before re-freezing
    // and canonicalizing, so the RDF 1.2 statement overlay canonicalizes as the same
    // flat `rdf:reifies` / annotation triple set the prior oxigraph-flat path produced
    // — NOT the native folded overlay sentinels. The canonical N-Quads lines are then
    // collected into the order-independent set (each already `.`-terminated).
    for media_type in ["text/turtle", "application/n-quads"] {
        let Ok(ir) = purrdf::parse_dataset(bytes, media_type, None) else {
            continue;
        };
        // The full flat quad stream (base ∪ un-folded reifier/annotation rows) — the
        // same emptiness predicate the prior `flat_oxigraph_quads_from_dataset` guarded.
        let quads = purrdf::flat_rdf_quads_from_dataset(&ir);
        if !quads.is_empty() {
            let flat = purrdf::flat_dataset_from_quads(&quads).ok()?;
            let set: std::collections::BTreeSet<String> = purrdf::try_canonicalize(
                &flat,
                &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
            )
            .ok()?
            .nquads()
            .lines()
            .map(str::to_owned)
            .collect();
            return Some(set);
        }
    }
    None
}

/// Certify the build plan under the DAG-workflow contract and lower the verdict
/// into the typed [`ReasoningResult`] a build run surfaces.
///
/// The `gmeow:pipeline-build` plan is a `logic:Plan` declared under the DAG-workflow
/// contract; this certifies its producer → consumer dataflow closure with the SHARED
/// [`certify_acyclic`] certifier — the SAME one [`crate::graph::StageGraph::build`]
/// runs at load time and the RDF `teleology::emit_dag_certification` runs for a
/// reified plan — so the typed result and the RDF surface agree by construction.
///
/// The loader already rejected any cycle before a run reaches its result, so in
/// practice the verdict is `Certified`. The hard-fail here is the no-silent-default
/// backstop (Principle: no degraded fallback): a build that reached its result yet
/// is NOT certified is a defect, returned as a [`crate::error::InvalidDag`] rather
/// than a degraded result.
fn certify_build_plan(spec: &PipelineSpec) -> Result<ReasoningResult, gmeow_errors::Diag> {
    // Producer → consumer orientation, matching the canonical logic:dataflowConsumes
    // (consumer → producer) the executor inverts — identical to the edge derivation
    // `StageGraph::build` and the `dag_profile_tests` use.
    let edges: Vec<(String, String)> = spec
        .stages
        .iter()
        .flat_map(|s| {
            s.consumes
                .iter()
                .map(move |dep| (dep.clone(), s.id.clone()))
        })
        .collect();
    let cert = certify_acyclic(edges.iter().map(|(a, b)| (a.as_str(), b.as_str())));
    if !cert.is_certified() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
            message: format!(
                "the build plan {} reached its result but is NOT certified under the \
             DAG-workflow contract; offending cycle members: {}",
                spec.id,
                cert.witness().join(" → ")
            ),
        }));
    }
    Ok(cert.into_reasoning_result(BUILD_DAG_CONTRACT, BUILD_DAG_WORLD))
}

/// Write `bytes` to `root.join(path)` when content changed, creating parent directories.
///
/// Returns `true` when the file was rewritten and `false` when the existing bytes
/// already matched.
pub(crate) fn write_artifact(
    root: &Path,
    path: &str,
    bytes: &[u8],
) -> Result<bool, gmeow_errors::Diag> {
    let target = root.join(path);
    match std::fs::read(&target) {
        Ok(existing) if existing == bytes => return Ok(false),
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("artifact");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = target.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, &target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(true)
}

#[path = "run.ledger_integration_tests.rs"]
#[cfg(test)]
mod ledger_integration_tests;

#[path = "run.dag_profile_tests.rs"]
#[cfg(test)]
mod dag_profile_tests;

#[path = "run.single_writer_gate.rs"]
/// The runtime one-terminal gate (PIPELINE_SPINE §4): exactly one produced product may
/// carry the `gmeow.gts` byte artifact.
#[cfg(test)]
mod single_writer_gate;

#[path = "run.diagnostics_ingest_gate.rs"]
/// G4/G5: the run ledger is a LOAD-BEARING FORWARD projection of a producer's report
/// findings, attributed to the REAL producing stage. Drives the real `stage-validate`
/// stage over a fixture whose (empty) source graph violates a SHACL shape, folds its
/// FORWARD `StageOutput.diags` into a run ledger the same way the scheduler does, and
/// asserts the SHACL diagnostic reaches the ledger attributed to `stage-validate` (never
/// the synthetic reconcile stage) and projects into the wire findings — via the FORWARD
/// path (`diag_render::finding_nodes`), not the retired backward RDF→ledger read.
#[cfg(test)]
mod diagnostics_ingest_gate;

#[cfg(test)]
#[path = "run/acceptance_tests.rs"]
mod acceptance_tests;

#[cfg(test)]
#[path = "run.output_scope_tests.rs"]
mod output_scope_tests;

#[cfg(test)]
#[path = "run.bound_stage_tests.rs"]
mod bound_stage_tests;

#[cfg(test)]
pub(crate) use bound_stage_tests::bound_production_stage;
