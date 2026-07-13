// SPDX-License-Identifier: AGPL-3.0-only

//! Shared helpers for whole-ontology native SHACL conformance tests.
//!
//! This module centralises:
//! - Repo-root and shapes-corpus assembly helpers (mirrors Python `_shapes_turtle`).
//! - Fixture-to-N-Triples converters.
//! - Report helpers (`ok`, `violations`, `warnings`).
//! - Two validation entry-points: `validate` (fixture-only) and
//!   `validate_with_ontology` (merged ontology + fixture).
//!
//! All symbols are `pub` so sibling `#[test]` integration files can use them
//! via `mod conformance_support; use conformance_support::*;`.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use std::sync::Arc;

use purrdf::slice::rdf_query::{Dataset as SliceDataset, GraphSel, Object, Subject};

use purrdf::shapes::engine::{parse_shapes, validate_dataset};
use purrdf::shapes::report::{Severity, ValidationReport};
use purrdf::shapes::shapes::Shapes;
use purrdf::shapes::term::Term;
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, SerializeGraph, SparqlEngine,
    SparqlRequest, SparqlResult, TermValue, flat_dataset_from_quads, flat_rdf_quads_from_dataset,
    parse_dataset, serialize_dataset,
};

// ── Repo-root resolution ──────────────────────────────────────────────────────

/// Absolute path to the repository root (`crates/validate/../../`).
///
/// `CARGO_MANIFEST_DIR` is the `crates/validate` directory. Walking up two
/// levels yields the repository root that contains `shapes/`, `generated/`, and
/// `slices/`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..") // crates/
        .join("..") // repo root
        .canonicalize()
        .expect("repo root must be resolvable")
}

// ── Shapes corpus assembly ────────────────────────────────────────────────────

/// DSL-specific shapes files excluded from the domain test corpus.
///
/// Mirrors Python's `dsl_shapes` exclusion set in `gmeow_tools.validate._shapes_turtle`.
pub const DSL_SHAPE_FILENAMES: &[&str] = &[
    "mapping-dsl-shapes.ttl",
    "statement-dsl-shapes.ttl",
    "test-dsl-shapes.ttl",
    "slice-manifest-shapes.ttl",
    // The derived validation-shape surface is a DECLARED ValidationOnly projection (the OPT
    // constraint axis + the OWL-restriction reading). This exclusion is LOCAL to the
    // fixture-conformance corpus assembled here (`collect_shapes_dir`), where an open-world
    // someValuesFrom reading would over-flag hand-built fixture data. It is NOT mirrored by
    // `purrdf::shapes::shape_union::EXCLUDED`, which lists only the four DSL shape files —
    // the production shape union (`shape_union::load_shapes`) DOES enforce
    // `validation-shapes.ttl`, and the reasoning-layer cardinality bounds reach the gate
    // through it. Tests that must exercise the projected bounds validate against that
    // production union directly, not this fixture corpus.
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
///
/// Hard-fails if the directory is absent or empty — the generated frame shapes
/// are load-bearing for Principle 11 enforcement (same contract as Python).
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
         run `gmeow regenerate frame-shapes` (P11 enforcement lives there)"
    );
    paths.sort();
    paths
}

/// Collect per-slice `shapes.ttl` files from `slices/`, sorted.
///
/// Mirrors Python's `iter_slice_shape_files()`.
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
/// Replicates `gmeow_tools.validate._shapes_turtle(SHAPES_FILE)`:
///   1. `shapes/gmeow-shapes.ttl` (the base shapes file, listed first),
///   2. other `shapes/*.ttl` excluding DSL-specific files,
///   3. `generated/shapes/*.ttl` (frame-relativity shapes, Principle 11),
///   4. per-slice `shapes.ttl` files.
///
/// Cached via [`std::sync::OnceLock`] so the disk I/O happens at most once per
/// test process even when many tests run in parallel.
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
/// Parses every `slices/*/*/module.ttl` (recursively under `slices/`, files
/// literally named `module.ttl`) into the native IR, flattens every named graph into
/// the default graph, and dumps as N-Triples. Mirrors
/// `load_merged_graph(include_imports=False)`.
///
/// Cached via [`OnceLock`] so disk I/O happens at most once per test process.
pub fn base_ontology_nt() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| dataset_default_graph_to_nt(base_ontology_dataset()))
}

/// Merged ontology as a frozen native dataset (flattened to the default graph).
///
/// The native twin of [`base_ontology_nt`]. The conformance tests use it directly
/// so `validate_with_ontology` does not serialize the full ontology to N-Triples
/// and immediately parse it back for every case.
pub fn base_ontology_dataset() -> &'static Arc<RdfDataset> {
    static CACHE: OnceLock<Arc<RdfDataset>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let root = repo_root();
        let slices_dir = root.join("slices");
        let mut module_paths: Vec<PathBuf> = Vec::new();
        collect_module_ttls(&slices_dir, &mut module_paths);
        module_paths.sort();

        // Merge every module through the builder's `push_dataset`, which allocates a
        // FRESH blank-node scope per source dataset (standardize-apart, C0.2): two
        // modules that both mint `_:b0` — or two distinct `rdf:List` cons cells that
        // parsed to the same label — stay DISTINCT instead of collapsing into one
        // over-connected blank node (which corrupts blank `rdf:List`/`owl:Restriction`
        // walks: a single blank head with multiple `rdf:first` objects). A raw
        // quad-collect (`flat_rdf_quads_from_dataset` per module → one vec) has no
        // per-source scope and DOES collide, so build through the interner instead.
        let mut builder = RdfDatasetBuilder::new();
        for path in &module_paths {
            let ttl = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            // Native codec parse: lenient on private-use language tags.
            // Continue on a local parse error — some module.ttl files import
            // cross-slice IRIs that are not resolvable in the local parse; the
            // merged dataset is still built from the resolvable modules.
            if let Ok(ds) = parse_dataset(ttl.as_bytes(), "text/turtle", None) {
                builder.push_dataset(&ds);
            }
        }
        // Re-home every quad to the default graph so the default-graph-only query
        // helpers see the whole ontology. The blank labels are already standardized
        // apart (distinct qualified strings), so flattening cannot re-collide them.
        let merged = builder
            .freeze()
            .expect("merged ontology dataset must freeze");
        flatten_to_default_graph(&merged)
    })
}

/// Parsed SHACL shape model for the whole conformance corpus.
///
/// `purrdf::shapes::engine::validate_graphs` parses shapes on every call. These
/// tests repeatedly validate small fixture graphs against the same shape model,
/// so cache the parsed `Shapes` inside each test process.
pub fn whole_shapes() -> &'static Shapes {
    static CACHE: OnceLock<Shapes> = OnceLock::new();
    CACHE.get_or_init(|| parse_shapes(whole_shapes_ttl()).expect("whole SHACL shapes parse"))
}

/// Parse N-Triples into a frozen native dataset (flattened to the default graph).
fn nt_to_dataset(nt: &str) -> Arc<RdfDataset> {
    let dataset = parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .unwrap_or_else(|e| panic!("N-Triples parse failed: {e}"));
    flatten_to_default_graph(&dataset)
}

/// Validate `base_ontology + fixture_nt` against `whole_shapes()`.
///
/// Use this variant when the fixture triples rely on class/property declarations
/// from the merged ontology to pass SHACL class-constraint checks.
pub fn validate_with_ontology(fixture_nt: &str) -> ValidationReport {
    let mut merged: Vec<purrdf::RdfQuad> = flat_rdf_quads_from_dataset(base_ontology_dataset());
    let fixture = nt_to_dataset(fixture_nt);
    merged.extend(flat_rdf_quads_from_dataset(&fixture));
    let dataset = flat_dataset_from_quads(&merged).expect("merged dataset must freeze");
    validate_dataset(&dataset, whole_shapes()).expect("native SHACL validation must succeed")
}

/// Parsed SHACL shape model for the LIVE production shape union.
///
/// `purrdf::shapes::shape_union::load_shapes` assembles exactly the corpus the
/// live validator (`gmeow validate` / `stage-validate`) runs — including
/// `generated/shapes/validation-shapes.ttl`, the OWL-restriction cardinality
/// projection that [`whole_shapes`] deliberately EXCLUDES. Cached in a
/// [`OnceLock`] so the disk I/O + parse happens at most once per test process.
pub fn production_shapes() -> &'static Shapes {
    static CACHE: OnceLock<Shapes> = OnceLock::new();
    CACHE.get_or_init(|| {
        let (_store, shapes) = purrdf::shapes::shape_union::load_shapes(&repo_root())
            .expect("load production SHACL shape union");
        shapes
    })
}

/// Validate `base_ontology + fixture` against the LIVE production shape union
/// (`purrdf::shapes::shape_union::load_shapes`, what `gmeow validate` /
/// `stage-validate` run) — this INCLUDES generated/shapes/validation-shapes.ttl (the
/// OWL-derived cardinality projection), unlike `whole_shapes()`.
pub fn validate_with_ontology_shape_union(fixture_nt: &str) -> ValidationReport {
    let mut merged: Vec<purrdf::RdfQuad> = flat_rdf_quads_from_dataset(base_ontology_dataset());
    let fixture = nt_to_dataset(fixture_nt);
    merged.extend(flat_rdf_quads_from_dataset(&fixture));
    let dataset = flat_dataset_from_quads(&merged).expect("merged dataset must freeze");
    validate_dataset(&dataset, production_shapes()).expect("native SHACL validation must succeed")
}

// ── Fixture helpers ───────────────────────────────────────────────────────────

/// Parse a fixture `.ttl` file into an in-memory oxigraph store and emit as
/// N-Triples text, which the SHACL engine accepts as data input.
///
/// `subdir` is relative to `tests/fixtures/` (e.g. `"shapes"` or `"coverage"`).
pub fn fixture_as_nt(subdir: &str, name: &str) -> String {
    let root = repo_root();
    // The pytest test directory is `tests/` at the repo root.
    let path = root
        .join("tests")
        .join("fixtures")
        .join(subdir)
        .join(format!("{name}.ttl"));
    ttl_file_to_nt(&path)
}

/// Read a Turtle file at `path`, load it into an oxigraph store, and emit as
/// N-Triples text.
pub fn ttl_file_to_nt(path: &Path) -> String {
    let ttl = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    ttl_str_to_nt(&ttl)
}

/// Parse an inline Turtle string into the native IR (flattening every named graph
/// into the default graph) and emit as N-Triples.
///
/// Uses the native codec parser (same as `purrdf::shapes::engine::validate_graphs`) so
/// private-use `@x-gmeow-*` language tags are accepted.
pub fn ttl_str_to_nt(ttl: &str) -> String {
    let dataset = parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("Turtle parse failed: {e}\nInput:\n{ttl}"));
    let flattened = flatten_to_default_graph(&dataset);
    dataset_default_graph_to_nt(&flattened)
}

/// Re-home every quad's graph component to the default graph (`None`), returning a
/// fresh frozen flat dataset — the native twin of the prior oxigraph
/// `GraphPolicy::FlattenToDefaultGraph`.
fn flatten_to_default_graph(dataset: &RdfDataset) -> Arc<RdfDataset> {
    let mut quads = flat_rdf_quads_from_dataset(dataset);
    for quad in &mut quads {
        quad.graph_name = None;
    }
    flat_dataset_from_quads(&quads).expect("flattened dataset must freeze")
}

/// Serialize a dataset's default graph as N-Triples via the native codec. The
/// `application/n-quads` codec on the `DefaultGraph` selection emits graphless rows
/// (N-Triples) and is byte-lenient on private-use language tags.
fn dataset_default_graph_to_nt(dataset: &RdfDataset) -> String {
    let buf = serialize_dataset(dataset, "application/n-quads", SerializeGraph::DefaultGraph)
        .expect("native N-Triples serialisation is infallible");
    String::from_utf8(buf).expect("native N-Triples output is valid UTF-8")
}

// ── Report helpers ────────────────────────────────────────────────────────────

/// Collect human-readable messages for results at `Violation` severity.
///
/// Mirrors Python's `result.errors` from `ValidationResult.errors`.
pub fn violations(report: &ValidationReport) -> Vec<String> {
    report
        .results
        .iter()
        .filter(|r| r.severity == Severity::Violation)
        .map(|r| r.message.clone().unwrap_or_default())
        .collect()
}

/// Collect human-readable messages for results at `Warning` severity.
///
/// Mirrors Python's `result.warnings` from `ValidationResult.warnings`.
pub fn warnings(report: &ValidationReport) -> Vec<String> {
    report
        .results
        .iter()
        .filter(|r| r.severity == Severity::Warning)
        .map(|r| r.message.clone().unwrap_or_default())
        .collect()
}

/// Return `true` when there are no `Violation`-severity results.
///
/// Mirrors Python's `result.ok` which is `not result.errors`.  A graph with
/// only `Warning`-severity results is "ok" in the Python sense: `run_shacl`
/// routes `sh:Warning` / `sh:Info` results to `result.warnings` and leaves
/// `result.errors` empty, so `result.ok` is `True`. SHACL's own `conforms`
/// field is `False` whenever any result (including warnings) is present, so
/// we cannot use `report.conforms` for this check.
pub fn ok(report: &ValidationReport) -> bool {
    violations(report).is_empty()
}

/// Run validation of `data_nt` (N-Triples) against the whole shapes corpus.
pub fn validate(data_nt: &str) -> ValidationReport {
    let dataset = nt_to_dataset(data_nt);
    validate_dataset(&dataset, whole_shapes()).expect("native SHACL validation must succeed")
}

// ── Graph query helpers ───────────────────────────────────────────────────────

/// A thin graph-query wrapper over a frozen [`RdfDataset`].
///
/// Mirrors the small subset of rdflib graph access that the migrated Python
/// domain tests used: triple existence, `g.objects()`, `g.subjects()`, and
/// SPARQL ASK/SELECT. All lookups are default-graph only, matching the
/// `load_merged_graph(include_imports=False)` and fixture-only graphs the
/// originals operated on.
#[derive(Clone)]
pub struct GraphStore {
    ds: Arc<RdfDataset>,
    /// Lazily-built, cached blank-node-aware slice view of `ds` (see
    /// [`Self::slice_dataset`]). `Arc<OnceLock<_>>` is always `Clone` regardless
    /// of whether [`SliceDataset`] is, and every clone SHARES the same cache —
    /// correct because all clones share the same immutable `ds`. A genuinely new
    /// store (a different `ds`) must get a FRESH `Arc::new(OnceLock::new())`.
    slice_ds: Arc<OnceLock<SliceDataset>>,
}

impl GraphStore {
    /// Parse a Turtle file into a fresh store.
    pub fn parse_ttl_file(path: &Path) -> Self {
        let ttl = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        Self::parse_ttl(&ttl)
    }

    /// Parse an inline Turtle string into a fresh store.
    pub fn parse_ttl(ttl: &str) -> Self {
        let dataset = parse_dataset(ttl.as_bytes(), "text/turtle", None)
            .unwrap_or_else(|e| panic!("Turtle parse failed: {e}\nInput:\n{ttl}"));
        Self {
            ds: flatten_to_default_graph(&dataset),
            slice_ds: Arc::new(OnceLock::new()),
        }
    }

    /// Wrap an already-parsed default-graph dataset.
    pub fn from_dataset(ds: Arc<RdfDataset>) -> Self {
        Self {
            ds,
            slice_ds: Arc::new(OnceLock::new()),
        }
    }

    /// The merged ontology store (no imports), mirroring `_graph()`.
    pub fn ontology() -> Self {
        Self::from_dataset(base_ontology_dataset().clone())
    }

    /// Return a new store containing the merged ontology plus the Turtle file at
    /// `path`, flattened to the default graph. Mirrors Python `_graph() + _fixture()`.
    pub fn ontology_plus_ttl_file(path: &Path) -> Self {
        let mut quads: Vec<purrdf::RdfQuad> = flat_rdf_quads_from_dataset(base_ontology_dataset());
        let ttl = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let fixture = parse_dataset(ttl.as_bytes(), "text/turtle", None)
            .unwrap_or_else(|e| panic!("fixture parse failed: {e}\n{ttl}"));
        for mut quad in flat_rdf_quads_from_dataset(&fixture) {
            quad.graph_name = None;
            quads.push(quad);
        }
        let merged = flat_dataset_from_quads(&quads).expect("merged dataset must freeze");
        Self {
            ds: merged,
            slice_ds: Arc::new(OnceLock::new()),
        }
    }

    /// Return a new store containing the merged ontology plus each Turtle file in
    /// `paths` (parsed in the given order), flattened to the default graph. The
    /// explicit-path twin of [`Self::ontology_plus_ttl_dir`], mirroring the Python
    /// originals that closed the merged graph with two successive `graph.parse(f)`
    /// calls over a fixed pair of fixtures (e.g. the suppression canary + coarsen
    /// corpora).
    pub fn ontology_plus_ttl_files(paths: &[PathBuf]) -> Self {
        let mut quads: Vec<purrdf::RdfQuad> = flat_rdf_quads_from_dataset(base_ontology_dataset());
        for path in paths {
            let ttl = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let fixture = parse_dataset(ttl.as_bytes(), "text/turtle", None)
                .unwrap_or_else(|e| panic!("fixture parse failed: {e}\n{ttl}"));
            for mut quad in flat_rdf_quads_from_dataset(&fixture) {
                quad.graph_name = None;
                quads.push(quad);
            }
        }
        let merged = flat_dataset_from_quads(&quads).expect("merged dataset must freeze");
        Self {
            ds: merged,
            slice_ds: Arc::new(OnceLock::new()),
        }
    }

    /// Return a new store containing the merged ontology plus every `*.ttl` file in
    /// `dir` (sorted by path), flattened to the default graph. The multi-file twin of
    /// [`Self::ontology_plus_ttl_file`], mirroring the Python originals that closed the
    /// merged graph with `for f in sorted(dir.glob("*.ttl")): graph.parse(f)` (the
    /// dreaming `examples/` and music `fixtures/` corpora).
    pub fn ontology_plus_ttl_dir(dir: &Path) -> Self {
        let mut quads: Vec<purrdf::RdfQuad> = flat_rdf_quads_from_dataset(base_ontology_dataset());
        let mut ttl_paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("ttl"))
            .collect();
        ttl_paths.sort();
        for path in &ttl_paths {
            let ttl = std::fs::read_to_string(path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            let fixture = parse_dataset(ttl.as_bytes(), "text/turtle", None)
                .unwrap_or_else(|e| panic!("fixture parse failed: {e}\n{ttl}"));
            for mut quad in flat_rdf_quads_from_dataset(&fixture) {
                quad.graph_name = None;
                quads.push(quad);
            }
        }
        let merged = flat_dataset_from_quads(&quads).expect("merged dataset must freeze");
        Self {
            ds: merged,
            slice_ds: Arc::new(OnceLock::new()),
        }
    }

    fn term_id(&self, value: &TermValue) -> Option<purrdf::TermId> {
        self.ds.term_id_by_value(value)
    }

    fn iri_id(&self, iri: &str) -> Option<purrdf::TermId> {
        self.term_id(&TermValue::iri(iri))
    }

    fn subject_iri(quad: &purrdf::QuadIds, ds: &RdfDataset) -> Option<String> {
        match ds.resolve(quad.s) {
            purrdf::TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        }
    }

    fn object_iri(quad: &purrdf::QuadIds, ds: &RdfDataset) -> Option<String> {
        match ds.resolve(quad.o) {
            purrdf::TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        }
    }

    /// Return true if `<s> <p> <o>` exists in the default graph.
    ///
    /// Any component may be `None` to act as a wildcard. For literal objects use
    /// [`Self::has_literal`] or pass a typed [`TermValue`].
    pub fn has(&self, s: Option<&str>, p: Option<&str>, o: Option<&str>) -> bool {
        let s_id = s.and_then(|iri| self.iri_id(iri));
        let p_id = p.and_then(|iri| self.iri_id(iri));
        let o_id = o.and_then(|iri| self.iri_id(iri));
        // A bound IRI that is not interned cannot participate in any quad.
        if s.is_some() && s_id.is_none() {
            return false;
        }
        if p.is_some() && p_id.is_none() {
            return false;
        }
        if o.is_some() && o_id.is_none() {
            return false;
        }
        self.ds
            .quads_for_pattern(s_id, p_id, o_id, GraphMatch::Default)
            .next()
            .is_some()
    }

    /// Return true if `<s> <p> "lex"^^datatype` exists in the default graph.
    pub fn has_literal(&self, s: &str, p: &str, lexical: &str, datatype: &str) -> bool {
        let s_id = self.iri_id(s);
        let p_id = self.iri_id(p);
        let o_value = TermValue::typed_literal(lexical, datatype);
        let o_id = self.term_id(&o_value);
        self.ds
            .quads_for_pattern(s_id, p_id, o_id, GraphMatch::Default)
            .next()
            .is_some()
    }

    /// Return every IRI object of `<s> <p> ?o` in the default graph, sorted + deduped.
    pub fn objects(&self, s: &str, p: &str) -> BTreeSet<String> {
        let s_id = self.iri_id(s);
        let p_id = self.iri_id(p);
        match (s_id, p_id) {
            (Some(s), Some(p)) => self
                .ds
                .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
                .filter_map(|q| Self::object_iri(&q, &self.ds))
                .collect(),
            _ => BTreeSet::new(),
        }
    }

    /// Return every object of `<s> <p> ?o` rendered as its string form — an IRI's
    /// full string OR a literal's lexical form — sorted + deduped. The native twin of
    /// Python's `{str(o) for o in graph.objects(s, p)}`, where `str(URIRef)` is the IRI
    /// and `str(Literal)` is its lexical value. Unlike [`Self::objects`] (IRI-only),
    /// this surfaces literal-valued objects so a projection's flattened string values
    /// (`spdx:licenseId`, `cc:attributionName`, `dcterms:rights`, …) can be asserted.
    pub fn objects_lex(&self, s: &str, p: &str) -> BTreeSet<String> {
        let s_id = self.iri_id(s);
        let p_id = self.iri_id(p);
        match (s_id, p_id) {
            (Some(s), Some(p)) => self
                .ds
                .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
                .filter_map(|q| match self.ds.resolve(q.o) {
                    purrdf::TermRef::Iri(iri) => Some(iri.to_owned()),
                    purrdf::TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
                    _ => None,
                })
                .collect(),
            _ => BTreeSet::new(),
        }
    }

    /// Return every IRI subject of `?s <p> <o>` in the default graph, sorted + deduped.
    pub fn subjects(&self, p: &str, o: &str) -> BTreeSet<String> {
        let p_id = self.iri_id(p);
        let o_id = self.iri_id(o);
        match (p_id, o_id) {
            (Some(p), Some(o)) => self
                .ds
                .quads_for_pattern(None, Some(p), Some(o), GraphMatch::Default)
                .filter_map(|q| Self::subject_iri(&q, &self.ds))
                .collect(),
            _ => BTreeSet::new(),
        }
    }

    /// Return the set of `rdf:type` subjects for a given class IRI.
    pub fn subjects_of_type(&self, type_iri: &str) -> BTreeSet<String> {
        self.subjects(RDF_TYPE, type_iri)
    }

    /// The reflexive-transitive closure over `rdfs:subClassOf` edges from `start`
    /// (`start` plus all its ancestor classes). One shared walk so the domain
    /// conformance twins assert over the same closure instead of hand-rolled copies.
    pub fn subclass_closure(&self, start: &str) -> BTreeSet<String> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<String> = vec![start.to_owned()];
        while let Some(node) = stack.pop() {
            if !seen.insert(node.clone()) {
                continue;
            }
            for parent in self.objects(&node, RDFS_SUBCLASS_OF) {
                if !seen.contains(&parent) {
                    stack.push(parent);
                }
            }
        }
        seen
    }

    /// Every `gmeow:`-namespaced subject IRI whose local name starts with `primary`
    /// or `preferred` (case-insensitive) — the Principle-9 selector-term offenders.
    /// One shared whole-graph sweep so each domain twin asserts over the SAME dynamic
    /// scan instead of a hand-rolled copy that could silently narrow it.
    pub fn primary_or_preferred_terms(&self) -> Vec<String> {
        let (_vars, rows) = self.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
        let mut offenders: Vec<String> = Vec::new();
        for row in &rows {
            let Some(Some(term)) = row.first() else {
                continue;
            };
            let Some(iri) = term.as_iri() else {
                continue;
            };
            if let Some(local) = iri.strip_prefix(GMEOW_NS) {
                let lower = local.to_lowercase();
                if !local.contains('/')
                    && (lower.starts_with("primary") || lower.starts_with("preferred"))
                {
                    offenders.push(iri.to_owned());
                }
            }
        }
        offenders
    }

    // ── SPARQL entry-points (single bindings path) ────────────────────────────
    //
    // `ask`/`select`/`construct` all thread `bindings` into
    // `SparqlRequest.substitutions` — the native `initBindings` equivalent that
    // pre-binds query variables (including a blank-node focus), the same field
    // `crates/pipeline/src/cli_ops/temporal.rs` drives in production. There is ONE
    // path, not a bound/unbound pair: a case with no pre-bindings passes `&[]`.
    // `bindings` leads the query text so callers can extend the argument list
    // uniformly, and so the "pre-bind, then ask" reading matches the substitution
    // semantics. `QueryCase` (below) is the primary consumer.

    /// Run a SPARQL ASK query against the default-graph-only dataset, pre-binding
    /// `bindings` as substitutions (`&[]` for none).
    pub fn ask(&self, bindings: &[(String, TermValue)], sparql: &str) -> bool {
        let result = NativeSparqlEngine::new()
            .query(
                &self.ds,
                SparqlRequest {
                    query: sparql,
                    base_iri: None,
                    substitutions: bindings,
                },
            )
            .unwrap_or_else(|e| panic!("SPARQL ASK failed: {e}\n{sparql}"));
        match result {
            SparqlResult::Boolean(b) => b,
            other => panic!("expected boolean ASK result, got {other:?}"),
        }
    }

    /// Run a SPARQL SELECT query and return the variable names and rows of term
    /// values, pre-binding `bindings` as substitutions (`&[]` for none).
    pub fn select(
        &self,
        bindings: &[(String, TermValue)],
        sparql: &str,
    ) -> (Vec<String>, Vec<Vec<Option<TermValue>>>) {
        let result = NativeSparqlEngine::new()
            .query(
                &self.ds,
                SparqlRequest {
                    query: sparql,
                    base_iri: None,
                    substitutions: bindings,
                },
            )
            .unwrap_or_else(|e| panic!("SPARQL SELECT failed: {e}\n{sparql}"));
        match result {
            SparqlResult::Solutions {
                variables, rows, ..
            } => {
                let out_rows: Vec<Vec<Option<TermValue>>> = rows;
                (variables, out_rows)
            }
            other => panic!("expected SELECT solutions, got {other:?}"),
        }
    }

    /// Run a SPARQL CONSTRUCT query and return a fresh store over the resulting
    /// graph (flattened to the default graph), pre-binding `bindings` as
    /// substitutions (`&[]` for none). The native twin of Python
    /// `data.query(construct_text).graph`: it materialises the projection triples
    /// so the caller can assert over them with `has`/`objects`/`ask`/`objects_h`.
    pub fn construct(&self, bindings: &[(String, TermValue)], sparql: &str) -> GraphStore {
        let result = NativeSparqlEngine::new()
            .query(
                &self.ds,
                SparqlRequest {
                    query: sparql,
                    base_iri: None,
                    substitutions: bindings,
                },
            )
            .unwrap_or_else(|e| panic!("SPARQL CONSTRUCT failed: {e}\n{sparql}"));
        match result {
            SparqlResult::Graph(ds) => GraphStore {
                ds: flatten_to_default_graph(&ds),
                slice_ds: Arc::new(OnceLock::new()),
            },
            other => panic!("expected CONSTRUCT graph result, got {other:?}"),
        }
    }

    /// Number of triples in the (default-graph-only) store — the native twin of
    /// `len(graph)`. Used by [`QueryCase::construct_len`].
    pub fn triple_count(&self) -> usize {
        self.ds.quad_count()
    }

    /// Serialize the (default-graph-only) store to N-Triples text — the native twin
    /// of rdflib `Graph.serialize(format=...)`. Used by the projection leak sweeps to
    /// assert a canary substring never (or always) surfaces in a projection's output,
    /// exactly as the Python originals scanned the serialized projection string.
    pub fn to_nt(&self) -> String {
        dataset_default_graph_to_nt(&self.ds)
    }

    /// True iff the exact triple `s p o` (each a [`TermValue`]) is present in the
    /// default graph. Unlike [`Self::has`], the object may be a literal or blank
    /// term, so CONSTRUCT projections carrying literal objects can be asserted.
    pub fn contains_triple(&self, s: &TermValue, p: &TermValue, o: &TermValue) -> bool {
        let (Some(s_id), Some(p_id), Some(o_id)) =
            (self.term_id(s), self.term_id(p), self.term_id(o))
        else {
            // A term that is not interned cannot participate in any quad.
            return false;
        };
        self.ds
            .quads_for_pattern(Some(s_id), Some(p_id), Some(o_id), GraphMatch::Default)
            .next()
            .is_some()
    }

    // ── Blank-node-aware traversal (purrdf 0.3 `slice::rdf_query`) ─────────────
    //
    // The IRI-only helpers above (`has`, `objects`, `subjects`, …) drop blank
    // nodes: their `subject_iri`/`object_iri` return `None` for a bnode. The
    // `*_h` helpers below are the bnode-aware twins, built on the native
    // `purrdf::slice::rdf_query::Dataset` value model (`Subject`/`Object`), so a
    // blank `owl:Restriction`, an `rdf:List`, or an `owl:Axiom` reifier can be
    // walked. They construct the slice `Dataset` on demand from the shared frozen
    // dataset and unwrap the `Result` (panicking on a `SliceError`) exactly as
    // `ask`/`select` unwrap their SPARQL results.

    /// Build the blank-node-aware slice query surface over this store's quads.
    ///
    /// A blank [`Subject`] round-trips (is re-resolvable via `TermValue::blank`)
    /// only in a *uniquely-owned* frozen dataset: `Dataset::from_frozen` on a
    /// SHARED `Arc` (which `self.ds` always is — the store hands out clones) falls
    /// back to `push_dataset`, which scope-qualifies blank labels into a form the
    /// blank lookup can no longer resolve. So we reconstruct a fresh, uniquely-owned
    /// dataset from the store's flat quads via `from_owned_quads`; its blank labels
    /// are assigned deterministically (same quad order → same labels), so a blank
    /// `Subject` obtained from one call resolves in the next. Built once and cached
    /// per store instance (`slice_ds`), so the many `*_h` helper calls a heavy
    /// conformance test makes share one reconstruction rather than rebuilding —
    /// and, since every call returns the SAME dataset instance, the blank-node
    /// round-trip guarantee is strengthened, not merely preserved.
    fn slice_dataset(&self) -> &SliceDataset {
        self.slice_ds.get_or_init(|| {
            let quads = flat_rdf_quads_from_dataset(&self.ds);
            SliceDataset::from_owned_quads(&quads)
                .unwrap_or_else(|e| panic!("slice dataset reconstruction failed: {e}"))
        })
    }

    /// Convert an [`Object`] into the [`Subject`] you would traverse INTO: a named
    /// or blank object becomes the corresponding subject term, so a blank object
    /// (e.g. an `rdf:List` head or an `owl:Restriction`) can be walked. A literal
    /// or quoted triple term is not a subject, so this returns `None`.
    pub fn object_as_subject(object: &Object) -> Option<Subject> {
        match object {
            Object::Named(iri) => Some(Subject::Named(iri.clone())),
            Object::Blank(label) => Some(Subject::Blank(label.clone())),
            _ => None,
        }
    }

    /// Bnode-aware objects of `<subject> <pred> ?o` in the default graph. The
    /// subject may be a blank node. Panics on a `SliceError` (as `ask`/`select` do).
    pub fn objects_h(&self, subject: &Subject, pred: &str) -> Vec<Object> {
        self.slice_dataset()
            .objects_of_subject(subject, pred)
            .unwrap_or_else(|e| panic!("bnode-aware objects failed for {subject:?} <{pred}>: {e}"))
    }

    /// The first bnode-aware object of `<subject> <pred> ?o`, or `None`.
    pub fn value_h(&self, subject: &Subject, pred: &str) -> Option<Object> {
        self.objects_h(subject, pred).into_iter().next()
    }

    /// Every subject (named OR blank) of `?s a <type_iri>` in the default graph.
    pub fn subjects_of_type_h(&self, type_iri: &str) -> Vec<Subject> {
        self.slice_dataset()
            .subject_terms_of_type(type_iri)
            .unwrap_or_else(|e| panic!("bnode-aware subjects-of-type failed for <{type_iri}>: {e}"))
    }

    /// The members of the RDF Collection (`rdf:first`/`rdf:rest`/`rdf:nil`) whose
    /// head is `head`, in list order. A blank head (the usual case) is walked.
    pub fn rdf_list_h(&self, head: &Subject) -> Vec<Object> {
        self.slice_dataset()
            .rdf_list(head)
            .unwrap_or_else(|e| panic!("rdf:List walk failed for {head:?}: {e}"))
    }

    /// The members of the RDF Collection OR Container (`rdf:_n`) whose head is
    /// `head`, shape-dispatched by the slice walker.
    pub fn members_h(&self, head: &Subject) -> Vec<Object> {
        self.slice_dataset()
            .members(head)
            .unwrap_or_else(|e| panic!("container/collection walk failed for {head:?}: {e}"))
    }

    /// True iff the (typically blank) `restriction` has `owl:onProperty on_property`
    /// AND a `filler_pred` (`owl:someValuesFrom`/`owl:allValuesFrom`, passed as its
    /// full IRI) whose value is `filler_iri`. Both edges are matched on named-node
    /// objects; a blank or literal filler never matches an IRI filler.
    pub fn restriction_matches(
        &self,
        restriction: &Subject,
        on_property: &str,
        filler_pred: &str,
        filler_iri: &str,
    ) -> bool {
        let ds = self.slice_dataset();
        let on_property_matches = ds
            .objects_of_subject(restriction, OWL_ON_PROPERTY)
            .unwrap_or_else(|e| panic!("restriction owl:onProperty read failed: {e}"))
            .iter()
            .any(|o| matches!(o, Object::Named(iri) if iri == on_property));
        if !on_property_matches {
            return false;
        }
        ds.objects_of_subject(restriction, filler_pred)
            .unwrap_or_else(|e| panic!("restriction <{filler_pred}> read failed: {e}"))
            .iter()
            .any(|o| matches!(o, Object::Named(iri) if iri == filler_iri))
    }

    /// For every `owl:Axiom` reifier whose `owl:annotatedSource` is `source_iri`,
    /// return each `(owl:annotatedProperty IRI, owl:annotatedTarget object)` pair.
    ///
    /// Graph-agnostic: it scans `GraphSel::Any`, so it finds the reification
    /// whether it lives in the default graph or a named graph.
    pub fn axiom_annotations(&self, source_iri: &str) -> Vec<(String, Object)> {
        let ds = self.slice_dataset();
        let any = ds.graph(GraphSel::Any);
        let mut out = Vec::new();
        for axiom in any
            .subject_terms_of_type(OWL_AXIOM)
            .unwrap_or_else(|e| panic!("owl:Axiom enumeration failed: {e}"))
        {
            let matches_source = any
                .objects_of_subject(&axiom, OWL_ANNOTATED_SOURCE)
                .unwrap_or_else(|e| panic!("owl:annotatedSource read failed: {e}"))
                .iter()
                .any(|o| matches!(o, Object::Named(iri) if iri == source_iri));
            if !matches_source {
                continue;
            }
            let properties = any
                .objects_of_subject(&axiom, OWL_ANNOTATED_PROPERTY)
                .unwrap_or_else(|e| panic!("owl:annotatedProperty read failed: {e}"));
            let targets = any
                .objects_of_subject(&axiom, OWL_ANNOTATED_TARGET)
                .unwrap_or_else(|e| panic!("owl:annotatedTarget read failed: {e}"));
            for property in &properties {
                let Object::Named(prop_iri) = property else {
                    continue;
                };
                for target in &targets {
                    out.push((prop_iri.clone(), target.clone()));
                }
            }
        }
        out
    }
}

pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:subClassOf` — the closure edge for [`GraphStore::subclass_closure`].
pub const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// The `gmeow:` namespace base — for local-name sweeps like
/// [`GraphStore::primary_or_preferred_terms`].
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// `owl:onProperty` — the property a restriction constrains.
pub const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
/// `owl:someValuesFrom` — the existential filler of a restriction.
pub const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
/// `owl:allValuesFrom` — the universal filler of a restriction.
pub const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
/// `owl:Axiom` — the type of an OWL annotation-reification node.
pub const OWL_AXIOM: &str = "http://www.w3.org/2002/07/owl#Axiom";
/// `owl:annotatedSource` — the subject a reified axiom annotates.
pub const OWL_ANNOTATED_SOURCE: &str = "http://www.w3.org/2002/07/owl#annotatedSource";
/// `owl:annotatedProperty` — the predicate a reified axiom annotates.
pub const OWL_ANNOTATED_PROPERTY: &str = "http://www.w3.org/2002/07/owl#annotatedProperty";
/// `owl:annotatedTarget` — the object a reified axiom annotates.
pub const OWL_ANNOTATED_TARGET: &str = "http://www.w3.org/2002/07/owl#annotatedTarget";

// ── Parameterized case harness ──────────────────────────────────────────

/// Where a [`Case`]'s data graph comes from.
pub enum Source {
    /// Inline Turtle text, owned so `format!`/helper-assembled cases work in a
    /// `#[case(...)]` expression (`rstest` evaluates the expr at runtime).
    /// Parsed + re-serialized through [`ttl_str_to_nt`].
    Inline(String),
    /// Raw N-Triples fed DIRECTLY to the validator, bypassing the Turtle
    /// parse/re-serialize round-trip. Mirrors originals that called
    /// `validate(nt)` on a hand-written N-Triples literal (e.g. the
    /// case-insensitive language-tag check, whose tag casing must not be
    /// normalised by a round-trip).
    RawNt(String),
    /// `tests/fixtures/{subdir}/{name}.ttl` — see [`fixture_as_nt`].
    File {
        subdir: &'static str,
        name: &'static str,
    },
    /// Repo-root-relative path, e.g. `"tests/fixtures/software.ttl"`.
    RepoPath(&'static str),
}

/// A single parameterized SHACL conformance case.
///
/// Collapses the load→validate→assert tail shared by the ~37 `conformance_*.rs`
/// twin files into one reusable spec, driven by `rstest` `#[case]` rows.
/// Construct with [`Case::inline`], [`Case::file`], or [`Case::repo_path`],
/// refine with the builder methods, then call [`Case::run`].
///
/// Assertion semantics (the contract Task-3 parity rests on):
/// - [`Case::violations`] / [`Case::warnings`] are **subset** checks: every
///   listed substring must be present, extra messages are allowed. An empty list
///   (the default) asserts *nothing* on that channel — [`Case::run`] never
///   implicitly requires "no warnings"/"no violations".
/// - [`Case::no_warning`] asserts a warning substring is absent.
/// - [`Case::messages`] is a subset check over the UNION of violations and
///   warnings (mirror originals that joined `violations().chain(warnings())`).
/// - [`Case::violations_ci`] / [`Case::warnings_ci`] are case-insensitive subset
///   checks (mirror originals that folded `.to_lowercase()` before `.contains`).
/// - [`Case::any_violation`] / [`Case::any_violation_ci`] assert at least one of a
///   group of substrings is present (mirror originals using `||` disjunctions).
/// - [`Case::with_ontology`] routes through [`validate_with_ontology`] (merged
///   ontology) instead of [`validate`].
/// - conforms is checked through [`ok`] (warnings alone still pass), never
///   SHACL's own `conforms` field. Default expectation is "conforms";
///   [`Case::fails`] flips it.
pub struct Case {
    source: Source,
    with_ontology: bool,
    shape_union: bool,
    expect_conforms: bool,
    expected_violations: Vec<&'static str>,
    expected_warnings: Vec<&'static str>,
    expected_messages: Vec<&'static str>,
    expected_violations_ci: Vec<&'static str>,
    expected_warnings_ci: Vec<&'static str>,
    any_violations: Vec<Vec<&'static str>>,
    any_violations_ci: Vec<Vec<&'static str>>,
    forbidden_warnings: Vec<&'static str>,
    /// `(result-path IRI, constraint-component substring)` pairs each requiring at
    /// least one `Violation` result whose `sh:resultPath` is that IRI AND whose
    /// `sh:sourceConstraintComponent` contains that substring. Used to assert on the
    /// PROJECTED (message-less) cardinality shapes by component + path.
    expected_path_components: Vec<(&'static str, &'static str)>,
    /// The severity-agnostic twin of [`Self::expected_path_components`]: `(result-path
    /// IRI, constraint-component substring)` pairs each requiring at least one result
    /// AT ANY SEVERITY (`sh:Warning` OR `sh:Violation`) on that path with that
    /// component. Used for a shape whose severity is MID-MIGRATION (Warning→Violation),
    /// where the regression must hold at BOTH ends because the finding is present at
    /// either severity — see [`Case::flags_on_path`].
    expected_flag_path_components: Vec<(&'static str, &'static str)>,
    /// Message substrings each requiring at least one result AT ANY SEVERITY
    /// (`sh:Warning` OR `sh:Violation`) whose message contains it. The severity-agnostic,
    /// path-free twin of [`Self::expected_violations`]/[`Self::expected_warnings`]: used
    /// to witness a `sh:sparql` constraint (which binds NO `sh:resultPath`, so
    /// [`Self::expected_flag_path_components`] cannot anchor it) whose severity is
    /// MID-MIGRATION (Warning→Violation) — see [`Case::flags`].
    expected_flags: Vec<&'static str>,
}

impl Case {
    fn new(source: Source) -> Self {
        Self {
            source,
            with_ontology: false,
            shape_union: false,
            expect_conforms: true,
            expected_violations: Vec::new(),
            expected_warnings: Vec::new(),
            expected_messages: Vec::new(),
            expected_violations_ci: Vec::new(),
            expected_warnings_ci: Vec::new(),
            any_violations: Vec::new(),
            any_violations_ci: Vec::new(),
            forbidden_warnings: Vec::new(),
            expected_path_components: Vec::new(),
            expected_flag_path_components: Vec::new(),
            expected_flags: Vec::new(),
        }
    }

    /// Case fed by inline Turtle (owned `String`; accepts `&str`/`String`/`format!`).
    pub fn inline(ttl: impl Into<String>) -> Self {
        Self::new(Source::Inline(ttl.into()))
    }

    /// Case fed by raw N-Triples passed DIRECTLY to the validator (no Turtle
    /// round-trip). Use when the original called `validate(nt)` on an N-Triples
    /// literal and the round-trip could alter the data (e.g. language-tag casing).
    pub fn raw_nt(nt: impl Into<String>) -> Self {
        Self::new(Source::RawNt(nt.into()))
    }

    /// Case fed by `tests/fixtures/{subdir}/{name}.ttl`.
    pub fn file(subdir: &'static str, name: &'static str) -> Self {
        Self::new(Source::File { subdir, name })
    }

    /// Case fed by a repo-root-relative path (e.g. `"tests/fixtures/software.ttl"`).
    pub fn repo_path(rel: &'static str) -> Self {
        Self::new(Source::RepoPath(rel))
    }

    /// Validate against the merged ontology + fixture (`validate_with_ontology`).
    pub fn with_ontology(mut self) -> Self {
        self.with_ontology = true;
        self
    }

    /// Validate against the LIVE production shape union
    /// ([`validate_with_ontology_shape_union`]) — the corpus `gmeow validate` runs,
    /// which INCLUDES `generated/shapes/validation-shapes.ttl`. Implies the ontology
    /// merge (the projected cardinality shapes' class constraints need the merged
    /// class/subclass declarations), so it need not be combined with
    /// [`Case::with_ontology`].
    pub fn shape_union(mut self) -> Self {
        self.shape_union = true;
        self
    }

    /// Require at least one `Violation` result whose `sh:resultPath` is `path` (a
    /// property IRI) AND whose `sh:sourceConstraintComponent` contains `component`
    /// (e.g. `"MaxCountConstraintComponent"`). Asserts directly on the report's
    /// structured results, so it matches the PROJECTED cardinality shapes that carry
    /// no `sh:message`.
    pub fn fails_on_path(mut self, path: &'static str, component: &'static str) -> Self {
        self.expected_path_components.push((path, component));
        self
    }

    /// The SEVERITY-AGNOSTIC twin of [`Case::fails_on_path`]: require at least one
    /// result — at `sh:Warning` OR `sh:Violation` — whose `sh:resultPath` is `path`
    /// AND whose `sh:sourceConstraintComponent` contains `component`.
    ///
    /// Use this (not [`Case::fails_on_path`], which filters to `Violation`) when the
    /// constraint being witnessed lives on a shape whose severity is MID-MIGRATION —
    /// e.g. a generated frame-relativity shape moving from `sh:Warning` to
    /// `sh:Violation`. The regression must hold at BOTH ends of that migration: the
    /// finding is present at either severity, so it is anchored on the (path,
    /// constraint-component) pair without pinning the severity. Pair it with
    /// [`Case::fails`] only when a SEPARATE, severity-stable violation makes the graph
    /// a hard SHACL failure independent of the mid-migration shape.
    pub fn flags_on_path(mut self, path: &'static str, component: &'static str) -> Self {
        self.expected_flag_path_components.push((path, component));
        self
    }

    /// The SEVERITY-AGNOSTIC, path-free twin of [`Case::violations`]: require each
    /// substring to be present in the message of some result AT ANY SEVERITY
    /// (`sh:Warning` OR `sh:Violation`).
    ///
    /// Use this (not [`Case::violations`], which filters to `Violation`, nor
    /// [`Case::flags_on_path`], which requires a `sh:resultPath`) when the constraint
    /// being witnessed is a `sh:sparql` node constraint — which binds NO result path —
    /// whose severity is MID-MIGRATION (Warning→Violation). The finding is present at
    /// either severity and carries a message but no path, so it is anchored on the
    /// message substring without pinning the severity, holding at BOTH ends of the flip.
    pub fn flags(mut self, subs: &[&'static str]) -> Self {
        self.expected_flags.extend_from_slice(subs);
        self
    }

    /// Expect the graph to FAIL SHACL (at least one violation).
    pub fn fails(mut self) -> Self {
        self.expect_conforms = false;
        self
    }

    /// Require each substring to be present in some violation message (subset).
    pub fn violations(mut self, subs: &[&'static str]) -> Self {
        self.expected_violations.extend_from_slice(subs);
        self
    }

    /// Require each substring to be present in some warning message (subset).
    pub fn warnings(mut self, subs: &[&'static str]) -> Self {
        self.expected_warnings.extend_from_slice(subs);
        self
    }

    /// Case-insensitive subset: each substring must be present in some violation
    /// message, comparing both sides lowercased (mirrors `.to_lowercase().contains`).
    pub fn violations_ci(mut self, subs: &[&'static str]) -> Self {
        self.expected_violations_ci.extend_from_slice(subs);
        self
    }

    /// Case-insensitive subset over warning messages.
    pub fn warnings_ci(mut self, subs: &[&'static str]) -> Self {
        self.expected_warnings_ci.extend_from_slice(subs);
        self
    }

    /// Require each substring to be present in the UNION of violation and warning
    /// messages (mirrors originals that checked `violations().chain(warnings())`,
    /// where a message may land in either channel).
    pub fn messages(mut self, subs: &[&'static str]) -> Self {
        self.expected_messages.extend_from_slice(subs);
        self
    }

    /// Require at least ONE of `subs` to be present in some violation message
    /// (case-sensitive; mirrors an `a || b || c` disjunction in the original).
    pub fn any_violation(mut self, subs: &[&'static str]) -> Self {
        self.any_violations.push(subs.to_vec());
        self
    }

    /// Case-insensitive variant of [`Case::any_violation`].
    pub fn any_violation_ci(mut self, subs: &[&'static str]) -> Self {
        self.any_violations_ci.push(subs.to_vec());
        self
    }

    /// Assert no warning message contains `sub`.
    pub fn no_warning(mut self, sub: &'static str) -> Self {
        self.forbidden_warnings.push(sub);
        self
    }

    /// Load the source, validate, and assert the configured expectations.
    pub fn run(&self) {
        let nt = match &self.source {
            Source::Inline(ttl) => ttl_str_to_nt(ttl),
            Source::RawNt(nt) => nt.clone(),
            Source::File { subdir, name } => fixture_as_nt(subdir, name),
            Source::RepoPath(rel) => ttl_file_to_nt(&repo_root().join(rel)),
        };
        let report = if self.shape_union {
            validate_with_ontology_shape_union(&nt)
        } else if self.with_ontology {
            validate_with_ontology(&nt)
        } else {
            validate(&nt)
        };
        let got_violations = violations(&report);
        let got_warnings = warnings(&report);

        if self.expect_conforms {
            assert!(
                ok(&report),
                "expected graph to conform (no violations); violations: {got_violations:?}"
            );
        } else {
            assert!(
                !ok(&report),
                "expected graph to FAIL SHACL (violations expected); got none"
            );
        }

        for sub in &self.expected_violations {
            assert!(
                got_violations.iter().any(|v| v.contains(sub)),
                "expected a violation containing {sub:?}; got: {got_violations:?}"
            );
        }
        for sub in &self.expected_warnings {
            assert!(
                got_warnings.iter().any(|w| w.contains(sub)),
                "expected a warning containing {sub:?}; got: {got_warnings:?}"
            );
        }
        for sub in &self.expected_messages {
            assert!(
                got_violations
                    .iter()
                    .chain(&got_warnings)
                    .any(|m| m.contains(sub)),
                "expected a violation OR warning containing {sub:?}; \
                 violations: {got_violations:?}; warnings: {got_warnings:?}"
            );
        }
        for sub in &self.expected_violations_ci {
            let needle = sub.to_lowercase();
            assert!(
                got_violations
                    .iter()
                    .any(|v| v.to_lowercase().contains(&needle)),
                "expected a violation containing {sub:?} (case-insensitive); got: {got_violations:?}"
            );
        }
        for sub in &self.expected_warnings_ci {
            let needle = sub.to_lowercase();
            assert!(
                got_warnings
                    .iter()
                    .any(|w| w.to_lowercase().contains(&needle)),
                "expected a warning containing {sub:?} (case-insensitive); got: {got_warnings:?}"
            );
        }
        for group in &self.any_violations {
            assert!(
                group
                    .iter()
                    .any(|sub| got_violations.iter().any(|v| v.contains(sub))),
                "expected a violation containing one of {group:?}; got: {got_violations:?}"
            );
        }
        for group in &self.any_violations_ci {
            assert!(
                group.iter().any(|sub| {
                    let needle = sub.to_lowercase();
                    got_violations
                        .iter()
                        .any(|v| v.to_lowercase().contains(&needle))
                }),
                "expected a violation containing one of {group:?} (case-insensitive); got: {got_violations:?}"
            );
        }
        for sub in &self.forbidden_warnings {
            assert!(
                !got_warnings.iter().any(|w| w.contains(sub)),
                "expected NO warning containing {sub:?}; got: {got_warnings:?}"
            );
        }
        for (path, component) in &self.expected_path_components {
            let hit = report
                .results
                .iter()
                .filter(|r| r.severity == Severity::Violation)
                .any(|r| {
                    let path_ok = matches!(
                        r.result_path.as_ref(),
                        Some(Term::NamedNode(p)) if p.as_str().contains(path)
                    );
                    let component_ok = r.source_constraint_component.as_str().contains(component);
                    path_ok && component_ok
                });
            assert!(
                hit,
                "expected a Violation on path {path:?} with constraint component \
                 containing {component:?}; results: {:?}",
                report
                    .results
                    .iter()
                    .map(|r| (
                        r.result_path.as_ref().map(ToString::to_string),
                        r.source_constraint_component.to_string(),
                        r.severity.clone(),
                    ))
                    .collect::<Vec<_>>()
            );
        }
        for sub in &self.expected_flags {
            // Severity-agnostic: match a result at ANY severity (Warning OR Violation)
            // whose message carries the substring, so a mid-migration `sh:sparql`
            // constraint is witnessed at both ends of the Warning→Violation flip.
            let hit = report
                .results
                .iter()
                .any(|r| r.message.as_deref().is_some_and(|m| m.contains(sub)));
            assert!(
                hit,
                "expected a result (any severity) with a message containing {sub:?}; \
                 messages: {:?}",
                report
                    .results
                    .iter()
                    .map(|r| (r.message.clone(), r.severity.clone()))
                    .collect::<Vec<_>>()
            );
        }
        for (path, component) in &self.expected_flag_path_components {
            // Severity-agnostic: match a result at ANY severity (Warning OR Violation),
            // so a mid-migration frame shape is witnessed at both ends of the
            // Warning→Violation flip.
            let hit = report.results.iter().any(|r| {
                let path_ok = matches!(
                    r.result_path.as_ref(),
                    Some(Term::NamedNode(p)) if p.as_str().contains(path)
                );
                let component_ok = r.source_constraint_component.as_str().contains(component);
                path_ok && component_ok
            });
            assert!(
                hit,
                "expected a result (any severity) on path {path:?} with constraint \
                 component containing {component:?}; results: {:?}",
                report
                    .results
                    .iter()
                    .map(|r| (
                        r.result_path.as_ref().map(ToString::to_string),
                        r.source_constraint_component.to_string(),
                        r.severity.clone(),
                    ))
                    .collect::<Vec<_>>()
            );
        }
    }
}

// ── Shared query-text loader ──────────────────────────────────────────────────

/// Query-source search roots, relative to the repo root, tried in order for a
/// bare `.rq` name in [`read_query`]. The `slices/**/queries/` dirs are appended
/// dynamically after these fixed roots.
pub const QUERY_SEARCH_ROOTS: &[&str] = &["generated/queries", "queries/competency"];

/// Load a `.rq` query **verbatim** — the query text is the single source of truth
/// for a competency question, never paraphrased into Rust.
///
/// `name` resolves two ways:
/// - A **repo-relative path** (contains `/`, e.g. `"queries/competency/foo.rq"`)
///   is read directly from the repo root.
/// - A **bare file name** (e.g. `"standpoint-owl2.rq"`) is searched, in order,
///   under [`QUERY_SEARCH_ROOTS`] and then every `slices/**/queries/` directory,
///   returning the first hit.
///
/// A name found nowhere is a HARD FAIL naming the roots searched (a missing
/// required query is never papered over).
pub fn read_query(name: &str) -> String {
    let root = repo_root();
    if name.contains('/') {
        let path = root.join(name);
        return std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read query {}: {e}", path.display()));
    }
    let mut roots: Vec<PathBuf> = QUERY_SEARCH_ROOTS.iter().map(|r| root.join(r)).collect();
    collect_slice_query_dirs(&root.join("slices"), &mut roots);
    for dir in &roots {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return std::fs::read_to_string(&candidate)
                .unwrap_or_else(|e| panic!("read query {}: {e}", candidate.display()));
        }
    }
    panic!(
        "query {name:?} not found under any of {} search roots \
         (generated/queries, queries/competency, slices/**/queries)",
        roots.len()
    );
}

/// Collect every `slices/**/queries/` directory (directories literally named
/// `queries`) into `out`, recursively.
fn collect_slice_query_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in read.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            if path.file_name().and_then(|n| n.to_str()) == Some("queries") {
                out.push(path.clone());
            }
            collect_slice_query_dirs(&path, out);
        }
    }
}

// ── Ergonomic expected-term constructors ──────────────────────────────────────
//
// [`QueryCase`] expected rows/triples are [`TermValue`]s. These free helpers keep
// case tables readable — `iri("…")`, `lit("…")`, `int_lit(3)` — instead of the
// verbose `TermValue::…` constructors.

/// `xsd:string` datatype IRI — the default plain-literal datatype.
pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// `xsd:integer` datatype IRI.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// An IRI term.
pub fn iri(s: &str) -> TermValue {
    TermValue::iri(s)
}

/// A plain `xsd:string` literal term.
pub fn lit(s: &str) -> TermValue {
    TermValue::simple_literal(s)
}

/// A typed literal term with an explicit datatype IRI.
pub fn typed_lit(lexical: &str, datatype: &str) -> TermValue {
    TermValue::typed_literal(lexical, datatype)
}

/// An `xsd:integer` literal term.
pub fn int_lit(n: i64) -> TermValue {
    TermValue::typed_literal(n.to_string(), XSD_INTEGER)
}

/// A language-tagged literal term (`rdf:langString`).
pub fn lang_lit(lexical: &str, language: &str) -> TermValue {
    TermValue::lang_literal(lexical, language)
}

// ── SPARQL feature coverage ────────────────────────────────────────────────────

/// A SPARQL language feature a migrated competency query exercises.
///
/// The [`MIGRATION_FEATURE_REGISTRY`] must, in union, cover every variant (see the
/// `feature_registry_covers_all_features` invariant in
/// `conformance_sparql_features.rs`) so the native migration never silently drops a
/// feature the source `.rq` corpus relies on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Feature {
    /// `{ … } UNION { … }`.
    Union,
    /// `OPTIONAL { … }`.
    Optional,
    /// `FILTER NOT EXISTS { … }`.
    FilterNotExists,
    /// `BIND(expr AS ?v)`.
    Bind,
    /// `COALESCE(?a, ?b, …)`.
    Coalesce,
    /// `CONSTRUCT { … } WHERE { … }` graph projection.
    ConstructGraph,
    /// Pre-bound query variables (`SparqlRequest.substitutions`, the native
    /// `initBindings` equivalent), driven by [`QueryCase::bind`].
    InitBindings,
}

impl Feature {
    /// Every `Feature` variant — the coverage bar the registry union must meet.
    pub const ALL: &'static [Feature] = &[
        Feature::Union,
        Feature::Optional,
        Feature::FilterNotExists,
        Feature::Bind,
        Feature::Coalesce,
        Feature::ConstructGraph,
        Feature::InitBindings,
    ];
}

/// The registry of migration [`QueryCase`] identities and the SPARQL features each
/// exercises. It is a **checked-in, append-only** list: every cluster task adds the
/// `(cq_id, feature_tags)` of the cases it lands, and the tag-union must stay ⊇
/// [`Feature::ALL`] (enforced by `feature_registry_covers_all_features`).
///
/// The rows below are the Task-1 seed: `conformance_sparql_features.rs` lands one
/// small, self-contained [`QueryCase`] per feature so the invariant is green from
/// the first commit; later migrations extend the union, never shrink it.
pub const MIGRATION_FEATURE_REGISTRY: &[(&str, &[Feature])] = &[
    ("sparql-features/union", &[Feature::Union]),
    ("sparql-features/optional", &[Feature::Optional]),
    (
        "sparql-features/filter-not-exists",
        &[Feature::FilterNotExists],
    ),
    ("sparql-features/bind", &[Feature::Bind]),
    ("sparql-features/coalesce", &[Feature::Coalesce]),
    (
        "sparql-features/construct-graph",
        &[Feature::ConstructGraph],
    ),
    ("sparql-features/init-bindings", &[Feature::InitBindings]),
    // Migrated narrative-interior cluster cases (conformance_{narration,disclosure}.rs).
    ("narrative/narration-cooccurrence", &[Feature::Union]),
    (
        "disclosure/public-candidates",
        &[Feature::FilterNotExists, Feature::InitBindings],
    ),
    (
        "disclosure/schema-org-projection",
        &[Feature::ConstructGraph, Feature::FilterNotExists],
    ),
    // Migrated email cluster cases (conformance_email.rs).
    ("email/dsn-kinds", &[Feature::InitBindings]),
    (
        "email/version-memberships",
        &[Feature::Optional, Feature::InitBindings],
    ),
    // Migrated identity cluster cases (conformance_{gender,sexuality,risk,competency}.rs).
    ("gender/gender-values", &[Feature::Optional]),
    (
        "sexuality/orientation-values",
        &[Feature::Union, Feature::Bind],
    ),
    ("risk/severity-order", &[Feature::FilterNotExists]),
    (
        "competency/expertise-expiring-credentials",
        &[Feature::Bind],
    ),
    // Migrated slice cluster cases (conformance_{gts_slice,music_competency,
    // music_oral_tradition}.rs). The `gts-slice`/`music-oral` rows document the
    // SPARQL features of migrated `.rq` queries run as smoke/aggregate selects; the
    // `music-competency` row is a live `QueryCase` (15-way UNION with per-branch BIND).
    ("gts-slice/evidence-packages-signers", &[Feature::Optional]),
    (
        "music-competency/query-bundle",
        &[Feature::Union, Feature::Bind],
    ),
    ("music-oral/oral-works", &[Feature::FilterNotExists]),
];

/// The de-duplicated union of every feature tag in [`MIGRATION_FEATURE_REGISTRY`].
pub fn registry_feature_union() -> Vec<Feature> {
    let mut union: Vec<Feature> = Vec::new();
    for (_, tags) in MIGRATION_FEATURE_REGISTRY {
        for &tag in *tags {
            if !union.contains(&tag) {
                union.push(tag);
            }
        }
    }
    union
}

// ── Parameterized competency-query case harness ────────────────────────────────

/// Where a [`QueryCase`]'s data graph comes from — the twin of [`Source`] for the
/// SPARQL/CONSTRUCT surface. Non-ontology paths may be repo-relative (resolved
/// against [`repo_root`]) or absolute.
enum QuerySource {
    /// The merged ontology (`GraphStore::ontology()`, OnceLock-cached).
    Ontology,
    /// The merged ontology plus a Turtle file (`GraphStore::ontology_plus_ttl_file`).
    OntologyPlus(PathBuf),
    /// The merged ontology plus every `*.ttl` in a directory
    /// (`GraphStore::ontology_plus_ttl_dir`).
    OntologyPlusDir(PathBuf),
    /// A standalone Turtle file (`GraphStore::parse_ttl_file`).
    TtlFile(PathBuf),
    /// Inline Turtle text (`GraphStore::parse_ttl`).
    RawTtl(String),
}

/// A declarative competency-query conformance case — the SPARQL/CONSTRUCT twin of
/// the SHACL [`Case`] builder. It carries the competency-question id, the SPARQL
/// features it exercises, its data source, the query text (loaded verbatim from a
/// `.rq` via [`read_query`], or inline), optional pre-bindings, and one family of
/// assertions.
///
/// The query verb (ASK / SELECT / CONSTRUCT) is inferred from which assertions are
/// set; mixing families is a HARD FAIL. All comparisons are by [`TermValue`], and
/// SELECT comparisons are **set-based by default** (native SELECT order differs
/// from rdflib) — [`QueryCase::select_ordered`] is the explicit order-sensitive
/// variant for `ORDER BY`. Every mismatch panics with the query text (matching the
/// harness's hard-fail style).
pub struct QueryCase {
    cq_id: &'static str,
    feature_tags: &'static [Feature],
    source: QuerySource,
    query: Option<String>,
    bindings: Vec<(String, TermValue)>,

    // Assertion families (exactly one family may be populated per case).
    ask_expect: Option<bool>,
    contains_rows: Vec<Vec<TermValue>>,
    row_set: Option<Vec<Vec<TermValue>>>,
    distinct_row_set: Option<Vec<Vec<TermValue>>>,
    ordered_rows: Option<Vec<Vec<TermValue>>>,
    count_at_least: Option<usize>,
    column_supersets: Vec<(String, Vec<TermValue>)>,
    construct_has: Vec<(TermValue, TermValue, TermValue)>,
    construct_len: Option<usize>,
}

impl QueryCase {
    /// A new case for competency question `cq_id`, exercising `feature_tags`. The
    /// default source is the merged ontology; set a query with
    /// [`Self::query_file`]/[`Self::query`] and at least one assertion before
    /// [`Self::run`].
    pub fn new(cq_id: &'static str, feature_tags: &'static [Feature]) -> Self {
        Self {
            cq_id,
            feature_tags,
            source: QuerySource::Ontology,
            query: None,
            bindings: Vec::new(),
            ask_expect: None,
            contains_rows: Vec::new(),
            row_set: None,
            distinct_row_set: None,
            ordered_rows: None,
            count_at_least: None,
            column_supersets: Vec::new(),
            construct_has: Vec::new(),
            construct_len: None,
        }
    }

    /// The competency-question id (matches a [`MIGRATION_FEATURE_REGISTRY`] row).
    pub fn cq_id(&self) -> &'static str {
        self.cq_id
    }

    /// The SPARQL features this case exercises.
    pub fn feature_tags(&self) -> &'static [Feature] {
        self.feature_tags
    }

    // ── source ────────────────────────────────────────────────────────────────

    /// Query the merged ontology (the default).
    pub fn over_ontology(mut self) -> Self {
        self.source = QuerySource::Ontology;
        self
    }

    /// Query the merged ontology plus a Turtle fixture (repo-relative or absolute).
    pub fn over_ontology_plus(mut self, path: impl Into<PathBuf>) -> Self {
        self.source = QuerySource::OntologyPlus(path.into());
        self
    }

    /// Query the merged ontology plus every `*.ttl` in a directory (repo-relative or
    /// absolute). Mirrors the Python `glob("*.ttl")` fixture/example corpora.
    pub fn over_ontology_plus_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.source = QuerySource::OntologyPlusDir(path.into());
        self
    }

    /// Query a standalone Turtle file (repo-relative or absolute).
    pub fn over_ttl_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.source = QuerySource::TtlFile(path.into());
        self
    }

    /// Query an inline Turtle string.
    pub fn over_raw_ttl(mut self, ttl: impl Into<String>) -> Self {
        self.source = QuerySource::RawTtl(ttl.into());
        self
    }

    // ── query ─────────────────────────────────────────────────────────────────

    /// Load the query verbatim from a `.rq` (bare name or repo-relative path; see
    /// [`read_query`]).
    pub fn query_file(mut self, name_or_path: &str) -> Self {
        self.query = Some(read_query(name_or_path));
        self
    }

    /// Set the query text inline.
    pub fn query(mut self, text: impl Into<String>) -> Self {
        self.query = Some(text.into());
        self
    }

    /// Pre-bind query variable `var` to `value` (accumulates; the native
    /// `initBindings` equivalent — threads into `SparqlRequest.substitutions`).
    pub fn bind(mut self, var: &str, value: TermValue) -> Self {
        self.bindings.push((var.to_owned(), value));
        self
    }

    // ── assertions ──────────────────────────────────────────────────────────────

    /// Assert the ASK query returns `true`.
    pub fn ask_true(mut self) -> Self {
        self.ask_expect = Some(true);
        self
    }

    /// Assert the ASK query returns `false`.
    pub fn ask_false(mut self) -> Self {
        self.ask_expect = Some(false);
        self
    }

    /// Assert each listed row is present in the SELECT result (subset; extra rows
    /// allowed). Each row's terms align to the query's projection order.
    pub fn select_contains_rows(mut self, rows: Vec<Vec<TermValue>>) -> Self {
        self.contains_rows.extend(rows);
        self
    }

    /// Assert the SELECT result equals `rows` as a **set** (order-insensitive,
    /// multiplicity-preserving).
    pub fn select_row_set(mut self, rows: Vec<Vec<TermValue>>) -> Self {
        self.row_set = Some(rows);
        self
    }

    /// Assert the SELECT result equals `rows` as a **distinct set** (order- AND
    /// multiplicity-insensitive: the actual rows are de-duplicated first). The
    /// native twin of rdflib originals that folded `graph.query(…)` into a Python
    /// `set(...)` before comparing — the projection legitimately repeats a row when
    /// an unprojected join variable multiplies (e.g. a DISTINCT-less competency
    /// UNION branch with a `hasMetricGroup ?group` join).
    pub fn select_distinct_set(mut self, rows: Vec<Vec<TermValue>>) -> Self {
        self.distinct_row_set = Some(rows);
        self
    }

    /// Assert the SELECT result equals `rows` **in order** (for `ORDER BY`).
    pub fn select_ordered(mut self, rows: Vec<Vec<TermValue>>) -> Self {
        self.ordered_rows = Some(rows);
        self
    }

    /// Assert the SELECT result has at least `n` rows.
    pub fn select_count_at_least(mut self, n: usize) -> Self {
        self.count_at_least = Some(n);
        self
    }

    /// Assert the SELECT `var` column contains (as a superset) every listed value.
    pub fn column_superset(mut self, var: &str, values: Vec<TermValue>) -> Self {
        self.column_supersets.push((var.to_owned(), values));
        self
    }

    /// Assert each listed triple is present in the CONSTRUCT result graph.
    pub fn construct_has(mut self, triples: Vec<(TermValue, TermValue, TermValue)>) -> Self {
        self.construct_has.extend(triples);
        self
    }

    /// Assert the CONSTRUCT result graph has exactly `n` triples.
    pub fn construct_len(mut self, n: usize) -> Self {
        self.construct_len = Some(n);
        self
    }

    // ── execute ───────────────────────────────────────────────────────────────

    /// Resolve a case path against [`repo_root`] when relative.
    fn resolve(path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            repo_root().join(path)
        }
    }

    /// Load the source, run the query, and assert the configured expectations.
    /// Panics (with the query text) on any mismatch.
    pub fn run(&self) {
        let store = match &self.source {
            QuerySource::Ontology => GraphStore::ontology(),
            QuerySource::OntologyPlus(p) => GraphStore::ontology_plus_ttl_file(&Self::resolve(p)),
            QuerySource::OntologyPlusDir(p) => GraphStore::ontology_plus_ttl_dir(&Self::resolve(p)),
            QuerySource::TtlFile(p) => GraphStore::parse_ttl_file(&Self::resolve(p)),
            QuerySource::RawTtl(t) => GraphStore::parse_ttl(t),
        };
        let query = self.query.as_deref().unwrap_or_else(|| {
            panic!(
                "QueryCase {:?}: no query set (call .query_file or .query)",
                self.cq_id
            )
        });

        let has_select = !self.contains_rows.is_empty()
            || self.row_set.is_some()
            || self.distinct_row_set.is_some()
            || self.ordered_rows.is_some()
            || self.count_at_least.is_some()
            || !self.column_supersets.is_empty();
        let has_construct = !self.construct_has.is_empty() || self.construct_len.is_some();
        let has_ask = self.ask_expect.is_some();

        let families = u8::from(has_ask) + u8::from(has_select) + u8::from(has_construct);
        assert!(
            families > 0,
            "QueryCase {:?}: no assertion configured\n{query}",
            self.cq_id
        );
        assert!(
            families == 1,
            "QueryCase {:?}: mixes ASK/SELECT/CONSTRUCT assertions — split into \
             separate cases\n{query}",
            self.cq_id
        );

        if has_ask {
            let got = store.ask(&self.bindings, query);
            let want = self.ask_expect.unwrap();
            assert!(
                got == want,
                "QueryCase {:?}: expected ASK {want}, got {got}\n{query}",
                self.cq_id
            );
            return;
        }

        if has_construct {
            let out = store.construct(&self.bindings, query);
            for (s, p, o) in &self.construct_has {
                assert!(
                    out.contains_triple(s, p, o),
                    "QueryCase {:?}: CONSTRUCT graph missing triple {s:?} {p:?} {o:?}\n{query}",
                    self.cq_id
                );
            }
            if let Some(n) = self.construct_len {
                let got = out.triple_count();
                assert!(
                    got == n,
                    "QueryCase {:?}: CONSTRUCT graph has {got} triples, expected {n}\n{query}",
                    self.cq_id
                );
            }
            return;
        }

        // SELECT.
        let (vars, rows) = store.select(&self.bindings, query);
        let want_row =
            |r: &[TermValue]| -> Vec<Option<TermValue>> { r.iter().cloned().map(Some).collect() };

        for exp in &self.contains_rows {
            let w = want_row(exp);
            assert!(
                rows.contains(&w),
                "QueryCase {:?}: SELECT result is missing expected row {exp:?}\n\
                 vars {vars:?}, rows {rows:?}\n{query}",
                self.cq_id
            );
        }

        if let Some(exp_rows) = &self.row_set {
            let want: Vec<Vec<Option<TermValue>>> = exp_rows.iter().map(|r| want_row(r)).collect();
            assert!(
                multiset_eq(&rows, &want),
                "QueryCase {:?}: SELECT result set != expected set\n\
                 vars {vars:?}, rows {rows:?}, expected {want:?}\n{query}",
                self.cq_id
            );
        }

        if let Some(exp_rows) = &self.distinct_row_set {
            // De-duplicate the actual rows (rdflib `set(...)` semantics) before the
            // set comparison; the expected list is authored distinct.
            let mut distinct: Vec<Vec<Option<TermValue>>> = Vec::new();
            for r in &rows {
                if !distinct.contains(r) {
                    distinct.push(r.clone());
                }
            }
            let want: Vec<Vec<Option<TermValue>>> = exp_rows.iter().map(|r| want_row(r)).collect();
            assert!(
                multiset_eq(&distinct, &want),
                "QueryCase {:?}: SELECT distinct result set != expected set\n\
                 vars {vars:?}, distinct rows {distinct:?}, expected {want:?}\n{query}",
                self.cq_id
            );
        }

        if let Some(exp_rows) = &self.ordered_rows {
            let want: Vec<Vec<Option<TermValue>>> = exp_rows.iter().map(|r| want_row(r)).collect();
            assert!(
                rows == want,
                "QueryCase {:?}: SELECT result order != expected\n\
                 vars {vars:?}, rows {rows:?}, expected {want:?}\n{query}",
                self.cq_id
            );
        }

        if let Some(n) = self.count_at_least {
            assert!(
                rows.len() >= n,
                "QueryCase {:?}: SELECT returned {} rows, expected at least {n}\n{query}",
                self.cq_id,
                rows.len()
            );
        }

        for (var, values) in &self.column_supersets {
            let idx = vars.iter().position(|v| v == var).unwrap_or_else(|| {
                panic!(
                    "QueryCase {:?}: column {var:?} not in projection {vars:?}\n{query}",
                    self.cq_id
                )
            });
            for want in values {
                let target = Some(want.clone());
                assert!(
                    rows.iter().any(|r| r.get(idx) == Some(&target)),
                    "QueryCase {:?}: column {var:?} is missing value {want:?}\n\
                     vars {vars:?}, rows {rows:?}\n{query}",
                    self.cq_id
                );
            }
        }
    }
}

/// Multiset equality over solution rows (order-insensitive, multiplicity-aware).
/// `TermValue` is `Eq` but not `Ord`/`Hash`, so this is an O(n²) match-and-mark —
/// fine for the small row sets a competency case asserts.
fn multiset_eq(a: &[Vec<Option<TermValue>>], b: &[Vec<Option<TermValue>>]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut used = vec![false; b.len()];
    for x in a {
        let mut matched = false;
        for (i, y) in b.iter().enumerate() {
            if !used[i] && x == y {
                used[i] = true;
                matched = true;
                break;
            }
        }
        if !matched {
            return false;
        }
    }
    true
}

// ── Unit tests for the blank-node-aware `GraphStore` helpers ──────────────────
//
// This support module is compiled into every sibling integration binary, so plain
// `#[test]` fns here are collected and run. They exercise the new `*_h` helpers
// against small inline Turtle fixtures parsed through the existing
// `GraphStore::parse_ttl` path (the same path the IRI-only helpers use), and never
// touch the on-disk shapes/ontology corpus.

/// A blank `owl:Restriction`, an `owl:unionOf`/`intersectionOf` `rdf:List` (with a
/// blank member), and an `owl:Axiom` reification — everything the helpers walk.
#[cfg(test)]
const BNODE_FIXTURE_TTL: &str = "\
@prefix ex: <https://example.org/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:Union owl:unionOf ( ex:A ex:B ) .
ex:Inter owl:intersectionOf ( ex:A [ a ex:Marker ] ex:C ) .
ex:Widget a owl:Class .
ex:C rdfs:subClassOf [
    a owl:Restriction ;
    owl:onProperty ex:hasPart ;
    owl:someValuesFrom ex:Widget
] .
[ a owl:Axiom ;
  owl:annotatedSource ex:C ;
  owl:annotatedProperty rdfs:label ;
  owl:annotatedTarget \"a widget-bearing class\" ] .
";

#[cfg(test)]
const EX: &str = "https://example.org/";

#[cfg(test)]
fn ex(local: &str) -> String {
    format!("{EX}{local}")
}

#[test]
fn object_as_subject_converts_named_and_blank_only() {
    assert_eq!(
        GraphStore::object_as_subject(&Object::Named(ex("A"))),
        Some(Subject::Named(ex("A")))
    );
    assert_eq!(
        GraphStore::object_as_subject(&Object::Blank("b0".to_owned())),
        Some(Subject::Blank("b0".to_owned()))
    );
    // A literal is not a subject term.
    assert_eq!(
        GraphStore::object_as_subject(&Object::Literal {
            value: "x".to_owned(),
            datatype: "http://www.w3.org/2001/XMLSchema#string".to_owned(),
            language: None,
            direction: None,
        }),
        None
    );
}

#[test]
fn rdf_list_h_walks_union_of_in_order() {
    let g = GraphStore::parse_ttl(BNODE_FIXTURE_TTL);
    // The list head is a blank node reached from the named `ex:Union` subject.
    let head = GraphStore::object_as_subject(
        &g.value_h(&Subject::Named(ex("Union")), OWL_UNION_OF)
            .expect("owl:unionOf head present"),
    )
    .expect("list head is named or blank");
    assert!(matches!(head, Subject::Blank(_)), "list head is a bnode");
    assert_eq!(
        g.rdf_list_h(&head),
        vec![Object::Named(ex("A")), Object::Named(ex("B"))]
    );
    // `members_h` dispatches to the same Collection walk for an rdf:first head.
    assert_eq!(g.members_h(&head), g.rdf_list_h(&head));
}

#[test]
fn rdf_list_h_preserves_order_with_a_blank_member() {
    let g = GraphStore::parse_ttl(BNODE_FIXTURE_TTL);
    let head = GraphStore::object_as_subject(
        &g.value_h(&Subject::Named(ex("Inter")), OWL_INTERSECTION_OF)
            .expect("owl:intersectionOf head present"),
    )
    .expect("list head is named or blank");
    let members = g.rdf_list_h(&head);
    assert_eq!(members.len(), 3, "three list members, order preserved");
    assert_eq!(members[0], Object::Named(ex("A")));
    // The middle member is an anonymous `[ a ex:Marker ]` node — proves the walk
    // yields a blank object we can chain INTO as a subject.
    assert!(
        matches!(members[1], Object::Blank(_)),
        "middle member is a blank node, got {:?}",
        members[1]
    );
    assert_eq!(members[2], Object::Named(ex("C")));
    // Chain into the blank member and read its own edge.
    let marker = GraphStore::object_as_subject(&members[1]).expect("blank member as subject");
    assert_eq!(
        g.value_h(&marker, RDF_TYPE),
        Some(Object::Named(ex("Marker")))
    );
}

#[test]
fn subjects_of_type_h_and_restriction_matches_a_blank_restriction() {
    let g = GraphStore::parse_ttl(BNODE_FIXTURE_TTL);
    let restrictions = g.subjects_of_type_h(OWL_RESTRICTION);
    assert_eq!(restrictions.len(), 1, "one blank owl:Restriction");
    let restriction = &restrictions[0];
    assert!(
        matches!(restriction, Subject::Blank(_)),
        "the restriction is a bnode"
    );
    // Matches on onProperty + someValuesFrom.
    assert!(g.restriction_matches(
        restriction,
        &ex("hasPart"),
        OWL_SOME_VALUES_FROM,
        &ex("Widget"),
    ));
    // Wrong filler property (allValuesFrom) does not match.
    assert!(!g.restriction_matches(
        restriction,
        &ex("hasPart"),
        OWL_ALL_VALUES_FROM,
        &ex("Widget"),
    ));
    // Wrong onProperty does not match.
    assert!(!g.restriction_matches(
        restriction,
        &ex("hasWhole"),
        OWL_SOME_VALUES_FROM,
        &ex("Widget"),
    ));
}

#[test]
fn axiom_annotations_found_by_annotated_source() {
    let g = GraphStore::parse_ttl(BNODE_FIXTURE_TTL);
    let annotations = g.axiom_annotations(&ex("C"));
    assert_eq!(annotations.len(), 1, "one reified axiom over ex:C");
    let (property, target) = &annotations[0];
    assert_eq!(property, "http://www.w3.org/2000/01/rdf-schema#label");
    match target {
        Object::Literal { value, .. } => assert_eq!(value, "a widget-bearing class"),
        other => panic!("expected a literal annotatedTarget, got {other:?}"),
    }
    // A source with no reified axiom yields nothing.
    assert!(g.axiom_annotations(&ex("Union")).is_empty());
}

#[cfg(test)]
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
#[cfg(test)]
const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
#[cfg(test)]
const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";
