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

use gmeow_diagnostics::{Finding, Location, Report, Severity};
use gmeow_gts::model::Graph;
use oxigraph::model::Term;
use oxigraph::store::Store;
use serde::{Deserialize, Serialize};

use crate::cache::{CachedResult, ValidationCache};
use crate::dsl;
use crate::findings::finding_from_shacl;
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

/// Signature/trust policy configuration for the GTS verification pre-gate.
#[derive(Debug, Clone, Default)]
pub struct SignatureConfig {
    /// Signer KIDs or e-mail addresses considered trusted by this deployment.
    pub trusted_signers: Vec<String>,
    /// Require at least one signature frame to be present in the bundle.
    pub require_signatures: bool,
    /// Require at least one cryptographically valid signature from a trusted signer.
    pub require_trusted_signer: bool,
    /// Optional path to an ASCII-armored OpenPGP public key used instead of the
    /// bundle's embedded `gts:transportKey`.
    pub trusted_key: Option<String>,
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
    /// Project root for the content-addressed `.cache/validate` cache. When
    /// `None`, caching is disabled; Task 4 wires Python to pass `PROJECT_ROOT`
    /// so CI/local reruns share the same cache.
    pub project_root: Option<PathBuf>,
    /// Optional GTS byte bundle. When present, the orchestration builds the
    /// shared store from the bundle instead of from `source_paths`, and the
    /// per-file Turtle phases (syntax check, `owl:sameAs` ban) are skipped.
    pub gts_bytes: Option<Vec<u8>>,
    /// Optional signature/trust policy configuration for the GTS verification
    /// pre-gate (#646). When `None`, signature verification is disabled and the
    /// orchestration behaves as before.
    pub signature_config: Option<SignatureConfig>,
}

/// The result of one validation phase.
#[derive(Debug, Default)]
struct PhaseResult {
    errors: Vec<String>,
    warnings: Vec<String>,
}

/// A complete validation run: shared store, parsed shapes, timings, diagnostics,
/// and any data Python needs to finish phases that stay Python-side.
///
/// The single diagnostic product is [`ValidationRun::report`] — one canonical
/// [`Report`] (#654). The legacy `errors`/`warnings` string surfaces are
/// *derived* from it ([`ValidationRun::errors`] / [`ValidationRun::warnings`]),
/// never separately stored, so there is no dual-truth.
pub struct ValidationRun {
    /// The shared ontology store built from `source_paths`.
    pub store: Store,
    /// The parsed normal SHACL shapes model.
    pub shapes: gmeow_shacl::shapes::Shapes,
    /// Per-phase timing records (populated when requested).
    pub timings: Vec<Timing>,
    /// The single canonical diagnostics report aggregated across all phases.
    pub report: Report,
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
        // String scratch for the cheap lint phases; the SHACL phases produce
        // structured findings directly. All three fold into ONE report at the
        // aggregation points below (#654).
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut shacl_findings: Vec<Finding> = Vec::new();

        if source_paths.is_empty() && options.gts_bytes.is_none() {
            return Err(
                "ValidationRun::run: source_paths must not be empty unless gts_bytes is provided"
                    .to_owned(),
            );
        }

        // Parse GTS bytes once outside the timed store-build phase; the timing
        // should measure only oxigraph construction from the parsed graph.
        let gts_graph: Option<Graph> = if let Some(bytes) = &options.gts_bytes {
            Some(store::read_gts_graph(bytes)?)
        } else {
            None
        };

        // Build the shared store once.
        let store = timed(&mut timings, "build-store", options, None, || {
            if let Some(graph) = &gts_graph {
                store::build_store_from_graph(graph)
            } else {
                let paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
                store::build_store(&paths)
            }
        })?;

        // Parse the normal SHACL shapes once.
        let shapes = timed(&mut timings, "parse-shapes", options, None, || {
            gmeow_shacl::engine::parse_shapes(shapes_ttl)
        })?;

        // Phase 1: Turtle syntax check (only meaningful for per-file sources).
        if options.gts_bytes.is_none() {
            let result = timed(&mut timings, "syntax", options, None, || {
                check_syntax(source_paths)
            })?;
            errors.extend(result.errors);
            warnings.extend(result.warnings);

            // Phase 2: owl:sameAs external-entity ban.
            let result = timed(&mut timings, "sameas-ban", options, None, || {
                check_sameas_ban(
                    source_paths,
                    &lint_config.namespace,
                    &options.sameas_allowlist,
                )
            })?;
            errors.extend(result.errors);
            warnings.extend(result.warnings);
        }

        // Python short-circuits if syntax or sameAs failed — no merged graph work.
        if !errors.is_empty() {
            return Ok(Self {
                store,
                shapes,
                timings,
                report: build_report(errors, warnings, shacl_findings),
                declared_terms: Vec::new(),
            });
        }

        // Phase 3: structural lint.
        let result = timed(&mut timings, "structural-lint", options, None, || {
            let report = lint::structural_lint(&store, lint_config);
            PhaseResult {
                errors: report.errors,
                warnings: report.warnings,
            }
        });
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 4: term-naming lint.
        let result = timed(&mut timings, "term-naming-lint", options, None, || {
            let report = lint::term_naming_lint(&store, lint_config);
            PhaseResult {
                errors: report.errors,
                warnings: report.warnings,
            }
        });
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 5: slice-ownership lint.
        let result = timed(&mut timings, "slice-ownership-lint", options, None, || {
            check_slice_ownership(&options.module_specs, lint_config)
        })?;
        errors.extend(result.errors);
        warnings.extend(result.warnings);

        // Phase 6: declared-term collection for Python's guide-anchor lint.
        let declared_terms = timed(&mut timings, "declared-terms", options, None, || {
            lint::declared_terms(&store, lint_config)
        });

        // Phase 7: reasoning/gUFO invariants.
        let result = timed(&mut timings, "reasoning-invariants", options, None, || {
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

        // Initialize the content-addressed cache if a project root was supplied.
        let cache = options.project_root.as_ref().map(ValidationCache::new);

        // Phase 8: merged SHACL validation against the shared store.
        let merged_shacl_key = if let Some(cache) = cache.as_ref() {
            let source_key = if let Some(graph) = &gts_graph {
                let mut heads: Vec<&[u8]> =
                    graph.segment_heads.iter().map(|h| h.as_slice()).collect();
                heads.sort();
                ValidationCache::cache_key(&heads)
            } else {
                let source_paths_buf: Vec<PathBuf> =
                    source_paths.iter().map(PathBuf::from).collect();
                cache.files_cache_key(&source_paths_buf)?
            };
            let shapes_key = ValidationCache::cache_key(&[shapes_ttl.as_bytes()]);
            let salt = ValidationCache::toolchain_salt();
            ValidationCache::cache_key(&[
                source_key.as_bytes(),
                shapes_key.as_bytes(),
                salt.as_bytes(),
            ])
        } else {
            ValidationCache::cache_key(&[shapes_ttl.as_bytes()])
        };
        let start = Instant::now();
        let (result, meta) = run_cached(cache.as_ref(), "merged-shacl", &merged_shacl_key, || {
            let report = gmeow_shacl::engine::validate(&store, &shapes);
            Ok(shacl_findings_from_report(&report, None))
        })?;
        if options.timings {
            timings.push(Timing {
                phase: "merged-shacl".to_owned(),
                elapsed_ms: start.elapsed().as_millis(),
                metadata: meta,
            });
        }
        shacl_findings.extend(result);

        // Phase 9: example coverage check.
        if let Some(slices_dir) = &options.slices_dir {
            let result = timed(&mut timings, "example-coverage", options, None, || {
                check_example_coverage(slices_dir)
            })?;
            errors.extend(result.errors);
            warnings.extend(result.warnings);

            // Phase 10: per-example SHACL via scoped overlay.
            let start = Instant::now();
            let (result, meta) = check_examples(
                &store,
                &shapes,
                slices_dir,
                cache.as_ref(),
                &merged_shacl_key,
            )?;
            if options.timings {
                timings.push(Timing {
                    phase: "example-shacl".to_owned(),
                    elapsed_ms: start.elapsed().as_millis(),
                    metadata: meta,
                });
            }
            shacl_findings.extend(result);
        }

        // Phase 11: mapping DSL SHACL.
        if !mapping_dsl_dir.is_empty() {
            if let Some(dsl_shapes_ttl) = &options.mapping_shapes_ttl {
                let start = Instant::now();
                let (result, meta) =
                    check_dsl(mapping_dsl_dir, dsl_shapes_ttl, "mapping", cache.as_ref())?;
                if options.timings {
                    timings.push(Timing {
                        phase: "mapping-dsl-shacl".to_owned(),
                        elapsed_ms: start.elapsed().as_millis(),
                        metadata: meta,
                    });
                }
                shacl_findings.extend(result);
            }
        }

        // Phase 12: statement DSL SHACL.
        if !statement_dsl_dir.is_empty() {
            if let Some(dsl_shapes_ttl) = &options.statement_shapes_ttl {
                let start = Instant::now();
                let (result, meta) = check_dsl(
                    statement_dsl_dir,
                    dsl_shapes_ttl,
                    "statement",
                    cache.as_ref(),
                )?;
                if options.timings {
                    timings.push(Timing {
                        phase: "statement-dsl-shacl".to_owned(),
                        elapsed_ms: start.elapsed().as_millis(),
                        metadata: meta,
                    });
                }
                shacl_findings.extend(result);
            }
        }

        Ok(Self {
            store,
            shapes,
            timings,
            report: build_report(errors, warnings, shacl_findings),
            declared_terms,
        })
    }

    /// The error messages, derived from the single [`Report`].
    pub fn errors(&self) -> Vec<String> {
        self.report.legacy_errors()
    }

    /// The warning messages, derived from the single [`Report`].
    pub fn warnings(&self) -> Vec<String> {
        self.report.legacy_warnings()
    }

    /// The single canonical report serialized to JSON (always available).
    pub fn report_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.report.normalized())
    }

    /// Serialize the diagnostic/timing output to JSON.
    ///
    /// The shared [`Store`] and [`gmeow_shacl::shapes::Shapes`] are not
    /// serializable, so the JSON only carries the derived errors/warnings, the
    /// timings, and the declared-term list.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        struct JsonRun {
            errors: Vec<String>,
            warnings: Vec<String>,
            timings: Vec<Timing>,
            declared_terms: Vec<String>,
        }
        serde_json::to_string_pretty(&JsonRun {
            errors: self.errors(),
            warnings: self.warnings(),
            timings: self.timings.clone(),
            declared_terms: self.declared_terms.clone(),
        })
    }
}

/// Fold the cheap-lint string scratch plus the structured SHACL findings into
/// ONE canonical [`Report`] (#654). `from_legacy` turns each error/warning
/// string into a finding, so `report.legacy_errors()/legacy_warnings()`
/// reproduce the original strings exactly; the SHACL findings add focus-node
/// locations on top.
fn build_report(
    errors: Vec<String>,
    warnings: Vec<String>,
    shacl_findings: Vec<Finding>,
) -> Report {
    let mut report = Report::from_legacy("validate", errors, warnings);
    for finding in shacl_findings {
        report.add_finding(finding);
    }
    report
}

/// Convert a SHACL [`ValidationReport`] into structured findings via the
/// [`finding_from_shacl`] bridge, optionally tagging each with the example/DSL
/// source (`origin`) as the finding's primary path so SARIF and the `gmeow:`
/// RDF projection can attribute it.
fn shacl_findings_from_report(
    report: &gmeow_shacl::report::ValidationReport,
    origin: Option<&str>,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = report
        .results
        .iter()
        .map(|result| {
            let mut finding = finding_from_shacl(result);
            // Attribute the example/DSL source file as the finding's PRIMARY
            // physical location (a repo-relative path), keeping the focus-node
            // IRI as that location's logical anchor. SARIF `artifactLocation.uri`
            // must be repo-relative — an absolute IRI is rejected by GitHub
            // code-scanning — so the file, not the IRI, is the physical artifact.
            if let Some(origin) = origin {
                if let Some(primary) = finding.locations.first_mut() {
                    primary.path = Some(origin.to_owned());
                } else {
                    finding.add_location(Location {
                        path: Some(origin.to_owned()),
                        ..Location::default()
                    });
                }
            }
            finding
        })
        .collect();
    // Preserve the original "non-conforming with no results" guard so a failed
    // graph never validates silently when the engine reports zero results.
    if findings.is_empty() && !report.conforms {
        let mut finding = Finding::new(
            Severity::Error,
            "shacl.nonconforming",
            "SHACL validation failed: non-conforming with no results",
        )
        .with_tool("shacl");
        if let Some(origin) = origin {
            finding.add_location(Location {
                path: Some(origin.to_owned()),
                ..Location::default()
            });
        }
        findings.push(finding);
    }
    findings
}

/// Run `closure` and, if timings are enabled, record how long it took.
fn timed<F, T>(
    timings: &mut Vec<Timing>,
    phase: &str,
    options: &ValidateOptions,
    metadata: Option<String>,
    closure: F,
) -> T
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
        metadata,
    });
    result
}

/// Look up cached findings for a phase or compute and store them.
///
/// Returns the findings plus timing metadata describing whether the result came
/// from the cache (`cache-hit`), was freshly computed (`cache-miss`), or could
/// not be cached because no cache root was configured (`cache-disabled`). The
/// cached unit is the structured [`Finding`] list, so a hit preserves SHACL
/// focus nodes and wire coordinates exactly as a fresh compute would (#654).
fn run_cached<F>(
    cache: Option<&ValidationCache>,
    kind: &str,
    key: &str,
    compute: F,
) -> Result<(Vec<Finding>, Option<String>), String>
where
    F: FnOnce() -> Result<Vec<Finding>, String>,
{
    if let Some(cache) = cache {
        if let Some(cached) = cache.read_cached_result(kind, key) {
            return Ok((cached.findings, Some("cache-hit".to_owned())));
        }
        let findings = compute()?;
        cache.write_cached_result(kind, key, &CachedResult::from_findings(findings.clone()))?;
        Ok((findings, Some("cache-miss".to_owned())))
    } else {
        Ok((compute()?, Some("cache-disabled".to_owned())))
    }
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
    cache: Option<&ValidationCache>,
    base_key: &str,
) -> Result<(Vec<Finding>, Option<String>), String> {
    let mut findings: Vec<Finding> = Vec::new();
    let mut hits: usize = 0;
    let mut misses: usize = 0;

    for (name, path) in find_example_files(slices_dir)? {
        let example_key = if let Some(cache) = cache {
            let file_key = cache.files_cache_key(std::slice::from_ref(&path))?;
            ValidationCache::cache_key(&[base_key.as_bytes(), file_key.as_bytes()])
        } else {
            ValidationCache::cache_key(&[base_key.as_bytes(), path.to_string_lossy().as_bytes()])
        };

        let (example_findings, meta) = run_cached(cache, "example-shacl", &example_key, || {
            run_example_shacl(store, shapes, &path, &name)
        })?;
        match meta.as_deref() {
            Some("cache-hit") => hits += 1,
            Some("cache-miss") => misses += 1,
            _ => {}
        }
        findings.extend(example_findings);
    }

    let metadata = if cache.is_some() {
        Some(format!("cache-hit:{hits};cache-miss:{misses}"))
    } else {
        Some("cache-disabled".to_owned())
    };
    Ok((findings, metadata))
}

/// Validate one example file against the ontology + shapes via scoped overlay.
fn run_example_shacl(
    store: &Store,
    shapes: &gmeow_shacl::shapes::Shapes,
    path: &Path,
    name: &str,
) -> Result<Vec<Finding>, String> {
    let example_quads = match parse_file(path) {
        Ok(q) => q,
        Err(e) => {
            return Ok(vec![Finding::new(
                Severity::Error,
                "example.parse",
                format!("example {name}: failed to parse {}: {e}", path.display()),
            )
            .with_tool("validate")]);
        }
    };
    let inserted = scoped_overlay_insert(store, example_quads.iter());
    let report = gmeow_shacl::engine::validate(store, shapes);
    scoped_overlay_remove(store, &inserted);
    Ok(shacl_findings_from_report(&report, Some(name)))
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

/// Phase 11/12: validate a DSL directory against its dedicated SHACL shapes.
fn check_dsl(
    dsl_dir: &str,
    shapes_ttl: &str,
    label: &str,
    cache: Option<&ValidationCache>,
) -> Result<(Vec<Finding>, Option<String>), String> {
    let paths = collect_ttl_paths(dsl_dir)?;
    if paths.is_empty() {
        return Ok((Vec::new(), Some("no-inputs".to_owned())));
    }

    let key = if let Some(cache) = cache {
        let file_key = cache.files_cache_key(&paths)?;
        let shapes_key = ValidationCache::cache_key(&[shapes_ttl.as_bytes()]);
        let salt = ValidationCache::toolchain_salt();
        ValidationCache::cache_key(&[
            file_key.as_bytes(),
            shapes_key.as_bytes(),
            label.as_bytes(),
            salt.as_bytes(),
        ])
    } else {
        ValidationCache::cache_key(&[dsl_dir.as_bytes(), label.as_bytes()])
    };

    run_cached(cache, &format!("dsl-shacl/{label}"), &key, || {
        let merge = dsl::merge_with_provenance(&paths)?;
        let data_store = store::build_store_from_nt(&merge.data_nt)?;
        let shapes = gmeow_shacl::engine::parse_shapes(shapes_ttl)?;
        let report = gmeow_shacl::engine::validate(&data_store, &shapes);
        let focus_to_file: HashMap<String, String> = merge.focus_to_file.into_iter().collect();
        Ok(dsl_findings(&report, &focus_to_file, label))
    })
}

/// Convert DSL SHACL results into structured findings, attributing each to its
/// authored source file (via the focus→file map) as the finding's primary path.
fn dsl_findings(
    report: &gmeow_shacl::report::ValidationReport,
    focus_to_file: &HashMap<String, String>,
    label: &str,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    for result in &report.results {
        let mut finding = finding_from_shacl(result);
        finding.tool = Some(format!("{label}-dsl"));
        if let Term::NamedNode(node) = &result.focus_node {
            if let Some(source) = focus_to_file.get(node.as_str()) {
                if let Some(primary) = finding.locations.first_mut() {
                    primary.path = Some(source.clone());
                } else {
                    finding.add_location(Location {
                        path: Some(source.clone()),
                        ..Location::default()
                    });
                }
            }
        }
        findings.push(finding);
    }
    if findings.is_empty() && !report.conforms {
        findings.push(
            Finding::new(
                Severity::Error,
                format!("{label}-dsl.nonconforming"),
                "SHACL validation failed: non-conforming with no results",
            )
            .with_tool(format!("{label}-dsl")),
        );
    }
    findings
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
        if path.is_dir() && !path.is_symlink() {
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
    use std::collections::{BTreeSet, HashSet};

    use crate::store::dump_store_to_ntriples;

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

    fn minimal_gts_bytes() -> Vec<u8> {
        use gmeow_gts::model::{Term, TermKind};
        use gmeow_gts::writer::Writer;

        let mut graph = gmeow_gts::model::Graph::default();
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/a".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/p".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/b".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        });
        graph.quads.push((0, 1, 2, None));

        let writer = Writer::deterministic(&graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed");
        writer.to_bytes()
    }

    fn minimal_lint_config() -> LintConfig {
        LintConfig {
            namespace: "https://blackcatinformatics.ca/gmeow/".to_owned(),
            ontology_iri: "https://blackcatinformatics.ca/gmeow".to_owned(),
            selector_tokens: BTreeSet::new(),
            core_slice_iris: HashSet::new(),
            annotation_predicates: HashSet::new(),
        }
    }

    #[test]
    fn run_with_gts_bytes_succeeds_with_empty_source_paths() {
        let bytes = minimal_gts_bytes();
        let options = ValidateOptions {
            gts_bytes: Some(bytes),
            ..ValidateOptions::default()
        };

        let run = ValidationRun::run(&[], "", "", "", &minimal_lint_config(), &options)
            .expect("ValidationRun::run with gts_bytes must succeed");

        assert!(
            run.errors().is_empty(),
            "unexpected errors: {:?}",
            run.errors()
        );
        assert!(
            run.warnings().is_empty(),
            "unexpected warnings: {:?}",
            run.warnings()
        );
        // report_json is always available, even on a clean run.
        assert!(run.report_json().is_ok());
        assert_eq!(run.store.len().unwrap(), 1);

        let nt = dump_store_to_ntriples(&run.store).expect("store must serialize to N-Triples");
        assert!(nt.contains("<https://example.org/a>"));
        assert!(nt.contains("<https://example.org/p>"));
        assert!(nt.contains("<https://example.org/b>"));
    }
}
