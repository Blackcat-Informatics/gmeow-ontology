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

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use std::sync::Arc;

use gmeow_errors::{
    Advice, Diag, DiagLedger, Finding, FindingCategory, Grade, Report, Severity, StageId,
    Standpoint, register_code,
};
use gmeow_logic::certificate::ContradictionPolicy;
use purrdf::gts::model::Graph;
use purrdf::{
    PROJECTION_CODECS, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfTerm, RdfTriple,
    pair_loss_ledger,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use purrdf::slice::catalog::SliceCatalog;
use purrdf::slice::ownership::{DependencyEdge, OwnershipAnalyzer, OwnershipReport};
use purrdf::slice::{Phase, ToolchainContext, product_unit_key};

use crate::cache::{CachedResult, ValidationCache};
use crate::gufo::{self, GufoConfig};
use crate::lint::{self, LintConfig};
use crate::model::{owl, rdf, rdfs};
use crate::report_bridge::shacl_findings_from_report;
use crate::signature;
use crate::store;

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

        // Read the GTS segment-heads graph once (for the cache key) outside the timed
        // store-build phase; the timing should measure only dataset construction.
        let gts_graph: Option<Graph> = if let Some(bytes) = &options.gts_bytes {
            Some(store::read_gts_graph(bytes)?)
        } else {
            None
        };

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
            if let Some(bytes) = &options.gts_bytes {
                store::dataset_from_gts(bytes)
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
        let (shape_store, shapes) =
            timed(
                &mut timings,
                "parse-shapes",
                options,
                None,
                || match &options.shape_union_root {
                    Some(root) => purrdf::shapes::shape_union::load_shapes(root)
                        .map(|(store, shapes)| (Some(store), shapes))
                        .map_err(|e| Diag::of_kind(crate::error::Parse { detail: e })),
                    None => purrdf::shapes::engine::parse_shapes(shapes_ttl)
                        .map(|shapes| (None, shapes))
                        .map_err(|e| Diag::of_kind(crate::error::Parse { detail: e })),
                },
            )?;

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
        //   • gts_graph present  → segment_heads (already content-addressed).
        //   • slices_dir present → semantic Merkle product key over the catalog.
        //   • neither            → shapes-only key (the no-root case is preserved).
        let merged_shacl_key = if let Some(cache) = cache.as_ref() {
            let source_key = if let Some(graph) = &gts_graph {
                let mut heads: Vec<&[u8]> =
                    graph.segment_heads.iter().map(|h| h.as_slice()).collect();
                heads.sort();
                ValidationCache::cache_key(&heads)
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
                    Ok(shacl_findings_from_report(&report, None))
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
        // The shape STORE the advisory split scans is the one the shapes were parsed
        // from: the union loader already returns it (so a second parse of a
        // re-concatenated text — with different blank-node scoping than the union the
        // engine actually ran — is impossible), and the literal-document source parses
        // its own text.
        let advisory_shapes = match &shape_store {
            Some(store) => Some(Arc::clone(store)),
            None => purrdf::parse_dataset(shapes_ttl.as_bytes(), "text/turtle", None).ok(),
        };
        let advisories = advisory_shapes
            .as_deref()
            .map(|shapes| crate::advisory::split_advisory_findings(&mut report, shapes, &dataset))
            .unwrap_or_default();
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
            if let Some(bytes) = &options.gts_bytes {
                timed(&mut timings, "deep-semantic", options, None, || {
                    deep_semantic_findings(bytes, &mut report)
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
/// `validate.deep.*` findings the enrichment pass attaches `derived_from_quads` to
/// (Task 5), and it INHERITS the same hard-fail discipline: a verdict that cannot
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
    let bundle = purrdf::import_gts_events(gts_bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: format!("validate --deep: GTS read error: {e}"),
        })
    })?;
    let result = gmeow_logic::reason::reason_all(bundle.dataset.as_ref()).map_err(|e| {
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
    let policy =
        ContradictionPolicy::resolve_from_dataset(bundle.dataset.as_ref()).map_err(|e| {
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

    // Build the scoped coherence certificate from the SAME reasoning result, under
    // the SAME resolved policy and bundle hash, and attach it to the report metadata
    // (C2). The validate and release lanes share ONE certificate constructor.
    let bundle_hash = purrdf::gts::writer::digest_string(gts_bytes);
    let axiom_hashes = gmeow_logic::certificate::per_graph_axiom_hashes(
        bundle.dataset.as_ref(),
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

/// `owl:Nothing` — the class every contradiction/unsatisfiability clash quad forces
/// its witness into. The clash quad the explain skeleton is attached from is exactly
/// `type(individual, owl:Nothing)` (a contradiction) or `subClassOf(class,
/// owl:Nothing)` (an unsatisfiable class), so a witness's derivation is located by
/// the explanation whose target quad has the witness as subject, the witness world,
/// and `owl:Nothing` as object.
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

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
/// target object is `owl:Nothing` (the clash quad). Returns `None` when NO such
/// explanation exists — the absent-witness invariant violation the fold hard-fails on.
fn derived_quads_for_witness(
    explanations: &[gmeow_logic::explain::Explanation],
    witness_name: &str,
    witness_world: &str,
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
        if target.subject_iri != witness_name {
            continue;
        }
        // The clash quad forces the witness into owl:Nothing; match its N3 object
        // IRI so an unrelated quad about the same subject is never over-cited.
        let object_iri = target
            .obj_n3
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'));
        if object_iri != Some(OWL_NOTHING) {
            continue;
        }
        matched = true;
        cited.extend(expl.cited_iris.iter().cloned());
    }
    matched.then(|| cited.into_iter().collect())
}

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
            let derived_from_quads =
                derived_quads_for_witness(explanations, &witness.individual, &witness.world)
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
        let derived_from_quads =
            derived_quads_for_witness(explanations, &unsat.class, &unsat.world).ok_or_else(
                || WitnessDerivationMissing {
                    message: format!(
                        "unsatisfiable class {} (world {}) names a quad absent from the \
                         reasoning result's derivations — no explain skeleton could be located",
                        unsat.class, unsat.world
                    ),
                },
            )?;
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

    // Fast path: if every example's SHACL result is already cached, skip the
    // parallel re-validation entirely (main's example-shacl cache).
    if let Some(cache) = cache {
        let mut cached_findings: Vec<Finding> = Vec::new();
        let mut all_hit = true;
        for (_, path) in &examples {
            let example_key = example_shacl_key(cache, base_key, path)?;
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

    let base_projected = purrdf::shapes::engine::project_dataset(dataset).map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!("example base SHACL projection failed: {e}"),
        })
    })?;

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
                run_example_shacl(&base_projected, shapes, path, name)
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
    shapes: &purrdf::shapes::shapes::Shapes,
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
        purrdf::shapes::engine::project_dataset(example_ds.as_ref()).map_err(|e| {
            Diag::of_kind(crate::error::Engine {
                detail: format!("example {name}: SHACL projection failed: {e}"),
            })
        })?;
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
    let report = if example_allows_focus_pruning(example_ds.as_ref()) {
        let affected = affected_focus_terms(example_projected.as_ref());
        // ABox-only examples cannot alter the ontology's target/class/property
        // structure, so the merged run only needs to recheck focus terms touched
        // by the example. Examples that edit schema or SHACL shapes take the
        // full-scan branch above.
        purrdf::shapes::engine::validate_projected_dataset_with_focus_filter(
            merged,
            shapes,
            |_, focus| affected.contains(focus),
        )
    } else {
        purrdf::shapes::engine::validate_projected_dataset(merged, shapes)
    }
    .map_err(|e| {
        Diag::of_kind(crate::error::Engine {
            detail: format!("example {name}: SHACL validation failed: {e}"),
        })
    })?;
    Ok(shacl_findings_from_report(&report, Some(name)))
}

const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
const SHACL_NS: &str = "http://www.w3.org/ns/shacl#";

fn example_allows_focus_pruning(example: &RdfDataset) -> bool {
    for quad in example.owned_quads() {
        // Scan both the canonical `logic:subClassOf`/`logic:subPropertyOf` edges and
        // their `rdfs:` projection (gmeow_ns::subsumption_predicates doctrine;
        // crates/ns/src/lib.rs:106-166) — an example whose only structural content
        // is a re-authored canonical subsumption edge must be treated the same as
        // one authored in `rdfs:`, or focus pruning silently drops it.
        if gmeow_ns::SUB_CLASS_OF.contains(&quad.predicate.as_str())
            || gmeow_ns::SUB_PROPERTY_OF.contains(&quad.predicate.as_str())
            || quad.predicate.starts_with(SHACL_NS)
        {
            return false;
        }
        if quad.predicate == rdf::TYPE && is_schema_type(&quad.object) {
            return false;
        }
    }
    true
}

fn is_schema_type(term: &RdfTerm) -> bool {
    matches!(
        term,
        RdfTerm::Iri(iri)
            if iri == owl::CLASS
                || iri == rdfs::DATATYPE
                || iri == RDF_PROPERTY
                || iri == owl::OBJECT_PROPERTY
                || iri == owl::DATATYPE_PROPERTY
                || iri == owl::ANNOTATION_PROPERTY
                || iri == owl::FUNCTIONAL_PROPERTY
                || iri.starts_with(SHACL_NS)
    )
}

fn affected_focus_terms(example_projected: &RdfDataset) -> HashSet<purrdf::shapes::term::Term> {
    let mut affected = HashSet::new();
    for quad in example_projected.owned_quads() {
        affected.insert(rdf_term_to_shacl_term(&quad.subject));
        affected.insert(purrdf::shapes::term::Term::NamedNode(
            purrdf::shapes::term::NamedNode::new_unchecked(quad.predicate),
        ));
        affected.insert(rdf_term_to_shacl_term(&quad.object));
    }
    affected
}

fn rdf_term_to_shacl_term(term: &RdfTerm) -> purrdf::shapes::term::Term {
    use purrdf::shapes::term::{NamedNode, Term};

    match term {
        RdfTerm::Iri(iri) => Term::NamedNode(NamedNode::new_unchecked(iri.clone())),
        RdfTerm::BlankNode(label) => Term::BlankNode(label.clone()),
        RdfTerm::Literal(literal) => Term::Literal(rdf_literal_to_shacl_literal(literal)),
        RdfTerm::Triple(triple) => Term::Triple(Box::new(rdf_triple_to_shacl_triple(triple))),
    }
}

fn rdf_triple_to_shacl_triple(triple: &RdfTriple) -> purrdf::shapes::term::Triple {
    use purrdf::shapes::term::{NamedNode, Triple};

    let subject = rdf_term_to_shacl_term(&triple.subject);
    let predicate = NamedNode::new_unchecked(triple.predicate.clone());
    let object = rdf_term_to_shacl_term(&triple.object);
    Triple::new(subject, predicate, object)
}

fn rdf_literal_to_shacl_literal(literal: &RdfLiteral) -> purrdf::shapes::term::Literal {
    use purrdf::shapes::term::Literal;

    match (&literal.language, &literal.datatype) {
        (Some(lang), _) => match literal.direction {
            Some(direction) => Literal::new_directional_language_tagged_literal_unchecked(
                literal.lexical_form.clone(),
                lang.clone(),
                direction,
            ),
            None => Literal::new_language_tagged_literal_unchecked(
                literal.lexical_form.clone(),
                lang.clone(),
            ),
        },
        (None, Some(datatype)) => Literal::new_typed_literal(
            literal.lexical_form.clone(),
            purrdf::shapes::term::NamedNode::new_unchecked(datatype.clone()),
        ),
        (None, None) => Literal::new_simple_literal(literal.lexical_form.clone()),
    }
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
fn collect_ttl_paths(dir: &str) -> gmeow_errors::Result<Vec<PathBuf>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashSet};

    use purrdf::{DatasetView, GraphMatch, parse_dataset};

    /// Write `contents` to `name` inside a fresh RAII temp directory.
    ///
    /// The returned [`tempfile::TempDir`] owns the directory: it is removed on
    /// drop, including on panic and early return. Bind it to a named `_tmp`
    /// (never a bare `_`, which would drop it immediately) so it outlives the
    /// path. The file *name* is preserved because the validation run dispatches
    /// on the `.ttl` extension.
    fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    /// The per-example `base ∪ example` merge dedups shared base quads and adds the
    /// example-only quads, leaving the base unaffected (each example merges into a
    /// fresh dataset; there is no shared mutable store to leak into).
    #[test]
    fn example_merge_unions_base_and_example() {
        let base = parse_dataset(
            b"@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
            "text/turtle",
            None,
        )
        .unwrap();
        let base_quads: Vec<purrdf::RdfQuad> = base.owned_quads().collect();

        // An example carrying one duplicate of the base quad plus one new quad.
        let (_tmp, example_path) = write_tmp(
            "gmeow_validate_example_merge.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\nex:c ex:p ex:d .\n",
        );
        let example = store::parse_file_dataset(&example_path).unwrap();

        let mut builder = RdfDatasetBuilder::new();
        for q in &base_quads {
            builder.push_owned_quad(q);
        }
        builder.push_dataset(&example);
        let merged = builder.freeze().unwrap();
        // The duplicate base quad collapses; the example-only quad is added → 2 total.
        assert_eq!(merged.quad_count(), 2, "duplicate base quad must dedup");
        // The base dataset is unchanged by the merge.
        assert_eq!(base.quad_count(), 1, "base dataset must be untouched");
        assert_eq!(
            merged
                .quads_for_pattern(None, None, None, GraphMatch::Default)
                .count(),
            2
        );
    }

    /// G9 canonical-subsumption sweep: an example whose only structural content is a
    /// canonical `logic:subClassOf` edge must disable focus pruning exactly like an
    /// `rdfs:subClassOf` example does — otherwise the node under validation could be
    /// silently pruned away (crates/ns/src/lib.rs:106-166).
    #[test]
    fn example_with_canonical_logic_subclass_of_disallows_focus_pruning() {
        let example = parse_dataset(
            b"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
              gmeow:Cyborg logic:subClassOf gmeow:Animal .\n",
            "text/turtle",
            None,
        )
        .unwrap();
        assert!(
            !example_allows_focus_pruning(example.as_ref()),
            "a canonical logic:subClassOf edge must disable focus pruning"
        );
    }

    /// The `rdfs:subPropertyOf` projected spelling must also disable pruning (the
    /// existing arm this migration preserves).
    #[test]
    fn example_with_rdfs_subproperty_of_disallows_focus_pruning() {
        let example = parse_dataset(
            b"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
              gmeow:mediatesRole rdfs:subPropertyOf gmeow:mediates .\n",
            "text/turtle",
            None,
        )
        .unwrap();
        assert!(
            !example_allows_focus_pruning(example.as_ref()),
            "a projected rdfs:subPropertyOf edge must disable focus pruning"
        );
    }

    #[test]
    fn affected_focus_terms_include_predicate_iris() {
        use purrdf::shapes::term::{NamedNode, Term};

        let example = parse_dataset(
            b"@prefix ex: <https://example.org/> .\nex:s ex:p ex:o .\n",
            "text/turtle",
            None,
        )
        .unwrap();
        let affected = affected_focus_terms(example.as_ref());

        assert!(affected.contains(&Term::NamedNode(NamedNode::new_unchecked(
            "https://example.org/s"
        ))));
        assert!(affected.contains(&Term::NamedNode(NamedNode::new_unchecked(
            "https://example.org/p"
        ))));
        assert!(affected.contains(&Term::NamedNode(NamedNode::new_unchecked(
            "https://example.org/o"
        ))));
    }

    fn minimal_gts_bytes() -> Vec<u8> {
        use purrdf::gts::model::{Term, TermKind};
        use purrdf::gts::writer::Writer;

        let mut graph = purrdf::gts::model::Graph::default();
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/a".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/p".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/b".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        });
        graph.quads.push((0, 1, 2, None));

        let writer = Writer::deterministic(&graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed");
        writer.to_bytes()
    }

    #[test]
    fn deep_semantic_pass_flags_inconsistency_and_consistency() {
        // An inconsistent bundle: A⊑B, A⊑C, B disjointWith C, x:A forces x into
        // owl:Nothing — the shared ReasoningResult reports information=both.
        let inconsistent = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
";
        let bytes = gts_bytes_from_turtle(inconsistent);
        let mut report = Report::new("validate");
        deep_semantic_findings(&bytes, &mut report).expect("deep pass must run");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "validate.deep.inconsistent"),
            "the deep pass must flag the inconsistency: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );

        // A consistent bundle: A⊑B, x:A. No clash → a consistency note, no error.
        let consistent = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:x rdf:type ex:A .
";
        let bytes = gts_bytes_from_turtle(consistent);
        let mut report = Report::new("validate");
        deep_semantic_findings(&bytes, &mut report).expect("deep pass must run");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == "validate.deep.consistent"),
            "a consistent bundle must record the consistency note"
        );
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "validate.deep.inconsistent"),
            "a consistent bundle must NOT flag inconsistency"
        );
    }

    #[test]
    fn fold_categorizes_permitted_versus_forbidden_glut() {
        // A real within-world glut, reasoned from an inconsistent fixture.
        let inconsistent = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
";
        let bytes = gts_bytes_from_turtle(inconsistent);
        let bundle = purrdf::import_gts_events(&bytes).expect("gts read");
        let result = gmeow_logic::reason::reason_all(bundle.dataset.as_ref()).expect("reason");
        assert!(!result.is_consistent(), "the fixture must produce a glut");
        let explanations = gmeow_logic::explain::explanations_for_result(&result)
            .expect("explain skeletons must build for a real verdict");

        // FORBIDDEN (classical): an Error categorized ContradictionWitness; gate fails.
        let mut forbidden = Report::new("validate");
        fold_reasoning_result(
            &result,
            ContradictionPolicy::ForbidGapAndGlut,
            &explanations,
            &mut forbidden,
        )
        .expect("fold must locate every witness derivation");
        let f = forbidden
            .findings
            .iter()
            .find(|f| f.code == "validate.deep.inconsistent")
            .expect("forbidden glut must emit a deep.inconsistent error");
        assert_eq!(f.severity, Severity::Error);
        assert_eq!(f.category, Some(FindingCategory::ContradictionWitness));
        assert!(!forbidden.ok(), "a forbidden glut must fail the gate");

        // PERMITTED (glut-admitting): a Warning categorized PermittedEpistemicConflict;
        // the gate stays green — the load-bearing acceptance criterion (c).
        let mut permitted = Report::new("validate");
        fold_reasoning_result(
            &result,
            ContradictionPolicy::ForbidGap,
            &explanations,
            &mut permitted,
        )
        .expect("fold must locate every witness derivation");
        assert!(
            !permitted
                .findings
                .iter()
                .any(|f| f.code == "validate.deep.inconsistent"),
            "a permitted glut must NOT emit the forbidden inconsistency error"
        );
        let p = permitted
            .findings
            .iter()
            .find(|f| f.code == "validate.deep.permitted-conflict")
            .expect("permitted glut must emit a permitted-conflict warning");
        assert_eq!(p.severity, Severity::Warning);
        assert_eq!(
            p.category,
            Some(FindingCategory::PermittedEpistemicConflict)
        );
        assert!(
            permitted.ok(),
            "a permitted, disclosed contradiction must NOT fail the gate"
        );
    }

    /// The inconsistent bundle every derivation-attach test reasons over: `x : A`,
    /// `A ⊑ B`, `A ⊑ C`, `B ⊐⊏ C` forces `x` into `owl:Nothing`.
    const INCONSISTENT_TTL: &str = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
";

    /// A forbidden contradiction finding carries the explain-skeleton
    /// cited-quad-reifier derivation (`derived_from_quads`) of its clash quad, and
    /// leaves the finding-fingerprint edges (`antecedents`/`root_cause`) untouched —
    /// the namespace guard: the two edges are NEVER conflated.
    #[test]
    fn deep_inconsistent_finding_carries_derivation_not_antecedents() {
        let bytes = gts_bytes_from_turtle(INCONSISTENT_TTL);
        let bundle = purrdf::import_gts_events(&bytes).expect("gts read");
        let result = gmeow_logic::reason::reason_all(bundle.dataset.as_ref()).expect("reason");
        assert!(!result.is_consistent(), "the fixture must produce a glut");
        let explanations = gmeow_logic::explain::explanations_for_result(&result)
            .expect("explain skeletons must build for a real verdict");

        let mut report = Report::new("validate");
        fold_reasoning_result(
            &result,
            ContradictionPolicy::ForbidGapAndGlut,
            &explanations,
            &mut report,
        )
        .expect("fold must locate every witness derivation");

        let finding = report
            .findings
            .iter()
            .find(|f| f.code == "validate.deep.inconsistent")
            .expect("a forbidden glut must emit a deep.inconsistent error");

        assert!(
            !finding.derived_from_quads.is_empty(),
            "the reasoned-quad verdict must carry its explain-skeleton derivation"
        );
        // The cited-IRI skeleton names the clash quad's own reifier and its world —
        // the load-bearing logic-world coordinates of the derivation.
        assert!(
            finding
                .derived_from_quads
                .iter()
                .any(|iri| iri.starts_with("https://blackcatinformatics.ca/gmeow/reifier/")),
            "derived_from_quads must cite at least one logic-world quad reifier; got {:?}",
            finding.derived_from_quads
        );
        assert!(
            finding
                .derived_from_quads
                .contains(&"https://blackcatinformatics.ca/gmeow/graph/rl-default".to_owned()),
            "the derivation is cited within its world; got {:?}",
            finding.derived_from_quads
        );
        // Namespace guard: the finding-fingerprint edges stay empty — a quad reifier
        // must NEVER be written into antecedents/root_cause.
        assert!(
            finding.antecedents.is_empty(),
            "antecedents (finding-fingerprint IRIs) must stay empty"
        );
        assert!(
            finding.root_cause.is_none(),
            "root_cause (finding-fingerprint IRI) must stay unset"
        );
    }

    /// The absent-witness invariant: a verdict names a witness whose clash quad has
    /// no locatable explain skeleton → `fold_reasoning_result` HARD-FAILS with
    /// [`WitnessDerivationMissing`], never a silent (or advisory-Note) attach. Here
    /// the real inconsistent result is folded with an EMPTY explanation set, so no
    /// witness can be located — the same shape as a verdict referencing a quad the
    /// result does not carry.
    #[test]
    fn fold_hard_fails_when_witness_derivation_absent() {
        let bytes = gts_bytes_from_turtle(INCONSISTENT_TTL);
        let bundle = purrdf::import_gts_events(&bytes).expect("gts read");
        let result = gmeow_logic::reason::reason_all(bundle.dataset.as_ref()).expect("reason");
        assert!(!result.is_consistent(), "the fixture must produce a glut");

        let mut report = Report::new("validate");
        let err = fold_reasoning_result(
            &result,
            ContradictionPolicy::ForbidGapAndGlut,
            &[],
            &mut report,
        )
        .expect_err("an unlocatable witness derivation must HARD-FAIL the fold");
        assert!(
            err.message.contains("contradiction witness")
                && err.message.contains("no explain skeleton"),
            "the invariant violation must name the unlocatable witness; got {:?}",
            err.message
        );
    }

    /// Build canonical GTS bytes from a Turtle string for the deep-pass test.
    fn gts_bytes_from_turtle(ttl: &str) -> Vec<u8> {
        let dataset =
            purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse test turtle");
        purrdf::gts_write::to_gts(
            &dataset,
            &purrdf::RdfLookaside::default(),
            "gmeow-validate-deep-test",
        )
        .expect("encode GTS bytes")
    }

    /// A bundle that BOTH reasons to a within-world glut AND declares a
    /// `logic:ReasoningContract` whose `logic:admissibleValuation` is the supplied
    /// policy local name (e.g. `ForbidGap` admits a glut; `ForbidGapAndGlut` forbids
    /// it). The contract is real RDF in the bundle, so `deep_semantic_findings`
    /// resolves the governing policy off the bundle exactly as production does.
    fn glut_bundle_with_contract(valuation_local: &str) -> Vec<u8> {
        let ttl = format!(
            "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
ex:governingContract rdf:type logic:ReasoningContract ;
    logic:admissibleValuation logic:{valuation_local} .
"
        );
        gts_bytes_from_turtle(&ttl)
    }

    /// H3(a)+(b)+(c): the FULL deep path on a real glut-admitting bundle. The policy
    /// comes from the bundle's declared contract (proving the C1 wiring works on real
    /// data): a `ForbidGap` (glut-admitting) contract turns the within-world glut into
    /// a PERMITTED, disclosed conflict — a non-error finding that keeps the gate
    /// GREEN — and a coherence certificate is attached to the report metadata.
    #[test]
    fn deep_pass_permitted_glut_stays_green_with_certificate() {
        let bytes = glut_bundle_with_contract("ForbidGap");
        let mut report = Report::new("validate");
        deep_semantic_findings(&bytes, &mut report).expect("deep pass must run");

        // (a) a PermittedEpistemicConflict finding at NON-error severity.
        let permitted = report
            .findings
            .iter()
            .find(|f| f.category == Some(FindingCategory::PermittedEpistemicConflict))
            .expect("a glut-admitting contract must emit a permitted-conflict finding");
        assert_ne!(
            permitted.severity,
            Severity::Error,
            "a permitted, disclosed conflict must NOT be an error"
        );

        // (b) NO error-severity finding from the conflict — the gate stays GREEN.
        assert!(
            report.ok(),
            "a permitted glut under its declared contract must keep the gate green: {:?}",
            report.legacy_errors()
        );
        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code == "validate.deep.inconsistent"),
            "no forbidden inconsistency error must be emitted"
        );

        // (c) a coherence certificate is present in report metadata.
        assert!(
            report.metadata.contains_key("coherence_certificate"),
            "the deep pass must attach a coherence certificate"
        );
        let cert = report.metadata["coherence_certificate"]
            .as_str()
            .expect("certificate is serialized as N-Quads string");
        assert!(
            cert.contains("permittedConflictWitness"),
            "the certificate must disclose the permitted conflict: {cert}"
        );
    }

    /// H3 (forbidding side): the SAME glut under a glut-FORBIDDING declared contract
    /// (`ForbidGapAndGlut`) yields an Error-severity ContradictionWitness (gate fails),
    /// and the release-lane certificate build REFUSES (Err) on the same bundle.
    #[test]
    fn deep_pass_forbidden_glut_fails_and_release_refuses() {
        let bytes = glut_bundle_with_contract("ForbidGapAndGlut");
        let mut report = Report::new("validate");
        deep_semantic_findings(&bytes, &mut report).expect("deep pass must run");

        let witness = report
            .findings
            .iter()
            .find(|f| f.category == Some(FindingCategory::ContradictionWitness))
            .expect("a glut-forbidding contract must emit a contradiction witness");
        assert_eq!(witness.severity, Severity::Error);
        assert!(!report.ok(), "a forbidden glut must fail the gate");

        // The release lane reasons over the SAME bundle bytes under the SAME
        // bundle-resolved policy, then REFUSES to sign an incoherent bundle (hard-fail,
        // no DEFAULT papering-over). gmeow-pipeline cannot be imported here (it depends
        // on gmeow-validate — a cycle), so exercise the exact decision the release lane
        // makes: resolve the policy off the bundle, build the outcome, and assert it is
        // refused (release.rs returns Err on `outcome.is_refused()`).
        let bundle = purrdf::import_gts_events(&bytes).expect("gts read");
        let result = gmeow_logic::reason::reason_all(bundle.dataset.as_ref()).expect("reason");
        let policy =
            ContradictionPolicy::resolve_from_dataset(bundle.dataset.as_ref()).expect("policy");
        assert_eq!(
            policy,
            ContradictionPolicy::ForbidGapAndGlut,
            "the bundle's declared contract must resolve to the glut-forbidding policy"
        );
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
            purrdf::gts::writer::digest_string(&bytes),
            gmeow_logic::certificate::per_graph_axiom_hashes(
                bundle.dataset.as_ref(),
                purrdf::gts::writer::digest_string,
            ),
            policy,
            "2026-06-28T00:00:00Z",
            projection_loss_codes,
        )
        .expect("outcome build");
        assert!(
            outcome.is_refused(),
            "the release lane must refuse to sign a bundle carrying a forbidden integrity violation"
        );
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
        // The canonical report is always present, even on a clean run.
        assert!(run.report.normalized().ok());
        assert_eq!(run.dataset.quad_count(), 1);

        // The single triple (s,p,o) is present in the shared dataset.
        let ds = &run.dataset;
        let s = ds.term_id_by_value(&purrdf::TermValue::iri("https://example.org/a"));
        let p = ds.term_id_by_value(&purrdf::TermValue::iri("https://example.org/p"));
        let o = ds.term_id_by_value(&purrdf::TermValue::iri("https://example.org/b"));
        assert!(
            ds.quads_for_pattern(s, p, o, GraphMatch::Any)
                .next()
                .is_some(),
            "the (a,p,b) triple must be present in the shared dataset"
        );
    }

    /// With the fixed demonstrator removed (greenfield), a normal-completion run
    /// over a bundle carrying NO accepted recommendation candidates emits an EMPTY
    /// advisory tier — honest absence, not a synthetic always-on Note. This proves the
    /// unconditional demonstrator is gone. Harvested advisories surfacing on a real
    /// candidate-bearing dataset is covered by the advisory-bridge unit tests
    /// (`harvest_yields_note_with_subject_and_howtouse_suggestion` et al.) and the
    /// pipeline stage test; the full `make check` over gmeow.gts (which ships the advisory
    /// candidates) exercises the whole path end to end.
    #[test]
    fn clean_run_over_candidate_free_bundle_emits_no_advisory() {
        let bytes = minimal_gts_bytes();
        let options = ValidateOptions {
            gts_bytes: Some(bytes),
            ..ValidateOptions::default()
        };

        let run = ValidationRun::run(&[], "", "", "", &minimal_lint_config(), &options)
            .expect("ValidationRun::run must succeed");

        // No advisory contaminates the error/warning surfaces, and a clean run is ok.
        assert!(
            run.errors().is_empty(),
            "no errors on a clean run: {:?}",
            run.errors()
        );
        assert!(
            run.warnings().is_empty(),
            "no warnings on a clean run: {:?}",
            run.warnings()
        );
        assert!(
            run.report.normalized().ok(),
            "a clean report must still be ok"
        );

        // A candidate-free bundle harvests NOTHING — no claim hook, no advice.* finding.
        assert!(
            run.advisory_claims.is_empty(),
            "a candidate-free bundle must harvest no advisory claims; got: {:?}",
            run.advisory_claims
        );
        assert!(
            !run.report
                .findings
                .iter()
                .any(|f| f.code.starts_with(crate::codes::ADVICE_FAMILY)),
            "a candidate-free bundle must emit no advice.* finding"
        );
    }

    /// The syntax/sameAs short-circuit early return (a hard-failed run) must NOT
    /// emit any advisory. Triggered with VALID Turtle carrying a banned
    /// `owl:sameAs` to an external entity: the file parses (so build-store
    /// succeeds), then Phase 2 records a sameAs-ban error, so `run` returns at the
    /// `!errors.is_empty()` short-circuit (NOT via an Err) — exercising the real
    /// Ok early-return path, not the vacuous build-store-failure path.
    #[test]
    fn early_return_path_emits_no_advisory() {
        let (_tmp, banned_ttl_path) = write_tmp(
            "gmeow_validate_advisory_early_return_sameas.ttl",
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:Foo owl:sameAs <https://external.example.org/bar> .\n",
        );
        let source = banned_ttl_path.to_string_lossy().to_string();

        let options = ValidateOptions::default();
        let run = ValidationRun::run(&[source], "", "", "", &minimal_lint_config(), &options)
            .expect("valid-but-banned Turtle must reach the Ok short-circuit, not Err");

        // The run hard-failed (the sameAs ban is an error), proving we hit the
        // short-circuit early-return path.
        assert!(
            !run.errors().is_empty(),
            "expected a sameAs-ban error to drive the short-circuit; got none"
        );
        // The hard-fail path emits NO advisory claim and NO advisory finding.
        assert!(
            run.advisory_claims.is_empty(),
            "early-return path must emit no advisory claims"
        );
        assert!(
            !run.report
                .findings
                .iter()
                .any(|f| f.code.starts_with(crate::codes::ADVICE_FAMILY)),
            "early-return path must emit no advice.* finding"
        );
    }

    // ── Category-assignment tests (H1) ──────────────────────────────────────

    /// A SHACL constraint violation folded through `shacl_findings_from_report`
    /// must carry `FindingCategory::DataShapeViolation`.
    #[test]
    fn shacl_violation_is_categorized_data_shape_violation() {
        use purrdf::shapes::report::{ValidationReport, ValidationResult};
        use purrdf::shapes::term::{Literal, NamedNode, Term};

        let result = ValidationResult {
            focus_node: Term::NamedNode(NamedNode::new_unchecked("https://example.org/FocusA")),
            result_path: None,
            path_structure: None,
            value: None,
            source_constraint_component: NamedNode::new_unchecked(
                "http://www.w3.org/ns/shacl#MinCountConstraintComponent",
            ),
            source_shape: Term::NamedNode(NamedNode::new_unchecked("https://example.org/ShapeA")),
            severity: purrdf::shapes::report::Severity::Violation,
            message: Some("must have at least one value".to_owned()),
            source_box_roles: vec![],
            path_box_roles: vec![],
            result_box_roles: vec![],
            attributions: vec![],
        };
        let _ = Literal::new_simple_literal("unused");
        let report = ValidationReport {
            conforms: false,
            results: vec![result],
        };

        let findings = shacl_findings_from_report(&report, None);

        assert_eq!(findings.len(), 1, "expected exactly one finding");
        assert_eq!(
            findings[0].category,
            Some(FindingCategory::DataShapeViolation),
            "a SHACL violation must carry DataShapeViolation; got {:?}",
            findings[0].category
        );
        assert!(
            findings[0].code.starts_with("shacl."),
            "finding code must start with 'shacl.'; got {}",
            findings[0].code
        );
    }

    /// A non-conforming SHACL report with zero results (the `shacl.nonconforming`
    /// guard) must also carry `FindingCategory::DataShapeViolation`.
    #[test]
    fn shacl_nonconforming_guard_is_categorized_data_shape_violation() {
        use purrdf::shapes::report::ValidationReport;

        let report = ValidationReport {
            conforms: false,
            results: vec![],
        };

        let findings = shacl_findings_from_report(&report, None);

        assert_eq!(
            findings.len(),
            1,
            "expected the nonconforming guard finding"
        );
        assert_eq!(findings[0].code, "shacl.nonconforming");
        assert_eq!(
            findings[0].category,
            Some(FindingCategory::DataShapeViolation),
            "the nonconforming guard must carry DataShapeViolation"
        );
    }

    /// A `ReasoningResult` with `evaluation=BudgetExhausted` must cause
    /// `fold_reasoning_result` to emit a finding categorized `IncompleteCheck`.
    #[test]
    fn budget_exhausted_result_emits_incomplete_check() {
        use gmeow_logic::result::{
            CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
            ReasoningResult, ResultPayload, ResultProvenance,
        };

        let result = ReasoningResult::new(
            InputStatus::Valid,
            EvaluationStatus::BudgetExhausted,
            // BudgetExhausted → completeness must be Incomplete or Unknown (not CompleteForFragment
            // with BudgetExhausted, as that would mean conclusive). Use Incomplete.
            CompletenessStatus::Incomplete,
            PreservationClaim::exact(),
            // Neither requires conclusive, but BudgetExhausted + Incomplete is non-conclusive,
            // so the information state must be Undetermined (not Neither).
            InformationState::Undetermined,
            ResultProvenance::native("test-contract", "test-world"),
            ResultPayload::Empty,
        );

        let mut report = Report::new("validate");
        fold_reasoning_result(
            &result,
            ContradictionPolicy::ForbidGapAndGlut,
            &[],
            &mut report,
        )
        .expect("synthetic empty-payload result folds without witnesses");

        let incomplete_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.category == Some(FindingCategory::IncompleteCheck))
            .collect();

        assert!(
            !incomplete_findings.is_empty(),
            "a BudgetExhausted result must emit at least one IncompleteCheck finding; \
             got findings: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
        assert_eq!(
            incomplete_findings[0].code, "validate.deep.incomplete",
            "incomplete finding must carry the expected code"
        );
        assert_eq!(
            incomplete_findings[0].severity,
            Severity::Warning,
            "incomplete check must be a Warning, not an error"
        );
    }

    /// A `ReasoningResult` with `completeness=Incomplete` (but evaluation Completed)
    /// must also trigger the `IncompleteCheck` category.
    #[test]
    fn completeness_incomplete_result_emits_incomplete_check() {
        use gmeow_logic::result::{
            CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
            ReasoningResult, ResultPayload, ResultProvenance,
        };

        // Completed + CompleteForFragment is conclusive → Neither is valid.
        // Completed + Incomplete is conclusive via Completed → Neither is still valid.
        // We want to fire the IncompleteCheck path: evaluation=Completed, completeness=Incomplete.
        let result = ReasoningResult::new(
            InputStatus::Valid,
            EvaluationStatus::Completed,
            CompletenessStatus::Incomplete,
            PreservationClaim::exact(),
            // Completed alone makes it conclusive, so Neither is valid.
            InformationState::Neither,
            ResultProvenance::native("test-contract", "test-world"),
            ResultPayload::Empty,
        );

        let mut report = Report::new("validate");
        fold_reasoning_result(
            &result,
            ContradictionPolicy::ForbidGapAndGlut,
            &[],
            &mut report,
        )
        .expect("synthetic empty-payload result folds without witnesses");

        let incomplete = report
            .findings
            .iter()
            .find(|f| f.category == Some(FindingCategory::IncompleteCheck))
            .expect("completeness=Incomplete must emit an IncompleteCheck finding");
        assert_eq!(incomplete.code, "validate.deep.incomplete");
        assert_eq!(incomplete.severity, Severity::Warning);
    }

    /// A fully-conclusive, consistent result (evaluation=Completed, completeness=CompleteForFragment)
    /// must NOT emit any IncompleteCheck finding.
    #[test]
    fn conclusive_consistent_result_does_not_emit_incomplete_check() {
        use gmeow_logic::result::{
            CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
            ReasoningResult, ResultPayload, ResultProvenance,
        };

        let result = ReasoningResult::new(
            InputStatus::Valid,
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
            PreservationClaim::exact(),
            InformationState::Neither,
            ResultProvenance::native("test-contract", "test-world"),
            ResultPayload::Empty,
        );

        let mut report = Report::new("validate");
        fold_reasoning_result(
            &result,
            ContradictionPolicy::ForbidGapAndGlut,
            &[],
            &mut report,
        )
        .expect("synthetic empty-payload result folds without witnesses");

        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.category == Some(FindingCategory::IncompleteCheck)),
            "a conclusive, consistent result must NOT emit IncompleteCheck; \
             got: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
    }

    /// `fold_reasoning_result` must emit at least one `FindingCategory::ProjectionLoss`
    /// finding sourced from the genuine static loss ledger, distinct from any
    /// `UnsupportedSemanticFeature` findings. The ledger has at least one entry for
    /// `gts → owl-dl` (named-graph-dropped + owl-dl-projection), so the count is
    /// deterministically at least 2.
    #[test]
    fn fold_emits_projection_loss_findings_from_ledger() {
        use gmeow_logic::result::{
            CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
            ReasoningResult, ResultPayload, ResultProvenance,
        };

        let result = ReasoningResult::new(
            InputStatus::Valid,
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
            PreservationClaim::exact(),
            InformationState::Neither,
            ResultProvenance::native("test-contract", "test-world"),
            ResultPayload::Empty,
        );

        let mut report = Report::new("validate");
        fold_reasoning_result(
            &result,
            ContradictionPolicy::ForbidGapAndGlut,
            &[],
            &mut report,
        )
        .expect("synthetic empty-payload result folds without witnesses");

        let projection_loss_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.category == Some(FindingCategory::ProjectionLoss))
            .collect();

        assert!(
            !projection_loss_findings.is_empty(),
            "fold_reasoning_result must emit at least one ProjectionLoss finding; \
             got findings: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );

        // All ProjectionLoss findings carry the expected code and severity.
        for f in &projection_loss_findings {
            assert_eq!(
                f.code, "validate.deep.projection-loss",
                "ProjectionLoss finding must carry the expected code"
            );
            assert_eq!(
                f.severity,
                Severity::Note,
                "ProjectionLoss must be a Note (informational), not a failure"
            );
        }

        // The ledger must contribute at least the owl-dl pair (named-graph-dropped +
        // owl-dl-projection), so we expect at least 2 findings.
        assert!(
            projection_loss_findings.len() >= 2,
            "must have at least 2 ProjectionLoss findings (owl-dl pair); got {}",
            projection_loss_findings.len()
        );

        // Messages must name the target codec and contain the loss code.
        let has_named_graph_dropped = projection_loss_findings
            .iter()
            .any(|f| f.message.contains("named-graph-dropped"));
        assert!(
            has_named_graph_dropped,
            "at least one ProjectionLoss finding must mention 'named-graph-dropped'"
        );

        // ProjectionLoss findings must NOT carry UnsupportedSemanticFeature category.
        for f in &projection_loss_findings {
            assert_ne!(
                f.category,
                Some(FindingCategory::UnsupportedSemanticFeature),
                "ProjectionLoss must not be conflated with UnsupportedSemanticFeature"
            );
        }
    }

    /// Verify that `deep_semantic_findings` (the full GTS path) also emits
    /// ProjectionLoss findings — confirming they reach the report via the real bundle path.
    #[test]
    fn deep_semantic_findings_emits_projection_loss_on_consistent_bundle() {
        // A minimal consistent bundle is sufficient; the projection losses come from
        // the static ledger, not from bundle content.
        let consistent = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:x rdf:type ex:A .
";
        let bytes = gts_bytes_from_turtle(consistent);
        let mut report = Report::new("validate");
        deep_semantic_findings(&bytes, &mut report).expect("deep pass must run");

        let projection_loss_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.category == Some(FindingCategory::ProjectionLoss))
            .collect();

        assert!(
            !projection_loss_findings.is_empty(),
            "deep_semantic_findings must emit at least one ProjectionLoss finding; \
             got findings: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn interned_shacl_finding_roundtrips_related_locations_and_detail() {
        use gmeow_errors::Location;

        // A SHACL-shaped finding: focus-node PRIMARY location, result-path + value
        // RELATED locations, and a "source shape: …" detail (exactly what
        // `finding_from_shacl` produces). Interning it onto the run ledger and
        // projecting back via `project_report` must carry ALL of them — the
        // fingerprint keys on the message-INDEPENDENT structural identity, but no
        // structural anchor is lost through the ledger round-trip.
        let mut finding = Finding::new(
            Severity::Error,
            "shacl.MinCountConstraintComponent",
            "missing required property",
        )
        .with_tool("shacl")
        .with_category(FindingCategory::DataShapeViolation);
        finding.add_location(Location {
            logical: Some("https://ex/a".to_owned()),
            ..Location::default()
        });
        finding.related_locations.push(Location {
            logical: Some("path https://ex/p".to_owned()),
            ..Location::default()
        });
        finding.related_locations.push(Location {
            logical: Some("value https://ex/bad".to_owned()),
            ..Location::default()
        });
        finding.detail = Some("source shape: https://ex/shape".to_owned());

        let mut ledger = DiagLedger::new();
        intern_finding(
            &mut ledger,
            StageId::new("validate.shacl"),
            Standpoint::Binding,
            &finding,
        );
        let report = ledger.project_report("validate");
        let projected = report
            .findings
            .iter()
            .find(|f| f.code == "shacl.MinCountConstraintComponent")
            .expect("interned SHACL finding must project back into the report");

        // The focus-node primary location survives.
        assert_eq!(
            projected
                .primary_location()
                .and_then(|l| l.logical.as_deref()),
            Some("https://ex/a"),
            "focus-node primary location must round-trip"
        );
        // The SHACL result-path and offending-value related locations survive
        // (carried as first-class Labels, re-emitted by `to_finding`).
        assert!(
            projected
                .related_locations
                .iter()
                .any(|l| l.logical.as_deref() == Some("path https://ex/p")),
            "result-path related location must round-trip; got {:?}",
            projected.related_locations
        );
        assert!(
            projected
                .related_locations
                .iter()
                .any(|l| l.logical.as_deref() == Some("value https://ex/bad")),
            "offending-value related location must round-trip; got {:?}",
            projected.related_locations
        );
        // The "source shape: …" detail survives (carried as a context frame,
        // folded back into the projected finding's detail).
        assert_eq!(
            projected.detail.as_deref(),
            Some("source shape: https://ex/shape"),
            "the source-shape detail must round-trip"
        );

        // Hard Invariant 6: two findings identical in structural identity but
        // differing only in message are the SAME witness — interning a
        // message-variant of the same finding must NOT add a second finding.
        let mut variant = finding.clone();
        variant.message = "a differently-worded violation".to_owned();
        intern_finding(
            &mut ledger,
            StageId::new("validate.shacl"),
            Standpoint::Binding,
            &variant,
        );
        let merged = ledger.project_report("validate");
        assert_eq!(
            merged
                .findings
                .iter()
                .filter(|f| f.code == "shacl.MinCountConstraintComponent")
                .count(),
            1,
            "a message-only variant must hash-cons-merge, not fork a new finding"
        );
    }

    #[test]
    fn distinct_lines_of_same_constraint_do_not_hash_cons_merge() {
        use gmeow_errors::Location;

        // Two structurally-distinct violations of the SAME constraint at DIFFERENT
        // lines of one file: identical code / severity / path / detail, no `logical`
        // location, differing only by `line`. Because line/column are part of the
        // message-independent structural identity, these are genuinely different
        // witnesses and must NOT hash-cons-merge — both line numbers must survive.
        let make = |line: u32| {
            let mut finding = Finding::new(
                Severity::Error,
                "shacl.MinCountConstraintComponent",
                "missing required property",
            )
            .with_tool("shacl")
            .with_category(FindingCategory::DataShapeViolation);
            finding.add_location(Location {
                path: Some("ontology.ttl".to_owned()),
                line: Some(line),
                ..Location::default()
            });
            finding.detail = Some("source shape: https://ex/shape".to_owned());
            finding
        };

        let mut ledger = DiagLedger::new();
        for line in [10u32, 40u32] {
            intern_finding(
                &mut ledger,
                StageId::new("validate.shacl"),
                Standpoint::Binding,
                &make(line),
            );
        }
        let report = ledger.project_report("validate");

        let lines: std::collections::BTreeSet<u32> = report
            .findings
            .iter()
            .filter(|f| f.code == "shacl.MinCountConstraintComponent")
            .filter_map(|f| f.primary_location().and_then(|l| l.line))
            .collect();
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|f| f.code == "shacl.MinCountConstraintComponent")
                .count(),
            2,
            "two distinct-line violations of one constraint must NOT merge; got \
             lines {lines:?}"
        );
        assert!(
            lines.contains(&10) && lines.contains(&40),
            "both violated line numbers must survive the ledger round-trip; got {lines:?}"
        );
    }
}
