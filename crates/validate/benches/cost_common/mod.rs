// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared build helpers for the whole-ontology-union conformance cost-partition
//! benches (`conformance_union_cost_iai` and `conformance_union_cost_alloc`).
//!
//! These reconstruct — using **owned locals only** — exactly the work the
//! `conformance_support` test helpers do behind their process-global `OnceLock`
//! caches: build the merged ontology dataset from every `slices/**/module.ttl`,
//! compile the live production shape union, and validate a graph against it. The
//! benches deliberately DO NOT touch `conformance_support` (a `tests/`-only module,
//! and its `OnceLock`s would silently attribute first-touch cache init to whichever
//! measured region hit them first, polluting the setup-vs-scan partition).
//!
//! The partition the benches measure, all against the SAME production shape corpus
//! so the only thing that varies is the size of the validated data graph:
//! - `S`  — build the merged ontology + compile the production shape union (one-time
//!   setup; under nextest's process-per-test model this is re-paid by every twin).
//! - `V_ontology_only` — validate the merged ontology with NO fixture (the invariant,
//!   fixture-independent whole-graph SHACL scan; a disk cache can never remove this).
//! - `V_full` — validate the merged ontology UNIONED with a representative twin fixture.
//! - `V_marginal = V_full − V_ontology_only` — the true per-fixture marginal cost.
//! - `V_fixture` — validate the tiny fixture ALONE (the ~0.05 s on-gate anchor).

// Each bench target includes its own copy of this module via `#[path]`, and no
// single bench exercises every helper (the iai bench never unions a fixture), so
// per-bench "unused" is expected — mirror the conformance_support harness pattern.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::shapes::engine::{project_dataset, validate_dataset, validate_projected_dataset};
use purrdf::shapes::report::ValidationReport;
use purrdf::shapes::shapes::Shapes;
use purrdf::{
    RdfDataset, RdfDatasetBuilder, RdfQuad, flat_dataset_from_quads, flat_rdf_quads_from_dataset,
    parse_dataset,
};

/// A representative whole-ontology-union twin fixture: the single-decided-label
/// conforming `gmeow:AffectDecision` (mirrors
/// `twin_affect_decision_single_decided_label_conforms`). Any tiny valid fixture
/// yields essentially the same `V_full` — the scan cost is dominated by the merged
/// ontology, not the fixture — so `V_marginal` quantifies exactly that.
pub const FIXTURE_TTL: &str = concat!(
    "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    "@prefix gmeow-hf: <https://blackcatinformatics.ca/gmeow-registry/hf/> .\n",
    "@prefix ex: <https://example.org/affect/> .\n",
    "ex:d a gmeow:AffectDecision ; gmeow:vantage ex:run ; gmeow:observedFeature ex:t ; \
     gmeow:decidedLabel gmeow-hf:ekmanJoy ; gmeow:decisionCrossedThreshold false ; \
     gmeow:derivedByFunction gmeow:fnArgmax .\n\
     ex:run a gmeow:Entity .\n"
);

/// Absolute path to the repository root (`crates/validate/../../`). Mirrors
/// `conformance_support::repo_root` without depending on that `tests/`-only module.
#[must_use]
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..") // crates/
        .join("..") // repo root
        .canonicalize()
        .expect("repo root must be resolvable")
}

/// Re-home every quad to the default graph and freeze — the native twin of
/// `conformance_support::flatten_to_default_graph`.
fn flatten_to_default_graph(dataset: &RdfDataset) -> Arc<RdfDataset> {
    let mut quads = flat_rdf_quads_from_dataset(dataset);
    for quad in &mut quads {
        quad.graph_name = None;
    }
    flat_dataset_from_quads(&quads).expect("flattened dataset must freeze")
}

/// Recursively collect every file literally named `module.ttl` under `dir`.
fn collect_module_ttls(dir: &Path, paths: &mut Vec<PathBuf>) {
    let read = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read dir {}: {e}", dir.display()));
    for entry in read {
        let entry = entry.unwrap_or_else(|e| panic!("read dir entry under {}: {e}", dir.display()));
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            let candidate = path.join("module.ttl");
            if candidate.is_file() {
                paths.push(candidate);
            }
            collect_module_ttls(&path, paths);
        }
    }
}

/// Build the merged ontology dataset (flattened to the default graph) from every
/// `slices/**/module.ttl`. Owned-local reconstruction of
/// `conformance_support::base_ontology_dataset` — this is the setup cost `S_onto`.
#[must_use]
pub fn build_merged_ontology() -> Arc<RdfDataset> {
    let root = repo_root();
    let mut module_paths: Vec<PathBuf> = Vec::new();
    collect_module_ttls(&root.join("slices"), &mut module_paths);
    module_paths.sort();

    // Merge through the builder so each source dataset gets a fresh blank-node scope
    // (standardize-apart) — identical to base_ontology_dataset.
    let mut builder = RdfDatasetBuilder::new();
    for path in &module_paths {
        let ttl = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let ds = parse_dataset(ttl.as_bytes(), "text/turtle", None)
            .unwrap_or_else(|e| panic!("parse {}: {e:?}", path.display()));
        builder.push_dataset(&ds);
    }
    let merged = builder
        .freeze()
        .expect("merged ontology dataset must freeze");
    flatten_to_default_graph(&merged)
}

/// Compile the live production shape union (`gmeow validate`'s corpus, including
/// `generated/shapes/validation-shapes.ttl`) — the setup cost `S_shapes`. Calls
/// `purrdf::shapes::shape_union::load_shapes` directly (the FULL production union);
/// the conformance test harness instead uses `conformance_support::conformance_shapes`,
/// which drops `result-shapes.ttl` for speed.
#[must_use]
pub fn load_production_shapes() -> Shapes {
    let (_store, shapes) = purrdf::shapes::shape_union::load_shapes(&repo_root())
        .expect("load production SHACL shape union");
    shapes
}

fn load_shape_files(files: &[PathBuf]) -> Shapes {
    let mut prefixes = BTreeMap::new();
    let mut datasets = Vec::with_capacity(files.len());
    for file in files {
        let bytes = std::fs::read(file)
            .unwrap_or_else(|e| panic!("read shape file {}: {e}", file.display()));
        let text = std::str::from_utf8(&bytes)
            .unwrap_or_else(|e| panic!("shape file {} is not UTF-8: {e}", file.display()));
        let dataset = parse_dataset(&bytes, "text/turtle", None)
            .unwrap_or_else(|e| panic!("parse shape file {}: {e}", file.display()));
        datasets.push(dataset);
        for (prefix, namespace) in purrdf::shapes::text_ingest::extract_prefixes(text) {
            prefixes.insert(prefix, namespace);
        }
    }
    let refs: Vec<&RdfDataset> = datasets.iter().map(AsRef::as_ref).collect();
    let merged = Arc::new(RdfDataset::union(&refs));
    purrdf::shapes::shapes::from_dataset_with_prefixes(
        &merged,
        &prefixes.into_iter().collect::<Vec<_>>(),
    )
    .expect("parse selected production shapes")
}

/// Production shape corpus excluding the generated procedural-constraint
/// projection, used only to attribute the whole-graph scan cost.
#[must_use]
pub fn load_shapes_without_procedural() -> Shapes {
    let files = purrdf::shapes::shape_union::shape_files(&repo_root())
        .expect("enumerate production SHACL shape union")
        .into_iter()
        .filter(|path| {
            path.file_name().and_then(|name| name.to_str()) != Some("procedural-constraints.ttl")
        })
        .collect::<Vec<_>>();
    load_shape_files(&files)
}

/// The generated procedural-constraint projection in isolation, used only to
/// attribute the whole-graph scan cost.
#[must_use]
pub fn load_procedural_shapes() -> Shapes {
    load_shape_files(&[repo_root().join("generated/shapes/procedural-constraints.ttl")])
}

/// Parse the representative fixture into default-graph quads.
#[must_use]
pub fn fixture_quads() -> Vec<RdfQuad> {
    let ds = parse_dataset(FIXTURE_TTL.as_bytes(), "text/turtle", None)
        .expect("representative fixture parses");
    let flat = flatten_to_default_graph(&ds);
    flat_rdf_quads_from_dataset(&flat)
}

/// A frozen dataset holding ONLY the fixture (the tiny-data anchor `V_fixture`).
#[must_use]
pub fn fixture_only_dataset() -> Arc<RdfDataset> {
    flat_dataset_from_quads(&fixture_quads()).expect("fixture dataset must freeze")
}

/// Merge the (already-built) ontology with the fixture quads into one frozen graph
/// — the `V_full` data graph, exactly what `validate_with_ontology_shape_union`
/// validates.
#[must_use]
pub fn ontology_plus_fixture(ontology: &RdfDataset) -> Arc<RdfDataset> {
    let mut merged: Vec<RdfQuad> = flat_rdf_quads_from_dataset(ontology);
    merged.extend(fixture_quads());
    flat_dataset_from_quads(&merged).expect("merged dataset must freeze")
}

/// Validate `data` against `shapes` — the measured scan.
#[must_use]
pub fn validate(data: &RdfDataset, shapes: &Shapes) -> ValidationReport {
    validate_dataset(data, shapes).expect("native SHACL validation must succeed")
}

/// Materialize the SHACL default-graph/reifier projection separately so the
/// cost harness can distinguish transport preparation from constraint execution.
#[must_use]
pub fn project(data: &RdfDataset) -> Arc<RdfDataset> {
    project_dataset(data).expect("native SHACL projection must succeed")
}

/// Validate an already-projected graph, isolating the constraint-engine cost.
#[must_use]
pub fn validate_projected(data: Arc<RdfDataset>, shapes: &Shapes) -> ValidationReport {
    validate_projected_dataset(data, shapes).expect("native SHACL validation must succeed")
}
