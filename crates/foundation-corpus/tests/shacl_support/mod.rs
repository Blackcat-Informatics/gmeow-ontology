// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-ontology native SHACL helpers for the foundation-corpus acceptance test
//! (#944), ported from `crates/validate/tests/conformance_support/mod.rs`.
//!
//! `repo_root()` resolves from THIS crate: `CARGO_MANIFEST_DIR` is
//! `crates/foundation-corpus`, so `../..` is the repository root that contains
//! `shapes/`, `generated/`, and `slices/`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gmeow_shacl::engine::validate_graphs;
use gmeow_shacl::report::{Severity, ValidationReport};
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;

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
                 run `gmeow regenerate frame-shapes` (P11 enforcement lives there): {e}"
            )
        })
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ttl"))
        .collect();
    assert!(
        !paths.is_empty(),
        "no generated shapes under generated/shapes/ — \
         run `gmeow regenerate frame-shapes` (P11 enforcement lives there)"
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

/// Merged ontology as a single N-Triples string.
///
/// Parses every `slices/*/*/module.ttl` (recursively, files literally named
/// `module.ttl`) into one oxigraph Store and dumps as N-Triples.
pub fn base_ontology_nt() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| {
        let root = repo_root();
        let slices_dir = root.join("slices");
        let mut module_paths: Vec<PathBuf> = Vec::new();
        collect_module_ttls(&slices_dir, &mut module_paths);
        module_paths.sort();

        let store = Store::new().expect("in-memory store creation is infallible");
        for path in &module_paths {
            let ttl = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            // Use lenient parser to accept private-use language tags.
            if let Err(e) = store.load_from_reader(
                RdfParser::from_format(RdfFormat::Turtle).lenient(),
                ttl.as_bytes(),
            ) {
                eprintln!(
                    "warning: lenient Turtle parse of {} had errors: {e}",
                    path.display()
                );
            }
        }

        let mut buf: Vec<u8> = Vec::new();
        store
            .dump_graph_to_writer(
                oxigraph::model::GraphNameRef::DefaultGraph,
                RdfFormat::NTriples,
                &mut buf,
            )
            .expect("N-Triples serialisation is infallible");
        String::from_utf8(buf).expect("oxigraph N-Triples output is valid UTF-8")
    })
}

/// Validate `base_ontology_nt() + "\n" + fixture_nt` against `whole_shapes_ttl()`.
pub fn validate_with_ontology(fixture_nt: &str) -> ValidationReport {
    let combined = format!("{}\n{}", base_ontology_nt(), fixture_nt);
    validate_graphs(&combined, whole_shapes_ttl()).expect("validate_graphs must not error")
}

// ── Fixture helpers ───────────────────────────────────────────────────────────

/// Parse an inline Turtle string into an oxigraph store and emit as N-Triples.
///
/// Uses the lenient parser (same as `gmeow_shacl::engine::validate_graphs`) so
/// private-use `@x-gmeow-*` language tags are accepted.
pub fn ttl_str_to_nt(ttl: &str) -> String {
    let store = Store::new().expect("in-memory store creation is infallible");
    store
        .load_from_reader(
            RdfParser::from_format(RdfFormat::Turtle).lenient(),
            ttl.as_bytes(),
        )
        .unwrap_or_else(|e| panic!("Turtle parse failed: {e}\nInput:\n{ttl}"));
    let mut buf: Vec<u8> = Vec::new();
    store
        .dump_graph_to_writer(
            oxigraph::model::GraphNameRef::DefaultGraph,
            RdfFormat::NTriples,
            &mut buf,
        )
        .expect("N-Triples serialisation is infallible");
    String::from_utf8(buf).expect("oxigraph N-Triples output is valid UTF-8")
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
