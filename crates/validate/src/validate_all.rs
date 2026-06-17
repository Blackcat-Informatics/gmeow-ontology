// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust-native validation orchestration that reproduces Python `validate_all()`.
//!
//! The orchestration builds the ontology oxigraph [`Store`] once and parses the
//! SHACL shapes once, then runs every lint/SHACL phase against the shared store.
//! Example files are validated via a scoped overlay (example-only quads are
//! inserted, validated, then removed) so the base store is never contaminated.
//!
//! Timing records are collected when [`ValidateOptions::timings`] is true and
//! can be serialized to JSON alongside the error/warning output.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use oxigraph::model::Term;
use oxigraph::store::Store;
use serde::{Deserialize, Serialize};

use crate::dsl;
use crate::gufo::{self, GufoConfig};
use crate::lint::{self, LintConfig, ModuleSpec};
use crate::store::{self, parse_file};

/// One per-phase timing record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timing {
    /// Human-readable phase name.
    pub phase: String,
    /// Wall-clock elapsed time in milliseconds.
    pub elapsed_ms: u128,
    /// Optional free-form metadata (e.g. number of files processed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

/// Optional/extended inputs for the validation orchestration.
#[derive(Debug, Clone, Default)]
pub struct ValidateOptions {
    /// Record per-phase timings.
    pub timings: bool,
    /// `(subject_display, object)` pairs allowed to use `owl:sameAs` with an
    /// external entity (mirrors `config._SAMEAS_ALLOWLIST`).
    pub sameas_allowlist: Vec<(String, String)>,
    /// `(module_path, expected_slice_iri)` pairs for the slice-ownership lint
    /// (mirrors the registry Python builds in `slice_ownership_lint`).
    pub module_specs: Vec<(String, String)>,
    /// Path to the `slices/` directory. When provided, example coverage and
    /// per-example SHACL validation are run in Rust.
    pub slices_dir: Option<String>,
    /// Turtle text of the mapping DSL SHACL shapes. When provided, mapping DSL
    /// SHACL validation is run in Rust.
    pub mapping_shapes_ttl: Option<String>,
    /// Turtle text of the statement DSL SHACL shapes. When provided, statement
    /// DSL SHACL validation is run in Rust.
    pub statement_shapes_ttl: Option<String>,
}

/// The result of one validation phase.
#[derive(Debug, Default)]
struct PhaseResult {
    errors: Vec<String>,
    warnings: Vec<String>,
}

/// A complete validation run: shared store, parsed shapes, timings, diagnostics,
/// and any data Python needs to finish phases that stay Python-side.
pub struct ValidationRun {
    /// The shared ontology store built from `source_paths`.
    pub store: Store,
    /// The parsed normal SHACL shapes model.
    pub shapes: gmeow_shacl::shapes::Shapes,
    /// Per-phase timing records (populated when requested).
    pub timings: Vec<Timing>,
    /// Error diagnostics aggregated across all phases.
    pub errors: Vec<String>,
    /// Warning diagnostics aggregated across all phases.
    pub warnings: Vec<String>,
    /// Declared GMEOW-term IRIs, for Python's `guide_anchor_lint`.
    pub declared_terms: Vec<String>,
}

impl ValidationRun {
    /// Run the full validation orchestration.
    ///
    /// The phase order matches Python's `validate_all()`:
    /// 1. Turtle syntax check
    /// 2. `owl:sameAs` external-entity ban
    /// 3. Structural lint
    /// 4. Term-naming lint
    /// 5. Slice-ownership lint
    /// 6. Declared-term collection (for Python guide-anchor lint)
    /// 7. Reasoning/gUFO invariants
    /// 8. Merged SHACL validation
    /// 9. Example coverage check
    /// 10. Per-example SHACL via scoped overlay
    /// 11. Mapping DSL SHACL
    /// 12. Statement DSL SHACL
    ///
    /// Phases 9–12 are skipped when their required inputs are absent in
    /// `options`; callers that provide `slices_dir` and the DSL shape texts get
    /// the full gate.
    pub fn run(
        source_paths: &[String],
        shapes_ttl: &str,
        mapping_dsl_dir: &str,
        statement_dsl_dir: &str,
        lint_config: &LintConfig,
        options: &ValidateOptions,
    ) -> Result<Self, String> {
        let mut timings: Vec<Timing> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        // Build the shared store once.
        let store = timed(&mut timings, "build-store", options, || {
            let paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
            store::build_store(&paths)
        })?;

        // Parse the normal SHACL shapes once.
        let shapes = timed(&mut timings, "parse-shapes", options, || {
            gmeow_shacl::engine::parse_shapes(shapes_ttl)
        })?;

        // Phase 1: Turtle syntax check.
        let result = timed(&mut timings, "syntax", options, || {
            check_syntax(source_paths)
        })?;
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 2: owl:sameAs external-entity ban.
        let result = timed(&mut timings, "sameas-ban", options, || {
            check_sameas_ban(
                source_paths,
                &lint_config.namespace,
                &options.sameas_allowlist,
            )
        })?;
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Python short-circuits if syntax or sameAs failed — no merged graph work.
        if !errors.is_empty() {
            return Ok(Self {
                store,
                shapes,
                timings,
                errors,
                warnings,
                declared_terms: Vec::new(),
            });
        }

        // Phase 3: structural lint.
        let result = timed(&mut timings, "structural-lint", options, || {
            let report = lint::structural_lint(&store, lint_config);
            PhaseResult {
                errors: report.errors,
                warnings: report.warnings,
            }
        });
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 4: term-naming lint.
        let result = timed(&mut timings, "term-naming-lint", options, || {
            let report = lint::term_naming_lint(&store, lint_config);
            PhaseResult {
                errors: report.errors,
                warnings: report.warnings,
            }
        });
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 5: slice-ownership lint.
        let result = timed(&mut timings, "slice-ownership-lint", options, || {
            check_slice_ownership(&options.module_specs, lint_config)
        })?;
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 6: declared-term collection for Python's guide-anchor lint.
        let declared_terms = timed(&mut timings, "declared-terms", options, || {
            lint::declared_terms(&store, lint_config)
        });

        // Phase 7: reasoning/gUFO invariants.
        let result = timed(&mut timings, "reasoning-invariants", options, || {
            let cfg = GufoConfig {
                namespace: lint_config.namespace.clone(),
            };
            PhaseResult {
                errors: gufo::reasoning_invariants(&store, &cfg),
                warnings: Vec::new(),
            }
        });
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 8: merged SHACL validation against the shared store.
        let result = timed(&mut timings, "merged-shacl", options, || {
            let report = gmeow_shacl::engine::validate(&store, &shapes);
            format_normal_shacl_results(&report)
        });
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 9: example coverage check.
        if let Some(slices_dir) = &options.slices_dir {
            let result = timed(&mut timings, "example-coverage", options, || {
                check_example_coverage(slices_dir)
            })?;
            errors.extend(result.errors);
            warnings.extend(result.warnings);

            // Phase 10: per-example SHACL via scoped overlay.
            let result = timed(&mut timings, "example-shacl", options, || {
                check_examples(&store, &shapes, slices_dir)
            })?;
            errors.extend(result.errors);
            warnings.extend(result.warnings);
        }

        // Phase 11: mapping DSL SHACL.
        if !mapping_dsl_dir.is_empty() {
            if let Some(dsl_shapes_ttl) = &options.mapping_shapes_ttl {
                let result = timed(&mut timings, "mapping-dsl-shacl", options, || {
                    check_dsl(mapping_dsl_dir, dsl_shapes_ttl, "mapping")
                })?;
                errors.extend(result.errors);
                warnings.extend(result.warnings);
            }
        }

        // Phase 12: statement DSL SHACL.
        if !statement_dsl_dir.is_empty() {
            if let Some(dsl_shapes_ttl) = &options.statement_shapes_ttl {
                let result = timed(&mut timings, "statement-dsl-shacl", options, || {
                    check_dsl(statement_dsl_dir, dsl_shapes_ttl, "statement")
                })?;
                errors.extend(result.errors);
                warnings.extend(result.warnings);
            }
        }

        Ok(Self {
            store,
            shapes,
            timings,
            errors,
            warnings,
            declared_terms,
        })
    }

    /// Serialize the diagnostic/timing output to JSON.
    ///
    /// The shared [`Store`] and [`gmeow_shacl::shapes::Shapes`] are not
    /// serializable, so the JSON only carries the aggregated errors, warnings,
    /// timings, and declared-term list.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        struct JsonRun<'a> {
            errors: &'a [String],
            warnings: &'a [String],
            timings: &'a [Timing],
            declared_terms: &'a [String],
        }
        serde_json::to_string_pretty(&JsonRun {
            errors: &self.errors,
            warnings: &self.warnings,
            timings: &self.timings,
            declared_terms: &self.declared_terms,
        })
    }
}

/// Run `closure` and, if timings are enabled, record how long it took.
fn timed<F, T>(timings: &mut Vec<Timing>, phase: &str, options: &ValidateOptions, closure: F) -> T
where
    F: FnOnce() -> T,
{
    if !options.timings {
        return closure();
    }
    let start = Instant::now();
    let result = closure();
    timings.push(Timing {
        phase: phase.to_owned(),
        elapsed_ms: start.elapsed().as_millis(),
        metadata: None,
    });
    result
}

/// Phase 1: parse every source Turtle file individually and collect syntax errors.
fn check_syntax(source_paths: &[String]) -> Result<PhaseResult, String> {
    let mut result = PhaseResult::default();
    for path in source_paths {
        if let Err(exc) = parse_file(Path::new(path)) {
            result.errors.push(format!("syntax error in {path}: {exc}"));
        }
    }
    Ok(result)
}

/// Phase 2: scan each source for banned `owl:sameAs` links to external entities.
fn check_sameas_ban(
    source_paths: &[String],
    namespace: &str,
    allowlist: &[(String, String)],
) -> Result<PhaseResult, String> {
    let mut result = PhaseResult::default();
    for path in source_paths {
        let quads = match parse_file(Path::new(path)) {
            Ok(q) => q,
            Err(exc) => {
                result.errors.push(format!("failed to parse {path}: {exc}"));
                continue;
            }
        };
        for (subject_text, obj) in store::sameas_violations(&quads, namespace, allowlist) {
            result.errors.push(format!(
                "{path}: banned owl:sameAs to external entity \
                 {subject_text} owl:sameAs {obj} (Principle 5); \
                 use skos:exactMatch or gmeow:authorityLink"
            ));
        }
    }
    Ok(result)
}

/// Phase 5: build per-module stores and run the slice-ownership lint.
fn check_slice_ownership(
    module_specs: &[(String, String)],
    lint_config: &LintConfig,
) -> Result<PhaseResult, String> {
    let mut modules: Vec<(ModuleSpec, Store)> = Vec::new();
    for (module_path, expected_slice_iri) in module_specs {
        let store = store::build_store(&[PathBuf::from(module_path)])?;
        modules.push((
            ModuleSpec {
                module_path: module_path.clone(),
                expected_slice_iri: expected_slice_iri.clone(),
            },
            store,
        ));
    }
    let report = lint::slice_ownership_lint(&modules, lint_config);
    Ok(PhaseResult {
        errors: report.errors,
        warnings: report.warnings,
    })
}

/// Format a normal SHACL report the way Python `run_shacl` does.
fn format_normal_shacl_results(report: &gmeow_shacl::report::ValidationReport) -> PhaseResult {
    use gmeow_shacl::report::Severity;
    let mut result = PhaseResult::default();

    let violations: Vec<String> = report
        .results
        .iter()
        .filter(|r| r.severity == Severity::Violation)
        .map(format_result_line)
        .collect();
    let warnings: Vec<String> = report
        .results
        .iter()
        .filter(|r| r.severity != Severity::Violation)
        .map(format_result_line)
        .collect();

    if !violations.is_empty() {
        result
            .errors
            .push(format!("SHACL violations:\n{}", violations.join("\n")));
    }
    if !warnings.is_empty() {
        result
            .warnings
            .push(format!("SHACL warnings:\n{}", warnings.join("\n")));
    }
    if !report.conforms && violations.is_empty() && warnings.is_empty() {
        result
            .errors
            .push("SHACL validation failed: non-conforming with no results".to_owned());
    }
    result
}

/// Format a SHACL result as `<focus>: <message>` (or just the focus node).
fn format_result_line(result: &gmeow_shacl::report::ValidationResult) -> String {
    let focus = term_to_str(&result.focus_node);
    match &result.message {
        Some(msg) => format!("{focus}: {msg}"),
        None => focus,
    }
}

/// Mirror Python `shacl_engine.term_to_str`.
fn term_to_str(term: &Term) -> String {
    let s = term.to_string();
    if s.starts_with('<') && s.ends_with('>') {
        s[1..s.len() - 1].to_owned()
    } else if let Some(rest) = s.strip_prefix("_:") {
        rest.to_owned()
    } else {
        s
    }
}

/// Phase 9: every slice must ship at least one `examples/*.ttl` file.
fn check_example_coverage(slices_dir: &str) -> Result<PhaseResult, String> {
    let mut result = PhaseResult::default();
    for manifest in find_slice_manifests(slices_dir)? {
        let slice_dir = manifest
            .parent()
            .ok_or_else(|| format!("manifest has no parent: {}", manifest.display()))?;
        let slice_name = slice_dir
            .file_name()
            .ok_or_else(|| format!("slice dir has no name: {}", slice_dir.display()))?
            .to_string_lossy();
        let examples_dir = slice_dir.join("examples");
        let has_example = examples_dir.is_dir()
            && std::fs::read_dir(&examples_dir)
                .map_err(|e| format!("read_dir {}: {e}", examples_dir.display()))?
                .filter_map(|e| e.ok())
                .any(|e| {
                    let p = e.path();
                    p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("ttl")
                });
        if !has_example {
            result.errors.push(format!(
                "slice {slice_name}: no examples/*.ttl — every slice must \
                 ship at least one validating example (#579)"
            ));
        }
    }
    Ok(result)
}

/// Phase 10: validate every slice example against the ontology via scoped overlay.
fn check_examples(
    store: &Store,
    shapes: &gmeow_shacl::shapes::Shapes,
    slices_dir: &str,
) -> Result<PhaseResult, String> {
    let mut result = PhaseResult::default();
    for (name, path) in find_example_files(slices_dir)? {
        let example_quads = parse_file(&path)?;
        let inserted = scoped_overlay_insert(store, example_quads.iter());
        let report = gmeow_shacl::engine::validate(store, shapes);
        scoped_overlay_remove(store, &inserted);

        let (violations, warnings) = partition_shacl_results(&report);
        for v in violations {
            result.errors.push(format!("example {name}: {v}"));
        }
        for w in warnings {
            result.warnings.push(format!("example {name}: {w}"));
        }
    }
    Ok(result)
}

/// Insert only quads that are not already present in `store` and return the
/// inserted set so it can be removed later.
pub fn scoped_overlay_insert<'a>(
    store: &Store,
    quads: impl Iterator<Item = &'a oxigraph::model::Quad>,
) -> Vec<oxigraph::model::Quad> {
    let mut inserted: Vec<oxigraph::model::Quad> = Vec::new();
    for quad in quads {
        if !store_contains_quad(store, quad) {
            // Store insert is fallible in principle; in-memory inserts are not.
            store.insert(quad).expect("in-memory store insert");
            inserted.push(quad.clone());
        }
    }
    inserted
}

/// Remove exactly the quads that were inserted by [`scoped_overlay_insert`].
pub fn scoped_overlay_remove(store: &Store, quads: &[oxigraph::model::Quad]) {
    for quad in quads {
        store.remove(quad).expect("in-memory store remove");
    }
}

/// Check whether `store` already contains `quad`.
fn store_contains_quad(store: &Store, quad: &oxigraph::model::Quad) -> bool {
    store
        .quads_for_pattern(
            Some(quad.subject.as_ref()),
            Some(quad.predicate.as_ref()),
            Some(quad.object.as_ref()),
            Some(quad.graph_name.as_ref()),
        )
        .next()
        .is_some()
}

/// Split a SHACL report into (violations, warnings) using the same formatting
/// as the normal SHACL path.
fn partition_shacl_results(
    report: &gmeow_shacl::report::ValidationReport,
) -> (Vec<String>, Vec<String>) {
    use gmeow_shacl::report::Severity;
    let violations: Vec<String> = report
        .results
        .iter()
        .filter(|r| r.severity == Severity::Violation)
        .map(format_result_line)
        .collect();
    let warnings: Vec<String> = report
        .results
        .iter()
        .filter(|r| r.severity != Severity::Violation)
        .map(format_result_line)
        .collect();
    (violations, warnings)
}

/// Phase 11/12: validate a DSL directory against its dedicated SHACL shapes.
fn check_dsl(dsl_dir: &str, shapes_ttl: &str, label: &str) -> Result<PhaseResult, String> {
    let paths = collect_ttl_paths(dsl_dir)?;
    if paths.is_empty() {
        return Ok(PhaseResult::default());
    }
    let merge = dsl::merge_with_provenance(&paths)?;
    let data_store = store::build_store_from_nt(&merge.data_nt)?;
    let shapes = gmeow_shacl::engine::parse_shapes(shapes_ttl)?;
    let report = gmeow_shacl::engine::validate(&data_store, &shapes);

    let focus_to_file: HashMap<String, String> = merge.focus_to_file.into_iter().collect();
    let violations = format_dsl_results(&report, &focus_to_file);

    let mut result = PhaseResult::default();
    if !violations.is_empty() {
        result.errors.push(format!(
            "{label} DSL SHACL violations:\n  {}",
            violations.join("\n  ")
        ));
    }
    if !report.conforms && violations.is_empty() {
        result
            .errors
            .push("SHACL validation failed: non-conforming with no results".to_owned());
    }
    Ok(result)
}

/// Format DSL SHACL results with focus/path/message/source provenance.
fn format_dsl_results(
    report: &gmeow_shacl::report::ValidationReport,
    focus_to_file: &HashMap<String, String>,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for result in &report.results {
        let mut parts: Vec<String> = Vec::new();
        let focus = term_to_str(&result.focus_node);
        parts.push(format!("focus={focus}"));
        if let Some(path) = &result.result_path {
            parts.push(format!("path={}", term_to_str(path)));
        }
        if let Some(msg) = &result.message {
            parts.push(format!("msg={msg}"));
        }
        if let Term::NamedNode(n) = &result.focus_node {
            if let Some(src) = focus_to_file.get(n.as_str()) {
                parts.push(format!("source={src}"));
            }
        }
        lines.push(parts.join(" | "));
    }
    lines
}

/// Recursively collect all `.ttl` files under `dir`, sorted deterministically.
fn collect_ttl_paths(dir: &str) -> Result<Vec<PathBuf>, String> {
    let root = PathBuf::from(dir);
    let mut paths: Vec<PathBuf> = Vec::new();
    collect_ttl_paths_recursive(&root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_ttl_paths_recursive(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("dir entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_ttl_paths_recursive(&path, paths)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("ttl") {
            paths.push(path);
        }
    }
    Ok(())
}

/// Find every `slices/*/*/manifest.ttl` file under `slices_dir`, sorted.
fn find_slice_manifests(slices_dir: &str) -> Result<Vec<PathBuf>, String> {
    let root = PathBuf::from(slices_dir);
    let mut manifests: Vec<PathBuf> = Vec::new();
    for group in
        std::fs::read_dir(&root).map_err(|e| format!("read_dir {}: {e}", root.display()))?
    {
        let group = group
            .map_err(|e| format!("dir entry in {}: {e}", root.display()))?
            .path();
        if !group.is_dir() {
            continue;
        }
        for slice in
            std::fs::read_dir(&group).map_err(|e| format!("read_dir {}: {e}", group.display()))?
        {
            let slice = slice
                .map_err(|e| format!("dir entry in {}: {e}", group.display()))?
                .path();
            if !slice.is_dir() {
                continue;
            }
            let manifest = slice.join("manifest.ttl");
            if manifest.is_file() {
                manifests.push(manifest);
            }
        }
    }
    manifests.sort();
    Ok(manifests)
}

/// Find every `slices/*/*/examples/*.ttl` file, returning `(relative_posix_name, path)`.
fn find_example_files(slices_dir: &str) -> Result<Vec<(String, PathBuf)>, String> {
    let root = PathBuf::from(slices_dir);
    let mut examples: Vec<(String, PathBuf)> = Vec::new();
    for manifest in find_slice_manifests(slices_dir)? {
        let slice_dir = manifest.parent().expect("manifest has parent");
        let examples_dir = slice_dir.join("examples");
        if !examples_dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&examples_dir)
            .map_err(|e| format!("read_dir {}: {e}", examples_dir.display()))?
        {
            let entry = entry
                .map_err(|e| format!("dir entry in {}: {e}", examples_dir.display()))?
                .path();
            if !entry.is_file() || entry.extension().and_then(|s| s.to_str()) != Some("ttl") {
                continue;
            }
            let name = entry
                .strip_prefix(&root)
                .map_err(|e| {
                    format!(
                        "strip prefix {} from {}: {e}",
                        root.display(),
                        entry.display()
                    )
                })?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            examples.push((name, entry));
        }
    }
    examples.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(examples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::{RdfFormat, RdfParser};

    fn store_from(ttl: &str) -> Store {
        let store = Store::new().unwrap();
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(ttl.as_bytes())
        {
            store.insert(&triple.unwrap()).unwrap();
        }
        store
    }

    fn write_tmp(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn scoped_overlay_does_not_leak_example_quads() {
        let base_ttl = "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n";
        let store = store_from(base_ttl);
        assert_eq!(store.len().unwrap(), 1);

        let example_path = write_tmp(
            "gmeow_validate_overlay_example.ttl",
            "@prefix ex: <https://example.org/> .\nex:c ex:p ex:d .\n",
        );
        let quads = parse_file(&example_path).unwrap();
        std::fs::remove_file(&example_path).ok();

        let inserted = scoped_overlay_insert(&store, quads.iter());
        assert_eq!(inserted.len(), 1, "example-only quad must be inserted");
        assert_eq!(store.len().unwrap(), 2, "overlay quad must be visible");

        scoped_overlay_remove(&store, &inserted);
        assert_eq!(store.len().unwrap(), 1, "base store must be restored");
    }

    #[test]
    fn scoped_overlay_skips_already_present_quads() {
        let base_ttl = "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n";
        let store = store_from(base_ttl);

        let example_path = write_tmp(
            "gmeow_validate_overlay_dup_example.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\nex:c ex:p ex:d .\n",
        );
        let quads = parse_file(&example_path).unwrap();
        std::fs::remove_file(&example_path).ok();

        let inserted = scoped_overlay_insert(&store, quads.iter());
        assert_eq!(inserted.len(), 1, "only the new quad must be inserted");
        assert_eq!(store.len().unwrap(), 2);

        scoped_overlay_remove(&store, &inserted);
        assert_eq!(store.len().unwrap(), 1);
    }
}
