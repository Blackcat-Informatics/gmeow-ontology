// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `validate` stage: DAG-native SHACL diagnostics over the loaded source graph.
//!
//! This stage runs the same Rust SHACL engine and the same shape-file union used
//! by `gmeow-dev validate` / the JSON-Schema emitter, but as a first-class
//! pipeline node. It emits deterministic diagnostics projections so the build
//! DAG has an inspectable SHACL product instead of treating validation as an
//! out-of-band Make target only.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gmeow_errors::{DiagLedger, Finding, Report, Severity, StageId};
use gmeow_logic::result_rdf::GRAPH_REASONING;
use purrdf::provenance::DatasetProvenance;
use serde_json::json;

use crate::bundle::PipelineHandle;
use crate::node::{Stage, StageInput, StageOutput, StageProduct, StageRunTiming};
pub mod norm_claims;
mod substrate;

/// Committed JSON projection of the DAG SHACL diagnostics report.
pub const SHACL_JSON_PATH: &str = "generated/diagnostics/shacl.json";
/// Committed SARIF projection of the DAG SHACL diagnostics report.
pub const SHACL_SARIF_PATH: &str = "generated/diagnostics/shacl.sarif";
/// Committed HTML projection of the DAG SHACL diagnostics report.
pub const SHACL_HTML_PATH: &str = "generated/diagnostics/shacl.html";
/// Committed `gmeow:Finding` N-Quads projection of the DAG SHACL diagnostics report.
pub const SHACL_RDF_PATH: &str = "generated/diagnostics/shacl.nq";

/// The `shacl.json` metadata key carrying [`shacl_input_digest`] — the exact input
/// set this stage validated.
///
/// A consumer that reads the RECORDED merged-SHACL verdict instead of re-running the
/// pass MUST recompute this digest over the working tree and hard-fail on absence or
/// mismatch. Absence means the record predates the digest contract and its vintage is
/// unknowable; mismatch means the record describes different bytes than are on disk.
/// Neither is ever a skip and neither is ever a silent pass.
pub const SHACL_INPUT_DIGEST_KEY: &str = "shaclInputDigest";

/// The `shacl.json` metadata key carrying the verdict's fold over ITS OWN recorded
/// content ([`crate::stages::diag_render::record_digest`]).
///
/// [`SHACL_INPUT_DIGEST_KEY`] and this answer two different questions, and neither
/// substitutes for the other: the input digest says WHICH BYTES were validated, this says
/// WHAT VERDICT WAS RECORDED. Hand-deleting a violation from `shacl.json` touches no
/// validated input, so the input digest still matches the working tree exactly — which is
/// precisely why a consumer that gates on the recorded findings must also verify that the
/// findings are the ones the pass produced.
///
/// The stamp is applied by the RENDERER that writes `shacl.json`
/// ([`crate::stages::diag_render::render_diagnostics_artifacts`]'s `seal` argument), not
/// by this stage, so it necessarily attests the bytes that are written rather than an
/// intermediate value; a consumer recomputes it with
/// [`crate::stages::diag_render::verify_record_digest`].
pub const SHACL_RECORD_DIGEST_KEY: &str = "shaclRecordDigest";

/// The canonical digest of everything the merged-SHACL pass consumed: the authored
/// source corpus and the shape union.
///
/// `members` is a sequence of `(repo-relative path, `[`ShaclInputMember`]`)` pairs; the
/// digest sorts them, so a caller may assemble the two halves in any order. Each entry
/// folds its path, its byte length, and its bytes, so a rename, a truncation, and a
/// content edit are all distinguishable. Bytes are supplied BY REFERENCE — a resident
/// member borrows this run's carrier bytes and an on-disk member is read at its turn and
/// released immediately — so the fold's peak residency is ONE member, not the whole
/// validated corpus.
///
/// The SHAPE half is what makes this the drift detector `generated/shapes/*.ttl` needs.
/// `stage-validate` structurally never reads that directory — it validates against THIS
/// run's freshly-produced shape bytes (the deliberate anti-stale-fold law in
/// [`crate::stages::shape_union_fresh`]) — so a consumer comparing its own DISK-read
/// union against this digest is precisely testing whether the committed shape files
/// still equal what the pipeline produced and validated with. That is the one thing the
/// old duplicate whole-corpus SHACL run caught and nothing else did; it is preserved
/// here rather than lost.
///
/// # Errors
/// A member whose bytes must be read from disk and cannot be. A missing input makes the
/// digest meaningless, so it is a hard failure rather than a shorter fold.
pub fn shacl_input_digest(
    mut members: Vec<(String, ShaclInputMember<'_>)>,
) -> Result<String, gmeow_errors::Diag> {
    // Stable sort by path: a caller may assemble the two halves in any order, and two
    // members sharing a path (an authored shape file listed by both halves) keep their
    // assembly order, so the fold is exactly the one the whole-corpus `Vec` produced.
    members.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"gmeow-shacl-input-v1\x1e");
    for (path, member) in &members {
        // ONE member's bytes are resident at a time: an on-disk member is read, folded,
        // and dropped before the next is opened. Materializing the whole corpus first
        // (authored sources + the shape union) peaked at the full byte size of every
        // validated input simultaneously, on top of the already-resident source graph.
        let bytes: std::borrow::Cow<'_, [u8]> = match member {
            ShaclInputMember::Resident(bytes) => std::borrow::Cow::Borrowed(bytes),
            ShaclInputMember::OnDisk(path) => {
                std::borrow::Cow::Owned(std::fs::read(path).map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Parse {
                        message: format!("digesting SHACL input {}: {e}", path.display()),
                    })
                })?)
            }
        };
        hasher.update(path.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        hasher.update(b"\x1e");
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

/// One member of the [`shacl_input_digest`] fold, carried BY REFERENCE so the fold never
/// materializes the whole validated corpus at once.
#[derive(Debug, Clone)]
pub enum ShaclInputMember<'a> {
    /// Bytes already resident in this run's carrier — a shape surface THIS run produced
    /// ([`crate::stages::shape_union_fresh::fresh_generated_shape_members`]). Folding
    /// borrows them; nothing is copied.
    Resident(&'a [u8]),
    /// A file whose bytes the fold reads at its turn and releases immediately after.
    OnDisk(std::path::PathBuf),
}

/// The repo-relative, forward-slashed logical path of `path` under `root` — the key each
/// [`shacl_input_digest`] member folds under.
pub(crate) fn digest_member_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// `paths` as on-disk [`shacl_input_digest`] members, keyed repo-relative.
fn on_disk_members(
    root: &Path,
    paths: Vec<std::path::PathBuf>,
) -> Vec<(String, ShaclInputMember<'static>)> {
    paths
        .into_iter()
        .map(|path| {
            let rel = digest_member_path(root, &path);
            (rel, ShaclInputMember::OnDisk(path))
        })
        .collect()
}

/// The digest of the merged-SHACL input set as it stands ON DISK under `root`: every
/// authored source file plus every member of the committed shape union
/// (`purrdf::shapes::shape_union::shape_files`, `generated/shapes/*.ttl` included).
///
/// This is the value a consumer of the recorded verdict recomputes and compares
/// against the `shaclInputDigest` in `shacl.json`. It reads `generated/shapes/*.ttl`
/// off disk DELIBERATELY — that read is the whole point, since the recorded digest
/// covers the bytes the pipeline actually validated with.
///
/// # Errors
/// If the authored source list or the shape-union file list cannot be built, or any
/// member cannot be read. A missing input makes the digest meaningless, so it is a
/// hard failure rather than a shorter fold.
pub fn on_disk_shacl_input_digest(root: &Path) -> Result<String, gmeow_errors::Diag> {
    let mut members = on_disk_members(root, crate::stages::source_load::authored_files(root)?);
    let shape_files = purrdf::shapes::shape_union::shape_files(root).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("listing the committed shape union: {e}"),
        })
    })?;
    members.extend(on_disk_members(root, shape_files));
    // The substrate A-Box is folded into the validated corpus from these
    // build INPUTS, so the on-disk recompute must fold them too — otherwise a consumer's
    // digest would never match the recorded `shaclInputDigest`.
    members.extend(on_disk_members(
        root,
        crate::stages::substrate_graph::substrate_input_paths(root),
    ));
    shacl_input_digest(members)
}

/// Convert the native SHACL engine report into the canonical diagnostics report.
///
/// Each `ValidationResult` is routed through a [`DiagLedger`] (via
/// [`diag_from_shacl`](gmeow_validate::findings::diag_from_shacl)) rather than
/// hand-built into a `Finding`, so every projected SHACL finding carries the ledger's
/// blake3 `finding_iri` + code-blind `anchor_iri` (`gmeow:findingAnchor`) with
/// `anchor_non_trivial` — the identity the cross-node-glut meta-rule joins on. The
/// findings are the ledger's `project_report` body; the metadata (and the
/// non-conforming-with-no-results fallback) is folded on afterwards.
///
/// `failure_classes` is the enforced shape union's own `gmeow:enforcesFailureClass`
/// index, so every projected finding NAMES the typed conformance failure its law
/// declares instead of carrying only the generic constraint-component code.
fn diagnostics_report(
    report: &purrdf::shapes::report::ValidationReport,
    failure_classes: &gmeow_validate::findings::FailureClassIndex,
) -> Report {
    let mut ledger = DiagLedger::new();
    let stage = StageId::new("stage-validate");
    for result in &report.results {
        ledger.attach(
            gmeow_validate::findings::diag_from_shacl(result, failure_classes),
            stage.clone(),
        );
    }
    let mut out = ledger.project_report("shacl");
    out.metadata.insert("category".to_owned(), json!("shacl"));
    out.metadata
        .insert("stage".to_owned(), json!("stage-validate"));
    out.metadata
        .insert("shaclConforms".to_owned(), json!(report.conforms));
    out.metadata
        .insert("shaclResultCount".to_owned(), json!(report.results.len()));

    if out.findings.is_empty() && !report.conforms {
        out.add_finding(
            Finding::new(
                Severity::Error,
                "shacl.nonconforming",
                "SHACL validation failed: non-conforming with no results",
            )
            .with_tool("shacl"),
        );
    }
    // A clean, conforming run still produced a validation report. Emit one
    // informational record so the diagnostics projection — and therefore this stage's
    // `graph/diagnostics` + `diagnostics:nodes` attach delta — is never empty. The
    // per-stage attach delta must be stable whether or not the corpus carries
    // violations: a zero-findings validation is a report, not an absence (no-optionality
    // / hard-fail — an empty delta would trip the AttachDrift guard). This is the
    // conforming twin of the non-conforming fallback above.
    if out.findings.is_empty() && report.conforms {
        out.add_finding(
            Finding::new(
                Severity::Info,
                "shacl.clean",
                "SHACL validation passed: no findings",
            )
            .with_tool("shacl"),
        );
    }
    out.metadata
        .insert("shaclGatePassed".to_owned(), json!(out.ok()));
    out.metadata
        .insert("shaclErrorCount".to_owned(), json!(out.error_count()));
    out.metadata
        .insert("shaclWarningCount".to_owned(), json!(out.warning_count()));
    out
}

/// Render the four committed SHACL diagnostics projections for a canonical report,
/// through the shared [`crate::stages::diag_render`] renderer (the one path both
/// this stage and `stage-compile-logic` route their reports through).
fn render_artifacts(
    report: Report,
    gate: Option<&crate::stages::gate_verdict::GateProgram>,
    meta: Option<&crate::stages::meta_findings::MetaProgram>,
) -> Result<crate::stages::diag_render::RenderedDiagnostics, gmeow_errors::Diag> {
    crate::stages::diag_render::render_diagnostics_artifacts(
        "stage-validate",
        report,
        &crate::stages::diag_render::DiagnosticsPaths {
            json: SHACL_JSON_PATH,
            sarif: SHACL_SARIF_PATH,
            html: SHACL_HTML_PATH,
            rdf: SHACL_RDF_PATH,
        },
        gate,
        meta,
        // The SHACL verdict is the one diagnostics record a gate reads back instead of
        // re-running (`gmeow-dev validate`'s recorded-merged-SHACL path), so it is sealed.
        Some(SHACL_RECORD_DIGEST_KEY),
    )
}

/// Run SHACL over source-graph N-Quads bytes and return deterministic diagnostics.
///
/// The shape union is loaded through the FRESH loader
/// ([`crate::stages::shape_union_fresh::load_shapes_fresh`]): every
/// `generated/shapes/*.ttl` member's bytes come from `fresh` (THIS run's consumed
/// producer products), never from the previous run's committed files (the
/// stale-disk-fold class).
pub fn validate_source_graph(
    root: &Path,
    source_nquads: &[u8],
    fresh: &BTreeMap<String, Vec<u8>>,
) -> Result<(Report, Vec<gmeow_validate::advisory::Advisory>), gmeow_errors::Diag> {
    let dataset =
        purrdf::parse_dataset(source_nquads, "application/n-quads", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("source graph parse: {e}"),
            })
        })?;
    validate_parsed_source_graph(root, dataset.as_ref(), fresh)
}

/// Validate an already parsed source graph against this run's exact shape union.
///
/// `ValidateStage::run` retains the indexed dataset for its guidance and abductive
/// consumers, so the production path parses the multi-megabyte authored graph once.
/// The byte-oriented public helper above remains available to focused fixtures.
fn validate_parsed_source_graph(
    root: &Path,
    dataset: &purrdf::RdfDataset,
    fresh: &BTreeMap<String, impl AsRef<[u8]>>,
) -> Result<(Report, Vec<gmeow_validate::advisory::Advisory>), gmeow_errors::Diag> {
    let (shape_store, shapes) = crate::stages::shape_union_fresh::load_shapes_fresh(root, fresh)?;
    let mut report = purrdf::shapes::engine::validate_dataset(dataset, &shapes)
        .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))?;
    // The stage calls the engine directly (it needs the shape store for the advisory
    // split), so it applies the same result-set collapse
    // `gmeow_validate::store::shacl_validate_dataset` does: a violation the engine
    // reports once per SPARQL solution is ONE violation in the recorded verdict.
    gmeow_validate::store::dedupe_validation_results(&mut report);
    // The shape union's typed-failure-class annotations, read from the SAME store the
    // shapes were parsed from (this run's fresh product bytes, never the stale disk).
    let failure_classes =
        gmeow_validate::findings::FailureClassIndex::from_shapes_dataset(&shape_store);
    // Split the advisory tier out of the raw results: an Info-severity result comes from a
    // `logic:severity "Info"` advisory constraint, so its raw shacl.* finding is suppressed
    // and it is re-projected as a Note + deonticRecommendation advisory (fires from a DATA
    // MATCH). The shape store carries each advisory shape's `logic:formalizes` provenance; the
    // source `dataset` carries the formalized terms' howToUse/useWhen prose the advisory surfaces.
    let (retained, advisories) =
        gmeow_validate::advisory::split_advisory_results(report, &shape_store, dataset);
    Ok((diagnostics_report(&retained, &failure_classes), advisories))
}

/// The IRI namespace every substrate reconciliation node (component / claim /
/// reconciled pin) is minted under — the aboutness key that isolates the substrate
/// A-Box from the rest of `graph/provenance`.
const SUBSTRATE_IRI_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/substrate/";

/// Extract the substrate reconciliation A-Box from the consumed
/// `stage-source-load` product's `graph/provenance` named graph, as default-graph
/// native RDF statements.
///
/// The A-Box was folded into `graph/provenance` at source-load time
/// ([`crate::stages::substrate_graph::build_substrate_projection`], read from build
/// INPUTS only — non-self-referential), so reading it off the carrier here is a pure
/// keyed fold, NOT a second disk derivation (PIPELINE_SPINE §4). Every substrate node
/// IRI lies under [`SUBSTRATE_IRI_PREFIX`], so the subject filter admits ONLY the A-Box
/// and never the rest of `graph/provenance` — the build-provenance corpus that shares
/// the graph must not enter the SHACL target set (it is not validated today, and folding
/// it would risk minting findings on the normal corpus). An empty provenance graph (a
/// mock-repo fixture with no substrate) yields an empty native dataset.
fn substrate_abox_from_source_load(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Arc<purrdf::RdfDataset>, gmeow_errors::Diag> {
    let product = upstream.get("stage-source-load").ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-validate".to_owned(),
            message: "missing stage-source-load product for the substrate A-Box".to_owned(),
        })
    })?;
    substrate::project(product.bundle().dataset())
}

/// The `stage-validate` pipeline stage.
pub struct ValidateStage {
    consumes: Vec<String>,
    entities: Vec<(String, Vec<String>)>,
}

impl ValidateStage {
    /// Construct the SHACL validation stage. It consumes the native source catalog,
    /// source-load's spans and substrate, plus the four generated-shape producers
    /// ([`crate::stages::shape_union_fresh::GENERATED_SHAPE_PRODUCERS`]), so the
    /// enforced shape union's `generated/shapes/*.ttl` members are THIS run's
    /// product bytes — the authored `shapes/*.ttl` / `slices/*/*/shapes.ttl` half is
    /// read from disk, but the generated members are never (the stale-disk-fold
    /// class).
    ///
    /// Typed dataflow (artifact-level): the `stage-compile-logic` dependency is
    /// narrowed to the complete compiled carrier graphs
    /// ([`crate::stages::compile_logic::CARRIER_GRAPHS`]) — the program-level
    /// digest standing in for the validation-shape byte artifacts this stage
    /// actually reads off that product. The narrowing is what keeps this stage's
    /// `graph/diagnostics` attachment a genuine DELTA (compile-logic's product
    /// carries a graph of the same name); byte-level cache soundness for the
    /// OPT-lifted shape surface is restored by declaring the compiler's non-authored
    /// raw sources in [`Stage::input_files`].
    ///
    /// It also consumes `stage-reason`, narrowed to the single `graph/reasoning`
    /// named graph (the typed Reasoning handle's backing graph, mirroring
    /// [`crate::stages::goal_directed::GoalDirectedStage`]): the D5 abductive tier
    /// reads the REASONED graph, so the stage feeds the producer the union of the
    /// authored source graph and the derived closure read off that handle. The
    /// narrowing is faithful for cache soundness — `graph/reasoning` reifies EVERY
    /// derived axiom, so its digest changes exactly when the closure this stage reads
    /// changes — and it keeps this stage's `graph/diagnostics` attachment a genuine
    /// DELTA (the reason product also carries a `graph/diagnostics`, which the whole
    /// product would fold into this stage's input set and mask the attach).
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-export-constraint-shapes".to_string(),
                "stage-export-frame-shapes".to_string(),
                "stage-export-result-shapes".to_string(),
                "stage-parse-sources".to_string(),
                "stage-reason".to_string(),
                "stage-source-load".to_string(),
            ],
            entities: vec![
                (
                    "stage-compile-logic".to_string(),
                    crate::stages::compile_logic::carrier_entity_list(),
                ),
                (
                    "stage-parse-sources".to_string(),
                    vec![crate::stages::parse_sources::GRAPH_SOURCE_CATALOG.to_string()],
                ),
                (
                    "stage-reason".to_string(),
                    vec![GRAPH_REASONING.to_string()],
                ),
            ],
        }
    }
}

impl Default for ValidateStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ValidateStage {
    fn id(&self) -> &str {
        "stage-validate"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.entities
    }
    fn consumes_span_table(&self) -> bool {
        true
    }
    /// The named graphs this stage attaches to the carrier (its delta), from the
    /// single Rust-side attach table; mirrored by the slice module.ttl gmeow:attachesGraph
    /// declarations and verified against the run-time delta by the scheduler.
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    /// The blob-representation lanes this stage attaches (its delta), from the single
    /// Rust-side attach table; mirrored by gmeow:attachesBlobRep and run-time-verified.
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        // v9: reuse the native catalog and shared compilation; import the substrate
        // through a fixed native role view without any RDF text intermediate.
        // v8: parse the authored source graph once and reuse its indexed dataset for
        // SHACL, guidance enrichment, and the abductive union. The optional substrate
        // A-Box remains validation-only through an explicit dataset union.
        // v6: the advisory tier is HARVESTED — the fixed demonstrator is gone;
        // every ACCEPTED logic:CategoryRecommendation FormalizationCandidate in the
        // source graph projects into a Note finding (→ `graph/diagnostics`) + a
        // `gmeow:ComplianceAssessment` claim (→ `graph/norm-claims`), so the emitted
        // advisory content now depends on the authored candidates (a version bump).
        // v5: emit BOTH wings of the advisory dual-projection unconditionally — the
        // flat Note finding folded into `report` (→ `graph/diagnostics`) AND the
        // materialised `gmeow:ComplianceAssessment` claim in the new `graph/norm-claims`
        // carrier named graph (D4), unioned into this stage's product dataset.
        // v4: the shape union's generated/shapes/*.ttl members are product-sourced
        // from the consumed producer stages (shape_union_fresh) instead of read off
        // disk, so a shape-source edit is ENFORCED (and its diagnostics rendered) in
        // ONE regenerate.
        // v3: attribute each SHACL finding to its DOCUMENTED constrained property (the
        // `sh:path`) via the finding's `documented_terms` carrier, so the docs
        // diagnostics→term join lights up the property's per-term page. Additive: the
        // finding's blake3 identity/anchor and the rendered SARIF/RDF/HTML bytes are
        // unchanged; only the full-fidelity JSON report gains the attribution.
        // v2: lift stage-source-load's source spans onto each SHACL finding's focus-node
        // location (path + line/column) before rendering + the forward diagnostics fold.
        // v7: fold the substrate reconciliation A-Box into the validated
        // corpus so the derived PinAgreement/PinCoverage constraints target it on the
        // production path; the substrate build inputs join the recorded shaclInputDigest.
        // The bump busts the stage cache so the wider corpus is validated on cached inputs.
        "validate.v14-shipped-norm-claims-reasoning"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The AUTHORED half of the shape union only — the GENERATED members are
        // product-sourced off the consumed producer stages (declaring a `generated/`
        // path here would itself be the stale-disk-fold bug class).
        let mut files = crate::stages::shape_union_fresh::authored_shape_files(root)?;
        // The compile-logic dependency is narrowed to the object-level graphs
        // (see `ValidateStage::new`), so the validation-shape BYTE artifacts this
        // stage reads are not covered by that narrowed key leg. Their complete
        // non-authored change basis is the compiler's raw sources below (the
        // authored slice modules are covered by the consumed whole
        // `stage-source-load` product); folding them here keeps the cache key sound
        // byte-for-byte without re-widening the dependency to the whole compile-logic
        // product (which would break this stage's graph/diagnostics attach delta).
        files.push(root.join(crate::stages::compile_logic::OPT_SOURCE_PATH));
        files.push(root.join(crate::stages::compile_logic::OPT_TEST_DATATYPES_PATH));
        files.push(root.join(crate::stages::compile_logic::PATH_SHAPES_EXAMPLE_PATH));
        files.extend(crate::stages::conformance::inference_validation::input_files(root));
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let catalog = crate::stages::parse_sources::catalog(&input)?;
        let fresh = crate::stages::shape_union_fresh::fresh_generated_shape_members(
            self.id(),
            input.upstream,
        )?;
        let constraint_shapes = fresh
            .get(crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: "missing required constraint-shapes product".to_owned(),
                })
            })?;
        let mut source_observations = BTreeMap::new();
        crate::stages::conformance::inference_validation::record(
            input.root,
            catalog,
            constraint_shapes,
            &mut source_observations,
        )?;
        self.validate_input(input, source_observations)
    }
}

impl ValidateStage {
    /// Run validation over explicit native inputs. Authored source contracts are
    /// mandatory in the stage entry; tiny kernel tests supply synthetic inputs.
    fn validate_input(
        &self,
        input: StageInput<'_>,
        mut artifacts: BTreeMap<String, Vec<u8>>,
    ) -> Result<StageOutput, gmeow_errors::Diag> {
        let mut timings = Vec::new();
        let catalog = crate::stages::parse_sources::catalog(&input)?;
        let source_dataset = catalog.materialized();
        let fresh = crate::stages::shape_union_fresh::fresh_generated_shape_members(
            self.id(),
            input.upstream,
        )?;
        let selection_started = Instant::now();
        // Reconcile the substrate A-Box in its validation-only role. The native
        // selection preserves statement tables and keeps unrelated provenance out.
        let substrate_dataset = substrate_abox_from_source_load(input.upstream)?;
        let has_substrate = substrate_dataset.quad_count() != 0
            || substrate_dataset.reifier_quads().next().is_some()
            || substrate_dataset.annotation_quads().next().is_some();
        let validation_dataset = if !has_substrate {
            Arc::clone(source_dataset)
        } else {
            Arc::new(purrdf::RdfDataset::union(&[
                source_dataset.as_ref(),
                substrate_dataset.as_ref(),
            ]))
        };
        timings.push(StageRunTiming {
            phase: "native-validation-selection".to_string(),
            elapsed_ms: selection_started.elapsed().as_millis(),
            metadata: Some(format!(
                "serialized_intermediate_bytes=0;source_quads={};validation_quads={}",
                source_dataset.quad_count(),
                validation_dataset.quad_count(),
            )),
        });
        let shacl_started = Instant::now();
        let (mut report, advisories) =
            validate_parsed_source_graph(input.root, validation_dataset.as_ref(), &fresh)?;
        timings.push(StageRunTiming::new(
            "fresh-shape-union-and-shacl",
            shacl_started.elapsed().as_millis(),
        ));
        let post_validation_started = Instant::now();
        // Record EXACTLY what this pass validated: the authored source corpus, read
        // from disk, plus the effective shape union — authored members from disk and
        // generated members from THIS run's product bytes (never `generated/shapes` off
        // disk, which is the previous run's projection). A consumer that reads this
        // verdict instead of re-running the whole-corpus pass recomputes the same digest
        // over its own DISK view and hard-fails on any difference; that comparison is
        // what carries forward the `generated/shapes` drift detection the duplicate run
        // used to provide.
        {
            let mut members = on_disk_members(
                input.root,
                crate::stages::source_load::authored_files(input.root)?,
            );
            members.extend(crate::stages::shape_union_fresh::effective_union_members(
                input.root, &fresh,
            )?);
            // The validated corpus now also carries the substrate A-Box,
            // derived from these build INPUTS, so they join the recorded digest — a
            // freshness consumer re-deriving over the same disk files (via
            // [`on_disk_shacl_input_digest`]) computes the identical digest. Gated on the
            // A-Box actually being folded: a mock-repo fixture with no substrate carries no
            // A-Box, so the corpus validated no substrate bytes and the digest must not
            // claim (and try to read) substrate inputs that do not exist.
            if has_substrate {
                members.extend(on_disk_members(
                    input.root,
                    crate::stages::substrate_graph::substrate_input_paths(input.root),
                ));
            }
            report.metadata.insert(
                SHACL_INPUT_DIGEST_KEY.to_owned(),
                json!(shacl_input_digest(members)?),
            );
        }
        // Lift the authored source spans onto each SHACL finding whose focus node (a bare
        // IRI in the finding's logical location) matches a span-index entry — the path +
        // 1-based line/column travel onto the SHIPPED finding locations (and, via the
        // forward fold below, into the run-ledger DiagNodes). The span table is read off
        // the consumed stage-source-load product (its SINGLE source; a swappable ingestion
        // adapter produced it), never re-derived here.
        let spans = input
            .upstream
            .get("stage-source-load")
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: "missing stage-source-load product for the source-span table"
                        .to_owned(),
                })
            })?
            .span_index()?;
        crate::ingest::enrich_findings_with_spans(&mut report, &spans);
        // The single proof-carrying enrichment pass (Part 1): populate rule identity
        // (catalog help URIs) onto `report.rules`, then attach the registry-authored
        // remediation prose onto each finding through the annotate-by-fingerprint seam
        // (D1) — resolve each finding's code to the rule catalogue's remediation
        // guidance and hang it on the finding via `DiagLedger::annotate`, so the
        // RENDERED SARIF `fixes` (and CLI/HTML "how to fix" lines) are the genuine
        // product of the annotate API, not a bypass. Shared with the CLI consumer path
        // (`gmeow_validate::data_validate::run`) so the two surfaces cannot drift.
        //
        // The admitted raw catalog also supplies authored per-term guidance.
        // The generated ValidationRule catalog is a later DAG product and is not
        // in this selection; its rule-governing-term joins therefore remain absent.
        gmeow_validate::enrich::enrich_findings(
            &mut report,
            source_dataset.as_ref(),
            source_dataset.as_ref(),
        );
        // Advisory tier: `validate_source_graph` already split the DATA-MATCHED advisory
        // constraints (logic:severity "Info") out of the raw SHACL results — each is an
        // instance whose data matched an anti-pattern guard, re-projected here through BOTH
        // wings of the advisory dual-projection (the raw shacl.* finding was suppressed).
        // This path is UNCONDITIONAL (no early-return between the report build above and
        // here), so advice rides even a non-conforming corpus. A source in which no advisory
        // guard matched yields nothing (honest empty advisory tier + empty norm-claims).
        // Flat wing: fold each graded Note finding into `report` (→ rendered into
        // `graph/diagnostics` below), routed through a `DiagLedger` exactly as
        // `gmeow_validate::advisory`'s own test helper does, so each finding carries
        // genuine ledger identity (finding_iri/anchor), not a hand-built stand-in.
        let mut advisory_ledger = DiagLedger::new();
        let mut advisory_claims = Vec::with_capacity(advisories.len());
        for advisory in &advisories {
            let projection = advisory.project();
            advisory_ledger.attach(projection.diag, StageId::new("validate.advisory"));
            advisory_claims.push(projection.claim);
            report.add_rule(advisory.rule());
        }
        // D5 abductive tier: the constructive "what to ADD" wing. Each corroborated candidate
        // is a WARRANT-as-Finding (attached first so it earns a real fingerprint_iri, its
        // DiagRef captured) plus an advisory whose diag carries a genuine finding→finding
        // `findingAntecedent` to that warrant — so the root-cause meta-fold resolves the warrant
        // join non-DARK (ledger identity), not a bare string. The producer runs the native
        // conjecture engine over an ISOLATED scenario world per candidate; `source_dataset` is
        // only READ, never mutated (nothing is auto-asserted). Both wings ride the SAME
        // advisory dual-projection loop below: flat Note advisory + warrant findings →
        // `graph/diagnostics`, `deonticRecommendation` claim → `graph/norm-claims`.
        // The D5 abductive tier reads the REASONED graph ("asserted OR entailed"), so it is
        // fed the UNION of the authored source graph (its A-Box/TBox individuals + asserted
        // types/relata — without which the schema guards match ZERO subjects) AND the derived
        // closure read off the consumed `stage-reason` product's typed Reasoning handle (the
        // entailed-only types/relata that let the producer catch a subject/relatum only an
        // inference makes true). HARD-fail if the reason product or its Reasoning handle is
        // missing — never a silent fall back to the authored-only graph (the
        // silent-capability-degradation violation): a validate run without its reasoned
        // upstream is an incomplete build, not licence for a weaker abductive pass.
        let reason_product = input.upstream.get("stage-reason").ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: "missing stage-reason product — the D5 abductive tier requires the \
                          reasoned closure (asserted OR entailed), never the authored graph alone"
                    .to_owned(),
            })
        })?;
        let reasoning_entry = reason_product
            .bundle()
            .handle(GRAPH_REASONING)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!(
                        "stage-reason product carries no typed Reasoning handle at \
                         <{GRAPH_REASONING}>"
                    ),
                })
            })?;
        let PipelineHandle::Reasoning(reasoning) = &reasoning_entry.payload else {
            return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("the handle at <{GRAPH_REASONING}> is not the Reasoning arm"),
            }));
        };
        // The live typed handle supplies the derived closure directly, preserving
        // source worlds, literal identity and quoted triples without an RDF round trip.
        let closure_dataset = gmeow_logic::reason::inferred_axioms_to_dataset(
            reasoning.inferred().iter().filter(|axiom| !axiom.is_edb),
        )?;
        let reasoned_dataset = Arc::new(purrdf::RdfDataset::union(&[
            source_dataset.as_ref(),
            closure_dataset.as_ref(),
        ]));
        let abductive_suggestions =
            gmeow_validate::abductive::abductive_advisories(reasoned_dataset.as_ref());
        for suggestion in abductive_suggestions {
            let warrant_ref =
                advisory_ledger.attach(suggestion.warrant, StageId::new("validate.advisory"));
            let projection = suggestion.advisory.project();
            advisory_ledger.attach(
                projection.diag.with_antecedents([warrant_ref]),
                StageId::new("validate.advisory"),
            );
            advisory_claims.push(projection.claim);
            report.add_rule(suggestion.advisory.rule());
        }
        // The flat findings are added after the ledger is fully attached (findings("validate")
        // reads the whole batch), keeping their genuine ledger identity.
        for advisory_finding in advisory_ledger.findings("validate") {
            report.add_finding(advisory_finding);
        }
        // Claim wing: materialise the ComplianceAssessment claims as N-Quads into THEIR
        // OWN carrier named graph (`graph/norm-claims`). The diagnostic renderer
        // publishes its finding graph directly through the native builder below.
        let claim_nq = gmeow_validate::advisory::project_compliance_assessment(
            &advisory_claims,
            crate::stages::carrier::GRAPH_NORM_CLAIMS,
        );
        let claim_dataset = crate::stages::carrier::parse_into_graph(
            claim_nq.as_bytes(),
            "application/n-quads",
            crate::stages::carrier::GRAPH_NORM_CLAIMS,
        )?;
        norm_claims::record(catalog, &claim_dataset, &mut artifacts)?;
        // Both diagnostic folds consume the same full source/import compilation
        // used by compile-logic. Their own explicitly world-scoped finding inputs
        // remain separate from the object-level reasoning EDB.
        let compilation_started = Instant::now();
        let theory = catalog.compiled_logic()?;
        let gate = crate::stages::gate_verdict::GateProgram::from_compiled_theory(&theory)?;
        let meta = crate::stages::meta_findings::MetaProgram::from_compiled_theory(&theory)
            .map_err(|message| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!("diagnostic meta-fold: {message}"),
                })
            })?;
        timings.push(StageRunTiming::new(
            "shared-diagnostic-programs",
            compilation_started.elapsed().as_millis(),
        ));
        // The verdict is SEALED inside `render_artifacts` (the `seal` argument below):
        // the digest of the record's own content is stamped there, after the meta-fold
        // enrichment and as the last mutation before the renderers run, so it attests
        // exactly the findings and metadata the committed `shacl.json` carries. Stamping
        // it HERE instead would digest a value no consumer ever sees — `to_json` writes
        // `Report::normalized()`, which reorders findings and deduplicates rules. A
        // consumer that reads those findings instead of re-running the pass recomputes
        // the digest and refuses a record whose content has been edited since — deleting
        // a violation by hand is a corrupt record, never a clean run.
        // Keep the run ledger's established pre-meta findings. The final native
        // report below includes all enrichment and seals used by the renderers.
        let nodes = crate::stages::diag_render::finding_nodes(&report, self.id());
        let rendered = render_artifacts(report, gate.as_ref(), meta.as_ref())?;
        let diagnostic_report = rendered.report;
        artifacts.extend(rendered.artifacts);
        // Carry the same native publication the terminal RDF artifact projects.
        let diagnostics_dataset = rendered.dataset;
        // UNION the two named-graph datasets so this stage's product bundle carries
        // BOTH `graph/diagnostics` (the flat advisory Note + SHACL findings) AND
        // `graph/norm-claims` (the materialised ComplianceAssessment claim, D4) —
        // one stage product, two carrier destinations from the same advisory event.
        let dataset = Arc::new(purrdf::RdfDataset::union(&[
            diagnostics_dataset.as_ref(),
            claim_dataset.as_ref(),
        ]));
        // FORWARD diagnostics fold: the producer's report findings are the SINGLE source
        // of both the shipped `graph/diagnostics` RDF (above) AND the run-level
        // DiagLedger. Project the findings once to pre-lowered DiagNodes, carry them on
        // the product's `diagnostics:nodes` blob (so a cache hit re-serves them), and
        // hand them up as `StageOutput.diags` for the scheduler to fold on a fresh run.
        let diag_blob = serde_json::to_vec(&nodes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("encode diagnostics nodes blob: {e}"),
            })
        })?;
        let mut bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            artifacts,
            DatasetProvenance::new(),
            crate::stages::carrier::REP_DIAG_NODES,
            "application/json",
            diag_blob,
        );
        crate::bundle::pin_diagnostics(
            &mut bundle,
            self.id(),
            Arc::new(crate::bundle::DiagnosticsPublication::producer(
                crate::bundle::DiagnosticReportOwner::Validate,
                diagnostic_report,
            )?),
        )?;
        Ok(StageOutput {
            product: StageProduct::from_bundle(self.id(), Arc::new(bundle)),
            diags: nodes,
            timings: {
                timings.push(StageRunTiming::new(
                    "post-validation-projections",
                    post_validation_started.elapsed().as_millis(),
                ));
                timings
            },
        })
    }
}

#[path = "validate.tests.rs"]
#[cfg(test)]
mod tests;
