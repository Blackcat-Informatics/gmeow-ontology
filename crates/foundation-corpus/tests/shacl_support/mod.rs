// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-ontology native SHACL helpers for the foundation-corpus acceptance test,
//! ported from `crates/validate/tests/conformance_support/mod.rs`.
//!
//! `repo_root()` resolves from THIS crate: `CARGO_MANIFEST_DIR` is
//! `crates/foundation-corpus`, so `../..` is the repository root that contains
//! `shapes/`, `generated/`, and `slices/`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use purrdf::RdfDataset;
use purrdf::parse_dataset;
use purrdf::shapes::engine::validate_dataset_graphs;
use purrdf::shapes::report::{Severity, ValidationReport};

// ── Repo-root resolution ──────────────────────────────────────────────────────

/// Absolute path to the repository root (`crates/foundation-corpus/../../`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..") // crates/
        .join("..") // repo root
        .canonicalize()
        .expect("repo root must be resolvable")
}

// ── Shapes corpus assembly ────────────────────────────────────────────────────

/// DSL-specific shapes files excluded from the domain test corpus.
pub const DSL_SHAPE_FILENAMES: &[&str] = &[
    "mapping-dsl-shapes.ttl",
    "statement-dsl-shapes.ttl",
    "test-dsl-shapes.ttl",
    "slice-manifest-shapes.ttl",
    // The derived validation-shape surface is a DECLARED ValidationOnly projection carried in
    // gmeow.gts but NOT enforced (an open-world someValuesFrom reading over-flags valid data);
    // excluded exactly as `purrdf::shapes::shape_union::EXCLUDED` excludes it.
    "validation-shapes.ttl",
];

/// Collect `shapes/*.ttl` paths, sorted, excluding DSL-specific files.
pub fn collect_shapes_dir(root: &Path) -> Vec<PathBuf> {
    let dir = root.join("shapes");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read shapes/: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("ttl")
                && !DSL_SHAPE_FILENAMES
                    .iter()
                    .any(|x| p.file_name().and_then(|n| n.to_str()) == Some(x))
        })
        .collect();
    paths.sort();
    paths
}

/// Collect `generated/shapes/*.ttl` paths, sorted.
pub fn collect_generated_shapes(root: &Path) -> Vec<PathBuf> {
    let dir = root.join("generated").join("shapes");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "no generated shapes under generated/shapes/ — \
                 run `gmeow-dev sync --mode update --outputs generated frame-shapes` (P11 enforcement lives there): {e}"
            )
        })
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("ttl")
                && !DSL_SHAPE_FILENAMES
                    .iter()
                    .any(|x| p.file_name().and_then(|n| n.to_str()) == Some(x))
        })
        .collect();
    assert!(
        !paths.is_empty(),
        "no generated shapes under generated/shapes/ — \
         run `gmeow-dev sync --mode update --outputs generated frame-shapes` (P11 enforcement lives there)"
    );
    paths.sort();
    paths
}

/// Collect per-slice `shapes.ttl` files from `slices/`, sorted.
pub fn collect_slice_shapes(root: &Path) -> Vec<PathBuf> {
    let slices_dir = root.join("slices");
    let mut paths: Vec<PathBuf> = Vec::new();
    collect_slice_shapes_recursive(&slices_dir, &mut paths);
    paths.sort();
    paths
}

pub fn collect_slice_shapes_recursive(dir: &Path, paths: &mut Vec<PathBuf>) {
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in read.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            let candidate = path.join("shapes.ttl");
            if candidate.is_file() {
                paths.push(candidate);
            }
            collect_slice_shapes_recursive(&path, paths);
        }
    }
}

/// Read a Turtle file as raw UTF-8 text.
pub fn read_ttl(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Assemble the full SHACL shapes corpus as one concatenated Turtle string.
///
///   1. `shapes/gmeow-shapes.ttl` (the base shapes file, listed first),
///   2. other `shapes/*.ttl` excluding DSL-specific files,
///   3. `generated/shapes/*.ttl` (frame-relativity shapes, Principle 11),
///   4. per-slice `shapes.ttl` files.
pub fn whole_shapes_ttl() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| {
        let root = repo_root();
        let mut parts: Vec<String> = Vec::new();

        // 1. Base shapes file first.
        parts.push(read_ttl(&root.join("shapes").join("gmeow-shapes.ttl")));

        // 2. Additional domain shapes (excludes gmeow-shapes.ttl — already added —
        //    and DSL files).
        let base_name = "gmeow-shapes.ttl";
        for path in collect_shapes_dir(&root) {
            if path.file_name().and_then(|n| n.to_str()) != Some(base_name) {
                parts.push(read_ttl(&path));
            }
        }

        // 3. Generated shapes.
        for path in collect_generated_shapes(&root) {
            parts.push(read_ttl(&path));
        }

        // 4. Per-slice shapes.
        for path in collect_slice_shapes(&root) {
            parts.push(read_ttl(&path));
        }

        parts.join("\n")
    })
}

// ── Merged-ontology helpers ───────────────────────────────────────────────────

/// Collect every `module.ttl` file under `slices/` recursively.
fn collect_module_ttls(dir: &Path, paths: &mut Vec<PathBuf>) {
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in read.filter_map(|e| e.ok()) {
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

/// The merged ontology as a single frozen [`RdfDataset`].
///
/// Parses every `slices/*/*/module.ttl` (recursively, files literally named
/// `module.ttl`) through the canonical native codec (lenient on the private-use
/// `@x-gmeow-*` language tags) and unions them into one dataset — the oxigraph-free
/// twin of the old "load into a Store and dump N-Triples" path.
pub fn base_ontology() -> Arc<RdfDataset> {
    static CACHE: OnceLock<Arc<RdfDataset>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let root = repo_root();
            let slices_dir = root.join("slices");
            let mut module_paths: Vec<PathBuf> = Vec::new();
            collect_module_ttls(&slices_dir, &mut module_paths);
            module_paths.sort();

            let parsed: Vec<Arc<RdfDataset>> = module_paths
                .iter()
                .map(|path| {
                    let bytes = std::fs::read(path)
                        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
                    parse_dataset(&bytes, "text/turtle", None)
                        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
                })
                .collect();
            let refs: Vec<&RdfDataset> = parsed.iter().map(AsRef::as_ref).collect();
            Arc::new(RdfDataset::union(&refs))
        })
        .clone()
}

/// Validate the merged ontology unioned with `fixture_ttl` against `whole_shapes_ttl()`.
///
/// The fixture is parsed through the native codec and unioned with [`base_ontology`]
/// (blank scopes standardized apart), then validated IR-natively via
/// `validate_dataset_graphs` — no oxigraph Store, no N-Triples text round-trip.
pub fn validate_with_ontology(fixture_ttl: &str) -> ValidationReport {
    let fixture = parse_dataset(fixture_ttl.as_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("parse fixture turtle: {e}"));
    let combined = RdfDataset::union(&[base_ontology().as_ref(), fixture.as_ref()]);
    validate_dataset_graphs(&combined, whole_shapes_ttl())
        .expect("validate_dataset_graphs must not error")
}

// ── Report helpers ────────────────────────────────────────────────────────────

/// Collect human-readable messages for results at `Violation` severity.
pub fn violations(report: &ValidationReport) -> Vec<String> {
    report
        .results
        .iter()
        .filter(|r| r.severity == Severity::Violation)
        .map(|r| r.message.clone().unwrap_or_default())
        .collect()
}

/// Return `true` when there are no `Violation`-severity results.
pub fn ok(report: &ValidationReport) -> bool {
    violations(report).is_empty()
}
