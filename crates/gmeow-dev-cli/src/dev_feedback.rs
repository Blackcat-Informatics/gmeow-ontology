// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The diagnostics-rail commands: `external-tool`, `feedback`, `slice-fix-deps`.
//!
//! All three project onto the shared `gmeow_diagnostics` rail: `external-tool`
//! wraps a foreign tool's failure as one canonical finding and MIRRORS its exit
//! code; `feedback` folds every offline dev-gate surface into one report,
//! projects it to the console, and writes the `{json,sarif,html,gts}` artifacts;
//! `slice-fix-deps` renders the native ownership analyzer's manifest patches as a
//! reviewable unified diff.

use std::path::Path;
use std::process::Command;

use gmeow_cli_core::ConsoleMode;
use gmeow_diagnostics::{render, Finding, Report, Severity};

use crate::dev_common::{
    fail, project_root, reporter_for, resolve_console, NAMESPACE, ONTOLOGY_IRI,
};

/// Logs at or under this many characters ride verbatim in a finding's detail;
/// larger logs are digested (head + tail + a SHA-256 of the full bytes).
const DEFAULT_DETAIL_LIMIT: usize = 4096;

// ── external-tool ─────────────────────────────────────────────────────────────

/// Resolved diagnostics output policy (flag > `GMEOW_DIAGNOSTICS_*` env > default).
struct DiagnosticsConfig {
    console: ConsoleMode,
    artifacts: Vec<String>,
    directory: std::path::PathBuf,
    stem: String,
    category: String,
}

impl DiagnosticsConfig {
    /// Resolve the five `--diagnostics-*` knobs against their env fallbacks.
    fn resolve(
        console: Option<ConsoleMode>,
        artifacts: Option<&str>,
        directory: Option<&Path>,
        stem: Option<&str>,
        category: Option<&str>,
    ) -> Self {
        let console = resolve_console(console);
        let artifacts_raw = artifacts
            .map(str::to_owned)
            .or_else(|| std::env::var("GMEOW_DIAGNOSTICS_ARTIFACTS").ok())
            .unwrap_or_else(|| "all".to_owned());
        let artifacts = parse_artifacts(&artifacts_raw);
        let stem = stem
            .map(str::to_owned)
            .or_else(|| std::env::var("GMEOW_DIAGNOSTICS_STEM").ok())
            .unwrap_or_else(|| "gmeow-feedback".to_owned());
        let category = category
            .map(str::to_owned)
            .or_else(|| std::env::var("GMEOW_DIAGNOSTICS_CATEGORY").ok())
            .unwrap_or_else(|| "default".to_owned());
        let directory = directory
            .map(Path::to_path_buf)
            .or_else(|| std::env::var("GMEOW_DIAGNOSTICS_DIR").ok().map(Into::into))
            .unwrap_or_else(|| std::path::PathBuf::from("dist").join("diagnostics"));
        Self {
            console,
            artifacts,
            directory,
            stem,
            category,
        }
    }
}

/// Parse the `none|all|comma-list` artifact selector into the set of kinds.
fn parse_artifacts(raw: &str) -> Vec<String> {
    match raw.trim() {
        "none" => Vec::new(),
        "all" | "" => vec!["json".into(), "sarif".into(), "html".into()],
        list => list
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect(),
    }
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
    let config = DiagnosticsConfig::resolve(console, artifacts, directory, stem, category);

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

    reporter_for(config.console).report(&report.normalized());
    if let Err(code) = write_artifacts(&report, &config) {
        return code;
    }

    if report.ok() {
        println!("{name} passed");
        0
    } else {
        eprintln!("{name} failed ({} error(s))", report.error_count());
        // Mirror the wrapped tool's exact code; a report with findings but a 0 exit
        // still fails (use 1).
        if code != 0 {
            code
        } else {
            1
        }
    }
}

/// Write the selected `{json,sarif,html}` artifacts for a report under the
/// resolved directory. Product artifacts → filesystem; the "wrote" lines → stdout.
fn write_artifacts(report: &Report, config: &DiagnosticsConfig) -> Result<(), i32> {
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
    for kind in &config.artifacts {
        let (ext, body) = match kind.as_str() {
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
        println!("wrote {}", path.display());
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
    let config = DiagnosticsConfig::resolve(console, artifacts, directory, stem, category);
    let root = project_root();

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
/// projections as content-addressed blobs in a fresh GTS package.
fn write_feedback_bundle(report: &Report, config: &DiagnosticsConfig) -> Result<(), i32> {
    let normalized = report.normalized();
    let json = render::to_json(&normalized).map_err(|e| fail(format!("json render: {e}")))?;
    let sarif = render::to_sarif(&normalized).map_err(|e| fail(format!("sarif render: {e}")))?;

    // The findings JSON + SARIF projections ARE the self-describing payload; each
    // rides as a content-addressed blob keyed by its `rep` label.
    let mut writer = purrdf::gts::writer::Writer::new("feedback");
    writer.add_blob(
        json.as_bytes(),
        Some("application/json"),
        Some("feedback.json"),
    );
    writer.add_blob(
        sarif.as_bytes(),
        Some("application/sarif+json"),
        Some("feedback.sarif"),
    );
    writer.add_index();
    let bytes = writer.to_bytes();

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
type SurfaceThunk = fn(&Path) -> Result<Report, String>;

fn surfaces() -> Vec<(&'static str, SurfaceThunk)> {
    vec![
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
            let audit =
                gmeow_validate::box_roles::audit_box_roles(&paths, ONTOLOGY_IRI, NAMESPACE)?;
            Ok(gmeow_validate::box_roles::to_diagnostics_report(
                &audit,
                ONTOLOGY_IRI,
                NAMESPACE,
            ))
        }),
        ("coverage", |root| {
            let rep = gmeow_validate::coverage::run_coverage(
                &root.join("tests/fixtures/coverage"),
                &root.join("generated/mappings"),
                NAMESPACE,
            )?;
            Ok(gmeow_validate::coverage::coverage_to_diagnostics(&rep))
        }),
        ("wikidata", |root| {
            gmeow_validate::mapping_eval::wikidata_diagnostics(&root.join("generated/mappings"))
        }),
        ("alignment", |root| {
            let findings =
                gmeow_pipeline::stages::correspondence_soundness::lint_correspondence_soundness(
                    root, false,
                )
                .map_err(|e| e.to_string())?;
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
    ]
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
        if apply {
            if let Err(e) = std::fs::write(&patch.manifest_path, &patch.patched_text) {
                return fail(format!("cannot write {}: {e}", patch.manifest_path));
            }
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
