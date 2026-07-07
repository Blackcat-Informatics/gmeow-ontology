// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust repository-static guards for source, workflow, and lane policy.
//!
//! These checks replace pytest files whose subject was the repository itself:
//! Python import surfaces, Makefile recipes, and GitHub workflow structure. The
//! gate is deliberately native Rust and fails hard in the existing `crate-check`
//! lane.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use gmeow_errors::{Finding, Report, Severity};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};
use regex::Regex;
use serde_yaml::Value as Yaml;

use crate::model::rdf;

const TOOL: &str = "repo-static";

// No runtime rdflib keeper remains: the rdflib↔purrdf query cross-check
// (`oracles/engine_crosscheck.py`) — the last first-party user of upstream
// rdflib — has been DELETED (purrdf's own rdflib-parity + W3C SPARQL
// conformance make it redundant), so first-party code is now rdflib-free and
// must use `purrdf.compat.rdflib` exclusively. An empty keeper list means the
// lint rejects ANY upstream-rdflib import in `src/gmeow_tools`.
const RDFLIB_KEEPERS: &[&str] = &[];

// The ELK/HermiT/Docker OWL-reasoner lane has been DELETED entirely — the
// native `logic:` reasoner + in-process `purrdf::entail` oracle replaced it, so
// no Makefile target reaches Docker/Java anymore. The invariant is now that the
// Makefile and required CI are ENTIRELY Docker-free.
//
// `LANE_MAKE_TARGETS` is the allowlist of targets permitted to reach Docker. No
// legitimate Docker lane exists any longer, so it is EMPTY: every Makefile
// target is checked for Docker-freedom, and the lint's job is to catch Docker
// or Java being RE-INTRODUCED anywhere in the Makefile (or required CI).
const LANE_MAKE_TARGETS: &[&str] = &[];
// pull-images.sh (the deleted image-pull helper) is kept as a re-introduction
// guard: no recipe may shell out to it.
const LANE_SCRIPTS: &[&str] = &["pull-images.sh"];
const DOCKER_PATTERNS: &[&str] = &[
    r"\bdocker\s+(?:run|pull|build|image|compose)\b",
    r"--mode\s+docker",
    r"obolibrary/robot",
    r"stain/jena",
    r"\bjava\s+-(?:jar|cp)\b",
    r"\b(?:javac|gradlew?)\b",
];

static DOCKER_REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    DOCKER_PATTERNS
        .iter()
        .map(|pattern| Regex::new(&format!("(?i){pattern}")).expect("static regex"))
        .collect()
});

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoStaticReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl RepoStaticReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    fn error(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
    }
}

pub fn check_repo_static(root: &Path) -> RepoStaticReport {
    let mut report = RepoStaticReport::default();
    check_lane_purity(root, &mut report);
    check_no_rdflib_in_runtime(root, &mut report);
    check_no_docker_lane_python(root, &mut report);
    check_projection_compute_purity(root, &mut report);
    check_projection_shape_purity(root, &mut report);
    check_no_generated_read_in_pipeline_stages(root, &mut report);
    check_no_run_shacl_seam(root, &mut report);
    report
}

pub fn to_diagnostics_report(report: &RepoStaticReport) -> Report {
    let mut out = Report::new(TOOL);
    for message in &report.errors {
        out.add_finding(
            Finding::new(
                Severity::Error,
                crate::codes::REPO_STATIC_VIOLATION,
                message.clone(),
            )
            .with_tool(TOOL),
        );
    }
    for message in &report.warnings {
        out.add_finding(
            Finding::new(
                Severity::Warning,
                crate::codes::REPO_STATIC_OBSERVATION,
                message.clone(),
            )
            .with_tool(TOOL),
        );
    }
    out
}

fn check_no_rdflib_in_runtime(root: &Path, report: &mut RepoStaticReport) {
    let src = root.join("src").join("gmeow_tools");
    let allowed = RDFLIB_KEEPERS
        .iter()
        .copied()
        .collect::<BTreeSet<&'static str>>();
    let mut actual = BTreeSet::new();
    for path in python_files(&src, report) {
        let rel = slash_path(path.strip_prefix(&src).unwrap_or(&path));
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        let code = strip_python_non_code(&text);
        let imports = python_imported_top_modules(&code);
        if imports.contains("rdflib") {
            actual.insert(rel);
        }
    }

    let allowed_owned = allowed
        .iter()
        .map(|s| (*s).to_owned())
        .collect::<BTreeSet<_>>();
    let offenders = actual
        .difference(&allowed_owned)
        .cloned()
        .collect::<Vec<_>>();
    if !offenders.is_empty() {
        report.error(format!(
            "first-party modules must use purrdf.compat.rdflib, not upstream rdflib: {}",
            offenders.join(", ")
        ));
    }

    let actual_refs = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if actual_refs != allowed {
        let expected = allowed.iter().copied().collect::<Vec<_>>().join(", ");
        let found = actual.iter().cloned().collect::<Vec<_>>().join(", ");
        report.error(format!(
            "rdflib keeper allow-list is stale: expected {{{expected}}}, found {{{found}}}"
        ));
    }
}

/// The retired Docker/Java OWL-reasoner lane's first-party Python symbols. That lane (ROBOT +
/// Jena pinned-image subprocess reasoning) has been permanently removed; no `src/gmeow_tools`
/// module or the root `conftest.py` may reference these again. This seals the deletion against
/// re-introduction — the Python-surface complement of the Makefile + required-CI Docker-freedom
/// guards (`check_makefile_lane_purity` / `check_required_ci_jobs`).
const DOCKER_LANE_PYTHON_SYMBOLS: &[&str] = &[
    "gmeow_tools.runner",
    "image_available",
    "ROBOT_IMAGE",
    "JENA_IMAGE",
];

/// Hard-fail if any first-party Python (`src/gmeow_tools/**` + the root `conftest.py`) references
/// a retired Docker-reasoning-lane symbol. Scans code only (comments/strings are stripped), so a
/// docstring mentioning the removed lane does not trip it. Scoped ONLY to the permanently-gone
/// Docker symbols — NOT a general dead-module gate (that is a whole-surface concern for later).
fn check_no_docker_lane_python(root: &Path, report: &mut RepoStaticReport) {
    let mut files = python_files(&root.join("src").join("gmeow_tools"), report);
    let conftest = root.join("conftest.py");
    if conftest.is_file() {
        files.push(conftest);
    }
    files.sort();
    // Match each retired symbol on a word boundary, NOT as a bare substring: `contains` would
    // false-positive when a symbol is a substring of a live identifier (`ROBOT_IMAGE` ⊂
    // `ROBOT_IMAGE_PATH`, `image_available` ⊂ `is_image_available`). `regex::escape` neutralizes
    // the `.` in `gmeow_tools.runner`; `_` is a word char, so `\bROBOT_IMAGE\b` cannot match
    // inside `ROBOT_IMAGE_PATH` (the boundary between `E` and `_` fails).
    let mut symbol_regexes: Vec<(&&str, Regex)> =
        Vec::with_capacity(DOCKER_LANE_PYTHON_SYMBOLS.len());
    for sym in DOCKER_LANE_PYTHON_SYMBOLS {
        let pattern = format!(r"\b{}\b", regex::escape(sym));
        match Regex::new(&pattern) {
            Ok(re) => symbol_regexes.push((sym, re)),
            Err(err) => {
                report.error(format!(
                    "Docker-reasoning-lane guard: symbol regex for `{sym}` failed to compile: {err}"
                ));
                return;
            }
        }
    }
    for path in files {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        let code = strip_python_non_code(&text);
        let hits = symbol_regexes
            .iter()
            .filter(|(_, re)| re.is_match(&code))
            .map(|(sym, _)| **sym)
            .collect::<Vec<_>>();
        if !hits.is_empty() {
            let rel = slash_path(path.strip_prefix(root).unwrap_or(&path));
            report.error(format!(
                "retired Docker-reasoning-lane symbol(s) re-introduced in first-party Python {rel}: {} — that lane is permanently removed",
                hits.join(", ")
            ));
        }
    }
}

/// Hard-fail if the retired black-box SHACL test seam is re-introduced. The
/// `gmeow_tools.validate.run_shacl` helper (validate an rdflib graph → N-Triples → SHACL) and the
/// `tests/_graph_nt.py` rdflib→N-Triples adapter drove the domain conformance tests; that surface
/// is now native (`crates/validate/tests/conformance_*.rs` + `label_completeness.rs` over
/// `structural_lint_dataset`). This seals the deletion — the Python-surface complement of the
/// native conformance twins — so the next author cannot silently re-add a black-box Python SHACL
/// helper. Scans code only (comments/strings stripped), so a docstring mentioning `run_shacl` is
/// fine. NOT tripped by `src/gmeow_tools/language_tags.py`'s unrelated private `_graph_nt` helper
/// (that is not the `tests._graph_nt` module).
fn check_no_run_shacl_seam(root: &Path, report: &mut RepoStaticReport) {
    // 1. The rdflib→N-Triples adapter file must not exist.
    if root.join("tests").join("_graph_nt.py").is_file() {
        report.error(
            "tests/_graph_nt.py has been retired (its run_shacl / structural_lint shims are \
             native now); it must not be re-created"
                .to_owned(),
        );
    }

    // 2. No first-party Python may define a `run_shacl` helper or import `tests._graph_nt`.
    let mut files = python_files(&root.join("src").join("gmeow_tools"), report);
    for dir in [root.join("tests"), root.join("slices")] {
        if dir.is_dir() {
            collect_python_files(&dir, report, &mut files);
        }
    }
    files.sort();
    files.dedup();

    let run_shacl_def = match Regex::new(r"\bdef\s+run_shacl\b") {
        Ok(re) => re,
        Err(err) => {
            report.error(format!(
                "run_shacl-seam guard: def regex failed to compile: {err}"
            ));
            return;
        }
    };
    let graph_nt_import = match Regex::new(r"\btests\._graph_nt\b") {
        Ok(re) => re,
        Err(err) => {
            report.error(format!(
                "run_shacl-seam guard: import regex failed to compile: {err}"
            ));
            return;
        }
    };

    for path in files {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        let code = strip_python_non_code(&text);
        let rel = slash_path(path.strip_prefix(root).unwrap_or(&path));
        if run_shacl_def.is_match(&code) {
            report.error(format!(
                "black-box SHACL test seam re-introduced in {rel}: `def run_shacl` — SHACL \
                 conformance is native now (crates/validate/tests/conformance_*.rs)"
            ));
        }
        if graph_nt_import.is_match(&code) {
            report.error(format!(
                "import of the retired tests._graph_nt rdflib→N-Triples seam in {rel} — it has \
                 been deleted"
            ));
        }
    }
}

/// The SHACL namespace — the SHACL-AF computational vocabulary all lives under it, so the
/// `shacl#` substring appears (in a `@prefix` binding or a full IRI) in every file that uses it
/// regardless of the prefix chosen. Used as a cheap pre-filter before parsing.
const SHACL_NS: &str = "http://www.w3.org/ns/shacl#";

/// The SHACL **computational** (derivation) *property* IRIs whose subject node declares a
/// computational construct. Resolved as IRIs (not source tokens), so an alternate prefix or a
/// full IRI cannot bypass the gate. The SHACL *constraint* vocabulary (`sh:sparql` /
/// `sh:SPARQLTarget` / `sh:SPARQLConstraint`) is validation, not computation, and is excluded.
const COMPUTE_PROPERTY_LOCALS: &[&str] = &["rule", "js", "values"];

/// The SHACL **computational** *class* IRIs: a subject typed with one of these (via `rdf:type`)
/// declares a computational construct (the rule node itself).
const COMPUTE_CLASS_LOCALS: &[&str] = &["SPARQLRule", "TripleRule", "JSRule"];

/// The Hybrid-placement back-reference IRI (`logic:formalizes`) that legalizes a hand-authored
/// projection-surface construct: it names the `logic:` source the construct is the projection of.
const PROJECTION_FORMALIZES_IRI: &str = "https://blackcatinformatics.ca/logic/formalizes";

/// Whether `node` is back-referenced: it carries `logic:formalizes` directly, or it is reachable
/// upward — through the computational-property edges that link a shape to its rule node — from a
/// node that does. This binds the back-reference to the *specific* construct node (or its owning
/// shape), so an unrelated `logic:formalizes` triple elsewhere in the file cannot legalize it.
fn formalizes_backed(
    node: TermId,
    directly_backed: &BTreeSet<TermId>,
    parents: &BTreeMap<TermId, BTreeSet<TermId>>,
) -> bool {
    let mut stack = vec![node];
    let mut seen = BTreeSet::new();
    while let Some(n) = stack.pop() {
        if !seen.insert(n) {
            continue;
        }
        if directly_backed.contains(&n) {
            return true;
        }
        if let Some(ps) = parents.get(&n) {
            stack.extend(ps.iter().copied());
        }
    }
    false
}

/// A short display label for a subject node in a diagnostic.
fn node_label(ds: &RdfDataset, id: TermId) -> String {
    match ds.resolve(id) {
        TermRef::Iri(iri) => format!("<{iri}>"),
        _ => "[blank node]".to_owned(),
    }
}

/// Computation-surfaces-are-projections purity gate (Principles 17/4/12,
/// `design/LOGIC-SHACL-AF.md` / `design/LOGIC-RDFQUERY.md`): scan the authored RDF sources
/// (`slices/` + `dsl/`, `.ttl` only — NOT `generated/`, NOT prose `.md` docs) for computational
/// SHACL-AF vocabulary. The check is **Turtle-aware**: it parses each file and resolves the
/// computational predicates/classes as IRIs (so an alternate prefix or a full IRI cannot bypass
/// it), and it requires the `logic:formalizes` back-reference **on the construct node itself or
/// its owning shape** (so an unrelated back-reference elsewhere in the file cannot legalize it).
/// A construct without such a back-reference is a hand-authored second source of truth and fails.
fn check_projection_compute_purity(root: &Path, report: &mut RepoStaticReport) {
    let mut ttl_files = Vec::new();
    for sub in ["slices", "dsl"] {
        let dir = root.join(sub);
        if dir.is_dir() {
            collect_ttl_files(&dir, report, &mut ttl_files);
        }
    }
    ttl_files.sort();
    for path in ttl_files {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        // Cheap pre-filter: the SHACL namespace IRI is present (as a prefix binding or full IRI)
        // in any file that uses SHACL vocabulary, whatever prefix it binds.
        if !text.contains(SHACL_NS) {
            continue;
        }
        let rel = slash_path(path.strip_prefix(root).unwrap_or(&path));
        let ds = match purrdf::parse_dataset(text.as_bytes(), "text/turtle", None) {
            Ok(ds) => ds,
            Err(err) => {
                report.error(format!("{rel}: does not parse as Turtle: {err}"));
                continue;
            }
        };

        // Construct-bearing subjects, with the marker that flagged them, and the shape→rule-node
        // parent links the back-reference may sit on.
        let mut construct_subjects: BTreeMap<TermId, BTreeSet<String>> = BTreeMap::new();
        let mut parents: BTreeMap<TermId, BTreeSet<TermId>> = BTreeMap::new();

        for local in COMPUTE_PROPERTY_LOCALS {
            let Some(pid) = iri_id_static(&ds, &format!("{SHACL_NS}{local}")) else {
                continue;
            };
            for q in ds.quads_for_pattern(None, Some(pid), None, GraphMatch::Any) {
                construct_subjects
                    .entry(q.s)
                    .or_default()
                    .insert(format!("sh:{local}"));
                parents.entry(q.o).or_default().insert(q.s);
            }
        }
        if let Some(type_id) = iri_id_static(&ds, rdf::TYPE) {
            for local in COMPUTE_CLASS_LOCALS {
                let Some(cid) = iri_id_static(&ds, &format!("{SHACL_NS}{local}")) else {
                    continue;
                };
                for q in ds.quads_for_pattern(None, Some(type_id), Some(cid), GraphMatch::Any) {
                    construct_subjects
                        .entry(q.s)
                        .or_default()
                        .insert(format!("sh:{local}"));
                }
            }
        }
        if construct_subjects.is_empty() {
            continue;
        }

        let mut directly_backed: BTreeSet<TermId> = BTreeSet::new();
        if let Some(fid) = iri_id_static(&ds, PROJECTION_FORMALIZES_IRI) {
            for q in ds.quads_for_pattern(None, Some(fid), None, GraphMatch::Any) {
                directly_backed.insert(q.s);
            }
        }

        for (subj, markers) in &construct_subjects {
            if formalizes_backed(*subj, &directly_backed, &parents) {
                continue;
            }
            let constructs: Vec<&str> = markers.iter().map(String::as_str).collect();
            report.error(format!(
                "{rel}: {} hand-authors computational SHACL-AF vocabulary ({}) without a \
                 `logic:formalizes` back-reference on it or its owning shape: computation is \
                 authored in the logic: canon and PROJECTED to these surfaces under generated/ \
                 (Principle 17), never hand-authored as a second source of truth \
                 (design/LOGIC-SHACL-AF.md)",
                node_label(&ds, *subj),
                constructs.join(", ")
            ));
        }
    }
}

/// The gmeow domain namespace — the migrated FOL-axiom predicates live under it.
const GMEOW_NS_STATIC: &str = "https://blackcatinformatics.ca/gmeow/";

/// The migrated irreflexive/acyclic predicates: a hand-authored `sh:select` self-reference
/// `$this <P> $this` (optionally `+`/`*`) IS an irreflexivity/acyclicity axiom — a logical
/// characteristic that must be authored in the logic: canon and PROJECTED, not hand-authored.
const MIGRATED_SELF_PREDS: &[&str] = &["counterGoal", "overrides", "linkNext"];

/// The migrated relatum-distinctness role-pairs: a `sh:select` binding both roles to one value
/// IS a mutual-inequality axiom (`logic:RelatumDistinctnessAssertion`). Detected by both role
/// IRIs co-occurring in one select body — a pattern the retained closed-world checks never use.
const MIGRATED_DISTINCT_PAIRS: &[(&str, &str)] = &[
    ("committedAgent", "commitmentBeneficiary"),
    ("precedenceHigher", "precedenceLower"),
    ("rewardPole", "penaltyPole"),
    ("linkAntecedent", "linkConsequent"),
];

/// The shape-half of the projection-purity seal (the peer of [`check_projection_compute_purity`]):
/// an authored `sh:sparql`/`sh:select` that re-encodes a migrated open-world FOL axiom
/// (irreflexivity / acyclicity / relatum-distinctness — the distinctive self-reference and
/// coincident-role signatures) is a hand-authored second source of truth. It must be authored in
/// the logic: canon and PROJECTED to `generated/shapes/constraint-shapes.ttl` (Principle 17), or
/// carry a `logic:formalizes` back-reference on the construct or its owning shape. This realizes
/// the `sh:sparql` procedural-constraint fragment of the shape gate
/// (`design/LOGIC-VALIDATION.md`); the declarative `sh:PropertyShape` fragment follows as the
/// remaining closed-world shapes migrate (the same incremental realization the frame/result shape
/// stages are already described under). Scans `shapes/` (where the FOL SHACL lived) as well as
/// `slices/` + `dsl/`.
fn check_projection_shape_purity(root: &Path, report: &mut RepoStaticReport) {
    let mut ttl_files = Vec::new();
    for sub in ["shapes", "slices", "dsl"] {
        let dir = root.join(sub);
        if dir.is_dir() {
            collect_ttl_files(&dir, report, &mut ttl_files);
        }
    }
    ttl_files.sort();
    for path in ttl_files {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        if !text.contains(SHACL_NS) {
            continue;
        }
        let rel = slash_path(path.strip_prefix(root).unwrap_or(&path));
        let ds = match purrdf::parse_dataset(text.as_bytes(), "text/turtle", None) {
            Ok(ds) => ds,
            Err(err) => {
                report.error(format!("{rel}: does not parse as Turtle: {err}"));
                continue;
            }
        };

        // Parents: a `sh:sparql` / `sh:target` construct block → its owning shape, so a
        // `logic:formalizes` on the shape legalizes the block (the upward walk).
        let mut parents: BTreeMap<TermId, BTreeSet<TermId>> = BTreeMap::new();
        for local in ["sparql", "target"] {
            let Some(pid) = iri_id_static(&ds, &format!("{SHACL_NS}{local}")) else {
                continue;
            };
            for q in ds.quads_for_pattern(None, Some(pid), None, GraphMatch::Any) {
                parents.entry(q.o).or_default().insert(q.s);
            }
        }
        let mut directly_backed: BTreeSet<TermId> = BTreeSet::new();
        if let Some(fid) = iri_id_static(&ds, PROJECTION_FORMALIZES_IRI) {
            for q in ds.quads_for_pattern(None, Some(fid), None, GraphMatch::Any) {
                directly_backed.insert(q.s);
            }
        }

        let Some(sel_id) = iri_id_static(&ds, &format!("{SHACL_NS}select")) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(sel_id), None, GraphMatch::Any) {
            let TermRef::Literal { lexical, .. } = ds.resolve(q.o) else {
                continue;
            };
            // The sh:select lexical with ALL whitespace removed. The seal is a lexical
            // heuristic; stripping whitespace means a hand-authored re-encoding cannot slip
            // it by padding the `<prop> $this` self-loop (or the `<prop1> … <prop2>` pair)
            // with tabs, newlines, or extra spaces. IRIs and `$this` carry no interior
            // whitespace, so removal preserves every token while collapsing the evasion
            // surface (it also defeats a stray space before a `+` property-path modifier).
            let sel: String = lexical.split_whitespace().collect();
            let mut matched: Option<String> = None;
            for p in MIGRATED_SELF_PREDS {
                let base = format!("{GMEOW_NS_STATIC}{p}>");
                if sel.contains(&format!("{base}$this")) || sel.contains(&format!("{base}+$this")) {
                    matched = Some(format!("irreflexivity/acyclicity on gmeow:{p}"));
                }
            }
            for (a, b) in MIGRATED_DISTINCT_PAIRS {
                if sel.contains(&format!("{GMEOW_NS_STATIC}{a}>"))
                    && sel.contains(&format!("{GMEOW_NS_STATIC}{b}>"))
                {
                    matched = Some(format!("relatum-distinctness on gmeow:{a}/gmeow:{b}"));
                }
            }
            let Some(desc) = matched else {
                continue;
            };
            if formalizes_backed(q.s, &directly_backed, &parents) {
                continue;
            }
            report.error(format!(
                "{rel}: a hand-authored sh:sparql re-encodes the migrated FOL axiom \
                 ({desc}) without a `logic:formalizes` back-reference on it or its owning \
                 shape: this axiom is authored in the logic: canon and PROJECTED to \
                 generated/shapes/constraint-shapes.ttl (Principle 17, H8), never \
                 hand-authored as a second source of truth (design/LOGIC-VALIDATION.md)"
            ));
        }
    }
}

/// Resolve an IRI to its interned [`TermId`] in `ds`, if present.
fn iri_id_static(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

fn collect_ttl_files(dir: &Path, report: &mut RepoStaticReport, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            report.error(format!("{}: cannot read directory: {err}", dir.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                report.error(format!(
                    "{}: cannot read directory entry: {err}",
                    dir.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        // Recurse into real subdirectories only — a symlinked directory could form a cycle and
        // recurse forever (is_dir follows symlinks, so the !is_symlink guard is required).
        if path.is_dir() && !path.is_symlink() {
            collect_ttl_files(&path, report, out);
        } else if !path.is_symlink() && path.extension().is_some_and(|ext| ext == "ttl") {
            out.push(path);
        }
    }
}

fn check_lane_purity(root: &Path, report: &mut RepoStaticReport) {
    check_required_ci_jobs(root, report);
    // The ELK/HermiT/Docker oracle lane has been DELETED: its
    // .github/workflows/classic-cross-check.yml workflow is gone, so there is no
    // dedicated oracle workflow to structurally police. What remains is enforcing
    // that the Makefile is entirely Docker-free (below) and that required CI never
    // re-introduces the oracle tokens (in check_required_ci_jobs).
    check_makefile_lane_purity(root, report);
}

fn check_required_ci_jobs(root: &Path, report: &mut RepoStaticReport) {
    let rel = ".github/workflows/ci.yml";
    let Some(text) = read_required(root, rel, report) else {
        return;
    };
    let Some(ci) = parse_yaml(rel, &text, report) else {
        return;
    };
    let Some(jobs) = yaml_get(&ci, "jobs").and_then(Yaml::as_mapping) else {
        report.error(format!("{rel}: missing jobs mapping"));
        return;
    };
    let Some(quality) = yaml_map_get(jobs, "quality") else {
        report.error(format!("{rel}: missing jobs.quality"));
        return;
    };
    let needs = match yaml_get(quality, "needs") {
        Some(Yaml::Sequence(items)) => items
            .iter()
            .filter_map(Yaml::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        Some(Yaml::String(one)) => vec![one.clone()],
        _ => {
            report.error(format!("{rel}: jobs.quality.needs must list gate jobs"));
            return;
        }
    };
    if needs.is_empty() {
        report.error(format!("{rel}: jobs.quality.needs must not be empty"));
        return;
    }

    let mut required_jobs = needs.clone();
    required_jobs.push("quality".to_owned());
    for job_name in &required_jobs {
        let Some(job) = yaml_map_get(jobs, job_name) else {
            report.error(format!("{rel}: quality needs missing job {job_name:?}"));
            continue;
        };
        let blob = recursive_yaml_text(job);
        let hits = forbidden_hits(&blob);
        if !hits.is_empty() {
            report.error(format!(
                "required CI job {job_name:?} reaches Docker/Java: {}",
                hits.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        let lowered = blob.to_lowercase();
        for token in [
            "make maint-classic-cross-check",
            "--reasoner hermit",
            "--reasoner elk",
        ] {
            if lowered.contains(token) {
                report.error(format!(
                    "required CI job {job_name:?} invokes the oracle lane: {token:?}"
                ));
            }
        }
    }

    if needs.iter().any(|need| need == "classic-cross-check") {
        report.error(format!(
            "{rel}: classic-cross-check must not appear in quality.needs"
        ));
    }
}

fn check_makefile_lane_purity(root: &Path, report: &mut RepoStaticReport) {
    let rel = "Makefile";
    let Some(text) = read_required(root, rel, report) else {
        return;
    };
    let recipes = makefile_recipes(&text);
    let lane_targets = LANE_MAKE_TARGETS.iter().copied().collect::<BTreeSet<_>>();

    for (target, lines) in &recipes {
        if lane_targets.contains(target.as_str()) {
            continue;
        }
        let hits = forbidden_hits(&lines.join("\n"));
        if !hits.is_empty() {
            report.error(format!(
                "non-lane Makefile target {target:?} reaches Docker/Java: {}",
                hits.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }

    if !recipes.contains_key("check") {
        report.error("Makefile: the `check` target vanished");
    }
}

fn read_required(root: &Path, rel: &str, report: &mut RepoStaticReport) -> Option<String> {
    let path = root.join(rel);
    match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(err) => {
            report.error(format!("{rel}: cannot read: {err}"));
            None
        }
    }
}

fn parse_yaml(rel: &str, text: &str, report: &mut RepoStaticReport) -> Option<Yaml> {
    match serde_yaml::from_str::<Yaml>(text) {
        Ok(value) => Some(value),
        Err(err) => {
            report.error(format!("{rel}: cannot parse YAML: {err}"));
            None
        }
    }
}

fn yaml_get<'a>(value: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    value
        .as_mapping()
        .and_then(|mapping| yaml_map_get(mapping, key))
}

fn yaml_map_get<'a>(mapping: &'a serde_yaml::Mapping, key: &str) -> Option<&'a Yaml> {
    mapping.get(Yaml::String(key.to_owned()))
}

fn recursive_yaml_text(value: &Yaml) -> String {
    match value {
        Yaml::String(s) => s.clone(),
        Yaml::Number(n) => n.to_string(),
        Yaml::Bool(b) => b.to_string(),
        Yaml::Sequence(items) => items
            .iter()
            .map(recursive_yaml_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Yaml::Mapping(map) => map
            .iter()
            .flat_map(|(k, v)| [recursive_yaml_text(k), recursive_yaml_text(v)])
            .collect::<Vec<_>>()
            .join("\n"),
        Yaml::Null | Yaml::Tagged(_) => String::new(),
    }
}

fn forbidden_hits(text: &str) -> BTreeSet<String> {
    let mut hits = BTreeSet::new();
    for (pattern, re) in DOCKER_PATTERNS.iter().zip(DOCKER_REGEXES.iter()) {
        if re.is_match(text) {
            hits.insert((*pattern).to_owned());
        }
    }
    let lowered = text.to_lowercase();
    hits.extend(
        LANE_SCRIPTS
            .iter()
            .filter(|script| lowered.contains(&script.to_lowercase()))
            .map(|script| (*script).to_owned()),
    );
    hits
}

fn makefile_recipes(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut recipes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if line.starts_with('\t') {
            if let Some(target) = &current {
                recipes
                    .entry(target.clone())
                    .or_default()
                    .push(line.to_owned());
            }
            continue;
        }
        if let Some(target) = makefile_target_name(line) {
            recipes.entry(target.clone()).or_default();
            current = Some(target);
        } else if !line.trim().is_empty() && !line.starts_with('#') {
            current = None;
        }
    }
    recipes
}

fn makefile_target_name(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let first = *bytes.first()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut end = 1;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'-')
    {
        end += 1;
    }
    let mut cursor = end;
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b':') || bytes.get(cursor + 1) == Some(&b'=') {
        return None;
    }
    Some(line[..end].to_owned())
}

fn python_imported_top_modules(code: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let import_re = Regex::new(r"(?m)^\s*import\s+([^\n]+)").expect("static regex");
    for caps in import_re.captures_iter(code) {
        for part in caps[1].split(',') {
            let name = part
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .split('.')
                .next()
                .unwrap_or_default();
            if is_identifier(name) {
                out.insert(name.to_owned());
            }
        }
    }
    let from_re =
        Regex::new(r"(?m)^\s*from\s+([A-Za-z_][A-Za-z0-9_\.]*)\s+import\b").expect("static regex");
    for caps in from_re.captures_iter(code) {
        if let Some(name) = caps[1].split('.').next().filter(|name| is_identifier(name)) {
            out.insert(name.to_owned());
        }
    }
    out
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn strip_python_non_code(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0;
    while idx < chars.len() {
        let ch = chars[idx];
        if ch == '#' {
            while idx < chars.len() && chars[idx] != '\n' {
                idx += 1;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            idx = skip_python_string(&chars, idx, &mut out);
            continue;
        }
        out.push(ch);
        idx += 1;
    }
    out
}

fn skip_python_string(chars: &[char], start: usize, out: &mut String) -> usize {
    let quote = chars[start];
    let triple = chars.get(start + 1) == Some(&quote) && chars.get(start + 2) == Some(&quote);
    let mut idx = start + if triple { 3 } else { 1 };
    while idx < chars.len() {
        if chars[idx] == '\n' {
            out.push('\n');
            idx += 1;
            continue;
        }
        if chars[idx] == '\\' {
            idx = (idx + 2).min(chars.len());
            continue;
        }
        if triple {
            if chars[idx] == quote
                && chars.get(idx + 1) == Some(&quote)
                && chars.get(idx + 2) == Some(&quote)
            {
                return idx + 3;
            }
            idx += 1;
        } else if chars[idx] == quote {
            return idx + 1;
        } else {
            idx += 1;
        }
    }
    idx
}

fn python_files(src: &Path, report: &mut RepoStaticReport) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_python_files(src, report, &mut out);
    out.sort();
    out
}

fn collect_python_files(dir: &Path, report: &mut RepoStaticReport, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            report.error(format!("{}: cannot read directory: {err}", dir.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                report.error(format!(
                    "{}: cannot read directory entry: {err}",
                    dir.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            collect_python_files(&path, report, out);
        } else if path.extension().is_some_and(|ext| ext == "py") {
            out.push(path);
        }
    }
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

// ── generated/ produce-path read ban ────────────────────────────────────

/// No pipeline PRODUCE stage may read a `generated/` artifact off disk. Every `generated/`
/// file has exactly one producing stage, so a consumer must add a `dataflowConsumes` edge
/// and read the producer's IN-MEMORY product; a disk read at `run()` time carries the
/// last-committed bytes forever (the post-pipeline fanout rewrites those files from the
/// bundle), so `make regenerate` reports "unchanged" while `check-generated` reds
/// permanently — the stale-disk-fold bug class. This gate scans every Rust source
/// under `crates/pipeline/src/` (recursively — no produce helper outside `stages/`
/// can silently escape the gate) and flags any DISK-PATH construction under
/// `generated/`: a literal `.join("generated"…)` (whitespace- and `format!`-tolerant) or a
/// `.join(NAME)` — including the `.join(&NAME)` / `.join(NAME.as_str())` indirections — where
/// `NAME` is a `const … = "generated/…"` path constant. A read from a stage PRODUCT
/// (`.artifact(NAME)`) is not a disk read and is never flagged. A read whose `.join(` argument is
/// split across physical lines or aliased through a local binding is left to the fixed-point
/// backstop below (a single-line textual scanner cannot see it).
///
/// Exemptions (both fail the class OPEN unless justified): `#[cfg(test)] mod …` blocks (test
/// fixtures may mirror committed files) and a read carrying an inline `// GENERATED-READ-OK:
/// <reason>` marker in the contiguous comment block directly above it (or trailing on the line
/// itself) — reserved for any read whose result NEVER folds into `gmeow.gts`: dev-CLI audit
/// lanes (lint committed output), verification oracles, the monotonic-changelog prior-state
/// read, and the gitignored `.pipeline-cache` scratch dir. The textual gate catches literal +
/// traceable const-indirected reads; the pipeline's regenerate→check-generated fixed-point test
/// is the semantic backstop for the rest.
fn check_no_generated_read_in_pipeline_stages(root: &Path, report: &mut RepoStaticReport) {
    let src = root.join("crates").join("pipeline").join("src");
    // Nothing to scan when the pipeline crate is absent (synthetic minimal-repo fixtures).
    // The real repo always carries it; `live_repo_static_passes` scans it on-gate.
    if !src.is_dir() {
        return;
    }
    let mut files = Vec::new();
    collect_rust_files(&src, report, &mut files);
    files.sort();

    // Pass 1: collect path constants whose value is under `generated/` (emit targets), across
    // ALL scanned files. Reading one of these via `.join(NAME)` builds a disk path; a product
    // read (`.artifact(NAME)`) does not, so only `.join(NAME)` is flagged in pass 2.
    let const_re = match Regex::new(
        r#"const\s+([A-Z0-9_]+)\s*:\s*&(?:'static\s+)?str\s*=\s*"generated(?:/|")"#,
    ) {
        Ok(re) => re,
        Err(err) => {
            report.error(format!(
                "generated-read guard: const regex failed to compile: {err}"
            ));
            return;
        }
    };
    let mut generated_consts: BTreeSet<String> = BTreeSet::new();
    for path in &files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        for cap in const_re.captures_iter(&text) {
            generated_consts.insert(cap[1].to_string());
        }
    }

    // A literal `.join("generated"…)` disk read: tolerant of whitespace after `(`, a
    // `format!(` wrapper, and raw strings — so a re-encoding cannot slip past a naive
    // single-form `contains` check.
    let literal_re = match Regex::new(r#"\.join\(\s*(?:format!\s*\(\s*)?r?#*"generated"#) {
        Ok(re) => re,
        Err(err) => {
            report.error(format!(
                "generated-read guard: literal regex failed to compile: {err}"
            ));
            return;
        }
    };

    // Pass 2: flag disk-path construction under `generated/` in produce code.
    for path in &files {
        let rel = slash_path(path.strip_prefix(root).unwrap_or(path));
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{rel}: cannot read: {err}"));
                continue;
            }
        };
        // `detect`: comments blanked (so prose mentioning generated/ paths cannot match),
        // string literals KEPT (so `.join("generated"…)` is still visible), and every
        // `#[cfg(test)] mod …` body blanked (test fixtures are exempt). Line-aligned with the
        // original, so line numbers and the GENERATED-READ-OK look-up stay correct.
        let detect = blank_comments_and_cfg_test_modules(&text);
        let orig_lines: Vec<&str> = text.lines().collect();
        for (idx, line) in detect.lines().enumerate() {
            let literal = literal_re.is_match(line);
            // A `.join(NAME)` builds a disk path from a generated/ const. Catch the
            // idiomatic indirections too — `.join(&NAME)` (borrow), `.join(NAME.as_str())`
            // (method), and `.join(NAME, …)` (wrapped first arg) — each bounded by a
            // terminator (`)`/`.`/`,`) so one const name that prefixes another cannot
            // false-match. A `.artifact(NAME)` PRODUCT read has no `.join(` and is ignored.
            let const_indirect = generated_consts.iter().any(|name| {
                [')', '.', ','].iter().any(|t| {
                    line.contains(&format!(".join({name}{t}"))
                        || line.contains(&format!(".join(&{name}{t}"))
                })
            });
            if !(literal || const_indirect) {
                continue;
            }
            if generated_read_ok_marked(&orig_lines, idx) {
                continue;
            }
            report.error(format!(
                "{rel}:{}: pipeline produce-stage constructs a generated/ disk path \
                 (stale-disk-fold bug class) — consume the producing stage's in-memory \
                 product instead of reading the committed file, or, for a dev-CLI AUDIT-lane read \
                 of committed output, mark it `// GENERATED-READ-OK: <reason>`: {}",
                idx + 1,
                orig_lines.get(idx).unwrap_or(&"").trim()
            ));
        }
    }
}

/// True if the contiguous line-comment block directly above `idx` (or the line itself)
/// carries the `GENERATED-READ-OK` marker — the audit-lane exemption.
fn generated_read_ok_marked(orig_lines: &[&str], idx: usize) -> bool {
    if orig_lines
        .get(idx)
        .is_some_and(|l| l.contains("GENERATED-READ-OK"))
    {
        return true;
    }
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let trimmed = orig_lines[i].trim_start();
        if !trimmed.starts_with("//") {
            break;
        }
        if trimmed.contains("GENERATED-READ-OK") {
            return true;
        }
    }
    false
}

/// Return `text` with (a) all comments and (b) every `#[cfg(test)]`-attributed item body
/// replaced by spaces, preserving newlines (so line numbers and column offsets are unchanged)
/// and KEEPING string-literal contents (so `.join("generated"…)` stays visible to the scanner).
/// A Rust-aware char scanner — handling line/block comments, string / raw-string / byte-string
/// literals, char literals **distinguished from lifetimes** (`'a`, `'static`), and byte prefixes
/// — builds a `skeleton` (strings + comments blanked) so the `#[cfg(test)]` item body can be
/// brace-matched without being fooled by braces inside strings/comments. Works entirely in CHAR
/// indices (never byte offsets), so multi-byte chars (→, ∪, ×) never misalign it.
fn blank_comments_and_cfg_test_modules(text: &str) -> String {
    let src: Vec<char> = text.chars().collect();
    let n = src.len();
    let mut out: Vec<char> = src.clone();
    let mut skeleton: Vec<char> = src.clone();
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';

    let mut i = 0;
    while i < n {
        let c = src[i];
        // Line comment.
        if c == '/' && i + 1 < n && src[i + 1] == '/' {
            while i < n && src[i] != '\n' {
                out[i] = blank(src[i]);
                skeleton[i] = blank(src[i]);
                i += 1;
            }
            continue;
        }
        // Block comment (non-nesting is sufficient for this gate).
        if c == '/' && i + 1 < n && src[i + 1] == '*' {
            while i < n && !(src[i] == '*' && i + 1 < n && src[i + 1] == '/') {
                out[i] = blank(src[i]);
                skeleton[i] = blank(src[i]);
                i += 1;
            }
            if i + 1 < n {
                out[i] = blank(src[i]);
                out[i + 1] = blank(src[i + 1]);
                skeleton[i] = blank(src[i]);
                skeleton[i + 1] = blank(src[i + 1]);
                i += 2;
            }
            continue;
        }
        // Raw string: r"…" / r#…"…"#… (optionally byte-prefixed: br"…"). The opening `r`/`br`
        // must start an identifier boundary (not be part of a longer ident).
        let raw_start = (c == 'r' || (c == 'b' && i + 1 < n && src[i + 1] == 'r'))
            && (i == 0 || !is_ident(src[i - 1]));
        if raw_start {
            let mut j = if c == 'b' { i + 2 } else { i + 1 };
            let mut hashes = 0;
            while j < n && src[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && src[j] == '"' {
                // A genuine raw string: blank (skeleton only) through the closing "###.
                j += 1;
                loop {
                    if j >= n {
                        break;
                    }
                    if src[j] == '"' {
                        let mut h = 0;
                        while h < hashes && j + 1 + h < n && src[j + 1 + h] == '#' {
                            h += 1;
                        }
                        if h == hashes {
                            for p in j..=(j + hashes).min(n - 1) {
                                skeleton[p] = blank(src[p]);
                            }
                            j += 1 + hashes;
                            break;
                        }
                    }
                    skeleton[j] = blank(src[j]);
                    j += 1;
                }
                for p in i..j.min(n) {
                    skeleton[p] = blank(src[p]);
                }
                i = j;
                continue;
            }
        }
        // Normal / byte string.
        if c == '"' {
            skeleton[i] = blank(c);
            i += 1;
            while i < n {
                if src[i] == '\\' && i + 1 < n {
                    skeleton[i] = blank(src[i]);
                    skeleton[i + 1] = blank(src[i + 1]);
                    i += 2;
                    continue;
                }
                let end = src[i] == '"';
                skeleton[i] = blank(src[i]);
                i += 1;
                if end {
                    break;
                }
            }
            continue;
        }
        // Char literal vs lifetime. A char literal is `'x'` or `'\x'` — a `'`, an optional
        // escape + one char, then a closing `'`. A lifetime (`'a`, `'static`) has NO closing
        // `'`, so it must NOT enter char-literal scanning (the bug that desynced the scanner).
        if c == '\'' {
            let is_char_lit = if i + 1 < n && src[i + 1] == '\\' {
                // Escaped: `'\n'`, `'\x41'`, `'\u{1F}'` — closing quote is a few chars along.
                (i + 3 < n && src[i + 3] == '\'')
                    || (2..8).any(|k| i + 2 + k < n && src[i + 2 + k] == '\'')
            } else {
                i + 2 < n && src[i + 2] == '\''
            };
            if is_char_lit {
                let mut j = i + 1;
                while j < n {
                    if src[j] == '\\' && j + 1 < n {
                        skeleton[j] = blank(src[j]);
                        skeleton[j + 1] = blank(src[j + 1]);
                        j += 2;
                        continue;
                    }
                    let end = src[j] == '\'';
                    skeleton[j] = blank(src[j]);
                    j += 1;
                    if end {
                        break;
                    }
                }
                skeleton[i] = blank(c);
                i = j;
                continue;
            }
            // Lifetime: leave `'` as code, advance one char.
            i += 1;
            continue;
        }
        i += 1;
    }

    // On the skeleton, blank every `#[cfg(test)]`-attributed item body. After the attribute,
    // the item's brace-delimited body is the region from the next `{` to its matching `}`
    // (fn / mod / impl); items with no body before a `;` (a `use`/`const`) carry no read.
    let marker: Vec<char> = "#[cfg(test)]".chars().collect();
    let mut m = 0;
    while m + marker.len() <= skeleton.len() {
        if skeleton[m..m + marker.len()] != marker[..] {
            m += 1;
            continue;
        }
        // Find the item's opening brace, but stop at a `;` (a semicolon-terminated item has no
        // body to blank — e.g. `#[cfg(test)] use super::*;`).
        let mut j = m + marker.len();
        while j < skeleton.len() && skeleton[j] != '{' && skeleton[j] != ';' {
            j += 1;
        }
        if j >= skeleton.len() || skeleton[j] == ';' {
            m += marker.len();
            continue;
        }
        let mut depth = 0i32;
        let mut k = j;
        while k < skeleton.len() {
            match skeleton[k] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        k += 1;
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        for pos in j..k.min(out.len()) {
            out[pos] = blank(src[pos]);
        }
        m = k;
    }
    out.iter().collect()
}

/// Recursively collect `.rs` files under `dir`.
fn collect_rust_files(dir: &Path, report: &mut RepoStaticReport, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            report.error(format!("{}: cannot read directory: {err}", dir.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                report.error(format!(
                    "{}: cannot read directory entry: {err}",
                    dir.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, report, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn write_minimal_repo(root: &Path) {
        // First-party code is rdflib-free (no keeper): a minimal valid repo has a
        // real src/gmeow_tools package with NO upstream-rdflib import anywhere (it
        // uses the purrdf.compat.rdflib facade instead).
        write(&root.join("src/gmeow_tools/sparql.py"), "import purrdf\n");
        write(
            &root.join(".github/workflows/ci.yml"),
            "on:\n  push:\n  pull_request:\njobs:\n  lint:\n    steps:\n      - run: make lint\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
        );
        // The Docker-free reality: no target reaches Docker/Java. The ELK/HermiT
        // lane and its maint-reason-hermit / maint-verify-docker / maint-pull-images
        // targets are gone.
        write(
            &root.join("Makefile"),
            "check:\n\t$(MAKE) lint\nlint:\n\ttrue\n",
        );
    }

    #[test]
    fn python_scanner_ignores_comments_and_strings() {
        let code = strip_python_non_code(
            "import os\n# import rdflib\nTEXT = \"import purrdf\"\n'''load_merged_graph'''\n",
        );
        let imports = python_imported_top_modules(&code);
        assert!(imports.contains("os"));
        assert!(!imports.contains("rdflib"));
    }

    #[test]
    fn docker_lane_python_guard_flags_reintroduction_and_passes_clean() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Clean tree: a gmeow_tools module with no Docker-lane symbol → passes.
        write(&root.join("src/gmeow_tools/ok.py"), "import os\nX = 1\n");
        let mut clean = RepoStaticReport::default();
        check_no_docker_lane_python(root, &mut clean);
        assert!(
            clean.errors.is_empty(),
            "clean tree must pass; got {:?}",
            clean.errors
        );

        // Re-introducing a retired Docker-lane import in conftest.py must fail.
        write(
            &root.join("conftest.py"),
            "from gmeow_tools.runner import image_available\n",
        );
        let mut dirty = RepoStaticReport::default();
        check_no_docker_lane_python(root, &mut dirty);
        assert!(
            dirty
                .errors
                .iter()
                .any(|e| e.contains("Docker-reasoning-lane") && e.contains("conftest.py")),
            "a re-introduced gmeow_tools.runner import must be flagged; got {:?}",
            dirty.errors
        );

        // A retired symbol appearing only in a comment/string must NOT trip the guard.
        std::fs::remove_file(root.join("conftest.py")).unwrap();
        write(
            &root.join("src/gmeow_tools/ok.py"),
            "# image_available was removed\nDOC = \"ROBOT_IMAGE\"\n",
        );
        let mut commented = RepoStaticReport::default();
        check_no_docker_lane_python(root, &mut commented);
        assert!(
            commented.errors.is_empty(),
            "symbols in comments/strings must not trip the guard; got {:?}",
            commented.errors
        );

        // NEGATIVE: live identifiers that merely CONTAIN a retired symbol as a substring
        // (`is_image_available`, `ROBOT_IMAGE_PATH`, `JENA_IMAGE_PATH`, and a plain `runner` that is
        // NOT `gmeow_tools.runner`) must NOT trip the word-boundary guard.
        std::fs::remove_file(root.join("conftest.py")).ok();
        write(
            &root.join("src/gmeow_tools/ok.py"),
            "def is_image_available():\n    ROBOT_IMAGE_PATH = \"x\"\n    JENA_IMAGE_PATH = \"y\"\n    runner = object()\n    return runner\n",
        );
        let mut substrings = RepoStaticReport::default();
        check_no_docker_lane_python(root, &mut substrings);
        assert!(
            substrings.errors.is_empty(),
            "live identifiers that merely contain a retired symbol as a substring must not trip the guard; got {:?}",
            substrings.errors
        );

        // POSITIVE: each bare retired symbol on a word boundary MUST still trip the guard.
        write(
            &root.join("src/gmeow_tools/ok.py"),
            "from gmeow_tools.runner import image_available\nA = ROBOT_IMAGE\nB = JENA_IMAGE\n",
        );
        let mut bare = RepoStaticReport::default();
        check_no_docker_lane_python(root, &mut bare);
        let bare_msg = bare
            .errors
            .iter()
            .find(|e| e.contains("Docker-reasoning-lane") && e.contains("ok.py"));
        let bare_msg = bare_msg.unwrap_or_else(|| {
            panic!(
                "bare retired symbols must trip the guard; got {:?}",
                bare.errors
            )
        });
        for sym in [
            "gmeow_tools.runner",
            "image_available",
            "ROBOT_IMAGE",
            "JENA_IMAGE",
        ] {
            assert!(
                bare_msg.contains(sym),
                "guard message must list `{sym}`; got {bare_msg:?}"
            );
        }
    }

    #[test]
    fn run_shacl_seam_guard_flags_reintroduction_and_passes_clean() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Clean tree: gmeow_tools with no run_shacl def, no tests/_graph_nt.py → passes.
        write(
            &root.join("src/gmeow_tools/validate.py"),
            "import os\nX = 1\n",
        );
        let mut clean = RepoStaticReport::default();
        check_no_run_shacl_seam(root, &mut clean);
        assert!(
            clean.errors.is_empty(),
            "clean tree must pass; got {:?}",
            clean.errors
        );

        // A `def run_shacl` re-added to first-party Python must fail.
        write(
            &root.join("src/gmeow_tools/validate.py"),
            "def run_shacl(data_nt):\n    return None\n",
        );
        let mut def = RepoStaticReport::default();
        check_no_run_shacl_seam(root, &mut def);
        assert!(
            def.errors
                .iter()
                .any(|e| e.contains("black-box SHACL test seam") && e.contains("validate.py")),
            "a re-introduced `def run_shacl` must be flagged; got {:?}",
            def.errors
        );

        // Re-creating tests/_graph_nt.py must fail.
        write(&root.join("src/gmeow_tools/validate.py"), "X = 1\n");
        write(
            &root.join("tests/_graph_nt.py"),
            "def run_shacl(g):\n    return None\n",
        );
        let mut seam = RepoStaticReport::default();
        check_no_run_shacl_seam(root, &mut seam);
        assert!(
            seam.errors
                .iter()
                .any(|e| e.contains("tests/_graph_nt.py has been retired")),
            "a re-created tests/_graph_nt.py must be flagged; got {:?}",
            seam.errors
        );

        // Importing the retired seam must fail.
        std::fs::remove_file(root.join("tests/_graph_nt.py")).unwrap();
        write(
            &root.join("tests/test_thing.py"),
            "from tests._graph_nt import run_shacl\n",
        );
        let mut imp = RepoStaticReport::default();
        check_no_run_shacl_seam(root, &mut imp);
        assert!(
            imp.errors
                .iter()
                .any(|e| e.contains("tests._graph_nt") && e.contains("test_thing.py")),
            "an import of the retired seam must be flagged; got {:?}",
            imp.errors
        );

        // `run_shacl` / `tests._graph_nt` mentioned only in a comment or string must NOT trip it,
        // and the unrelated private `_graph_nt` helper in language_tags.py is fine.
        std::fs::remove_file(root.join("tests/test_thing.py")).unwrap();
        write(
            &root.join("src/gmeow_tools/language_tags.py"),
            "# run_shacl was retired; tests._graph_nt is gone\nDOC = \"def run_shacl\"\ndef _graph_nt(graph):\n    return graph\n",
        );
        let mut commented = RepoStaticReport::default();
        check_no_run_shacl_seam(root, &mut commented);
        assert!(
            commented.errors.is_empty(),
            "run_shacl/tests._graph_nt in comments/strings and the unrelated private _graph_nt \
             helper must not trip the guard; got {:?}",
            commented.errors
        );
    }

    #[test]
    fn projection_compute_purity_flags_unbacked_construct_and_passes_a_backed_one() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let module = root.join("slices/core/demo/module.ttl");

        // A hand-authored SHACL-AF derivation rule with NO logic:formalizes back-reference
        // is a forbidden second source of truth → the gate must fail.
        write(
            &module,
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
        );
        let mut report = RepoStaticReport::default();
        check_projection_compute_purity(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("computational SHACL-AF") && e.contains("module.ttl")),
            "a hand-authored sh:SPARQLRule without logic:formalizes must be flagged; got {:?}",
            report.errors
        );

        // The SAME construct WITH a logic:formalizes back-reference is the legal Hybrid
        // placement (it names its logic: source) → the gate must pass.
        write(
            &module,
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:S a sh:NodeShape ;\n    \
                 logic:formalizes ex:someLogicRule ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
        );
        let mut backed = RepoStaticReport::default();
        check_projection_compute_purity(root, &mut backed);
        assert!(
            backed.errors.is_empty(),
            "a logic:formalizes-backed construct must pass the purity gate; got {:?}",
            backed.errors
        );
    }

    #[test]
    fn purity_gate_catches_alternate_prefix_and_full_iri_bypass() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Alternate prefix bound to the SHACL namespace — a substring scan for "sh:rule" misses it.
        write(
            &root.join("slices/core/altprefix/module.ttl"),
            "@prefix af: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a af:NodeShape ;\n    \
                 af:rule [ a af:SPARQLRule ; \
                 af:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
        );
        // Full-IRI form — no SHACL prefix at all.
        write(
            &root.join("slices/core/fulliri/module.ttl"),
            "@prefix ex: <https://example.org/> .\n\
             ex:T a <http://www.w3.org/ns/shacl#NodeShape> ;\n    \
                 <http://www.w3.org/ns/shacl#rule> [ a <http://www.w3.org/ns/shacl#SPARQLRule> ; \
                 <http://www.w3.org/ns/shacl#construct> \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
        );
        let mut report = RepoStaticReport::default();
        check_projection_compute_purity(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("altprefix/module.ttl")),
            "an alternate-prefix computational construct must be flagged; got {:?}",
            report.errors
        );
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("fulliri/module.ttl")),
            "a full-IRI computational construct must be flagged; got {:?}",
            report.errors
        );
    }

    #[test]
    fn purity_gate_rejects_backref_on_unrelated_node_or_in_a_comment() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // logic:formalizes is present but on an UNRELATED node, not on the construct's shape — a
        // file-scoped substring check would wrongly pass this.
        write(
            &root.join("slices/core/unrelated/module.ttl"),
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:Unrelated logic:formalizes ex:somewhere .\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
        );
        // logic:formalizes appears ONLY in a comment → no triple → must still be flagged.
        write(
            &root.join("slices/core/comment/module.ttl"),
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             # ex:S logic:formalizes ex:source -- a comment is not a triple\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
        );
        let mut report = RepoStaticReport::default();
        check_projection_compute_purity(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("unrelated/module.ttl")),
            "a back-reference on an unrelated node must NOT legalize the construct; got {:?}",
            report.errors
        );
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("comment/module.ttl")),
            "a back-reference present only in a comment must NOT legalize the construct; got {:?}",
            report.errors
        );
    }

    #[test]
    fn collect_ttl_files_skips_symlinked_dirs_without_looping() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let dir = root.join("slices/core/loop");
        fs::create_dir_all(&dir).unwrap();
        write(
            &dir.join("real.ttl"),
            "@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n",
        );
        // A symlink back to an ancestor would make a naive recursion loop forever.
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("slices"), dir.join("cycle")).unwrap();
        let mut report = RepoStaticReport::default();
        let mut out = Vec::new();
        // Must terminate (no stack overflow / infinite loop) and still find the real file.
        collect_ttl_files(&root.join("slices"), &mut report, &mut out);
        assert!(
            out.iter().any(|p| p.ends_with("real.ttl")),
            "the real .ttl must be collected; got {out:?}"
        );
    }

    #[test]
    fn minimal_repo_passes() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        let report = check_repo_static(temp.path());
        assert!(report.ok(), "{:?}", report.errors);
    }

    #[test]
    fn rdflib_runtime_offender_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("src/gmeow_tools/bad.py"),
            "import rdflib\n",
        );
        let report = check_repo_static(temp.path());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("bad.py") && e.contains("upstream rdflib"))
        );
    }

    #[test]
    fn required_ci_docker_token_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join(".github/workflows/ci.yml"),
            "on:\n  pull_request:\njobs:\n  lint:\n    steps:\n      - run: docker run obolibrary/robot\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
        );
        let report = check_repo_static(temp.path());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("required CI job") && e.contains("docker"))
        );
    }

    #[test]
    fn required_ci_job_container_token_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join(".github/workflows/ci.yml"),
            "on:\n  pull_request:\njobs:\n  lint:\n    container: obolibrary/robot\n    steps:\n      - run: make lint\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
        );
        let report = check_repo_static(temp.path());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("required CI job") && e.contains("obolibrary/robot"))
        );
    }

    #[test]
    fn makefile_target_reaching_docker_fails() {
        // No legitimate Docker lane exists: any target that reaches `docker` is a
        // re-introduction of the deleted ELK/HermiT lane and must be flagged.
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("Makefile"),
            "check:\n\t$(MAKE) lint\nlint:\n\ttrue\nmaint-reason-hermit:\n\tdocker run obolibrary/robot\n",
        );
        let report = check_repo_static(temp.path());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("target \"maint-reason-hermit\"")
                    && e.contains("reaches Docker/Java")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn makefile_target_reaching_java_fails() {
        // Java is banned everywhere too — the classic reasoner was a Java robot.jar.
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("Makefile"),
            "check:\n\t$(MAKE) lint\nlint:\n\ttrue\nrobot:\n\tjava -jar robot.jar reason\n",
        );
        let report = check_repo_static(temp.path());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("target \"robot\"") && e.contains("reaches Docker/Java")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn makefile_target_invoking_pull_images_script_fails() {
        // pull-images.sh was deleted; shelling out to it re-introduces the lane.
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("Makefile"),
            "check:\n\t$(MAKE) lint\nlint:\n\ttrue\npull:\n\tbash scripts/pull-images.sh\n",
        );
        let report = check_repo_static(temp.path());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("target \"pull\"") && e.contains("pull-images.sh")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn required_ci_oracle_reasoner_token_fails() {
        // The oracle-token ban ENFORCES the lane's removal: a required CI job that
        // invokes `--reasoner hermit` / `--reasoner elk` must still be rejected.
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join(".github/workflows/ci.yml"),
            "on:\n  pull_request:\njobs:\n  lint:\n    steps:\n      - run: gmeow-dev reason --reasoner hermit\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
        );
        let report = check_repo_static(temp.path());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("invokes the oracle lane") && e.contains("--reasoner hermit")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn shape_purity_flags_unbacked_migrated_axioms_and_passes_backed_and_closed_world() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let g = "https://blackcatinformatics.ca/gmeow/";

        // A hand-authored irreflexivity self-reference axiom with NO logic:formalizes → flagged.
        write(
            &root.join("shapes/bad-irreflexive.ttl"),
            &format!(
                "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:S a sh:NodeShape ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}counterGoal> $this . }}\"\"\" ] .\n"
            ),
        );
        // A hand-authored coincident-role distinctness axiom, unbacked → flagged.
        write(
            &root.join("shapes/bad-distinct.ttl"),
            &format!(
                "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:S a sh:NodeShape ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}committedAgent> ?v . $this <{g}commitmentBeneficiary> ?v . }}\"\"\" ] .\n"
            ),
        );
        // The SAME irreflexivity axiom WITH a logic:formalizes on its owning shape → legal.
        write(
            &root.join("shapes/good-backed.ttl"),
            &format!(
                "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
                 ex:S a sh:NodeShape ; logic:formalizes ex:someAxiom ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}counterGoal> $this . }}\"\"\" ] .\n"
            ),
        );
        // A retained closed-world check (FILTER NOT EXISTS existence) → NOT a migrated axiom,
        // must NOT be flagged even without logic:formalizes.
        write(
            &root.join("shapes/closed-world.ttl"),
            &format!(
                "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:S a sh:NodeShape ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}deonticModality> ?m . FILTER NOT EXISTS {{ $this <{g}normIssuer> ?i . }} }}\"\"\" ] .\n"
            ),
        );

        // The SAME irreflexivity axiom, unbacked, but padded with a newline + tab + extra
        // spaces between the predicate and `$this` — a whitespace re-encoding a single-space
        // `contains` check would miss. Must still be flagged.
        write(
            &root.join("shapes/bad-irreflexive-ws.ttl"),
            &format!(
                "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:S a sh:NodeShape ;\n    \
                     sh:sparql [ a sh:SPARQLConstraint ; \
                     sh:select \"\"\"SELECT $this WHERE {{ $this <{g}counterGoal>\n\t  $this . }}\"\"\" ] .\n"
            ),
        );

        let mut report = RepoStaticReport::default();
        check_projection_shape_purity(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("bad-irreflexive.ttl")),
            "an unbacked irreflexivity self-reference axiom must be flagged; got {:?}",
            report.errors
        );
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("bad-irreflexive-ws.ttl")),
            "a whitespace-padded re-encoding of a migrated axiom must still be flagged; got {:?}",
            report.errors
        );
        assert!(
            report.errors.iter().any(|e| e.contains("bad-distinct.ttl")),
            "an unbacked coincident-role distinctness axiom must be flagged; got {:?}",
            report.errors
        );
        assert!(
            !report.errors.iter().any(|e| e.contains("good-backed.ttl")),
            "a logic:formalizes-backed axiom must pass; got {:?}",
            report.errors
        );
        assert!(
            !report.errors.iter().any(|e| e.contains("closed-world.ttl")),
            "a retained closed-world check must NOT be flagged; got {:?}",
            report.errors
        );
    }

    #[test]
    fn live_repo_static_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("validate crate should live under crates/");
        let report = check_repo_static(root);
        assert!(report.ok(), "{:?}", report.errors);
    }

    // ── generated/-read ban ─────────────────────────────────────────────

    #[test]
    fn blank_pass_keeps_strings_blanks_comments_and_cfg_test_bodies() {
        let src = "let a = root.join(\"generated/x.rq\"); // prose generated/y\n\
                   #[cfg(test)]\n\
                   mod t {\n    fn f() { let _ = root.join(\"generated/z.rq\"); }\n}\n";
        let out = blank_comments_and_cfg_test_modules(src);
        // Real string literals survive (so the scanner can still see them)…
        assert!(out.contains(".join(\"generated/x.rq\""));
        // …the line comment is blanked (prose mentioning a generated/ path cannot match)…
        assert!(!out.contains("generated/y"));
        // …and the whole `#[cfg(test)] mod` body is blanked (test fixtures are exempt).
        assert!(!out.contains("generated/z"));
        // Line count is preserved so line numbers stay aligned.
        assert_eq!(out.lines().count(), src.lines().count());
    }

    fn stage_file(root: &Path, name: &str, body: &str) {
        write(
            &root.join(format!("crates/pipeline/src/stages/{name}")),
            body,
        );
    }

    fn ban_errors(root: &Path) -> Vec<String> {
        let mut report = RepoStaticReport::default();
        check_no_generated_read_in_pipeline_stages(root, &mut report);
        report.errors
    }

    #[test]
    fn ban_flags_a_literal_generated_disk_read_in_a_produce_stage() {
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "fn run(root: &std::path::Path) {\n    let _ = list_files(&root.join(\"generated/queries\"), \"rq\");\n}\n",
        );
        let errs = ban_errors(temp.path());
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("stale-disk-fold"), "{errs:?}");
    }

    #[test]
    fn ban_flags_a_const_indirected_generated_disk_read() {
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "const DCAT: &str = \"generated/queries/dcat.rq\";\n\
             fn run(root: &std::path::Path) {\n    let _ = std::fs::read(root.join(DCAT));\n}\n",
        );
        let errs = ban_errors(temp.path());
        assert_eq!(errs.len(), 1, "{errs:?}");
    }

    #[test]
    fn ban_flags_a_borrowed_or_method_const_generated_read() {
        // Idiomatic indirections must not slip the ban: `.join(&NAME)` (borrow) and
        // `.join(NAME.as_str())` (method) both build a disk path from a generated const.
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "const DCAT: &str = \"generated/queries/dcat.rq\";\n\
             fn run(root: &std::path::Path) {\n\
             \x20   let _ = std::fs::read(root.join(&DCAT));\n\
             \x20   let _ = std::fs::read(root.join(DCAT.as_str()));\n}\n",
        );
        let errs = ban_errors(temp.path());
        assert_eq!(errs.len(), 2, "{errs:?}");
    }

    #[test]
    fn ban_ignores_a_product_read_of_a_generated_const() {
        // Reading the artifact off a stage PRODUCT (`.artifact(NAME)`) is not a disk read.
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "const DCAT: &str = \"generated/queries/dcat.rq\";\n\
             fn run(p: &Product) {\n    let _ = p.artifact(DCAT);\n}\n",
        );
        assert!(ban_errors(temp.path()).is_empty());
    }

    #[test]
    fn ban_exempts_cfg_test_modules() {
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "fn run() {}\n\
             #[cfg(test)]\nmod tests {\n    fn t(root: &std::path::Path) {\n        let _ = list_files(&root.join(\"generated/mappings\"), \"tsv\");\n    }\n}\n",
        );
        assert!(ban_errors(temp.path()).is_empty());
    }

    #[test]
    fn ban_exempts_a_marked_audit_read() {
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "fn audit(root: &std::path::Path) {\n\
             \x20   // GENERATED-READ-OK: audit lane, lints committed output, never folds into gmeow.gts.\n\
             \x20   let _ = root.join(\"generated/mappings\");\n}\n",
        );
        assert!(ban_errors(temp.path()).is_empty());
    }

    #[test]
    fn ban_ignores_prose_mentioning_generated_paths() {
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "fn run() {\n    // this fold used to read generated/queries off disk; now product-sourced.\n    let _ = 1;\n}\n",
        );
        assert!(ban_errors(temp.path()).is_empty());
    }

    // ── bypass-coverage: hardened literal regex + widened scan scope ─────

    fn pipeline_src_file(root: &Path, name: &str, body: &str) {
        write(&root.join(format!("crates/pipeline/src/{name}")), body);
    }

    #[test]
    fn ban_flags_a_whitespace_join_generated_read() {
        // A space after `.join(` — a naive `.contains(".join(\"generated")` misses it.
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "fn run(root: &std::path::Path) {\n    let _ = list_files(&root.join( \"generated/queries\"), \"rq\");\n}\n",
        );
        let errs = ban_errors(temp.path());
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("stale-disk-fold"), "{errs:?}");
    }

    #[test]
    fn ban_flags_a_format_join_generated_read() {
        // A `.join(format!("generated/{p}.rq"))` wrapper — must still be flagged.
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "fn run(root: &std::path::Path, p: &str) {\n    let _ = std::fs::read(root.join(format!(\"generated/{p}.rq\")));\n}\n",
        );
        let errs = ban_errors(temp.path());
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("stale-disk-fold"), "{errs:?}");
    }

    #[test]
    fn ban_flags_a_slashless_const_generated_read() {
        // A slash-less `const … = "generated";` used via `.join(G)` — the relaxed const regex
        // must catch the bare directory name, not only `"generated/…"`.
        let temp = tempfile::tempdir().unwrap();
        stage_file(
            temp.path(),
            "foo.rs",
            "const G: &str = \"generated\";\n\
             fn run(root: &std::path::Path) {\n    let _ = std::fs::read(root.join(G));\n}\n",
        );
        let errs = ban_errors(temp.path());
        assert_eq!(errs.len(), 1, "{errs:?}");
    }

    #[test]
    fn ban_flags_a_produce_read_outside_stages_dir() {
        // A produce read in a helper OUTSIDE stages/ — the old stages/-only scan would have
        // missed it; the widened recursive scan of crates/pipeline/src/ catches it.
        let temp = tempfile::tempdir().unwrap();
        pipeline_src_file(
            temp.path(),
            "helper.rs",
            "fn run(root: &std::path::Path) {\n    let _ = list_files(&root.join(\"generated/queries\"), \"rq\");\n}\n",
        );
        let errs = ban_errors(temp.path());
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("stale-disk-fold"), "{errs:?}");
    }
}
