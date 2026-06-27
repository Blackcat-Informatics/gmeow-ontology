// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `mappings` stage (#861 P3): compile the alignment artifacts.
//!
//! All mapping artifact families are Rust-owned and wired directly here:
//!   * **SSSOM** → `gmeow_slice::emit_sssom_sets(root)` (byte-identical to the
//!     historical Python emitter, its own parity gate) → `generated/mappings/*.sssom.tsv`.
//!   * **FnO** → `gmeow_slice::emit_fno(root)` → `generated/projections/functions.fno.ttl`.
//!   * **EDOAL** → `gmeow_slice::emit_edoal_sets(root)` (byte-identical to the
//!     historical Python emitter — built as N-Triples then serialized through the
//!     project's canonical Turtle serializer) → `generated/projections/*.edoal.ttl`.
//!   * **SPARQL CONSTRUCT** → `gmeow_slice::emit_sparql_sets(root)` (the
//!     closed-algebra text renderer) → `generated/queries/*.rq`.
//!   * **Standpoint projections** → `gmeow_slice::emit_standpoint_sets(root)` — the
//!     seven hand-authored `standpoint-*.rq` (six peer-model re-expressions:
//!     Standpoint-OWL 2, CRMinf, PROV-O, Web Annotation, schema.org Claim, BBC
//!     News; plus the legacy-modality projection), fixed template-coded SPARQL
//!     with no DSL input → `generated/queries/standpoint-*.rq`.
//!   * **DSL stats** → `gmeow_slice::emit_dsl_stats(root)` — the committed,
//!     drift-gated counts summary (equivalences / functions / mapping_sets /
//!     projections / cells_by_set) → `generated/mappings/dsl-stats.json`.
//!
//! Every output is byte-identical to the historical Python driver.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_diagnostics::{Finding, Location, Report, Severity};
use gmeow_rdf::RdfSeverity;
use gmeow_slice::emit_sssom_sets;
use gmeow_slice::fno_emit::emit_fno;
use gmeow_slice::prefix_emit::{emit_core_prefixes, emit_jsonld_context};
use gmeow_slice::{
    emit_claim_view, emit_dsl_stats, emit_edoal_sets, emit_list_functions, emit_sparql_sets,
    emit_standpoint_sets, lint_prefix_consistency, lint_projection, CLAIM_VIEW_FILE,
};

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// Directory (logical-path prefix) of the SSSOM TSV sets.
pub const SSSOM_DIR: &str = "generated/mappings";
/// Committed logical path of the FnO transform catalog.
pub const FNO_PATH: &str = "generated/projections/functions.fno.ttl";
/// Directory (logical-path prefix) of the EDOAL alignment Turtle files.
pub const EDOAL_DIR: &str = "generated/projections";
/// Directory (logical-path prefix) of the SPARQL CONSTRUCT projection queries
/// (also home to the seven standpoint `standpoint-*.rq` projections).
pub const QUERIES_DIR: &str = "generated/queries";
/// Committed logical path of the DSL surface-count summary.
pub const DSL_STATS_PATH: &str = "generated/mappings/dsl-stats.json";
/// Committed logical path of the importable named prefix set (#1009 §2).
pub const CORE_PREFIXES_PATH: &str = "generated/projections/core-prefixes.ttl";
/// Committed logical path of the JSON-LD `@context` (#1009 §2; replaces the
/// retired Python `jsonld_context.py` builder).
pub const JSONLD_CONTEXT_PATH: &str = "generated/context.jsonld";
/// Committed logical path of the first-class RDF list functions (#1009 §5).
pub const LIST_FUNCTIONS_PATH: &str = "generated/projections/list-functions.fno.ttl";

/// Compile all five mapping families (SSSOM + FnO + EDOAL + SPARQL + standpoint
/// projections) plus the DSL surface-count summary from `root`, returning
/// `{logical_path → bytes}`. The mappings stage is now complete.
pub fn compile_mappings(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    // Prefix-consistency gate (#1009 §2): no authored source may shadow a registry
    // prefix with a foreign namespace — a shadow desynchronizes authored CURIEs from
    // the registry-driven shortener. Hard-fail before emitting any artifact
    // (no-optionality); this makes `regenerate` / `check-generated` / `make check`
    // all reject a shadow.
    let prefix_problems = lint_prefix_consistency(root).map_err(|e| PipelineError::Stage {
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

    // SSSOM — byte-identical to the Python emitter.
    let sssom = emit_sssom_sets(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("SSSOM emission failed: {e}"),
    })?;
    for (filename, tsv) in sssom {
        artifacts.insert(format!("{SSSOM_DIR}/{filename}"), tsv.into_bytes());
    }

    // FnO — the transform catalog as N-Triples (compared by isomorphism).
    let fno = emit_fno(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("FnO emission failed: {e}"),
    })?;
    artifacts.insert(FNO_PATH.to_string(), fno.into_bytes());

    // EDOAL — per-profile alignment Turtle (byte-identical to the Python emitter).
    let edoal = emit_edoal_sets(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("EDOAL emission failed: {e}"),
    })?;
    for (filename, ttl) in edoal {
        artifacts.insert(format!("{EDOAL_DIR}/{filename}"), ttl.into_bytes());
    }

    // SPARQL CONSTRUCT — per-profile executable projection queries.
    let sparql = emit_sparql_sets(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("SPARQL emission failed: {e}"),
    })?;
    for (filename, rq) in sparql {
        artifacts.insert(format!("{QUERIES_DIR}/{filename}"), rq.into_bytes());
    }

    // Standpoint projections — the seven fixed `standpoint-*.rq` queries (byte-identical
    // to the Python template-coded emitters; no DSL input).
    let standpoint = emit_standpoint_sets(root).map_err(|e| PipelineError::Stage {
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
        emit_claim_view().into_bytes(),
    );

    // DSL surface-count summary — the committed, drift-gated counts JSON.
    let dsl_stats = emit_dsl_stats(root).map_err(|e| PipelineError::Stage {
        stage: "stage-mappings".to_string(),
        message: format!("dsl-stats emission failed: {e}"),
    })?;
    artifacts.insert(DSL_STATS_PATH.to_string(), dsl_stats.into_bytes());

    // Prefix-set projections (#1009 §2) — both derived from the single
    // PREFIX_REGISTRY authority: the importable `gmeow:CorePrefixes` SHACL set
    // and the JSON-LD `@context`. Deterministic by construction (const-derived),
    // so they ride the `generated/` drift gate and fold into `gmeow.gts` exactly
    // like the FnO catalog, with no new pipeline stage.
    artifacts.insert(
        CORE_PREFIXES_PATH.to_string(),
        emit_core_prefixes().into_bytes(),
    );
    artifacts.insert(
        JSONLD_CONTEXT_PATH.to_string(),
        emit_jsonld_context().into_bytes(),
    );

    // First-class RDF list functions (#1009 §5) — six FnO primitives backed by the
    // reasoning layer's recursive rdf:List resolution. Fixed content, deterministic;
    // folds into gmeow.gts like the FnO catalog.
    artifacts.insert(
        LIST_FUNCTIONS_PATH.to_string(),
        emit_list_functions().into_bytes(),
    );

    Ok(artifacts)
}

/// Compile mappings and fold their diagnostics into the native report.
///
/// This is the Rust-owned implementation behind the Python feedback surface:
/// Python remains the CLI/interface, while compilation, SSSOM validation, and
/// cross-layer projection linting all run through native Rust authorities.
pub fn compile_diagnostics_report(root: &Path) -> Report {
    let mut report = Report::new("mapping-compile");
    let artifacts = match compile_mappings(root) {
        Ok(artifacts) => artifacts,
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

    match lint_projection(root, false) {
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

    let set = match gmeow_rdf::sssom::parse_tsv(text) {
        Ok(set) => set,
        Err(diag) => {
            report.add_finding(sssom_finding(path, None, diag.message, "parse", diag.code));
            return;
        }
    };

    // Structural SSSOM parse failures returned above are already folded into the
    // report; semantic validation diagnostics use the closed RDF severity enum.
    for diag in gmeow_rdf::sssom::validate(&set) {
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
/// a mapping source, #863). A missing directory yields nothing.
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
/// FnO + EDOAL + SPARQL + standpoint projections) plus the DSL surface-count summary.
pub struct MappingsStage;

impl Stage for MappingsStage {
    fn id(&self) -> &str {
        "stage-mappings"
    }
    fn kind(&self) -> StageKind {
        StageKind::Transform
    }
    fn consumes(&self) -> &[String] {
        // Reads dsl/mappings + slice mapping cells from the root (like statements
        // reads dsl/statements). The slice DAG edge is reconciled at P6 wiring.
        &[]
    }
    fn impl_version(&self) -> &str {
        // v6: routes the list-functions catalog through the shared
        // `gmeow_rdf::fno::to_quads` serializer (§19 one-path) — the committed
        // artifact form becomes N-Triples like `functions.fno.ttl`. Bump busts
        // the stage cache.
        "mappings.v6-list-functions-fno"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, PipelineError> {
        // Raw source read: the alignment artifacts compile from the `dsl/mappings/`
        // tree plus the per-slice mapping cells in the slice modules — none of which
        // any upstream product reflects. Declare them ALL so a mapping edit busts the
        // cache. `consumes() == []` (the leaf reads sources, not upstream products).
        let mut files = Vec::new();
        collect_files_recursive(&root.join("dsl").join("mappings"), &mut files)?;
        files.extend(crate::stages::source_load::module_files(root)?);
        files.sort();
        files.dedup();
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let artifacts = compile_mappings(input.root)?;
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::source_load::rdf_bytes_to_store;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn triple_set(bytes: &[u8], media_type: &str) -> std::collections::BTreeSet<String> {
        let store = rdf_bytes_to_store(bytes, media_type, "triple_set").unwrap();
        store
            .iter()
            .map(|q| {
                let q = q.unwrap();
                format!("{} {} {} .", q.subject, q.predicate, q.object)
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
        // The stage wires `gmeow_slice::emit_sssom_sets` — the SAME Rust the
        // Python build calls — so for every set it emits that has a committed
        // counterpart, the bytes MUST match exactly (the emitter's own parity
        // contract). The total set count vs committed is subject to the
        // committed-vs-local env/staleness drift and is the CI `check-generated`
        // gate, not asserted here.
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");
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
        // The stage wires `gmeow_slice::emit_edoal_sets` / `emit_sparql_sets` — the
        // same Rust the byte-parity unit tests in the slice crate exercise. Every
        // EDOAL `.edoal.ttl` and SPARQL `.rq` the stage emits MUST equal its
        // committed counterpart byte-for-byte (the emitters' parity contract).
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");
        let mut edoal = 0usize;
        let mut sparql = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for (path, bytes) in &artifacts {
            let name = path.rsplit('/').next().unwrap_or(path);
            let is_edoal = path.starts_with(EDOAL_DIR) && path.ends_with(".edoal.ttl");
            // The per-profile SPARQL projections only; the seven `standpoint-*.rq`
            // queries are covered by their own dedicated parity test below.
            let is_sparql = path.starts_with(QUERIES_DIR)
                && name.ends_with(".rq")
                && !name.starts_with("standpoint-");
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
            edoal, 45,
            "expected 45 EDOAL files byte-matching, got {edoal}"
        );
        assert_eq!(
            sparql, 45,
            "expected 45 SPARQL files byte-matching, got {sparql}"
        );
    }

    #[test]
    fn standpoint_and_dsl_stats_emit_byte_identically_with_committed() {
        // The stage wires `emit_standpoint_sets` / `emit_dsl_stats` — the same Rust
        // the slice-crate byte-parity unit tests exercise. The seven standpoint `.rq`
        // and `dsl-stats.json` the stage emits MUST equal their committed counterparts
        // byte-for-byte (the emitters' parity contract).
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");
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
        let artifacts = compile_mappings(&root).expect("compile");
        let path = format!("{QUERIES_DIR}/{CLAIM_VIEW_FILE}");
        let bytes = artifacts.get(&path).expect("claim-view artifact");
        let committed =
            std::fs::read(root.join(&path)).unwrap_or_else(|_| panic!("committed missing: {path}"));
        assert_eq!(bytes, &committed, "claim view drifted from committed");
    }

    #[test]
    fn fno_is_well_formed_ntriples() {
        // Wiring check: `emit_fno` produces a non-empty FnO catalog that parses.
        // (Committed-byte/iso parity is the CI `check-generated` gate, env-matched.)
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");
        let fno = artifacts.get(FNO_PATH).expect("fno artifact");
        let triples = triple_set(fno, "application/n-triples");
        assert!(
            triples.len() > 20,
            "FnO catalog unexpectedly small: {} triples",
            triples.len()
        );
    }

    #[test]
    fn prefix_set_projections_are_emitted_and_parse() {
        // Wiring check (#1009 §2): the mappings stage emits the importable prefix
        // set + JSON-LD context, and the Turtle parses with the importable node
        // carrying the generalized sh:declare surface.
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");

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
        // Wiring check (#1009 §5): the mappings stage emits the six list functions
        // as well-formed FnO N-Triples (routed through the shared
        // `gmeow_rdf::fno::to_quads` serializer, §19 one-path), each typed via
        // fno:Output and fno:Function.
        let root = repo_root();
        let artifacts = compile_mappings(&root).expect("compile");
        let lf = artifacts
            .get(LIST_FUNCTIONS_PATH)
            .expect("list-functions artifact");
        let triples = triple_set(lf, "application/n-triples");
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
