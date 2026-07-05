// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `mappings` stage (P3): compile the alignment artifacts.
//!
//! All mapping artifact families are Rust-owned and wired directly here:
//!   * **SSSOM / FnO / EDOAL / SPARQL CONSTRUCT** → the oxigraph-free
//!     `gmeow-logic-compile` correspondence lowerings, driven by
//!     [`correspondence_lower::lower_all`]. EDOAL + SPARQL lower from one shared get-leg
//!     model, so the `spec-drift` invariant is gone by construction. SSSOM/EDOAL are
//!     byte-identical to the historical emitter; SPARQL/FnO use a deterministic cell
//!     order (content-equal to the historical hash order). Outputs:
//!     `generated/mappings/*.sssom.tsv`, `generated/projections/functions.fno.ttl`,
//!     `generated/projections/*.edoal.ttl`, `generated/queries/*.rq`.
//!   * **Standpoint projections** → `purrdf::slice::emit_standpoint_sets(root, &vocab)` — the
//!     seven hand-authored `standpoint-*.rq` (six peer-model re-expressions:
//!     Standpoint-OWL 2, CRMinf, PROV-O, Web Annotation, schema.org Claim, BBC
//!     News; plus the legacy-modality projection), fixed template-coded SPARQL
//!     with no DSL input → `generated/queries/standpoint-*.rq`.
//!   * **DSL stats** → `purrdf::slice::emit_dsl_stats(root, &vocab)` — the committed,
//!     drift-gated counts summary (equivalences / functions / mapping_sets /
//!     projections / cells_by_set) → `generated/mappings/dsl-stats.json`.
//!
//! Every output is byte-identical to the historical Python driver.

use std::collections::BTreeMap;
use std::path::Path;

use crate::mapping_purity::lint_dsl_mapping_purity;
use gmeow_diagnostics::{Finding, Location, Report, Severity};
use gmeow_logic_compile::projections::report::{build_projection_report_from, ReportHeader};
use gmeow_logic_compile::projections::ProjectionResult;
use purrdf::slice::prefix_emit::{emit_core_prefixes, emit_jsonld_context};
use purrdf::slice::{
    emit_claim_view, emit_dsl_stats, emit_list_functions, emit_standpoint_sets,
    lint_prefix_consistency, CLAIM_VIEW_FILE,
};
use purrdf::RdfSeverity;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::compile_logic::{
    LogicProjectionsChannel, LOGIC_PROJECTIONS_CHANNEL, PROJECTION_REPORT_PATH,
};
use crate::stages::correspondence_lower;

/// Directory (logical-path prefix) of the SSSOM TSV sets.
pub const SSSOM_DIR: &str = "generated/mappings";
/// Committed logical path of the FnO transform catalog.
pub const FNO_PATH: &str = "generated/projections/functions.fno.ttl";
/// Committed logical path of the EmotionML XML projection of the affect vocabulary.
pub const EMOTIONML_PATH: &str = "generated/projections/gmeow-affect.emotionml.xml";
/// Directory (logical-path prefix) of the EDOAL alignment Turtle files.
pub const EDOAL_DIR: &str = "generated/projections";
/// Directory (logical-path prefix) of the SPARQL CONSTRUCT projection queries
/// (also home to the seven standpoint `standpoint-*.rq` projections).
pub const QUERIES_DIR: &str = "generated/queries";
/// Committed logical path of the DSL surface-count summary.
pub const DSL_STATS_PATH: &str = "generated/mappings/dsl-stats.json";
/// Committed logical path of the importable named prefix set (§2).
pub const CORE_PREFIXES_PATH: &str = "generated/projections/core-prefixes.ttl";
/// Committed logical path of the JSON-LD `@context` (§2; replaces the
/// retired Python `jsonld_context.py` builder).
pub const JSONLD_CONTEXT_PATH: &str = "generated/context.jsonld";
/// Committed logical path of the first-class RDF list functions (§5).
pub const LIST_FUNCTIONS_PATH: &str = "generated/projections/list-functions.fno.ttl";

/// The mapping artifacts plus the per-correspondence loss ledger the SSSOM/FnO/EDOAL/
/// SPARQL lowerings produced (the residue set the projection report serializes).
pub struct CompiledMappings {
    /// Every emitted artifact, by logical path.
    pub artifacts: BTreeMap<String, Vec<u8>>,
    /// The per-correspondence loss ledger across all four dialects PLUS the live
    /// `lang:TranslationUnit` corpus rows (one per unit + one per language roll-up).
    pub ledger: Vec<ProjectionResult>,
    /// The live translation-corpus N-Triples graph (`graph/lang-translation-corpus`):
    /// every `.po` catalog pair typed as a `lang:TranslationUnit` carrying a
    /// `logic:Correspondence` with an honestly-computed preservation judgment. Carried
    /// as a named graph by [`MappingsStage::run`], excluded from the reasoned EDB exactly
    /// like the projection-ledger graph.
    pub lang_translation_corpus: Vec<u8>,
    /// The total prose-lift corpus N-Triples graph (`graph/lang-form-corpus`): every
    /// distinct `@x-gmeow-english` source literal interned as a raw `lang:SurfaceForm`
    /// carrying its `logic:candidateSourceHash` and an exact surface-round-trip
    /// `logic:Correspondence`. Carried as a named graph by [`MappingsStage::run`], excluded
    /// from the reasoned EDB exactly like the translation-corpus graph.
    pub lang_form_corpus: Vec<u8>,
}

/// Compile all five mapping families (SSSOM + FnO + EDOAL + SPARQL + standpoint
/// projections) plus the DSL surface-count summary from `root`, returning
/// `{logical_path → bytes}`. The mappings stage is now complete.
pub fn compile_mappings(root: &Path) -> Result<CompiledMappings, PipelineError> {
    let vocab = crate::gmeow_ns::gmeow_slice_vocab();
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // Discover the slice catalog ONCE, here, and share the single in-memory instance across
    // every source-slice consumer in this stage: the correspondence lowerings (Module +
    // Mapping merges) AND the total prose-lift corpus (every `@x-gmeow-english` literal,
    // all roles). Its artifact bytes are resident, so the `slices/` tree is walked once per
    // run — the total-lift universe is a projection of this composed source, never a second
    // independent disk read. `None` only when there is no `slices/` tree.
    let slices_dir = root.join("slices");
    let catalog = if slices_dir.is_dir() {
        Some(
            purrdf::slice::SliceCatalog::discover(
                &slices_dir,
                crate::gmeow_ns::gmeow_slice_vocab(),
            )
            .map_err(|e| PipelineError::Stage {
                stage: "stage-mappings".to_string(),
                message: format!("slice catalog discovery: {e}"),
            })?,
        )
    } else {
        None
    };

    // Prefix-consistency gate (§2): no authored source may shadow a registry
    // prefix with a foreign namespace — a shadow desynchronizes authored CURIEs from
    // the registry-driven shortener. Hard-fail before emitting any artifact
    // (no-optionality); this makes `regenerate` / `check-generated` / `make check`
    // all reject a shadow.
    let prefix_problems =
        lint_prefix_consistency(root, &vocab).map_err(|e| PipelineError::Stage {
            stage: "stage-mappings".to_string(),
            message: format!("prefix-consistency lint failed: {e}"),
        })?;
    if let Some(first) = prefix_problems.first() {
        return Err(PipelineError::Stage {
            stage: "stage-mappings".to_string(),
            message: format!(
                "prefix-consistency: {} registry-prefix shadow(s); first: {}",
                prefix_problems.len(),
                first.message
            ),
        });
    }

    // DSL mapping-purity gate: alignment linkage flows from slices. A
    // `gmeow:TermEquivalence` cell authored under `dsl/mappings/` is a linkage
    // restatement in the wrong place — it must live in the slice that defines its
    // alignSubject. Hard-fail before emitting any artifact (no-optionality); this
    // makes `regenerate` / `check-generated` / `make check` reject a stray cell.
    let purity_problems = lint_dsl_mapping_purity(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("dsl mapping-purity gate failed: {e}"),
    })?;
    if let Some(first) = purity_problems.first() {
        return Err(PipelineError::Stage {
            stage: "stage-mappings".to_string(),
            message: format!(
                "dsl-linkage-purity: {} dsl/mappings file(s) author alignment linkage that must \
                 live in slices; first: {}",
                purity_problems.len(),
                first.message
            ),
        });
    }

    // The four alignment dialects are now produced by the oxigraph-free
    // `gmeow-logic-compile` correspondence lowerings: SSSOM (1:1 lattice band), FnO
    // (transform functions), EDOAL + SPARQL-CONSTRUCT (one shared get leg, so
    // `spec-drift` is gone by construction). One native parse of the DSL + ontology
    // sources drives all four.
    let aligned = correspondence_lower::lower_all(root, catalog.as_ref()).map_err(|e| {
        PipelineError::Stage {
            stage: "stage-mappings".to_string(),
            message: format!("correspondence lowering failed: {e}"),
        }
    })?;
    for (filename, tsv) in aligned.sssom {
        artifacts.insert(format!("{SSSOM_DIR}/{filename}"), tsv.into_bytes());
    }
    artifacts.insert(FNO_PATH.to_string(), canon_fanout_ttl(&aligned.fno)?);
    for (filename, ttl) in aligned.edoal {
        artifacts.insert(format!("{EDOAL_DIR}/{filename}"), ttl.into_bytes());
    }
    for (filename, rq) in aligned.sparql {
        artifacts.insert(format!("{QUERIES_DIR}/{filename}"), rq.into_bytes());
    }
    // The EmotionML XML projection of the affect category + dimension vocabularies. Its
    // many-to-one collapse row already rides in `aligned.ledger` (folded into the union
    // projection-report below), so writing the document is all that remains here.
    artifacts.insert(EMOTIONML_PATH.to_string(), aligned.emotionml.into_bytes());
    let mut ledger = aligned.ledger;

    // Live `lang:TranslationUnit` corpus (Principle 15 consumer wiring): type every
    // multilingual `.po` catalog pair as a first-class crossing carrying a
    // `logic:Correspondence` with an honestly-computed preservation judgment, and fold
    // its per-unit + per-document rows into the loss ledger. The RDF graph is carried as
    // a named graph by the stage `run` below (never a `generated/` file).
    let lang_corpus = crate::stages::lang_translation::build_corpus(root)?;
    ledger.extend(lang_corpus.ledger);
    let lang_translation_corpus = lang_corpus.ntriples;

    // Total prose lift (Gate 1): type every distinct `@x-gmeow-english` source literal as a
    // raw `lang:SurfaceForm` carrying its prose-hash and an exact surface-round-trip
    // `logic:Correspondence`, and fold the single honest corpus row into the loss ledger.
    // The RDF graph rides as a named graph by the stage `run` below (never a `generated/`
    // file), excluded from the reasoned EDB exactly like the translation corpus.
    let form_corpus = crate::stages::lang_form::build_corpus(catalog.as_ref())?;
    ledger.extend(form_corpus.ledger);
    let lang_form_corpus = form_corpus.ntriples;

    // Standpoint projections — the seven fixed `standpoint-*.rq` queries (byte-identical
    // to the Python template-coded emitters; no DSL input).
    let standpoint = emit_standpoint_sets(root, &vocab).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("standpoint emission failed: {e}"),
    })?;
    for (filename, rq) in standpoint {
        artifacts.insert(format!("{QUERIES_DIR}/{filename}"), rq.into_bytes());
    }

    // Observation union view — the internal gmeow→gmeow `observation-claim-view.rq`
    // CONSTRUCT that materialises the legacy Observation / StandpointClaim query
    // surface from the canonical ClaimToken layer (no DSL input).
    artifacts.insert(
        format!("{QUERIES_DIR}/{CLAIM_VIEW_FILE}"),
        emit_claim_view(&vocab).into_bytes(),
    );

    // DSL surface-count summary — the committed, drift-gated counts JSON.
    let dsl_stats = emit_dsl_stats(root, &vocab).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("dsl-stats emission failed: {e}"),
    })?;
    artifacts.insert(DSL_STATS_PATH.to_string(), dsl_stats.into_bytes());

    // Prefix-set projections (§2) — both derived from the single
    // PREFIX_REGISTRY authority: the importable `gmeow:CorePrefixes` SHACL set
    // and the JSON-LD `@context`. Deterministic by construction (const-derived),
    // so they ride the `generated/` drift gate and fold into `gmeow.gts` exactly
    // like the FnO catalog, with no new pipeline stage.
    artifacts.insert(
        CORE_PREFIXES_PATH.to_string(),
        canon_fanout_ttl(&emit_core_prefixes(&vocab))?,
    );
    artifacts.insert(
        JSONLD_CONTEXT_PATH.to_string(),
        emit_jsonld_context(&vocab).into_bytes(),
    );

    // First-class RDF list functions (§5) — six FnO primitives backed by the
    // reasoning layer's recursive rdf:List resolution. Fixed content, deterministic;
    // folds into gmeow.gts like the FnO catalog.
    artifacts.insert(
        LIST_FUNCTIONS_PATH.to_string(),
        canon_fanout_ttl(&emit_list_functions(&vocab))?,
    );

    Ok(CompiledMappings {
        artifacts,
        ledger,
        lang_translation_corpus,
        lang_form_corpus,
    })
}

/// Emit a Turtle RDF projection as EXACTLY the canonical fold (shared prefix
/// authority, no banner) so it rides as an RDF-fanout named graph and the superset
/// gate reconstructs it byte-for-byte.
fn canon_fanout_ttl(body: &str) -> Result<Vec<u8>, PipelineError> {
    canon_fanout_ttl_bytes(body.as_bytes())
}

fn canon_fanout_ttl_bytes(body: &[u8]) -> Result<Vec<u8>, PipelineError> {
    purrdf::turtle_normalize::canonical_turtle(body, &crate::stages::superset::rdf_prefixes())
        .map(String::into_bytes)
        .map_err(|e| PipelineError::Stage {
            stage: "stage-mappings".to_string(),
            message: format!("canonicalize RDF projection: {e}"),
        })
}

/// Assemble the FINAL `generated/logic/projection-report.ttl` over the UNION of the
/// logic projection rows (handed over from compile-logic via [`LogicProjectionsChannel`])
/// and the correspondence-calculus loss ledger.  This is the ONE place the committed
/// report is serialized; it funnels through the single `build_projection_report_from`
/// routine, so the seven whole-program logic rows stay byte-identical — only the added
/// correspondence rows differ.
fn build_union_report(
    header: ReportHeader,
    channel: &LogicProjectionsChannel,
    correspondence_ledger: &[ProjectionResult],
) -> Result<Vec<u8>, PipelineError> {
    let mut rows: Vec<ProjectionResult> = channel.projections.clone();
    rows.extend(correspondence_ledger.iter().cloned());
    let report = build_projection_report_from(header, &rows).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("projection-report assembly: {e}"),
    })?;
    Ok(report.into_bytes())
}

/// Fold the gate-derived 591-term up-projection audit into the report header counts: the
/// `correspondenceCount` becomes the curated production cell PLUS every audited external term;
/// `lawfulUpliftCount` gains the proved tier (round-trip verified) and `claimedUpliftCount`
/// the claimed tier (alignment-asserted, not proved). The audit headline thus becomes a
/// gate-verdict ledger in the canonical loss ledger, not a heuristic bucket count.
///
/// Inputs are gathered the same way the `gmeow up-projection-audit` CLI does: the freshly
/// generated SSSOM (in-memory), the authored projection cells under `root`, and the vendored
/// coverage corpus (`tests/fixtures/coverage/external/{bii,paudley}.ttl`). The corpus is fixed
/// real RDF, so the folded counts are deterministic and ride the `generated/` drift gate.
fn fold_up_projection_audit(
    root: &Path,
    artifacts: &BTreeMap<String, Vec<u8>>,
    mut header: ReportHeader,
) -> Result<ReportHeader, PipelineError> {
    let stage_err = |message: String| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message,
    };

    // The freshly-generated SSSOM (never the on-disk copy, which may be stale this run).
    let sssom_texts: Vec<String> = artifacts
        .iter()
        .filter(|(path, _)| path.starts_with(SSSOM_DIR) && path.ends_with(".sssom.tsv"))
        .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        .collect();

    // The authored projection cells: dsl/mappings/projections/*.ttl + slices/**/mappings/*.ttl.
    let mut projection_paths: Vec<std::path::PathBuf> = Vec::new();
    collect_files_recursive(
        &root.join("dsl").join("mappings").join("projections"),
        &mut projection_paths,
    )?;
    let mut slice_files: Vec<std::path::PathBuf> = Vec::new();
    collect_files_recursive(&root.join("slices"), &mut slice_files)?;
    projection_paths.extend(
        slice_files
            .into_iter()
            .filter(|p| p.components().any(|c| c.as_os_str() == "mappings")),
    );
    projection_paths.retain(|p| p.extension().is_some_and(|e| e == "ttl"));
    projection_paths.sort();
    projection_paths.dedup();
    let projection_ttls: Vec<String> = projection_paths
        .iter()
        .map(|p| std::fs::read_to_string(p).map_err(|e| stage_err(format!("read {p:?}: {e}"))))
        .collect::<Result<_, _>>()?;

    // The vendored coverage corpus, converted Turtle→N-Triples natively.
    let mut corpus_nts: Vec<(String, String)> = Vec::new();
    for name in ["bii", "paudley"] {
        let path = root
            .join("tests")
            .join("fixtures")
            .join("coverage")
            .join("external")
            .join(format!("{name}.ttl"));
        let ttl =
            std::fs::read_to_string(&path).map_err(|e| stage_err(format!("read {path:?}: {e}")))?;
        let nt = crate::up_projection_corpus::ttl_to_nt(&ttl)
            .map_err(|e| stage_err(format!("corpus {name} ttl→nt: {e}")))?;
        corpus_nts.push((name.to_string(), nt));
    }

    let ledger =
        crate::up_projection_gates::gate_derived_audit(&sssom_texts, &projection_ttls, &corpus_nts)
            .map_err(|e| stage_err(format!("gate-derived up-projection audit: {e}")))?;

    header.correspondence_count += ledger.total();
    header.lawful_uplift_count += ledger.totals.proved;
    header.claimed_uplift_count += ledger.totals.claimed;
    Ok(header)
}

/// Compile mappings and fold their diagnostics into the native report.
///
/// This is the Rust-owned implementation behind the Python feedback surface:
/// Python remains the CLI/interface, while compilation, SSSOM validation, and
/// cross-layer projection linting all run through native Rust authorities.
pub fn compile_diagnostics_report(root: &Path) -> Report {
    let mut report = Report::new("mapping-compile");
    let artifacts = match compile_mappings(root) {
        Ok(compiled) => compiled.artifacts,
        Err(err) => {
            add_dsl_error(&mut report, err.to_string());
            return report;
        }
    };

    for (path, bytes) in artifacts
        .iter()
        .filter(|(path, _)| path.ends_with(".sssom.tsv"))
    {
        fold_sssom_findings(&mut report, path, bytes);
    }

    // The seven correspondence-stack soundness checks (the five alignment checks + the two
    // FnO back-end checks, incl. the sole native enforcer of Constitution Principle 5) run
    // through the oxigraph-free native pass
    // (`stages::correspondence_soundness::lint_correspondence_soundness`).
    match crate::stages::correspondence_soundness::lint_correspondence_soundness(root, false) {
        Ok(problems) => {
            for problem in problems {
                let mut finding = Finding::new(
                    match problem.severity.as_str() {
                        "ERROR" => Severity::Error,
                        "WARNING" => Severity::Warning,
                        "INFO" => Severity::Info,
                        _ => Severity::Warning,
                    },
                    format!("mapping-compile.{}", problem.check),
                    problem.message,
                )
                .with_tool("mapping-compile");
                if let Some(instance) = problem.instance {
                    finding.add_location(Location::new(None, None, None, Some(instance)));
                }
                report.add_finding(finding);
            }
        }
        Err(err) => {
            report.add_finding(
                Finding::new(
                    Severity::Warning,
                    "mapping-compile.projection-lint-skipped",
                    format!("projection lint findings not surfaced: {err}"),
                )
                .with_tool("mapping-compile"),
            );
        }
    }

    report
}

fn add_dsl_error(report: &mut Report, message: String) {
    report.add_finding(
        Finding::new(Severity::Error, "mapping-compile.dsl-error", message)
            .with_tool("mapping-compile"),
    );
}

fn fold_sssom_findings(report: &mut Report, path: &str, bytes: &[u8]) {
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            report.add_finding(sssom_finding(
                path,
                None,
                format!("SSSOM artifact is not UTF-8: {err}"),
                "parse",
                "Utf8",
            ));
            return;
        }
    };

    let set = match purrdf::sssom::parse_tsv(text) {
        Ok(set) => set,
        Err(diag) => {
            report.add_finding(sssom_finding(path, None, diag.message, "parse", diag.code));
            return;
        }
    };

    // Structural SSSOM parse failures returned above are already folded into the
    // report; semantic validation diagnostics use the closed RDF severity enum.
    for diag in purrdf::sssom::validate(&set) {
        if diag.severity == RdfSeverity::Error {
            report.add_finding(sssom_finding(
                path,
                diag.instance,
                diag.message,
                diag.check,
                diag.code,
            ));
        }
    }
}

fn sssom_finding(
    path: &str,
    instance: Option<String>,
    message: String,
    check: impl Into<String>,
    code: impl Into<String>,
) -> Finding {
    let mut finding = Finding::new(Severity::Error, "mapping-compile.sssom", message)
        .with_tool("mapping-compile");
    let location = Location::new(Some(path.to_owned()), None, None, instance);
    finding.add_location(location);
    finding.detail = Some(format!("check={} code={}", check.into(), code.into()));
    finding
}

/// Recursively collect every regular file under `dir` into `out` (fail-fast on a
/// `read_dir` entry error — a transient FS error must surface, not silently drop
/// a mapping source). A missing directory yields nothing.
fn collect_files_recursive(
    dir: &Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), PipelineError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files_recursive(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `mappings` pipeline stage — complete: all five mapping families (SSSOM +
/// FnO + EDOAL + SPARQL + standpoint projections) plus the DSL surface-count summary,
/// and the FINAL projection-report loss ledger (logic rows ∪ correspondence rows).
pub struct MappingsStage {
    consumes: Vec<String>,
}

impl MappingsStage {
    /// Construct the stage. It consumes the compile-logic product to obtain the logic
    /// projection rows + report-header counts it unions with the correspondence ledger
    /// when assembling the final `generated/logic/projection-report.ttl`.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-compile-logic".to_string()],
        }
    }
}

impl Default for MappingsStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for MappingsStage {
    fn id(&self) -> &str {
        "stage-mappings"
    }
    fn consumes(&self) -> &[String] {
        // Reads dsl/mappings + slice mapping cells from the root, AND the compile-logic
        // product (the logic projection rows + header counts) so it can assemble the
        // FINAL projection report over the union with the correspondence ledger.
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        // v10: added the dsl mapping-purity gate (alignment linkage must be authored
        // in slices, not dsl/mappings/). Bump busts the stage cache so the new gate
        // runs against existing cached inputs.
        "mappings.v10-dsl-linkage-purity-gate"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, PipelineError> {
        // Raw source read: the alignment artifacts compile from the `dsl/mappings/`
        // tree plus the per-slice mapping cells in the slice modules — none of which
        // any upstream product reflects. The vendored coverage corpus
        // (tests/fixtures/coverage/external/*.ttl) is also a raw source input that
        // feeds the committed audit ledger. Declare them ALL so any edit busts the
        // cache. `consumes() == []` (the leaf reads sources, not upstream products).
        let mut files = Vec::new();
        collect_files_recursive(&root.join("dsl").join("mappings"), &mut files)?;
        files.extend(crate::stages::source_load::module_files(root)?);
        // The slice Mapping cells (slices/**/mappings/*.ttl) are read directly by
        // compile_mappings (correspondence_lower::lower_all merges every
        // ArtifactRole::Mapping artifact) AND by fold_up_projection_audit, yet
        // module_files only declares each slice's module.ttl. Declare every slice
        // `mappings/` .ttl so an edit to a slice equivalence cell busts the cache
        // (a missing edge here silently ignores a guard→combinator correspondence
        // edit until the cache is cleared).
        let mut slice_files: Vec<std::path::PathBuf> = Vec::new();
        collect_files_recursive(&root.join("slices"), &mut slice_files)?;
        files.extend(slice_files.into_iter().filter(|p| {
            p.extension().is_some_and(|e| e == "ttl")
                && p.components().any(|c| c.as_os_str() == "mappings")
        }));
        for name in ["bii", "paudley"] {
            files.push(
                root.join("tests")
                    .join("fixtures")
                    .join("coverage")
                    .join("external")
                    .join(format!("{name}.ttl")),
            );
        }
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let compiled = compile_mappings(input.root)?;
        let mut artifacts = compiled.artifacts;

        // Assemble the FINAL committed projection report over the UNION of the logic
        // projection rows (from compile-logic's in-memory channel) and the
        // correspondence loss ledger. Serialized ONCE through the single routine, so
        // the logic rows stay byte-identical and only correspondence rows are added.
        let channel_bytes = input
            .upstream
            .get("stage-compile-logic")
            .and_then(|p| p.artifact(LOGIC_PROJECTIONS_CHANNEL))
            .ok_or_else(|| PipelineError::Stage {
                stage: "stage-mappings".to_string(),
                message: format!(
                    "missing compile-logic projection channel ({LOGIC_PROJECTIONS_CHANNEL})"
                ),
            })?;
        let channel: LogicProjectionsChannel =
            serde_json::from_slice(channel_bytes).map_err(|e| PipelineError::Stage {
                stage: "stage-mappings".to_string(),
                message: format!("decode logic-projections channel: {e}"),
            })?;
        // Fold the gate-derived 591-term up-projection audit into the curated-cell header
        // counts, so the committed loss ledger carries the gate-verdict liftability statistic
        // . Then canonicalize the report TTL so `projection-report.ttl` is carried as
        // the fold of its named graph (superset gate), not an opaque byte lane.
        let header = fold_up_projection_audit(input.root, &artifacts, channel.header)?;
        let report = build_union_report(header, &channel, &compiled.ledger)?;
        artifacts.insert(
            PROJECTION_REPORT_PATH.to_string(),
            canon_fanout_ttl_bytes(&report)?,
        );

        // Carry the union of the RDF outputs (`.ttl` / `.nq` / `.nt` — the alignment
        // axioms / projections this stage contributes to compose) as the bundle's
        // frozen dataset; the non-RDF outputs (`.json`, `.jsonld`, `.tsv`, `.rq`) stay
        // byte-lane only. Each RDF artifact is parsed and the per-input datasets are
        // unioned (standardize-apart per input), so `gts_compose` folds in this one
        // dataset instead of re-parsing each byte artifact.
        // The default-graph union of the RDF outputs (the alignment axioms `gts_compose`
        // folds default-graph-only) PLUS the projection-report loss ledger re-rooted into
        // the carrier's `graph/projection-ledger` named graph, so the presenter reads the
        // ledger as a pure keyed fold (PIPELINE_SPINE §4) instead of re-parsing the byte
        // artifact. `gts_compose` folds only the default graph, so the named ledger graph
        // never pollutes the composed EDB.
        let rdf_dataset = mappings_rdf_dataset(&artifacts)?;
        let report_ttl =
            artifacts
                .get(PROJECTION_REPORT_PATH)
                .ok_or_else(|| PipelineError::Stage {
                    stage: self.id().to_owned(),
                    message: format!("mappings run omitted {PROJECTION_REPORT_PATH}"),
                })?;
        let ledger_graph = crate::stages::carrier::parse_into_graph(
            &crate::stages::carrier::turtle_to_nquads(report_ttl)?,
            "application/n-quads",
            crate::stages::carrier::GRAPH_PROJECTION_LEDGER,
        )?;
        // graph/alignments — the SSSOM alignment axioms (one triple per data row, CURIEs
        // expanded), built from THIS run's freshly-compiled `generated/mappings/*.sssom.tsv`
        // product artifacts and carried as a named graph so the presenter and the reasoning
        // EDB read it via `producer_graph` (PIPELINE_SPINE §4) instead of source-load
        // re-reading the stale committed SSSOM off disk (the stale-disk-fold class).
        // `gts_compose` folds only the default graph, so this named graph never pollutes
        // the composed EDB (identical to the projection-ledger graph's treatment).
        let alignments_graph = crate::stages::carrier::parse_into_graph(
            &crate::stages::carrier::alignment_nquads_from_artifacts(&artifacts)?,
            "application/n-quads",
            crate::stages::carrier::GRAPH_ALIGNMENTS,
        )?;
        // graph/lang-translation-corpus — the live `lang:TranslationUnit` corpus (every
        // `.po` catalog pair typed as a crossing carrying a `logic:Correspondence`),
        // carried as a named graph so the presenter reads it via `producer_graph`. Like
        // the projection-ledger and alignments graphs it stays OUT of the reasoned EDB
        // (`gts_compose` folds only the default graph, so this named graph never pollutes
        // the composed object-level EDB).
        let lang_translation_graph = crate::stages::carrier::parse_into_graph(
            &compiled.lang_translation_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_LANG_TRANSLATION_CORPUS,
        )?;
        // graph/lang-form-corpus — the total prose lift (Gate 1): every distinct
        // `@x-gmeow-english` source literal typed as a raw `lang:SurfaceForm`. Carried as a
        // named graph so the presenter reads it via `producer_graph`; like the
        // translation-corpus graph it stays OUT of the reasoned EDB (`gts_compose` folds only
        // the default graph).
        let lang_form_graph = crate::stages::carrier::parse_into_graph(
            &compiled.lang_form_corpus,
            "application/n-triples",
            crate::stages::carrier::GRAPH_LANG_FORM_CORPUS,
        )?;
        let dataset = std::sync::Arc::new(purrdf::RdfDataset::union(&[
            rdf_dataset.as_ref(),
            ledger_graph.as_ref(),
            alignments_graph.as_ref(),
            lang_translation_graph.as_ref(),
            lang_form_graph.as_ref(),
        ]));
        Ok(StageOutput {
            product: StageProduct::from_artifacts_over(self.id(), dataset, artifacts),
        })
    }
}

/// Parse every RDF artifact (`.ttl` / `.nq` / `.nt`) of the mappings byte-artifact
/// map and union them into one frozen dataset (the native contribution
/// `gts_compose` folds). Non-RDF artifacts are skipped. Inputs are unioned in
/// sorted-path order (the `BTreeMap` order) under [`RdfDataset::union`], which
/// standardizes blank scopes apart per input and canonicalizes on freeze.
fn mappings_rdf_dataset(
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, PipelineError> {
    let mut parsed: Vec<std::sync::Arc<purrdf::RdfDataset>> = Vec::new();
    for (path, bytes) in artifacts {
        let media_type = if path.ends_with(".nq") {
            "application/n-quads"
        } else if path.ends_with(".nt") {
            "application/n-triples"
        } else if path.ends_with(".ttl") {
            "text/turtle"
        } else {
            continue;
        };
        let ds = purrdf::parse_dataset(bytes, media_type, None)
            .map_err(|e| PipelineError::Parse(format!("mappings RDF parse of {path}: {e}")))?;
        parsed.push(ds);
    }
    let refs: Vec<&purrdf::RdfDataset> = parsed.iter().map(|a| a.as_ref()).collect();
    Ok(std::sync::Arc::new(purrdf::RdfDataset::union(&refs)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::source_load::rdf_bytes_to_dataset;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn triple_set(bytes: &[u8], media_type: &str) -> std::collections::BTreeSet<String> {
        let dataset = rdf_bytes_to_dataset(bytes, media_type, "triple_set").unwrap();
        purrdf::flat_rdf_quads_from_dataset(&dataset)
            .iter()
            .map(|q| {
                // The predicate is a bare IRI string; wrap it as an IRI term so its
                // Display renders `<iri>` — matching the old oxigraph `NamedNode` form
                // the substring assertions key on.
                let predicate = purrdf::RdfTerm::iri(q.predicate.clone());
                format!("{} {} {} .", q.subject, predicate, q.object)
            })
            .collect()
    }

    #[test]
    fn sssom_diagnostics_surface_parse_and_validation_errors() {
        let mut report = Report::new("mapping-compile");
        fold_sssom_findings(
            &mut report,
            "generated/mappings/bad.sssom.tsv",
            b"# mapping_set_id: https://example.org/missing-body\n",
        );
        let parse = report
            .findings
            .iter()
            .find(|finding| finding.detail.as_deref() == Some("check=parse code=sssom-tsv-parse"))
            .expect("parse failure finding");
        assert_eq!(parse.code, "mapping-compile.sssom");
        assert_eq!(
            parse
                .primary_location()
                .and_then(|location| location.path.as_deref()),
            Some("generated/mappings/bad.sssom.tsv")
        );

        let invalid = "\
# mapping_set_id: https://example.org/mapping\n\
# mapping_set_version: 0.1.0\n\
# license: https://creativecommons.org/licenses/by/4.0/\n\
# curie_map:\n\
#   gmeow: https://blackcatinformatics.ca/gmeow/\n\
#   skos: http://www.w3.org/2004/02/skos/core#\n\
#   semapv: https://w3id.org/semapv/vocab/\n\
subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment\n\
nope:Foo\tskos:closeMatch\tgmeow:Bar\tsemapv:ManualMappingCuration\t0.7\tmissing prefix\n";
        fold_sssom_findings(
            &mut report,
            "generated/mappings/prefix.sssom.tsv",
            invalid.as_bytes(),
        );
        let validation = report
            .findings
            .iter()
            .find(|finding| {
                finding.detail.as_deref()
                    == Some("check=PrefixMapCompleteness code=prefix validation")
            })
            .expect("validation failure finding");
        assert_eq!(validation.code, "mapping-compile.sssom");
        assert_eq!(
            validation
                .primary_location()
                .and_then(|location| location.path.as_deref()),
            Some("generated/mappings/prefix.sssom.tsv")
        );
    }

    #[test]
    fn sssom_emits_and_overlaps_byte_identically_with_committed() {
        // The stage drives the oxigraph-free SSSOM correspondence lowering, so for
        // every set it emits that has a committed counterpart, the bytes MUST match
        // exactly (the lowering's parity contract). The total set count vs committed
        // is subject to the committed-vs-local env/staleness drift and is the CI
        // `check-generated` gate, not asserted here.
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile").artifacts;
        let mut overlap = 0usize;
        for (path, bytes) in &artifacts {
            if !path.ends_with(".sssom.tsv") {
                continue;
            }
            if let Ok(committed) = std::fs::read(root.join(path)) {
                assert_eq!(bytes, &committed, "SSSOM {path} drifted from committed");
                overlap += 1;
            }
        }
        assert!(
            overlap >= 60,
            "expected 60+ SSSOM sets byte-matching committed, got {overlap}"
        );
    }

    #[test]
    fn edoal_and_sparql_emit_byte_identically_with_committed() {
        // The stage drives the oxigraph-free EDOAL + SPARQL correspondence lowerings.
        // Every EDOAL `.edoal.ttl` and SPARQL `.rq` the stage emits MUST equal its
        // committed counterpart byte-for-byte (the lowerings' parity contract).
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile").artifacts;
        let mut edoal = 0usize;
        let mut sparql = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for (path, bytes) in &artifacts {
            let name = path.rsplit('/').next().unwrap_or(path);
            let is_edoal = path.starts_with(EDOAL_DIR) && path.ends_with(".edoal.ttl");
            // The per-profile SPARQL projections only; the `standpoint-*.rq`
            // queries and `observation-claim-view.rq` are covered by their own
            // dedicated parity tests below.
            let is_sparql = path.starts_with(QUERIES_DIR)
                && name.ends_with(".rq")
                && !name.starts_with("standpoint-")
                && name != CLAIM_VIEW_FILE;
            if !is_edoal && !is_sparql {
                continue;
            }
            let committed = std::fs::read(root.join(path))
                .unwrap_or_else(|_| panic!("committed missing: {path}"));
            if bytes != &committed {
                let got = String::from_utf8_lossy(bytes);
                let want = String::from_utf8_lossy(&committed);
                let mut detail = String::from("len/content differ");
                for (i, (a, b)) in got.lines().zip(want.lines()).enumerate() {
                    if a != b {
                        detail = format!("line {}: got {a:?} want {b:?}", i + 1);
                        break;
                    }
                }
                failures.push(format!("{path}: {detail}"));
            } else if is_edoal {
                edoal += 1;
            } else {
                sparql += 1;
            }
        }
        assert!(
            failures.is_empty(),
            "EDOAL/SPARQL byte-parity drift:\n{}",
            failures.join("\n")
        );
        assert_eq!(
            edoal, 46,
            "expected 46 EDOAL files byte-matching, got {edoal}"
        );
        assert_eq!(
            sparql, 46,
            "expected 46 SPARQL files byte-matching, got {sparql}"
        );
    }

    #[test]
    fn standpoint_and_dsl_stats_emit_byte_identically_with_committed() {
        // The stage wires `emit_standpoint_sets` / `emit_dsl_stats` — the same Rust
        // the slice-crate byte-parity unit tests exercise. The seven standpoint `.rq`
        // and `dsl-stats.json` the stage emits MUST equal their committed counterparts
        // byte-for-byte (the emitters' parity contract).
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile").artifacts;
        let mut standpoint = 0usize;
        let mut failures: Vec<String> = Vec::new();

        for (path, bytes) in &artifacts {
            let name = path.rsplit('/').next().unwrap_or(path);
            let is_standpoint = path.starts_with(QUERIES_DIR)
                && name.starts_with("standpoint-")
                && name.ends_with(".rq");
            if !is_standpoint {
                continue;
            }
            let committed = std::fs::read(root.join(path))
                .unwrap_or_else(|_| panic!("committed missing: {path}"));
            if bytes != &committed {
                failures.push(path.clone());
            } else {
                standpoint += 1;
            }
        }
        assert!(
            failures.is_empty(),
            "standpoint byte-parity drift:\n{}",
            failures.join("\n")
        );
        assert_eq!(
            standpoint, 7,
            "expected 7 standpoint files byte-matching, got {standpoint}"
        );

        let stats = artifacts
            .get(DSL_STATS_PATH)
            .expect("dsl-stats.json artifact");
        let committed_stats =
            std::fs::read(root.join(DSL_STATS_PATH)).expect("committed dsl-stats.json");
        assert_eq!(
            stats, &committed_stats,
            "dsl-stats.json drifted from committed"
        );
    }

    #[test]
    fn claim_view_emits_byte_identically_with_committed() {
        // The stage wires `emit_claim_view` — the internal observation union view.
        // The emitted `observation-claim-view.rq` MUST equal its committed counterpart
        // byte-for-byte (the emitter's parity contract).
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile").artifacts;
        let path = format!("{QUERIES_DIR}/{CLAIM_VIEW_FILE}");
        let bytes = artifacts.get(&path).expect("claim-view artifact");
        let committed =
            std::fs::read(root.join(&path)).unwrap_or_else(|_| panic!("committed missing: {path}"));
        assert_eq!(bytes, &committed, "claim view drifted from committed");
    }

    #[test]
    fn projection_report_unions_logic_and_correspondence_rows() {
        use crate::node::StageInput;
        use crate::stages::compile_logic::CompileLogicStage;

        let root = repo_root();
        // Run the real compile-logic stage to get the logic-projections channel, then the
        // real mappings stage to assemble the FINAL projection report over the union.
        let compile = CompileLogicStage::new()
            .run(StageInput {
                root: &root,
                upstream: &BTreeMap::new(),
            })
            .expect("compile-logic");
        let mut up: BTreeMap<String, StageProduct> = BTreeMap::new();
        up.insert("stage-compile-logic".to_string(), compile.product);
        let out = MappingsStage::new()
            .run(StageInput {
                root: &root,
                upstream: &up,
            })
            .expect("mappings");
        let report = std::str::from_utf8(
            out.product
                .artifact(PROJECTION_REPORT_PATH)
                .expect("report"),
        )
        .expect("utf8 report");

        // The report carries the correspondence rows (the whole point of the union): at
        // least one row per alignment dialect.
        for dialect in ["sssom:", "fno:", "edoal:", "sparql:"] {
            assert!(
                report.contains(&format!("/target/{dialect}")),
                "projection report missing {dialect} correspondence rows"
            );
        }

        // Byte-stability invariant: dropping every correspondence `ProjectionTarget`
        // block (and its `hasProjection` link) from the freshly assembled report and
        // from the committed report must leave byte-identical logic rows — the
        // correspondence union must never perturb the logic projection. (The committed
        // report itself carries the union, so both sides are filtered the same way.)
        let committed = std::fs::read_to_string(root.join(PROJECTION_REPORT_PATH))
            .expect("committed projection report");
        let is_correspondence = |line: &str| {
            ["sssom:", "fno:", "edoal:", "sparql:"]
                .iter()
                .any(|d| line.contains(&format!("/target/{d}")))
        };
        let strip_correspondence = |text: &str| -> String {
            text.lines()
                .filter(|l| !is_correspondence(l))
                .map(|l| format!("{l}\n"))
                .collect()
        };
        assert_eq!(
            strip_correspondence(report),
            strip_correspondence(&committed),
            "the logic projection rows must be byte-identical between the freshly \
             assembled report and the committed report; only correspondence rows differ"
        );
    }

    #[test]
    fn fno_is_well_formed_ntriples() {
        // Wiring check: the FnO correspondence lowering produces a non-empty FnO
        // catalog that parses. (Committed-byte/iso parity is the CI `check-generated`
        // gate, env-matched.)
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile").artifacts;
        let fno = artifacts.get(FNO_PATH).expect("fno artifact");
        let triples = triple_set(fno, "text/turtle");
        assert!(
            triples.len() > 20,
            "FnO catalog unexpectedly small: {} triples",
            triples.len()
        );
    }

    #[test]
    fn prefix_set_projections_are_emitted_and_parse() {
        // Wiring check (§2): the mappings stage emits the importable prefix
        // set + JSON-LD context, and the Turtle parses with the importable node
        // carrying the generalized sh:declare surface.
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile").artifacts;

        let core = artifacts
            .get(CORE_PREFIXES_PATH)
            .expect("core-prefixes artifact");
        let triples = triple_set(core, "text/turtle");
        // owl:Ontology declaration + at least one sh:declare per registry entry.
        let has_node = triples.iter().any(|t| {
            t.contains("CorePrefixes")
                && t.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                && t.contains("http://www.w3.org/2002/07/owl#Ontology")
        });
        assert!(has_node, "core-prefixes missing owl:Ontology node");
        let declares = triples
            .iter()
            .filter(|t| t.contains("http://www.w3.org/ns/shacl#prefix>"))
            .count();
        assert!(
            declares > 100,
            "expected one sh:prefix per registry entry, got {declares}"
        );

        let ctx = artifacts
            .get(JSONLD_CONTEXT_PATH)
            .expect("context.jsonld artifact");
        let text = std::str::from_utf8(ctx).expect("utf8 context");
        assert!(
            text.contains("\"@context\""),
            "context.jsonld has no @context"
        );
        assert!(text.contains("\"@vocab\""), "context.jsonld has no @vocab");
        assert!(text.ends_with("}\n}\n"), "context.jsonld malformed tail");
    }

    #[test]
    fn list_functions_are_emitted_and_parse() {
        // Wiring check (§5): the mappings stage emits the six list functions
        // as well-formed FnO Turtle (routed through the shared
        // `purrdf::fno::to_quads` serializer, §19 one-path), each typed via
        // fno:Output and fno:Function.
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile").artifacts;
        let lf = artifacts
            .get(LIST_FUNCTIONS_PATH)
            .expect("list-functions artifact");
        let triples = triple_set(lf, "text/turtle");
        let functions = triples
            .iter()
            .filter(|t| t.contains("https://w3id.org/function/ontology#Function"))
            .count();
        assert_eq!(functions, 6, "expected six fno:Function declarations");
        // Primitives are NOT gmeow:ProjectionFunction.
        assert!(
            !triples
                .iter()
                .any(|t| t.contains("https://blackcatinformatics.ca/gmeow/ProjectionFunction")),
            "list functions must not be gmeow:ProjectionFunction"
        );
        // Primitives bind no fno:predicate.
        assert!(
            !triples
                .iter()
                .any(|t| t.contains("<https://w3id.org/function/ontology#predicate>")),
            "list functions must bind no fno:predicate"
        );
        // Each issue-named function is present.
        for name in [
            "listLength",
            "listGet",
            "listIndexOf",
            "listSlice",
            "listConcat",
            "listContains",
        ] {
            assert!(
                triples
                    .iter()
                    .any(|t| t.contains(&format!("gmeow/{name}>"))),
                "missing function {name}"
            );
        }
    }
}
