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
pub mod coat_guard;
pub mod counting;
pub mod doc_maturity;
pub mod error;
pub mod gate;
pub mod graph;
mod grounding;
pub mod lattice;
pub mod lint;
pub mod model;
pub mod prioritize;
pub mod read;
pub mod reasoner;
pub mod report;
pub mod rubric;
pub mod score;

use std::path::{Path, PathBuf};

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../build-support/path_dependency_inputs.rs"]
mod build_inputs;
use std::sync::Arc;

use gmeow_lang_bridge::GmnDictionary;
use purrdf::RdfDataset;
// rayon is a native-only dependency: wasm32 has no threads (GitHub Pages serves
// no COOP/COEP headers, so `SharedArrayBuffer` is unavailable), and the browser
// engine that consumes this crate must build for wasm32.
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

pub use counting::{Construct, RelocationReason, Witness};
pub use lint::{
    LintOutcome, declared_quality_tier, lint_report, resolve_min_tier, tier_gate_passes,
};
pub use model::{
    Axis, AxisFloorCommitment, AxisGrade, CeilingRelocation, ContextScope, CountKind, Exemption,
    GovernanceFloors, MeasurementStandard, ProjectionCeilingCommitment, ProjectionVocabulary,
    Rubric, SliceAssessment, SliceTierFloorCommitment, Threshold, Tier,
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

/// Parse and UNION N LABELLED in-memory Turtle documents into one dataset, in the
/// order given — the byte-level counterpart to [`dataset_from_paths`].
///
/// This is what the file-map scoring path uses in place of a path list: the label is
/// the document's slice-relative key (`"module.ttl"`, `"mappings/equivalences.ttl"`,
/// …), so a parse failure names the offending file exactly as the path-based reader
/// did with `Path::display`. Bytes (not `&str`) because Turtle is a byte grammar and
/// a non-UTF-8 document must surface as a parse failure naming its file, never be
/// silently dropped on a lossy decode.
///
/// # Errors
/// Returns a message naming the document's label if any document fails to parse or
/// the union cannot be frozen.
pub fn dataset_from_documents(docs: &[(&str, &[u8])]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for (label, bytes) in docs {
        let ds = purrdf::parse_dataset(bytes, "text/turtle", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("{label}: {e}"),
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

/// Parse and UNION N in-memory Turtle documents into one dataset — the in-memory
/// counterpart to [`dataset_from_paths`]. Used to reconstruct a rubric from bytes read
/// out of git history (`git show <base>:<path>`) rather than the working tree, so the
/// ratchet gate's merge-base floor reconstruction can union every slice's module.ttl at
/// the base exactly as [`governance_source_modules`] unions them on disk.
///
/// # Errors
/// Returns a message if any document fails to parse or the union cannot be frozen.
pub fn dataset_from_texts(texts: &[&str]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for (i, text) in texts.iter().enumerate() {
        let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("in-memory turtle document #{i}: {e}"),
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
/// the CENTRALIZED half of the rubric (the tier ladder, the quality axes, and the
/// guarded [`ProjectionVocabulary`] registry) is loaded from. The DISTRIBUTED half
/// (governance commitments: floors, tier floors, ceilings, exemptions) is loaded from
/// every slice's `module.ttl` — see [`governance_source_modules`].
///
/// `pub` because this is the crate-public canonical path to the centralized
/// measurement-standard / vocabulary-registry authoring module — the single
/// defining literal; downstream crates (e.g. `gmeow-dev-cli`) reference this
/// constant instead of redeclaring the path.
pub const RUBRIC_MODULE: &str = "slices/core/slice-quality-rubric/module.ttl";

/// DISTRIBUTED per-slice governance commitments (`gmeow:AxisFloorCommitment`,
/// `gmeow:SliceTierFloor`, `gmeow:ProjectionCeilingCommitment`, `gmeow:AxisExemption`)
/// may be authored in ANY slice's `module.ttl` — a floor belongs in its owning
/// slice's own module, not bottlenecked through the rubric slice. This is THE single
/// distributed-governance source authority: the ratchet gate, both pipeline export
/// stages, and the floor-monotonicity base-diff all route through this, so the
/// enforced set can never diverge from the authored set.
///
/// Returns the canonical rubric module PLUS every discovered slice's `module.ttl`,
/// filtered to files that exist on disk, sorted, and deduplicated (the rubric slice
/// is itself discovered by [`discover_slice_dirs`], so dedup collapses that
/// duplicate — load-bearing, not an oversight).
#[must_use]
pub fn governance_source_modules(repo_root: &Path) -> Vec<PathBuf> {
    let mut modules = vec![repo_root.join(RUBRIC_MODULE)];
    for dir in discover_slice_dirs(&repo_root.join("slices")) {
        modules.push(dir.join("module.ttl"));
    }
    modules.retain(|p| p.is_file());
    modules.sort();
    modules.dedup();
    modules
}

/// Load the whole SEGREGATED rubric under `repo_root`: the CENTRALIZED
/// [`MeasurementStandard`] + [`ProjectionVocabulary`] registry from ONLY the
/// canonical rubric module, and the DISTRIBUTED [`GovernanceFloors`] commitments
/// unioned across every governance module ([`governance_source_modules`]). This is
/// an INTERNAL helper: its two roles are exposed separately — SCORING reads only the
/// floor-free [`MeasurementStandard`] (`repo_rubric(root)?.standard`, used by the
/// sweep), and the ratchet gate reads only the [`GovernanceFloors`]
/// ([`load_repo_floors`]). No consumer holds the conflated whole.
///
/// A centralized individual (`gmeow:QualityAxis`, `gmeow:QualityTier`,
/// `gmeow:ProjectionVocabulary`) authored OUTSIDE the rubric slice is a HARD FAIL
/// (the centralized-authority guard): the measurement standard and the guarded
/// vocabulary registry have exactly one authoring boundary, and a foreign slice
/// smuggling in a new axis or tier would silently widen what "the rubric" means.
///
/// # Errors
/// Returns a message if any governance module cannot be read, if the rubric is
/// structurally incomplete (a missing tier ladder, an axis without a producer,
/// etc.), if two different governance modules author the same
/// (slice, axis) / slice / (slice, vocabulary) commitment key, or if a centralized
/// individual is authored outside the rubric slice.
fn repo_rubric(repo_root: &Path) -> gmeow_errors::Result<Rubric> {
    let canonical = rubric::load_rubric(&*dataset_from_paths(&[&repo_root.join(RUBRIC_MODULE)])?)?;

    let modules = governance_source_modules(repo_root);
    detect_cross_file_governance_collisions(&modules)?;
    let module_refs: Vec<&Path> = modules.iter().map(PathBuf::as_path).collect();
    let widened = rubric::load_rubric(&*dataset_from_paths(&module_refs)?)?;

    segregate_rubric(canonical, widened)
}

/// Assemble the SEGREGATED rubric from a CANONICAL (rubric-module-only) load and a
/// WIDENED (unioned-across-slices) load: keep the centralized [`MeasurementStandard`] +
/// [`ProjectionVocabulary`] registry from `canonical`, take the distributed
/// floor / tier-floor / ceiling / exemption commitments from `widened`, and HARD-FAIL
/// (the centralized-authority guard) if `widened` carries any `gmeow:QualityAxis` /
/// `gmeow:QualityTier` / `gmeow:ProjectionVocabulary` the canonical load does not. This
/// is the SINGLE home of the segregation+guard, shared by the repo-root loader
/// ([`repo_rubric`]) and the ratchet gate's merge-base reconstruction (which builds its
/// two loads from git-history bytes via [`dataset_from_texts`]), so the guard can never
/// diverge between the working-tree gate and the base comparand.
///
/// # Errors
/// Returns the centralized-authority-violation diagnostic when a centralized individual
/// is authored outside the rubric slice.
pub fn segregate_rubric(canonical: Rubric, widened: Rubric) -> gmeow_errors::Result<Rubric> {
    if widened.standard != canonical.standard
        || widened.floors.vocabularies != canonical.floors.vocabularies
    {
        return Err(centralized_authority_violation(&canonical, &widened));
    }

    Ok(Rubric {
        standard: canonical.standard,
        floors: GovernanceFloors {
            exemptions: widened.floors.exemptions,
            commitments: widened.floors.commitments,
            tier_floors: widened.floors.tier_floors,
            vocabularies: canonical.floors.vocabularies,
            ceilings: widened.floors.ceilings,
            relocations: widened.floors.relocations,
        },
    })
}

/// Load the whole segregated rubric (centralized standard/registry + distributed
/// commitments) for `repo_root` — the gate's single loader, so a consumer crate (the
/// `gmeow-dev` ratchet-gate driver) never re-derives the segregated-load sequence.
///
/// # Errors
/// As [`repo_rubric`].
pub fn load_repo_rubric(repo_root: &Path) -> gmeow_errors::Result<Rubric> {
    repo_rubric(repo_root)
}

/// Build the centralized-authority-violation diagnostic: names every axis, tier, or
/// projection-vocabulary that is present in the DISTRIBUTED union (`widened`) but
/// absent from — or structurally different than — the CANONICAL rubric-only load.
/// Deterministic (sorted lines) so the message is stable across runs.
fn centralized_authority_violation(canonical: &Rubric, widened: &Rubric) -> gmeow_errors::Diag {
    let mut lines: Vec<String> = Vec::new();

    for axis in &widened.standard.axes {
        match canonical.standard.axes.iter().find(|a| a.iri == axis.iri) {
            None => lines.push(format!(
                "gmeow:QualityAxis {} authored outside the rubric slice ({RUBRIC_MODULE})",
                axis.iri
            )),
            Some(c) if c != axis => lines.push(format!(
                "gmeow:QualityAxis {} redefined outside the rubric slice ({RUBRIC_MODULE})",
                axis.iri
            )),
            Some(_) => {}
        }
    }

    for tier in &widened.standard.tiers {
        match canonical.standard.tiers.iter().find(|t| t.iri == tier.iri) {
            None => lines.push(format!(
                "gmeow:QualityTier {} authored outside the rubric slice ({RUBRIC_MODULE})",
                tier.iri
            )),
            Some(c) if c != tier => lines.push(format!(
                "gmeow:QualityTier {} redefined outside the rubric slice ({RUBRIC_MODULE})",
                tier.iri
            )),
            Some(_) => {}
        }
    }

    for vocab in &widened.floors.vocabularies {
        match canonical
            .floors
            .vocabularies
            .iter()
            .find(|v| v.prefix == vocab.prefix)
        {
            None => lines.push(format!(
                "gmeow:ProjectionVocabulary prefix {:?} authored outside the rubric slice ({RUBRIC_MODULE})",
                vocab.prefix
            )),
            Some(c) if c != vocab => lines.push(format!(
                "gmeow:ProjectionVocabulary prefix {:?} redefined outside the rubric slice ({RUBRIC_MODULE})",
                vocab.prefix
            )),
            Some(_) => {}
        }
    }

    lines.sort();
    gmeow_errors::Diag::of_kind(error::Rubric {
        detail: format!(
            "centralized rubric authority violated — a gmeow:QualityAxis / gmeow:QualityTier / \
             gmeow:ProjectionVocabulary individual may only be authored in {RUBRIC_MODULE}: {}",
            lines.join("; ")
        ),
    })
}

/// Detect a DISTRIBUTED governance commitment key
/// (`(slice, axis)` / `slice` / `(slice, vocabulary)`) authored in more than one
/// governance module. Runs BEFORE the widened union load: once every module's
/// triples share one dataset, a collision can only be reported by its ambiguous
/// individual IRI (`rubric::load_rubric`'s existing per-key guard) — this scans
/// each module SEPARATELY first, so a cross-file collision can name BOTH source
/// files. A same-file duplicate is intentionally NOT reported here; it still
/// hard-fails downstream via `rubric::load_rubric`'s own guard.
///
/// Thin path-based wrapper over [`detect_cross_file_collisions_labeled`]: each
/// module is parsed from disk and labeled by its own path (`Path::display`),
/// matching the diagnostic wording this crate has always emitted for the
/// working-tree loader ([`repo_rubric`]).
///
/// # Errors
/// Returns a message naming both source files when the same key is authored in two
/// DIFFERENT governance modules. Propagates a read/parse failure for any module.
fn detect_cross_file_governance_collisions(modules: &[PathBuf]) -> gmeow_errors::Result<()> {
    let mut labeled: Vec<(String, Arc<RdfDataset>)> = Vec::with_capacity(modules.len());
    for module in modules {
        labeled.push((
            module.display().to_string(),
            dataset_from_paths(&[module.as_path()])?,
        ));
    }
    detect_cross_file_collisions_labeled(&labeled)
}

/// Detect a DISTRIBUTED governance commitment key
/// (`(slice, axis)` / `slice` / `(slice, vocabulary)`) authored in more than one
/// governance SOURCE, over IN-MEMORY Turtle documents rather than on-disk paths.
/// The text-based counterpart to [`detect_cross_file_governance_collisions`],
/// sharing the SAME collision core ([`detect_cross_file_collisions_labeled`]) so
/// the two entry points can never diverge in which keys collide or how the
/// message is worded.
///
/// Used by the ratchet gate's merge-base reconstruction (`base_rubric_at` in
/// `gmeow-dev-cli`), which reads base module bytes via `git show <base>:<path>`
/// (text blobs, not on-disk files) and therefore cannot call the path-based
/// entry point directly — this lets the base comparand report a cross-file
/// collision identically to the working tree (naming both source labels)
/// instead of falling back to `rubric::load_rubric`'s less-precise ambiguous-IRI
/// guard.
///
/// # Errors
/// Returns a message naming both source labels when the same key is authored in
/// two DIFFERENT sources. Propagates a parse failure for any text.
pub fn detect_cross_file_governance_collisions_texts(
    labeled_texts: &[(&str, &str)],
) -> gmeow_errors::Result<()> {
    let mut labeled: Vec<(String, Arc<RdfDataset>)> = Vec::with_capacity(labeled_texts.len());
    for (label, text) in labeled_texts {
        labeled.push(((*label).to_owned(), dataset_from_texts(&[text])?));
    }
    detect_cross_file_collisions_labeled(&labeled)
}

/// The shared collision-detection core: keys a DISTRIBUTED governance commitment
/// (`(slice, axis)` / `slice` / `(slice, vocabulary)`) over every `(label, dataset)`
/// source, hard-failing naming both labels when the same key is authored twice
/// under two DIFFERENT labels. A same-label duplicate is intentionally NOT
/// reported here; it still hard-fails downstream via `rubric::load_rubric`'s own
/// per-key IRI guard. Both [`detect_cross_file_governance_collisions`] (path-based)
/// and [`detect_cross_file_governance_collisions_texts`] (text-based) delegate here
/// so the two can never diverge.
///
/// # Errors
/// Returns a message naming both source labels when the same key is authored under
/// two DIFFERENT labels.
fn detect_cross_file_collisions_labeled(
    sources: &[(String, Arc<RdfDataset>)],
) -> gmeow_errors::Result<()> {
    let mut axis_floor_keys: std::collections::BTreeMap<(String, String), &str> =
        std::collections::BTreeMap::new();
    let mut tier_floor_keys: std::collections::BTreeMap<String, &str> =
        std::collections::BTreeMap::new();
    let mut ceiling_keys: std::collections::BTreeMap<(String, String), &str> =
        std::collections::BTreeMap::new();

    for (label, ds) in sources {
        let floor_slice_p = graph::id(ds, &graph::g("floorSlice"));
        let floor_axis_p = graph::id(ds, &graph::g("floorAxis"));
        let ceiling_slice_p = graph::id(ds, &graph::g("ceilingSlice"));
        let ceiling_vocab_p = graph::id(ds, &graph::g("ceilingVocabulary"));

        for iri in graph::instances_of(ds, &graph::g("AxisFloorCommitment")) {
            let Some(sid) = graph::id(ds, &iri) else {
                continue;
            };
            let (Some(slice), Some(axis)) = (
                floor_slice_p.and_then(|p| graph::one_iri(ds, sid, p)),
                floor_axis_p.and_then(|p| graph::one_iri(ds, sid, p)),
            ) else {
                continue;
            };
            let key = (slice, axis);
            match axis_floor_keys.get(&key) {
                Some(prior) if *prior != label.as_str() => {
                    return Err(cross_file_collision_err(
                        "AxisFloorCommitment",
                        &format!("slice {} axis {}", key.0, key.1),
                        prior,
                        label,
                    ));
                }
                Some(_) => {}
                None => {
                    axis_floor_keys.insert(key, label.as_str());
                }
            }
        }

        for iri in graph::instances_of(ds, &graph::g("SliceTierFloor")) {
            let Some(sid) = graph::id(ds, &iri) else {
                continue;
            };
            let Some(slice) = floor_slice_p.and_then(|p| graph::one_iri(ds, sid, p)) else {
                continue;
            };
            match tier_floor_keys.get(&slice) {
                Some(prior) if *prior != label.as_str() => {
                    return Err(cross_file_collision_err(
                        "SliceTierFloor",
                        &format!("slice {slice}"),
                        prior,
                        label,
                    ));
                }
                Some(_) => {}
                None => {
                    tier_floor_keys.insert(slice, label.as_str());
                }
            }
        }

        for iri in graph::instances_of(ds, &graph::g("ProjectionCeilingCommitment")) {
            let Some(sid) = graph::id(ds, &iri) else {
                continue;
            };
            let (Some(slice), Some(vocab)) = (
                ceiling_slice_p.and_then(|p| graph::one_iri(ds, sid, p)),
                ceiling_vocab_p.and_then(|p| graph::one_iri(ds, sid, p)),
            ) else {
                continue;
            };
            let key = (slice, vocab);
            match ceiling_keys.get(&key) {
                Some(prior) if *prior != label.as_str() => {
                    return Err(cross_file_collision_err(
                        "ProjectionCeilingCommitment",
                        &format!("slice {} vocabulary {}", key.0, key.1),
                        prior,
                        label,
                    ));
                }
                Some(_) => {}
                None => {
                    ceiling_keys.insert(key, label.as_str());
                }
            }
        }
    }

    Ok(())
}

/// Build the cross-file-collision diagnostic naming both offending source labels
/// (a path's `Display` rendering for the working-tree caller, a rel-path label for
/// the base-reconstruction caller).
fn cross_file_collision_err(
    kind: &str,
    key_desc: &str,
    label_a: &str,
    label_b: &str,
) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(error::Rubric {
        detail: format!(
            "duplicate gmeow:{kind} for {key_desc} authored in two different governance \
             modules: {label_a} and {label_b}"
        ),
    })
}

/// Load ONLY the governance floors (dated exemptions + committed axis/tier floors)
/// unioned across every slice's `module.ttl` under `repo_root` — the ratchet gate's
/// and the pipeline governance tooling's floor source. Scoring never reads these;
/// this is the floor half of the segregated rubric ([`GovernanceFloors`]).
///
/// # Errors
/// As [`repo_rubric`].
pub fn load_repo_floors(repo_root: &Path) -> gmeow_errors::Result<GovernanceFloors> {
    Ok(repo_rubric(repo_root)?.floors)
}

/// Load the governance data the PROJECTION-VOCABULARY RATCHET reads — the guarded
/// [`GovernanceFloors::vocabularies`] registry (centralized, rubric-slice-only) and
/// the committed [`GovernanceFloors::ceilings`] (distributed, unioned across every
/// slice's `module.ttl`) — under `repo_root`. This is the ceiling ratchet's
/// counterpart to [`load_repo_floors`]; both project the same segregated
/// [`GovernanceFloors`] the gate reads, named for their consumer so a call site
/// declares which half of the ratchet it drives. Scoring never reads these.
///
/// # Errors
/// As [`repo_rubric`].
pub fn load_repo_ceilings(repo_root: &Path) -> gmeow_errors::Result<GovernanceFloors> {
    Ok(repo_rubric(repo_root)?.floors)
}

/// Load the projection-vocabulary ratchet's COMMITTED governance — the guarded
/// registry and the committed ceilings — straight from a shipped `gmeow.gts` bundle,
/// reusing the exact same [`rubric::load_rubric`] the repo path uses. This is the
/// shippable-`gmeow`-CLI counterpart to [`load_repo_ceilings`]: it surfaces the
/// resident `gmeow:ProjectionCeilingCommitment` / `gmeow:ProjectionVocabulary`
/// individuals (the commitments — never the live measured residue, which needs a repo
/// checkout to scan) from the bundle, dogfooding Principle 17 from the deliverable.
///
/// # Errors
/// HARD FAILS on a corrupt bundle that cannot be flattened, or a structurally
/// incomplete rubric — the shipped inputs are required, never papered over.
pub fn ceilings_from_gts(bundle_gts: &[u8]) -> gmeow_errors::Result<GovernanceFloors> {
    let ds = purrdf::gts::flattened_dataset_from_bytes(bundle_gts).map_err(|e| {
        gmeow_errors::Diag::of_kind(error::Io {
            detail: format!("cannot flatten gmeow.gts bundle: {e}"),
        })
    })?;
    Ok(rubric::load_rubric(&ds)?.floors)
}

/// Load ONLY the floor-free measurement standard (tier ladder + axes) — CENTRALIZED,
/// loaded from ONLY the canonical rubric slice under `repo_root` — the scoring half
/// of the segregated rubric ([`MeasurementStandard`]). The ratchet gate never reads
/// this; scoring (the sweep and the MCP advisory tool) never reads the floors.
///
/// # Errors
/// As [`repo_rubric`].
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
/// `DocsModel` sweep.
///
/// The generated `generated/catalog/constraint-catalog.nq` is DELIBERATELY NOT listed
/// here anymore: the in-pipeline `DocMaturity` sweep now sources the constraint catalog
/// from THIS run's freshly-rendered `stage-constraint-catalog` bytes
/// ([`gmeow_docs_model::model::DocsModel::discover_with_catalog`]), never a disk read of the
/// not-yet-materialized `generated/` file (the cold-absence class this retires). Its
/// SOURCE determinants — each slice's `module.ttl` (already listed via
/// [`report::slice_ttl_paths`]) and the root ontology — bust the cache when the catalog
/// would change, and the catalog content does not feed the coverage fraction anyway, so
/// dropping the generated file loses no cache soundness. `generated/catalog/term-content-manifest.nq`
/// is likewise not listed: `DocsModel::discover` tolerates its absence (the one-shot
/// bootstrap build that first mints it) and it too is provenance-only, not a coverage
/// determinant.
///
/// This is the SINGLE authority the pipeline's source-load cache key over the assessment
/// graph consults — if any scored file changes, the attached `graph/quality-assessment`
/// must be recomputed (cache soundness: a stale scored input would ship a stale
/// assessment in `gmeow.gts`, including a docs-only edit that must not serve a stale
/// `DocMaturity` verdict). Deterministic and deduplicated. All entries are authored
/// sources, so no generated artifact is required to be present — a cold tree scores
/// cleanly.
pub fn scored_source_files(repo_root: &Path) -> Vec<PathBuf> {
    let mut files = vec![repo_root.join(RUBRIC_MODULE)];
    for dir in discover_slice_dirs(&repo_root.join("slices")) {
        files.push(dir.join("manifest.ttl"));
        files.extend(report::slice_ttl_paths(&dir));
        files.push(dir.join("docs.md"));
        files.extend(doc_maturity_i18n_paths(&dir));
    }
    files.retain(|p| p.is_file());
    files.sort();
    files.dedup();
    files
}

/// The canonicalized content fingerprint of every authored file the quality sweep
/// scores — the freshness witness the projected corpus carries as its
/// `gmeow:versionFingerprint`, and the value a consumer of that corpus recomputes to
/// prove the record still describes the working tree
/// ([`read::RecordedCorpus::verify_fresh`]).
///
/// Folds, in sorted repo-relative path order, each file's path, byte length, and bytes
/// — so a rename, a truncation, and an edit are all distinguishable, and the digest is
/// identical across platforms for the same repository state.
///
/// The fold has TWO halves, because a score is a function of two things and a witness
/// over only one of them is not a freshness proof:
///
/// * the **scored data** — [`scored_source_files`], the SAME single authority the
///   pipeline's source-load cache key consults; there is deliberately no second
///   enumeration of the data half that could drift from it; and
/// * the **scoring code** — [`scorer_impl_files`], this crate's transitive
///   path-dependency closure. A path dependency carries no `Cargo.lock` checksum, so
///   editing the scorer (or `gmeow-docs`, which owns the whole `DocMaturity` coverage
///   computation) changes every grade while leaving every scored `.ttl` byte-identical.
///   Without this half a record produced by the OLD scorer verifies as current against
///   the NEW one — the record stands alone, so it must attest both.
///
/// The pipeline's stage cache does not need the code half here: `crates/pipeline`'s
/// `cache::BUILD_FINGERPRINT` already folds the whole workspace source + `Cargo.lock` +
/// the `rustc` version into EVERY stage key, so a code change re-runs the sweep there.
/// This witness has no such salt — nothing else guards a consumer that reads the record
/// without going through the DAG — so it carries the code half itself. That asymmetry is
/// deliberate: `scored_source_files` stays the pure authored-data authority the cache key
/// shares, and the extra half lives only in the freshness witness that needs it.
///
/// # Errors
/// If any scored source file cannot be read. A file that vanished between enumeration
/// and hashing makes the digest meaningless, so it is a hard failure rather than a
/// silently shorter fold.
pub fn scored_input_fingerprint(repo_root: &Path) -> gmeow_errors::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut inputs = scored_source_files(repo_root);
    inputs.extend(scorer_impl_files(repo_root));
    inputs.sort();
    inputs.dedup();
    for path in inputs {
        let rel = path
            .strip_prefix(repo_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = std::fs::read(&path).map_err(|e| {
            gmeow_errors::Diag::of_kind(error::Io {
                detail: format!("fingerprinting scored source {}: {e}", path.display()),
            })
        })?;
        hasher.update(rel.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
        hasher.update(b"\x1e");
    }
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

/// Every workspace crate whose Rust sources can execute inside the quality sweep — the
/// TRANSITIVE `path = "../…"` dependency closure of `gmeow-slice-quality`, itself
/// included.
///
/// Sorted; each entry's `src/` tree and `Cargo.toml` are folded into
/// [`scored_input_fingerprint`]. `scorer_dep_closure_is_fully_hashed` re-derives the
/// closure from the workspace manifests, so a NEW path dependency reds a test instead of
/// silently opening the hole again. The same defect (a path dependency carries no
/// `Cargo.lock` checksum) is handled on the documentation-fixture side by
/// `gmeow_docs_model::fixture`'s `fixture_crate_dirs`, which DERIVES its closure from the
/// manifests rather than restating it; hand-picking "the crates that really matter" is
/// exactly the argument that makes such a list wrong, which is why the test below is what
/// owns this list's contents.
const SCORER_CRATE_ROOTS: &[&str] = &[
    "action-cache",
    "docs-model",
    "errors",
    "gts-profile",
    "lang-bridge",
    "lang-form",
    "license",
    "logic",
    "logic-compile",
    "math",
    "math-lift",
    "ns",
    "slice-quality",
    "term-arena",
    "validate",
];

/// The Rust sources + manifests of [`SCORER_CRATE_ROOTS`] under `repo_root` — the CODE
/// half of [`scored_input_fingerprint`]. Sorted, and silent about crates absent from the
/// tree (a consumer scoring a partial checkout folds what is there; the closure test
/// pins the set against the real workspace).
fn scorer_impl_files(repo_root: &Path) -> Vec<PathBuf> {
    fn walk_rs(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk_rs(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let crates_dir = repo_root.join("crates");
    let mut files = Vec::new();
    for krate in SCORER_CRATE_ROOTS {
        walk_rs(&crates_dir.join(krate).join("src"), &mut files);
        let manifest = crates_dir.join(krate).join("Cargo.toml");
        if manifest.is_file() {
            files.push(manifest);
        }
    }
    files.sort();
    files
}

/// A slice's `i18n/*.po` translation catalogs (sorted; empty when the slice ships no
/// `i18n/` directory) — the `DocMaturity` axis's `TranslationCoverage` dimension input
/// ([`doc_maturity::DocMaturity`], via `gmeow_docs_model::i18n::Translations`).
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
    assessment_artifacts_inner(repo_root, None)
}

/// Score every discovered slice as [`assessment_artifacts`] does, but source the
/// `DocMaturity` axis's constraint catalog from `catalog_bytes` — THIS run's
/// freshly-rendered `stage-constraint-catalog` product — instead of the committed
/// `generated/catalog/constraint-catalog.nq` on disk. The IN-PIPELINE
/// quality-assessment stage MUST use this: on a cold tree the committed catalog is
/// not yet materialized, and the disk read would fail the whole documentation model
/// build, collapsing every slice's `DocMaturity` to a vacuous `1.0` and diverging
/// from a warm run's real scores (the two-generation determinism gate). The catalog
/// content does not feed the coverage fraction, so live bytes only guarantee the
/// model builds — identically cold and warm.
///
/// # Errors
/// Hard-fails if the rubric or ANY discovered slice cannot be scored.
pub fn assessment_artifacts_with_catalog(
    repo_root: &Path,
    catalog_bytes: &[u8],
) -> gmeow_errors::Result<AssessmentArtifacts> {
    assessment_artifacts_inner(repo_root, Some(catalog_bytes))
}

fn assessment_artifacts_inner(
    repo_root: &Path,
    catalog_bytes: Option<&[u8]>,
) -> gmeow_errors::Result<AssessmentArtifacts> {
    let rubric = repo_rubric(repo_root)?;
    let dirs = discover_slice_dirs(&repo_root.join("slices"));
    if dirs.is_empty() {
        return Err(gmeow_errors::Diag::of_kind(error::Report {
            detail: "quality-assessment sweep found no slices".to_string(),
        }));
    }

    let mut aggregate = gmeow_errors::Report::new("slice-quality");
    let scored = score_slices_with_rubric_timed(repo_root, &dirs, &rubric, catalog_bytes);
    let mut slice_timings = Vec::with_capacity(scored.len());
    // The per-slice blocks are accumulated first because the corpus header carries a
    // digest OVER them (`report::corpus_content_digest`): the header cannot be written
    // until every grade it attests is known.
    let mut blocks = String::new();
    let mut assessments: Vec<crate::model::SliceAssessment> = Vec::new();
    for (report, timing) in scored {
        slice_timings.push(timing);
        let report = report?;
        blocks.push_str(&report.to_gmeow_rdf());
        assessments.push(report.assessment.clone());
        let diagnostics = report.to_report();
        for finding in diagnostics.findings {
            aggregate.add_finding(finding);
        }
        for rule in diagnostics.rules {
            aggregate.add_rule(rule);
        }
    }
    // The corpus-level witnesses, emitted ONCE ahead of the per-slice blocks: the input
    // fingerprint (WHICH sources were scored) and the record content digest (WHAT was
    // recorded). It is what makes the projection readable AS a record rather than as a
    // snapshot of unknown vintage — see `read::RecordedCorpus::verify_fresh` and
    // `read::read_recorded_corpus_bytes`, which refuse a record failing either witness.
    let mut nquads = report::corpus_fingerprint_nquads(
        &scored_input_fingerprint(repo_root)?,
        &report::corpus_content_digest(&assessments),
    );
    nquads.push_str(&blocks);
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
    score_slices_with_rubric_timed(repo_root, dirs, rubric, None)
        .into_iter()
        .map(|(result, _timing)| result)
        .collect()
}

fn score_slices_with_rubric_timed(
    repo_root: &Path,
    dirs: &[PathBuf],
    rubric: &Rubric,
    catalog_bytes: Option<&[u8]>,
) -> Vec<(gmeow_errors::Result<report::SliceReport>, SliceScoreTiming)> {
    doc_maturity::prime_repo_facts(repo_root, catalog_bytes);
    let score = |dir: &PathBuf| {
        let started = std::time::Instant::now();
        let result = report::score_slice_with_standard(
            dir,
            &rubric.standard,
            ScoringEnv::Repo {
                slice_dir: dir.clone(),
            },
        );
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
    // On wasm32 the parallel branch is compiled out entirely -- there are no
    // threads to fan out to. `par_iter().map(..).collect()` into a Vec preserves
    // input order, so the serial path is byte-identical to the parallel one
    // rather than merely equivalent; `serial_and_parallel_scores_are_byte_identical`
    // pins that against the real rubric.
    #[cfg(target_arch = "wasm32")]
    {
        dirs.iter().map(score).collect()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if dirs.len() <= 1 {
            dirs.iter().map(score).collect()
        } else {
            dirs.par_iter().map(score).collect()
        }
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
///
/// THE single definition of "which files are a ratchet surface", for BOTH sides of the
/// gate: the merge-base measurement materializes the base tree with one `git archive`
/// and then runs this exact scanner over it, so there is no base-only path
/// reconstruction that could drift from the working-tree one.
#[must_use]
pub fn ratchet_surface_paths(slice_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![slice_dir.join("module.ttl"), slice_dir.join("shapes.ttl")];
    // RECURSIVE over the whole mappings/ subtree, not just its immediate children — a
    // nested mapping file is authoring surface exactly as an immediate child is, and
    // the base side runs this same walk over the materialized base tree.
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
    Ok(measure_repo_residue_constructs(repo_root, vocabularies)?
        .into_iter()
        .map(|(key, constructs)| (key, constructs.len() as u64))
        .collect())
}

/// The CONSTRUCT SET behind [`measure_repo_residues`] — the same single sweep, keyed
/// `(slice IRI, vocab prefix)`, with each residue construct's [`counting::Witness`]
/// intact. [`measure_repo_residues`] is this function's `.len()` projection, so there
/// is exactly ONE working-tree measurement and the relocation accounting can never
/// disagree with the count gate about which constructs are in the residue.
///
/// # Errors
/// As [`measure_repo_residues`].
pub fn measure_repo_residue_constructs(
    repo_root: &Path,
    vocabularies: &[ProjectionVocabulary],
) -> gmeow_errors::Result<std::collections::BTreeMap<(String, String), Vec<counting::Construct>>> {
    let mut out = std::collections::BTreeMap::new();
    for dir in discover_slice_dirs(&repo_root.join("slices")) {
        let slice_iri = report::slice_iri_of(&dir)?;
        let paths = ratchet_surface_paths(&dir);
        for (prefix, constructs) in
            measure_surface_residue_constructs(&paths, &slice_iri, vocabularies)?
        {
            out.insert((slice_iri.clone(), prefix), constructs);
        }
    }
    // The repo-level dsl/mappings/ surface (the hand-authored FnO carve-out) is not
    // under any slice — measure it once, attributed to the DSL surface IRI, so it is
    // guarded rather than silently missed.
    let dsl_paths = ratchet_dsl_surface_paths(repo_root);
    for (prefix, constructs) in
        measure_surface_residue_constructs(&dsl_paths, DSL_MAPPING_SURFACE_IRI, vocabularies)?
    {
        out.insert((DSL_MAPPING_SURFACE_IRI.to_owned(), prefix), constructs);
    }
    Ok(out)
}

/// Measure EVERY guarded vocab's ungrounded residue CONSTRUCT SET over one authoring
/// surface fileset (`paths`, read as plain files off disk), attributed to
/// `surface_iri`. Keyed by vocab prefix; a vocab whose residue is EMPTY is deliberately
/// omitted, mirroring [`measure_repo_residues`]'s "a missing key means the vocab's
/// `default_ceiling`" contract.
///
/// This is the SINGLE per-surface measurement primitive. [`measure_repo_residues`]
/// (working tree) and the ratchet gate's merge-base reconstruction (which materializes
/// the base tree with one `git archive` and then reads plain files) both go through it,
/// so base and working are an apples-to-apples measurement of the same code rather than
/// two parallel implementations that can drift. Each construct carries its
/// [`counting::Witness`], so a caller can compare the two sides by relocation-invariant
/// identity instead of by count alone.
///
/// # Errors
/// HARD-FAILS (never a silent fallback to residue 0) if any path is unreadable or fails
/// to parse as Turtle — this is the gate path, so a broken surface must stop the sweep.
pub fn measure_surface_residue_constructs(
    paths: &[PathBuf],
    surface_iri: &str,
    vocabularies: &[ProjectionVocabulary],
) -> gmeow_errors::Result<std::collections::BTreeMap<String, Vec<counting::Construct>>> {
    let mut out = std::collections::BTreeMap::new();
    if paths.is_empty() {
        return Ok(out);
    }
    let path_refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    let ds = dataset_from_paths(&path_refs)?;
    for vocab in vocabularies {
        let constructs = counting::residue_constructs_for_surface(&ds, vocab, surface_iri);
        if !constructs.is_empty() {
            out.insert(vocab.prefix.clone(), constructs);
        }
    }
    Ok(out)
}

/// Resolve `slice_dir`'s `gmeow:Slice` IRI from its `manifest.ttl` — a thin `pub`
/// wrapper over [`report::slice_iri_of`] so a caller outside this module (the coat
/// distinctiveness guard, and consumer-crate tests) resolves the same slice IRI
/// [`measure_repo_residues`] does, without a second re-implementation of manifest
/// resolution.
///
/// # Errors
/// As [`report::slice_iri_of`]: a message if the manifest cannot be read or
/// declares no `gmeow:Slice`.
pub fn slice_iri_of_dir(slice_dir: &Path) -> gmeow_errors::Result<String> {
    report::slice_iri_of(slice_dir)
}

/// Measure `vocab`'s ungrounded residue CONSTRUCT SET over an ALREADY-READ set of TTL
/// texts, merged into one dataset — the in-memory counterpart to
/// [`measure_surface_residue_constructs`] (which reads the same surfaces off disk).
/// This is the SAME counter ([`counting::residue_constructs_for_surface`]) fed bytes
/// instead of files, so "measured" can never diverge between the two carriers.
///
/// SURFACE-NORMALIZED MEASUREMENT: `surface_iri` is a free parameter, so measuring one
/// slice's bytes AS IF they sat at a DIFFERENT (destination) surface is exactly this
/// one call with the destination IRI — no second API. That matters because residue is
/// NOT conserved under relocation: `counting::is_bridge_exempt` is exempt only when
/// `surface_iri == vocab.owner`, so crossing the owner boundary creates or destroys
/// residue with no authoring at all. See [`counting::RelocationReason`] and
/// [`relocation_reasons_over_texts`].
///
/// # Errors
/// HARD-FAILS (never falls back to an empty residue) if any text fails to parse as
/// Turtle, or if merging the parsed datasets fails — this is the gate path, so a
/// broken surface must stop the sweep, never silently score as clean.
pub fn residue_constructs_over_texts(
    texts: &[String],
    vocab: &ProjectionVocabulary,
    surface_iri: &str,
) -> gmeow_errors::Result<Vec<counting::Construct>> {
    let ds = residue_dataset_from_texts(texts)?;
    Ok(counting::residue_constructs_for_surface(
        &ds,
        vocab,
        surface_iri,
    ))
}

/// The COUNT of [`residue_constructs_over_texts`] — a pure `.len()` projection of the
/// one construct set, never a second enumeration.
///
/// # Errors
/// As [`residue_constructs_over_texts`].
pub fn residue_over_texts(
    texts: &[String],
    vocab: &ProjectionVocabulary,
    surface_iri: &str,
) -> gmeow_errors::Result<u64> {
    Ok(residue_constructs_over_texts(texts, vocab, surface_iri)?.len() as u64)
}

/// Parse and union owned TTL `texts` into one frozen dataset — the shared reader behind
/// [`residue_constructs_over_texts`] and [`relocation_reasons_over_texts`].
fn residue_dataset_from_texts(texts: &[String]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    dataset_from_texts(&refs)
}

/// Explain, per relocation-invariant anchor IRI, why `vocab`'s residue over
/// `source_texts` (bytes as they sat on `source_surface_iri`) is NOT conserved when its
/// constructs are attributed to `destination_surface_iri` in the destination surface's
/// own bytes (`destination_texts`).
///
/// A thin text-carrier adapter over [`counting::relocation_reasons`] — the reasons are
/// COMPUTED from three real measurements of the two datasets, never inferred from a
/// count delta. The returned codes are [`counting::RelocationReason::code`].
///
/// # Errors
/// HARD-FAILS if either side's text fails to parse as Turtle or cannot be frozen.
pub fn relocation_reasons_over_texts(
    source_texts: &[String],
    source_surface_iri: &str,
    destination_texts: &[String],
    destination_surface_iri: &str,
    vocab: &ProjectionVocabulary,
) -> gmeow_errors::Result<
    std::collections::BTreeMap<String, std::collections::BTreeSet<counting::RelocationReason>>,
> {
    let source = residue_dataset_from_texts(source_texts)?;
    let destination = residue_dataset_from_texts(destination_texts)?;
    Ok(counting::relocation_reasons(
        &source,
        source_surface_iri,
        &destination,
        destination_surface_iri,
        vocab,
    ))
}

/// Explain, per relocation-invariant anchor IRI, why `vocab`'s residue is NOT
/// conserved when constructs move from the authoring surfaces at `source_paths`
/// (attributed to `source_surface_iri`) into those at `destination_paths`
/// (attributed to `destination_surface_iri`) — the FILE-carrier counterpart to
/// [`relocation_reasons_over_texts`], sharing the SAME
/// [`counting::relocation_reasons`] core.
///
/// The ratchet gate's driver calls this: its source side is the MATERIALIZED merge-base
/// tree (plain files, read through the very same [`ratchet_surface_paths`] scanner the
/// working tree uses) and its destination side is the working tree, so a refusal can
/// name the real reason a declared relocation failed to conserve residue rather than
/// merely reporting a count delta.
///
/// # Errors
/// HARD-FAILS if any path is unreadable or fails to parse as Turtle.
pub fn relocation_reasons_for_surfaces(
    source_paths: &[PathBuf],
    source_surface_iri: &str,
    destination_paths: &[PathBuf],
    destination_surface_iri: &str,
    vocab: &ProjectionVocabulary,
) -> gmeow_errors::Result<
    std::collections::BTreeMap<String, std::collections::BTreeSet<counting::RelocationReason>>,
> {
    let src_refs: Vec<&Path> = source_paths.iter().map(PathBuf::as_path).collect();
    let dst_refs: Vec<&Path> = destination_paths.iter().map(PathBuf::as_path).collect();
    let source = dataset_from_paths(&src_refs)?;
    let destination = dataset_from_paths(&dst_refs)?;
    Ok(counting::relocation_reasons(
        &source,
        source_surface_iri,
        &destination,
        destination_surface_iri,
        vocab,
    ))
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

/// Score an external slice held ENTIRELY IN MEMORY against the standards flattened
/// from a bundle (reuse one [`BundleStandards`] across many slices).
///
/// This is the terminal external-scoring call: no repo checkout, no slice directory,
/// nothing on a filesystem at all — the bundle bytes and the slice's own file map are
/// the complete input. The directory entry points use the same scoring implementation
/// while retaining their real manifest/module paths for diagnostics, so a slice scores
/// identically however its bytes were obtained without discarding actionable source
/// identity.
///
/// # Errors
/// As [`report::score_slice_files_with_standard`].
pub fn score_external_slice_from_files(
    std: &BundleStandards,
    files: &std::collections::BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<report::SliceReport> {
    report::score_slice_files_with_standard(
        files,
        &std.standard,
        ScoringEnv::Bundle(std.gmn_dict.clone()),
    )
}

/// Score an external slice DIRECTORY against the standards flattened from a bundle
/// (reuse one [`BundleStandards`] across many slices). Reads the bundle-carried
/// standards + the external `slice_dir` ONLY — never a repo checkout. This thin on-disk
/// convenience reads the directory into the same file-map scorer while retaining
/// `slice_dir` as the root of file-level diagnostic locations.
///
/// # Errors
/// As [`report::slice_files_from_dir`] and [`report::score_slice_with_standard`].
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

/// Score an external slice held in memory straight from bundle bytes — the one-slice
/// convenience over [`BundleStandards::from_gts`] + [`score_external_slice_from_files`].
/// The map twin of [`score_external_slice_bytes`], and the entry point a caller with
/// no filesystem (a browser console, a server handling an upload, a tool reading git
/// blobs) uses. Prefer the two-step form when scoring many slices from one bundle (it
/// flattens the wheel once).
///
/// # Errors
/// HARD FAILS on a corrupt wheel (as [`BundleStandards::from_gts`]) or an unscorable
/// slice — including a map carrying no `manifest.ttl` (as
/// [`report::slice_iri_of_files`]).
pub fn score_external_slice_files(
    bundle_gts: &[u8],
    files: &std::collections::BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<report::SliceReport> {
    score_external_slice_from_files(&BundleStandards::from_gts(bundle_gts)?, files)
}

/// Score an external slice directory straight from bundle bytes — the on-disk twin of
/// [`score_external_slice_files`]: flatten the bundle standard once, then use the
/// directory scorer so file-level diagnostics retain the supplied source paths. Both
/// forms share one scoring implementation. Prefer the two-step form when scoring many
/// slices from one bundle.
///
/// # Errors
/// HARD FAILS on a corrupt wheel (as [`BundleStandards::from_gts`]), an unreadable
/// directory, or an unscorable slice (as [`score_external_slice_files`]).
pub fn score_external_slice_bytes(
    bundle_gts: &[u8],
    slice_dir: &Path,
) -> gmeow_errors::Result<report::SliceReport> {
    let standards = BundleStandards::from_gts(bundle_gts)?;
    score_external_slice(&standards, slice_dir)
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    /// [`SCORER_CRATE_ROOTS`] must be EXACTLY `gmeow-slice-quality`'s transitive
    /// `path = "../…"` dependency closure, re-derived here from the workspace manifests.
    ///
    /// A path dependency carries no `Cargo.lock` checksum, so a crate outside the folded
    /// set can change what the sweep scores while every folded input stays byte-identical
    /// — and the recorded corpus then verifies as fresh against a scorer that no longer
    /// produces it. A NEW path dependency reds here rather than silently reopening that.
    #[test]
    fn scorer_dep_closure_is_fully_hashed() {
        let closure =
            build_inputs::transitive_path_dependency_dirs(Path::new(env!("CARGO_MANIFEST_DIR")))
                .into_iter()
                .map(|path| {
                    path.file_name()
                        .expect("crate directory has a name")
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<std::collections::BTreeSet<_>>();

        let hashed: std::collections::BTreeSet<String> = SCORER_CRATE_ROOTS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(
            hashed, closure,
            "SCORER_CRATE_ROOTS must be exactly gmeow-slice-quality's transitive \
             path-dependency closure — a path dependency carries no Cargo.lock checksum, so \
             an unhashed one lets the scorer change while the recorded corpus still verifies \
             as fresh"
        );
    }

    /// The CODE half of the freshness witness is load-bearing: editing a scorer source
    /// file must move the fingerprint even though no scored `.ttl` changed.
    #[test]
    fn a_scorer_source_edit_moves_the_fingerprint() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = tmp.path();
        // A minimal tree: one scored source (the rubric module) and one scorer source.
        let rubric = root.join(RUBRIC_MODULE);
        std::fs::create_dir_all(rubric.parent().expect("rubric parent")).expect("mkdir rubric");
        std::fs::write(&rubric, b"# rubric\n").expect("write rubric");
        let scorer_src = root.join("crates").join("slice-quality").join("src");
        std::fs::create_dir_all(&scorer_src).expect("mkdir scorer src");
        let unit = scorer_src.join("lib.rs");
        std::fs::write(&unit, b"// v1\n").expect("write scorer");

        let before = scored_input_fingerprint(root).expect("fingerprint v1");
        std::fs::write(&unit, b"// v2: the axis now scores differently\n").expect("rewrite scorer");
        let after = scored_input_fingerprint(root).expect("fingerprint v2");
        assert_ne!(
            before, after,
            "a scorer source edit must move the freshness fingerprint: a corpus produced by \
             the old scorer does not describe what the new one would record"
        );
    }
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

/// The text-carrier residue API: the ONE counter's `.len()` projection, the
/// SURFACE-NORMALIZED base measurement (the same bytes measured as if they sat at a
/// different destination surface), and the relocation reason codes derived from it.
#[cfg(test)]
mod residue_text_carrier_tests {
    use super::{
        ProjectionVocabulary, RelocationReason, relocation_reasons_over_texts,
        residue_constructs_over_texts, residue_over_texts,
    };
    use crate::model::CountKind;

    const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
    const KERNEL: &str = "https://blackcatinformatics.ca/gmeow/slices/kernel";

    fn prefixes() -> &'static str {
        "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix gufo: <https://w3id.org/gufo#> .\n"
    }

    fn text(body: &str) -> Vec<String> {
        vec![format!("{}{body}", prefixes())]
    }

    fn gufo_vocab() -> ProjectionVocabulary {
        ProjectionVocabulary {
            prefix: "gufo".to_owned(),
            namespaces: vec!["https://w3id.org/gufo#".to_owned()],
            subsumed_by: LOGIC_NS.to_owned(),
            owner: LOGIC_NS.to_owned(),
            count_kind: CountKind::TypedAxiom,
            default_ceiling: 0,
            preservation: "SoundUnderApproximation".to_owned(),
            alignment_predicates: Vec::new(),
            counted_predicates: Vec::new(),
        }
    }

    /// A validated grounding correspondence: exempt on the vocabulary's OWNER surface,
    /// plain residue anywhere else.
    fn grounding_cell() -> Vec<String> {
        text(
            r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
        )
    }

    #[test]
    fn count_over_texts_is_the_construct_sets_length() {
        let texts = text("gmeow:A a sh:NodeShape . gmeow:B a sh:NodeShape .");
        let vocab = crate::counting::shacl_vocab();
        let constructs = residue_constructs_over_texts(&texts, &vocab, &vocab.owner).unwrap();
        assert_eq!(constructs.len(), 2);
        assert_eq!(
            residue_over_texts(&texts, &vocab, &vocab.owner).unwrap(),
            constructs.len() as u64
        );
    }

    #[test]
    fn base_bytes_can_be_measured_at_a_destination_surface() {
        // The SAME base bytes measured at the OWNER surface and at a destination slice
        // surface differ — residue is not conserved across the owner boundary, and the
        // existing `surface_iri` parameter is all it takes to see that.
        let base = grounding_cell();
        let vocab = gufo_vocab();
        assert_eq!(residue_over_texts(&base, &vocab, &vocab.owner).unwrap(), 0);
        assert_eq!(residue_over_texts(&base, &vocab, KERNEL).unwrap(), 1);
    }

    #[test]
    fn relocation_reasons_over_texts_names_the_owner_boundary_shift() {
        let base = grounding_cell();
        let working = grounding_cell();
        let vocab = gufo_vocab();
        let reasons =
            relocation_reasons_over_texts(&base, &vocab.owner, &working, KERNEL, &vocab).unwrap();
        let codes: Vec<&str> = reasons
            .values()
            .flat_map(|set| set.iter().map(|r| r.code()))
            .collect();
        assert_eq!(codes, vec!["exemption-shift-owner-boundary"], "{reasons:?}");
        assert!(reasons.contains_key("https://blackcatinformatics.ca/gmeow/MyKind"));
    }

    #[test]
    fn relocation_reasons_over_texts_names_an_orphaned_grounding() {
        let base = text(
            "gmeow:S a sh:NodeShape ; logic:formalizes logic:sAxiom .\n\
             logic:sAxiom a logic:Formula .",
        );
        let working = text("gmeow:S a sh:NodeShape ; logic:formalizes logic:sAxiom .");
        let vocab = crate::counting::shacl_vocab();
        let reasons =
            relocation_reasons_over_texts(&base, &vocab.owner, &working, KERNEL, &vocab).unwrap();
        assert_eq!(
            reasons
                .get("https://blackcatinformatics.ca/gmeow/S")
                .map(|set| set.iter().copied().collect::<Vec<_>>()),
            Some(vec![RelocationReason::GroundingOrphaned]),
            "{reasons:?}"
        );
    }

    #[test]
    fn a_broken_base_surface_hard_fails_rather_than_measuring_zero() {
        let broken = vec!["this is not turtle {{{".to_owned()];
        let vocab = crate::counting::shacl_vocab();
        assert!(
            residue_constructs_over_texts(&broken, &vocab, &vocab.owner).is_err(),
            "an unparsable surface must HARD FAIL, never silently score as clean"
        );
    }
}
