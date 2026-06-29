// SPDX-License-Identifier: AGPL-3.0-only

//! Shared helpers for whole-ontology native SHACL conformance tests (#867).
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

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gmeow_rdf::oxigraph::{dataset_from_store, flat_oxigraph_quads_from_dataset};
use gmeow_rdf::{parse_dataset, serialize_dataset, SerializeGraph};
use gmeow_shacl::engine::{parse_shapes, validate as validate_store};
use gmeow_shacl::report::{Severity, ValidationReport};
use gmeow_shacl::shapes::Shapes;
use gmeow_shacl::text_ingest::parse_ntriples_to_store;
use oxigraph::store::Store;

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
/// literally named `module.ttl`) into one oxigraph Store and dumps as N-Triples.
/// Mirrors `load_merged_graph(include_imports=False)`.
///
/// Cached via [`OnceLock`] so disk I/O happens at most once per test process.
pub fn base_ontology_nt() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| store_dataset_to_nt(base_ontology_store()))
}

/// Merged ontology as an oxigraph store.
///
/// This is the store-native twin of [`base_ontology_nt`]. The conformance tests
/// use it directly so `validate_with_ontology` does not serialize the full
/// ontology to N-Triples and immediately parse it back for every case.
pub fn base_ontology_store() -> &'static Store {
    static CACHE: OnceLock<Store> = OnceLock::new();
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
            // Native codec parse (#909): lenient on private-use language tags.
            match parse_dataset(ttl.as_bytes(), "text/turtle", None)
                .and_then(|ds| flat_oxigraph_quads_from_dataset(&ds))
            {
                Ok(quads) => {
                    for quad in quads {
                        store.insert(&quad).expect("store insert is infallible");
                    }
                }
                // Warn but continue — some module.ttl files import cross-slice
                // IRIs that are not resolvable in the local store.
                Err(e) => eprintln!(
                    "warning: native Turtle parse of {} had errors: {e}",
                    path.display()
                ),
            }
        }

        store
    })
}

/// Parsed SHACL shape model for the whole conformance corpus.
///
/// `gmeow_shacl::engine::validate_graphs` parses shapes on every call. These
/// tests repeatedly validate small fixture graphs against the same shape model,
/// so cache the parsed `Shapes` inside each test process.
pub fn whole_shapes() -> &'static Shapes {
    static CACHE: OnceLock<Shapes> = OnceLock::new();
    CACHE.get_or_init(|| parse_shapes(whole_shapes_ttl()).expect("whole SHACL shapes parse"))
}

fn nt_to_store(nt: &str) -> Store {
    parse_ntriples_to_store(nt)
        .unwrap_or_else(|errors| panic!("N-Triples parse failed:\n{}", errors.join("\n")))
}

fn copy_store(source: &Store) -> Store {
    let target = Store::new().expect("in-memory store creation is infallible");
    for quad in source.quads_for_pattern(None, None, None, None) {
        let quad = quad.expect("source store iteration must succeed");
        target.insert(&quad).expect("store insert is infallible");
    }
    target
}

/// Validate `base_ontology_nt() + "\n" + fixture_nt` against `whole_shapes_ttl()`.
///
/// Use this variant when the fixture triples rely on class/property declarations
/// from the merged ontology to pass SHACL class-constraint checks.
pub fn validate_with_ontology(fixture_nt: &str) -> ValidationReport {
    let store = copy_store(base_ontology_store());
    let fixture_store = nt_to_store(fixture_nt);
    for quad in fixture_store.quads_for_pattern(None, None, None, None) {
        let quad = quad.expect("fixture store iteration must succeed");
        store.insert(&quad).expect("store insert is infallible");
    }
    validate_store(&store, whole_shapes())
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

/// Parse an inline Turtle string into an oxigraph store and emit as N-Triples.
///
/// Uses the lenient parser (same as `gmeow_shacl::engine::validate_graphs`) so
/// private-use `@x-gmeow-*` language tags are accepted.
pub fn ttl_str_to_nt(ttl: &str) -> String {
    let dataset = parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("Turtle parse failed: {e}\nInput:\n{ttl}"));
    let store = gmeow_rdf::oxigraph::store_from_dataset(
        &dataset,
        gmeow_rdf::oxigraph::GraphPolicy::FlattenToDefaultGraph,
    )
    .unwrap_or_else(|e| panic!("store from dataset failed: {e}"));
    store_dataset_to_nt(&store)
}

/// Fold an oxigraph store back to the IR and serialize its default graph as
/// N-Triples via the native codec (#909). The `application/n-quads` codec on the
/// `DefaultGraph` selection emits graphless rows (N-Triples) and is byte-lenient
/// on private-use language tags.
fn store_dataset_to_nt(store: &Store) -> String {
    let dataset = dataset_from_store(store).expect("store folds to the IR");
    let buf = serialize_dataset(
        &dataset,
        "application/n-quads",
        SerializeGraph::DefaultGraph,
    )
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
    let store = nt_to_store(data_nt);
    validate_store(&store, whole_shapes())
}

// ── Parameterized case harness (#1051) ──────────────────────────────────────────

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

/// A single parameterized SHACL conformance case (#1051).
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
    expect_conforms: bool,
    expected_violations: Vec<&'static str>,
    expected_warnings: Vec<&'static str>,
    expected_messages: Vec<&'static str>,
    expected_violations_ci: Vec<&'static str>,
    expected_warnings_ci: Vec<&'static str>,
    any_violations: Vec<Vec<&'static str>>,
    any_violations_ci: Vec<Vec<&'static str>>,
    forbidden_warnings: Vec<&'static str>,
}

impl Case {
    fn new(source: Source) -> Self {
        Self {
            source,
            with_ontology: false,
            expect_conforms: true,
            expected_violations: Vec::new(),
            expected_warnings: Vec::new(),
            expected_messages: Vec::new(),
            expected_violations_ci: Vec::new(),
            expected_warnings_ci: Vec::new(),
            any_violations: Vec::new(),
            any_violations_ci: Vec::new(),
            forbidden_warnings: Vec::new(),
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
        let report = if self.with_ontology {
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
    }
}
