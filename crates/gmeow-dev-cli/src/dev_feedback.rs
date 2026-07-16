// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The diagnostics-rail commands: `external-tool`, `feedback`, `slice-fix-deps`.
//!
//! All three project onto the shared `gmeow_errors` rail: `external-tool`
//! wraps a foreign tool's failure as one canonical finding and MIRRORS its exit
//! code; `feedback` folds every offline dev-gate surface into one report,
//! projects it to the console, and writes the `{json,sarif,html,gts}` artifacts;
//! `slice-fix-deps` renders the native ownership analyzer's manifest patches as a
//! reviewable unified diff.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::Path;
use std::process::Command;

use gmeow_cli_core::{ConsoleMode, DiagnosticsConfig};
use gmeow_errors::{Finding, Report, Severity, render};

use crate::dev_common::{NAMESPACE, ONTOLOGY_IRI, fail, note, project_root, reporter_for};
use crate::error;

/// Logs at or under this many characters ride verbatim in a finding's detail;
/// larger logs are digested (head + tail + a SHA-256 of the full bytes).
const DEFAULT_DETAIL_LIMIT: usize = 4096;

// ── external-tool ─────────────────────────────────────────────────────────────

/// Read the `GMEOW_DIAGNOSTICS_*` environment variables into a map for
/// [`DiagnosticsConfig::resolve`]. Only variables the config cares about are
/// captured; everything else is ignored.
pub(crate) fn diagnostics_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    for key in [
        "GMEOW_DIAGNOSTICS_CONSOLE",
        "GMEOW_DIAGNOSTICS_ARTIFACTS",
        "GMEOW_DIAGNOSTICS_CATEGORY",
        "GMEOW_DIAGNOSTICS_STEM",
        "GMEOW_DIAGNOSTICS_DIR",
    ] {
        if let Ok(value) = std::env::var(key) {
            env.insert(key.to_owned(), value);
        }
    }
    env
}

/// Deterministic head/tail digest of an over-budget log (mirrors `_digest_detail`).
fn digest_detail(raw: &str, limit: usize) -> String {
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() <= limit {
        return raw.to_owned();
    }
    let digest = sha256_hex(raw.as_bytes());
    let half = (limit / 2).max(1);
    let head: String = chars[..half].iter().collect();
    let tail: String = chars[chars.len() - half..].iter().collect();
    let elided = chars.len() - head.chars().count() - tail.chars().count();
    format!("{head}\n... [{elided} chars elided; sha256={digest}] ...\n{tail}")
}

/// A deterministic content digest for over-budget log detail — delegated to
/// purrdf's content-addressing digest so we never hand-roll a hash.
fn sha256_hex(data: &[u8]) -> String {
    purrdf::gts::writer::digest_string(data)
}

/// Map one external-tool result into a diagnostics report (pure; no I/O).
fn external_report(name: &str, argv: &[String], code: i32, stdout: &str, stderr: &str) -> Report {
    let tool = format!("external.{name}");
    if code == 0 {
        return Report::new(tool);
    }
    let mut sections = vec![
        format!("$ {}", argv.join(" ")),
        format!("exit code: {code}"),
    ];
    if !stdout.trim().is_empty() {
        sections.push(format!("--- stdout ---\n{}", stdout.trim_end()));
    }
    if !stderr.trim().is_empty() {
        sections.push(format!("--- stderr ---\n{}", stderr.trim_end()));
    }
    let combined = sections.join("\n");
    let mut finding = Finding::new(
        Severity::Error,
        tool.clone(),
        format!("{name} failed (exit {code})"),
    )
    .with_tool(name);
    finding.detail = Some(digest_detail(&combined, DEFAULT_DETAIL_LIMIT));
    let mut report = Report::new(tool);
    report.add_finding(finding);
    report
}

/// `gmeow-dev external-tool COMMAND… --name N` — run a foreign tool, wrap failure
/// as a finding, MIRROR its exit code.
#[allow(clippy::too_many_arguments)]
pub fn external_tool(
    command: &[String],
    name: &str,
    console: Option<ConsoleMode>,
    artifacts: Option<&str>,
    directory: Option<&Path>,
    stem: Option<&str>,
    category: Option<&str>,
) -> i32 {
    let dist_dir = project_root().join("dist");
    let config = match DiagnosticsConfig::resolve(
        console.map(ConsoleMode::as_str),
        artifacts,
        directory,
        stem,
        category,
        &diagnostics_env(),
        std::io::stderr().is_terminal(),
        &dist_dir,
    ) {
        Ok(c) => c,
        Err(e) => return fail(e.to_string()),
    };

    let (code, stdout, stderr) = if command.is_empty() {
        (127, String::new(), "empty command list provided".to_owned())
    } else {
        match Command::new(&command[0]).args(&command[1..]).output() {
            Ok(out) => (
                out.status.code().unwrap_or(1),
                String::from_utf8_lossy(&out.stdout).into_owned(),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ),
            Err(e) => (127, String::new(), e.to_string()),
        }
    };

    let mut report = external_report(name, command, code, &stdout, &stderr);
    report
        .metadata
        .insert("category".into(), serde_json::json!(config.category));

    let reporter = reporter_for(config.console);
    reporter.report(&report.normalized());
    if let Err(code) = write_artifacts(&report, &config) {
        return code;
    }

    if report.ok() {
        println!("{name} passed");
        0
    } else {
        gmeow_cli_core::note(
            reporter.as_ref(),
            "gmeow-dev",
            "gmeow-dev.external-tool.failed",
            format!("{name} failed ({} error(s))", report.error_count()),
        );
        // Mirror the wrapped tool's exact code; a report with findings but a 0 exit
        // still fails (use 1).
        if code != 0 { code } else { 1 }
    }
}

/// Write the selected `{json,sarif,html}` artifacts for a report under the
/// resolved directory. Product artifacts → filesystem; the "wrote" lines → stdout.
pub(crate) fn write_artifacts(report: &Report, config: &DiagnosticsConfig) -> Result<(), i32> {
    if config.artifacts.is_empty() {
        return Ok(());
    }
    if let Err(e) = std::fs::create_dir_all(&config.directory) {
        return Err(fail(format!(
            "cannot create {}: {e}",
            config.directory.display()
        )));
    }
    let normalized = report.normalized();
    for kind in DiagnosticsConfig::ARTIFACT_KINDS {
        if !config.artifacts.contains(*kind) {
            continue;
        }
        let (ext, body) = match *kind {
            "json" => (
                "json",
                render::to_json(&normalized).map_err(|e| fail(format!("json render: {e}")))?,
            ),
            "sarif" => (
                "sarif",
                render::to_sarif(&normalized).map_err(|e| fail(format!("sarif render: {e}")))?,
            ),
            "html" => ("html", render::to_html(&normalized)),
            other => return Err(fail(format!("unknown artifact kind {other:?}"))),
        };
        let path = config.directory.join(format!("{}.{ext}", config.stem));
        if let Err(e) = std::fs::write(&path, body) {
            return Err(fail(format!("cannot write {}: {e}", path.display())));
        }
        // Progress notice on STDERR, never stdout: stdout carries the command's data
        // render (e.g. `slice-quality --all --format json`), so a "wrote …" line must
        // not contaminate an otherwise single, parseable JSON/SARIF document.
        note(
            "gmeow-dev.feedback.wrote",
            format!("wrote {}", path.display()),
        );
    }
    Ok(())
}

// ── feedback ──────────────────────────────────────────────────────────────────

/// `gmeow-dev feedback` — fold every offline dev-gate surface into one report,
/// project it to the console, and write the `{json,sarif,html,gts}` artifacts.
///
/// The process exit code is driven SOLELY by the folded gate report: a per-surface
/// failure is isolated as a `feedback.<label>-skipped` warning (never an abort),
/// so `feedback` stays an artifact-builder whose verdict is the whole-gate report.
pub fn feedback(
    console: Option<ConsoleMode>,
    artifacts: Option<&str>,
    directory: Option<&Path>,
    stem: Option<&str>,
    category: Option<&str>,
) -> i32 {
    let root = project_root();
    let config = match DiagnosticsConfig::resolve(
        console.map(ConsoleMode::as_str),
        artifacts,
        directory,
        stem,
        category,
        &diagnostics_env(),
        std::io::stderr().is_terminal(),
        &root.join("dist"),
    ) {
        Ok(c) => c,
        Err(e) => return fail(e.to_string()),
    };

    let mut report = Report::new("feedback");
    for (label, thunk) in surfaces() {
        match thunk(&root) {
            Ok(surface) => {
                for finding in surface.findings {
                    report.add_finding(finding);
                }
            }
            Err(e) => report.add_finding(Finding::new(
                Severity::Warning,
                format!("feedback.{label}-skipped"),
                format!("{label} findings not folded: {e}"),
            )),
        }
    }
    report
        .metadata
        .insert("category".into(), serde_json::json!(config.category));

    reporter_for(config.console).report(&report.normalized());
    if let Err(code) = write_artifacts(&report, &config) {
        return code;
    }
    // The self-describing feedback bundle is the canonical record — ALWAYS written.
    if let Err(code) = write_feedback_bundle(&report, &config) {
        return code;
    }

    if report.ok() {
        println!("diagnostics feedback written");
        0
    } else {
        fail(format!("{} error(s)", report.error_count()))
    }
}

/// Write the self-describing `<stem>.gts` bundle: the report's JSON + SARIF
/// projections as content-addressed blobs in a GTS package whose snapshot graph
/// IS the findings RDF and whose metadata stamps the snapshot content id.
fn write_feedback_bundle(report: &Report, config: &DiagnosticsConfig) -> Result<(), i32> {
    let bytes = crate::feedback_bundle::build_feedback_bundle(report)
        .map_err(|e| fail(format!("build feedback bundle: {e}")))?;

    if let Err(e) = std::fs::create_dir_all(&config.directory) {
        return Err(fail(format!(
            "cannot create {}: {e}",
            config.directory.display()
        )));
    }
    let path = config.directory.join(format!("{}.gts", config.stem));
    if let Err(e) = std::fs::write(&path, bytes) {
        return Err(fail(format!("cannot write {}: {e}", path.display())));
    }
    println!("wrote {}", path.display());
    Ok(())
}

/// The `(label, thunk)` table of offline dev-gate surfaces folded into feedback.
/// Each thunk re-runs one native gate surface and returns its `Report`.
type SurfaceThunk = fn(&Path) -> gmeow_errors::Result<Report>;

fn surfaces() -> Vec<(&'static str, SurfaceThunk)> {
    vec![
        ("alignment", |root| {
            let findings =
                gmeow_pipeline::stages::correspondence_soundness::lint_correspondence_soundness(
                    root, false,
                )
                .map_err(error::feedback)?;
            let mut r = Report::new("alignment");
            for d in findings {
                let sev = Severity::parse(&d.severity.to_lowercase()).unwrap_or(Severity::Info);
                r.add_finding(Finding::new(
                    sev,
                    format!("alignment.{}", d.check),
                    d.message,
                ));
            }
            Ok(r)
        }),
        ("coverage", |root| {
            let rep = gmeow_validate::coverage::run_coverage(
                &root.join("tests/fixtures/coverage"),
                &root.join("generated/mappings"),
                NAMESPACE,
            )
            .map_err(error::feedback)?;
            Ok(gmeow_validate::coverage::coverage_to_diagnostics(&rep))
        }),
        ("acceptance", |root| {
            let results = gmeow_pipeline::scoreboards::run_acceptance_corpus(root, None)
                .map_err(error::feedback)?;
            Ok(gmeow_pipeline::scoreboards::acceptance_diagnostics(
                &results,
            ))
        }),
        ("wikidata", |root| {
            gmeow_validate::mapping_eval::wikidata_diagnostics(&root.join("generated/mappings"))
                .map_err(error::feedback)
        }),
        ("constitution", |root| {
            let findings = gmeow_validate::constitution::constitution_full_report(
                &root.join("governance").join("constitution.ttl"),
                &root.join("CONSTITUTION.md"),
                root,
            );
            let mut r = Report::new("constitution");
            for f in findings {
                r.add_finding(f);
            }
            Ok(r)
        }),
        ("crate-layering", |root| {
            let rep = gmeow_validate::crate_layering::check_crate_layering(&root.join("crates"));
            Ok(gmeow_validate::crate_layering::to_diagnostics_report(&rep))
        }),
        ("repo-static", |root| {
            let rep = gmeow_validate::repo_static::check_repo_static(root);
            Ok(gmeow_validate::repo_static::to_diagnostics_report(&rep))
        }),
        ("box-roles", |root| {
            let paths = crate::dev_gates::default_audit_paths(root);
            let audit = gmeow_validate::box_roles::audit_box_roles(&paths, ONTOLOGY_IRI, NAMESPACE)
                .map_err(error::feedback)?;
            Ok(gmeow_validate::box_roles::to_diagnostics_report(
                &audit,
                ONTOLOGY_IRI,
                NAMESPACE,
            ))
        }),
        ("audit", |root| {
            // The claim-audit gate over the committed hallucination corpus (the
            // ungrounded / contradicted / stale finder), exactly the corpus the
            // retired Python `_audit` folded.
            let corpus = root.join("tests/fixtures/coverage/hallucination-kg.ttl");
            let report = gmeow_pipeline::scoreboards::claim_audit(root, &[corpus])
                .map_err(error::feedback)?;
            Ok(gmeow_pipeline::scoreboards::claim_audit_diagnostics(
                &report,
            ))
        }),
        ("generated", |root| {
            // The build-drift surface: run the pipeline in CHECK mode (the build
            // authority) and project its drift into `generator.drift` error findings
            // plus the run's own error findings.
            let jobs = std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1);
            let run =
                gmeow_pipeline::run::run_full(root, jobs, gmeow_pipeline::run::RunMode::Check)
                    .map_err(error::feedback)?;
            let mut r = Report::new("generated");
            let mut drifted = run.drifted.clone();
            drifted.sort();
            for rel in drifted {
                let mut finding = Finding::new(Severity::Error, "generator.drift", rel.clone())
                    .with_tool("pipeline");
                finding.add_location(gmeow_errors::Location::new(Some(rel), None, None, None));
                r.add_finding(finding);
            }
            for finding in run.findings {
                if finding.severity == Severity::Error {
                    r.add_finding(finding);
                }
            }
            Ok(r)
        }),
        ("logic-compile", |root| {
            // The `logic:` compile diagnostics surface: parse diagnostics projected
            // into the canonical report; a hard parse/compile failure is surfaced as
            // one `logic-compile.failed` error rather than aborting the whole fold.
            let source = root.join("slices/grounding/logic/module.ttl");
            let source_ttl = std::fs::read_to_string(&source).map_err(|e| {
                error::source(format!(
                    "logic: source not found: {} ({e})",
                    source.display()
                ))
            })?;
            match compile_logic_report(&source_ttl) {
                Ok(r) => Ok(r),
                Err(msg) => {
                    let mut r = Report::new("logic-compile");
                    r.add_finding(
                        Finding::new(
                            Severity::Error,
                            "logic-compile.failed",
                            format!("logic: compile failed: {msg}"),
                        )
                        .with_tool("logic-compile"),
                    );
                    Ok(r)
                }
            }
        }),
        ("statement-compile", |root| {
            // The native statement compiler's invariant + losslessness diagnostics,
            // over the merged ontology (no imports) — the `include_imports=False`
            // graph the invariant twin unions with the emitted OWL.
            let ontology_nt = merged_ontology_nt(root)?;
            Ok(gmeow_pipeline::stages::statements::compile_diagnostics_report(root, &ontology_nt))
        }),
        ("mapping-compile", |root| {
            Ok(gmeow_pipeline::stages::mappings::compile_diagnostics_report(root))
        }),
        ("slice-ownership", |root| {
            // The FULL native slice-ownership report: ownership defects (Conflict /
            // Mismatch / Unowned) as errors PLUS the dependency observations the
            // focused `validate` gate keeps out, folded here as structured warnings.
            let slices = root.join("slices");
            let catalog = purrdf::slice::SliceCatalog::discover(
                &slices,
                purrdf::SliceVocab::for_namespace(NAMESPACE),
            )
            .map_err(error::feedback)?;
            let analysis = purrdf::slice::OwnershipAnalyzer::new(&catalog)
                .analyze()
                .map_err(error::feedback)?;
            let mut r = Report::new("slice-ownership");
            for finding in gmeow_validate::slice_ownership::ownership_findings(&analysis) {
                r.add_finding(finding);
            }
            Ok(r)
        }),
    ]
}

/// Compile the `logic:` source and project its parse diagnostics into the
/// canonical report — the native twin of `compile_logic`'s `diagnostics_report`.
/// A parse or compile hard error is returned as `Err` for the caller to surface
/// as a single `logic-compile.failed` finding.
fn compile_logic_report(source_ttl: &str) -> gmeow_errors::Result<Report> {
    let (program, diagnostics) = gmeow_logic_compile::frontend::parse_logic_str(source_ttl, None)
        .map_err(|e| error::feedback(e.0))?;
    // Discharge every authored correspondence's lens law by EXECUTION so the five
    // correspondence gates inside `compile_program` read a real per-correspondence verdict
    // instead of hitting their missing-verdict hard-fail on a correspondence-bearing source.
    // A correspondence-free source yields an empty map (the gates never run).
    let verdicts = gmeow_logic::correspondence_exec::logic_program_verdicts(&program)
        .map_err(error::feedback)?;
    gmeow_logic_compile::projections::compile_program(&program, &verdicts)
        .map_err(error::feedback)?;
    Ok(gmeow_logic::logic_diagnostics::diagnostics_report(
        &diagnostics,
    ))
}

/// The merged ontology (root `ontology/gmeow.ttl` + every slice `module.ttl`, no
/// imports) serialized to N-Triples — the `include_imports=False` graph the
/// statement-compile invariant twin unions with the emitted OWL. Each file is
/// parsed standalone and unioned (blank scopes standardized apart) exactly as the
/// pipeline's ontology loader does.
fn merged_ontology_nt(root: &Path) -> gmeow_errors::Result<String> {
    use gmeow_pipeline::stages::source_load;
    let mut files = vec![root.join("ontology").join("gmeow.ttl")];
    files.extend(source_load::module_files(root).map_err(error::feedback)?);
    let mut datasets = Vec::new();
    for file in &files {
        if !file.exists() {
            continue;
        }
        let bytes = std::fs::read(file)
            .map_err(|e| error::source(format!("read {}: {e}", file.display())))?;
        datasets.push(
            source_load::turtle_bytes_to_dataset(&bytes, &file.display().to_string())
                .map_err(error::feedback)?,
        );
    }
    let refs: Vec<&purrdf::RdfDataset> = datasets.iter().map(|d| d.as_ref()).collect();
    let merged = purrdf::RdfDataset::union(&refs);
    let bytes = purrdf::serialize_dataset(
        &merged,
        "application/n-quads",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| error::rdf(format!("serialize ontology: {e}")))?;
    String::from_utf8(bytes).map_err(|e| error::encoding(format!("serialize ontology (utf8): {e}")))
}

// ── slice-fix-deps ─────────────────────────────────────────────────────────────

/// `gmeow-dev slice-fix-deps [--apply] [--slices-dir DIR]` — render (or apply)
/// the native ownership analyzer's manifest-dependency patches as a unified diff.
pub fn slice_fix_deps(apply: bool, slices_dir: Option<&Path>) -> i32 {
    let root = project_root();
    let slices = slices_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("slices"));
    if !slices.is_dir() {
        return fail(format!("slices directory not found: {}", slices.display()));
    }

    let vocab = purrdf::OntologyProfile::for_namespace(NAMESPACE)
        .with_prefix("gmeow")
        .slice_vocab();
    let catalog = match purrdf::slice::SliceCatalog::discover(&slices, vocab) {
        Ok(c) => c,
        Err(e) => return fail(format!("slice catalog discovery failed: {e}")),
    };
    let patches = match purrdf::slice::compute_fix_deps(&catalog) {
        Ok(p) => p,
        Err(e) => return fail(format!("slice fix-deps failed: {e}")),
    };

    let mut diffs: Vec<(String, &purrdf::slice::ManifestPatch)> = Vec::new();
    for patch in &patches {
        let diff = unified_diff(
            &patch.original_text,
            &patch.patched_text,
            &patch.manifest_path,
        );
        if !diff.is_empty() {
            diffs.push((diff, patch));
        }
    }

    if diffs.is_empty() {
        println!("No dependency changes needed.");
        return 0;
    }
    for (diff, patch) in &diffs {
        print!("{diff}");
        if apply && let Err(e) = std::fs::write(&patch.manifest_path, &patch.patched_text) {
            return fail(format!("cannot write {}: {e}", patch.manifest_path));
        }
    }
    if apply {
        println!("Applied {} manifest patch(es).", diffs.len());
    } else {
        println!(
            "{} manifest(s) need changes. Run with --apply to apply.",
            diffs.len()
        );
    }
    0
}

/// A minimal unified diff between two texts (the review surface for a patch).
fn unified_diff(original: &str, patched: &str, path: &str) -> String {
    if original == patched {
        return String::new();
    }
    let orig: Vec<&str> = original.lines().collect();
    let new: Vec<&str> = patched.lines().collect();
    let mut out = String::new();
    out.push_str(&format!("--- a/{path}\n"));
    out.push_str(&format!("+++ b/{path}\n"));
    // A whole-file hunk keeps the surface honest and re-appliable without a full
    // Myers implementation: every original line is a deletion, every new line an
    // addition. The native patcher already guarantees a re-parse-validated result.
    out.push_str(&format!(
        "@@ -1,{} +1,{} @@\n",
        orig.len().max(1),
        new.len().max(1)
    ));
    for line in &orig {
        out.push_str(&format!("-{line}\n"));
    }
    for line in &new {
        out.push_str(&format!("+{line}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{compile_logic_report, surfaces};

    /// The canonical offline dev-gate surface set `feedback` folds into one report
    /// — the Rust twin of the retired Python `_EXPECTED_SURFACES`. Pinned here so a
    /// future edit that adds or drops a fold surface without updating this set fails
    /// the gate (the drift guard the deleted `test_feedback_surfaces.py` provided).
    const EXPECTED_SURFACES: &[&str] = &[
        "alignment",
        "coverage",
        "acceptance",
        "wikidata",
        "constitution",
        "crate-layering",
        "repo-static",
        "box-roles",
        "audit",
        "generated",
        "logic-compile",
        "statement-compile",
        "mapping-compile",
        "slice-ownership",
    ];

    /// `surfaces()` folds EXACTLY the canonical dev-gate surface set — no surface
    /// silently dropped (the coverage regression) and none silently added. On-gate:
    /// this inspects only the `(label, _)` table, never running any thunk.
    #[test]
    fn surfaces_cover_exactly_the_canonical_set() {
        let mut got: Vec<&str> = surfaces().iter().map(|(label, _)| *label).collect();
        got.sort_unstable();
        let mut expected: Vec<&str> = EXPECTED_SURFACES.to_vec();
        expected.sort_unstable();
        assert_eq!(
            got, expected,
            "feedback surface set drifted from the canonical dev-gate surfaces"
        );
    }

    /// A `logic:` source that DECLARES a `logic:Correspondence` (an isomorphism with a
    /// realized get leg) must compile cleanly through the public `compile_logic_report`
    /// surface — its correspondence gates run against EXECUTED lens-law verdicts computed
    /// by the caller. This pins the fix for the missing-verdict hard-fail: before the
    /// caller discharged the verdicts, `compile_program` fed the gates an empty verdict map
    /// and the round-trip gate PANICKED on this exact input (a `PanicException` on the PyO3
    /// twin) instead of returning a result. The assertion is `is_ok`; a regression re-arms
    /// the panic and aborts the test process rather than returning `Err`.
    #[test]
    fn correspondence_bearing_source_compiles_without_missing_verdict_panic() {
        // An Isomorphism cell with a single-step get leg. Its lawful put is the structural
        // inverse, which the round-trip gate composes + discharges — reaching `verdict_for`.
        let source = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <https://gmeow.example/corr/> .
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .

ex:iso a logic:Correspondence ;
    logic:correspondenceRelation logic:Equiv ;
    logic:morphismClass logic:Isomorphism ;
    logic:morphismKind logic:InstitutionMorphism ;
    logic:mnemomorphic \"true\"^^xsd:boolean ;
    logic:getLeg ex:isoGet .

ex:isoGet gm:path ex:isoStep .
";
        let report = compile_logic_report(source);
        assert!(
            report.is_ok(),
            "correspondence-bearing source must compile (verdicts discharged), got {report:?}"
        );
    }
}
