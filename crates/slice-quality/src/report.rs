// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end slice scoring: assemble the slice graph, run every rubric axis, and
//! build the assessment + the advisory report on the diagnostics substrate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_errors::{Finding, Report, Rule, Severity, Standpoint, seed_codes};
use gmeow_validate::rule_catalog::help_uri_for;

use crate::graph::{self, instances_of};
use crate::model::{Axis, AxisGrade, MeasurementStandard, SliceAssessment};
use crate::score::{ScoreContext, ScoringEnv, advisory};
use crate::{axes, lattice};

/// Advice-ranking KIND: an axis-level advice template (the rubric's
/// `gmeow:axisAdviceTemplate`, surfaced once per deficient axis) ranks strictly
/// AHEAD of that same axis's per-term instantiated findings. This is an EXPLICIT
/// ordering key — the order must never ride on code-string spelling (a `.template`
/// code sorts AFTER a `.no-stereotype` code because 'n' < 't', which would land the
/// template BELOW its per-term findings and violate "ahead of").
const ADVICE_KIND_TEMPLATE: u8 = 0;
/// The per-term instantiated advisory a primitive surfaced (ranks after its axis's
/// template item — see [`ADVICE_KIND_TEMPLATE`]).
const ADVICE_KIND_INSTANCE: u8 = 1;

/// Every diagnostic code a slice-quality report can emit — the two structural
/// codes minted by [`SliceReport::to_report`] (`grade`/`rollup`) plus every axis
/// and reasoner advisory code the primitives surface. This is the enumeration
/// authority the command seeds into the process-wide code registry so that every
/// emitted finding carries a *registered* code (never a bare, unregistered
/// string), and the set a help URI is attached to. Kept in one place so a new
/// advisory code is registered the moment it is listed here (a drift test in this
/// module pins the axes/reasoner emitters to this list).
pub const FINDING_CODES: &[&str] = &[
    // Structural report codes (report.rs).
    "slice-quality.grade",
    "slice-quality.rollup",
    // Reasoner-derived axis codes (reasoner.rs).
    "slice-quality.reasoner.no-closure",
    "slice-quality.reasoner.closure-redundant",
    "slice-quality.reasoner.no-obligations",
    "slice-quality.reasoner.counterexample-no-clash",
    // Per-axis advisory codes (axes.rs).
    "slice-quality.grounding.no-stereotype",
    "slice-quality.information.incomplete-coat",
    "slice-quality.prose.definition-no-boundary",
    "slice-quality.prose.example-not-triple",
    "slice-quality.prose.test-rationale",
    "slice-quality.linkage.no-correspondence-surface",
    "slice-quality.linkage.no-calculus-eligible-correspondence",
    "slice-quality.linkage.uncalculated-correspondence",
    "slice-quality.gmn-glyph-optimality.audit-graph-unavailable",
    "slice-quality.gmn-glyph-optimality.unaudited-executable-target",
    "slice-quality.projection.hand-authored-shapes",
    "slice-quality.projection.no-mappings",
    "slice-quality.testing.no-cells",
    "slice-quality.testing.untested-term",
    "slice-quality.documentation.thin-thesis",
    "slice-quality.documentation.no-docs",
    "slice-quality.translation.integrity-rejected",
    "slice-quality.translation.incomplete",
    "slice-quality.translation.uncovered-literal",
    "slice-quality.translation.uncovered-literal-truncated",
    "slice-quality.translation.notation-excluded",
    "slice-quality.translation.notation-excluded-truncated",
    "slice-quality.flagship.counterexample-structural-only",
    "slice-quality.gmn1-coverage.no-repo-root",
    "slice-quality.gmn1-coverage.no-dictionary",
    "slice-quality.gmn1-coverage.uncovered",
    "slice-quality.gmn-glyph-optimality.no-candidates",
    "slice-quality.gmn-glyph-optimality.incomplete",
    // Documentation-maturity axis codes (doc_maturity.rs).
    "slice-quality.doc-maturity.missing-dimension",
    "slice-quality.doc-maturity.model-unavailable",
    "slice-quality.doc-maturity.slice-untracked",
    // Advice-harvest-coverage axis codes (axes.rs advice_coverage_axis).
    "slice-quality.advice-coverage.unharvested",
    "slice-quality.advice-coverage.no-repo-root",
    "slice-quality.advice-coverage.no-constraint-source",
    // Axis-level advice-template item (report.rs) — the rubric's
    // `gmeow:axisAdviceTemplate` surfaced once per DEFICIENT axis, ranked ahead of
    // that axis's per-term findings, plus the latent-data-gap code minted when a
    // deficient axis carries no template (a rubric authoring gap, never swallowed).
    "slice-quality.axis-advice",
    "slice-quality.axis-advice.missing-template",
    // The lint-gate synthetic finding (`crate::lint::lint_report`) — minted only
    // when the measured roll-up fails to dominate the effective tier bar. Not a
    // scoring code (no axis, no rollup); registered here so it still carries a
    // registered code + help URI through the same json/sarif/html projections
    // every other slice-quality finding does.
    "slice-quality.lint.below-min-tier",
];

/// Seed every slice-quality finding code into the process-wide code registry
/// (idempotent). Called at the start of report construction so the codes are
/// registered before any finding is interned or rendered.
pub fn seed_finding_codes() {
    seed_codes(FINDING_CODES);
}

/// The full result of scoring one slice.
pub struct SliceReport {
    /// The measurement standard the slice was scored against (the floor-free
    /// projection of the rubric — scoring never touches a governance floor).
    pub standard: MeasurementStandard,
    /// The per-axis grade vector + roll-up tier.
    pub assessment: SliceAssessment,
    /// Every advisory finding the axes surfaced, ranked (heaviest axis first).
    pub advisories: Vec<Finding>,
    /// The axis IRI that PRODUCED `advisories[i]` — a stored back-reference, kept
    /// index-parallel through the rank sort.
    ///
    /// This is the ONLY axis-provenance mechanism in the crate: both
    /// [`Self::advisories_for_axis`] and `crate::lint`'s severity grading read this
    /// exact back-reference via [`Self::advisory_axis`] / [`Self::grade_for_axis_iri`].
    /// An earlier `crate::lint::attribute_axis` helper instead guessed the producing
    /// axis with a best-effort textual join on the finding CODE (returning `None`
    /// whenever the code's domain token matched no axis or more than one); it has
    /// been removed in favor of this exact stored reference — a second, guessing
    /// mechanism doing the same job was a GREENFIELD violation.
    advisory_axes: Vec<String>,
    /// The axis IRI paired with each advisory, for weight-ranking and grouping.
    axis_weight: std::collections::HashMap<String, f64>,
    /// Real slice-owned files available as file-level diagnostic anchors.
    /// Paths are portable, forward-slash paths rooted at the supplied slice
    /// directory (or slice-relative for an in-memory file map).
    source_files: SliceSourceFiles,
}

#[derive(Debug, Clone)]
struct SliceSourceFiles {
    manifest: String,
    module: Option<String>,
}

/// The slice-relative key of a slice's manifest — the ONE file that carries the
/// slice's ontology identity, and therefore the one whose absence is a hard error
/// rather than a scored shortfall.
pub const MANIFEST_KEY: &str = "manifest.ttl";
const MODULE_KEY: &str = "module.ttl";

impl SliceSourceFiles {
    fn from_files(files: &BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            manifest: MANIFEST_KEY.to_owned(),
            module: files
                .contains_key(MODULE_KEY)
                .then(|| MODULE_KEY.to_owned()),
        }
    }

    fn prefix_with_slice_dir(&mut self, slice_dir: &Path) {
        self.manifest = normalized_source_path(slice_dir, MANIFEST_KEY);
        self.module = self
            .module
            .as_ref()
            .map(|_| normalized_source_path(slice_dir, MODULE_KEY));
    }
}

fn normalized_source_path(slice_dir: &Path, file: &str) -> String {
    let normalized = slice_dir.join(file).to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_owned()
}

/// Extract the `gmeow:Slice` ontology IRI from already-parsed manifest bytes.
///
/// The SINGLE manifest-IRI extraction implementation: both the on-disk resolver
/// ([`slice_iri_of`]) and the file-map resolver ([`slice_iri_of_files`]) call it, so
/// the two entry points can never diverge on WHICH `gmeow:Slice` individual is the
/// slice's identity (`instances_of` is sorted, so "the first" is deterministic).
/// `source` names the manifest in the diagnostic — a path for the disk caller, the
/// map key for the in-memory caller.
fn slice_iri_from_manifest(manifest_bytes: &[u8], source: &str) -> gmeow_errors::Result<String> {
    let ds = crate::dataset_from_documents(&[(source, manifest_bytes)])?;
    instances_of(&ds, &graph::g("Slice"))
        .into_iter()
        .next()
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Report {
                detail: format!("{source} declares no gmeow:Slice"),
            })
        })
}

/// Discover a slice's ontology IRI from its on-disk `manifest.ttl` (`a gmeow:Slice`).
///
/// `pub(crate)` so [`crate::measure_repo_residues`] (the projection-ceiling seed's
/// and gate's shared residue-measurement helper) resolves the same slice IRI this
/// module's own scoring path does — one resolution authority, never a second
/// re-implementation that could silently diverge on IRI choice.
pub(crate) fn slice_iri_of(slice_dir: &Path) -> gmeow_errors::Result<String> {
    let manifest = slice_dir.join("manifest.ttl");
    let bytes = std::fs::read(&manifest).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("{}: {e}", manifest.display()),
        })
    })?;
    slice_iri_from_manifest(&bytes, &manifest.display().to_string())
}

/// Discover a slice's ontology IRI from an IN-MEMORY file map — the map twin of
/// [`slice_iri_of`], sharing its one [`slice_iri_from_manifest`] extraction core.
///
/// A map carrying no [`MANIFEST_KEY`] is a HARD ERROR, and the message names
/// `manifest.ttl` explicitly: identity is not a scored axis that can degrade to a
/// vacuous default, it is the precondition for scoring anything at all, and a caller
/// that forgot to include the manifest in its map must be told exactly which key is
/// missing rather than handed a mysterious "declares no gmeow:Slice".
///
/// # Errors
/// Hard-fails when the map carries no `manifest.ttl`, when those bytes do not parse
/// as Turtle, or when they declare no `gmeow:Slice`.
pub fn slice_iri_of_files(files: &BTreeMap<String, Vec<u8>>) -> gmeow_errors::Result<String> {
    let bytes = files.get(MANIFEST_KEY).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::Report {
            detail: format!(
                "the slice file map carries no {MANIFEST_KEY} — a slice's ontology identity is \
                 read from its {MANIFEST_KEY}, so it is a required entry, never an optional one"
            ),
        })
    })?;
    slice_iri_from_manifest(bytes, MANIFEST_KEY)
}

/// Read a slice directory into the in-memory file map the scorer consumes: every
/// regular file under `slice_dir`, keyed by its slice-relative FORWARD-SLASH path.
///
/// The sweep is the WHOLE subtree rather than a hand-listed set of directories, for
/// two reasons. First, it is trivially a superset of every path any axis asks for
/// (`manifest.ttl`, `module.ttl`, `shapes.ttl`, `docs.md`, `examples/`, `tests/`,
/// `queries/`, `i18n/`, `mappings/`) — a new axis reading a new file cannot silently
/// score against an absent entry because the map author forgot to widen a list.
/// Second, `DocMaturity`'s external arm rebuilds a real slice tree from this map and
/// hands it to `purrdf`'s artifact discovery, which itself walks the entire subtree:
/// anything short of the whole tree would make an off-repo documentation model differ
/// from the on-disk one it is supposed to reproduce.
///
/// The sweep is guarded by a `manifest.ttl` precondition: a slice directory is BY
/// DEFINITION one that carries a manifest ([`crate::discover_slice_dirs`] uses the
/// same test), and checking it before recursing is what stops a caller that pointed
/// at an arbitrary directory (`/etc`, a home directory, a whole checkout) from having
/// its entire subtree slurped into memory before failing on identity anyway. It is a
/// presence test, not a second manifest parser — [`slice_iri_of_files`] remains the
/// one identity authority.
///
/// # Errors
/// Hard-fails if `slice_dir` carries no `manifest.ttl`, if the directory cannot be
/// walked, or if any file cannot be read — a slice that is present but unreadable is
/// a broken input, never an empty map that would score as a clean, contentless slice.
pub fn slice_files_from_dir(slice_dir: &Path) -> gmeow_errors::Result<BTreeMap<String, Vec<u8>>> {
    fn walk(
        root: &Path,
        dir: &Path,
        out: &mut BTreeMap<String, Vec<u8>>,
    ) -> gmeow_errors::Result<()> {
        let entries = std::fs::read_dir(dir).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                detail: format!("{}: {e}", dir.display()),
            })
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Io {
                    detail: format!("{}: {e}", dir.display()),
                })
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Io {
                    detail: format!("{}: {e}", path.display()),
                })
            })?;
            if file_type.is_dir() {
                walk(root, &path, out)?;
            } else if file_type.is_file() {
                let rel = path.strip_prefix(root).map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Io {
                        detail: format!("{}: {e}", path.display()),
                    })
                })?;
                let key = rel.to_string_lossy().replace('\\', "/");
                let bytes = std::fs::read(&path).map_err(|e| {
                    gmeow_errors::Diag::of_kind(crate::error::Io {
                        detail: format!("{}: {e}", path.display()),
                    })
                })?;
                out.insert(key, bytes);
            }
        }
        Ok(())
    }
    if !slice_dir.join(MANIFEST_KEY).is_file() {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Report {
            detail: format!(
                "{} carries no {MANIFEST_KEY} — it is not a slice directory",
                slice_dir.display()
            ),
        }));
    }
    let mut out = BTreeMap::new();
    walk(slice_dir, slice_dir, &mut out)?;
    Ok(out)
}

/// Every `.ttl` under `slice_dir`'s `module.ttl`, `examples/`, and `tests/` —
/// deterministic (sorted), existing files only. The SINGLE authority for the PATH
/// LIST of a slice's own Turtle: the pipeline's cache-input enumeration and the
/// `slice-brief` loader both need the file paths themselves (a cache key is a path
/// set, not a byte map), so this stays path-shaped. The scoring kernel reads the same
/// file set out of the in-memory map via [`slice_ttl_documents`].
pub fn slice_ttl_paths(slice_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![slice_dir.join("module.ttl")];
    for sub in ["examples", "tests"] {
        collect_ttl(&slice_dir.join(sub), &mut paths);
    }
    paths.retain(|p| p.is_file());
    paths.sort();
    paths
}

fn collect_ttl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_ttl(&p, out);
        } else if p.extension().is_some_and(|x| x == "ttl") {
            out.push(p);
        }
    }
}

/// The `(key, bytes)` documents that make up a slice's own graph — `module.ttl` plus
/// every `.ttl` under `examples/` and `tests/`, in BTreeMap key order.
///
/// The map twin of [`slice_ttl_paths`], and the SINGLE authority the scoring kernel
/// assembles a slice graph through. Key order reproduces the sorted-path order the
/// path-based twin returns (`examples/…` < `module.ttl` < `tests/…` under both a
/// component-wise `Path` sort and a byte-wise string sort), so the union order — and
/// therefore the frozen dataset's term-interning order — is unchanged.
#[must_use]
pub fn slice_ttl_documents(files: &BTreeMap<String, Vec<u8>>) -> Vec<(&str, &[u8])> {
    files
        .iter()
        .filter(|(key, _)| {
            key.as_str() == "module.ttl"
                || ((key.starts_with("examples/") || key.starts_with("tests/"))
                    && key.ends_with(".ttl"))
        })
        .map(|(key, bytes)| (key.as_str(), bytes.as_slice()))
        .collect()
}

/// Score the slice carried by `files` against an already-loaded measurement standard
/// (the floor-free projection of the rubric; the sweep path reuses one) in the given
/// scoring environment.
///
/// This is THE scoring implementation — every other entry point (the on-disk
/// [`score_slice_with_standard`], the bundle-facing
/// [`crate::score_external_slice_files`] and its directory twins) builds a file map
/// and delegates here, so there is exactly one place a slice's grade is computed.
/// `files` carries the slice's own bytes keyed by slice-relative forward-slash path;
/// nothing here touches a filesystem, which is what lets the whole kernel run where
/// there is none.
///
/// Every rubric axis binds a measurement primitive; an axis whose producer the
/// kernel does not implement is a hard error (never a silent skip). `env` decides
/// where the two repo-anchored axes (`gmn1_coverage`, `DocMaturity`) source their
/// wide-scope inputs: [`ScoringEnv::Repo`] reads the surrounding checkout (the
/// in-repo sweep/CLI/MCP path), [`ScoringEnv::Bundle`] carries them in an embedded
/// wheel (the consumer path — no repo around the slice).
///
/// # Errors
/// Returns a message if the map carries no `manifest.ttl`, if the slice graph cannot
/// be parsed, or if the rubric names a producer with no implemented primitive.
pub fn score_slice_files_with_standard(
    files: &BTreeMap<String, Vec<u8>>,
    standard: &MeasurementStandard,
    env: ScoringEnv,
) -> gmeow_errors::Result<SliceReport> {
    let slice_iri = slice_iri_of_files(files)?;
    let source_files = SliceSourceFiles::from_files(files);
    let ds = crate::dataset_from_documents(&slice_ttl_documents(files))?;
    // The scoring environment decides where the two repo-anchored axes source their
    // wide-scope inputs; every in-repo caller passes `ScoringEnv::Repo` (byte-identical
    // to the pre-seam behaviour), the consumer wheel passes `ScoringEnv::Bundle`.
    let ctx = ScoreContext::new(slice_iri.clone(), files, &ds, env);
    score_with_context(&ctx, &slice_iri, standard, source_files)
}

/// Score the slice at `slice_dir` — the on-disk convenience over
/// [`score_slice_files_with_standard`]: read the directory into a file map
/// ([`slice_files_from_dir`]) and delegate, so the disk path and the in-memory path
/// are the same computation over the same bytes.
///
/// `slice_dir` supplies the slice's CONTENT; when `env` is [`ScoringEnv::Repo`] its
/// own `slice_dir` supplies the CHECKOUT ANCHOR the repo-scoped axis arms walk up
/// from. In-repo callers pass the same directory for both — they are the same slice —
/// but the two roles are genuinely distinct, and only the second one is a repo fact.
///
/// # Errors
/// As [`score_slice_files_with_standard`], plus a message if the directory cannot be
/// read.
pub fn score_slice_with_standard(
    slice_dir: &Path,
    standard: &MeasurementStandard,
    env: ScoringEnv,
) -> gmeow_errors::Result<SliceReport> {
    let files = slice_files_from_dir(slice_dir)?;
    let mut report = score_slice_files_with_standard(&files, standard, env)?;
    report.source_files.prefix_with_slice_dir(slice_dir);
    Ok(report)
}

/// Run every axis in `standard` over `ctx` and assemble the ranked report.
fn score_with_context(
    ctx: &ScoreContext,
    slice_iri: &str,
    standard: &MeasurementStandard,
    source_files: SliceSourceFiles,
) -> gmeow_errors::Result<SliceReport> {
    let mut scores: Vec<(&Axis, f64)> = Vec::with_capacity(standard.axes.len());
    // Each entry is (axis_iri, axis_weight, advice_kind, finding). `advice_kind`
    // ranks an axis-level template item ahead of that axis's per-term findings.
    let mut advisories: Vec<(String, f64, u8, Finding)> = Vec::new();
    let mut axis_weight = std::collections::HashMap::new();
    for axis in &standard.axes {
        axis_weight.insert(axis.iri.clone(), axis.weight);
        let primitive = axes::resolve(&axis.producer).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Report {
                detail: format!(
                    "rubric axis {} names producer '{}' with no implemented primitive (hard fail)",
                    axis.iri, axis.producer
                ),
            })
        })?;
        let result = primitive(ctx);
        for f in result.findings {
            advisories.push((axis.iri.clone(), axis.weight, ADVICE_KIND_INSTANCE, f));
        }
        scores.push((axis, result.score.clamp(0.0, 1.0)));
    }

    // Surface the rubric's per-axis uplift advice (`gmeow:axisAdviceTemplate`) as an
    // axis-level advice item for every DEFICIENT axis. "Deficient" = an axis that
    // surfaced >= 1 per-term advisory (NOT "below target tier":
    // axisFlagshipCounterExampleDepth sits at Maximal with all-0.0 floors yet still
    // advises). The template item is ranked strictly ahead of that axis's per-term
    // findings via `ADVICE_KIND_TEMPLATE`; the loader already read the template into
    // `Axis.advice`, but until now nothing emitted it (dead rubric information).
    let deficient: std::collections::BTreeSet<&str> = advisories
        .iter()
        .map(|(axis_iri, _, _, _)| axis_iri.as_str())
        .collect();
    let mut templates: Vec<(String, f64, u8, Finding)> = Vec::new();
    for axis in &standard.axes {
        if !deficient.contains(axis.iri.as_str()) {
            continue;
        }
        let local = axis.iri.rsplit(['/', '#']).next().unwrap_or(&axis.iri);
        let finding = if axis.advice.trim().is_empty() {
            // A deficient axis with an empty template is a latent rubric data gap.
            // Do not silently swallow it (hard-fail discipline): surface it as its
            // own visible advisory so the missing remediation text is caught and
            // authored, rather than emitting a blank advice line.
            advisory(
                "slice-quality.axis-advice.missing-template",
                format!(
                    "{local}: axis is deficient but carries no gmeow:axisAdviceTemplate — a latent rubric data gap; author its uplift advice."
                ),
            )
        } else {
            advisory(
                "slice-quality.axis-advice",
                format!("{local}: {}", axis.advice),
            )
        };
        templates.push((axis.iri.clone(), axis.weight, ADVICE_KIND_TEMPLATE, finding));
    }
    advisories.extend(templates);

    let assessment = lattice::assess(slice_iri, &scores, standard);

    // Rank advice: heaviest axis first, then group all advisories for the same axis
    // together (axis IRI tiebreak — otherwise two same-weight axes interleave and a
    // template can land ahead of a *different* axis's per-term findings), then within
    // an axis the axis-level template item ahead of that axis's per-term findings
    // (`advice_kind`), then finding code, then message — a deterministic total order
    // (no derived Ord over the float; explicit key).
    advisories.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.code.cmp(&b.3.code))
            .then_with(|| a.3.message.cmp(&b.3.message))
    });
    // Split the ranked pairs into the two index-parallel vectors AFTER the sort, so
    // `advisory_axes[i]` is the producing axis of `advisories[i]` by construction.
    let (advisory_axes, advisories): (Vec<String>, Vec<Finding>) = advisories
        .into_iter()
        .map(|(axis_iri, _, _, f)| (axis_iri, f))
        .unzip();

    Ok(SliceReport {
        standard: standard.clone(),
        assessment,
        advisories,
        advisory_axes,
        axis_weight,
        source_files,
    })
}

#[cfg(test)]
impl SliceReport {
    /// Test-only constructor: assemble a [`SliceReport`] from already-computed
    /// parts, bypassing [`score_slice_with_standard`]'s scoring pass entirely.
    /// Used by `crate::lint`'s unit tests to build synthetic reports (a
    /// declared tier ratchet, a graded advisory, a degenerate empty-grade
    /// slice, …) without a real slice directory or rubric dataset.
    /// `advisory_axes` must be index-parallel to `advisories`; a caller that does
    /// not care about axis provenance passes an empty vector and gets none.
    pub(crate) fn for_test(
        standard: MeasurementStandard,
        assessment: SliceAssessment,
        advisories: Vec<Finding>,
        advisory_axes: Vec<String>,
        axis_weight: std::collections::HashMap<String, f64>,
    ) -> Self {
        assert!(
            advisory_axes.is_empty() || advisory_axes.len() == advisories.len(),
            "advisory_axes must be index-parallel to advisories (or empty)"
        );
        Self {
            standard,
            assessment,
            advisories,
            advisory_axes,
            axis_weight,
            source_files: SliceSourceFiles {
                manifest: MANIFEST_KEY.to_owned(),
                module: Some(MODULE_KEY.to_owned()),
            },
        }
    }

    pub(crate) fn remove_module_source_for_test(&mut self) {
        self.source_files.module = None;
    }
}

impl SliceReport {
    /// The real slice-owned file that can honestly anchor a lint finding when no
    /// parser span exists. Term-specific findings belong to `module.ttl` when the
    /// slice carries it; slice-level policy and tier findings belong to the
    /// manifest that declares the slice identity.
    pub(crate) fn lint_source_path(&self, finding: &Finding) -> &str {
        if !finding.documented_terms.is_empty()
            && let Some(module) = self.source_files.module.as_deref()
        {
            return module;
        }
        &self.source_files.manifest
    }

    /// The advisories PRODUCED BY one axis, in report order.
    ///
    /// Reads the stored `advisory_axes` back-reference, so the answer is exact: it
    /// never guesses from the finding code the way the severity-grading decoration in
    /// `crate::lint` has to.
    #[must_use]
    pub fn advisories_for_axis(&self, axis_iri: &str) -> Vec<&Finding> {
        self.advisory_axes
            .iter()
            .zip(&self.advisories)
            .filter(|(produced_by, _)| produced_by.as_str() == axis_iri)
            .map(|(_, finding)| finding)
            .collect()
    }

    /// The axis IRI that produced `self.advisories[idx]`, read exactly from the
    /// stored [`Self`]`::advisory_axes` back-reference — never a guess from the
    /// finding's code. `None` when `idx` is out of range, or when this report
    /// carries no axis provenance at all (the `#[cfg(test)]`-only `Self::for_test`
    /// constructor — absent from this documentation because it is compiled out of a
    /// non-test build — accepts an empty `advisory_axes` for callers that do not need
    /// axis attribution).
    #[must_use]
    pub fn advisory_axis(&self, idx: usize) -> Option<&str> {
        self.advisory_axes.get(idx).map(String::as_str)
    }

    /// The already-computed grade for a given axis IRI — an exact lookup against
    /// `self.assessment.grades`, never a textual guess.
    #[must_use]
    pub fn grade_for_axis_iri(&self, axis_iri: &str) -> Option<&AxisGrade> {
        self.assessment
            .grades
            .iter()
            .find(|g| g.axis_iri == axis_iri)
    }

    /// The roll-up tier label.
    #[must_use]
    pub fn rollup_label(&self) -> &str {
        &self.assessment.rollup.label
    }

    /// The per-axis grades sorted by the weakest tier first (the uplift order).
    #[must_use]
    pub fn grades_weakest_first(&self) -> Vec<&AxisGrade> {
        let mut v: Vec<&AxisGrade> = self.assessment.grades.iter().collect();
        v.sort_by(|a, b| {
            a.tier
                .rank
                .cmp(&b.tier.rank)
                .then_with(|| {
                    let wa = self.axis_weight.get(&a.axis_iri).copied().unwrap_or(0.0);
                    let wb = self.axis_weight.get(&b.axis_iri).copied().unwrap_or(0.0);
                    wb.partial_cmp(&wa).unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.axis_iri.cmp(&b.axis_iri))
        });
        v
    }

    /// Build the advisory [`Report`] on the diagnostics substrate: every axis grade
    /// as an informational note, every uplift item as an `Advisory` warning.
    #[must_use]
    pub fn to_report(&self) -> Report {
        // Register every slice-quality code before any finding is built, so the
        // emitted report carries registered (never bare) diagnostic codes.
        seed_finding_codes();
        let mut report = Report::new("slice-quality");
        // Per-axis grade notes (never gating).
        for grade in self.grades_weakest_first() {
            let local = grade
                .axis_iri
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(&grade.axis_iri);
            report.add_finding(
                Finding::new(
                    Severity::Info,
                    "slice-quality.grade",
                    format!("{local}: {} (score {:.2})", grade.tier.label, grade.score),
                )
                .with_tool("slice-quality")
                .with_standpoint(Standpoint::Advisory),
            );
        }
        // Roll-up.
        report.add_finding(
            Finding::new(
                Severity::Info,
                "slice-quality.rollup",
                format!(
                    "roll-up tier: {} ({})",
                    self.assessment.rollup.label, self.assessment.slice
                ),
            )
            .with_tool("slice-quality")
            .with_standpoint(Standpoint::Advisory),
        );
        // Ranked uplift advisories.
        for f in &self.advisories {
            report.add_finding(f.clone());
        }

        // Attach a rule descriptor for every distinct emitted code, each carrying a
        // help URI into the generated constraint catalog (`help_uri_for` — the SAME
        // anchor transform the pipeline's constraint-catalog stage and `gmeow
        // validate` stamp onto rule help URIs, never a hand-rolled base). The
        // renderers join a finding to its rule by code, so every finding surfaces a
        // registered code + help URI in the json/sarif/html projections.
        let mut severities: std::collections::BTreeMap<String, Severity> =
            std::collections::BTreeMap::new();
        for finding in &report.findings {
            severities
                .entry(finding.code.clone())
                .or_insert(finding.severity);
        }
        for (code, severity) in severities {
            let mut rule = Rule::new(code.clone(), severity);
            rule.help_uri = Some(help_uri_for(&code));
            report.add_rule(rule);
        }
        report
    }

    /// A deterministic human-facing text rendering.
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "slice-quality: {}\n  roll-up tier: {}\n",
            self.assessment.slice, self.assessment.rollup.label
        ));
        out.push_str("  per-axis grades (weakest first):\n");
        for grade in self.grades_weakest_first() {
            let local = grade
                .axis_iri
                .rsplit(['/', '#'])
                .next()
                .unwrap_or(&grade.axis_iri);
            out.push_str(&format!(
                "    {local}: {} ({:.2})\n",
                grade.tier.label, grade.score
            ));
        }
        if self.advisories.is_empty() {
            out.push_str("  no uplift advice — the slice meets every axis.\n");
        } else {
            out.push_str(&format!(
                "  ranked uplift advice ({}):\n",
                self.advisories.len()
            ));
            for (i, f) in self.advisories.iter().enumerate() {
                out.push_str(&format!("    {}. [{}] {}\n", i + 1, f.code, f.message));
            }
        }
        out
    }
}

// -----------------------------------------------------------------------------
// RDF projection of a slice assessment.
// -----------------------------------------------------------------------------

/// The named graph the slice-quality assessment projection lives in.
pub(crate) const SLICE_QUALITY_GRAPH: &str =
    "https://blackcatinformatics.ca/gmeow/graph/slice-quality";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// The computational-model observation method every advisor-produced grade carries
/// (the scorer is a deterministic Rust primitive, not expert judgement).
const METHOD_COMPUTATIONAL_MODEL: &str =
    "https://blackcatinformatics.ca/gmeow/methodComputationalModel";
impl SliceReport {
    /// Project this slice assessment into the `gmeow:` RDF vocabulary as
    /// deterministic N-Quads, all in the `gmeow:graph/slice-quality` named graph.
    ///
    /// The assessment IS a bundle of quality observations (per the worked example
    /// `slices/core/slice-quality-rubric/examples/rubric-assessment.ttl`): each
    /// per-axis grade becomes a `gmeow:QualityAssessment` (a `gmeow:Observation`
    /// subclass) whose `gmeow:assessedEntity` is the slice IRI, whose
    /// `gmeow:qualityDimension` is the axis's emitted dimension, whose
    /// `gmeow:observationMethod` is `gmeow:methodComputationalModel`, and whose two
    /// coexisting `gmeow:observationResult`s are (a) a `math:Quantity` wrapping
    /// the normalized score (`math:quantityValue` + `math:dimensionless`) and (b)
    /// the categorical `gmeow:QualityTier` the score earned. The roll-up tier is one
    /// more top-level `gmeow:QualityAssessment` whose sole result is the meet tier —
    /// dimension-spanning, so it carries no `gmeow:qualityDimension`.
    ///
    /// This mirrors the discipline of `gmeow-docs`/`gmeow-errors` `to_gmeow_rdf`:
    /// N-Quads (no TriG/prefix handling), `nq_escape`d literals, IRIs (never blank
    /// nodes) minted deterministically from the slice + axis local names (never a
    /// counter), sorted iteration, every generated A-Box subject stamped with
    /// `rdfs:label` / `skos:definition` / `rdfs:isDefinedBy` / `gmeow:graphBoxRole
    /// gmeow:boxABox`, and a trailing newline.
    #[must_use]
    pub fn to_gmeow_rdf(&self) -> String {
        let graph = format!("<{SLICE_QUALITY_GRAPH}>");
        let mut lines: Vec<String> = Vec::new();

        let triple = |s: &str, p: &str, o: &str, lines: &mut Vec<String>| {
            lines.push(format!("{s} <{p}> {o} {graph} ."));
        };
        let literal = |value: &str| format!("\"{}\"", nq_escape(value));

        // Every projected subject is generated A-Box instance data: stamp it with a
        // human label, a definition-equivalent, its named-graph provenance anchor, and
        // the assertional `gmeow:boxABox` role so the folded bundle satisfies the
        // assertional-tier validation contract (mirrors docs/errors emitters).
        let role_object = format!("<{}boxABox>", crate::model::GMEOW);
        let isdefinedby_object = format!("<{SLICE_QUALITY_GRAPH}>");
        let annotate = |subject: &str, label: &str, definition: &str, lines: &mut Vec<String>| {
            triple(subject, RDFS_LABEL, &literal(label), lines);
            triple(subject, SKOS_DEFINITION, &literal(definition), lines);
            triple(subject, RDFS_IS_DEFINED_BY, &isdefinedby_object, lines);
            triple(
                subject,
                &format!("{}graphBoxRole", crate::model::GMEOW),
                &role_object,
                lines,
            );
        };

        let slice_iri = &self.assessment.slice;
        let slice_slug = local_slug(slice_iri);
        let assessed_object = format!("<{}>", nq_iri(slice_iri));

        // Map each axis IRI to its emitted quality dimension (the grade vector carries
        // only the axis; the dimension binding lives on the rubric axis).
        let dim_of: std::collections::HashMap<&str, &str> = self
            .standard
            .axes
            .iter()
            .map(|a| (a.iri.as_str(), a.dimension_iri.as_str()))
            .collect();

        // Per-axis grades, emitted in a deterministic axis-IRI order.
        let mut grades: Vec<&AxisGrade> = self.assessment.grades.iter().collect();
        grades.sort_by(|a, b| a.axis_iri.cmp(&b.axis_iri));
        for grade in grades {
            let axis_local = local_slug(&grade.axis_iri);
            let assessment_iri = format!(
                "{}slice-quality/assessment/{slice_slug}/{axis_local}",
                crate::model::GMEOW
            );
            let score_iri = format!(
                "{}slice-quality/score/{slice_slug}/{axis_local}",
                crate::model::GMEOW
            );
            let assessment_subject = format!("<{}>", nq_iri(&assessment_iri));
            let score_subject = format!("<{}>", nq_iri(&score_iri));

            triple(
                &assessment_subject,
                RDF_TYPE,
                &format!("<{}QualityAssessment>", crate::model::GMEOW),
                &mut lines,
            );
            triple(
                &assessment_subject,
                &format!("{}assessedEntity", crate::model::GMEOW),
                &assessed_object,
                &mut lines,
            );
            // The AXIS this grade measured — measurement identity, carried as a
            // first-class predicate so a reader recovers the grade vector exactly
            // (see `crate::read`). Neither of the two things that look like they
            // could stand in for it actually can: `gmeow:qualityDimension` is a
            // many-to-one projection (sixteen axes onto twelve dimensions), and the
            // minted subject IRI's `local_slug` lowercases and collapses runs, so it
            // is a display convention rather than an assertion. Emitted for per-axis
            // grades only; the roll-up spans every axis and names none.
            triple(
                &assessment_subject,
                &format!("{}assessmentAxis", crate::model::GMEOW),
                &format!("<{}>", nq_iri(&grade.axis_iri)),
                &mut lines,
            );
            // The dimension the grade is emitted under — a hard requirement: an axis
            // with no bound dimension is a rubric authoring error, never silently
            // dropped (the projection then carries a visibly under-specified grade).
            let dimension = dim_of.get(grade.axis_iri.as_str()).copied().unwrap_or("");
            if !dimension.is_empty() {
                triple(
                    &assessment_subject,
                    &format!("{}qualityDimension", crate::model::GMEOW),
                    &format!("<{}>", nq_iri(dimension)),
                    &mut lines,
                );
            }
            triple(
                &assessment_subject,
                &format!("{}observationMethod", crate::model::GMEOW),
                &format!("<{METHOD_COMPUTATIONAL_MODEL}>"),
                &mut lines,
            );
            // Result 1: the normalized score, wrapped in a math:Quantity (the range of
            // observationResult is logic:Individual — a bare literal is forbidden here).
            triple(
                &assessment_subject,
                &format!("{}observationResult", crate::model::GMEOW),
                &score_subject,
                &mut lines,
            );
            // Result 2: the categorical tier the score earned (observationResult is
            // non-functional; a scalar reading and a categorical verdict coexist).
            triple(
                &assessment_subject,
                &format!("{}observationResult", crate::model::GMEOW),
                &format!("<{}>", nq_iri(&grade.tier.iri)),
                &mut lines,
            );
            annotate(
                &assessment_subject,
                &format!("Quality grade: {axis_local} on {slice_iri}"),
                &format!(
                    "Slice-quality assessment of {slice_iri} on axis {} — measured {} (score {}).",
                    grade.axis_iri,
                    grade.tier.label,
                    fmt_score(grade.score)
                ),
                &mut lines,
            );

            // The score math:Quantity: value + dimensionless dimension. A normalized
            // ratio has no unit witness and therefore needs no measurement frame.
            triple(
                &score_subject,
                RDF_TYPE,
                &format!("<{}Quantity>", crate::model::MATH),
                &mut lines,
            );
            triple(
                &score_subject,
                &format!("{}quantityValue", crate::model::MATH),
                &format!("\"{}\"^^<{XSD_DECIMAL}>", exact_score(grade.score)),
                &mut lines,
            );
            triple(
                &score_subject,
                &format!("{}hasDimension", crate::model::MATH),
                &format!("<{}dimensionless>", crate::model::MATH),
                &mut lines,
            );
            annotate(
                &score_subject,
                &format!("Quality score: {axis_local} on {slice_iri}"),
                &format!(
                    "Normalized slice-quality score {} for axis {} of {slice_iri}.",
                    fmt_score(grade.score),
                    grade.axis_iri
                ),
                &mut lines,
            );
        }

        // The roll-up: one top-level QualityAssessment whose sole result is the meet
        // tier. It spans every dimension (the unweighted lattice meet of the axis
        // grades), so it carries NO gmeow:qualityDimension — it is not a value along
        // one axis but the combined verdict.
        let rollup_iri = format!(
            "{}slice-quality/assessment/{slice_slug}/rollup",
            crate::model::GMEOW
        );
        let rollup_subject = format!("<{}>", nq_iri(&rollup_iri));
        triple(
            &rollup_subject,
            RDF_TYPE,
            &format!("<{}QualityAssessment>", crate::model::GMEOW),
            &mut lines,
        );
        triple(
            &rollup_subject,
            &format!("{}assessedEntity", crate::model::GMEOW),
            &assessed_object,
            &mut lines,
        );
        triple(
            &rollup_subject,
            &format!("{}observationMethod", crate::model::GMEOW),
            &format!("<{METHOD_COMPUTATIONAL_MODEL}>"),
            &mut lines,
        );
        triple(
            &rollup_subject,
            &format!("{}observationResult", crate::model::GMEOW),
            &format!("<{}>", nq_iri(&self.assessment.rollup.iri)),
            &mut lines,
        );
        annotate(
            &rollup_subject,
            &format!("Roll-up quality tier on {slice_iri}"),
            &format!(
                "Roll-up slice-quality tier {} for {slice_iri} — the unweighted lattice meet of every per-axis grade.",
                self.assessment.rollup.label
            ),
            &mut lines,
        );

        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

/// The `gmeow:versionFingerprint` predicate (core `versions` slice) the corpus
/// carries — the semantic content fingerprint of the scored source set, NOT a
/// byte-exact digest of any one file (which is what `gmeow:contentDigest` is for).
pub(crate) const VERSION_FINGERPRINT: &str =
    "https://blackcatinformatics.ca/gmeow/versionFingerprint";

/// The `gmeow:contentDigest` predicate (core `sources` slice — domain-free, "a content
/// hash of an object's bytes … the reliable identity by content") the corpus carries
/// over ITS OWN recorded content.
///
/// [`VERSION_FINGERPRINT`] and this are answers to two different questions and neither
/// substitutes for the other: the fingerprint says WHICH SOURCES were scored, this says
/// WHAT WAS RECORDED. A record whose fingerprint still matches the tree but whose grades
/// have been edited by hand satisfies the first and violates the second.
pub(crate) const CONTENT_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/contentDigest";

/// The canonical digest of the corpus's OWN recorded content: every scored slice, its
/// per-axis (axis, exact score, tier) vector, and its roll-up tier.
///
/// This is a digest of the RECONSTRUCTION, not of the bytes — it folds exactly the
/// grade vector [`crate::read::read_recorded_corpus`] rebuilds from the record, in a
/// canonical (slice-IRI, then axis-IRI) order. Two consequences follow, and both are
/// the point:
///
/// * It is serialization-independent. The emitter produces graph-labelled N-Quads and
///   the fanout writes an unlabelled `.nt`; a byte digest would have to be recomputed
///   across that re-serialization and would be fragile to every escaping or ordering
///   detail. The reconstruction is identical across both, so producer and consumer
///   compute the same value from different bytes.
/// * It binds precisely the facts a reader acts on. Deleting a slice's assessment,
///   raising one axis's score, or swapping a tier all change it; a comment or a label
///   rewording does not, because no consumer grades on those.
///
/// The score is folded through [`exact_score`] — the same shortest round-tripping
/// lexical the projection emits — so the value the reader reparses folds identically
/// to the value the scorer produced, with no float-formatting drift.
#[must_use]
pub fn corpus_content_digest<'a>(
    assessments: impl IntoIterator<Item = &'a SliceAssessment>,
) -> String {
    // Canonical order, independent of the order the sweep produced or the reader
    // rebuilt: slice IRI, then axis IRI within a slice.
    let by_slice: BTreeMap<&str, &SliceAssessment> = assessments
        .into_iter()
        .map(|a| (a.slice.as_str(), a))
        .collect();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"gmeow-slice-quality-record-v1\x1e");
    for (slice, assessment) in by_slice {
        hasher.update(slice.as_bytes());
        hasher.update(b"\x1f");
        let mut grades: Vec<&AxisGrade> = assessment.grades.iter().collect();
        grades.sort_by(|a, b| a.axis_iri.cmp(&b.axis_iri));
        for grade in grades {
            hasher.update(grade.axis_iri.as_bytes());
            hasher.update(b"\x1f");
            hasher.update(exact_score(grade.score).as_bytes());
            hasher.update(b"\x1f");
            hasher.update(grade.tier.iri.as_bytes());
            hasher.update(b"\x1f");
        }
        hasher.update(b"rollup\x1f");
        hasher.update(assessment.rollup.iri.as_bytes());
        hasher.update(b"\x1e");
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

/// Project the corpus-level freshness witness: one `gmeow:versionFingerprint` on the
/// assessment graph itself, recording the canonicalized digest of every authored file
/// the sweep scored ([`crate::scored_input_fingerprint`]), and one `gmeow:contentDigest`
/// over the corpus's own recorded grades ([`corpus_content_digest`]).
///
/// This is what lets a CONSUMER of the recorded corpus prove the record still
/// describes the working tree instead of trusting that something regenerated it. The
/// gate recomputes the digest and hard-fails on absence or mismatch, so a stale
/// `generated/quality/gmeow.quality-assessment.nt` can never be read as if it were
/// current — the recomputation the gate no longer performs is replaced by a proof
/// that the recomputation is unnecessary, never by an assumption that it is.
///
/// The input fingerprint alone does not carry that proof, and this is exactly the hole
/// the content digest closes: it attests the INPUTS, so hand-editing the record (raising
/// a score, deleting a grade) leaves it satisfied. The `gmeow:contentDigest` attests the
/// RECORD, so the reader refuses an edited one.
///
/// Emitted exactly ONCE per corpus (by [`crate::assessment_artifacts`] and by the dev
/// CLI's RDF sweep), never per slice: it is a property of the whole scored source set.
#[must_use]
pub fn corpus_fingerprint_nquads(fingerprint: &str, record_digest: &str) -> String {
    let graph = format!("<{SLICE_QUALITY_GRAPH}>");
    let subject = format!("<{}>", nq_iri(SLICE_QUALITY_GRAPH));
    let triple = |p: &str, o: &str| format!("{subject} <{p}> {o} {graph} .\n");
    let literal = |value: &str| format!("\"{}\"", nq_escape(value));
    let mut out = String::new();
    // The corpus is a TYPED object, not a bag of properties on a graph IRI: the two
    // witnesses below are mandatory of a `gmeow:QualityAssessmentCorpus`
    // (`gmeow:QualityAssessmentCorpusWitnessedConstraint` in the slice-quality-rubric
    // slice, raising `gmeow:UnreadableQualityRecord`), and an untyped node is outside
    // that constraint's reach — which would leave the obligation stated only in Rust.
    out.push_str(&triple(
        RDF_TYPE,
        &format!("<{}QualityAssessmentCorpus>", crate::model::GMEOW),
    ));
    out.push_str(&triple(VERSION_FINGERPRINT, &literal(fingerprint)));
    out.push_str(&triple(CONTENT_DIGEST, &literal(record_digest)));
    out.push_str(&triple(
        RDFS_LABEL,
        &literal("Slice-quality assessment corpus"),
    ));
    out.push_str(&triple(
        SKOS_DEFINITION,
        &literal(
            "The slice-quality assessment corpus — every discovered slice scored against the \
             ontology-resident rubric — fingerprinted by the canonicalized digest of every \
             authored source file the sweep read, so a consumer can prove the record still \
             describes the working tree.",
        ),
    ));
    out.push_str(&triple(RDFS_IS_DEFINED_BY, &graph));
    out.push_str(&triple(
        &format!("{}graphBoxRole", crate::model::GMEOW),
        &format!("<{}boxABox>", crate::model::GMEOW),
    ));
    out
}

/// Format a normalized `[0,1]` score for HUMAN prose — the `rdfs:label` /
/// `skos:definition` sentences a reader sees — at a fixed six decimal places.
///
/// This is a DISPLAY form and must never be the machine-read value: it rounds, and
/// the per-axis floor gate compares at `f64::EPSILON` tolerance, so a score below a
/// committed floor by less than 5e-7 would round UP through the floor and flip a
/// FAIL into a PASS. The value a consumer reads back is [`exact_score`].
fn fmt_score(score: f64) -> String {
    format!("{score:.6}")
}

/// Format a normalized `[0,1]` score as the LOSSLESS machine-read `xsd:decimal`
/// lexical form: Rust's shortest round-tripping `f64` rendering, which parses back
/// to a bit-identical `f64`.
///
/// Two properties matter and both hold for a `[0,1]` normalized score. It round-trips
/// EXACTLY — `Display` for `f64` emits the shortest digit string that reparses to the
/// same value — which is what lets `crate::read` recover the grade vector the scorer
/// produced instead of a rounded shadow of it. And it is plain decimal, never
/// scientific notation (Rust's `f64` `Display` does not emit exponents), so it is a
/// legal `xsd:decimal`; `0.0` renders `0`, `1.0` renders `1`, both legal and exact.
///
/// A non-finite score would render `NaN`/`inf`, which is NOT a legal `xsd:decimal` —
/// so it is rejected here rather than emitted. Scores are clamped to `[0,1]` by
/// `crate::lattice::grade_axis` before ever reaching a grade, so this is an
/// unreachable-state assertion guarding the projection's datatype claim, not a
/// runtime condition.
///
/// # Panics
///
/// If `score` is not finite.
fn exact_score(score: f64) -> String {
    assert!(
        score.is_finite(),
        "a quality score must be a finite normalized [0,1] value to project as \
         xsd:decimal, got {score}"
    );
    format!("{score}")
}

/// The local name of an IRI (the tail after the last `/` or `#`), slugified to
/// `[a-z0-9-]` for use inside a minted subject IRI. Deterministic and stable.
fn local_slug(iri: &str) -> String {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    let name = &iri[cut..];
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Escape an IRI for an N-Quads IRIREF (the characters the grammar forbids raw:
/// controls, spaces, and the delimiter set `<>"{}|^`` ` and backslash).
fn nq_iri(iri: &str) -> String {
    let mut out = String::with_capacity(iri.len());
    for ch in iri.chars() {
        match ch {
            '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\' => {
                out.push_str(&format!("\\u{:04X}", ch as u32));
            }
            c if (c as u32) <= 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Escape a string literal for N-Triples/N-Quads (mirrors `gmeow_docs_model::rdf::nq_escape`).
fn nq_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
