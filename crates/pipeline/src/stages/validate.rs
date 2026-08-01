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

use gmeow_errors::{DiagLedger, Finding, Report, Severity, StageId};
use gmeow_logic::result_rdf::GRAPH_REASONING;
use purrdf::provenance::DatasetProvenance;
use serde_json::json;

use crate::bundle::PipelineHandle;
use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::source_load::BASE_GRAPH_PATH;

/// Strip a leading `<` / trailing `>` off a reasoned-axiom term string. The EL closure's
/// [`InferredAxiom`](gmeow_logic::reason::el::InferredAxiom) subject/predicate/object are
/// stored as bare or angle-wrapped IRIs; the reason stage's own closure serializer treats
/// them uniformly as IRIs, so this mirrors that when re-projecting the derived rows.
fn bare_iri(value: &str) -> &str {
    value
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(value)
}

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
fn diagnostics_report(report: &purrdf::shapes::report::ValidationReport) -> Report {
    let mut ledger = DiagLedger::new();
    let stage = StageId::new("stage-validate");
    for result in &report.results {
        ledger.attach(
            gmeow_validate::findings::diag_from_shacl(result),
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
    report: &Report,
    gate: Option<&crate::stages::gate_verdict::GateProgram>,
    meta: Option<&crate::stages::meta_findings::MetaProgram>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
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
    // Parse the source graph into the native IR and validate it directly through the
    // native SHACL engine (`validate_dataset`), oxigraph-free.
    let dataset =
        purrdf::parse_dataset(source_nquads, "application/n-quads", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("source graph parse: {e}"),
            })
        })?;
    let (shape_store, shapes) = crate::stages::shape_union_fresh::load_shapes_fresh(root, fresh)?;
    let report = purrdf::shapes::engine::validate_dataset(&dataset, &shapes)
        .map_err(|m| gmeow_errors::Diag::of_kind(crate::error::Parse { message: m }))?;
    // Split the advisory tier out of the raw results: an Info-severity result comes from a
    // `logic:severity "Info"` advisory constraint, so its raw shacl.* finding is suppressed
    // and it is re-projected as a Note + deonticRecommendation advisory (fires from a DATA
    // MATCH). The shape store carries each advisory shape's `logic:formalizes` provenance; the
    // source `dataset` carries the formalized terms' howToUse/useWhen prose the advisory surfaces.
    let (retained, advisories) =
        gmeow_validate::advisory::split_advisory_results(report, &shape_store, &dataset);
    Ok((diagnostics_report(&retained), advisories))
}

/// The `stage-validate` pipeline stage.
pub struct ValidateStage {
    consumes: Vec<String>,
    entities: Vec<(String, Vec<String>)>,
}

impl ValidateStage {
    /// Construct the SHACL validation stage. It consumes the loaded authored source
    /// graph (`stage-source-load`) plus the four generated-shape producers
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
                "stage-reason".to_string(),
                "stage-source-load".to_string(),
            ],
            entities: vec![
                (
                    "stage-compile-logic".to_string(),
                    crate::stages::compile_logic::carrier_entity_list(),
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
        "validate.v6-advice-harvest"
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
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let source_graph = input
            .upstream
            .get("stage-source-load")
            .and_then(|p| p.artifact(BASE_GRAPH_PATH))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!("missing stage-source-load {BASE_GRAPH_PATH} artifact"),
                })
            })?;
        let fresh = crate::stages::shape_union_fresh::fresh_generated_shape_members(
            self.id(),
            input.upstream,
        )?;
        let (mut report, advisories) = validate_source_graph(input.root, source_graph, &fresh)?;
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
        // Per-term usage guidance (Part 3) additionally needs an `&RdfDataset` to scan;
        // this stage runs before `stage-constraint-catalog` (it consumes only
        // `stage-source-load`, a sibling of `stage-reason` in the DAG, not a
        // descendant), so no bundle carrying the generated `gmeow:ValidationRule`
        // catalog is in scope here — the rule-governing-term key honestly resolves
        // to nothing on this path. The authored source graph IS in scope (already
        // consumed above as `source_graph`), so it is parsed once more and passed as
        // BOTH the bundle and the subject: `documented_terms` guidance (prose authored
        // directly on ontology terms) still resolves fully, and the rule-governing-term
        // key stays an honest, structurally-guaranteed absence rather than a fabricated
        // join. No new `consumes()` edge: this is the SAME `stage-source-load` product
        // already declared.
        let source_dataset = purrdf::parse_dataset(source_graph, "application/n-quads", None)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("source graph parse (guidance join): {e}"),
                })
            })?;
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
        // Reconstruct the derived (non-EDB) closure triples off the live handle — the same
        // rows `build_inferred_closure_ttl` serializes, minus the reifier provenance the
        // abductive readers never match. Every EL-closure term is an IRI (the reason stage's
        // `axiom_triple` uses `iri_term` for subject/predicate/object), so a bare N-Triples
        // projection is faithful; the abductive readers query with `GraphMatch::Any`, so the
        // default-graph landing is visible.
        let mut closure_nt = String::new();
        for axiom in reasoning.inferred().iter().filter(|axiom| !axiom.is_edb) {
            use std::fmt::Write as _;
            let _ = writeln!(
                closure_nt,
                "<{}> <{}> <{}> .",
                bare_iri(&axiom.subject),
                bare_iri(&axiom.predicate),
                bare_iri(&axiom.object),
            );
        }
        let closure_dataset =
            purrdf::parse_dataset(closure_nt.as_bytes(), "application/n-triples", None).map_err(
                |e| {
                    gmeow_errors::Diag::of_kind(crate::error::Parse {
                        message: format!("reasoned closure parse (abductive union): {e}"),
                    })
                },
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
        // OWN carrier named graph (`graph/norm-claims`), parsed the same way the SHACL
        // diagnostics RDF is parsed into `graph/diagnostics` below.
        let claim_nq = gmeow_validate::advisory::project_compliance_assessment(
            &advisory_claims,
            crate::stages::carrier::GRAPH_NORM_CLAIMS,
        );
        let claim_dataset = crate::stages::carrier::parse_into_graph(
            claim_nq.as_bytes(),
            "application/n-quads",
            crate::stages::carrier::GRAPH_NORM_CLAIMS,
        )?;
        // Build the reasoner-derived gate-verdict program ONCE from the authored source
        // graph (the base-graph bytes carry the logic + diagnostics slices, hence the
        // authored logic:ruleGateFatalVerdict rule + the gmeow:categoryBlocking wiring).
        // These SHACL findings are the ones that can join the gate-fatal up-set, so their
        // diagnostics graph must carry the DERIVED verdict or gmeow:GateFatalUpsetShape
        // fires under the authored-source `make validate` / stage-validate SHACL pass.
        // A source without the authored rule yields None and the projection stays
        // byte-unchanged (never a faked verdict).
        let gate = crate::stages::gate_verdict::GateProgram::from_source(source_graph);
        // Build the reasoner-derived diagnostic meta-fold from the SAME authored source
        // graph (the base-graph carries the gmeow:DiagnosticMetaRule rules + the
        // gmeow:categoryPolarity wiring). A source without meta-rules yields None and
        // the projection stays byte-unchanged.
        let meta = crate::stages::meta_findings::MetaProgram::from_source(source_graph).map_err(
            |message| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: self.id().to_owned(),
                    message: format!("diagnostic meta-fold: {message}"),
                })
            },
        )?;
        let artifacts = render_artifacts(&report, gate.as_ref(), meta.as_ref())?;
        // Attach the SHACL diagnostics RDF as the carrier's `graph/diagnostics` named
        // graph so the presenter reads it as a pure keyed fold (PIPELINE_SPINE §4) and
        // unions it with the logic-compile diagnostics, never re-parsing the byte
        // artifact. The four committed byte projections are kept on the byte lane.
        let shacl_rdf = artifacts.get(SHACL_RDF_PATH).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("render_artifacts omitted {SHACL_RDF_PATH}"),
            })
        })?;
        let diagnostics_dataset = crate::stages::carrier::parse_into_graph(
            shacl_rdf,
            "application/n-quads",
            crate::stages::carrier::GRAPH_DIAGNOSTICS,
        )?;
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
        let nodes = crate::stages::diag_render::finding_nodes(&report, self.id());
        let diag_blob = serde_json::to_vec(&nodes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("encode diagnostics nodes blob: {e}"),
            })
        })?;
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            artifacts,
            DatasetProvenance::new(),
            crate::stages::carrier::REP_DIAG_NODES,
            "application/json",
            diag_blob,
        );
        Ok(StageOutput {
            product: StageProduct::from_bundle(self.id(), Arc::new(bundle)),
            diags: nodes,
            timings: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn mock_repo(shapes: &str) -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        write(&repo.path().join("shapes/gmeow-shapes.ttl"), shapes);
        write(
            &repo.path().join("generated/shapes/frame-shapes.ttl"),
            "# generated\n",
        );
        std::fs::create_dir_all(repo.path().join("slices")).unwrap();
        repo
    }

    /// The fresh product-byte map covering the mock repo's one generated union
    /// member — `validate_source_graph` fails closed on any on-disk generated
    /// member without a fresh entry (the stale-disk-fold class).
    fn mock_fresh() -> std::collections::BTreeMap<String, Vec<u8>> {
        std::collections::BTreeMap::from([(
            "generated/shapes/frame-shapes.ttl".to_string(),
            b"# generated\n".to_vec(),
        )])
    }

    #[test]
    fn validate_stage_emits_sarif_for_shacl_violation() {
        let repo = mock_repo(
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
        let (report, _adv) =
            validate_source_graph(repo.path(), b"", &mock_fresh()).expect("validate");
        assert_eq!(report.error_count(), 1);
        assert_eq!(
            report.metadata["shaclGatePassed"],
            serde_json::Value::Bool(false)
        );

        let artifacts = render_artifacts(&report, None, None).expect("render");
        let sarif: serde_json::Value =
            serde_json::from_slice(&artifacts[SHACL_SARIF_PATH]).expect("SARIF artifact is JSON");
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(
            sarif["runs"][0]["automationDetails"]["id"],
            serde_json::Value::String("shacl".to_string())
        );
        assert_eq!(
            sarif["runs"][0]["results"][0]["ruleId"],
            serde_json::Value::String("shacl.MinCountConstraintComponent".to_string())
        );
    }

    #[test]
    fn diagnostics_report_finding_carries_ledger_identity_and_nontrivial_anchor() {
        // The G1c production path: `validate_source_graph` → `diagnostics_report` routes
        // the SHACL result through a `DiagLedger`, so the projected finding carries the
        // blake3 `finding_iri` + code-blind `anchor_iri` (with `anchor_non_trivial`) the
        // cross-node-glut meta-rule joins on — NOT the identity-less hand-built finding.
        let repo = mock_repo(
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
        let (report, _adv) =
            validate_source_graph(repo.path(), b"", &mock_fresh()).expect("validate");
        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert!(
            finding
                .finding_iri
                .as_deref()
                .is_some_and(|iri| iri
                    .starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/finding/")),
            "a routed SHACL finding must carry a blake3 finding IRI, not the FNV fallback"
        );
        assert!(
            finding
                .anchor_iri
                .as_deref()
                .is_some_and(|iri| iri
                    .starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/anchor/")),
            "a routed SHACL finding must carry a code-blind anchor IRI"
        );
        assert!(
            finding.anchor_non_trivial,
            "the focus node is a NonTrivial anchor the glut join can fire on"
        );
    }

    /// Task 7 Part C (adversary F1, cross-surface parity/drift guard): the FULL
    /// `ValidateStage::run` (not just `validate_source_graph`, which returns
    /// BEFORE the enrichment call) routes its report through the SAME
    /// `gmeow_validate::enrich::enrich_findings` the CLI/consumer
    /// `data_validate::run` path calls
    /// (`crates/validate/tests/proof_carrying_findings.rs`'s
    /// `cross_surface_parity_cli_path_is_enriched`), so the two consumer surfaces
    /// cannot silently drift apart — the original bug this whole feature fixes.
    /// Falsifiable: removing the `enrich_findings` call at the bottom of
    /// `ValidateStage::run` (this file) makes both assertions below fail.
    #[test]
    fn stage_validate_run_is_enriched_matching_the_cli_path() {
        use purrdf::RdfDatasetBuilder;

        let repo = mock_repo(
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

        // A minimal `stage-source-load` product: an empty base graph (mirrors the
        // existing `validate_source_graph(repo.path(), b"")` fixtures) plus the
        // digest-pinned `REP_SPAN_TABLE` blob every downstream consumer of the
        // span table requires present (`StageProduct::span_index`).
        let dataset = RdfDatasetBuilder::new().freeze().expect("empty dataset");
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(BASE_GRAPH_PATH.to_string(), Vec::new());
        let span_index = crate::ingest::SpanIndex::new();
        let span_blob = serde_json::to_vec(&span_index).expect("encode span index");
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            artifacts,
            DatasetProvenance::new(),
            crate::stages::carrier::REP_SPAN_TABLE,
            "application/json",
            span_blob,
        );
        let product = StageProduct::from_bundle("stage-source-load", Arc::new(bundle));
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert("stage-source-load".to_string(), product);
        // The stage consumes the four shape producers fail-closed (the
        // stale-disk-fold class): every generated union member must arrive as a
        // fresh product byte, so the fixture supplies header-only members.
        for (producer, rels) in [
            (
                "stage-compile-logic",
                &[
                    crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
                    crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
                ][..],
            ),
            (
                "stage-export-constraint-shapes",
                &[crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH][..],
            ),
            (
                "stage-export-frame-shapes",
                &[crate::stages::frame_shapes::FRAME_SHAPES_PATH][..],
            ),
            (
                "stage-export-result-shapes",
                &[crate::stages::result_shapes::RESULT_SHAPES_PATH][..],
            ),
        ] {
            let artifacts: BTreeMap<String, Vec<u8>> = rels
                .iter()
                .map(|rel| ((*rel).to_string(), b"# generated\n".to_vec()))
                .collect();
            upstream.insert(
                producer.to_string(),
                StageProduct::from_artifacts(producer, artifacts),
            );
        }
        // The D5 abductive tier consumes stage-reason's reasoned closure; an empty-EDB
        // fixture yields an empty closure (the reasoned union is the authored graph alone).
        upstream.insert(
            "stage-reason".to_string(),
            crate::stages::reason::reason_product(b"").expect("stage-reason fixture product"),
        );
        let input = StageInput {
            root: repo.path(),
            upstream: &upstream,
        };

        let output = ValidateStage::new().run(input).expect("validate stage run");
        let json_bytes = output
            .product
            .artifact(SHACL_JSON_PATH)
            .expect("shacl.json artifact on the stage product");
        let report: Report =
            serde_json::from_slice(json_bytes).expect("shacl.json parses as a Report");

        assert!(
            !report.rules.is_empty(),
            "ValidateStage::run must populate report.rules (rule_catalog::populate_rules), \
             matching the CLI data_validate::run path"
        );
        let finding = report
            .findings
            .iter()
            .find(|f| f.code == "shacl.MinCountConstraintComponent")
            .expect("the SHACL minCount finding");
        assert!(
            !finding.remediation.is_empty(),
            "the pipeline validate-stage report must carry a remediation, matching the CLI \
             path: {finding:?}"
        );
    }

    /// Build the full `ValidateStage::run` harness — a `stage-source-load` product
    /// with an empty base graph + `REP_SPAN_TABLE` blob, plus header-only members for
    /// the four shape producers — parameterized on the authored `shapes/gmeow-shapes.ttl`
    /// body, and run the stage. Factored out of
    /// `stage_validate_run_is_enriched_matching_the_cli_path` so Task 4's two new tests
    /// reuse the EXACT same harness shape rather than a hand-rolled twin.
    /// The base-graph fixture (N-Quads, default graph): an individual whose data MATCHES
    /// the advisory constraint in `ADVICE_SHAPE` (`ex:badThing a gmeow:Foo`). The
    /// data-matching guard fires exactly one Info result, which the bridge lifts into a
    /// Note advisory + one ComplianceAssessment through the full stage.
    const ADVICE_BASE_NQ: &str = "<https://ex.test/badThing> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Foo> .\n";

    /// An advisory `logic:Constraint` in its projected SHACL form: a `sh:SPARQLConstraint`
    /// at `sh:severity sh:Info` (the advisory tier) carrying `logic:formalizes` (its
    /// provenance), whose guard returns every `gmeow:Foo` instance. It fires against
    /// `ADVICE_BASE_NQ`'s individual, and the bridge re-projects that Info match as a
    /// Note + deonticRecommendation advisory.
    const ADVICE_SHAPE: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
<https://ex.test/FooAdviceShape> a sh:NodeShape ;
    logic:formalizes gmeow:Foo ;
    sh:targetClass gmeow:Foo ;
    sh:sparql [
        a sh:SPARQLConstraint ;
        sh:severity sh:Info ;
        sh:message "prefer a more specific sortal than bare gmeow:Foo" ;
        sh:select "SELECT $this WHERE { $this a <https://blackcatinformatics.ca/gmeow/Foo> }" ;
    ] .
"#;

    fn run_full_stage(base_nq: &str, shapes: &str) -> StageOutput {
        use purrdf::RdfDatasetBuilder;

        let repo = mock_repo(shapes);

        let dataset = RdfDatasetBuilder::new().freeze().expect("empty dataset");
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(BASE_GRAPH_PATH.to_string(), base_nq.as_bytes().to_vec());
        let span_index = crate::ingest::SpanIndex::new();
        let span_blob = serde_json::to_vec(&span_index).expect("encode span index");
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            artifacts,
            DatasetProvenance::new(),
            crate::stages::carrier::REP_SPAN_TABLE,
            "application/json",
            span_blob,
        );
        let product = StageProduct::from_bundle("stage-source-load", Arc::new(bundle));
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert("stage-source-load".to_string(), product);
        for (producer, rels) in [
            (
                "stage-compile-logic",
                &[
                    crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
                    crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
                ][..],
            ),
            (
                "stage-export-constraint-shapes",
                &[crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH][..],
            ),
            (
                "stage-export-frame-shapes",
                &[crate::stages::frame_shapes::FRAME_SHAPES_PATH][..],
            ),
            (
                "stage-export-result-shapes",
                &[crate::stages::result_shapes::RESULT_SHAPES_PATH][..],
            ),
        ] {
            let artifacts: BTreeMap<String, Vec<u8>> = rels
                .iter()
                .map(|rel| ((*rel).to_string(), b"# generated\n".to_vec()))
                .collect();
            upstream.insert(
                producer.to_string(),
                StageProduct::from_artifacts(producer, artifacts),
            );
        }
        // The D5 abductive tier consumes stage-reason's reasoned closure. A fixture with an
        // empty EDB yields an empty closure, so the reasoned union is exactly the authored
        // source graph — this harness exercises the advisory wiring, not entailment.
        upstream.insert(
            "stage-reason".to_string(),
            crate::stages::reason::reason_product(b"").expect("stage-reason fixture product"),
        );
        let input = StageInput {
            root: repo.path(),
            upstream: &upstream,
        };
        ValidateStage::new().run(input).expect("validate stage run")
    }

    /// The GMEOW namespace prefix. `crates/validate/src/advisory.rs`'s `GMEOW`
    /// constant is crate-private, so this trivial namespace string is redeclared
    /// here — the same per-module local-const idiom used across the workspace
    /// (`crates/docs`, `crates/conformance`, …) rather than a shared export.
    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    /// Assert the `graph/norm-claims` graph carries the D4 `gmeow:ComplianceAssessment`
    /// claim shape: exactly one subject typed `gmeow:ComplianceAssessment`, with exactly
    /// one `gmeow:complianceVerdict` and a `gmeow:vantage` of `gmeowBestPractice`, whose
    /// `gmeow:assessedNorm` object carries `gmeow:deonticModality` = `deonticRecommendation`
    /// AND a `gmeow:normIssuer`. Falsifiable: an empty or malformed norm-claims graph
    /// fails every assertion below (not a vacuous existence check).
    fn assert_compliance_assessment_present(ds: &purrdf::RdfDataset) {
        use purrdf::RdfTerm;

        let quads: Vec<_> = ds.owned_quads().collect();

        let assessment_class = format!("{GMEOW}ComplianceAssessment");
        let assessment_subjects: Vec<RdfTerm> = quads
            .iter()
            .filter(|q| {
                q.predicate.as_str() == RDF_TYPE_IRI
                    && matches!(&q.object, RdfTerm::Iri(o) if o == &assessment_class)
            })
            .map(|q| q.subject.clone())
            .collect();
        assert_eq!(
            assessment_subjects.len(),
            1,
            "expected exactly one gmeow:ComplianceAssessment subject in graph/norm-claims, \
             got {assessment_subjects:?}"
        );
        let assessment = &assessment_subjects[0];

        let verdict_pred = format!("{GMEOW}complianceVerdict");
        let verdicts: Vec<_> = quads
            .iter()
            .filter(|q| &q.subject == assessment && q.predicate.as_str() == verdict_pred)
            .collect();
        assert_eq!(
            verdicts.len(),
            1,
            "expected exactly one gmeow:complianceVerdict on the assessment, got {verdicts:?}"
        );

        let vantage_pred = format!("{GMEOW}vantage");
        let best_practice_standpoint =
            RdfTerm::Iri(gmeow_validate::advisory::BEST_PRACTICE_STANDPOINT_IRI.to_owned());
        let vantages: Vec<_> = quads
            .iter()
            .filter(|q| {
                &q.subject == assessment
                    && q.predicate.as_str() == vantage_pred
                    && q.object == best_practice_standpoint
            })
            .collect();
        assert_eq!(
            vantages.len(),
            1,
            "expected exactly one gmeow:vantage = gmeowBestPractice on the assessment, \
             got {vantages:?}"
        );

        let assessed_norm_pred = format!("{GMEOW}assessedNorm");
        let norms: Vec<RdfTerm> = quads
            .iter()
            .filter(|q| &q.subject == assessment && q.predicate.as_str() == assessed_norm_pred)
            .map(|q| q.object.clone())
            .collect();
        assert_eq!(
            norms.len(),
            1,
            "expected exactly one gmeow:assessedNorm on the assessment, got {norms:?}"
        );
        let norm = &norms[0];

        let modality_pred = format!("{GMEOW}deonticModality");
        let deontic_recommendation =
            RdfTerm::Iri(gmeow_validate::advisory::DEONTIC_RECOMMENDATION_IRI.to_owned());
        let modalities: Vec<_> = quads
            .iter()
            .filter(|q| {
                &q.subject == norm
                    && q.predicate.as_str() == modality_pred
                    && q.object == deontic_recommendation
            })
            .collect();
        assert_eq!(
            modalities.len(),
            1,
            "expected the assessedNorm to carry gmeow:deonticModality = deonticRecommendation, \
             got {modalities:?}"
        );

        let issuer_pred = format!("{GMEOW}normIssuer");
        let issuers: Vec<_> = quads
            .iter()
            .filter(|q| &q.subject == norm && q.predicate.as_str() == issuer_pred)
            .collect();
        assert!(
            !issuers.is_empty(),
            "expected the assessedNorm to carry a gmeow:normIssuer, found none"
        );
    }

    /// BOTH advisory wings must ride a CONFORMING run over a base graph
    /// carrying one accepted recommendation candidate. Reuses the full
    /// `ValidateStage::run` harness with a shape module that cannot fire against the
    /// base graph (no `sh:targetNode`/property shape), so the run is genuinely
    /// conforming (`shacl.clean`), and asserts:
    ///  - the report carries a HARVESTED flat advisory finding (`advice.*`, tagged
    ///    `advisory-harvested`) at the Advisory standpoint (routed into
    ///    `graph/diagnostics`), NOT the raw `shacl.*` Info finding (suppressed);
    ///  - the stage product's `graph/norm-claims` carries the materialised
    ///    `gmeow:ComplianceAssessment` claim, in full documented shape.
    ///
    /// Falsifiable: this asserts the actual emitted content, not mere presence.
    #[test]
    fn stage_validate_emits_both_advice_projections() {
        let output = run_full_stage(ADVICE_BASE_NQ, ADVICE_SHAPE);

        let json_bytes = output
            .product
            .artifact(SHACL_JSON_PATH)
            .expect("shacl.json artifact on the stage product");
        let report: Report =
            serde_json::from_slice(json_bytes).expect("shacl.json parses as a Report");
        assert_eq!(
            report.error_count(),
            0,
            "the advisory Info match must NOT gate — a conforming run: {report:?}"
        );

        let advisory_finding = report
            .findings
            .iter()
            .find(|f| {
                f.code.starts_with("advice.") && f.tags.iter().any(|t| t == "advisory-harvested")
            })
            .expect("a harvested advice.* finding must be present when the guard matched");
        assert_eq!(
            advisory_finding.severity,
            gmeow_errors::Severity::Note,
            "the harvested advisory is a Note: {advisory_finding:?}"
        );
        assert_eq!(
            advisory_finding.standpoint,
            Some(gmeow_errors::Standpoint::Advisory),
            "the advisory finding must carry the Advisory standpoint: {advisory_finding:?}"
        );
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.severity == gmeow_errors::Severity::Info
                    && f.code.starts_with("shacl.")
                    && f.code != "shacl.clean"),
            "the raw shacl.* Info constraint finding must be SUPPRESSED (re-projected as the \
             Note; only the informational shacl.clean record may remain): {report:?}"
        );

        let norm_claims = output
            .product
            .dataset()
            .project_named_graph(crate::stages::carrier::GRAPH_NORM_CLAIMS);
        assert_compliance_assessment_present(&norm_claims);
    }

    /// Task 4 (Completion-Adversary F5): the `gmeow:ComplianceAssessment` claim must be
    /// emitted UNCONDITIONALLY — even on a NON-conforming run — because it rides the
    /// same unconditional completion path as the flat advisory Note (never gated behind
    /// `report.conforms`). Reuses the SHACL-violation shape from
    /// `validate_stage_emits_sarif_for_shacl_violation` inside the full `run` harness so
    /// the report genuinely carries a SHACL error. Falsifiable: guarding the emit behind
    /// `if report.conforms` (or any early return before the emit) makes this test fail.
    #[test]
    fn stage_validate_emits_advice_claim_even_when_nonconforming() {
        // Both the advisory Info shape (which the base graph's gmeow:Foo individual matches)
        // AND a hard minCount violation shape, so the run is genuinely non-conforming yet the
        // advisory claim still rides the unconditional completion path.
        let shapes = format!(
            "{ADVICE_SHAPE}\n\
@prefix ex: <https://example.test/> .\n\
ex:RequiredShape a sh:NodeShape ;\n\
    sh:targetNode ex:thing ;\n\
    sh:property [\n\
        sh:path ex:required ;\n\
        sh:minCount 1 ;\n\
        sh:message \"required value is missing\" ;\n\
    ] .\n"
        );
        let output = run_full_stage(ADVICE_BASE_NQ, &shapes);

        let json_bytes = output
            .product
            .artifact(SHACL_JSON_PATH)
            .expect("shacl.json artifact on the stage product");
        let report: Report =
            serde_json::from_slice(json_bytes).expect("shacl.json parses as a Report");
        assert!(
            report.error_count() >= 1,
            "the minCount-violation corpus must be genuinely non-conforming: {report:?}"
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "shacl.MinCountConstraintComponent"),
            "expected the SHACL minCount violation finding: {report:?}"
        );

        let norm_claims = output
            .product
            .dataset()
            .project_named_graph(crate::stages::carrier::GRAPH_NORM_CLAIMS);
        assert_compliance_assessment_present(&norm_claims);
    }

    /// The D5 abductive tier reads the REASONED graph, so `ValidateStage::run` HARD-FAILS
    /// when its `stage-reason` upstream is absent — it never silently falls back to the
    /// authored-only source graph (the silent-capability-degradation violation this fix
    /// forbids). Falsifiable: restoring an authored-graph fallback in place of the
    /// stage-reason `ok_or_else` makes this expect-err assertion fail.
    #[test]
    fn stage_validate_hard_fails_without_the_reasoned_upstream() {
        use purrdf::RdfDatasetBuilder;

        let repo = mock_repo("# no shapes\n");
        let dataset = RdfDatasetBuilder::new().freeze().expect("empty dataset");
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(BASE_GRAPH_PATH.to_string(), Vec::new());
        let span_blob =
            serde_json::to_vec(&crate::ingest::SpanIndex::new()).expect("encode span index");
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            artifacts,
            DatasetProvenance::new(),
            crate::stages::carrier::REP_SPAN_TABLE,
            "application/json",
            span_blob,
        );
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-source-load".to_string(),
            StageProduct::from_bundle("stage-source-load", Arc::new(bundle)),
        );
        // Every generated-shape producer is present, so the stage reaches the abductive
        // tier — but stage-reason is deliberately OMITTED.
        for (producer, rels) in [
            (
                "stage-compile-logic",
                &[
                    crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
                    crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
                ][..],
            ),
            (
                "stage-export-constraint-shapes",
                &[crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH][..],
            ),
            (
                "stage-export-frame-shapes",
                &[crate::stages::frame_shapes::FRAME_SHAPES_PATH][..],
            ),
            (
                "stage-export-result-shapes",
                &[crate::stages::result_shapes::RESULT_SHAPES_PATH][..],
            ),
        ] {
            let artifacts: BTreeMap<String, Vec<u8>> = rels
                .iter()
                .map(|rel| ((*rel).to_string(), b"# generated\n".to_vec()))
                .collect();
            upstream.insert(
                producer.to_string(),
                StageProduct::from_artifacts(producer, artifacts),
            );
        }
        let err = match ValidateStage::new().run(StageInput {
            root: repo.path(),
            upstream: &upstream,
        }) {
            Ok(_) => panic!("validate must hard-fail without stage-reason, never authored-only"),
            Err(e) => e,
        };
        assert!(
            format!("{err:?}").contains("stage-reason"),
            "the hard-fail must name the missing stage-reason upstream: {err:?}"
        );
    }
}
