// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust-native validation orchestration.
//!
//! The orchestration builds the ontology [`RdfDataset`] once and parses the SHACL
//! shapes once, then runs every lint/SHACL phase against the shared immutable
//! dataset. Example files are validated in parallel from isolated projected
//! datasets, so the shared base is never contaminated.
//!
//! Timing records are collected when [`ValidateOptions::timings`] is true and
//! can be serialized to JSON alongside the error/warning output.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use std::sync::Arc;

use gmeow_errors::{
    Advice, Diag, DiagLedger, Finding, FindingCategory, Grade, Report, Severity, StageId,
    Standpoint, register_code,
};
use gmeow_logic::certificate::ContradictionPolicy;
use purrdf::{PROJECTION_CODECS, RdfDataset, RdfDatasetBuilder, pair_loss_ledger};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use purrdf::slice::catalog::SliceCatalog;
use purrdf::slice::ownership::{DependencyEdge, OwnershipAnalyzer, OwnershipReport};
use purrdf::slice::{Phase, ToolchainContext, product_unit_key};

use crate::cache::{CachedResult, ValidationCache};
use crate::findings::FailureClassIndex;
use crate::gufo::{self, GufoConfig};
use crate::lint::{self, LintConfig};
use crate::report_bridge::shacl_findings_from_report;
use crate::signature;
use crate::store;

#[cfg(test)]
pub(crate) mod verification_fixture;

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

/// Where the whole-corpus merged-SHACL verdict (Phase 8) comes from.
///
/// This is an explicit source selection, never a switch that can turn the phase off:
/// both variants put a complete merged-SHACL verdict into the run, and there is no
/// third "skip" state.
#[derive(Debug, Clone, Default)]
pub enum MergedShacl {
    /// Run the pass here, over the shared store and the parsed shape union.
    #[default]
    Live,
    /// Consume a verdict already produced over the SAME inputs by the pipeline's
    /// `stage-validate`, rather than validating the whole corpus a second time.
    ///
    /// The caller MUST have proven the record current before constructing this — by
    /// recomputing `stage-validate`'s recorded input digest over the working tree and
    /// hard-failing on absence or mismatch. This type carries findings, not a
    /// promise: an unverified record must never reach it.
    Recorded(Vec<Finding>),
}

/// Optional/extended inputs for the validation orchestration.
#[derive(Debug, Clone, Default)]
pub struct ValidateOptions {
    /// The source of the Phase 8 merged-SHACL verdict — see [`MergedShacl`].
    pub merged_shacl: MergedShacl,
    /// Record per-phase timings.
    pub timings: bool,
    /// `(subject_display, object)` pairs allowed to use `owl:sameAs` with an
    /// external entity (mirrors `config._SAMEAS_ALLOWLIST`).
    pub sameas_allowlist: Vec<(String, String)>,
    /// Path to the `slices/` directory. When provided, example coverage and
    /// per-example SHACL validation are run in Rust.
    pub slices_dir: Option<String>,
    /// Turtle text of the mapping DSL SHACL shapes. When provided, mapping DSL
    /// SHACL validation is run in Rust.
    pub mapping_shapes_ttl: Option<String>,
    /// Turtle text of the statement DSL SHACL shapes. When provided, statement
    /// DSL SHACL validation is run in Rust.
    pub statement_shapes_ttl: Option<String>,
    /// Path to the test DSL vocabulary directory (`dsl/tests/`). When provided
    /// along with `test_dsl_shapes_ttl` and `slices_dir`, test DSL SHACL
    /// validation is run in Rust.
    pub test_dsl_dir: Option<String>,
    /// Turtle text of the test DSL SHACL shapes. When provided, test DSL SHACL
    /// validation is run in Rust.
    pub test_dsl_shapes_ttl: Option<String>,
    /// Repository root whose SHAPE UNION supplies the normal SHACL shapes, loaded
    /// through `purrdf::shapes::shape_union::load_shapes`.
    ///
    /// This is an explicit choice of shape SOURCE, not an optional extra: when it is
    /// set, `shapes_ttl` is not the shape source and must be empty. The two sources
    /// are the repository union (every in-repo caller — the `make validate` gate and
    /// the `--gts` bundle path) and a literal Turtle document (`shapes_ttl`, for
    /// fixtures and benches that have no repository around them).
    ///
    /// The union loader is what makes this run's shape assembly identical to the
    /// pipeline's BY CONSTRUCTION rather than by accident. A caller that instead
    /// concatenated the union members' raw TEXT would parse one document where the
    /// loader parses N and unions them: labelled blank nodes would fuse across files,
    /// a second `@base` would silently re-resolve relative IRIs, and a prefix bound
    /// twice would take the first binding rather than the last. Today's corpus happens
    /// to have none of those, and nothing gated that it stays that way.
    pub shape_union_root: Option<PathBuf>,
    /// Project root for the content-addressed `.cache/validate` cache. When
    /// `None`, caching is disabled; `gmeow-dev validate` passes `PROJECT_ROOT`
    /// so CI/local reruns share the same cache.
    pub project_root: Option<PathBuf>,
    /// Optional GTS byte bundle. When present, the orchestration builds the
    /// shared store from the bundle instead of from `source_paths`, and the
    /// per-file Turtle phases (syntax check, `owl:sameAs` ban) are skipped.
    pub gts_bytes: Option<Vec<u8>>,
    /// Optional signature/trust policy configuration for the GTS verification
    /// pre-gate. When `None`, signature verification is disabled and the
    /// orchestration behaves as before.
    pub signature_config: Option<SignatureConfig>,
    /// When `true`, run the native semantic (`--deep`) pass after the structural
    /// phases: reason over the bundle (`gmeow_logic::reason::reason_all`) and read
    /// the shared `logic:ReasoningResult` to emit semantic findings —
    /// inconsistency (`information=both`), unsatisfiable classes, and undecided DL
    /// constructs. Requires `gts_bytes`. This runs the full reasoner, so it is
    /// opt-in (the structural gate stays fast); the deep pass itself is single-path.
    pub deep: bool,
}

/// The result of one validation phase.
#[derive(Debug, Default)]
struct PhaseResult {
    errors: Vec<String>,
    warnings: Vec<String>,
}

/// The stage the plain-string phases (syntax, `owl:sameAs` ban, reasoning
/// invariants, example coverage) and the structured findings (SHACL, signature,
/// slice ownership) intern under on the single run ledger. The lint sub-ledgers
/// keep their own `validate.lint` stage and fold in via [`DiagLedger::union`].
fn run_stage() -> StageId {
    StageId::new("validate.run")
}

/// Intern one [`PhaseResult`]'s error/warning strings onto the run ledger and
/// report whether it contributed any gate-fatal error.
///
/// These cheap phases carry no richer focus node than the message itself, so the
/// message doubles as the hash-cons focus: two distinct messages get distinct
/// fingerprints and never merge-drop, while an identical message emitted twice
/// correctly dedups.
///
/// This is the DELIBERATE, NARROW exception to the "never key the fingerprint on
/// the message" rule (Hard Invariant 6): these are anchor-less GLOBAL diagnostics
/// (syntax error, banned `owl:sameAs`, reasoning-invariant, coverage) whose
/// identity genuinely IS their message — they have no structural anchor to key on,
/// so keeping distinct messages as distinct findings requires the message to be the
/// focus. It is NOT the general rule: anchored findings ([`intern_finding`]) key on
/// message-INDEPENDENT structural identity and must never fold the message into the
/// fingerprint.
///
/// Errors gate (Error / ModelingDisciplineViolation / Binding —
/// a Blocking category under a Binding standpoint); warnings are perspectival
/// policy notes (Warning / PolicyWarning / Perspectival). The `validate.error` /
/// `validate.warning` codes are the same generic codes the legacy string surface
/// carried, so the projected report is code-identical for these phases.
fn intern_phase(ledger: &mut DiagLedger, phase: PhaseResult) -> bool {
    let had_errors = !phase.errors.is_empty();
    for message in phase.errors {
        let diag = Diag::new(
            register_code("validate.error"),
            Grade::new(
                Severity::Error,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Binding,
            ),
            message.clone(),
        )
        .with_focus(message);
        ledger.attach(diag, run_stage());
    }
    for message in phase.warnings {
        let diag = Diag::new(
            register_code("validate.warning"),
            Grade::new(
                Severity::Warning,
                FindingCategory::PolicyWarning,
                Standpoint::Perspectival,
            ),
            message.clone(),
        )
        .with_focus(message);
        ledger.attach(diag, run_stage());
    }
    had_errors
}

/// Intern one already-structured [`Finding`] (a SHACL result, a slice-ownership
/// defect, or a signature diagnostic) onto the run ledger as a graded [`Diag`],
/// preserving its code, severity, category, message, primary location, secondary
/// (related) locations, detail, tags, and attributions. `standpoint` is the vantage
/// the producer speaks from (Binding for the gate-contributing SHACL / ownership
/// surfaces).
///
/// Every structural anchor is carried as first-class Diag data so the round-trip
/// through `to_finding` is lossless:
///   • the primary location → [`Diag::with_location`];
///   • each `related_locations` entry (the SHACL result-path / offending value) →
///     a [`gmeow_errors::Label`] via [`Diag::with_label`], which `to_finding`
///     projects back into `related_locations`;
///   • `finding.detail` (e.g. "source shape: X") → a [`Diag::with_context`] frame,
///     which `to_finding` folds back into the projected finding's `detail`. Context
///     frames are excluded from the fingerprint, so carrying the detail this way
///     respects Hard Invariant 6.
///
/// The hash-cons focus is the finding's message-INDEPENDENT structural identity
/// (see [`finding_identity_key`]) — every location logical/path plus the detail —
/// so two genuinely distinct findings (the same constraint component on different
/// focus nodes, or two signer diagnostics sharing a code) get distinct fingerprints
/// and never merge-drop, while two findings identical in structure but differing
/// only in message correctly hash-cons-merge (their messages ride as observations).
fn intern_finding(
    ledger: &mut DiagLedger,
    stage: StageId,
    standpoint: Standpoint,
    finding: &Finding,
) {
    let category = finding
        .category
        .unwrap_or(FindingCategory::ModelingDisciplineViolation);
    let mut diag = Diag::new(
        register_code(&finding.code),
        Grade::new(finding.severity, category, standpoint),
        finding.message.clone(),
    )
    .with_focus(finding_identity_key(finding));
    if let Some(location) = finding.locations.first() {
        diag = diag.with_location(location.clone());
    }
    // Carry each secondary anchor (SHACL result-path / offending value) as a
    // first-class labelled span; `to_finding` re-emits these as related locations.
    for related in &finding.related_locations {
        diag = diag.with_label(gmeow_errors::Label {
            text: related.logical.clone().unwrap_or_default(),
            location: related.clone(),
        });
    }
    // Carry the finding's detail (e.g. "source shape: X") as a context frame;
    // `to_finding` folds context frames back into the projected finding's detail,
    // and context frames are correctly excluded from the fingerprint (Invariant 6).
    if let Some(detail) = &finding.detail {
        diag = diag.with_context(detail.clone());
    }
    for suggestion in &finding.suggestions {
        diag = diag.with_advice(Advice {
            standpoint,
            text: suggestion.clone(),
            help_uri: None,
        });
    }
    for tag in &finding.tags {
        diag = diag.with_tag(tag.clone());
    }
    for attribution in &finding.attributions {
        diag = diag.with_attribution(attribution.clone());
    }
    ledger.attach(diag, stage);
}

/// Intern a batch of SHACL findings (merged, per-example, or DSL) onto the run
/// ledger under the `validate.shacl` stage at the Binding gate standpoint.
fn intern_shacl_findings(ledger: &mut DiagLedger, findings: Vec<Finding>) {
    for finding in &findings {
        intern_finding(
            ledger,
            StageId::new("validate.shacl"),
            Standpoint::Binding,
            finding,
        );
    }
}

/// The message-independent structural identity of a [`Finding`] used as its
/// hash-cons focus — the unit separator joins every primary and related location's
/// structural coordinates (logical, path, and line/column when present) plus the
/// detail. The ONLY thing deliberately excluded is `finding.message`: the
/// content-address fingerprint (which folds the focus) must NEVER depend on the
/// message (substrate Hard Invariant 6, see the module docs in
/// `gmeow_errors::ledger`). Line/column ARE part of the identity: two structurally
/// distinct violations of the same constraint at different lines of one file (same
/// `path`, no `logical`) are genuinely different witnesses and must get distinct
/// fingerprints — otherwise one line/message would be silently hash-cons-dropped.
/// Locations without line/column contribute nothing new, so their keys stay
/// byte-stable. Two findings identical in all structural identity (including
/// line/column) but differing only in message ARE the same witness by the
/// substrate's design and SHOULD hash-cons-merge; no message is lost, because
/// `LintReport::messages()` / `errors()` / `warnings()` emit per-observation and
/// the report projection folds every extra observation into the finding's detail.
fn finding_identity_key(finding: &Finding) -> String {
    let mut parts: Vec<String> = Vec::new();
    for location in finding
        .locations
        .iter()
        .chain(finding.related_locations.iter())
    {
        if let Some(logical) = &location.logical {
            parts.push(logical.clone());
        }
        if let Some(path) = &location.path {
            parts.push(path.clone());
        }
        // Structural line/column distinguish two distinct violations at different
        // positions of the same file/constraint. Only appended when present, so
        // locations without them keep their prior byte-stable key.
        if let Some(line) = location.line {
            parts.push(line.to_string());
        }
        if let Some(column) = location.column {
            parts.push(column.to_string());
        }
    }
    if let Some(detail) = &finding.detail {
        parts.push(detail.clone());
    }
    parts.join("\u{1f}")
}

/// The content key of the repository shape union at `root`: each member's
/// repo-relative path, byte length, and bytes, in the loader's own file order.
///
/// Keyed on the same member set and order `purrdf::shapes::shape_union::load_shapes`
/// parses, so the cache entry is invalidated by exactly the edits that change the
/// shapes the engine ran with — including a `generated/shapes/*.ttl` rewrite.
///
/// # Errors
/// If the union file list cannot be built (it fails closed on an empty
/// `generated/shapes/`) or a member cannot be read.
fn shape_union_key_bytes(root: &Path) -> gmeow_errors::Result<Vec<u8>> {
    let files = purrdf::shapes::shape_union::shape_files(root)
        .map_err(|e| Diag::of_kind(crate::error::Parse { detail: e }))?;
    let mut out: Vec<u8> = Vec::new();
    for file in &files {
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(file).map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("reading shape file {}: {e}", file.display()),
            })
        })?;
        out.extend_from_slice(rel.as_bytes());
        out.push(0x1f);
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(&bytes);
        out.push(0x1e);
    }
    Ok(out)
}

/// A complete validation run: shared store, parsed shapes, timings, diagnostics,
/// and the auxiliary data downstream Rust phases consume (declared terms for
/// the authoring-integrity undeclared-term gate; advisory claims for the D4
/// dual-projection materialisation).
///
/// The single diagnostic product is [`ValidationRun::report`] — one canonical
/// [`Report`]. The legacy `errors`/`warnings` string surfaces are
/// *derived* from it ([`ValidationRun::errors`] / [`ValidationRun::warnings`]),
/// never separately stored, so there is no dual-truth.
pub struct ValidationRun {
    /// The shared ontology dataset built from `source_paths` (or the GTS bundle).
    pub dataset: Arc<RdfDataset>,
    /// The parsed normal SHACL shapes model.
    pub shapes: purrdf::shapes::shapes::Shapes,
    /// Per-phase timing records (populated when requested).
    pub timings: Vec<Timing>,
    /// The single canonical diagnostics report aggregated across all phases.
    pub report: Report,
    /// Declared GMEOW-term IRIs collected from this run's dataset via
    /// [`lint::declared_terms_dataset`] — the same collector
    /// [`crate::authoring_integrity`] uses independently for its
    /// undeclared-term gate.
    pub declared_terms: Vec<String>,
    /// The dual-projection claim hooks for advisory findings (D1);
    /// D4 materialises them as RDF.
    pub advisory_claims: Vec<crate::advisory::AdvisoryClaim>,
}

impl ValidationRun {
    /// Run the full validation orchestration.
    ///
    /// The phases run in this fixed order:
    /// 1. Turtle syntax check
    /// 2. `owl:sameAs` external-entity ban
    /// 3. Structural lint
    /// 4. Term-naming lint
    /// 5. Slice-ownership lint
    /// 6. Declared-term collection (feeds the authoring-integrity
    ///    undeclared-term gate)
    /// 7. Reasoning/gUFO invariants
    /// 8. Merged SHACL validation
    /// 9. Example coverage check
    /// 10. Per-example SHACL via per-worker scoped overlay (parallel)
    /// 11. Mapping DSL SHACL
    /// 12. Statement DSL SHACL
    /// 13. Test DSL SHACL
    ///
    /// Phases 9–13 are skipped when their required inputs are absent in
    /// `options`; callers that provide `slices_dir` and the DSL shape texts get
    /// the full gate.
    pub fn run(
        source_paths: &[String],
        shapes_ttl: &str,
        mapping_dsl_dir: &str,
        statement_dsl_dir: &str,
        lint_config: &LintConfig,
        options: &ValidateOptions,
    ) -> gmeow_errors::Result<Self> {
        let mut timings: Vec<Timing> = Vec::new();
        // The SINGLE run-level carrier: every producer — the cheap string phases,
        // the SHACL/ownership/signature findings, the lint sub-ledgers, and the
        // advisory diagnostic — interns onto this one hash-consed ledger, and the
        // final report is its projection. There is no independent Vec<Finding> /
        // Vec<String> findings store (invariant 8).
        let mut run_ledger = DiagLedger::new();

        if source_paths.is_empty() && options.gts_bytes.is_none() {
            return Err(Diag::of_kind(crate::error::Argument {
                detail:
                    "ValidationRun::run: source_paths must not be empty unless gts_bytes is provided"
                        .to_owned(),
            }));
        }

        // One authoritative native import retains the envelope and the exact archive
        // selection. The shared dataset supplies both the flat validation view and
        // the graph-preserving deep pass; segment heads need no separate Graph fold.
        let imported = timed(&mut timings, "import-bundle", options, None, || {
            options
                .gts_bytes
                .as_deref()
                .map(|bytes| import_bundle_for_validation(bytes, options.deep))
                .transpose()
        })?;

        // Parse every source Turtle file exactly once before the timed store-build
        // phase. The per-file frozen datasets are reused by:
        //   • the `build-store` timed phase (merge into the shared dataset),
        //   • Phase 1: syntax check (report Err entries),
        //   • Phase 2: sameAs ban (scan Ok entries).
        // This eliminates the ~3× redundant parse that existed when each phase parsed
        // the file independently.
        let parsed_sources: Vec<(PathBuf, gmeow_errors::Result<Arc<RdfDataset>>)> =
            if options.gts_bytes.is_none() {
                source_paths
                    .iter()
                    .map(|p| {
                        let path = PathBuf::from(p);
                        let res = store::parse_file_dataset(&path);
                        (path, res)
                    })
                    .collect()
            } else {
                Vec::new()
            };

        // Build the shared dataset once: from the GTS bundle (flattened) or by merging
        // the per-file parsed datasets under fresh blank scopes.
        let dataset = timed(&mut timings, "build-store", options, None, || {
            if let Some(imported) = &imported {
                crate::data_validate::flatten_to_default_graph(&imported.bundle.dataset)
            } else {
                merge_parsed_sources(&parsed_sources)
            }
        })?;

        // Parse the normal SHACL shapes once, from whichever of the two shape SOURCES
        // the caller selected (see `ValidateOptions::shape_union_root`). Supplying both
        // is a caller bug, not a precedence question — reject it rather than pick one.
        if options.shape_union_root.is_some() && !shapes_ttl.is_empty() {
            return Err(Diag::of_kind(crate::error::Argument {
                detail: "ValidationRun::run: shape_union_root and a non-empty shapes_ttl are two \
                         different shape sources; supply exactly one"
                    .to_owned(),
            }));
        }
        let shapes = timed(
            &mut timings,
            "parse-shapes",
            options,
            None,
            || match &options.shape_union_root {
                Some(root) => purrdf::shapes::shape_union::load_shapes(root)
                    .map(|(_, shapes)| shapes)
                    .map_err(|e| Diag::of_kind(crate::error::Parse { detail: e })),
                None => purrdf::shapes::engine::parse_shapes(shapes_ttl, None)
                    .map_err(|e| Diag::of_kind(crate::error::Parse { detail: e })),
            },
        )?;

        // Failure classes and advisory provenance read the exact source dataset
        // retained by these parsed shapes, including its original blank scopes.
        let failure_classes = FailureClassIndex::from_shapes_dataset(shapes.dataset());

        // Signature/trust verification pre-gate.
        // Runs after the GTS bundle has been folded into a graph but before any
        // ontology validation phases, so malformed, unsigned, or untrusted bundles
        // are rejected early.
        let mut signature_hard_failures = false;
        if let (Some(bytes), Some(config)) = (&options.gts_bytes, &options.signature_config) {
            let (findings, hard) = timed(&mut timings, "signature-verify", options, None, || {
                signature::verify_gts_bundle(bytes, config)
            })?;
            // Signature diagnostics carry their own category (PolicyWarning); the
            // vantage is the Binding gate. Interned before the hard-failure gate so
            // a hard-failed run still projects them.
            for finding in &findings {
                intern_finding(
                    &mut run_ledger,
                    StageId::new("validate.signature"),
                    Standpoint::Binding,
                    finding,
                );
            }
            signature_hard_failures = hard;
        }

        if signature_hard_failures {
            let mut report = run_ledger.project_report("validate");
            crate::rule_catalog::populate_rules(&mut report);
            return Ok(Self {
                dataset,
                shapes,
                timings,
                report,
                declared_terms: Vec::new(),
                advisory_claims: Vec::new(),
            });
        }

        // Phase 1: Turtle syntax check (only meaningful for per-file sources).
        // `short_circuit` tracks the syntax / sameAs errors specifically: the run
        // short-circuits iff syntax or sameAs failed (signature errors above
        // never drive it).
        let mut short_circuit = false;
        if options.gts_bytes.is_none() {
            let result = timed(&mut timings, "syntax", options, None, || {
                check_syntax_from_parsed(&parsed_sources)
            })?;
            short_circuit |= intern_phase(&mut run_ledger, result);

            // Phase 2: owl:sameAs external-entity ban.
            let result = timed(&mut timings, "sameas-ban", options, None, || {
                check_sameas_ban_from_parsed(
                    &parsed_sources,
                    &lint_config.namespace,
                    &options.sameas_allowlist,
                )
            })?;
            short_circuit |= intern_phase(&mut run_ledger, result);
        }

        // Short-circuit iff syntax or sameAs failed — no merged graph work.
        if short_circuit {
            let mut report = run_ledger.project_report("validate");
            crate::rule_catalog::populate_rules(&mut report);
            return Ok(Self {
                dataset,
                shapes,
                timings,
                report,
                declared_terms: Vec::new(),
                advisory_claims: Vec::new(),
            });
        }

        // Phase 3: structural lint — fold its graded `validate.lint.*` sub-ledger
        // into the run ledger (never re-stringified).
        let lint_report = timed(&mut timings, "structural-lint", options, None, || {
            lint::structural_lint_dataset(&dataset, lint_config)
        });
        run_ledger.union(lint_report.ledger());

        // Phase 4: term-naming lint — same union fold.
        let lint_report = timed(&mut timings, "term-naming-lint", options, None, || {
            lint::term_naming_lint_dataset(&dataset, lint_config)
        });
        run_ledger.union(lint_report.ledger());

        // Phase 6: declared-term collection, feeding the authoring-integrity
        // undeclared-term gate.
        let declared_terms = timed(&mut timings, "declared-terms", options, None, || {
            lint::declared_terms_dataset(&dataset, lint_config)
        });

        // Phase 7: reasoning/gUFO invariants.
        let result = timed(&mut timings, "reasoning-invariants", options, None, || {
            let cfg = GufoConfig {
                namespace: lint_config.namespace.clone(),
            };
            PhaseResult {
                errors: gufo::reasoning_invariants(&dataset, &cfg),
                warnings: Vec::new(),
            }
        });
        intern_phase(&mut run_ledger, result);

        // Initialize the content-addressed cache if a project root was supplied.
        let cache = options.project_root.as_ref().map(ValidationCache::new);

        // Phase 5: slice ownership defects. The full slice-ownership feedback
        // surface still reports dependency observations as warnings; the validate
        // gate folds only ownership defects, preserving the same gating surface
        // while avoiding a second ownership-analysis pass over the same dataset.
        //
        // The ownership/catalog pass is needed only for the cached real-repo
        // gate: it supplies both those ownership-defect errors and the semantic
        // merged-SHACL source key. No-cache harnesses may pass a minimal
        // `slices_dir` solely to collect test DSL files, so they keep the
        // pre-existing shapes-only cache-key behavior instead of requiring a full
        // slice manifest catalog.
        let slice_analysis = if cache.is_some() {
            if let Some(slices_dir) = &options.slices_dir {
                Some(timed(
                    &mut timings,
                    "slice-ownership",
                    options,
                    None,
                    || slice_catalog_and_ownership(slices_dir),
                )?)
            } else {
                None
            }
        } else {
            None
        };
        if let Some((catalog, ownership)) = &slice_analysis {
            for finding in
                crate::slice_peerage::peerage_aware_ownership_findings(ownership, catalog)?
                    .into_iter()
                    .filter(|finding| finding.severity == Severity::Error)
            {
                intern_finding(
                    &mut run_ledger,
                    StageId::new("validate.ownership"),
                    Standpoint::Binding,
                    &finding,
                );
            }
        }

        // Phase 5b: ontology-surface authoring gates — the whole-corpus structural
        // invariants (shape-IRI ownership, graft isolation, slice discipline). Same
        // cached-real-repo posture as ownership: both read the on-disk slice/shape
        // corpus, so they fold exactly when `slice_analysis` ran (cache + slices_dir
        // present). Error findings gate `make validate` — a duplicate slice IRI, a
        // missing tier, a merged-shape IRI collision, or a norms graft leaking into
        // the core `rights` module HARD-FAILS on the live path, not just in a test.
        // Ontology-surface authoring gates run whenever `project_root` names a real
        // repository source tree (it carries `slices/` and `shapes/`), deriving the
        // slice tree from the repo root when `slices_dir` is not supplied
        // explicitly. Gating on the repo-source MARKERS — NOT `slice_analysis` — is
        // deliberate: the live `gmeow-dev validate` / `make validate` entry sets
        // `project_root` but not `slices_dir`, so this fold fires there and its
        // Error findings HARD-FAIL the live gate (a merged-shape IRI collision, a
        // norms graft leak, a duplicate slice IRI, a missing tier, an undeclared
        // term, or an untagged localizable literal). A `--gts` bundle validation or
        // a repo-free cache harness has no `slices/`/`shapes/` source tree, so the
        // gates correctly do not apply there (feature scoping, not degradation).
        if let Some(project_root) = &options.project_root
            && project_root.join("slices").is_dir()
            && project_root.join("shapes").is_dir()
        {
            let slices_path = options.slices_dir.as_deref().map_or_else(
                || project_root.join("slices"),
                |d| std::path::Path::new(d).to_path_buf(),
            );
            let authoring = crate::authoring_integrity::authoring_integrity_findings(
                project_root,
                &slices_path,
            )?;
            // EVERY authoring finding is folded, not just the Errors. An Error is
            // Binding (it hard-fails the run); a non-Error is Advisory (it is
            // reported and never gates). Dropping the non-Errors would silently
            // discard the R7 seam-registry gate's "NOT COMPARED against a
            // materialized page" record — the one thing that must never vanish, since
            // its whole purpose is to keep an uncompared projection from reading as a
            // clean one. Advisory findings do not affect `ValidationRun::ok`, so the
            // gate's hard-fail surface is unchanged.
            for finding in authoring {
                let standpoint = if finding.severity == Severity::Error {
                    Standpoint::Binding
                } else {
                    Standpoint::Advisory
                };
                intern_finding(
                    &mut run_ledger,
                    StageId::new("validate.authoring_integrity"),
                    standpoint,
                    &finding,
                );
            }
        }

        // Phase 5c: ownership + example-coverage on the live repo-source path.
        // Phase 5 (validate.ownership) and Phase 9 (example-coverage) gate on
        // `slices_dir`, which the live `gmeow-dev validate` / `make validate` entry
        // never sets — so both were DARK there. When `project_root` names a real
        // source tree (carries `slices/` + `shapes/`) but no explicit `slices_dir`
        // drove `slice_analysis`, derive the slice tree from the repo root and fold
        // both gates so an ownership defect or a missing example HARD-FAILS live,
        // exactly like Phase 5b. Guarded on `slice_analysis.is_none()` so a harness
        // that supplies `slices_dir` never runs these gates twice.
        if slice_analysis.is_none()
            && let Some(project_root) = &options.project_root
            && project_root.join("slices").is_dir()
            && project_root.join("shapes").is_dir()
        {
            let slices_path = options.slices_dir.as_deref().map_or_else(
                || project_root.join("slices"),
                |d| std::path::Path::new(d).to_path_buf(),
            );
            let slices_path_str = slices_path.to_string_lossy().into_owned();
            let (catalog, ownership) =
                timed(&mut timings, "slice-ownership-live", options, None, || {
                    slice_catalog_and_ownership(&slices_path_str)
                })?;
            for finding in
                crate::slice_peerage::peerage_aware_ownership_findings(&ownership, &catalog)?
                    .into_iter()
                    .filter(|finding| finding.severity == Severity::Error)
            {
                intern_finding(
                    &mut run_ledger,
                    StageId::new("validate.ownership"),
                    Standpoint::Binding,
                    &finding,
                );
            }
            let coverage = timed(&mut timings, "example-coverage-live", options, None, || {
                check_example_coverage(&slices_path_str)
            })?;
            intern_phase(&mut run_ledger, coverage);
        }

        // Phase 8: merged SHACL validation against the shared store.
        //
        // The whole-ontology merged-SHACL source key is the S6a semantic Merkle
        // PRODUCT key over the slice composition (RFC §12): path-independent
        // (renaming a slice's group dir does not bust the key) and
        // comment-insensitive (a comment-only module/manifest edit folds the same
        // *semantic* digest). Three mutually exclusive sources, no silent
        // degraded path (no-optionality):
        //   • native bundle present → segment heads (already content-addressed).
        //   • slices_dir present → semantic Merkle product key over the catalog.
        //   • neither            → shapes-only key (the no-root case is preserved).
        let merged_shacl_key = if let Some(cache) = cache.as_ref() {
            let source_key = if let Some(imported) = &imported {
                native_segment_heads_cache_key(&imported.bundle.envelope)?
            } else if let Some((catalog, ownership)) = &slice_analysis {
                // Reuse the Phase 5 catalog + S4 dependency edges so validate does
                // not run the ownership analyzer twice. A catalog/ownership failure
                // when slices_dir IS present is a HARD failure — never a silent
                // fall-back to the byte-sensitive files key.
                merged_shacl_merkle_root_from_parts(catalog, &ownership.edges)?
            } else {
                let source_paths_buf: Vec<PathBuf> =
                    source_paths.iter().map(PathBuf::from).collect();
                cache.files_cache_key(&source_paths_buf)?
            };
            // The shape leg of the key: the literal document when that is the source,
            // else the union members' own bytes in union order. Both are exact content
            // keys over the shapes actually parsed — a shape edit busts the key either
            // way.
            let union_key_bytes = match &options.shape_union_root {
                Some(root) => Some(shape_union_key_bytes(root)?),
                None => None,
            };
            let shapes_key = ValidationCache::cache_key(&[union_key_bytes
                .as_deref()
                .unwrap_or(shapes_ttl.as_bytes())]);
            let salt = ValidationCache::toolchain_salt();
            ValidationCache::cache_key(&[
                source_key.as_bytes(),
                shapes_key.as_bytes(),
                salt.as_bytes(),
            ])
        } else {
            match &options.shape_union_root {
                Some(root) => ValidationCache::cache_key(&[&shape_union_key_bytes(root)?]),
                None => ValidationCache::cache_key(&[shapes_ttl.as_bytes()]),
            }
        };
        // `merged_shacl_key` is computed above in BOTH source modes: Phase 10
        // (`check_examples`) consumes it as its own per-example cache salt, so it is
        // load-bearing beyond this phase and must not be made conditional.
        let start = Instant::now();
        let (result, meta) = match &options.merged_shacl {
            MergedShacl::Live => {
                run_cached(cache.as_ref(), "merged-shacl", &merged_shacl_key, || {
                    // No `rdf:type` pre-materialization: the engine closes `sh:class`/`sh:targetClass`
                    // over the asserted `rdfs:subClassOf` chain, and every projected `sh:sparql` /
                    // `sh:SPARQLTarget` body now reads class membership through the `a/<subClassOf>*`
                    // property path (constraint projector + the legacy shape bodies), so the raw dataset
                    // is validated directly.
                    let report = store::shacl_validate_dataset(&dataset, &shapes);
                    Ok(shacl_findings_from_report(&report, None, &failure_classes))
                })?
            }
            // The verdict the pipeline already recorded over the same inputs, proven
            // current by the caller. Folded through the SAME `intern_shacl_findings`
            // path a live pass takes, so a recorded violation reaches the report and
            // the exit code exactly as a live one does.
            MergedShacl::Recorded(findings) => (
                findings.clone(),
                Some("stage-validate recorded verdict".to_owned()),
            ),
        };
        if options.timings {
            timings.push(Timing {
                phase: "merged-shacl".to_owned(),
                elapsed_ms: start.elapsed().as_millis(),
                metadata: meta,
            });
        }
        intern_shacl_findings(&mut run_ledger, result);

        // Phase 9: example coverage check.
        if let Some(slices_dir) = &options.slices_dir {
            let result = timed(&mut timings, "example-coverage", options, None, || {
                check_example_coverage(slices_dir)
            })?;
            intern_phase(&mut run_ledger, result);

            // Phase 10: per-example SHACL via per-example base ∪ example dataset.
            let start = Instant::now();
            let (result, meta) = check_examples(
                &dataset,
                &shapes,
                &failure_classes,
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
            intern_shacl_findings(&mut run_ledger, result);
        }

        // Phases 11-13: mapping / statement / test DSL SHACL. Each builds its OWN
        // merged store and runs one independent SHACL pass, so the three run
        // concurrently via `rayon::join`. Each closure keeps its original guard and
        // builds its own `Timing`; results are folded — and timings pushed — in fixed
        // (mapping, statement, test) order AFTER the join, so the shared `timings`
        // vec is never touched concurrently and the output stays deterministic.
        type DslPhaseResult = gmeow_errors::Result<Option<(Vec<Finding>, Timing)>>;

        let dsl_mapping = || -> DslPhaseResult {
            if mapping_dsl_dir.is_empty() {
                return Ok(None);
            }
            let Some(dsl_shapes_ttl) = &options.mapping_shapes_ttl else {
                return Ok(None);
            };
            let start = Instant::now();
            let paths = collect_ttl_paths(mapping_dsl_dir)?;
            let (result, meta) = check_dsl(&paths, dsl_shapes_ttl, "mapping", cache.as_ref())?;
            Ok(Some((
                result,
                Timing {
                    phase: "mapping-dsl-shacl".to_owned(),
                    elapsed_ms: start.elapsed().as_millis(),
                    metadata: meta,
                },
            )))
        };

        let dsl_statement = || -> DslPhaseResult {
            if statement_dsl_dir.is_empty() {
                return Ok(None);
            }
            let Some(dsl_shapes_ttl) = &options.statement_shapes_ttl else {
                return Ok(None);
            };
            let start = Instant::now();
            let paths = collect_ttl_paths(statement_dsl_dir)?;
            let (result, meta) = check_dsl(&paths, dsl_shapes_ttl, "statement", cache.as_ref())?;
            Ok(Some((
                result,
                Timing {
                    phase: "statement-dsl-shacl".to_owned(),
                    elapsed_ms: start.elapsed().as_millis(),
                    metadata: meta,
                },
            )))
        };

        let dsl_test = || -> DslPhaseResult {
            let (Some(test_dsl_dir), Some(dsl_shapes_ttl)) =
                (&options.test_dsl_dir, &options.test_dsl_shapes_ttl)
            else {
                return Ok(None);
            };
            if test_dsl_dir.is_empty() {
                return Ok(None);
            }
            let start = Instant::now();
            let mut paths = collect_ttl_paths(test_dsl_dir)?;
            if let Some(slices_dir) = &options.slices_dir {
                paths.extend(collect_slice_test_files(slices_dir)?);
            }
            paths.sort();
            if paths.is_empty() {
                return Ok(None);
            }
            let (result, meta) = check_dsl(&paths, dsl_shapes_ttl, "test", cache.as_ref())?;
            Ok(Some((
                result,
                Timing {
                    phase: "test-dsl-shacl".to_owned(),
                    elapsed_ms: start.elapsed().as_millis(),
                    metadata: meta,
                },
            )))
        };

        let (mapping_res, (statement_res, test_res)) =
            rayon::join(dsl_mapping, || rayon::join(dsl_statement, dsl_test));

        for phase_res in [mapping_res, statement_res, test_res] {
            if let Some((result, timing)) = phase_res? {
                intern_shacl_findings(&mut run_ledger, result);
                if options.timings {
                    timings.push(timing);
                }
            }
        }

        // The single carrier is complete: project it to the one canonical report.
        let mut report = run_ledger.project_report("validate");

        // Advisory tier (data-matched): the merged-SHACL phase already interned every
        // result — including the Info-severity advisory-constraint matches — as `shacl.*`
        // findings. Split those out of the projected report: each Info `shacl.*` finding
        // whose source shape carries a `logic:formalizes` is an instance whose data matched
        // an advisory anti-pattern guard. Its raw finding is SUPPRESSED and re-projected as
        // a Note + deonticRecommendation advisory. Advice fires from a DATA MATCH, never
        // merely because a rule exists. Find harvested findings via the "advisory-harvested"
        // tag. (CLI twin of the pipeline's result-based split; both build advisories through
        // `advisory::build_advisory`, so the two surfaces cannot drift.)
        // The advisory split reads the retained shape dataset. There is no reparse
        // or parse-error fallback that could silently drop requested advice.
        let advisories =
            crate::advisory::split_advisory_findings(&mut report, shapes.dataset(), &dataset);
        let mut advisory_ledger = DiagLedger::new();
        let mut advisory_claims = Vec::with_capacity(advisories.len());
        for advisory in &advisories {
            let projection = advisory.project();
            advisory_ledger.attach(projection.diag, StageId::new("validate.advisory"));
            advisory_claims.push(projection.claim);
            report.add_rule(advisory.rule());
        }
        // D5 abductive tier (CLI twin of the pipeline wiring): the constructive "what to ADD"
        // wing. Each warranted candidate is a warrant-as-Finding (attached first, its DiagRef
        // captured) plus an advisory whose diag carries a genuine finding→finding antecedent to
        // that warrant, so the warrant join resolves non-DARK. The producer is ENGINE-FREE — the
        // relatum path warrants by construction, the sortal path by a sound class-disjointness
        // lookup — and `dataset` is only READ, never mutated. Both wings ride the same
        // dual-projection loop → the `gmeow` CLI surfaces D5 with closed warrant edges.
        //
        // `dataset` IS the reasoned surface the producer's `reasoned` parameter names: when a
        // `gmeow.gts` bundle is validated it is `dataset_from_gts`, which already carries the
        // reason stage's folded closure (entailed types/relata), so the abductive tier sees
        // entailment. A raw-source run has no reasoner, so `dataset` is the merged
        // asserted graph only — an HONEST asserted-only surface (no fabricated reasoning), the
        // exact contract the producer doc records. There is no authored-only surface masquerading
        // as reasoned: the pipeline path unions the real closure, this path passes the real bundle.
        let abductive_suggestions = crate::abductive::abductive_advisories(&dataset);
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
        // Flat findings after the ledger is fully attached (findings("validate") reads the batch).
        for note in advisory_ledger.findings("validate") {
            report.add_finding(note);
        }

        // Semantic (`--deep`) pass (ME2): reason over the bundle and read the
        // shared logic:ReasoningResult, folding its semantic verdict into the same
        // canonical report. Opt-in (runs the full reasoner) and gts-bundle-scoped.
        if options.deep {
            if let (Some(bytes), Some(imported)) = (&options.gts_bytes, &imported) {
                timed(&mut timings, "deep-semantic", options, None, || {
                    deep_semantic_findings_imported(bytes, imported, &mut report)
                })?;
            } else {
                report.add_finding(
                    Finding::new(
                        Severity::Warning,
                        crate::codes::VALIDATE_DEEP_SKIPPED,
                        "validate --deep requires a GTS bundle (gts_bytes); the semantic pass was skipped",
                    )
                    .with_tool("validate"),
                );
            }
        }

        // Resolve every emitted finding code to its constraint-catalog entry:
        // populate `report.rules` so each code carries a rule whose `helpUri`
        // anchors the "what GMEOW enforces" catalog page. Idempotent — the
        // advisory demonstrator's own rule (with its help URI) is left intact.
        crate::rule_catalog::populate_rules(&mut report);

        Ok(Self {
            dataset,
            shapes,
            timings,
            report,
            declared_terms,
            advisory_claims,
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

    /// Serialize the diagnostic/timing output to JSON.
    ///
    /// The shared [`RdfDataset`] and [`purrdf::shapes::shapes::Shapes`] are not
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

/// The native semantic (`--deep`) pass (ME2): reason over the GTS bundle and
/// read the shared `logic:ReasoningResult`, folding its verdict into `report`.
///
/// Emits an error per contradiction witness when the bundle is inconsistent
/// (`information=both`), a warning per unsatisfiable (provably-empty) class, and a
/// warning per DL construct the native reasoner could not decide
/// (`preservation.unsupported_constructs`). A consistent, fully-covered bundle
/// adds one informational note. The single shared model is the authority — these
/// findings are a consumer projection of it, not a re-derivation.
///
/// # Errors
/// Returns `Err` if the GTS bundle cannot be read or the reasoning run fails.
/// The PUBLIC deep-semantic entry over a GTS bundle — the reasoned-verdict pass
/// `gmeow verify` shares with the dev bundle-only pass.
///
/// It is a thin, single-line delegation to [`deep_semantic_findings`] (the dev
/// bundle pass), so both surfaces run the EXACT same path: reason over the bundle,
/// build the [`gmeow_logic::explain::explanations_for_result`] derivation
/// skeletons, and fold the shared `logic:ReasoningResult` verdict into `report`
/// via [`fold_reasoning_result`]. That means the report gains the same reasoned
/// `validate.deep.*` findings the enrichment pass attaches `derived_from_quads` to,
/// and it INHERITS the same hard-fail discipline: a verdict that cannot
/// be joined to its explain-skeleton derivation (an internal invariant violation)
/// propagates as `Err`, never a graceful advisory — the caller must treat it as a
/// `Severity::Error` failure, not swallow it. There is no reimplementation of the
/// fold here.
///
/// # Errors
/// Returns `Err` if the GTS bundle cannot be read, the native reasoning run fails,
/// the declared contradiction contract cannot be resolved, or a reasoning verdict
/// cannot be joined to its explain-skeleton derivation.
pub fn bundle_deep_findings(gts_bytes: &[u8], report: &mut Report) -> gmeow_errors::Result<()> {
    deep_semantic_findings(gts_bytes, report)
}

fn deep_semantic_findings(gts_bytes: &[u8], report: &mut Report) -> gmeow_errors::Result<()> {
    let imported = import_bundle_for_validation(gts_bytes, true)?;
    deep_semantic_findings_imported(gts_bytes, &imported, report)
}

fn deep_semantic_findings_imported(
    gts_bytes: &[u8],
    imported: &purrdf::GtsImportWithBlobs,
    report: &mut Report,
) -> gmeow_errors::Result<()> {
    let gates = prepared_imported_gates(imported)?;
    let verification = gmeow_logic::verify::PreparedVerification::new(&[], &gates)?;
    deep_semantic_findings_dataset(
        gts_bytes,
        imported.bundle.dataset.as_ref(),
        report,
        &verification,
    )
}

fn deep_semantic_findings_dataset(
    gts_bytes: &[u8],
    dataset: &RdfDataset,
    report: &mut Report,
    verification: &gmeow_logic::verify::PreparedVerification<'_>,
) -> gmeow_errors::Result<()> {
    // Narrow the full bundle down to the object-level reasoning EDB — the SAME
    // boundary `crates/pipeline`'s `assemble_object_level_edb` / `stage-reason` use at
    // build time (shared via `gmeow_logic::reasoning_graphs::project_object_level_edb`),
    // so this CLI deep pass reasons over byte-identical worlds to the pipeline's own
    // `make reason-verify` gate rather than silently drifting by also reasoning over
    // meta/report graphs (documentation, diagnostics, correspondence, …) that assert no
    // object-level axioms.
    let edb = gmeow_logic::reasoning_graphs::project_object_level_edb(dataset).map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!("validate --deep: object-level EDB projection failed: {e}"),
        })
    })?;
    let result = gmeow_logic::reason::reason_all(
        gmeow_logic::reason::prepare_reasoning_input(edb.as_ref())?,
        &gmeow_logic::reasoning_graphs::object_level_domains()?,
    )
    .map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!("validate --deep: native reasoning failed: {e}"),
        })
    })?;
    // The governing contradiction policy is READ from the bundle's declared
    // `logic:ReasoningContract` (`logic:admissibleValuation` facet), not pinned. The
    // resolution rule (see `ContradictionPolicy::resolve_from_dataset`): no contract
    // / no valuation ⇒ conservative classical DEFAULT (a glut IS owl:Nothing, a
    // forbidden violation); multiple conflicting valuations ⇒ the MOST CONSERVATIVE
    // governs. A garbled valuation HARD-FAILS rather than silently relaxing the gate.
    let policy = ContradictionPolicy::resolve_from_dataset(dataset).map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!("validate --deep: contract resolution failed: {e}"),
        })
    })?;
    // Build the faithful cited-quad-reifier derivation skeletons for the SAME
    // result; a build failure AFTER a real verdict is an internal invariant
    // violation and HARD-FAILS the dev bundle pass (propagated as Err), never
    // downgraded to an advisory note.
    let explanations = gmeow_logic::explain::explanations_for_result(&result).map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!(
                "validate --deep: explanation-skeleton build failed after a real verdict \
                 (internal invariant): {e}"
            ),
        })
    })?;
    fold_reasoning_result(&result, policy, &explanations, report).map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!("validate --deep: {}", e.message),
        })
    })?;

    // Shared with `crate::data_validate::deep_consistency_findings` via
    // `run_math_reasoned_gates` (see its doc comment for why this is safe to run
    // unconditionally, and why the two callers deliberately map its failure
    // differently). Runs over the SAME `edb` + `result` the consistency fold above
    // just used, so both halves agree on what "object-level" means. A failure here
    // means this dev bundle-only pass's own EDB projection produced a graph the
    // shared gate could not materialize — an internal invariant violation, so it
    // hard-fails rather than degrading to an advisory note.
    run_math_reasoned_gates(edb.as_ref(), &result, verification, report).map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!("validate --deep: reasoned-graph materialization failed: {e}"),
        })
    })?;

    // Build the scoped coherence certificate from the SAME reasoning result, under
    // the SAME resolved policy and bundle hash, and attach it to the report metadata
    // (C2). The validate and release lanes share ONE certificate constructor.
    let bundle_hash = purrdf::gts::writer::digest_string(gts_bytes);
    let axiom_hashes = gmeow_logic::certificate::per_graph_axiom_hashes(
        dataset,
        purrdf::gts::writer::digest_string,
    );
    // Compute genuine projection-loss codes from the static loss ledger — the same
    // computation the release lane uses, ensuring validate and release agree.
    let projection_loss_codes: BTreeSet<String> = PROJECTION_CODECS
        .iter()
        .flat_map(|&to| {
            pair_loss_ledger("gts", to)
                .entries()
                .iter()
                .map(|e| e.code.to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    let outcome = gmeow_logic::certificate::CoherenceOutcome::from_reasoning_result(
        &result,
        bundle_hash,
        axiom_hashes,
        policy,
        // Injected, never sampled — the deep pass certificate stays deterministic.
        DETERMINISTIC_ISSUED_AT,
        projection_loss_codes,
    )
    .map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!("validate --deep: coherence certificate build failed: {e}"),
        })
    })?;
    attach_coherence_certificate(report, &outcome);
    Ok(())
}

/// The named graph the deep-pass coherence certificate is projected into.
const COHERENCE_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/attestations";

/// The injected issue timestamp for the deep-pass certificate. The validate lane is
/// not a release; a fixed timestamp keeps the certificate fold byte-deterministic so
/// it never perturbs a cached report.
const DETERMINISTIC_ISSUED_AT: &str = "1970-01-01T00:00:00Z";

/// Attach a built [`CoherenceOutcome`] to `report.metadata` under the
/// `"coherence_certificate"` key as its projected N-Quads (the same serialization
/// the release lane folds into the signed bundle), so the validate and release lanes
/// present ONE certificate form. A refused outcome serializes to empty — the
/// violation rides as the error finding instead, so nothing is attached.
fn attach_coherence_certificate(
    report: &mut Report,
    outcome: &gmeow_logic::certificate::CoherenceOutcome,
) {
    let nquads = outcome.to_nquads(COHERENCE_GRAPH);
    if nquads.is_empty() {
        return;
    }
    report.metadata.insert(
        "coherence_certificate".to_owned(),
        serde_json::Value::String(nquads),
    );
}

/// An internal-invariant violation raised while folding a reasoning verdict: a
/// contradiction / permitted-conflict / unsatisfiability witness named a
/// `(subject, world)` quad that is NOT present among the reasoning result's derived
/// (or asserted) quads — no explain skeleton could be located for it.
///
/// This is NOT graceful degradation: the verdict referenced a quad absent from the
/// result it was read off, which can only be an engine/fold contract violation. The
/// callers HARD-FAIL on it (a `Severity::Error` finding on the CLI path, a propagated
/// `Err` on the dev-bundle path), never a `Severity::Note`.
#[derive(Debug, Clone)]
pub(crate) struct WitnessDerivationMissing {
    /// The invariant-violation detail, naming the unlocatable witness.
    pub message: String,
}

/// Locate the explain-skeleton cited-IRI derivation for one clash witness.
///
/// Returns the sorted, deduped UNION of the `cited_iris` of every explanation whose
/// target quad concerns `(witness_name, witness_world)` — i.e. whose target step's
/// `subject_iri` is the witness, whose `world_iri` is the witness world, and whose
/// target assertion has the requested empty-class role. The shared native
/// classifier recognizes canonical operators and their declared grounding views;
/// data predicates mentioning the empty class cannot supply a derivation. Returns
/// `None` when no such explanation exists, which the fold hard-fails on.
fn derived_quads_for_witness(
    explanations: &[gmeow_logic::explain::Explanation],
    witness_name: &str,
    witness_world: &str,
    role: gmeow_logic::reason::dl::EmptyClassAssertion,
) -> Option<Vec<String>> {
    let mut cited: BTreeSet<String> = BTreeSet::new();
    let mut matched = false;
    for expl in explanations {
        if expl.world_iri != witness_world {
            continue;
        }
        let Some(target) = expl.step_skeleton.first() else {
            continue;
        };
        if target.subject_iri != witness_name || target.graph_iri != witness_world {
            continue;
        }
        // Explanation objects are an output spelling, never reparsed into a
        // reasoning input. Retain the exact predicate and class-marker roles.
        let object_iri = target
            .obj_n3
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'));
        if object_iri.and_then(|iri| {
            gmeow_logic::reason::dl::EmptyClassAssertion::classify(&target.predicate_iri, iri)
        }) != Some(role)
        {
            continue;
        }
        matched = true;
        cited.extend(expl.cited_iris.iter().cloned());
    }
    matched.then(|| cited.into_iter().collect())
}

#[cfg(test)]
#[path = "validate_all_verdict_tests.rs"]
mod verdict_derivation_tests;

/// Fold a shared `logic:ReasoningResult` verdict into `report` as the deep-pass
/// finding projection. The SINGLE fold both the dev bundle-only pass
/// ([`deep_semantic_findings`]) and the consumer user-data-merge pass
/// ([`crate::data_validate::run`]) share, so the two surfaces can never drift.
///
/// Emits an error per contradiction witness when the run is inconsistent
/// (`information=both`), a warning per unsatisfiable (provably-empty) class, and a
/// warning per DL construct the native reasoner could not decide
/// (`preservation.unsupported_constructs`). A consistent, fully-covered run adds one
/// informational note. These findings are a projection of the single shared model,
/// not a re-derivation.
///
/// Each INCONSISTENT / PERMITTED_CONFLICT / UNSATISFIABLE verdict finding also gains
/// the explain-skeleton cited-quad-reifier derivation of its clash quad, attached via
/// [`gmeow_errors::Finding::with_derived_from_quads`] (a SEPARATE edge from
/// `antecedents`/`root_cause`, which stay untouched). `explanations` is the owned
/// [`gmeow_logic::explain::explanations_for_result`] skeleton for the SAME `result`.
///
/// # Errors
///
/// Returns [`WitnessDerivationMissing`] when a witness names a `(subject, world)`
/// clash quad that is absent from `explanations` — an internal invariant violation
/// the callers HARD-FAIL on (never a graceful advisory).
pub(crate) fn fold_reasoning_result(
    result: &gmeow_logic::result::ReasoningResult,
    policy: ContradictionPolicy,
    explanations: &[gmeow_logic::explain::Explanation],
    report: &mut Report,
) -> Result<(), WitnessDerivationMissing> {
    if !result.is_consistent() {
        // A within-world glut is a permitted, DISCLOSED conflict when the governing
        // contract admits gluts, and a FORBIDDEN integrity violation otherwise. A
        // permitted conflict is coherent — it is emitted at NON-error severity
        // (logic:FindingPermittedEpistemicConflict) so the gate stays green; a
        // forbidden one is the failing logic:FindingContradictionWitness.
        let permitted = policy.glut_permitted();
        for witness in &result.provenance.contradiction_witnesses {
            // Locate the explain-skeleton derivation of this witness's clash quad
            // BEFORE minting the finding; an unlocatable witness is a hard-fail
            // invariant violation, never a silently-underived verdict.
            let derived_from_quads = derived_quads_for_witness(
                explanations,
                &witness.individual,
                &witness.world,
                gmeow_logic::reason::dl::EmptyClassAssertion::Membership,
            )
            .ok_or_else(|| WitnessDerivationMissing {
                message: format!(
                    "contradiction witness (individual {}, world {}) names a quad absent \
                     from the reasoning result's derivations — no explain skeleton could \
                     be located",
                    witness.individual, witness.world
                ),
            })?;
            let finding = if permitted {
                Finding::new(
                    Severity::Warning,
                    crate::codes::VALIDATE_DEEP_PERMITTED_CONFLICT,
                    format!(
                        "individual {} carries a within-world contradiction in world {}, \
                         permitted and disclosed under contradiction policy {} \
                         (logic:ReasoningResult information=both)",
                        witness.individual,
                        witness.world,
                        policy.local_name()
                    ),
                )
                .with_category(FindingCategory::PermittedEpistemicConflict)
            } else {
                Finding::new(
                    Severity::Error,
                    crate::codes::VALIDATE_DEEP_INCONSISTENT,
                    format!(
                        "individual {} forced into owl:Nothing in world {} \
                         (logic:ReasoningResult information=both)",
                        witness.individual, witness.world
                    ),
                )
                .with_category(FindingCategory::ContradictionWitness)
            };
            report.add_finding(
                finding
                    .with_tool("validate")
                    .with_derived_from_quads(derived_from_quads),
            );
        }
    }

    for unsat in gmeow_logic::reason::dl::unsatisfiable_from_inferred(result.inferred()) {
        // The unsatisfiable class is the subject of a `subClassOf(class, owl:Nothing)`
        // clash quad; attach its explain-skeleton derivation, hard-failing if the
        // verdict named a quad the result does not carry.
        let derived_from_quads = derived_quads_for_witness(
            explanations,
            &unsat.class,
            &unsat.world,
            gmeow_logic::reason::dl::EmptyClassAssertion::Subsumption,
        )
        .ok_or_else(|| WitnessDerivationMissing {
            message: format!(
                "unsatisfiable class {} (world {}) names a quad absent from the \
                         reasoning result's derivations — no explain skeleton could be located",
                unsat.class, unsat.world
            ),
        })?;
        report.add_finding(
            Finding::new(
                Severity::Warning,
                crate::codes::VALIDATE_DEEP_UNSATISFIABLE,
                format!(
                    "class {} is unsatisfiable (provably empty) in world {}",
                    unsat.class, unsat.world
                ),
            )
            .with_tool("validate")
            .with_category(FindingCategory::ModelingDisciplineViolation)
            .with_derived_from_quads(derived_from_quads),
        );
    }

    for construct in &result.preservation.unsupported_constructs {
        report.add_finding(
            Finding::new(
                Severity::Warning,
                crate::codes::VALIDATE_DEEP_UNSUPPORTED_CONSTRUCT,
                format!(
                    "DL construct {construct} is present but was not decided by the native \
                     reasoner; the semantic verdict is incomplete for it"
                ),
            )
            .with_tool("validate")
            .with_category(FindingCategory::UnsupportedSemanticFeature),
        );
    }

    // Emit one ProjectionLoss finding per genuine ledger entry: intentional losses
    // incurred projecting this GTS bundle to each canonical projection codec. These
    // are serialization/semantic-subset losses from the static loss ledger, entirely
    // distinct from the DL-reasoner's unsupported_constructs above.
    for &to in PROJECTION_CODECS {
        for entry in pair_loss_ledger("gts", to).entries() {
            report.add_finding(
                Finding::new(
                    Severity::Note,
                    crate::codes::VALIDATE_DEEP_PROJECTION_LOSS,
                    format!(
                        "projection gts → {to}: loss code '{}' — {}",
                        entry.code, entry.note
                    ),
                )
                .with_tool("validate")
                .with_category(FindingCategory::ProjectionLoss),
            );
        }
    }

    // Emit an IncompleteCheck finding when the reasoning run did not reach a
    // conclusive verdict: budget exhaustion on the computation axis, or an
    // incomplete result on the completeness axis. These are orthogonal signals —
    // `BudgetExhausted` means the engine stopped early; `Incomplete` means the
    // answer covers only part of the fragment. Either alone warrants disclosure.
    let budget_exhausted =
        result.evaluation == gmeow_logic::result::EvaluationStatus::BudgetExhausted;
    let completeness_incomplete =
        result.completeness == gmeow_logic::result::CompletenessStatus::Incomplete;
    if budget_exhausted || completeness_incomplete {
        report.add_finding(
            Finding::new(
                Severity::Warning,
                crate::codes::VALIDATE_DEEP_INCOMPLETE,
                format!(
                    "native deep semantic pass did not reach a conclusive verdict \
                     (evaluation={}, completeness={}); results may be partial",
                    result.evaluation.wire(),
                    result.completeness.wire(),
                ),
            )
            .with_tool("validate")
            .with_category(FindingCategory::IncompleteCheck),
        );
    }

    if result.is_consistent() && result.preservation.unsupported_constructs.is_empty() {
        report.add_finding(
            Finding::new(
                Severity::Note,
                crate::codes::VALIDATE_DEEP_CONSISTENT,
                format!(
                    "native deep semantic pass: consistent (information={}, evaluation={}, \
                     completeness={})",
                    result.information.wire(),
                    result.evaluation.wire(),
                    result.completeness.wire()
                ),
            )
            .with_tool("validate"),
        );
    }

    Ok(())
}

/// Decode required native laws from the same imported dataset/envelope selection.
pub(crate) fn prepared_imported_gates(
    imported: &purrdf::GtsImportWithBlobs,
) -> gmeow_errors::Result<gmeow_logic::verify::PreparedReasonedGates> {
    use gmeow_gts_profile::archive;
    let blob = archive::required_imported_blob(imported, archive::REASONING_REP)?;
    let bytes = archive::archive_member(
        &blob.bytes,
        archive::REASONED_GATES_MEMBER,
        archive::MAX_NATIVE_MEMBER_BYTES,
    )?;
    let gates: gmeow_logic::verify::PreparedReasonedGates =
        serde_json::from_slice(bytes).map_err(|error| {
            Diag::of_kind(crate::error::Engine {
                detail: format!("required native verification laws cannot be decoded: {error}"),
            })
        })?;
    gates.validate_source_identity()?;
    Ok(gates)
}

fn import_bundle_for_validation(
    bytes: &[u8],
    deep: bool,
) -> gmeow_errors::Result<purrdf::GtsImportWithBlobs> {
    let selectors = if deep {
        vec![purrdf::GtsBlobSelector::Representation(
            gmeow_gts_profile::archive::REASONING_REP,
        )]
    } else {
        Vec::new()
    };
    let limit = gmeow_gts_profile::archive::MAX_SELECTED_ARCHIVE_BYTES;
    purrdf::import_gts_events_with_blobs(
        bytes,
        &selectors,
        purrdf::GtsBlobLimits::new(limit, limit),
    )
    .map_err(|error| {
        Diag::of_kind(crate::error::Dataset {
            detail: format!("validate: native GTS import failed: {error}"),
        })
    })
}

/// Preserve the existing cache's sorted raw-head-byte key, rather than hashing
/// the native envelope's hexadecimal presentation as if it were the head itself.
/// The upstream fixed-width hex decoder supplies raw bytes here; no hash is recomputed.
fn native_segment_heads_cache_key(envelope: &purrdf::RdfEnvelope) -> gmeow_errors::Result<String> {
    let mut heads = envelope
        .lookaside
        .segments
        .iter()
        .filter_map(|segment| segment.head.as_deref())
        .map(|head| {
            purrdf::ContentDigest::from_hex(head)
                .map(|head| *head.as_bytes())
                .ok_or_else(|| {
                    Diag::of_kind(crate::error::Dataset {
                        detail: "native envelope contains an invalid segment head".to_owned(),
                    })
                })
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    heads.sort();
    Ok(ValidationCache::cache_key(
        &heads.iter().map(|head| head.as_slice()).collect::<Vec<_>>(),
    ))
}

/// Run the `math:` dimensional-homogeneity + `math:` expression-identity reasoned
/// gates: the SAME two checks `stage-verify` / `gmeow-dev reason-verify` run at
/// build time over the pipeline's own `assemble_object_level_edb`, now reachable
/// from the `gmeow` CLI's deep passes — both the dev bundle-only pass
/// ([`deep_semantic_findings`]) and the consumer user-data-merge pass
/// ([`crate::data_validate::deep_consistency_findings`]) call this ONE helper so
/// the two surfaces can never drift. A consumer with their own math AST graph gets
/// `math:StructuralKeyDrift` / `math:FalseStructuralNormalizationClaim` findings
/// directly from the `gmeow` CLI, not only from the MCP `verify_graph` tool.
///
/// Deliberately narrower than the FULL
/// [`gmeow_logic::verify::verify_with_reasoning_result`] battery: that also runs
/// the embedded `queries/verify/*.rq` bad-example queries, several of which check
/// for FIXED gmeow vocabulary (e.g. `axis-not-disjoint`'s seven identity-axis
/// classes) that only the real production bundle carries — misfiring on a
/// caller-supplied `edb` that is a non-production bundle, or a production bundle
/// unioned with a consumer's own PARTIAL data graph. The math: gates carry no such
/// fixed-vocabulary assumption: they read whatever `math:MathematicalExpression` /
/// `math:GramMatrix` individuals the reasoned graph actually has, so they are safe
/// to run unconditionally here.
///
/// `edb` and `result` MUST be the same pair the caller's own `fold_reasoning_result`
/// fold just ran over, so both halves of the deep pass agree on what
/// "object-level" means.
///
/// # Errors
///
/// Returns `Err` if reasoned-graph materialization
/// ([`gmeow_logic::verify::PreparedVerification::materialize_reasoned_graph`]) fails. The two callers
/// intentionally map this failure differently (a caller-supplied-data pass
/// degrades it to an advisory note; the dev bundle-only pass hard-fails on it,
/// since a failure there can only mean the bundle itself is broken) — that
/// divergence is deliberately left to each call site's own `.map_err`, not hidden
/// in here.
pub(crate) fn run_math_reasoned_gates(
    edb: &RdfDataset,
    result: &gmeow_logic::result::ReasoningResult,
    verification: &gmeow_logic::verify::PreparedVerification<'_>,
    report: &mut Report,
) -> gmeow_errors::Result<()> {
    match verification.materialize_reasoned_graph(edb, result)? {
        gmeow_logic::verify::ReasonedGraphOutcome::Ready(reasoned) => {
            for finding in gmeow_logic::math_dimension::check_math_dimension_findings(
                reasoned.dataset.as_ref(),
            ) {
                report.add_finding(finding);
            }
            for finding in gmeow_logic::math_expression::check_math_expression_findings(
                edb,
                reasoned.dataset.as_ref(),
            ) {
                report.add_finding(finding);
            }
        }
        gmeow_logic::verify::ReasonedGraphOutcome::IncompleteClosure(findings) => {
            for finding in findings {
                report.add_finding(finding);
            }
        }
    }
    Ok(())
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
/// focus nodes and wire coordinates exactly as a fresh compute would.
fn run_cached<F>(
    cache: Option<&ValidationCache>,
    kind: &str,
    key: &str,
    compute: F,
) -> gmeow_errors::Result<(Vec<Finding>, Option<String>)>
where
    F: FnOnce() -> gmeow_errors::Result<Vec<Finding>>,
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

/// The toolchain context folded into the merged-SHACL Merkle key. The
/// `compiler_version` carries the same crate-version triple as
/// [`ValidationCache::toolchain_salt`] (so a toolchain bump invalidates the key
/// through this *and* the salt), and the reasoning-profile slot is pinned to the
/// merged-SHACL phase ("shacl") — the merged whole-ontology validation has no
/// per-profile reasoning mode.
fn merged_shacl_toolchain() -> ToolchainContext {
    let compiler_version = format!(
        "gmeow-validate={};gmeow-shacl={};gmeow-gts-wire={}",
        env!("CARGO_PKG_VERSION"),
        purrdf::shapes::VERSION,
        purrdf::gts::wire::VERSION,
    );
    ToolchainContext::new(compiler_version, "shacl")
}

/// Compute the S6a semantic Merkle PRODUCT key for the whole-ontology
/// merged-SHACL phase over the slices catalog discovered at `slices_dir`.
///
/// Seeds are ALL slice IRIs in the catalog (the merged-SHACL validates the whole
/// composition); the product key folds each slice's *semantic* (canonical
/// N-Triples) module/shapes/manifest digests, so it is path-independent and
/// comment-insensitive. Hard-fails (no silent degraded path) if the catalog or
/// edges cannot be built.
pub fn merged_shacl_source_key(slices_dir: &str) -> gmeow_errors::Result<String> {
    merged_shacl_merkle_root(slices_dir)
}

fn merged_shacl_merkle_root(slices_dir: &str) -> gmeow_errors::Result<String> {
    let (catalog, ownership) = slice_catalog_and_ownership(slices_dir)?;
    merged_shacl_merkle_root_from_parts(&catalog, &ownership.edges)
}

fn slice_catalog_and_ownership(
    slices_dir: &str,
) -> gmeow_errors::Result<(SliceCatalog, OwnershipReport)> {
    let catalog = SliceCatalog::discover(Path::new(slices_dir), gmeow_ns::gmeow_slice_vocab())
        .map_err(|e| {
            Diag::of_kind(crate::error::Catalog {
                detail: format!("merged-SHACL Merkle key: slice catalog discovery failed: {e}"),
            })
        })?;
    // S4 dependency edges (the same edges the ownership/dependency analyzer
    // produces) drive the Merkle dependency composition.
    let ownership = OwnershipAnalyzer::new(&catalog).analyze().map_err(|e| {
        Diag::of_kind(crate::error::Catalog {
            detail: format!("merged-SHACL Merkle key: ownership analysis failed: {e}"),
        })
    })?;
    Ok((catalog, ownership))
}

fn merged_shacl_merkle_root_from_parts(
    catalog: &SliceCatalog,
    edges: &[DependencyEdge],
) -> gmeow_errors::Result<String> {
    let toolchain = merged_shacl_toolchain();
    // Seeds = every slice IRI; the product closes over deps but the union of all
    // slices already covers the whole composition.
    let seeds: Vec<String> = catalog
        .records()
        .iter()
        .map(|r| r.manifest.slice_iri.clone())
        .collect();
    let product = purrdf::slice::product_unit(catalog, edges, &seeds);
    let key =
        product_unit_key(Phase::Shacl, catalog, edges, &product, &toolchain).map_err(|e| {
            Diag::of_kind(crate::error::Catalog {
                detail: format!("merged-SHACL Merkle key: product key computation failed: {e}"),
            })
        })?;
    Ok(key.root)
}

/// The per-file parse result: each source file parsed once into a frozen native
/// dataset (or its parse error), in `source_paths` order.
type ParsedSource = (PathBuf, gmeow_errors::Result<Arc<RdfDataset>>);

/// Merge every successfully-parsed per-file dataset into ONE frozen shared dataset,
/// each under a fresh blank scope (C0.2), matching [`store::dataset_from_paths`]. A
/// parse failure propagates with the same `"syntax error in {path}: {msg}"` format the
/// per-file parse produced, preserving the `build-store` error contract.
fn merge_parsed_sources(parsed: &[ParsedSource]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for (path, result) in parsed {
        let ds = result.as_ref().map_err(|e| {
            Diag::of_kind(crate::error::Parse {
                detail: format!("syntax error in {}: {}", path.display(), e.message()),
            })
        })?;
        builder.push_dataset(ds);
    }
    builder.freeze().map_err(|e| {
        Diag::of_kind(crate::error::Serialize {
            detail: format!("dataset freeze failed: {e}"),
        })
    })
}

/// Phase 1: report syntax errors from the already-parsed per-file results.
///
/// The datasets were produced before the `build-store` phase. Any `Err` entry is a
/// file that failed to parse; `build-store` (`merge_parsed_sources`) will have already
/// returned `Err` for that case (propagated via `?`), so in practice this function
/// only runs when all files parsed successfully and always returns an empty error
/// list. It is kept as a separate timed phase so the phase label and timing structure
/// remain identical to the original.
fn check_syntax_from_parsed(parsed: &[ParsedSource]) -> gmeow_errors::Result<PhaseResult> {
    let mut result = PhaseResult::default();
    for (path, parse_result) in parsed {
        if let Err(exc) = parse_result {
            result.errors.push(format!(
                "syntax error in {}: {}",
                path.display(),
                exc.message()
            ));
        }
    }
    Ok(result)
}

/// Phase 2: scan each already-parsed dataset for banned `owl:sameAs` links.
///
/// Files that failed to parse are skipped — they already produced an error in Phase 1
/// (and caused `build-store` to fail before reaching this phase in practice).
fn check_sameas_ban_from_parsed(
    parsed: &[ParsedSource],
    namespace: &str,
    allowlist: &[(String, String)],
) -> gmeow_errors::Result<PhaseResult> {
    let mut result = PhaseResult::default();
    for (path, parse_result) in parsed {
        let ds = match parse_result {
            Ok(ds) => ds,
            Err(exc) => {
                result.errors.push(format!(
                    "failed to parse {}: {}",
                    path.display(),
                    exc.message()
                ));
                continue;
            }
        };
        for (subject_text, obj) in store::sameas_violations(ds, namespace, allowlist) {
            result.errors.push(format!(
                "{}: banned owl:sameAs to external entity \
                 {subject_text} owl:sameAs {obj} (Principle 5); \
                 use skos:exactMatch or gmeow:authorityLink",
                path.display()
            ));
        }
    }
    Ok(result)
}

/// Phase 9: every slice must ship at least one `examples/*.ttl` file.
fn check_example_coverage(slices_dir: &str) -> gmeow_errors::Result<PhaseResult> {
    let mut result = PhaseResult::default();
    for manifest in find_slice_manifests(slices_dir)? {
        let slice_dir = manifest.parent().ok_or_else(|| {
            Diag::of_kind(crate::error::Io {
                detail: format!("manifest has no parent: {}", manifest.display()),
            })
        })?;
        let slice_name = slice_dir
            .file_name()
            .ok_or_else(|| {
                Diag::of_kind(crate::error::Io {
                    detail: format!("slice dir has no name: {}", slice_dir.display()),
                })
            })?
            .to_string_lossy();
        let examples_dir = slice_dir.join("examples");
        let has_example = examples_dir.is_dir()
            && std::fs::read_dir(&examples_dir)
                .map_err(|e| {
                    Diag::of_kind(crate::error::Io {
                        detail: format!("read_dir {}: {e}", examples_dir.display()),
                    })
                })?
                .filter_map(|e| e.ok())
                .any(|e| {
                    let p = e.path();
                    p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("ttl")
                });
        if !has_example {
            result.errors.push(format!(
                "slice {slice_name}: no examples/*.ttl — every slice must \
                 ship at least one validating example"
            ));
        }
    }
    Ok(result)
}

/// One cached SHACL phase outcome: the structured findings plus a cache-status tag
/// (`"cache-hit"` / `"cache-miss"` / `"cache-disabled"`), or a hard error. Matches
/// the return shape of [`run_cached`].
type CachedPhaseResult = gmeow_errors::Result<(Vec<Finding>, Option<String>)>;

/// Phase 10: validate every slice example against the ontology, in parallel, over a
/// fresh `base ∪ example` native dataset per example.
fn check_examples(
    dataset: &RdfDataset,
    shapes: &purrdf::shapes::shapes::Shapes,
    failure_classes: &FailureClassIndex,
    slices_dir: &str,
    cache: Option<&ValidationCache>,
    base_key: &str,
) -> gmeow_errors::Result<(Vec<Finding>, Option<String>)> {
    // `find_example_files` returns a name-sorted list (see its `sort_by`). Each
    // example is an independent whole-ontology SHACL pass — the dominant cost of
    // `validate` — so validate them in parallel.
    //
    // The SHACL shapes include SHACL-SPARQL targets, which need a queryable
    // `base ∪ example` graph. Project the base ontology into the flattened SHACL
    // view ONCE, then each example only projects its own small graph before merging
    // the two projected datasets under fresh blank scopes.
    let examples = find_example_files(slices_dir)?;
    // Older entries were computed with an unsound touched-term filter. They
    // cannot certify complete example validation even when input bytes match.
    let base_key = ValidationCache::cache_key(&[
        base_key.as_bytes(),
        b"gmeow-example-shacl-canonical-complete-targets-v3",
    ]);

    // Fast path: if every example's SHACL result is already cached, skip the
    // parallel re-validation entirely (main's example-shacl cache).
    if let Some(cache) = cache {
        let mut cached_findings: Vec<Finding> = Vec::new();
        let mut all_hit = true;
        for (_, path) in &examples {
            let example_key = example_shacl_key(cache, &base_key, path)?;
            let Some(cached) = cache.read_cached_result("example-shacl", &example_key) else {
                all_hit = false;
                break;
            };
            cached_findings.extend(cached.findings);
        }
        if all_hit {
            return Ok((
                cached_findings,
                Some(format!("cache-hit:{};cache-miss:0", examples.len())),
            ));
        }
    }

    let base_projected = gmeow_logic_compile::projections::reader_view::shacl_reader_view(dataset);
    let prepared_shapes = purrdf::shapes::engine::PreparedShapes::new(Arc::new(shapes.clone()));

    let results: Vec<CachedPhaseResult> = examples
        .par_iter()
        .map(|(name, path)| -> CachedPhaseResult {
            let example_key = if let Some(cache) = cache {
                let file_key = cache.files_cache_key(std::slice::from_ref(path))?;
                ValidationCache::cache_key(&[base_key.as_bytes(), file_key.as_bytes()])
            } else {
                ValidationCache::cache_key(&[
                    base_key.as_bytes(),
                    path.to_string_lossy().as_bytes(),
                ])
            };
            run_cached(cache, "example-shacl", &example_key, || {
                run_example_shacl(
                    &base_projected,
                    &prepared_shapes,
                    failure_classes,
                    path,
                    name,
                )
            })
        })
        .collect();

    // Sequential, in-order fold: accumulate findings and hit/miss counts, and
    // propagate the FIRST error by index (deterministic regardless of which thread
    // finished first).
    let mut findings: Vec<Finding> = Vec::new();
    let mut hits: usize = 0;
    let mut misses: usize = 0;
    for result in results {
        let (example_findings, meta) = result?;
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

/// Validate one example file against the ontology + shapes over a fresh
/// projected `base ∪ example` native dataset.
///
/// The per-example SHACL cache key: the base graph key combined with the example
/// file's content key, so an example re-validates only when the base graph OR the
/// example file changes.
fn example_shacl_key(
    cache: &ValidationCache,
    base_key: &str,
    path: &Path,
) -> gmeow_errors::Result<String> {
    let file_key = cache.files_cache_key(std::slice::from_ref(&path.to_path_buf()))?;
    Ok(ValidationCache::cache_key(&[
        base_key.as_bytes(),
        file_key.as_bytes(),
    ]))
}

/// The example file is parsed under its own blank scope, projected into the SHACL
/// flattened view, merged with the already-projected base graph, then validated
/// with the native SHACL engine.
fn run_example_shacl(
    base_projected: &Arc<RdfDataset>,
    shapes: &purrdf::shapes::engine::PreparedShapes,
    failure_classes: &FailureClassIndex,
    path: &Path,
    name: &str,
) -> gmeow_errors::Result<Vec<Finding>> {
    let example_ds = match store::parse_file_dataset(path) {
        Ok(ds) => ds,
        Err(e) => {
            return Ok(vec![
                Finding::new(
                    Severity::Error,
                    crate::codes::EXAMPLE_PARSE,
                    format!(
                        "example {name}: failed to parse {}: {}",
                        path.display(),
                        e.message()
                    ),
                )
                .with_tool("validate"),
            ]);
        }
    };
    let example_projected =
        gmeow_logic_compile::projections::reader_view::shacl_reader_view(&example_ds);
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(base_projected);
    builder.push_dataset(&example_projected);
    // The base graph carries the class hierarchy (`rdfs:subClassOf` edges) and the example
    // asserts only the most-specific type; class membership is resolved by the engine
    // (`sh:class`/`sh:targetClass`) and by the `a/<subClassOf>*` property path the projected
    // `sh:sparql` / `sh:SPARQLTarget` bodies carry, so no `rdf:type` pre-materialization is
    // needed over the merged projected dataset.
    let merged = builder.freeze().map_err(|e| {
        Diag::of_kind(crate::error::Serialize {
            detail: format!("example {name}: projected base ∪ example freeze failed: {e}"),
        })
    })?;
    // Target dependencies may reach arbitrary nodes through property paths,
    // SPARQL, class membership or custom expressions. Mentioning only ABox
    // terms does not prove that untouched focus nodes are unaffected. Until an
    // effect analysis certifies the complete affected set, evaluate all targets.
    let mut report = shapes
        .bind_projected_dataset(merged)
        .and_then(|validator| validator.validate())
        .map_err(|e| {
            Diag::of_kind(crate::error::Engine {
                detail: format!("example {name}: SHACL validation failed: {e}"),
            })
        })?;
    // The per-example path calls the engine directly, so it applies the same
    // result-set collapse
    // `store::shacl_validate_dataset` does — a violation reported twice is one
    // violation on every validate surface, not just the bundle-driven one.
    store::dedupe_validation_results(&mut report);
    Ok(shacl_findings_from_report(
        &report,
        Some(name),
        failure_classes,
    ))
}

/// Phase 11/12/13: validate a merged set of DSL Turtle sources against dedicated
/// SHACL shapes.
fn check_dsl(
    paths: &[PathBuf],
    shapes_ttl: &str,
    label: &str,
    cache: Option<&ValidationCache>,
) -> gmeow_errors::Result<(Vec<Finding>, Option<String>)> {
    if paths.is_empty() {
        return Ok((Vec::new(), Some("no-inputs".to_owned())));
    }

    let key = if let Some(cache) = cache {
        let file_key = cache.files_cache_key(paths)?;
        let shapes_key = ValidationCache::cache_key(&[shapes_ttl.as_bytes()]);
        let salt = ValidationCache::toolchain_salt();
        ValidationCache::cache_key(&[
            file_key.as_bytes(),
            shapes_key.as_bytes(),
            label.as_bytes(),
            salt.as_bytes(),
        ])
    } else {
        ValidationCache::cache_key(&[label.as_bytes()])
    };

    run_cached(cache, &format!("dsl-shacl/{label}"), &key, || {
        crate::dsl_shacl::validate_dsl(paths, shapes_ttl, label)
    })
}

/// Recursively collect all `.ttl` files under `dir`, sorted deterministically.
///
/// Public so the DSL-coverage resolver and its liveness/correspondence tests
/// enumerate the exact file set the DSL SHACL phases validate — one authority
/// for "which `.ttl` files a `dsl/` surface contributes", never a second walk
/// that could drift.
pub fn collect_ttl_paths(dir: &str) -> gmeow_errors::Result<Vec<PathBuf>> {
    let root = PathBuf::from(dir);
    let mut paths: Vec<PathBuf> = Vec::new();
    collect_ttl_paths_recursive(&root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_ttl_paths_recursive(dir: &Path, paths: &mut Vec<PathBuf>) -> gmeow_errors::Result<()> {
    for entry in std::fs::read_dir(dir).map_err(|e| {
        Diag::of_kind(crate::error::Io {
            detail: format!("read_dir {}: {e}", dir.display()),
        })
    })? {
        let entry = entry.map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("dir entry in {}: {e}", dir.display()),
            })
        })?;
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            collect_ttl_paths_recursive(&path, paths)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("ttl") {
            paths.push(path);
        }
    }
    Ok(())
}

/// Collect every slice-resident test-DSL fixture (`slices/*/*/tests/*.ttl`),
/// non-recursive within each slice's `tests/` directory.
fn collect_slice_test_files(slices_dir: &str) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for manifest in find_slice_manifests(slices_dir)? {
        let slice_dir = manifest.parent().ok_or_else(|| {
            Diag::of_kind(crate::error::Io {
                detail: format!("manifest has no parent: {}", manifest.display()),
            })
        })?;
        let tests_dir = slice_dir.join("tests");
        if !tests_dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&tests_dir).map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("read_dir {}: {e}", tests_dir.display()),
            })
        })? {
            let entry = entry.map_err(|e| {
                Diag::of_kind(crate::error::Io {
                    detail: format!("dir entry in {}: {e}", tests_dir.display()),
                })
            })?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("ttl") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

/// Find every `slices/*/*/manifest.ttl` file under `slices_dir`, sorted.
fn find_slice_manifests(slices_dir: &str) -> gmeow_errors::Result<Vec<PathBuf>> {
    let root = PathBuf::from(slices_dir);
    let mut manifests: Vec<PathBuf> = Vec::new();
    for group in std::fs::read_dir(&root).map_err(|e| {
        Diag::of_kind(crate::error::Io {
            detail: format!("read_dir {}: {e}", root.display()),
        })
    })? {
        let group = group
            .map_err(|e| {
                Diag::of_kind(crate::error::Io {
                    detail: format!("dir entry in {}: {e}", root.display()),
                })
            })?
            .path();
        if !group.is_dir() {
            continue;
        }
        for slice in std::fs::read_dir(&group).map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("read_dir {}: {e}", group.display()),
            })
        })? {
            let slice = slice
                .map_err(|e| {
                    Diag::of_kind(crate::error::Io {
                        detail: format!("dir entry in {}: {e}", group.display()),
                    })
                })?
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
fn find_example_files(slices_dir: &str) -> gmeow_errors::Result<Vec<(String, PathBuf)>> {
    let root = PathBuf::from(slices_dir);
    let mut examples: Vec<(String, PathBuf)> = Vec::new();
    for manifest in find_slice_manifests(slices_dir)? {
        let slice_dir = manifest.parent().expect("manifest has parent");
        let examples_dir = slice_dir.join("examples");
        if !examples_dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&examples_dir).map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("read_dir {}: {e}", examples_dir.display()),
            })
        })? {
            let entry = entry
                .map_err(|e| {
                    Diag::of_kind(crate::error::Io {
                        detail: format!("dir entry in {}: {e}", examples_dir.display()),
                    })
                })?
                .path();
            if !entry.is_file() || entry.extension().and_then(|s| s.to_str()) != Some("ttl") {
                continue;
            }
            let name = entry
                .strip_prefix(&root)
                .map_err(|e| {
                    Diag::of_kind(crate::error::Io {
                        detail: format!(
                            "strip prefix {} from {}: {e}",
                            root.display(),
                            entry.display()
                        ),
                    })
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

#[path = "validate_all.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "validate_all_test_support.rs"]
mod test_support;
#[cfg(test)]
use test_support::deep_semantic_findings_prepared;
