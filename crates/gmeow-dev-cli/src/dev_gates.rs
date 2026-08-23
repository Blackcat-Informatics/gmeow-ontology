// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The repo-gate commands: box-roles, constitution, crate/repo-static, coverage,
//! wikidata, doc-lint, lint-alignment, temporal, audit, and the license guard.
//!
//! Each delegates to an already-native authority in `gmeow_validate` /
//! `gmeow_pipeline` / `gmeow_docs`, following the console convention (product →
//! stdout, diagnostics → stderr, `0`/`1` exit).

use std::path::{Path, PathBuf};
use std::time::Duration;

use gmeow_errors::{Finding, Report, Severity, render};

use crate::dev_common::{
    NAMESPACE, ONTOLOGY_IRI, emit_report, fail, note, project_root, snapshot_bytes,
};
use crate::error;

/// The authored term sources the default repo-only audit covers: every slice
/// `module.ttl` + `manifest.ttl` plus the shared slice `vocabulary.ttl`.
pub fn default_audit_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let slices = root.join("slices");
    collect_ttl_named(&slices, "module.ttl", &mut paths);
    collect_ttl_named(&slices, "manifest.ttl", &mut paths);
    let vocab = slices.join("vocabulary.ttl");
    if vocab.exists() {
        paths.push(vocab);
    }
    paths.sort();
    paths
}

/// Recursively collect every file named `name` under `dir`.
fn collect_ttl_named(dir: &Path, name: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ttl_named(&path, name, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            out.push(path);
        }
    }
}

// ── box-roles audit ─────────────────────────────────────────────────────────

/// `gmeow-dev box-roles audit [--json]` — audit graph-box role coverage.
pub fn box_roles_audit(json_out: bool) -> i32 {
    let root = project_root();
    let paths = default_audit_paths(&root);
    let audit = match gmeow_validate::box_roles::audit_box_roles(&paths, ONTOLOGY_IRI, NAMESPACE) {
        Ok(a) => a,
        Err(e) => return fail(format!("box-roles audit failed: {e}")),
    };
    if json_out {
        let value = serde_json::json!({
            "term_count": audit.term_count,
            "role_counts": audit.role_counts,
            "missing": audit.missing.iter().map(|f| f.term.clone()).collect::<Vec<_>>(),
            "invalid": audit.invalid.iter().map(|f| f.term.clone()).collect::<Vec<_>>(),
            "ok": audit.missing.is_empty() && audit.invalid.is_empty(),
        });
        match serde_json::to_string_pretty(&value) {
            Ok(s) => println!("{s}"),
            Err(e) => return fail(format!("cannot serialize JSON: {e}")),
        }
    } else {
        println!("box-roles: {} typed term(s)", audit.term_count);
        for (role, count) in &audit.role_counts {
            println!("  {role}: {count}");
        }
        for finding in &audit.missing {
            note(
                "gmeow-dev.box-roles.missing",
                format!("missing {}", finding.term),
            );
        }
        for finding in &audit.invalid {
            note(
                "gmeow-dev.box-roles.invalid",
                format!("invalid {}", finding.term),
            );
        }
    }
    if !audit.missing.is_empty() || !audit.invalid.is_empty() {
        return fail(format!(
            "{} missing, {} invalid role(s)",
            audit.missing.len(),
            audit.invalid.len()
        ));
    }
    0
}

// ── constitution-check ──────────────────────────────────────────────────────

/// `gmeow-dev constitution-check` — verify every principle has live enforcement.
pub fn constitution_check() -> i32 {
    let root = project_root();
    let findings = gmeow_validate::constitution::constitution_full_report(
        &root.join("governance").join("constitution.ttl"),
        &root.join("CONSTITUTION.md"),
        &root,
    );
    let mut report = Report::new("constitution");
    for f in findings {
        report.add_finding(f);
    }
    emit_report(&report);
    if report.ok() {
        println!("constitution check passed");
        0
    } else {
        fail("constitution check failed")
    }
}

// ── crate-check ─────────────────────────────────────────────────────────────

/// `gmeow-dev crate-check` — verify Rust crate layering + repo-static policy.
pub fn crate_check() -> i32 {
    let root = project_root();
    let layering = gmeow_validate::crate_layering::check_crate_layering(&root.join("crates"));
    let static_rep = gmeow_validate::repo_static::check_repo_static(&root);
    let mut report = gmeow_validate::crate_layering::to_diagnostics_report(&layering);
    for f in gmeow_validate::repo_static::to_diagnostics_report(&static_rep).findings {
        report.add_finding(f);
    }
    // A3 docs-loss-lattice gate: the four documentation formats' capability partitions
    // (from the single gmeow_docs::formats source) must be total + monotone. Lives in the
    // pipeline crate because it reads gmeow_docs (a validate→docs edge would cycle the
    // crate DAG); wired here onto the crate-check gate surface.
    let lattice = gmeow_pipeline::docs_loss_lattice::check_docs_loss_lattice();
    for message in &lattice.errors {
        report.add_finding(
            Finding::new(
                Severity::Error,
                "docs-loss-lattice-violation",
                message.clone(),
            )
            .with_tool("docs-loss-lattice"),
        );
    }
    // F4/F5 attestation gate: no documentation format may REPRESENT an interactive
    // capability (LiveSparql / Interactivity / LiveReasoning) unless every shipped engine
    // backing it carries a present, current native↔wasm witness-attestation. Composed with
    // the `wasm-parity` lane on the required CI `make heavy` lane (which RUNS the parity
    // for ALL FOUR engines — query/validate/reason/gmn, on every pull request; it is off
    // the local `make check` only because its cost is breadth, not the change under test)
    // and the digest pin, this enforces the conjunction "the format
    // declares the capability AND its engine's parity is proven-and-current", so the
    // interactive preservation-kind is not a decorative self-claim. Every engine is built
    // in this repository from the workspace purrdf pin; the query engine's witness is the
    // digest-pinned native query attestation its own Node lane byte-compares. A
    // missing/stale attestation HARD-FAILS here.
    for message in gmeow_docs::vendored_asset::check_capability_attestations() {
        report.add_finding(
            Finding::new(
                Severity::Error,
                "interactive-capability-attestation-missing",
                message,
            )
            .with_tool("capability-attestation-gate"),
        );
    }
    // Vendored-corpus license guard: every `crates/*/tests/vendored/*/corpus.json` descriptor
    // must classify IMPORT_OK under `gmeow_license::policy_for_vendored_corpus`, so an
    // unattributed/unfenced (or otherwise restrictive) vendored corpus hard-fails on-gate
    // rather than only in the unit tests that exercise the classifier itself.
    for f in vendored_corpus_license_findings(&root) {
        report.add_finding(f);
    }
    emit_report(&report);
    if report.ok() {
        println!("crate/static guards OK");
        0
    } else {
        fail(format!(
            "{} crate/static violation(s)",
            report.error_count()
        ))
    }
}

/// The four `corpus.json` fields [`gmeow_license::VendoredCorpus`] needs, plus the descriptor
/// path, for a single readable error message when a field is missing or the wrong shape.
struct VendoredCorpusDescriptor {
    spdx_license: String,
    source_url: String,
    attribution: String,
    ring_fenced: bool,
}

/// Enumerate every `crates/*/tests/vendored/*/corpus.json` descriptor and classify it under
/// [`gmeow_license::policy_for_vendored_corpus`], the production license-reuse policy for a
/// vendored third-party corpus.
///
/// No optionality: a present-but-unparseable descriptor, or one missing a required field, is
/// itself a HARD FAIL (an `Error` finding), never a silent skip. A descriptor that parses but
/// classifies as anything other than [`gmeow_license::LicensePolicy::ImportOk`] is likewise an
/// `Error` finding — the corpus was vendored without clearing the license-reuse policy the
/// `gmeow-license` crate defines. An empty return means every vendored corpus in the tree
/// clears vendoring.
fn vendored_corpus_license_findings(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let crates_dir = root.join("crates");
    let Ok(crate_entries) = std::fs::read_dir(&crates_dir) else {
        return findings;
    };
    for crate_entry in crate_entries.flatten() {
        let crate_path = crate_entry.path();
        if !crate_path.is_dir() {
            continue;
        }
        let vendored_dir = crate_path.join("tests").join("vendored");
        if !vendored_dir.is_dir() {
            continue;
        }
        let Ok(corpus_entries) = std::fs::read_dir(&vendored_dir) else {
            continue;
        };
        for corpus_entry in corpus_entries.flatten() {
            let corpus_dir = corpus_entry.path();
            if !corpus_dir.is_dir() {
                continue;
            }
            let descriptor_path = corpus_dir.join("corpus.json");
            if !descriptor_path.is_file() {
                continue;
            }
            match load_vendored_corpus_descriptor(&descriptor_path) {
                Ok(descriptor) => {
                    let corpus = gmeow_license::VendoredCorpus {
                        spdx_license: &descriptor.spdx_license,
                        source_url: &descriptor.source_url,
                        attribution: &descriptor.attribution,
                        ring_fenced: descriptor.ring_fenced,
                    };
                    if gmeow_license::policy_for_vendored_corpus(&corpus)
                        != gmeow_license::LicensePolicy::ImportOk
                    {
                        findings.push(
                            Finding::new(
                                Severity::Error,
                                "vendored-corpus-license-violation",
                                format!(
                                    "{}: spdx_license {:?} does not clear vendoring (ring_fenced={}, attribution={:?}, source_url={:?}) — refuse to vendor or fix the descriptor",
                                    descriptor_path.display(),
                                    descriptor.spdx_license,
                                    descriptor.ring_fenced,
                                    descriptor.attribution,
                                    descriptor.source_url,
                                ),
                            )
                            .with_tool("vendored-corpus-license"),
                        );
                    }
                }
                Err(diag) => {
                    findings.push(
                        Finding::new(
                            Severity::Error,
                            "vendored-corpus-license-invalid",
                            diag.to_string(),
                        )
                        .with_tool("vendored-corpus-license"),
                    );
                }
            }
        }
    }
    findings
}

/// Read + parse one `corpus.json` descriptor into the fields the license policy needs.
/// `Err` carries a human-readable reason: unreadable file, invalid JSON, or a missing/
/// wrong-shaped required field — every case a HARD FAIL, never a silent default.
fn load_vendored_corpus_descriptor(
    path: &Path,
) -> Result<VendoredCorpusDescriptor, gmeow_errors::Diag> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        error::vendored_corpus(format!("{}: cannot read corpus.json: {e}", path.display()))
    })?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| error::vendored_corpus(format!("{}: invalid JSON: {e}", path.display())))?;
    let field_str = |key: &str| -> Result<String, gmeow_errors::Diag> {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                error::vendored_corpus(format!(
                    "{}: missing required field {key:?}",
                    path.display()
                ))
            })
    };
    let spdx_license = field_str("spdx_license")?;
    let source_url = field_str("source_url")?;
    let attribution = field_str("attribution")?;
    let ring_fenced = value
        .get("ring_fenced")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            error::vendored_corpus(format!(
                "{}: missing required field \"ring_fenced\"",
                path.display()
            ))
        })?;
    Ok(VendoredCorpusDescriptor {
        spdx_license,
        source_url,
        attribution,
        ring_fenced,
    })
}

// ── coverage family ─────────────────────────────────────────────────────────

/// `gmeow-dev coverage [--gaps --min-class --min-predicate]` — vendored-entity
/// coverage report, optionally a hard gate against coverage floors.
pub fn coverage(show_gaps: bool, min_class: Option<f64>, min_predicate: Option<f64>) -> i32 {
    let root = project_root();
    let report = match gmeow_validate::coverage::run_coverage(
        &root.join("tests/fixtures/coverage"),
        &root.join("generated/mappings"),
        NAMESPACE,
    ) {
        Ok(r) => r,
        Err(e) => return fail(format!("coverage failed: {e}")),
    };
    let class_total = report.covered_classes.len() + report.gap_classes.len();
    let pred_total = report.covered_predicates.len() + report.gap_predicates.len();
    let class_cov = ratio(report.covered_classes.len(), class_total);
    let pred_cov = ratio(report.covered_predicates.len(), pred_total);
    println!(
        "classes   {} covered / {} gap ({:.0}%)",
        report.covered_classes.len(),
        report.gap_classes.len(),
        class_cov * 100.0
    );
    println!(
        "predicates {} covered / {} gap ({:.0}%)",
        report.covered_predicates.len(),
        report.gap_predicates.len(),
        pred_cov * 100.0
    );
    if show_gaps {
        let mut classes: Vec<&String> = report.gap_classes.iter().collect();
        classes.sort();
        for iri in classes {
            note("gmeow-dev.coverage.gap-class", format!("gap class {iri}"));
        }
        let mut preds: Vec<&String> = report.gap_predicates.iter().collect();
        preds.sort();
        for iri in preds {
            note(
                "gmeow-dev.coverage.gap-predicate",
                format!("gap predicate {iri}"),
            );
        }
    }
    if let Some(floor) = min_class
        && class_cov < floor
    {
        return fail(format!(
            "class coverage {class_cov:.4} is below the required floor {floor:.4}"
        ));
    }
    if let Some(floor) = min_predicate
        && pred_cov < floor
    {
        return fail(format!(
            "predicate coverage {pred_cov:.4} is below the required floor {floor:.4}"
        ));
    }
    0
}

fn ratio(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 }
}

/// `gmeow-dev wikidata-coverage [--json --threshold]`.
pub fn wikidata_coverage(json_mode: bool, threshold: f64) -> i32 {
    let root = project_root();
    match gmeow_validate::mapping_eval::wikidata_coverage(
        &root,
        &root.join("generated/mappings"),
        threshold,
    ) {
        Ok(report) => {
            print!(
                "{}",
                gmeow_validate::mapping_eval::render_wikidata_coverage(&report, json_mode)
            );
            println!();
            0
        }
        Err(e) => fail(format!("wikidata coverage failed: {e}")),
    }
}

/// `gmeow-dev dc-coverage [--json --threshold]`.
pub fn dc_coverage(json_mode: bool, threshold: f64) -> i32 {
    let root = project_root();
    match gmeow_validate::mapping_eval::dc_coverage(&root.join("generated/mappings"), threshold) {
        Ok(report) => {
            print!(
                "{}",
                gmeow_validate::mapping_eval::render_dc_coverage(&report, json_mode)
            );
            println!();
            0
        }
        Err(e) => fail(format!("dc coverage failed: {e}")),
    }
}

/// `gmeow-dev wikidata [--existence --fixtures]` — validate the QIDs/PIDs in use.
pub fn wikidata(existence: bool, fixtures: bool) -> i32 {
    let root = project_root();
    if fixtures {
        let mut paths: Vec<PathBuf> = Vec::new();
        collect_ttl_all(&root.join("tests/fixtures"), &mut paths);
        collect_ttl_named(&root.join("slices"), "module.ttl", &mut paths);
        paths.sort();
        let report = gmeow_validate::wikidata_audit::audit_files(&paths);
        let text = gmeow_validate::wikidata_audit::render_audit(&report);
        if !text.trim().is_empty() {
            note("gmeow-dev.wikidata.fixture-audit", text);
        }
        if report.ok() {
            println!("fixture audit passed");
            return 0;
        }
        return fail(format!("{} error(s)", report.errors()));
    }
    let syntax = match gmeow_validate::mapping_eval::wikidata_mapping_syntax(
        &root.join("generated/mappings"),
    ) {
        Ok(s) => s,
        Err(e) => return fail(format!("wikidata syntax failed: {e}")),
    };
    println!("{} id(s) valid syntax", syntax.valid.len());
    if !syntax.invalid.is_empty() {
        note(
            "gmeow-dev.wikidata.invalid-ids",
            format!("invalid ids: {:?}", syntax.invalid),
        );
    }
    if !syntax.invalid.is_empty() || !syntax.misuses.is_empty() {
        return fail(format!(
            "{} invalid, {} misuse(s)",
            syntax.invalid.len(),
            syntax.misuses.len()
        ));
    }
    if existence {
        // The live lookup: query every syntactically-valid QID/PID against the
        // Wikidata entity API (native `check_existence`, `ureq` under the hood)
        // and hard-fail on any id that does not resolve (missing / redirected).
        let statuses = match gmeow_validate::mapping_eval::check_existence(
            &syntax.valid,
            &root,
            Duration::from_secs(30),
            50,
            Duration::from_millis(100),
        ) {
            Ok(s) => s,
            // A network failure is a visible, non-fatal skip (mirrors the Python
            // `existence check skipped` path), never a silent pass.
            Err(e) => {
                note(
                    "gmeow-dev.wikidata.existence-skipped",
                    format!("existence check skipped: {e}"),
                );
                return 0;
            }
        };
        let mut bad: Vec<(&String, &str)> = statuses
            .iter()
            .filter(|(_, v)| v.as_str() != "ok")
            .map(|(k, v)| (k, v.as_str()))
            .collect();
        // `statuses` is a HashMap; sort by id so the reported failures are in a
        // stable, reproducible order (Principle 18) regardless of hash iteration.
        bad.sort_by(|a, b| a.0.cmp(b.0));
        for (id, status) in &bad {
            note(
                "gmeow-dev.wikidata.existence-fail",
                format!("{id}: {status}"),
            );
        }
        if !bad.is_empty() {
            return fail(format!("{} id(s) failed existence check", bad.len()));
        }
        println!("{} id(s) resolve on Wikidata", statuses.len());
    }
    0
}

/// Recursively collect every `*.ttl` file under `dir`.
fn collect_ttl_all(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ttl_all(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
}

// ── doc-lint ────────────────────────────────────────────────────────────────

/// The site-relative path of the rendered grounding seam-registry page
/// (`gmeow_docs::render::Page::SeamRegistry`), keyed exactly as
/// [`gmeow_docs::render::Site::files`] keys it.
const SEAM_REGISTRY_SITE_PAGE: &str = "seams/index.md";

/// The UNCONDITIONAL leg of the R7 seam-registry drift gate.
///
/// `gmeow-validate`'s on-disk leg can only compare against `ontology-docs/`, which a
/// docs-selected sync writes — and neither `make validate` (no sync) nor the
/// `make check` DAG (`check-sync --outputs generated`) materializes it, so on the
/// gate path that leg has no page to read. Here the page ALWAYS exists: `doc-lint`
/// has just rendered the whole site in memory, and `doc-lint` is itself a
/// `make check` DAG task. So the per-seam comparison
/// ([`gmeow_validate::authoring_integrity::detect_seam_registry_drift`]) runs on
/// every gate run, over the authored `gmeow:Seam` registry read through the one
/// shared reader — and a missing seam page, an unreadable manifest, or any drift is
/// a HARD FAIL, never a skip.
///
/// This is the ONLY direction the dependency can go: `gmeow-docs` depends on
/// `gmeow-validate`, so `gmeow-validate` cannot render the page itself; this crate
/// depends on both and is where the two halves legitimately meet.
fn seam_registry_drift_over_rendered_site(
    root: &Path,
    site: &gmeow_docs::render::Site,
) -> gmeow_errors::Result<Vec<Finding>> {
    let seams = gmeow_validate::authoring_integrity::seam_registry_of_slices(&root.join("slices"))
        .map_err(|e| error::source(format!("cannot read the gmeow:Seam registry: {e}")))?;
    if seams.is_empty() {
        return Err(error::source(format!(
            "no gmeow:Seam individuals discovered under {} — the seam-registry drift \
             comparison would be vacuous, refusing to pass",
            root.join("slices").display(),
        )));
    }
    let Some(bytes) = site.files.get(SEAM_REGISTRY_SITE_PAGE) else {
        return Err(error::source(format!(
            "the rendered site carries no {SEAM_REGISTRY_SITE_PAGE}, but {n} gmeow:Seam \
             individual(s) are declared in the grounding manifests",
            n = seams.len(),
        )));
    };
    let page = std::str::from_utf8(bytes).map_err(|e| {
        error::encoding(format!(
            "the rendered {SEAM_REGISTRY_SITE_PAGE} is not UTF-8: {e}"
        ))
    })?;
    Ok(gmeow_validate::authoring_integrity::detect_seam_registry_drift(&seams, page))
}

/// `gmeow-dev doc-lint` — lint the rust-rendered ontology-docs site.
pub fn doc_lint() -> i32 {
    let root = project_root();
    // The model and the English site come from the content-addressed
    // `.cache/docs-fixture` store, NOT a fresh ~12 s `DocsModel::discover` + render.
    // This is the identical artifact by construction: `fixture::try_load` is
    // byte-identical to `discover()` (its envelope carries the three `#[serde(skip)]`
    // i18n fields explicitly) and `fixture::load_site` is byte-identical to
    // `render_site(&load(root))` (`render_site` IS `render_site_lang(_, ENGLISH)`).
    // The cache key folds every input `discover()` reads plus the whole transitive
    // path-dependency closure of `gmeow-docs`, so no edit that could change what this
    // gate lints can leave the key unmoved; a present-but-corrupt entry panics rather
    // than silently rebuilding. Nothing about what doc-lint ASSERTS changes here —
    // only how many times the same model gets built in one `make check`. The model
    // loader lives in `gmeow-docs-model` and the site loader in `gmeow-docs`; both
    // hang off the one cache key the model crate owns.
    let model = match gmeow_docs_model::fixture::try_load(&root) {
        Ok(m) => m,
        Err(e) => return fail(format!("doc-lint: cannot build model: {e}")),
    };
    let site = gmeow_docs::fixture::load_site(&root);

    // R7 seam-registry drift, over the page just rendered — the leg that makes the
    // per-seam comparison unconditional on-gate (see the helper's doc comment).
    let seam_drift = match seam_registry_drift_over_rendered_site(&root, &site) {
        Ok(findings) => findings,
        Err(e) => return fail(format!("doc-lint: seam-registry drift gate: {e}")),
    };
    for finding in &seam_drift {
        note(
            "gmeow-dev.doc-lint.seam-registry-drift",
            format!("{:?} {}", finding.severity, finding.message),
        );
    }
    let seam_errors = seam_drift
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();

    let report = gmeow_docs::lint(&model, &site);
    let text = render::to_text_summarized(&report.normalized());
    if !text.trim().is_empty() {
        println!("{text}");
    }
    if report.error_count() > 0 || seam_errors > 0 {
        return fail(format!(
            "doc-lint: {} error(s), {} warning(s), {seam_errors} seam-registry drift error(s)",
            report.error_count(),
            report.warning_count()
        ));
    }
    println!(
        "doc-lint OK ({} warning(s)); seam-registry drift OK (per-seam comparison ran)",
        report.warning_count()
    );
    0
}

// ── lint-alignment ──────────────────────────────────────────────────────────

/// `gmeow-dev lint-alignment [--network --strict]`.
pub fn lint_alignment(network: bool, strict: bool) -> i32 {
    let root = project_root();
    let findings =
        match gmeow_pipeline::stages::correspondence_soundness::lint_correspondence_soundness(
            &root, network,
        ) {
            Ok(f) => f,
            Err(e) => return fail(format!("lint-alignment failed: {e}")),
        };
    let errors = findings.iter().filter(|f| f.severity == "ERROR").count();
    let warnings = findings.iter().filter(|f| f.severity == "WARNING").count();
    let infos = findings.iter().filter(|f| f.severity == "INFO").count();
    for f in &findings {
        let line = match &f.instance {
            Some(i) => format!("[{}] {i}: {}", f.check, f.message),
            None => format!("[{}] {}", f.check, f.message),
        };
        match f.severity.as_str() {
            "ERROR" => note("gmeow-dev.lint-alignment.error", format!("error {line}")),
            "WARNING" => note(
                "gmeow-dev.lint-alignment.warning",
                format!("warning {line}"),
            ),
            _ => {}
        }
    }
    if errors > 0 || (strict && warnings > 0) {
        return fail(format!(
            "{errors} error(s), {warnings} warning(s) in alignments"
        ));
    }
    println!("alignment directions OK ({warnings} warning(s), {infos} skipped)");
    0
}

// ── audit ───────────────────────────────────────────────────────────────────

/// `gmeow-dev audit FILES… [--json --strict]` — claim gates over data files.
pub fn audit(files: &[PathBuf], json_out: bool, strict: bool) -> i32 {
    let root = project_root();
    let report = match gmeow_pipeline::scoreboards::claim_audit(&root, files) {
        Ok(r) => r,
        Err(e) => return fail(format!("audit failed: {e}")),
    };
    let diag = gmeow_pipeline::scoreboards::claim_audit_diagnostics(&report);
    if json_out {
        match render::to_json(&diag.normalized()) {
            Ok(s) => println!("{s}"),
            Err(e) => return fail(format!("cannot render JSON: {e}")),
        }
    } else {
        let text = render::to_text(&diag.normalized());
        if !text.trim().is_empty() {
            println!("{text}");
        }
    }
    if !report.shacl_errors.is_empty() {
        return fail(format!("{} SHACL error(s)", report.shacl_errors.len()));
    }
    let flagged: usize = report.findings.values().map(Vec::len).sum();
    if strict && flagged > 0 {
        return fail(format!("{flagged} flagged claim(s) (--strict)"));
    }
    0
}

// ── acceptance ──────────────────────────────────────────────────────────────

/// `gmeow-dev acceptance [SOURCE -o --min-recall]`.
pub fn acceptance(source: Option<&Path>, out: Option<&Path>, min_recall: Option<f64>) -> i32 {
    let root = project_root();
    let results = match gmeow_pipeline::scoreboards::run_acceptance_corpus(&root, source) {
        Ok(r) => r,
        Err(e) => return fail(format!("acceptance failed: {e}")),
    };
    let markdown = gmeow_pipeline::scoreboards::render_acceptance_report(&results);
    match out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &markdown) {
                return fail(format!("cannot write {}: {e}", path.display()));
            }
            note(
                "gmeow-dev.acceptance.wrote",
                format!("wrote {}", path.display()),
            );
        }
        None => println!("{markdown}"),
    }
    for fa in &results {
        note(
            "gmeow-dev.acceptance.result",
            format!(
                "{} {}",
                if fa.passed() { "PASS" } else { "FAIL" },
                fa.source
            ),
        );
    }
    let floor = min_recall.unwrap_or(gmeow_pipeline::scoreboards::ACCEPTANCE_MIN_RECALL_PCT);
    let gate = gmeow_pipeline::scoreboards::aggregate_recall_gate(&results, floor);
    let aggregate = gate.metrics.get("aggregate_recall").copied().unwrap_or(0.0);
    if !gate.passed {
        return fail(format!(
            "corpus-aggregate round-trip recall {aggregate:.2}% is below the floor {floor:.2}% ({} source(s))",
            results.len()
        ));
    }
    note(
        "gmeow-dev.acceptance.recall",
        format!("corpus-aggregate round-trip recall {aggregate:.2}% >= floor {floor:.2}%"),
    );
    0
}

// ── extract (license guard) ─────────────────────────────────────────────────

/// `gmeow-dev extract --target T` — report the import/extract policy for an
/// alignment target, refusing (exit 1) reference-only targets.
pub fn extract(target: &str) -> i32 {
    let Some((name, license)) = crate::dev_targets::target(target) else {
        return fail(format!("unknown alignment target: {target}"));
    };
    match gmeow_license::policy_for_license(license) {
        gmeow_license::LicensePolicy::ImportOk => {
            println!("{name} ({license}) is import-ok — extraction permitted");
            0
        }
        gmeow_license::LicensePolicy::ReferenceOnly => fail(format!(
            "refusing to extract {name} ({license}): reference-only. Link it by IRI instead."
        )),
    }
}

// ── quality (OOPS! / FOOPS!) ─────────────────────────────────────────────────

/// `gmeow-dev quality [--foops-url --strict]` — OOPS! (pitfalls) + optional
/// FOOPS! (FAIR). Network, best-effort unless `--strict`.
pub fn quality(foops_url: &str, strict: bool) -> i32 {
    let root = project_root();
    let bytes = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let base = match gmeow_pipeline::projections::gts_base_graph(&bytes) {
        Ok(b) => b,
        Err(e) => return fail(format!("cannot read base graph: {e}")),
    };
    let flat = match purrdf::flat_dataset_from_quads(&base) {
        Ok(f) => f,
        Err(e) => return fail(format!("cannot flatten base graph: {e}")),
    };
    let ttl = match purrdf::serialize_dataset(&flat, "text/turtle", purrdf::SerializeGraph::Dataset)
    {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => return fail(format!("cannot serialize ontology: {e}")),
    };

    let timeout = Duration::from_secs(120);
    match gmeow_pipeline::cli_ops::quality::run_oops(&ttl, timeout) {
        Ok(report) => println!("OOPS! returned {} bytes", report.len()),
        Err(e) => {
            if strict {
                return fail(format!("OOPS! failed: {e}"));
            }
            note(
                "gmeow-dev.quality.oops-skipped",
                format!("OOPS! skipped: {e}"),
            );
        }
    }
    if !foops_url.is_empty() {
        match gmeow_pipeline::cli_ops::quality::run_foops(foops_url, timeout) {
            Ok(result) => println!(
                "FOOPS! score {:.2} ({}/{})",
                result.score, result.checks_passed, result.checks_total
            ),
            Err(e) => {
                if strict {
                    return fail(format!("FOOPS! failed: {e}"));
                }
                note(
                    "gmeow-dev.quality.foops-skipped",
                    format!("FOOPS! skipped: {e}"),
                );
            }
        }
    }
    0
}

#[cfg(test)]
mod vendored_corpus_license_tests {
    use super::vendored_corpus_license_findings;

    /// A fresh temp directory owned by the returned [`tempfile::TempDir`].
    ///
    /// Bind the guard to a live local (`let (_tmp, root) = tempdir("slug");`): when it
    /// drops the directory and everything written under it is removed, on success, on
    /// early return, and on panic alike.
    fn tempdir(slug: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::Builder::new()
            .prefix(&format!("gmeow-dev-cli-vendored-license-{slug}-"))
            .tempdir()
            .expect("create temp dir");
        let path = tmp.path().to_path_buf();
        (tmp, path)
    }

    fn write_descriptor(root: &std::path::Path, crate_name: &str, corpus_name: &str, json: &str) {
        let dir = root
            .join("crates")
            .join(crate_name)
            .join("tests")
            .join("vendored")
            .join(corpus_name);
        std::fs::create_dir_all(&dir).expect("mkdir vendored corpus dir");
        std::fs::write(dir.join("corpus.json"), json).expect("write corpus.json");
    }

    /// Positive: the real EWT descriptor (ring-fenced, attributed CC-BY-SA-4.0) shipped at
    /// `crates/lang-bridge/tests/vendored/ud-english-ewt/corpus.json` is IMPORT_OK — the gate
    /// yields NO findings against the live tree.
    #[test]
    fn real_ewt_descriptor_is_import_ok_no_findings() {
        let root = crate::dev_common::project_root();
        let descriptor = root
            .join("crates")
            .join("lang-bridge")
            .join("tests")
            .join("vendored")
            .join("ud-english-ewt")
            .join("corpus.json");
        assert!(
            descriptor.is_file(),
            "expected the real EWT descriptor to exist at {}",
            descriptor.display()
        );
        let findings = vendored_corpus_license_findings(&root);
        assert!(
            findings.is_empty(),
            "expected no vendored-corpus-license findings against the live tree, got: {findings:?}"
        );
    }

    /// Negative: a CC-BY-SA-4.0 descriptor that is NOT ring-fenced fails the classifier's
    /// share-alike vendoring exception, so the gate must fold exactly one Error finding — the
    /// proof that this actually hard-fails in production, not just in the classifier's own
    /// unit tests.
    #[test]
    fn unfenced_cc_by_sa_descriptor_yields_one_error_finding() {
        let (_tmp, root) = tempdir("unfenced");
        write_descriptor(
            &root,
            "some-crate",
            "bad-corpus",
            r#"{
                "name": "bad-corpus",
                "treebank": "Bad_Treebank",
                "spdx_license": "CC-BY-SA-4.0",
                "source_url": "https://example.org/treebank.conllu",
                "version_or_commit": "main",
                "fetch_date": "2026-07-11",
                "attribution": "Some credited authors",
                "ring_fenced": false,
                "sent_ids": []
            }"#,
        );
        let findings = vendored_corpus_license_findings(&root);
        assert_eq!(
            findings.len(),
            1,
            "expected exactly one finding for the unfenced CC-BY-SA descriptor: {findings:?}"
        );
        assert_eq!(findings[0].code, "vendored-corpus-license-violation");
    }

    /// Negative: a CC-BY-SA-4.0 descriptor with empty attribution likewise fails the exception
    /// and is folded as an Error finding.
    #[test]
    fn unattributed_cc_by_sa_descriptor_yields_one_error_finding() {
        let (_tmp, root) = tempdir("unattributed");
        write_descriptor(
            &root,
            "some-crate",
            "bad-corpus",
            r#"{
                "name": "bad-corpus",
                "treebank": "Bad_Treebank",
                "spdx_license": "CC-BY-SA-4.0",
                "source_url": "https://example.org/treebank.conllu",
                "version_or_commit": "main",
                "fetch_date": "2026-07-11",
                "attribution": "   ",
                "ring_fenced": true,
                "sent_ids": []
            }"#,
        );
        let findings = vendored_corpus_license_findings(&root);
        assert_eq!(
            findings.len(),
            1,
            "expected exactly one finding for the unattributed CC-BY-SA descriptor: {findings:?}"
        );
        assert_eq!(findings[0].code, "vendored-corpus-license-violation");
    }

    /// A descriptor missing a required field (`ring_fenced`) is itself a HARD FAIL — no
    /// optionality, no silent skip.
    #[test]
    fn descriptor_missing_required_field_is_hard_fail() {
        let (_tmp, root) = tempdir("missing-field");
        write_descriptor(
            &root,
            "some-crate",
            "bad-corpus",
            r#"{
                "name": "bad-corpus",
                "spdx_license": "CC-BY-SA-4.0",
                "source_url": "https://example.org/treebank.conllu",
                "attribution": "Some credited authors"
            }"#,
        );
        let findings = vendored_corpus_license_findings(&root);
        assert_eq!(
            findings.len(),
            1,
            "expected exactly one finding for the descriptor missing ring_fenced: {findings:?}"
        );
        assert_eq!(findings[0].code, "vendored-corpus-license-invalid");
    }
}
