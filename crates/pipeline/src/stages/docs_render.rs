// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `docs_render` stage: the typed documentation model as data.
//!
//! Pure WIRING of the Rust docs crate — no port. It discovers the
//! `gmeow_docs::DocsModel` from the slice catalog and projects it to the
//! self-hosting documentation named graph via `gmeow_docs::to_gmeow_rdf` — the
//! exact N-Quads the Python `DocSet.to_gmeow_rdf()` folds into `gmeow.gts`. The
//! rendered HTML/Markdown site blobs (`render_site`) are folded by `gts_sink`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_docs::model::{
    ConstraintRule, DiagnosticsDigest, DocDiagFinding, DocsModel, ReasoningVerdict,
};
use gmeow_docs::rdf::to_gmeow_rdf;
use purrdf::RdfTerm;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Logical path of the documentation named graph (N-Quads, in-memory dataflow).
pub const DOCS_GRAPH_PATH: &str = "pipeline/documentation.nq";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

/// Derive the docs [`ReasoningVerdict`] from the `stage-reason` product's inferred
/// closure — the SOLE reasoning pass, never re-run here.
///
/// A class is unsatisfiable exactly when the native DL reasoner entailed
/// `<class> rdfs:subClassOf owl:Nothing` (the same signal
/// [`gmeow_logic::reason::dl::unsatisfiable_from_inferred`] keys on); the ontology
/// is inconsistent exactly when some individual is entailed `rdf:type owl:Nothing`
/// (a witnessed clash). Both are read off the already-carried closure, so no new
/// reasoning runs and no `crates/logic` type grows a field. Shared by the
/// docs-graph stage (here) and the rendered-site archive (`carrier`), so the two
/// surfaces report the SAME verdict. Hard-fails if the closure is absent — never a
/// silent "consistent" default.
pub(crate) fn reasoning_verdict_from_reason(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<ReasoningVerdict, gmeow_errors::Diag> {
    let closure = upstream
        .get("stage-reason")
        .and_then(|p| p.artifact(crate::stages::reason::CLOSURE_PATH))
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "missing stage-reason artifact {} for the reasoning verdict",
                    crate::stages::reason::CLOSURE_PATH
                ),
            })
        })?;
    let dataset = crate::stages::source_load::turtle_bytes_to_dataset(closure, "reason-closure")
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!("parse reasoned closure for the reasoning verdict: {e}"),
            })
        })?;
    let mut unsatisfiable: BTreeSet<String> = BTreeSet::new();
    let mut is_consistent = true;
    for q in dataset.owned_quads() {
        let RdfTerm::Iri(object) = &q.object else {
            continue;
        };
        if object != OWL_NOTHING {
            continue;
        }
        if q.predicate == RDFS_SUBCLASSOF {
            if let RdfTerm::Iri(subject) = &q.subject
                && subject != OWL_NOTHING
            {
                unsatisfiable.insert(subject.clone());
            }
        } else if q.predicate == RDF_TYPE {
            is_consistent = false;
        }
    }
    Ok(ReasoningVerdict {
        is_consistent,
        unsatisfiable,
    })
}

/// Project one full-fidelity `gmeow_errors::Finding` into a [`DocDiagFinding`],
/// resolving `help_uri` ONLY when `finding.code` exactly matches a
/// `ConstraintRule::code` in `by_code` (never a fabricated deep link — mirrors
/// `apply_fixture_catalog_slugs`'s honest-absence contract).
///
/// `category` defaults to `PolicyWarning`'s display spelling when the finding
/// carries no category (mirrors `diagnostics_reader::finding_gate`'s existing
/// non-blocking default) — an honest "uncategorized" rendering, never a hard
/// fail over a field genuinely absent on some findings.
fn doc_diag_finding(
    finding: &gmeow_errors::Finding,
    by_code: &BTreeMap<&str, &str>,
) -> DocDiagFinding {
    DocDiagFinding {
        code: finding.code.clone(),
        severity: finding.severity.as_str().to_string(),
        category: finding
            .category
            .unwrap_or(gmeow_errors::FindingCategory::PolicyWarning)
            .as_str()
            .to_string(),
        message: finding.message.clone(),
        slice_iri: finding.attributions.first().map(|a| a.slice_iri.clone()),
        help_uri: by_code
            .get(finding.code.as_str())
            .map(|uri| (*uri).to_string()),
    }
}

/// Fold the diagnostics→term join [`DiagnosticsDigest`] from the `stage-validate` +
/// `stage-compile-logic` products' committed **JSON** diagnostics artifacts
/// ([`crate::stages::validate::SHACL_JSON_PATH`] /
/// [`crate::stages::compile_logic::DIAG_JSON_PATH`]) — never a re-run of SHACL or
/// the logic compiler (reason/validate-once). This reads the full-fidelity
/// `gmeow_errors::Report` (`gmeow_errors::render::to_json`'s exact wire form), NOT
/// the lossy `diagnostics:nodes` blob: the forward `Finding → DiagNode` fold
/// (`diag_render::finding_nodes`, `rdf_location_lossy`) deliberately drops
/// `location.logical` (and the intermediate `purrdf::RdfDiagnostic` carries no
/// attributions at all) so the diagnostics RDF projection and the run-ledger stay
/// byte-identical — that lane can NEVER carry a term/slice join, no matter how
/// many diagnostics exist. The JSON artifact is a plain `serde` serialization of
/// the `Report`/`Finding` model itself, so `Location.logical` and
/// `Finding.attributions` survive intact. Hard-fails when either declared upstream
/// product/artifact is absent, or when the artifact bytes fail to parse as a
/// `Report` (never a silently empty digest).
///
/// The per-term join key is each finding's FIRST `Location` whose `logical` is
/// `Some` — matched by EXACT string equality against `known_term_iris`, never a
/// heuristic/fuzzy match. A finding whose first logical location names no known
/// term (or that carries no logical location at all) simply has no `by_term`
/// entry (an honest absence, not a bug).
///
/// `by_slice` is keyed on EVERY recorded [`gmeow_errors::DiagnosticAttribution`]
/// (a coarser join, available whenever the finding carries an attribution);
/// `help_uri` resolves through `constraint_rules` by exact `code` match.
pub(crate) fn diagnostics_digest_from_upstream(
    upstream: &BTreeMap<String, StageProduct>,
    known_term_iris: &BTreeSet<String>,
    constraint_rules: &[ConstraintRule],
) -> Result<DiagnosticsDigest, gmeow_errors::Diag> {
    let validate = upstream.get("stage-validate").ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-docs-render".to_string(),
            message: "missing stage-validate product for the diagnostics digest".to_string(),
        })
    })?;
    let compile_logic = upstream.get("stage-compile-logic").ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-docs-render".to_string(),
            message: "missing stage-compile-logic product for the diagnostics digest".to_string(),
        })
    })?;
    let shacl_json = validate
        .artifact(crate::stages::validate::SHACL_JSON_PATH)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "missing stage-validate artifact {} for the diagnostics digest",
                    crate::stages::validate::SHACL_JSON_PATH
                ),
            })
        })?;
    let compile_json = compile_logic
        .artifact(crate::stages::compile_logic::DIAG_JSON_PATH)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "missing stage-compile-logic artifact {} for the diagnostics digest",
                    crate::stages::compile_logic::DIAG_JSON_PATH
                ),
            })
        })?;
    let parse_report = |bytes: &[u8],
                        source: &str|
     -> Result<gmeow_errors::Report, gmeow_errors::Diag> {
        serde_json::from_slice(bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!("parse {source} diagnostics JSON for the diagnostics digest: {e}"),
            })
        })
    };
    let shacl_report = parse_report(shacl_json, crate::stages::validate::SHACL_JSON_PATH)?;
    let compile_report = parse_report(compile_json, crate::stages::compile_logic::DIAG_JSON_PATH)?;

    let findings: Vec<&gmeow_errors::Finding> = shacl_report
        .findings
        .iter()
        .chain(compile_report.findings.iter())
        .collect();
    let total = findings.len();

    let by_code: BTreeMap<&str, &str> = constraint_rules
        .iter()
        .map(|r| (r.code.as_str(), r.help_uri.as_str()))
        .collect();

    let mut by_term: BTreeMap<String, Vec<DocDiagFinding>> = BTreeMap::new();
    let mut by_slice: BTreeMap<String, Vec<DocDiagFinding>> = BTreeMap::new();
    for finding in &findings {
        let doc_finding = doc_diag_finding(finding, &by_code);

        let term_candidate = finding
            .locations
            .iter()
            .find_map(|loc| loc.logical.as_deref());
        if let Some(term_iri) = term_candidate
            && known_term_iris.contains(term_iri)
        {
            by_term
                .entry(term_iri.to_string())
                .or_default()
                .push(doc_finding.clone());
        }

        for attribution in &finding.attributions {
            by_slice
                .entry(attribution.slice_iri.clone())
                .or_default()
                .push(doc_finding.clone());
        }
    }

    Ok(DiagnosticsDigest {
        by_term,
        by_slice,
        total,
    })
}

/// Discover the docs model under `root`, attach the native-reasoner `verdict` and
/// the diagnostics→term join digest (from `upstream`), and project it to the
/// documentation named graph (N-Quads). The verdict is required so the SPARQL
/// surface always carries the per-term reasoning status (never a fabricated
/// default); the diagnostics digest is required (hard-fails on a missing
/// `stage-validate`/`stage-compile-logic` upstream product) so the per-term
/// "Diagnostics you might hit" surface and any `gmeow:doc*` diagnostics
/// projection never fabricate a "no diagnostics" claim. The per-term
/// content-address provenance is read from the committed manifest (self-healing
/// on a term-adding build; see `gmeow_docs::model::DocsModel::discover`).
pub fn render_docs_graph(
    root: &Path,
    verdict: ReasoningVerdict,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<String, gmeow_errors::Diag> {
    let mut model = DocsModel::discover(root).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-docs-render".to_string(),
            message: format!("docs model discovery failed: {e}"),
        })
    })?;
    model.attach_reasoning(verdict);
    let known_term_iris: BTreeSet<String> = model.terms.iter().map(|t| t.iri.clone()).collect();
    let diagnostics =
        diagnostics_digest_from_upstream(upstream, &known_term_iris, &model.constraint_rules)?;
    model.attach_diagnostics(diagnostics);
    Ok(to_gmeow_rdf(&model))
}

/// Recursively collect every regular file under `dir` into `out` (fail-fast on a
/// `read_dir` entry error; a missing directory yields nothing).
fn walk_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), gmeow_errors::Diag> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Every raw source file `gmeow_docs::DocsModel::discover` reads: slice modules,
/// per-slice `docs.md` guides, slice `examples/*.ttl`, `docs/four-boxes.md`,
/// per-slice `i18n/<lang>.po` gettext translation catalogs, per-slice
/// `shapes.ttl` SHACL constraint files, per-slice `tests/competency.ttl`
/// competency-question overlays, per-slice `tests/conformance-fixtures/*.ttl` /
/// `tests/counter-examples/*.ttl` Do/Don't fixtures + their
/// `tests/example-conformance.ttl` binding overlay, per-slice `queries/*` SPARQL
/// files (a `gmeow:cqQueryFile` may resolve into a slice's own `queries/competency/`
/// tree) plus the shared repo-root `queries/*` tree (the same `cqQueryFile` value
/// may instead point at a root-level shared query, e.g. `queries/competency/…` or
/// `queries/qc/…` — both forms are repo-root-relative, mirroring
/// `crates/slicetest/src/paths.rs::query_file`'s own resolution contract), and
/// root `shapes/*.ttl` aggregate node shapes. These are NOT reflected in the
/// composed `stage-gts-compose` product (guide bodies ride the bundle only as
/// blake3 digests), so any stage that derives an artifact from the docs model
/// must declare them as `input_files` for cache soundness. Shared by
/// `DocsRenderStage` (the documentation graph) and `SnapshotStage` (the
/// embedded rendered site).
pub(crate) fn docs_source_files(
    root: &Path,
) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for module in crate::stages::source_load::module_files(root)? {
        let dir = module.parent().unwrap_or(root);
        files.push(module.clone());
        let docs = dir.join("docs.md");
        if docs.is_file() {
            files.push(docs);
        }
        let shapes = dir.join("shapes.ttl");
        if shapes.is_file() {
            files.push(shapes);
        }
        let competency = dir.join("tests").join("competency.ttl");
        if competency.is_file() {
            files.push(competency);
        }
        // Conformance Do/Don't fixtures: the well-formed instances / counter-
        // examples themselves plus the binding overlay that joins them to an
        // expected outcome / violation code / rationale.
        let example_conformance = dir.join("tests").join("example-conformance.ttl");
        if example_conformance.is_file() {
            files.push(example_conformance);
        }
        for fixture_dir in ["conformance-fixtures", "counter-examples"] {
            if let Ok(entries) = std::fs::read_dir(dir.join("tests").join(fixture_dir)) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|x| x == "ttl") {
                        files.push(p);
                    }
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("examples")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|x| x == "ttl") {
                    files.push(p);
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("i18n")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|x| x == "po") {
                    files.push(p);
                }
            }
        }
        // Notation grammars: the first-class W3C EBNF renderings of the
        // project's own serialization surface syntaxes (`gmeow_docs::model::
        // DocGrammar`), authored under `slices/grounding/lang/grammars/*.ebnf`
        // today, but discovered generically per-slice.
        if let Ok(entries) = std::fs::read_dir(dir.join("grammars")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|x| x == "ebnf") {
                    files.push(p);
                }
            }
        }
        // A slice's own `queries/` tree (typically `queries/competency/*.rq`) — a
        // `gmeow:cqQueryFile` this slice's `competency.ttl` declares may resolve here.
        walk_files(&dir.join("queries"), &mut files)?;
    }
    let four_boxes = root.join("docs").join("four-boxes.md");
    if four_boxes.is_file() {
        files.push(four_boxes);
    }
    // The committed term content manifest: the docs model reads it for each term's
    // content digest, first-seen version, and computed changelog, so a manifest edit
    // must bust the docs cache (cache soundness).
    let term_manifest = root.join(crate::stages::term_manifest::TERM_MANIFEST_RDF_PATH);
    if term_manifest.is_file() {
        files.push(term_manifest);
    }
    walk_files(&root.join("i18n"), &mut files)?;
    walk_files(&root.join("shapes"), &mut files)?;
    // The shared repo-root query tree (`queries/competency/*.rq`, `queries/qc/*.rq`,
    // …) — many `gmeow:cqQueryFile` values resolve here rather than into a slice's
    // own directory (both forms are repo-root-relative; see the doc comment above).
    walk_files(&root.join("queries"), &mut files)?;
    files.sort();
    files.dedup();
    Ok(files)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `docs_render` pipeline stage.
pub struct DocsRenderStage {
    consumes: Vec<String>,
}

impl DocsRenderStage {
    /// Construct the stage. It discovers the docs model from the slice catalog at
    /// the root and consumes `stage-reason` so the projected documentation graph
    /// carries the per-term native-reasoner status (`gmeow:docReasoningStatus`), and
    /// `stage-validate` + `stage-compile-logic` so it carries the diagnostics→term
    /// join digest (the term page's "Diagnostics you might hit" surface).
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-gts-compose".to_string(),
                "stage-reason".to_string(),
                "stage-validate".to_string(),
            ],
        }
    }
}

impl Default for DocsRenderStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for DocsRenderStage {
    fn id(&self) -> &str {
        "stage-docs-render"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        // v5: `diagnostics_digest_from_upstream` now folds from the `stage-validate`
        // + `stage-compile-logic` products' full-fidelity JSON diagnostics artifacts
        // (`SHACL_JSON_PATH` / `DIAG_JSON_PATH`) rather than the lossy
        // `diagnostics:nodes` blob, so `by_term`/`by_slice` join against genuine
        // `Location.logical`/`Finding.attributions` data. Bumped so the cache
        // re-derives the digest after the data-source change.
        "docs_render.v5"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The raw-source half of this DocsRender leaf — declared so a guide /
        // four-boxes / per-slice i18n catalog edit busts the cache (cache soundness).
        // The snapshot stage embeds the rendered SITE from these same sources,
        // so it shares this list via `docs_source_files`.
        docs_source_files(root)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let verdict = reasoning_verdict_from_reason(input.upstream)?;
        let graph = render_docs_graph(input.root, verdict, input.upstream)?;
        let graph_bytes = graph.into_bytes();
        // Attach the documentation projection as the carrier's `graph/documentation`
        // named graph so the presenter reads it as a pure keyed fold (PIPELINE_SPINE §4),
        // never re-parses the byte artifact. The byte lane is kept for the byte readers.
        let dataset = crate::stages::carrier::parse_into_graph(
            &graph_bytes,
            "application/n-quads",
            crate::stages::carrier::GRAPH_DOCUMENTATION,
        )?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(DOCS_GRAPH_PATH.to_string(), graph_bytes);
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            dataset,
            artifacts,
        )))
    }
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

    #[test]
    fn docs_source_files_includes_new_inputs() {
        let root = repo_root();
        let files = docs_source_files(&root).expect("docs_source_files");
        let has_shapes_ttl = files
            .iter()
            .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("shapes.ttl"));
        assert!(
            has_shapes_ttl,
            "docs_source_files must include at least one per-slice shapes.ttl"
        );
        let has_competency = files.iter().any(|p| {
            p.file_name().and_then(|n| n.to_str()) == Some("competency.ttl")
                && p.parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|n| n.to_str())
                    == Some("tests")
        });
        assert!(
            has_competency,
            "docs_source_files must include at least one per-slice tests/competency.ttl"
        );
        let shapes_dir = root.join("shapes");
        let has_root_shapes = files.iter().any(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("ttl")
                && p.parent()
                    .map(|parent| parent == shapes_dir)
                    .unwrap_or(false)
        });
        assert!(
            has_root_shapes,
            "docs_source_files must include root shapes/*.ttl files"
        );
        // A CQ's `cqQueryFile` may resolve into a slice's own `queries/competency/`
        // tree (`slices/grounding/logic/queries/competency/named-parametric-paths.rq`)
        // or the shared repo-root tree (`queries/competency/citation-intents.rq`) —
        // both must be cache-salted (`gmeow_docs::model::apply_competency_query_text`).
        let has_slice_query = files
            .iter()
            .any(|p| p.ends_with("queries/competency/named-parametric-paths.rq"));
        assert!(
            has_slice_query,
            "docs_source_files must include at least one per-slice queries/competency/*.rq"
        );
        let root_query = root.join("queries/competency/citation-intents.rq");
        assert!(
            files.contains(&root_query),
            "docs_source_files must include the shared root queries/competency/*.rq tree"
        );
        // The notation-grammar exhibits (`gmeow_docs::model::DocGrammar`) — a
        // `grammars/*.ebnf` edit must bust the docs cache.
        let has_grammar = files
            .iter()
            .any(|p| p.ends_with("slices/grounding/lang/grammars/gmn.ebnf"));
        assert!(
            has_grammar,
            "docs_source_files must include the lang slice's grammars/*.ebnf files"
        );
    }

    /// A `stage-validate` + `stage-compile-logic` upstream pair whose JSON
    /// diagnostics artifacts carry an EMPTY [`gmeow_errors::Report`] (zero
    /// findings, never an absent artifact — a missing artifact must hard-fail,
    /// see [`diagnostics_digest_from_upstream`]) — the minimal upstream
    /// `render_docs_graph` needs so a source-lane test can render the whole-repo
    /// docs graph without a real pipeline run.
    fn empty_diagnostics_upstream() -> BTreeMap<String, StageProduct> {
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-validate".to_string(),
            report_json_product(
                "stage-validate",
                crate::stages::validate::SHACL_JSON_PATH,
                &gmeow_errors::Report::new("shacl"),
            ),
        );
        upstream.insert(
            "stage-compile-logic".to_string(),
            report_json_product(
                "stage-compile-logic",
                crate::stages::compile_logic::DIAG_JSON_PATH,
                &gmeow_errors::Report::new("logic-compile"),
            ),
        );
        upstream
    }

    /// Build a synthetic `stage_id` product carrying `report`'s JSON
    /// serialization ([`gmeow_errors::render::to_json`]) at `json_path` — the
    /// EXACT artifact lane `diagnostics_digest_from_upstream` reads, mirroring
    /// the real `stage-validate`/`stage-compile-logic` producers
    /// (`diag_render::render_diagnostics_artifacts`), so this test exercises the
    /// real production code path rather than the lossy `diagnostics:nodes` blob.
    fn report_json_product(
        stage_id: &str,
        json_path: &str,
        report: &gmeow_errors::Report,
    ) -> StageProduct {
        let json = gmeow_errors::render::to_json(report).expect("encode report json");
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(json_path.to_string(), json.into_bytes());
        StageProduct::from_artifacts(stage_id, artifacts)
    }

    /// A minimal synthetic [`gmeow_errors::Finding`] with the given code/
    /// category/term-location/slice-attribution — a plain builder, since (unlike
    /// the retired `DiagNode` lane) the JSON-artifact join needs no ledger
    /// fingerprint.
    fn synthetic_finding(
        code: &str,
        category: gmeow_errors::FindingCategory,
        term_iri: Option<&str>,
        slice_iri: Option<&str>,
    ) -> gmeow_errors::Finding {
        let mut finding = gmeow_errors::Finding::new(
            gmeow_errors::Severity::Warning,
            code,
            format!("synthetic finding for {code}"),
        )
        .with_category(category);
        if let Some(term_iri) = term_iri {
            finding.add_location(gmeow_errors::Location::new(
                None,
                None,
                None,
                Some(term_iri.to_string()),
            ));
        }
        if let Some(slice_iri) = slice_iri {
            finding
                .attributions
                .push(gmeow_errors::DiagnosticAttribution {
                    slice_iri: slice_iri.to_string(),
                    role: "focus-origin".to_string(),
                    evidence: None,
                });
        }
        finding
    }

    #[test]
    fn diagnostics_digest_joins_term_and_slice_and_hard_fails_on_missing_upstream() {
        let term_iri = "https://blackcatinformatics.ca/gmeow/Cat";
        let mut known_terms: BTreeSet<String> = BTreeSet::new();
        known_terms.insert(term_iri.to_string());

        let mut shacl_report = gmeow_errors::Report::new("shacl");
        shacl_report.findings.push(synthetic_finding(
            "shacl.MinCountConstraintComponent",
            gmeow_errors::FindingCategory::DataShapeViolation,
            Some(term_iri),
            Some("https://blackcatinformatics.ca/gmeow/slices/core"),
        ));
        // A finding whose location names no KNOWN term: honestly absent from
        // `by_term`, never a fuzzy/heuristic join.
        shacl_report.findings.push(synthetic_finding(
            "shacl.NodeKindConstraintComponent",
            gmeow_errors::FindingCategory::DataShapeViolation,
            Some("https://example.test/not-a-known-term"),
            None,
        ));

        let mut compile_report = gmeow_errors::Report::new("logic-compile");
        compile_report.findings.push(synthetic_finding(
            "logic-compile.UNKNOWN_PROFILE",
            gmeow_errors::FindingCategory::ModelingDisciplineViolation,
            None,
            None,
        ));

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-validate".to_string(),
            report_json_product(
                "stage-validate",
                crate::stages::validate::SHACL_JSON_PATH,
                &shacl_report,
            ),
        );
        upstream.insert(
            "stage-compile-logic".to_string(),
            report_json_product(
                "stage-compile-logic",
                crate::stages::compile_logic::DIAG_JSON_PATH,
                &compile_report,
            ),
        );

        let rule = gmeow_docs::model::ConstraintRule {
            code: "shacl.MinCountConstraintComponent".to_string(),
            slug: "shacl-min-count-constraint-component".to_string(),
            category: "https://blackcatinformatics.ca/gmeow/FindingDataShapeViolation".to_string(),
            severity: "binding".to_string(),
            help_uri:
                "https://blackcatinformatics.ca/gmeow/rules#shacl-min-count-constraint-component"
                    .to_string(),
            label: None,
            definition: None,
            applies_to_terms: Vec::new(),
            formalizes: None,
        };

        let digest =
            diagnostics_digest_from_upstream(&upstream, &known_terms, std::slice::from_ref(&rule))
                .expect("digest folds from synthetic upstream");
        assert_eq!(digest.total, 3, "3 findings folded across both producers");
        assert_eq!(
            digest.by_term.get(term_iri).map(Vec::len),
            Some(1),
            "only the finding whose location names a KNOWN term joins by_term"
        );
        let joined = &digest.by_term[term_iri][0];
        assert_eq!(joined.code, "shacl.MinCountConstraintComponent");
        assert_eq!(joined.help_uri.as_deref(), Some(rule.help_uri.as_str()));
        assert_eq!(
            digest
                .by_slice
                .get("https://blackcatinformatics.ca/gmeow/slices/core")
                .map(Vec::len),
            Some(1)
        );
        // The unattributed / unresolved-code findings never fabricate a slice or help_uri.
        assert!(
            !digest
                .by_slice
                .values()
                .flatten()
                .any(|f| f.code == "logic-compile.UNKNOWN_PROFILE" && f.help_uri.is_some()),
            "an unresolved code must never carry a fabricated help_uri"
        );

        // Missing EITHER declared upstream product hard-fails (never a silent empty digest).
        let mut only_validate: BTreeMap<String, StageProduct> = BTreeMap::new();
        only_validate.insert(
            "stage-validate".to_string(),
            report_json_product(
                "stage-validate",
                crate::stages::validate::SHACL_JSON_PATH,
                &gmeow_errors::Report::new("shacl"),
            ),
        );
        assert!(
            diagnostics_digest_from_upstream(&only_validate, &known_terms, &[]).is_err(),
            "missing stage-compile-logic must hard-fail"
        );
        assert!(
            diagnostics_digest_from_upstream(&BTreeMap::new(), &known_terms, &[]).is_err(),
            "missing both upstream products must hard-fail"
        );

        // A declared upstream product present but MISSING the JSON artifact (e.g. a
        // stale/partial product) hard-fails too — never silently treated as empty.
        let mut missing_artifact: BTreeMap<String, StageProduct> = BTreeMap::new();
        missing_artifact.insert(
            "stage-validate".to_string(),
            StageProduct::from_artifacts("stage-validate", BTreeMap::new()),
        );
        missing_artifact.insert(
            "stage-compile-logic".to_string(),
            report_json_product(
                "stage-compile-logic",
                crate::stages::compile_logic::DIAG_JSON_PATH,
                &gmeow_errors::Report::new("logic-compile"),
            ),
        );
        assert!(
            diagnostics_digest_from_upstream(&missing_artifact, &known_terms, &[]).is_err(),
            "a stage-validate product missing the SHACL JSON artifact must hard-fail"
        );
    }

    #[test]
    fn docs_graph_is_nonempty_and_parses() {
        let root = repo_root();
        let upstream = empty_diagnostics_upstream();
        let nq = render_docs_graph(&root, ReasoningVerdict::default(), &upstream)
            .expect("render docs graph");
        let dataset =
            rdf_bytes_to_dataset(nq.as_bytes(), "application/n-quads", "docs-graph").unwrap();
        let count = dataset.quad_count();
        // The documentation graph covers 50+ slices and their terms.
        assert!(
            count > 200,
            "docs named graph unexpectedly small: {count} quads"
        );
        // With a verdict attached, every documented term carries a reasoning status.
        assert!(
            nq.contains("docReasoningStatus"),
            "docs graph must carry per-term reasoning status when a verdict is attached"
        );
    }

    #[test]
    fn reasoning_verdict_reads_unsat_and_inconsistency_from_closure() {
        // A closure with one unsat class and one Nothing-typed individual.
        let closure = concat!(
            "<https://x/Empty> <http://www.w3.org/2000/01/rdf-schema#subClassOf> ",
            "<http://www.w3.org/2002/07/owl#Nothing> .\n",
            "<https://x/i> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
            "<http://www.w3.org/2002/07/owl#Nothing> .\n",
        );
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            crate::stages::reason::CLOSURE_PATH.to_string(),
            closure.as_bytes().to_vec(),
        );
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts("stage-reason", artifacts),
        );
        let verdict = reasoning_verdict_from_reason(&upstream).expect("verdict");
        assert!(
            !verdict.is_consistent,
            "Nothing-typed individual ⇒ inconsistent"
        );
        assert!(verdict.unsatisfiable.contains("https://x/Empty"));
        assert!(
            !verdict
                .unsatisfiable
                .contains("http://www.w3.org/2002/07/owl#Nothing")
        );

        // A missing stage-reason product hard-fails (never a silent default).
        assert!(reasoning_verdict_from_reason(&BTreeMap::new()).is_err());
    }
}
