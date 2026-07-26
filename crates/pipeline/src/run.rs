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
    Diag, DiagLedger, Finding, FindingCategory, Grade, Severity, StageId, Standpoint, register_code,
};
use gmeow_logic::dag_profile::certify_acyclic;
use gmeow_logic::result::ReasoningResult;

use crate::loader::{PipelineSpec, StageSpec, bind};
use crate::node::{ENGINE_RESOURCE, SINK_CAPABILITY, SOURCE_ORIGIN, StageProduct};
use crate::registry::default_registry;
use crate::scheduler::{RunContext, run};

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

/// The sole serialization exit; its product carries the `gmeow.gts` bytes.
const SINK_STAGE: &str = "stage-gts-sink";
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

/// Which output family an update-mode run materializes.
///
/// This is explicit feature selection, not capability degradation: the complete
/// DAG still executes and all strict gates still run. The selected output family
/// is then mandatory. [`Committed`](Self::Committed) suppresses only gitignored
/// `dist/*` runtime projections so a generated-only sync does not churn unrelated
/// presentation files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutputScope {
    /// Materialize committed outputs and every gitignored runtime projection.
    All,
    /// Materialize only committed/generated outputs.
    Committed,
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
    /// Drift / write findings (empty ⇒ full parity). These are a *projection* of
    /// [`ledger`](RunReport::ledger) — the drift/superset producers intern their
    /// diagnostics into the carrier ledger, and this field is
    /// `ledger.project_report(...).findings`, so the ledger is the single source
    /// of truth (not a parallel path).
    pub findings: Vec<Finding>,
    /// The carrier-borne diagnostic ledger: the hash-consed witness DAG every
    /// drift/superset finding is interned into (pin-on-attach, stage-attributed,
    /// deterministically ordered). A first-class member of the run's carrier
    /// output — [`findings`](RunReport::findings) is its lossy projection.
    pub ledger: DiagLedger,
    /// The drifted logical paths (check mode), sorted.
    pub drifted: Vec<String>,
    /// Per-phase timing records for profiling the gate without parsing stderr.
    pub timings: Vec<TimingRecord>,
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
    /// Whether the run reproduced every committed artifact with zero drift.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty() && self.drifted.is_empty()
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

/// Build the authoritative full `PipelineSpec` — the executable twin of the
/// dogfooded `gmeow:pipeline-build` DAG in `slices/core/pipeline/module.ttl`.
///
/// Every `impl_key` here is a `default_registry()` key and every `consumes` set
/// matches the bound Rust [`crate::node::Stage::consumes`] exactly (the loader's
/// `bind` proves this). The slice `module.ttl` mirrors this graph as data; this
/// Rust spec is the one the run executes.
pub fn full_spec() -> PipelineSpec {
    // ── the spine ──
    let mut stages = vec![
        st_source("stage-source-load", "source_load", &[]),
        st("stage-statements", "statements", &[]),
        st_compile_logic(
            "stage-compile-logic",
            "compile_logic",
            &["stage-source-load"],
        ),
        // Leaf compute: RUN the eight math producers (five flagship producers plus the
        // probability-model seam, p-value tri-slice, and Clifford producers) and attach each
        // producer's deterministic RDF graph to the carrier (folded into gmeow.gts by
        // stage-snapshot).
        st("stage-math-producers", "math_producers", &[]),
        // Compute: the rejection-sampled, proof-carrying GMN training-corpus emitter (req
        // #21/#20). A productive functor over the glyph signature: it consumes
        // stage-compile-logic (the typechecker/prover lane) and stage-mappings (the projected
        // GMN forms / glyph registry lane), enumerates well-typed GMN terms, rejection-samples
        // each through five verifiers, and attaches the certified corpus (+ typed rejections)
        // as graph/gmn-training-corpus (folded into gmeow.gts by stage-snapshot).
        st(
            "stage-gmn-training-corpus",
            "gmn-training-corpus",
            &["stage-compile-logic", "stage-mappings"],
        ),
        // Compute: assemble a gmeow:AuthoringPacket per in-repo slice batch and attach
        // the union as graph/authoring-briefs (folded into gmeow.gts by stage-snapshot).
        // It reads the authored slice sources directly, but consumes the four
        // generated-shape producers so its exemplar-tiering shape union folds THIS run's
        // fresh generated/shapes/*.ttl bytes (never the committed files — the
        // stale-disk-fold class) and is scheduled AFTER they materialize generated/shapes.
        st(
            "stage-slice-brief",
            "slice-brief",
            &[
                "stage-compile-logic",
                "stage-export-constraint-shapes",
                "stage-export-frame-shapes",
                "stage-export-result-shapes",
            ],
        ),
        // mappings additionally consumes the constraint-shapes export leaf: the
        // shape-grounding certificate ledger re-derives every logic:formalizes record's
        // preservation judgment over THIS run's projected constraint surfaces (the
        // procedural surface rides the compile-logic product, the FOL surface this leaf).
        st(
            "stage-mappings",
            "mappings",
            &["stage-compile-logic", "stage-export-constraint-shapes"],
        ),
        st_reason(
            "stage-reason",
            "reason",
            &[
                "stage-compile-logic",
                "stage-source-load",
                "stage-statements",
            ],
        ),
        // The production consumer of the native proof-carrying full-FOL backward engine: it
        // compiles the authored `logic:ReasoningProgram` demonstrator corpus (read off the
        // stage-compile-logic Logic handle) against the reasoned rdfs:subClassOf closure
        // (read off the stage-reason product's Reasoning handle, for the order-sorted
        // math-subsort demonstrator), proof-checks every answer, and attaches
        // graph/goal-directed (folded into gmeow.gts by stage-snapshot).
        st_goal_directed(
            "stage-goal-directed",
            "goal_directed",
            &["stage-compile-logic", "stage-reason"],
        ),
        st(
            "stage-gts-compose",
            "gts_compose",
            &[
                "stage-mappings",
                "stage-reason",
                "stage-source-load",
                "stage-statements",
            ],
        ),
        // SHACL validation enforces the FRESH shape union: the generated
        // `generated/shapes/*.ttl` members are read off THIS run's producer products
        // (compile-logic + the three shape export leaves), never the stale committed
        // files (the stale-disk-fold class). The compile-logic edge is narrowed to
        // the object-level graphs (see `st_validate`). It also consumes stage-reason,
        // narrowed to graph/reasoning: the D5 abductive tier reads the reasoned closure
        // (asserted OR entailed), so the validator is a DESCENDANT of the reasoner.
        st_validate(
            "stage-validate",
            "validate",
            &[
                "stage-compile-logic",
                "stage-export-constraint-shapes",
                "stage-export-frame-shapes",
                "stage-export-result-shapes",
                "stage-reason",
                "stage-source-load",
            ],
        ),
        st("stage-conformance", "conformance", &[]),
        // The agreement-matrix dashboard PROJECTS the single external-corpus grade:
        // it reads stage-conformance's attached per-corpus tallies (never re-grading
        // the corpus, PIPELINE_SPINE §3.2/§8) and folds an opaque Markdown member.
        st(
            "stage-export-agreement",
            "agreement",
            &["stage-conformance"],
        ),
        st(
            "stage-docs-render",
            "docs_render",
            &[
                "stage-compile-logic",
                // THIS run's fresh JSON Schema/OpenAPI product: the docs model's
                // per-term schema-fragment digest reads it in-memory, never the
                // previous run's committed generated/schemas/*.json (the
                // stale-disk-fold class).
                "stage-export-json-schema",
                "stage-gts-compose",
                "stage-mappings",
                "stage-reason",
                // THIS run's fresh term-content-manifest product: the docs model's
                // per-term content-address provenance (definition digest + first-seen
                // version + computed changelog) reads it in-memory, never the previous
                // run's committed generated/catalog/term-content-manifest.nq, which
                // lags one regenerate behind on a definition-digest change (the same
                // stale-disk-fold class).
                "stage-term-manifest",
                "stage-validate",
            ],
        ),
        // The generated constraint catalog: one gmeow:ValidationRule per finding code,
        // enriched from the reasoned graph, folded into the bundle by stage-snapshot.
        st(
            "stage-constraint-catalog",
            "constraint_catalog",
            &["stage-reason"],
        ),
        // The generated term content manifest: one gmeow:definitionDigest per
        // documented term (plus first-seen version + computed changelog entries),
        // folded into the bundle by stage-snapshot.
        st("stage-term-manifest", "term_manifest", &["stage-reason"]),
        st(
            "stage-snapshot",
            "snapshot",
            &[
                "stage-compile-logic",
                // Fold the external-corpus divergence Findings into graph/conformance.
                "stage-conformance",
                // Fold the generated constraint catalog into graph/fanout/catalog.
                "stage-constraint-catalog",
                "stage-docs-render",
                // The RDF fanout members ride in from their producing export leaves (the
                // render ran once, in the leaf): profiles / evals scores / glossary
                // vartrans.ttl / research-object graphs. The presenter reads them off these
                // products, never re-rendered.
                "stage-export-evals",
                // The OntoLex vartrans lowering (generated/projections/glossary.vartrans.ttl),
                // folded as its own graph/fanout/projections named graph.
                "stage-export-glossary",
                // Fold THIS run's fresh JSON Schema/OpenAPI into the bundle.
                "stage-export-json-schema",
                "stage-export-profiles",
                "stage-export-research-objects",
                // The generated SKOS concept-scheme surface (generated/skos/gmeow-skos.ttl),
                // folded as its own graph/fanout/skos named graph.
                "stage-export-skos-surface",
                // The certified GMN training corpus (graph/gmn-training-corpus), folded into
                // gmeow.gts (bundle-internal, dual carriage exactly like graph/goal-directed).
                "stage-gmn-training-corpus",
                // The proof-carrying backward engine's checked answers + proof derivations,
                // folded into graph/goal-directed of gmeow.gts.
                "stage-goal-directed",
                "stage-gts-compose",
                // The FINAL projection-report loss ledger (logic ∪ correspondence rows).
                "stage-mappings",
                // The eight math producer graphs (five flagship producers plus the
                // probability-model seam, p-value tri-slice, and Clifford producers),
                // folded into gmeow.gts.
                "stage-math-producers",
                "stage-reason",
                // The authoring-packet corpus (graph/authoring-briefs), folded into
                // gmeow.gts and its fanout twin generated/briefs/authoring-packets.nt.
                "stage-slice-brief",
                // The self-description named graphs (authored default / imports / metadata
                // / alignments / slice-analysis / verify / provenance): the presenter reads
                // them off this product instead of re-loading + re-canonicalizing sources.
                "stage-source-load",
                "stage-statements",
                // Fold the generated term content manifest into graph/fanout/catalog.
                "stage-term-manifest",
                "stage-validate",
            ],
        ),
    ];

    // ── fold-reading export leaves (consume THIS run's snapshot) ──
    for (id, impl_key) in [
        ("stage-export-lpg", "lpg"),
        ("stage-export-yaml-ld", "yaml_ld"),
        ("stage-export-metadata", "metadata"),
        ("stage-export-okf", "okf"),
    ] {
        stages.push(st(id, impl_key, &["stage-snapshot"]));
    }
    // `stage-export-export` additionally consumes THIS run's fresh
    // `stage-export-json-schema` product: its `llms-full.txt` inlined cards gate
    // their `python_model` link on the JSON Schema `$defs` key set (a class with
    // no `$defs` entry has no generated Pydantic model), so it needs the schema
    // in hand, not only the snapshot fold.
    stages.push(st(
        "stage-export-export",
        "export",
        &["stage-export-json-schema", "stage-snapshot"],
    ));

    // ── source-reading export leaves (independent; read slices/metadata/evals) ──
    for (id, impl_key) in [
        ("stage-export-catalog", "catalog"),
        ("stage-export-profiles", "profiles"),
        ("stage-export-frame-shapes", "frame_shapes"),
        ("stage-export-constraint-shapes", "constraint_shapes"),
        // The two slice-quality floor TSVs projected from the ontology-resident
        // gmeow:AxisFloorCommitment / gmeow:SliceTierFloor individuals (P4/P17).
        ("stage-export-governance-floors", "governance_floors"),
        // The two projection-vocabulary ratchet TSVs projected from the
        // ontology-resident gmeow:ProjectionCeilingCommitment / gmeow:ProjectionVocabulary
        // individuals (P4/P17), the ceiling ratchet's counterpart to governance-floors.
        ("stage-export-projection-ceilings", "projection_ceilings"),
        ("stage-export-result-shapes", "result_shapes"),
        ("stage-export-matrix", "matrix"),
        // The human-readable per-slice terminology glossary table: a byte-decorated
        // Markdown projection of the graph/lang-glossary-corpus fold, derived from the
        // SAME reviewed `.po` entry list (crate::stages::lang_glossary::build_entries),
        // never a second parse. Source-reading leaf like matrix (consumes nothing).
        ("stage-export-glossary", "glossary"),
        // The generated SKOS concept-scheme surface: a projection of the lifted
        // NodeKind::Annotation axioms. A source-reading leaf like glossary;
        // its RDF `.ttl` folds as the RDF-fanout named graph graph/fanout/skos/gmeow-skos.ttl.
        ("stage-export-skos-surface", "skos_surface"),
        ("stage-export-apache", "apache"),
        ("stage-export-references", "references"),
        ("stage-export-evals", "evals"),
        ("stage-export-bench", "bench"),
        ("stage-export-cost-ledger", "cost-ledger"),
    ] {
        stages.push(st(id, impl_key, &[]));
    }
    // ── fresh-shape-union export leaves: json-schema, pydantic, and schemas compile
    //    the SHACL shape union whose `generated/shapes/*.ttl` members are THIS run's
    //    producer products (compile-logic + the three shape export leaves), never the
    //    stale committed files (the stale-disk-fold class). All three consume the same
    //    four producers (crate::stages::shape_union_fresh::GENERATED_SHAPE_PRODUCERS). ──
    for (id, impl_key) in [
        ("stage-export-json-schema", "json_schema"),
        // The Pydantic model package (functional documentation surface): co-derived
        // from the SAME fresh shape compilation as json-schema (plus the docs
        // model), folded into REP_MODELS_PYTHON by the sink.
        ("stage-export-pydantic", "pydantic"),
        // The LinkML/TypeScript/GraphQL developer schema surfaces: co-derived from the
        // SAME fresh shape compilation via the shared schema_compile builder, folded
        // into REP_GENERATED by the sink (read off this leaf's product, never
        // re-derived from the carrier).
        ("stage-export-schemas", "schemas"),
    ] {
        stages.push(st(
            id,
            impl_key,
            &[
                "stage-compile-logic",
                "stage-export-constraint-shapes",
                "stage-export-frame-shapes",
                "stage-export-result-shapes",
            ],
        ));
    }
    // research-objects reads two generated projections off upstream products, never the
    // stale/absent committed files on disk: `generated/evals/scores.ttl` off the
    // stage-export-evals product and `generated/queries/dcat.rq` off the stage-mappings
    // product. It therefore consumes both rather than running source-only (kept in sorted
    // position to match the registry consumes() and the module.ttl dataflowConsumes).
    stages.push(st(
        "stage-export-research-objects",
        "research-objects",
        &["stage-export-evals", "stage-mappings"],
    ));

    // ── source-reading validation leaf: enforces the typed result-shape
    //    composition contract across competency files (emits no bundle artifact). ──
    stages.push(st(
        "stage-validate-result-shape-composition",
        "result_shape_composition",
        &[],
    ));

    // ── the single Sink: the terminal gts ARCHIVE writer. It
    //    serializes the assembled carrier (read off `stage-snapshot`'s bundle — no
    //    re-assembly) and folds the by-reference blob archives gathered from the
    //    in-memory JSON-Schema / axiom / reasoning / SHACL-report products. ──
    stages.push(st_sink(
        SINK_STAGE,
        "gts_sink",
        &[
            "stage-compile-logic",
            // The opaque fanout members ride in from their producing export leaves (each
            // rendered once, in the leaf); `collect_fanout_opaque_members` reads them off these
            // products instead of re-rendering from disk (PIPELINE_SPINE §3.2/§4).
            "stage-export-agreement",
            "stage-export-apache",
            "stage-export-bench",
            // constraint-shapes.ttl (logic: FOL-axiom SHACL projection) is folded fresh into
            // REP_SHAPES by build_archive_blobs, so the sink consumes it (kept in sorted
            // position to match the registry consumes()); a first run has no on-disk file.
            "stage-export-constraint-shapes",
            // The deterministic engine-cost ledger (bench/cost-baseline.json projection)
            // rides in as an opaque fanout member exactly like the perf leaderboard (sorted
            // position: constraint-shapes < cost-ledger < evals).
            "stage-export-cost-ledger",
            "stage-export-evals",
            // The generated shape surfaces (P11 frame shapes + the ResultShape SHACL
            // projection): REP_SHAPES folds THESE runs' fresh bytes, never a stale
            // disk read (the same freshness rule as validation-shapes.ttl) — without
            // these edges a competency/frame-shape edit could never reach the bundle,
            // and the fanout would rewrite the stale committed bytes forever.
            "stage-export-frame-shapes",
            // The human-readable terminology glossary table (byte-decorated Markdown)
            // rides in as an opaque REP_GENERATED fanout member, read off this leaf's
            // product (sorted position: frame-shapes < glossary < governance-floors).
            "stage-export-glossary",
            // The two slice-quality floor TSVs (P17 projection of the ontology floor
            // commitments) ride in as opaque REP_GENERATED fanout members, read off this
            // leaf's product (sorted position: glossary < governance-floors < json-schema).
            "stage-export-governance-floors",
            "stage-export-json-schema",
            "stage-export-matrix",
            "stage-export-metadata",
            // The two projection-vocabulary ratchet TSVs (P17 projection of the
            // ontology-resident ceiling commitments) ride in as opaque REP_GENERATED
            // fanout members, read off this leaf's product (sorted position:
            // metadata < projection-ceilings < pydantic).
            "stage-export-projection-ceilings",
            // THIS run's freshly-rendered Pydantic model package, folded into
            // REP_MODELS_PYTHON by build_archive_blobs (sorted position:
            // projection-ceilings < pydantic < references).
            "stage-export-pydantic",
            "stage-export-references",
            "stage-export-research-objects",
            // THIS run's freshly-projected result-shapes.ttl, folded into REP_SHAPES so a
            // competency ResultShape edit reaches the bundle without a manual disk write.
            "stage-export-result-shapes",
            // THIS run's freshly-rendered LinkML/TypeScript/GraphQL developer schema
            // surfaces, folded into REP_GENERATED by collect_fanout_opaque_members (sorted
            // position: result-shapes < schemas < mappings) — schemas moved from a
            // carrier projection to a fresh SHACL-shape-union compilation, so the sink
            // now reads its product instead of re-deriving from the in-memory carrier.
            "stage-export-schemas",
            "stage-mappings",
            "stage-reason",
            "stage-snapshot",
            "stage-source-load",
            "stage-statements",
            "stage-validate",
        ],
    ));

    // Fill each stage's attach declaration (gmeow:attachesGraph / gmeow:attachesBlobRep)
    // from the bound Rust impl — the single Rust-side authority. The scheduler verifies
    // the ACTUAL run-time delta against this same set (error::AttachDrift), and the
    // dogfooding parity gate (tests/dag_dogfood.rs) proves the slice module.ttl mirror
    // matches this Rust spec, so the RDF declaration is verified against code without
    // re-authoring the set in a third literal here.
    let registry = default_registry();
    for s in &mut stages {
        if let Some(stage) = registry.get(&s.impl_key) {
            s.attaches_graphs = {
                let mut g = stage.attaches_graphs().to_vec();
                g.sort();
                g.dedup();
                g
            };
            s.attaches_blob_reps = {
                let mut b = stage.attaches_blob_reps().to_vec();
                b.sort();
                b.dedup();
                b
            };
        }
    }

    PipelineSpec {
        id: "pipeline-build".to_string(),
        stages,
    }
}

/// A capability-free stage (Generated provenance, non-sink) — the default for every
/// transform / validate / docs-render / export leaf (the four behaviorally-inert
/// former kinds collapse to no declaration).
fn st(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    StageSpec {
        id: id.to_string(),
        capabilities: Vec::new(),
        impl_key: impl_key.to_string(),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        resources: Vec::new(),
        dataflow_entities: Vec::new(),
        formats: Vec::new(),
        // Filled by full_spec()'s registry-derivation pass from the bound Rust impl's
        // attaches_graphs() / attaches_blob_reps() (the Rust side of the attach
        // declaration; the slice module.ttl mirrors it and dag_dogfood proves parity).
        attaches_graphs: Vec::new(),
        attaches_blob_reps: Vec::new(),
    }
}

/// The authored-source loader: holds [`SOURCE_ORIGIN`], so its emitted quads' provenance
/// origin is `Source`. Mirrors [`crate::stages::source_load::SourceLoadStage`]'s
/// capabilities() so the loader's bind-agreement holds.
fn st_source(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    let mut s = st(id, impl_key, consumes);
    s.capabilities = vec![SOURCE_ORIGIN.to_string()];
    s
}

/// The sole serialization exit (the gts narrow waist): holds [`SINK_CAPABILITY`], the
/// one stage the loader requires to hold it. Mirrors
/// [`crate::stages::gts_sink::GtsSinkStage`]'s capabilities() for bind-agreement.
fn st_sink(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    let mut s = st(id, impl_key, consumes);
    s.capabilities = vec![SINK_CAPABILITY.to_string()];
    s
}

/// The logic compiler stage: it reads ONLY the narrowed `graph/logic-compile-inputs` named
/// graph off the `stage-source-load` product (a SOUND denylist narrowing of the whole
/// authored corpus its five augmentation readers walk), so its single typed dataflow entity
/// is that graph — a documentation-only edit that leaves the graph's digest unchanged skips
/// re-running the (expensive) compiler. Derives the SAME entity list as
/// [`crate::stages::compile_logic::CompileLogicStage`]'s consumed_entities() so the
/// dag_dogfood parity and the loader's bind-agreement both hold.
fn st_compile_logic(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    let mut s = st(id, impl_key, consumes);
    s.dataflow_entities = vec![(
        "stage-source-load".to_string(),
        vec![crate::stages::carrier::GRAPH_LOGIC_COMPILE_INPUTS.to_string()],
    )];
    s
}

/// The reasoning stage: it requires the exclusive reasoning engine (resource-conflict
/// serialization) AND reads only the object-level named graphs
/// ([`crate::stages::compile_logic::OBJECT_LEVEL_GRAPHS`]) from `stage-compile-logic`
/// (artifact-level typed dataflow). Derives the SAME entity list as
/// [`crate::stages::reason::ReasonStage`]'s consumed_entities() so the dag_dogfood
/// parity and the loader's bind-agreement both hold.
fn st_reason(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    let mut s = st(id, impl_key, consumes);
    s.resources = vec![ENGINE_RESOURCE.to_string()];
    s.dataflow_entities = vec![(
        "stage-compile-logic".to_string(),
        crate::stages::compile_logic::object_level_entity_list(),
    )];
    s
}

/// The SHACL validation stage: its `stage-compile-logic` dependency is narrowed to
/// the complete compiled carrier graphs ([`crate::stages::compile_logic::CARRIER_GRAPHS`])
/// — the program-level digest standing in for the validation-shape byte artifacts it
/// reads off that product, and the narrowing that keeps its `graph/diagnostics`
/// attachment a genuine delta (compile-logic's product carries a graph of the same
/// name). Its `stage-reason` dependency is narrowed to the single `graph/reasoning` named
/// graph (mirroring `st_goal_directed`): the D5 abductive tier reads the reasoned closure,
/// and that projection reifies every derived axiom, so its digest is a faithful key for the
/// closure this stage folds. Derives the SAME entity list as
/// [`crate::stages::validate::ValidateStage`]'s consumed_entities() so the
/// dag_dogfood parity and the loader's bind-agreement both hold.
fn st_validate(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    let mut s = st(id, impl_key, consumes);
    s.dataflow_entities = vec![
        (
            "stage-compile-logic".to_string(),
            crate::stages::compile_logic::carrier_entity_list(),
        ),
        (
            "stage-reason".to_string(),
            vec![gmeow_logic::result_rdf::GRAPH_REASONING.to_string()],
        ),
    ];
    s
}

/// The goal-directed stage: its `stage-reason` dependency is narrowed to the single
/// `graph/reasoning` named graph (the typed Reasoning handle's backing graph, C7) — the
/// reasoned `rdfs:subClassOf` closure the order-sorted math-subsort demonstrator's
/// `subsort_edges` are read from. Derives the SAME entity list as
/// [`crate::stages::goal_directed::GoalDirectedStage`]'s consumed_entities() so the
/// dag_dogfood parity and the loader's bind-agreement both hold.
fn st_goal_directed(id: &str, impl_key: &str, consumes: &[&str]) -> StageSpec {
    let mut s = st(id, impl_key, consumes);
    s.dataflow_entities = vec![(
        "stage-reason".to_string(),
        vec![gmeow_logic::result_rdf::GRAPH_REASONING.to_string()],
    )];
    s
}

/// Run the FULL dogfooded build single-pass and either write every produced
/// artifact (update) or compare it to the committed bytes (check).
///
/// `jobs` is the per-level parallelism budget. Returns a [`RunReport`]; in check
/// mode `report.is_clean()` is the cutover gate (zero drift across every
/// committed artifact). RDF artifacts compare by bytes (they are byte
/// deterministic); the `gmeow.gts` bundle is compared by the FOLD (see
/// `tests/full_parity.rs`) because CBOR has encoding skew.
pub fn run_full(root: &Path, jobs: usize, mode: RunMode) -> Result<RunReport, gmeow_errors::Diag> {
    run_full_scoped(root, jobs, mode, RunOutputScope::All)
}

/// Run the complete build once while materializing only the explicitly selected
/// output family. Selection never removes a stage or gate from the DAG.
pub fn run_full_scoped(
    root: &Path,
    jobs: usize,
    mode: RunMode,
    output_scope: RunOutputScope,
) -> Result<RunReport, gmeow_errors::Diag> {
    run_full_scoped_with_progress(root, jobs, mode, output_scope, None)
}

/// Run the scoped pipeline with an optional live progress sink.
pub fn run_full_scoped_with_progress(
    root: &Path,
    jobs: usize,
    mode: RunMode,
    output_scope: RunOutputScope,
    progress: Option<Arc<dyn Reporter>>,
) -> Result<RunReport, gmeow_errors::Diag> {
    let total_started = Instant::now();
    let spec = full_spec();

    // Single-pass: the schemas leaf is now a normal carrier-reading
    // export leaf (it consumes `stage-snapshot`, not the sink bytes), so the WHOLE DAG —
    // the terminal gts sink and the schemas tail included — runs in ONE scheduler pass.
    // No producer/serialization re-derivation, no SINK_STAGE-only sub-run.
    let graph = spec.validate()?;
    let registry = default_registry();
    let bound = bind(&spec, &graph, &registry)?;
    // The whole-run sync manifest is the profitable cache boundary. Pipeline stage
    // products are cumulative carrier snapshots; persisting or hydrating them creates
    // multi-gigabyte duplicate state and measured slower than recomputation. On a
    // manifest miss, execute the DAG once with no per-stage cache I/O.
    let mut ctx = RunContext::open_uncached(root, jobs);
    if let Some(progress) = progress.as_ref() {
        ctx = ctx.with_progress(Arc::clone(progress));
        progress.stage_start("pipeline:dag");
    }
    let scheduler_started = Instant::now();
    let result = run(&graph, &bound, &mut ctx)?;
    let scheduler_elapsed = scheduler_started.elapsed().as_millis();
    if let Some(progress) = progress.as_ref() {
        progress.stage_end(
            "pipeline:dag",
            std::time::Duration::from_millis(u64::try_from(scheduler_elapsed).unwrap_or(u64::MAX)),
        );
    }
    let mut timings: Vec<TimingRecord> = Vec::new();
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
            metadata: Some(format!("level={};cached={}", timing.level, timing.cached)),
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
    let products: BTreeMap<String, StageProduct> = result.products;
    // The run-level ledger is the scheduler's FORWARD fold of every producer's
    // diagnostic nodes (their report findings, projected once — the single source). The
    // reconcile phase below attaches its own run-level drift/superset findings to this
    // SAME ledger (they are forward, run-level diagnostics), so the ledger stays the one
    // source of truth. The backward RDF→ledger read is gone (greenfield).
    let mut ledger = result.ledger;

    // PIPELINE_SPINE §4/§7: exactly one stage writes the bundle bytes, AND it must be
    // the stage that DECLARED the sink capability. Assert it on the ACTUAL produced
    // artifacts — stronger than the loader's capability-declaration-count gate — so a
    // stage emitting `gmeow.gts` WITHOUT declaring `sinkCapability` (identity mismatch),
    // a stage emitting it in addition to the declared sink, or a second declared sink,
    // is a hard failure in BOTH update and check modes.
    let declared_sink = declared_sink_stage(&spec)?;
    assert_single_gts_writer(&products, declared_sink)?;

    let mut drifted: Vec<String> = Vec::new();
    let mut produced = 0usize;
    let mut reproduced = 0usize;
    let mut written = 0usize;
    let mut skipped_writes = 0usize;
    let mut removed = 0usize;
    let mut output_paths: Vec<String> = Vec::new();

    // ── Reconcile every produced artifact against committed / write it. ──
    let reconcile_started = Instant::now();
    if let Some(progress) = progress.as_ref() {
        progress.stage_start("pipeline:reconcile");
    }
    for product in products.values() {
        for (path, bytes) in &product.artifacts() {
            // Internal in-memory dataflow artifacts (under the `pipeline/` logical
            // prefix: base-graph.nq, composed.nq, documentation.nq) are NOT
            // committed outputs — they exist only to pass between stages. Skip.
            if path.starts_with("pipeline/") {
                continue;
            }
            output_paths.push(path.clone());
            produced += 1;

            // The `gmeow.gts` bundle: in Update mode WRITE the freshly-assembled
            // bundle to disk (the terminal's sole output — without this a stale
            // local bundle survives an `integrate-main` + update, the exact trap
            // CLAUDE.md warns about). The bundle is a git-ignored local product with
            // no committed copy to fall back on, so it must be regenerated, never
            // trusted stale. In Check mode it is compared by the FOLD
            // (per-named-graph quad set + reifier/annotation counts) elsewhere — CBOR
            // has encoding skew — so it is only counted here; the fold gate is
            // `tests/full_parity.rs`.
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
                    // build. Reads the grounding sources directly from `root` (the
                    // codec's covered domain is the AUTHORED slice content, not the
                    // composed bundle), independent of `bytes`.
                    let gmn1_started = Instant::now();
                    let gmn1_report = crate::stages::gmn1_gate::check_gmn1_roundtrip(root)?;
                    timings.push(TimingRecord {
                        phase: "gmn1-roundtrip".to_string(),
                        elapsed_ms: gmn1_started.elapsed().as_millis(),
                        metadata: Some(format!("failures={}", gmn1_report.failures.len())),
                    });
                    for failure in gmn1_report.failures {
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
                    let gmn1_coverage_report =
                        crate::stages::gmn1_gate::check_gmn1_construct_coverage(root)?;
                    timings.push(TimingRecord {
                        phase: "gmn1-construct-coverage".to_string(),
                        elapsed_ms: gmn1_coverage_started.elapsed().as_millis(),
                        metadata: Some(format!(
                            "unexercised={}",
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
            // SKIPPED in Check (their reproducibility is covered by the second-run
            // determinism check in `tests/full_parity.rs`).
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
                    drifted.push(path.clone());
                    attach_pipeline_finding(
                        &mut ledger,
                        CODE_MISSING,
                        path,
                        format!("{path} could not be read for comparison: {e}"),
                    );
                    continue;
                }
            };

            if committed == *bytes {
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
            drifted.push(path.clone());
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
        let gts = products
            .values()
            .find_map(|product| product.artifact(GTS_PATH))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "pipeline".to_string(),
                    message: format!("no produced {GTS_PATH} available for post-update gates"),
                })
            })?;
        let gates_started = Instant::now();
        if let Some(progress) = progress.as_ref() {
            progress.stage_start("pipeline:post-update-gates");
        }
        run_post_update_gates(root, gts, &mut ledger, &mut drifted, &mut timings)?;
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

    timings.push(TimingRecord {
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

    // Project the carrier ledger to the wire findings — the ledger is the single
    // source of truth, `findings` is its lossy projection (F3: not a parallel
    // path). The direct accessor projects the findings without building the
    // intermediate `Report` whose other fields would be discarded here.
    let findings = ledger.findings("gmeow-pipeline");

    Ok(RunReport {
        mode,
        produced,
        reproduced,
        written,
        skipped_writes,
        removed,
        findings,
        ledger,
        drifted,
        timings,
        output_paths,
        certification,
    })
}

/// Run the strict gates that historically lived only on the read-only drift path
/// against the freshly written carrier/tree. Update therefore has the same
/// superset and GMN teeth without executing the pipeline a second time.
fn run_post_update_gates(
    root: &Path,
    gts: &[u8],
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
    let roundtrip = crate::stages::gmn1_gate::check_gmn1_roundtrip(root)?;
    timings.push(TimingRecord {
        phase: "gmn1-roundtrip".to_string(),
        elapsed_ms: started.elapsed().as_millis(),
        metadata: Some(format!("failures={}", roundtrip.failures.len())),
    });
    for failure in roundtrip.failures {
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
    let coverage = crate::stages::gmn1_gate::check_gmn1_construct_coverage(root)?;
    timings.push(TimingRecord {
        phase: "gmn1-construct-coverage".to_string(),
        elapsed_ms: started.elapsed().as_millis(),
        metadata: Some(format!("unexercised={}", coverage.unexercised.len())),
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
            stage: "pipeline".to_string(),
            message:
                "no stage declares sinkCapability; PIPELINE_SPINE §4 requires exactly one terminal writer"
                    .to_string(),
        })),
        _ => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
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
                    stage: "pipeline".to_string(),
                    message: format!(
                        "stage {writer} emits the `{GTS_PATH}` bundle bytes but the declared sink (sinkCapability) is {declared_sink}; PIPELINE_SPINE §4/§7 requires the terminal writer's identity to match the declared sink"
                    ),
                }));
            }
            Ok(())
        }
        0 => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "pipeline".to_string(),
            message: format!(
                "no stage emits the `{GTS_PATH}` bundle bytes; PIPELINE_SPINE §4 requires exactly one terminal writer"
            ),
        })),
        n => Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
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
/// canonicalized quad set as sorted strings. `None` on a parse error.
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
            let set: std::collections::BTreeSet<String> = purrdf::canonicalize(&flat)
                .nquads
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

#[cfg(test)]
mod ledger_integration_tests {
    use super::{CODE_DRIFT, CODE_SUPERSET_MISSING, DiagLedger, attach_pipeline_finding};

    /// F3: the drift/superset producers intern their diagnostics into the carrier
    /// ledger, and the wire findings are the ledger's projection — the ledger is
    /// load-bearing, not a dark parallel path. This exercises the exact helper and
    /// projection `run_full` uses.
    #[test]
    fn drift_findings_flow_through_the_carrier_ledger() {
        let mut ledger = DiagLedger::new();
        attach_pipeline_finding(
            &mut ledger,
            CODE_DRIFT,
            "generated/a.ttl",
            "generated/a.ttl differs from the committed artifact".to_owned(),
        );
        attach_pipeline_finding(
            &mut ledger,
            CODE_SUPERSET_MISSING,
            "generated/b.ttl",
            "generated/b.ttl has no carrier representative in gmeow.gts".to_owned(),
        );

        // Two distinct drifting paths → two distinct content-addressed witnesses
        // (the path is the focus, so a shared code never collapses them).
        assert_eq!(ledger.len(), 2, "each drifting path is a distinct witness");

        // The wire findings `run_full` returns ARE the ledger projection.
        let findings = ledger.project_report("gmeow-pipeline").findings;
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|f| f.code == CODE_DRIFT));
        assert!(findings.iter().any(|f| f.code == CODE_SUPERSET_MISSING));
        // Deleting the fold (an empty ledger) yields zero findings — proving the
        // findings are sourced from the ledger, not a bypass path.
        assert!(
            DiagLedger::new()
                .project_report("gmeow-pipeline")
                .findings
                .is_empty()
        );
    }
}

#[cfg(test)]
mod dag_profile_tests {
    use super::{certify_build_plan, full_spec, write_artifact};
    use gmeow_logic::dag_profile::{DagCertification, certify_acyclic};
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, PreservationClaim,
    };

    /// The dogfooded build plan (`gmeow:pipeline-build`, a `logic:Plan`) is
    /// certified under the DAG-workflow profile (`logic:DagWorkflowResource`): its
    /// producer → consumer dataflow closure is acyclic, so the shared certifier
    /// returns `Certified` and the typed result is complete-for-fragment. This is
    /// the executable witness of "gmeow:Pipeline certified under the profile".
    #[test]
    fn build_dag_certifies_under_the_dag_workflow_profile() {
        let spec = full_spec();
        // Edges in producer → consumer orientation, matching the canonical
        // logic:dataflowConsumes (consumer -> producer) the executor inverts.
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
        assert_eq!(
            cert,
            DagCertification::Certified,
            "the build DAG must certify acyclic under logic:DagWorkflowResource; witness: {:?}",
            cert.witness()
        );
        assert_eq!(
            cert.result_status(),
            (
                EvaluationStatus::Completed,
                CompletenessStatus::CompleteForFragment
            ),
            "an acyclic build plan reports complete-for-fragment under the DAG profile"
        );
    }

    /// The build-pipeline executor hand-off: the build run's typed `ReasoningResult` certification
    /// surface. `certify_build_plan` is the EXACT wiring `run_full` folds into the
    /// returned `RunReport.certification` — it certifies the real `full_spec()`
    /// plan via the shared certifier and lowers the verdict to the typed result a
    /// consumer reads, so this asserts the run's certification field WITHOUT a full
    /// (off-budget) build. The real build is always acyclic, so the verdict is the
    /// certified `Completed` / `CompleteForFragment` result.
    #[test]
    fn build_run_surfaces_a_certified_typed_reasoning_result() {
        let spec = full_spec();
        let cert = certify_build_plan(&spec).expect("the build plan certifies (no cycle)");
        assert_eq!(
            cert.evaluation,
            EvaluationStatus::Completed,
            "the certified build plan's typed result is evaluation=completed"
        );
        assert_eq!(
            cert.completeness,
            CompletenessStatus::CompleteForFragment,
            "the certified build plan's typed result is complete-for-fragment"
        );
        assert_eq!(cert.information, InformationState::Supported);
        // A certified verdict carries an exact (loss-free) preservation claim and no
        // cycle witness — agreeing with the RDF emit_dag_certification mapping.
        assert_eq!(cert.preservation, PreservationClaim::exact());
        assert!(cert.preservation.unsupported_constructs.is_empty());
        assert!(cert.validate().is_ok());
    }

    #[test]
    fn write_artifact_skips_unchanged_bytes_and_rewrites_drift() {
        let dir = tempfile::tempdir().expect("tempdir");

        assert!(
            write_artifact(dir.path(), "generated/sample.txt", b"v1").expect("initial write"),
            "missing file should be written"
        );
        assert_eq!(
            std::fs::read(dir.path().join("generated/sample.txt")).expect("read initial"),
            b"v1"
        );

        assert!(
            !write_artifact(dir.path(), "generated/sample.txt", b"v1").expect("same bytes"),
            "identical bytes should be left untouched"
        );

        assert!(
            write_artifact(dir.path(), "generated/sample.txt", b"v2").expect("changed bytes"),
            "changed bytes should be rewritten"
        );
        assert_eq!(
            std::fs::read(dir.path().join("generated/sample.txt")).expect("read changed"),
            b"v2"
        );
    }
}

/// The runtime one-terminal gate (PIPELINE_SPINE §4): exactly one produced product may
/// carry the `gmeow.gts` byte artifact.
#[cfg(test)]
mod single_writer_gate {
    use super::{GTS_PATH, assert_single_gts_writer};
    use crate::node::StageProduct;
    use std::collections::BTreeMap;

    fn gts_writer(id: &str) -> StageProduct {
        let mut a: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        a.insert(GTS_PATH.to_string(), b"gts-bytes".to_vec());
        StageProduct::from_artifacts(id, a)
    }

    fn plain(id: &str) -> StageProduct {
        let mut a: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        a.insert("generated/other.ttl".to_string(), b"x".to_vec());
        StageProduct::from_artifacts(id, a)
    }

    fn products(items: Vec<StageProduct>) -> BTreeMap<String, StageProduct> {
        items.into_iter().map(|p| (p.stage_id.clone(), p)).collect()
    }

    #[test]
    fn exactly_one_writer_passes() {
        let p = products(vec![gts_writer("stage-gts-sink"), plain("stage-export")]);
        assert!(assert_single_gts_writer(&p, "stage-gts-sink").is_ok());
    }

    #[test]
    fn a_second_writer_is_rejected() {
        let p = products(vec![
            gts_writer("stage-gts-sink"),
            gts_writer("stage-rogue"),
        ]);
        let msg = format!(
            "{}",
            assert_single_gts_writer(&p, "stage-gts-sink").unwrap_err()
        );
        assert!(
            msg.contains("2 stages emit") && msg.contains("exactly one"),
            "a second GTS writer must hard-fail: got {msg}"
        );
        assert!(
            msg.contains("stage-gts-sink") && msg.contains("stage-rogue"),
            "the error must name both offending stages: got {msg}"
        );
    }

    #[test]
    fn no_writer_is_rejected() {
        let p = products(vec![plain("stage-export")]);
        let msg = format!(
            "{}",
            assert_single_gts_writer(&p, "stage-gts-sink").unwrap_err()
        );
        assert!(
            msg.contains("no stage emits"),
            "zero GTS writers must hard-fail: got {msg}"
        );
    }

    #[test]
    fn a_writer_that_is_not_the_declared_sink_is_rejected() {
        // A single writer exists, so the old count-only gate would have passed this —
        // but its stage_id does not match the declared sink (sinkCapability), so the
        // stronger identity gate must reject it as a rogue writer impersonating the
        // terminal (PIPELINE_SPINE §4/§7).
        let p = products(vec![gts_writer("stage-impostor"), plain("stage-export")]);
        let msg = format!(
            "{}",
            assert_single_gts_writer(&p, "stage-gts-sink").unwrap_err()
        );
        assert!(
            msg.contains("stage-impostor") && msg.contains("stage-gts-sink"),
            "the error must name both the actual writer and the declared sink: got {msg}"
        );
    }
}

/// G4/G5: the run ledger is a LOAD-BEARING FORWARD projection of a producer's report
/// findings, attributed to the REAL producing stage. Drives the real `stage-validate`
/// stage over a fixture whose (empty) source graph violates a SHACL shape, folds its
/// FORWARD `StageOutput.diags` into a run ledger the same way the scheduler does, and
/// asserts the SHACL diagnostic reaches the ledger attributed to `stage-validate` (never
/// the synthetic reconcile stage) and projects into the wire findings — via the FORWARD
/// path (`diag_render::finding_nodes`), not the retired backward RDF→ledger read.
#[cfg(test)]
mod diagnostics_ingest_gate {
    use super::PIPELINE_STAGE_ID;
    use crate::node::{Stage, StageInput, StageProduct};

    /// The real diagnostics producer the forward run ledger attributes nodes to.
    const DIAG_PRODUCER_VALIDATE: &str = "stage-validate";
    use crate::stages::source_load::BASE_GRAPH_PATH;
    use crate::stages::validate::ValidateStage;
    use gmeow_errors::DiagLedger;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// A fixture repo carrying one SHACL shape that requires `ex:required` on the
    /// target node `ex:thing` — an empty source graph therefore violates minCount.
    fn violating_repo() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join("shapes/gmeow-shapes.ttl"),
            r#"
@prefix ex: <https://example.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RequiredShape a sh:NodeShape ;
    sh:targetNode ex:thing ;
    sh:property [
        sh:path ex:required ;
        sh:minCount 1 ;
        sh:message "required value is missing" ;
    ] .
"#,
        );
        write(
            &repo.path().join("generated/shapes/frame-shapes.ttl"),
            "# generated\n",
        );
        std::fs::create_dir_all(repo.path().join("slices")).unwrap();
        repo
    }

    /// The repo-relative source path the fixture span index attributes `ex:thing` to.
    const FIXTURE_SPAN_PATH: &str = "slices/x/module.ttl";

    /// A `stage-source-load` product carrying the empty base graph AND a source-span
    /// table mapping the SHACL focus subject `ex:thing` to a source position — the same
    /// blob lane the real source-load stage attaches, so the validate stage's
    /// `span_index()` read (and its finding enrichment) is exercised end-to-end.
    fn source_load_product_with_spans() -> StageProduct {
        use std::sync::Arc;
        let mut source_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        source_artifacts.insert(BASE_GRAPH_PATH.to_owned(), Vec::new());
        let mut spans = crate::ingest::SpanIndex::new();
        spans.insert(
            "https://example.test/thing",
            crate::ingest::SourceSpan::new(Arc::from(FIXTURE_SPAN_PATH), 12, 3, 200),
        );
        let span_blob = serde_json::to_vec(&spans).expect("encode span index");
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            Arc::new(purrdf::RdfDataset::union(&[])),
            source_artifacts,
            purrdf::provenance::DatasetProvenance::new(),
            crate::stages::carrier::REP_SPAN_TABLE,
            "application/json",
            span_blob,
        );
        StageProduct::from_bundle("stage-source-load", Arc::new(bundle))
    }

    /// The four generated-shape producer products the fresh union hard-requires
    /// (`shape_union_fresh::fresh_generated_shape_members`): each carries its
    /// `generated/shapes/*.ttl` member as a comment-only Turtle byte product, the
    /// same lane the real producers attach.
    fn insert_generated_shape_producers(upstream: &mut BTreeMap<String, StageProduct>) {
        let product = |stage: &str, rels: &[&str]| {
            let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
            for rel in rels {
                artifacts.insert((*rel).to_string(), b"# generated\n".to_vec());
            }
            StageProduct::from_artifacts(stage, artifacts)
        };
        upstream.insert(
            "stage-compile-logic".to_owned(),
            product(
                "stage-compile-logic",
                &[
                    crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
                    crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
                ],
            ),
        );
        upstream.insert(
            "stage-export-constraint-shapes".to_owned(),
            product(
                "stage-export-constraint-shapes",
                &[crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH],
            ),
        );
        upstream.insert(
            "stage-export-frame-shapes".to_owned(),
            product(
                "stage-export-frame-shapes",
                &[crate::stages::frame_shapes::FRAME_SHAPES_PATH],
            ),
        );
        upstream.insert(
            "stage-export-result-shapes".to_owned(),
            product(
                "stage-export-result-shapes",
                &[crate::stages::result_shapes::RESULT_SHAPES_PATH],
            ),
        );
        // The validate stage's D5 abductive tier consumes stage-reason's reasoned closure;
        // an empty-EDB reason product yields an empty closure, so the reasoned union is the
        // authored source graph alone (this harness drives SHACL/enrichment, not entailment).
        upstream.insert(
            "stage-reason".to_owned(),
            crate::stages::reason::reason_product(b"").expect("stage-reason fixture product"),
        );
    }

    /// Run the real `stage-validate` stage over the violating fixture, returning its
    /// full output (product + forward diags).
    fn run_validate(repo: &Path) -> crate::node::StageOutput {
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-source-load".to_owned(),
            source_load_product_with_spans(),
        );
        insert_generated_shape_producers(&mut upstream);
        ValidateStage::new()
            .run(StageInput {
                root: repo,
                upstream: &upstream,
            })
            .expect("validate stage")
    }

    /// The source span attached by `source_load_product_with_spans` is LIFTED onto the
    /// real SHACL finding's focus-node location (path + line) and rides into the forward
    /// DiagNode — the proof the span table is genuinely consumed (non-dark).
    #[test]
    fn source_span_is_lifted_onto_the_real_shacl_finding_and_diag_node() {
        let repo = violating_repo();
        let out = run_validate(repo.path());

        // The shipped SHACL JSON projection carries the enriched focus location.
        let json = out
            .product
            .artifact(crate::stages::validate::SHACL_JSON_PATH)
            .expect("shacl.json artifact");
        let report: serde_json::Value = serde_json::from_slice(json).expect("shacl json");
        let location = report["findings"]
            .as_array()
            .and_then(|fs| {
                fs.iter()
                    .find(|f| f["code"].as_str().unwrap_or("").starts_with("shacl."))
            })
            .and_then(|f| f["locations"].as_array())
            .and_then(|ls| ls.first())
            .expect("a SHACL finding with a location");
        assert_eq!(
            location["path"].as_str(),
            Some(FIXTURE_SPAN_PATH),
            "the finding's focus location carries the lifted source path"
        );
        assert_eq!(
            location["line"].as_u64(),
            Some(12),
            "the finding's focus location carries the lifted 1-based line"
        );
        assert_eq!(
            location["logical"].as_str(),
            Some("https://example.test/thing"),
            "the bare-IRI focus join key is preserved"
        );

        // The forward DiagNode carries the same source path (the RDF projection is path +
        // GTS coords only, so line is intentionally lossy on the node — the path is what
        // travels into the run ledger).
        let node = out
            .diags
            .iter()
            .find(|n| n.code.starts_with("shacl."))
            .expect("a forward SHACL DiagNode");
        assert_eq!(
            node.source_ctx.location.path.as_deref(),
            Some(FIXTURE_SPAN_PATH),
            "the lifted source path travels into the forward DiagNode"
        );
    }

    /// Drop-after-last-consumer HARD FAIL: once the span blob is stripped from the
    /// source-load product (as the scheduler does after the last consumer level), any
    /// later `span_index()` read is a typed `SpanTableConsumedAfterDrop` error — and the
    /// stripped product no longer carries the span blob (so it cannot ship).
    #[test]
    fn span_index_hard_fails_after_the_drop_and_is_not_shipped() {
        use std::sync::Arc;
        let product = source_load_product_with_spans();
        // Before the drop the accessor resolves the table.
        assert!(
            product.span_index().is_ok(),
            "span table present before drop"
        );

        // Strip exactly as the scheduler does at the drop point.
        let stripped =
            crate::bundle::strip_rep_blob(product.bundle(), crate::stages::carrier::REP_SPAN_TABLE)
                .expect("strip span blob");
        let dropped = StageProduct::from_bundle("stage-source-load", Arc::new(stripped));

        // Not shipped: the stripped product carries no span blob for the sink to fold.
        assert!(
            crate::bundle::bundle_rep_blob(
                dropped.bundle(),
                crate::stages::carrier::REP_SPAN_TABLE
            )
            .is_none(),
            "the stripped product must not carry the span blob"
        );

        // Reachable HARD FAIL: a later read is the typed SpanTableConsumedAfterDrop.
        let err = dropped
            .span_index()
            .expect_err("span_index must hard-fail after drop");
        assert!(
            err.downcast_ref::<crate::error::SpanTableConsumedAfterDrop>()
                .is_some(),
            "the drop hard-fail must be a typed SpanTableConsumedAfterDrop, got: {err}"
        );
    }

    #[test]
    fn shacl_diagnostic_reaches_the_run_ledger_attributed_to_stage_validate() {
        let repo = violating_repo();
        // Run the REAL validate stage; its FORWARD diags are the single source the
        // scheduler replays into the run ledger.
        let out = run_validate(repo.path());
        assert!(
            !out.diags.is_empty(),
            "the violating fixture must produce forward diagnostic nodes"
        );

        // Fold the forward nodes exactly as the scheduler's commit phase does.
        let mut ledger = DiagLedger::new();
        ledger.replay(out.diags.clone());

        // The ledger node for the SHACL violation is attributed to the REAL producing
        // stage, never the synthetic reconcile stage.
        let nodes = ledger.emit_sorted();
        let shacl = nodes
            .iter()
            .find(|n| n.code.starts_with("shacl."))
            .expect("SHACL diagnostic folded into the run ledger");
        assert_eq!(
            shacl.stage.as_str(),
            DIAG_PRODUCER_VALIDATE,
            "the SHACL diagnostic must be attributed to the real producing stage"
        );
        assert_ne!(
            shacl.stage.as_str(),
            PIPELINE_STAGE_ID,
            "the SHACL diagnostic must NOT be attributed to the synthetic reconcile stage"
        );

        // It projects into RunReport.findings (the ledger is the single source).
        let findings = ledger.findings("gmeow-pipeline");
        assert!(
            findings.iter().any(|f| f.code.starts_with("shacl.")),
            "the forward SHACL diagnostic must project into the wire findings"
        );
    }

    /// The product's `diagnostics:nodes` blob carries EXACTLY the forward `diags` the
    /// stage emitted — the cache lane (which round-trips this blob) recovers the same
    /// run-ledger contribution byte-for-byte on a cache hit.
    #[test]
    fn product_diag_nodes_blob_equals_the_emitted_diags() {
        let repo = violating_repo();
        let out = run_validate(repo.path());
        let from_blob = out.product.diag_nodes();
        assert!(
            !from_blob.is_empty(),
            "the diagnostics:nodes blob must be non-empty"
        );
        assert_eq!(
            from_blob, out.diags,
            "the product blob must byte-equal the emitted forward diags"
        );
    }

    /// Cache-replay byte-identity over a REAL non-empty product: persisting the validate
    /// product to the per-stage cache and re-reading it recovers a `diagnostics:nodes`
    /// blob whose folded run-ledger `emit_sorted()` is BYTE-IDENTICAL to the fresh run
    /// (guarding against a vacuous `[]` blob).
    #[test]
    fn cache_replay_yields_byte_identical_run_ledger() {
        use crate::cache::PipelineCache;

        let repo = violating_repo();
        let out = run_validate(repo.path());
        assert!(
            !out.diags.is_empty(),
            "the fixture must yield a non-empty node set"
        );

        // Fresh-run ledger bytes.
        let fresh = {
            let mut ledger = DiagLedger::new();
            ledger.replay(out.diags.clone());
            serde_json::to_vec(
                &ledger
                    .emit_sorted()
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        };

        // Persist + re-read the product through the real per-stage cache.
        let dir = tempfile::tempdir().unwrap();
        let mut cache = PipelineCache::open(dir.path()).unwrap();
        cache.put("stage-validate", &out.product).unwrap();
        let restored = cache.get("stage-validate").unwrap().expect("cache hit");
        let restored_nodes = restored.diag_nodes();
        assert!(
            !restored_nodes.is_empty(),
            "the cache-restored product must carry a NON-empty diagnostics:nodes blob"
        );

        // Warm-cache ledger bytes.
        let warm = {
            let mut ledger = DiagLedger::new();
            ledger.replay(restored_nodes);
            serde_json::to_vec(
                &ledger
                    .emit_sorted()
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        };
        assert_eq!(
            fresh, warm,
            "a warm-cache run's ledger must be byte-identical to the fresh run"
        );
    }

    /// A fixture repo whose one shape CONFORMS over the empty source graph (minCount 0),
    /// so `stage-validate` produces only the informational `shacl.clean` record.
    fn conforming_repo() -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join("shapes/gmeow-shapes.ttl"),
            r#"
@prefix ex: <https://example.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RequiredShape a sh:NodeShape ;
    sh:targetNode ex:thing ;
    sh:property [
        sh:path ex:required ;
        sh:minCount 0 ;
    ] .
"#,
        );
        write(
            &repo.path().join("generated/shapes/frame-shapes.ttl"),
            "# generated\n",
        );
        std::fs::create_dir_all(repo.path().join("slices")).unwrap();
        repo
    }

    /// A2 golden-delta over the REAL producer: adding/removing a triggering SHACL
    /// violation changes BOTH the shipped `generated/diagnostics/shacl.nq` artifact AND
    /// the run-ledger node set — proving the two are bound to the same producer findings.
    #[test]
    fn violation_delta_changes_both_shacl_rdf_and_run_ledger_nodes() {
        use crate::stages::validate::SHACL_RDF_PATH;

        let violating = run_validate(violating_repo().path());
        let conforming = run_validate(conforming_repo().path());

        // The committed `shacl.nq` artifact differs (a real surface delta).
        assert_ne!(
            violating.product.artifact(SHACL_RDF_PATH),
            conforming.product.artifact(SHACL_RDF_PATH),
            "a triggered SHACL violation must change the shacl.nq artifact"
        );
        // And the run-ledger node set differs: the violation contributes real finding
        // nodes; the conforming run contributes exactly the informational `shacl.clean`
        // record (keeps stage-validate's graph/diagnostics attach delta stable on a
        // clean corpus — a zero-findings validation is a report, not an absence). With
        // the fixed advisory demonstrator removed (greenfield), this candidate-free
        // conforming corpus harvests NO advisory (no accepted logic:CategoryRecommendation
        // candidate is authored in it), so the clean run is exactly the one shacl.clean
        // node. (Harvested advisories surfacing on a candidate-bearing source is proven
        // by the stage test `stage_validate_emits_both_advice_projections`.)
        assert!(
            !violating.diags.is_empty(),
            "the violating run must contribute run-ledger nodes"
        );
        assert_eq!(
            conforming.diags.len(),
            1,
            "the conforming candidate-free run contributes exactly the shacl.clean record \
             (no harvested advisory): {:?}",
            conforming.diags
        );
        assert_eq!(
            conforming.diags[0].grade.severity,
            gmeow_errors::Severity::Info,
            "the conforming run's only node is the informational shacl.clean record"
        );
        assert_ne!(
            violating.diags, conforming.diags,
            "the run-ledger node set must differ with the violation"
        );
    }
}
