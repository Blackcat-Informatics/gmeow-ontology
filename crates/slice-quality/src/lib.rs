// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-slice quality report + opinionated uplift advisor — the shared kernel.
//!
//! `gmeow-dev slice-quality slices/<group>/<name>/` scores a slice across the
//! quality axes declared in the ontology-resident rubric
//! (`slices/core/slice-quality-rubric/`) and emits ranked, deterministic uplift
//! advice on the diagnostics substrate at `Standpoint::Advisory`.
//!
//! The rubric is **data** ([`rubric::load_rubric`]); Rust ships only a closed set
//! of measurement primitives the rubric's axes bind to. Grades form a bounded
//! lattice: the roll-up tier is the unweighted meet of the per-axis grades
//! ([`lattice`]). This crate is bound by both the dev CLI and the pipeline MCP.

pub mod axes;
pub mod counting;
pub mod doc_maturity;
pub mod error;
pub mod gate;
pub mod graph;
pub mod lattice;
pub mod model;
pub mod prioritize;
pub mod reasoner;
pub mod report;
pub mod rubric;
pub mod score;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_lang_bridge::GmnDictionary;
use purrdf::RdfDataset;
use rayon::prelude::*;

pub use model::{
    Axis, AxisFloorCommitment, AxisGrade, ContextScope, CountKind, Exemption, GovernanceFloors,
    MeasurementStandard, ProjectionCeilingCommitment, ProjectionVocabulary, Rubric,
    SliceAssessment, SliceTierFloorCommitment, Threshold, Tier,
};
pub use score::ScoringEnv;

/// The repo-wide slice-quality sweep products, scored in one pass over the discovered
/// slice set: the RDF assessment graph and the diagnostics report that backs JSON/SARIF/HTML
/// projections. Keeping these together prevents the pipeline from running the expensive
/// sweep twice when it needs both the queryable graph and the human-facing report.
pub struct AssessmentArtifacts {
    /// Deterministic N-Quads in the slice-quality assessment graph.
    pub nquads: String,
    /// Aggregate diagnostics report containing every scored slice's grades and advisories.
    pub report: gmeow_errors::Report,
    /// Per-slice wall timings in deterministic slice-directory order. Observational
    /// telemetry only; never serialized into the assessment graph or diagnostics.
    pub slice_timings: Vec<SliceScoreTiming>,
}

/// Observed wall time for one independently scored slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceScoreTiming {
    /// Repo-relative slice directory.
    pub slice: String,
    /// Wall time spent scoring that slice, including its reasoner probes.
    pub elapsed_ms: u128,
}

/// Parse one or more Turtle files into a single merged dataset.
///
/// # Errors
/// Returns a message if a file cannot be read or fails to parse.
pub fn dataset_from_paths(paths: &[&Path]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("{}: {e}", path.display()),
            })
        })?;
        let ds = purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("{}: {e}", path.display()),
            })
        })?;
        builder.push_dataset(&ds);
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(error::Io {
            detail: format!("dataset freeze failed: {e}"),
        })
    })
}

/// The canonical rubric module, relative to `repo_root` — the single on-disk file
/// the whole rubric (measurement standard + governance floors) is loaded from.
const RUBRIC_MODULE: &str = "slices/core/slice-quality-rubric/module.ttl";

/// Load the whole rubric from the canonical rubric slice under `repo_root`. This is
/// an INTERNAL helper: its two roles are exposed separately — SCORING reads only the
/// floor-free [`MeasurementStandard`] (`repo_rubric(root)?.standard`, used by the
/// sweep), and the ratchet gate reads only the [`GovernanceFloors`]
/// ([`load_repo_floors`]). No consumer holds the conflated whole.
///
/// # Errors
/// Returns a message if the rubric module cannot be read or is structurally
/// incomplete (a missing tier ladder, an axis without a producer, etc.).
fn repo_rubric(repo_root: &Path) -> gmeow_errors::Result<Rubric> {
    let module = repo_root.join(RUBRIC_MODULE);
    let ds = dataset_from_paths(&[&module])?;
    rubric::load_rubric(&ds)
}

/// Load ONLY the governance floors (dated exemptions + committed axis/tier floors)
/// from the canonical rubric slice under `repo_root` — the ratchet gate's and the
/// pipeline governance tooling's floor source. Scoring never reads these; this is
/// the floor half of the segregated rubric ([`GovernanceFloors`]).
///
/// # Errors
/// Returns a message if the rubric module cannot be read or is structurally
/// incomplete (the same hard-fail conditions as loading the whole rubric).
pub fn load_repo_floors(repo_root: &Path) -> gmeow_errors::Result<GovernanceFloors> {
    Ok(repo_rubric(repo_root)?.floors)
}

/// Load the governance data the PROJECTION-VOCABULARY RATCHET reads — the guarded
/// [`GovernanceFloors::vocabularies`] registry and the committed
/// [`GovernanceFloors::ceilings`] — from the canonical rubric slice under `repo_root`.
/// This is the ceiling ratchet's counterpart to [`load_repo_floors`]; both project the
/// same segregated [`GovernanceFloors`] the gate reads, named for their consumer so a
/// call site declares which half of the ratchet it drives. Scoring never reads these.
///
/// # Errors
/// Returns a message if the rubric module cannot be read or is structurally
/// incomplete (the same hard-fail conditions as loading the whole rubric).
pub fn load_repo_ceilings(repo_root: &Path) -> gmeow_errors::Result<GovernanceFloors> {
    Ok(repo_rubric(repo_root)?.floors)
}

/// Load ONLY the floor-free measurement standard (tier ladder + axes) from the
/// canonical rubric slice under `repo_root` — the scoring half of the segregated
/// rubric ([`MeasurementStandard`]). The ratchet gate never reads this; scoring
/// (the sweep and the MCP advisory tool) never reads the floors.
///
/// # Errors
/// Returns a message if the rubric module cannot be read or is structurally
/// incomplete (the same hard-fail conditions as loading the whole rubric).
pub fn repo_measurement_standard(repo_root: &Path) -> gmeow_errors::Result<MeasurementStandard> {
    Ok(repo_rubric(repo_root)?.standard)
}

/// Every `slices/<group>/<name>/` directory that holds a `manifest.ttl` — the slice
/// set the quality sweep scores, in deterministic (sorted) order. This is the SINGLE
/// discovery authority shared by the dev CLI sweep, the ratchet gate, and the pipeline
/// carrier producer, so all three score exactly the same slice set (dogfooding
/// coherence: the printed roll-up and the folded `graph/quality-assessment` agree).
#[must_use]
pub fn discover_slice_dirs(slices_root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if p.join("manifest.ttl").is_file() {
                    out.push(p.clone());
                }
                walk(&p, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(slices_root, &mut out);
    out.sort();
    out
}

/// Every authored `.ttl` the quality sweep reads across all slices: the rubric module,
/// each slice's `manifest.ttl`, and the files [`report::score_slice_with_standard`] ingests per slice
/// (`module.ttl`, `examples/`, `tests/`) — PLUS the `DocMaturity` axis's own real inputs
/// (`crates/slice-quality/src/doc_maturity.rs`), which are NOT covered by
/// [`report::slice_ttl_paths`]: each slice's `docs.md` (the `ThesisSentence` /
/// `RealizedState` slice-scoped coverage facts) and each slice's `i18n/*.po` translation
/// catalogs (the `TranslationCoverage` dimension), both read via `DocMaturity`'s own
/// `DocsModel::discover` sweep; and the generated `generated/catalog/constraint-catalog.nq`
/// catalog that same sweep hard-fails without (`crates/docs/src/model.rs::read_constraint_catalog`
/// — a regenerated tree always carries it, so its absence is a broken invariant, not an
/// optional input). `generated/catalog/term-content-manifest.nq` is deliberately NOT
/// listed here: `DocsModel::discover` itself tolerates its absence (the one-shot bootstrap
/// build that first mints it), so treating it as required-but-missing here would diverge
/// from the reader it keys.
///
/// This is the SINGLE authority the pipeline's source-load cache key over the assessment
/// graph consults — if any scored file changes, the attached `graph/quality-assessment`
/// must be recomputed (cache soundness: a stale scored input would ship a stale
/// assessment in `gmeow.gts`, including a docs-only edit that must not serve a stale
/// `DocMaturity` verdict). Deterministic and deduplicated.
///
/// # Errors
/// Hard-fails (never a silent skip) if a generated catalog `DocMaturity` requires is
/// missing on this tree — the same invariant [`gmeow_docs::model::DocsModel::discover`]
/// enforces. Per-slice files (`docs.md`, `i18n/*.po`, …) are legitimately optional and
/// are silently omitted when absent.
pub fn scored_source_files(repo_root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut files = vec![repo_root.join(RUBRIC_MODULE)];
    let constraint_catalog = repo_root.join("generated/catalog/constraint-catalog.nq");
    if !constraint_catalog.is_file() {
        return Err(gmeow_errors::Diag::of_kind(error::Io {
            detail: format!(
                "{}: required by the DocMaturity axis's DocsModel::discover sweep \
                 (crates/docs/src/model.rs::read_constraint_catalog) but not found on this tree",
                constraint_catalog.display()
            ),
        }));
    }
    files.push(constraint_catalog);
    for dir in discover_slice_dirs(&repo_root.join("slices")) {
        files.push(dir.join("manifest.ttl"));
        files.extend(report::slice_ttl_paths(&dir));
        files.push(dir.join("docs.md"));
        files.extend(doc_maturity_i18n_paths(&dir));
    }
    files.retain(|p| p.is_file());
    files.sort();
    files.dedup();
    Ok(files)
}

/// A slice's `i18n/*.po` translation catalogs (sorted; empty when the slice ships no
/// `i18n/` directory) — the `DocMaturity` axis's `TranslationCoverage` dimension input
/// ([`doc_maturity::DocMaturity`], via `gmeow_docs::i18n::Translations`).
fn doc_maturity_i18n_paths(slice_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(rd) = std::fs::read_dir(slice_dir.join("i18n")) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|x| x == "po") {
                paths.push(p);
            }
        }
    }
    paths.sort();
    paths
}

/// Score every discovered slice once and return all first-class assessment products:
/// the RDF graph projection and the aggregate diagnostics report. This is the shared
/// authority for repo-wide outputs, so the dev CLI, pipeline graph, and embedded HTML
/// report can agree without separate sweeps.
///
/// # Errors
/// Hard-fails if the rubric or ANY discovered slice cannot be scored.
pub fn assessment_artifacts(repo_root: &Path) -> gmeow_errors::Result<AssessmentArtifacts> {
    let rubric = repo_rubric(repo_root)?;
    let dirs = discover_slice_dirs(&repo_root.join("slices"));
    if dirs.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(error::Report {
            detail: "quality-assessment sweep found no slices".to_string(),
        }));
    }

    let mut nquads = String::new();
    let mut aggregate = gmeow_errors::Report::new("slice-quality");
    let scored = score_slices_with_rubric_timed(repo_root, &dirs, &rubric);
    let mut slice_timings = Vec::with_capacity(scored.len());
    for (report, timing) in scored {
        slice_timings.push(timing);
        let report = report?;
        nquads.push_str(&report.to_gmeow_rdf());
        let diagnostics = report.to_report();
        for finding in diagnostics.findings {
            aggregate.add_finding(finding);
        }
        for rule in diagnostics.rules {
            aggregate.add_rule(rule);
        }
    }
    Ok(AssessmentArtifacts {
        nquads,
        report: aggregate.normalized(),
        slice_timings,
    })
}

/// Score a sorted slice set concurrently and return one result per input directory
/// in that exact order.
///
/// Each slice is an independent immutable unit: its dataset, reasoning probes, and
/// advisories share only the loaded rubric and the read-only documentation facts.
/// Rayon therefore evaluates slices independently, while indexed collection and the
/// caller's sequential fold preserve the same first-error choice, assessment order,
/// diagnostics order, and RDF bytes as the serial implementation. A zero/one-slice
/// input stays on the direct path to avoid scheduler overhead on fixture-scale calls.
///
/// The repo-wide documentation model is primed before workers start. This prevents
/// every worker from parking behind the first `DocMaturity` cache fill and lets the
/// model's immutable per-slice facts become an ordinary shared read during scoring.
#[must_use]
pub fn score_slices_with_rubric(
    repo_root: &Path,
    dirs: &[PathBuf],
    rubric: &Rubric,
) -> Vec<gmeow_errors::Result<report::SliceReport>> {
    score_slices_with_rubric_timed(repo_root, dirs, rubric)
        .into_iter()
        .map(|(result, _timing)| result)
        .collect()
}

fn score_slices_with_rubric_timed(
    repo_root: &Path,
    dirs: &[PathBuf],
    rubric: &Rubric,
) -> Vec<(gmeow_errors::Result<report::SliceReport>, SliceScoreTiming)> {
    doc_maturity::prime_repo_facts(repo_root);
    let score = |dir: &PathBuf| {
        let started = std::time::Instant::now();
        let result = report::score_slice_with_standard(dir, &rubric.standard, ScoringEnv::Repo);
        let slice = dir
            .strip_prefix(repo_root)
            .unwrap_or(dir)
            .to_string_lossy()
            .replace('\\', "/");
        (
            result,
            SliceScoreTiming {
                slice,
                elapsed_ms: started.elapsed().as_millis(),
            },
        )
    };
    if dirs.len() <= 1 {
        dirs.iter().map(score).collect()
    } else {
        dirs.par_iter().map(score).collect()
    }
}

/// Score every discovered slice against the repo rubric and project the combined
/// assessment as deterministic N-Quads in the `gmeow:graph/slice-quality` named graph
/// (each slice's [`report::SliceReport::to_gmeow_rdf`] concatenated in sorted slice-dir
/// order). This is the SINGLE producer the pipeline attaches to the in-memory carrier
/// under `graph/quality-assessment`, so the `gmeow:QualityAssessment` graph ships inside
/// `gmeow.gts` (the issue's headline dogfooding deliverable) rather than only printing.
///
/// # Errors
/// Hard-fails (never a silent skip — no-optionality) if the rubric or ANY discovered
/// slice cannot be scored.
pub fn assessment_nquads(repo_root: &Path) -> gmeow_errors::Result<String> {
    Ok(assessment_artifacts(repo_root)?.nquads)
}

// -----------------------------------------------------------------------------
// The projection-vocabulary ratchet's shared residue-measurement helpers.
// -----------------------------------------------------------------------------

/// The ratchet-counted AUTHORING surface for one slice — the exact `.ttl` set
/// [`counting::residue`] measures a slice's guarded projection-vocabulary constructs
/// over: `module.ttl`, `shapes.ttl`, and every existing `mappings/*.ttl`, sorted.
///
/// This is DELIBERATELY NARROWER than [`report::slice_ttl_paths`] (which also walks
/// `examples/` and `tests/`): a demonstrator under `examples/` or a fixture under
/// `tests/` is not a second source of truth for hand-authored projection constructs
/// (the same "projection universe excludes conformance fixtures" doctrine the slice
/// producers already honor) — it is authored TO demonstrate or exercise a construct,
/// not TO author a new one. Counting them would let editing a fixture silently move
/// the ratchet (either inflating a slice's committed residue with non-authoring
/// artifacts, or letting a fixture's removal shrink a ceiling nobody actually paid
/// down), so only the slice's real authoring surface is scanned.
#[must_use]
pub fn ratchet_surface_paths(slice_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![slice_dir.join("module.ttl"), slice_dir.join("shapes.ttl")];
    // RECURSIVE over the whole mappings/ subtree, not just its immediate children —
    // the base-side scanner (`is_ratchet_surface`) already matches any `/mappings/`
    // path at any depth, so the working-tree scanner must too or the two diverge on a
    // nested mapping file (correction: one recursive scanner for base and working).
    collect_ttls_recursive(&slice_dir.join("mappings"), &mut paths);
    paths.retain(|p| p.is_file());
    paths.sort();
    paths.dedup();
    paths
}

/// The repo-level DSL mapping surface IRI a `dsl/mappings/` residue is attributed to
/// — a non-slice authoring surface (the hand-authored FnO carve-out
/// `dsl/mappings/transforms.fno.ttl` lives here) that the ratchet still guards.
pub const DSL_MAPPING_SURFACE_IRI: &str = "https://blackcatinformatics.ca/gmeow/dsl/mappings";

/// The repo-level `dsl/mappings/` authoring surface — every `.ttl` under it, sorted.
/// Scanned once (attributed to [`DSL_MAPPING_SURFACE_IRI`]), because it is a real
/// hand-authored projection surface (`transforms.fno.ttl`, the FnO carve-out) that
/// is NOT under any slice's `mappings/` directory and would otherwise be missed.
#[must_use]
pub fn ratchet_dsl_surface_paths(repo_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_ttls_recursive(&repo_root.join("dsl").join("mappings"), &mut paths);
    paths.retain(|p| p.is_file());
    paths.sort();
    paths.dedup();
    paths
}

/// Recursively collect every `.ttl` file under `dir` into `out` (no-op if `dir` does
/// not exist).
fn collect_ttls_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_ttls_recursive(&p, out);
        } else if p.extension().is_some_and(|x| x == "ttl") {
            out.push(p);
        }
    }
}

/// Measure the ungrounded [`counting::residue`] of every guarded `vocab` for every
/// discovered slice, over each slice's [`ratchet_surface_paths`] merged into one
/// dataset. This is the SINGLE measurement authority the projection-ceiling seed
/// (`gmeow-dev slice-quality-seed-ceilings`) and the ratchet gate both call — seed
/// and gate can never diverge on what "measured" means, because both count through
/// this one function over the one shared [`counting::residue`] primitive.
///
/// Keyed `(slice IRI, vocab prefix)`; a ZERO residue is DELIBERATELY OMITTED — the
/// gate treats a missing key as that vocab's `default_ceiling` (0), so the returned
/// map holds only the non-trivial ratchet cells worth seeding or gating. Iterating
/// [`discover_slice_dirs`] in its already-sorted order and returning a `BTreeMap`
/// makes the result fully deterministic.
///
/// # Errors
/// HARD-FAILS (propagates, never a silent fallback to residue 0) if a slice's
/// `manifest.ttl` cannot be resolved to a `gmeow:Slice` IRI, or if any of its
/// ratchet surfaces is unreadable or fails to parse — this is the gate path, so a
/// broken slice must stop the sweep, never silently score as clean.
pub fn measure_repo_residues(
    repo_root: &Path,
    vocabularies: &[ProjectionVocabulary],
) -> gmeow_errors::Result<std::collections::BTreeMap<(String, String), u64>> {
    let mut out = std::collections::BTreeMap::new();
    for dir in discover_slice_dirs(&repo_root.join("slices")) {
        let slice_iri = report::slice_iri_of(&dir)?;
        let paths = ratchet_surface_paths(&dir);
        let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        let ds = dataset_from_paths(&path_refs)?;
        for vocab in vocabularies {
            let residue = counting::residue_for_surface(&ds, vocab, &slice_iri);
            if residue > 0 {
                out.insert((slice_iri.clone(), vocab.prefix.clone()), residue);
            }
        }
    }
    // The repo-level dsl/mappings/ surface (the hand-authored FnO carve-out) is not
    // under any slice — measure it once, attributed to the DSL surface IRI, so it is
    // guarded rather than silently missed.
    let dsl_paths = ratchet_dsl_surface_paths(repo_root);
    if !dsl_paths.is_empty() {
        let dsl_refs: Vec<&Path> = dsl_paths.iter().map(PathBuf::as_path).collect();
        let dsl_ds = dataset_from_paths(&dsl_refs)?;
        for vocab in vocabularies {
            let residue = counting::residue_for_surface(&dsl_ds, vocab, DSL_MAPPING_SURFACE_IRI);
            if residue > 0 {
                out.insert(
                    (DSL_MAPPING_SURFACE_IRI.to_owned(), vocab.prefix.clone()),
                    residue,
                );
            }
        }
    }
    Ok(out)
}

/// Resolve `slice_dir`'s `gmeow:Slice` IRI from its `manifest.ttl` — a thin `pub`
/// wrapper over [`report::slice_iri_of`] so a consumer crate (the `gmeow-dev` CLI's
/// ratchet-gate driver, reconstructing which discovered slice a merge-base surface
/// set belongs to) can resolve the same slice IRI [`measure_repo_residues`] does,
/// without a second re-implementation of manifest resolution.
///
/// # Errors
/// As [`report::slice_iri_of`]: a message if the manifest cannot be read or
/// declares no `gmeow:Slice`.
pub fn slice_iri_of_dir(slice_dir: &Path) -> gmeow_errors::Result<String> {
    report::slice_iri_of(slice_dir)
}

/// Measure `vocab`'s ungrounded [`counting::residue`] over an ALREADY-READ set of
/// TTL texts, merged into one dataset — the base-reconstruction counterpart to
/// [`measure_repo_residues`] (which reads surfaces off disk). This is the SAME
/// counter ([`counting::residue`]) fed base-commit bytes instead of working-tree
/// files, so the gate's grandfather check (ratchet invariant 3) can never diverge
/// from what "measured" means on the working tree.
///
/// # Errors
/// HARD-FAILS (never falls back to residue 0) if any text fails to parse as
/// Turtle, or if merging the parsed datasets fails — this is the gate path, so a
/// broken base surface must stop the sweep, never silently score as clean.
pub fn residue_over_texts(
    texts: &[String],
    vocab: &ProjectionVocabulary,
    surface_iri: &str,
) -> gmeow_errors::Result<u64> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for text in texts {
        let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("residue_over_texts: Turtle parse failed: {e}"),
            })
        })?;
        builder.push_dataset(&ds);
    }
    let ds = builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(error::Io {
            detail: format!("residue_over_texts: dataset freeze failed: {e}"),
        })
    })?;
    Ok(counting::residue_for_surface(&ds, vocab, surface_iri))
}

// -----------------------------------------------------------------------------
// The consumer-wheel scoring entry point.
// -----------------------------------------------------------------------------

/// The two shipped standards a consumer scores a foreign slice against, flattened out
/// of a `gmeow.gts` wheel ONCE: the floor-free [`MeasurementStandard`] the lattice
/// scorer reads, and the shared `gmn1` [`GmnDictionary`] the `gmn1_coverage` axis
/// covers against. Both are required shipped inputs — [`Self::from_gts`] hard-fails a
/// corrupt wheel rather than degrade to a vacuous score. Reuse one instance across
/// many slices to avoid re-flattening the bundle per slice.
///
/// This path reads the bundle bytes and the external slice directory ONLY — never any
/// surrounding repo checkout, so a slice pulled in on its own scores identically
/// wherever it lives.
pub struct BundleStandards {
    standard: MeasurementStandard,
    gmn_dict: Arc<GmnDictionary>,
}

impl BundleStandards {
    /// Flatten the bundle + load the [`MeasurementStandard`] AND the shared `gmn1`
    /// dictionary ONCE.
    ///
    /// # Errors
    /// HARD FAILS on a corrupt wheel: a bundle that cannot be flattened, a missing or
    /// structurally-incomplete rubric, or a dictionary that is not a valid bijection —
    /// all required shipped inputs, never papered over with a fallback.
    pub fn from_gts(bundle_gts: &[u8]) -> gmeow_errors::Result<Self> {
        let ds = purrdf::gts::flattened_dataset_from_bytes(bundle_gts).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("cannot flatten gmeow.gts bundle: {e}"),
            })
        })?;
        let standard = rubric::load_rubric(&ds)?.standard;
        let gmn_dict = Arc::new(GmnDictionary::from_dataset(&ds).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Rubric {
                detail: format!("bundle gmn1 dictionary failed to load: {}", e.0),
            })
        })?);
        Ok(Self { standard, gmn_dict })
    }
}

/// Score an external slice directory against the standards flattened from a bundle
/// (reuse one [`BundleStandards`] across many slices). Reads the bundle-carried
/// standards + the external `slice_dir` ONLY — never a repo checkout.
///
/// # Errors
/// As [`report::score_slice_with_standard`].
pub fn score_external_slice(
    std: &BundleStandards,
    slice_dir: &Path,
) -> gmeow_errors::Result<report::SliceReport> {
    report::score_slice_with_standard(
        slice_dir,
        &std.standard,
        ScoringEnv::Bundle(std.gmn_dict.clone()),
    )
}

/// Score an external slice directory straight from bundle bytes — the one-slice
/// convenience over [`BundleStandards::from_gts`] + [`score_external_slice`]. Prefer
/// the two-step form when scoring many slices from one bundle (it flattens once).
///
/// # Errors
/// HARD FAILS on a corrupt wheel (as [`BundleStandards::from_gts`]) or an unscorable
/// slice (as [`score_external_slice`]).
pub fn score_external_slice_bytes(
    bundle_gts: &[u8],
    slice_dir: &Path,
) -> gmeow_errors::Result<report::SliceReport> {
    score_external_slice(&BundleStandards::from_gts(bundle_gts)?, slice_dir)
}

#[cfg(test)]
mod parallel_tests {
    use rayon::prelude::*;

    #[test]
    fn indexed_parallel_collection_preserves_input_and_error_order() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .expect("four-worker pool");
        let input: Vec<usize> = (0..128).collect();
        for _ in 0..8 {
            let output: Vec<Result<usize, usize>> = pool.install(|| {
                input
                    .par_iter()
                    .map(|value| {
                        if value % 17 == 0 {
                            Err(*value)
                        } else {
                            Ok(value * value)
                        }
                    })
                    .collect()
            });
            let serial: Vec<Result<usize, usize>> = input
                .iter()
                .map(|value| {
                    if value % 17 == 0 {
                        Err(*value)
                    } else {
                        Ok(value * value)
                    }
                })
                .collect();
            assert_eq!(output, serial);
        }
    }
}
