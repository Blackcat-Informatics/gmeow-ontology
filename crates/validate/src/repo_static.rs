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
    check_projection_compute_purity(root, &mut report);
    check_projection_shape_purity(root, &mut report);
    // The BLANKET declarative-shape peer (`check_declarative_shape_purity`) is deliberately NOT
    // wired here yet: it is activated at the terminal migration increment once the legacy shape
    // corpus is deleted; until then it would red on the ~245 coexisting legacy shapes by design.
    // Its production semantics are proven now over the live tree by
    // `declarative_gate_flags_the_live_legacy_corpus`.
    check_authored_shex_purity(root, &mut report);
    check_hand_authored_shapes_ratchet(root, &mut report);
    check_no_generated_read_in_pipeline_stages(root, &mut report);
    check_no_first_party_error_crate_deps(root, &mut report);
    check_no_string_result_error_type(root, &mut report);
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

/// The BLANKET declarative-shape fragment of the projection-purity seal — the declarative
/// `sh:NodeShape` / `sh:PropertyShape` peer of the procedural `sh:sparql` fragment
/// ([`check_projection_shape_purity`]). Every validation shape is authored in the `logic:` canon
/// and PROJECTED to `generated/shapes/*.ttl` (Principle 17, `design/LOGIC-VALIDATION.md`), never
/// hand-authored as a second source of truth. So ANY authored `sh:NodeShape` or `sh:PropertyShape`
/// construct (a typed shape subject, or an inline shape reached through `sh:property`) that lacks a
/// `logic:formalizes` back-reference — on itself or on its owning node shape (the upward walk) — is
/// a violation. No peer/projected-surface condition, no allowlist: the rule is blanket.
///
/// NOT wired into [`check_repo_static`] yet. Activated at the terminal migration increment once the
/// legacy shape corpus is deleted; until then it would (correctly, by design) red on the ~245
/// coexisting legacy shapes that have not yet migrated. Its production semantics are proven now
/// over the live tree by `declarative_gate_flags_the_live_legacy_corpus`.
// Not yet reachable from a non-test build (activation is deferred to the terminal migration
// increment); its live-tree production semantics are exercised by the gate test below.
#[allow(dead_code)]
fn check_declarative_shape_purity(root: &Path, report: &mut RepoStaticReport) {
    let mut ttl_files = Vec::new();
    for sub in ["slices", "shapes", "dsl"] {
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

        // Construct subjects: any subject typed `sh:NodeShape` / `sh:PropertyShape`, plus any
        // object of an `sh:property` triple (an inline property shape). Resolve the class /
        // property IRIs (not source tokens), so an alternate prefix or a full IRI cannot bypass.
        let mut construct_subjects: BTreeSet<TermId> = BTreeSet::new();
        if let Some(type_id) = iri_id_static(&ds, rdf::TYPE) {
            for local in ["NodeShape", "PropertyShape"] {
                let Some(cid) = iri_id_static(&ds, &format!("{SHACL_NS}{local}")) else {
                    continue;
                };
                for q in ds.quads_for_pattern(None, Some(type_id), Some(cid), GraphMatch::Any) {
                    construct_subjects.insert(q.s);
                }
            }
        }

        // Parents: an `sh:property` object → its owning node shape, so a `logic:formalizes` on the
        // node shape legalizes the inline property shape (the upward walk).
        let mut parents: BTreeMap<TermId, BTreeSet<TermId>> = BTreeMap::new();
        if let Some(pid) = iri_id_static(&ds, &format!("{SHACL_NS}property")) {
            for q in ds.quads_for_pattern(None, Some(pid), None, GraphMatch::Any) {
                construct_subjects.insert(q.o);
                parents.entry(q.o).or_default().insert(q.s);
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

        for subj in &construct_subjects {
            if formalizes_backed(*subj, &directly_backed, &parents) {
                continue;
            }
            report.error(format!(
                "{rel}: {} is a hand-authored declarative validation shape \
                 (sh:NodeShape/sh:PropertyShape) without a `logic:formalizes` back-reference on it \
                 or its owning shape: validation shapes are authored in the logic: canon and \
                 PROJECTED to generated/shapes/*.ttl (Principle 17), never hand-authored as a \
                 second source of truth (design/LOGIC-VALIDATION.md)",
                node_label(&ds, *subj)
            ));
        }
    }
}

/// ShEx is an emit-only projection of the `logic:` canon (Principle 17): the pipeline PROJECTS
/// `.shex` under `generated/`, and no `.shex` is ever hand-authored. So ANY authored `.shex` file
/// under the source dirs (`slices/`, `shapes/`, `dsl/`) is a forbidden second source of truth.
/// There are ZERO authored `.shex` files today, so this gate is armed-and-empty on the live tree —
/// it enforces the invariant going forward.
fn check_authored_shex_purity(root: &Path, report: &mut RepoStaticReport) {
    let mut shex_files = Vec::new();
    for sub in ["slices", "shapes", "dsl"] {
        let dir = root.join(sub);
        if dir.is_dir() {
            collect_shex_files(&dir, report, &mut shex_files);
        }
    }
    shex_files.sort();
    for path in shex_files {
        let rel = slash_path(path.strip_prefix(root).unwrap_or(&path));
        report.error(format!(
            "{rel}: authored ShEx surface is forbidden — ShEx is an emit-only projection of the \
             logic: canon, PROJECTED to generated/ (Principle 17), never hand-authored \
             (design/LOGIC-VALIDATION.md)"
        ));
    }
}

/// The pinned census (Principle 17) of slices that still ship a hand-authored `shapes.ttl` — the
/// legacy per-slice SHACL surface predating the `logic:`-grounded migration
/// (`docs/MIGRATING-SHAPES-TO-LOGIC.md`). The set is **shrink-only**: as a slice's obligations are
/// grounded in the `logic:` canon and re-projected, its `shapes.ttl` is deleted (mirroring
/// `slices/grounding/math`'s retirement) and this list may be trimmed to match — tidy
/// follow-through, not required for [`check_hand_authored_shapes_ratchet`] to pass. What must
/// NEVER happen is growth: a hand-authored `shapes.ttl` appearing in a slice absent from this list
/// is a second source of validation truth. Subset-or-equal (not strict equality) is the
/// deliberate choice — a retirement PR that deletes a `shapes.ttl` but forgets to trim its entry
/// here must still pass (shrinkage never reds the gate); only an unlisted ADDITION reds.
const PINNED_HAND_AUTHORED_SHAPES_TTL: &[&str] = &[
    "slices/core/ai/shapes.ttl",
    "slices/core/concepts/shapes.ttl",
    "slices/core/diagnostics/shapes.ttl",
    "slices/core/documentation/shapes.ttl",
    "slices/core/epistemics/shapes.ttl",
    "slices/core/gts/shapes.ttl",
    "slices/core/inference/shapes.ttl",
    "slices/core/inhabitation/shapes.ttl",
    "slices/core/kernel/shapes.ttl",
    "slices/core/learning/shapes.ttl",
    "slices/core/notation/shapes.ttl",
    "slices/core/pipeline/shapes.ttl",
    "slices/core/rights/shapes.ttl",
    "slices/core/standpoint/shapes.ttl",
    "slices/core/temporal/shapes.ttl",
    "slices/extensions/agentic/shapes.ttl",
    "slices/extensions/graphrag/shapes.ttl",
    "slices/extensions/model-serving/shapes.ttl",
    "slices/extensions/music/shapes.ttl",
    "slices/grounding/lang/shapes.ttl",
];

/// Enumerate every hand-authored `shapes.ttl` under `slices/<group>/<slice>/shapes.ttl` — the
/// two-directory-level shape [`PINNED_HAND_AUTHORED_SHAPES_TTL`] pins and the migration tooling
/// (`dev_shapes`) scans. Returns forward-slash, repo-relative paths, sorted. Only the direct
/// `<group>/<slice>/shapes.ttl` position is scanned (not an arbitrary-depth walk): that is the
/// only position a slice's own `shapes.ttl` can occupy, so this stays a precise census rather
/// than the broader legacy-shape-block walk `dev_shapes` does for migration bookkeeping.
///
/// `slices/` is a REQUIRED source tree (every real checkout carries it), not one of several
/// optional scan roots — unlike the peer `check_projection_*` gates, which legitimately skip a
/// same-named directory that is merely one of several alternative sources. A missing or
/// unreadable `slices/` (or a group directory that vanishes mid-scan) is therefore a HARD FAIL
/// (`.goals`: "a missing required thing is a HARD FAIL"), not a silent empty census — an empty
/// census is a subset of any pin and would otherwise let `check_hand_authored_shapes_ratchet`
/// pass on a broken repo. Matches the `read_required`/`collect_ttl_files` idiom: push an error
/// onto `report` and stop (or skip just the affected group) rather than fail open.
fn hand_authored_shapes_ttl_census(root: &Path, report: &mut RepoStaticReport) -> Vec<String> {
    let mut found = Vec::new();
    let slices_dir = root.join("slices");
    let groups = match fs::read_dir(&slices_dir) {
        Ok(groups) => groups,
        Err(err) => {
            report.error(format!(
                "{}: cannot read required directory: {err}",
                slices_dir.display()
            ));
            return found;
        }
    };
    let mut group_dirs: Vec<PathBuf> = groups
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && !p.is_symlink())
        .collect();
    group_dirs.sort();
    for group in group_dirs {
        let slices = match fs::read_dir(&group) {
            Ok(slices) => slices,
            Err(err) => {
                report.error(format!("{}: cannot read directory: {err}", group.display()));
                continue;
            }
        };
        let mut slice_dirs: Vec<PathBuf> = slices
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir() && !p.is_symlink())
            .collect();
        slice_dirs.sort();
        for slice in slice_dirs {
            let candidate = slice.join("shapes.ttl");
            if candidate.is_file() && !candidate.is_symlink() {
                found.push(slash_path(
                    candidate.strip_prefix(root).unwrap_or(&candidate),
                ));
            }
        }
    }
    found.sort();
    found
}

/// The shrink-only `shapes.ttl` ratchet (Principle 17): the live census
/// ([`hand_authored_shapes_ttl_census`]) must be a SUBSET-OR-EQUAL of the pinned allowlist
/// ([`PINNED_HAND_AUTHORED_SHAPES_TTL`]). Any live entry absent from the pin is a hand-authored
/// `shapes.ttl` that appeared in a slice never authorized to carry one — a new second source of
/// validation truth — and fails hard, pointing at `docs/MIGRATING-SHAPES-TO-LOGIC.md`.
fn check_hand_authored_shapes_ratchet(root: &Path, report: &mut RepoStaticReport) {
    let pinned: BTreeSet<&str> = PINNED_HAND_AUTHORED_SHAPES_TTL.iter().copied().collect();
    for rel in hand_authored_shapes_ttl_census(root, report) {
        if !pinned.contains(rel.as_str()) {
            report.error(format!(
                "{rel}: a hand-authored shapes.ttl exists in a slice outside the pinned \
                 shrink-only census (PINNED_HAND_AUTHORED_SHAPES_TTL in \
                 crates/validate/src/repo_static.rs) — the set of slices shipping a \
                 hand-authored shapes.ttl only ever SHRINKS as obligations are grounded in the \
                 logic: canon and re-projected (Principle 17); a new hand-authored shapes.ttl is \
                 a forbidden second source of validation truth. Ground its obligations in \
                 logic: and retire it — see docs/MIGRATING-SHAPES-TO-LOGIC.md — rather than \
                 adding it to the pin"
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

/// Recursively collect `.shex` files under `dir` (symlink-safe, mirroring [`collect_ttl_files`]).
fn collect_shex_files(dir: &Path, report: &mut RepoStaticReport, out: &mut Vec<PathBuf>) {
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
        // Recurse into real subdirectories only — a symlinked directory could form a cycle.
        if path.is_dir() && !path.is_symlink() {
            collect_shex_files(&path, report, out);
        } else if !path.is_symlink() && path.extension().is_some_and(|ext| ext == "shex") {
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

/// Return `text` blanked two ways in parallel, both with (a) all comments and (b) every
/// `#[cfg(test)]`-attributed item body replaced by spaces, preserving newlines (so line numbers
/// and column offsets are unchanged): `.0` KEEPS string-literal contents (so
/// `.join("generated"…)` stays visible to the generated/-read-ban scanner) and `.1` also blanks
/// string/char literal contents (CODE ONLY, for the `Result<_, String>` scan — a mention inside
/// a string literal is prose, not a type occurrence). A Rust-aware char scanner — handling
/// line/block comments, string / raw-string / byte-string literals, char literals
/// **distinguished from lifetimes** (`'a`, `'static`), and byte prefixes — builds `.1` (a
/// `skeleton`, strings + comments blanked) so the `#[cfg(test)]` item body can be brace-matched
/// without being fooled by braces inside strings/comments, and reuses the same brace-matched
/// span to blank both variants identically. Works entirely in CHAR indices (never byte
/// offsets), so multi-byte chars (→, ∪, ×) never misalign it.
fn blank_regions(text: &str) -> (String, String) {
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
            skeleton[pos] = blank(src[pos]);
        }
        m = k;
    }
    (out.iter().collect(), skeleton.iter().collect())
}

/// Comments and `#[cfg(test)]` bodies blanked, string/char literal CONTENTS kept — the
/// generated/-read ban's view (it must still see `.join("generated"…)` string literals).
fn blank_comments_and_cfg_test_modules(text: &str) -> String {
    blank_regions(text).0
}

/// Comments, string/char literals, AND `#[cfg(test)]` bodies all blanked — CODE ONLY. Used by
/// the `Result<_, String>` honest-invariant scan: a `Result<_, String>` mention inside a
/// string literal (a diagnostic message, a doc example, this very gate's own error text) is
/// prose, not a type occurrence, and must never be flagged.
fn blank_comments_strings_and_cfg_test_modules(text: &str) -> String {
    blank_regions(text).1
}

// ── honest-invariant #1: no first-party thiserror/anyhow dependency ──────

/// The dependency-table keys checked in every first-party `crates/*/Cargo.toml` (top-level
/// and inside every `[target.'cfg(...)'.dependencies]`-style sub-table): the Phase-6
/// Diag-substrate honest invariant that first-party manifests never declare `thiserror` or
/// `anyhow` — `gmeow_errors::Diag` is the single first-party error type. Transitive
/// occurrences pulled in by vendored third-party crates (`tiktoken-rs`, …) are
/// allowed and out of scope: this gate scans MANIFESTS only, never the resolved dependency
/// tree / `Cargo.lock`.
const DEP_TABLE_KEYS_STATIC: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];
const BANNED_ERROR_CRATES: &[&str] = &["thiserror", "anyhow"];

/// Every dependency table in `manifest` a banned crate could be declared in: the three
/// top-level tables plus the same three tables nested under every `[target.'cfg(...)'.…]`
/// entry (the native-only-dependency idiom this workspace's own crates use).
fn dependency_tables_static(manifest: &toml::Value) -> Vec<&toml::map::Map<String, toml::Value>> {
    let mut tables = Vec::new();
    for key in DEP_TABLE_KEYS_STATIC {
        if let Some(table) = manifest.get(*key).and_then(toml::Value::as_table) {
            tables.push(table);
        }
    }
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for cfg_table in targets.values().filter_map(toml::Value::as_table) {
            for key in DEP_TABLE_KEYS_STATIC {
                if let Some(table) = cfg_table.get(*key).and_then(toml::Value::as_table) {
                    tables.push(table);
                }
            }
        }
    }
    tables
}

/// Honest invariant #1 (Phase-6 Diag-substrate epic): no `crates/*/Cargo.toml` may declare
/// `thiserror` or `anyhow` in `[dependencies]`, `[dev-dependencies]`, or
/// `[build-dependencies]` (including target-cfg-scoped variants). `gmeow_errors::Diag` is
/// the sole first-party error type; a first-party crate reaching for `thiserror`/`anyhow`
/// would be a second, competing error substrate. Parses each manifest with the `toml` crate
/// (already a native-only dependency of this crate) so `thiserror.workspace = true`,
/// `anyhow = "1"`, and `anyhow = { version = "1", features = […] }` are all caught
/// identically — a key lookup, not a string scan.
fn check_no_first_party_error_crate_deps(root: &Path, report: &mut RepoStaticReport) {
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        return;
    }
    let mut crate_dirs: Vec<PathBuf> = match fs::read_dir(&crates_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
        Err(err) => {
            report.error(format!(
                "{}: cannot read directory: {err}",
                crates_dir.display()
            ));
            return;
        }
    };
    crate_dirs.sort();

    for crate_dir in crate_dirs {
        let manifest_path = crate_dir.join("Cargo.toml");
        if !manifest_path.is_file() {
            continue;
        }
        let rel = slash_path(manifest_path.strip_prefix(root).unwrap_or(&manifest_path));
        let text = match fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{rel}: cannot read Cargo.toml: {err}"));
                continue;
            }
        };
        let manifest = match text.parse::<toml::Value>() {
            Ok(manifest) => manifest,
            Err(err) => {
                report.error(format!("{rel}: cannot parse Cargo.toml: {err}"));
                continue;
            }
        };
        let crate_name = manifest
            .get("package")
            .and_then(toml::Value::as_table)
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .unwrap_or("<unnamed>")
            .to_owned();
        for table in dependency_tables_static(&manifest) {
            for banned in BANNED_ERROR_CRATES {
                if table.contains_key(*banned) {
                    report.error(format!(
                        "{rel}: first-party crate {crate_name:?} declares a `{banned}` \
                         dependency — gmeow_errors::Diag is the sole first-party error \
                         substrate (Phase-6 Diag-substrate honest invariant); a transitive \
                         occurrence via a vendored third-party crate is fine, but a first-party \
                         manifest entry is not"
                    ));
                }
            }
        }
    }
}

// ── honest-invariant #2: String is never a Result error type ─────────────

/// True at `chars[i]` iff a `Result` identifier starts there (word-boundary on both sides —
/// so `MyResult<` / `ResultSet<` never match).
fn result_word_at(chars: &[char], i: usize) -> bool {
    const WORD: [char; 6] = ['R', 'e', 's', 'u', 'l', 't'];
    if i + WORD.len() > chars.len() || chars[i..i + WORD.len()] != WORD {
        return false;
    }
    let before_ok = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
    let after = i + WORD.len();
    let after_ok = after >= chars.len() || !(chars[after].is_alphanumeric() || chars[after] == '_');
    before_ok && after_ok
}

/// Parse the top-level (depth-1, outside any nested `<>`/`()`/`[]`) comma-separated generic
/// argument list opening at `chars[open]` (which must be `<`). Returns the trimmed argument
/// strings and the index just past the matching closing `>`, or `None` if the angle brackets
/// never balance (a scan artifact — left unflagged rather than mis-flagged).
fn parse_top_level_generic_args(chars: &[char], open: usize) -> Option<(Vec<String>, usize)> {
    debug_assert_eq!(chars[open], '<');
    let mut depth = 1i32;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut args = Vec::new();
    let mut buf = String::new();
    let mut k = open + 1;
    while k < chars.len() {
        let c = chars[k];
        match c {
            '<' => {
                depth += 1;
                buf.push(c);
            }
            '>' => {
                depth -= 1;
                if depth == 0 {
                    args.push(buf.trim().to_owned());
                    return Some((args, k + 1));
                }
                buf.push(c);
            }
            '(' => {
                paren += 1;
                buf.push(c);
            }
            ')' => {
                paren -= 1;
                buf.push(c);
            }
            '[' => {
                bracket += 1;
                buf.push(c);
            }
            ']' => {
                bracket -= 1;
                buf.push(c);
            }
            ',' if depth == 1 && paren == 0 && bracket == 0 => {
                args.push(buf.trim().to_owned());
                buf.clear();
            }
            ';' if depth == 1 && paren == 0 && bracket == 0 => {
                // `[u8; N]`-style const-generic separators inside a top-level array-length
                // position would already be inside `[…]` (bracket > 0); a bare top-level `;`
                // never occurs in a real `Result<…>` arg list, but bail defensively rather
                // than mis-split.
                buf.push(c);
            }
            _ => buf.push(c),
        }
        k += 1;
    }
    None
}

/// Scan one file's `detect` text (comments, string/char literals, and `#[cfg(test)]` bodies
/// already blanked, exactly char-aligned with `orig_text` per
/// [`blank_comments_strings_and_cfg_test_modules`]) for a `Result<…>` / `…::Result<…>` whose
/// top-level SECOND generic argument is exactly `String`. Ok-position
/// `String` (`Result<String, Diag>`'s first arg, or a single-argument crate `Result<T>` alias)
/// is never flagged — only the top-level error (second) type parameter.
fn scan_result_string_error_type(
    rel: &str,
    orig_text: &str,
    detect: &str,
    report: &mut RepoStaticReport,
) {
    let chars: Vec<char> = detect.chars().collect();
    let orig_lines: Vec<&str> = orig_text.lines().collect();
    let mut i = 0usize;
    while i < chars.len() {
        if result_word_at(&chars, i) {
            let mut j = i + 6;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len()
                && chars[j] == '<'
                && let Some((args, end)) = parse_top_level_generic_args(&chars, j)
            {
                if args.len() >= 2 && args[1] == "String" {
                    let line_no = chars[..i].iter().filter(|&&c| c == '\n').count();
                    let snippet = orig_lines.get(line_no).map(|l| l.trim()).unwrap_or("");
                    report.error(format!(
                        "{rel}:{}: Result<_, String> uses String as the error type — \
                         gmeow_errors::Diag is the sole first-party error type (Phase-6 \
                         Diag-substrate honest invariant); a String in Ok position \
                         (Result<String, …> or a single-argument Result<String> alias) is \
                         fine, only String as the error (second) type parameter is banned: \
                         {snippet}",
                        line_no + 1
                    ));
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
}

/// Honest invariant #2 (Phase-6 Diag-substrate epic): no first-party Rust source may use a
/// two-argument `Result<T, String>` / `std::result::Result<T, String>` where `String` is the
/// error type — in return-type position (`-> Result<_, String>`) or anywhere else the
/// `Result<…>` generic is spelled out (a local `let x: Result<_, String> = …map_err(|_| …)…`
/// binding included, since the scanner matches the syntactic `Result<…>` occurrence, not just
/// function signatures). `gmeow_errors::Diag` (or a structured domain error convertible to it)
/// is the sole first-party error type. Scans every `.rs` file under `crates/` (comments,
/// string/char literals, and `#[cfg(test)]` bodies excluded, reusing
/// [`blank_comments_strings_and_cfg_test_modules`] / [`collect_rust_files`] — the same
/// `#[cfg(test)]`-blanking and file-collection machinery the generated/-read ban uses, but
/// with string/char literal contents ALSO blanked so a `Result<_, String>` mention inside a
/// diagnostic message string is never mistaken for a type occurrence).
fn check_no_string_result_error_type(root: &Path, report: &mut RepoStaticReport) {
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        return;
    }
    let mut files = Vec::new();
    collect_rust_files(&crates_dir, report, &mut files);
    files.sort();
    for path in &files {
        let rel = slash_path(path.strip_prefix(root).unwrap_or(path));
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{rel}: cannot read: {err}"));
                continue;
            }
        };
        let detect = blank_comments_strings_and_cfg_test_modules(&text);
        scan_result_string_error_type(&rel, &text, &detect, report);
    }
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
        // `slices/` is a REQUIRED source tree (hand_authored_shapes_ttl_census hard-fails
        // when it is missing/unreadable) — an empty directory satisfies the requirement
        // without pinning any hand-authored shapes.ttl.
        fs::create_dir_all(root.join("slices")).unwrap();
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
        let report = check_repo_static(live_repo_root());
        assert!(report.ok(), "{:?}", report.errors);
    }

    /// The workspace root: `crates/validate` → `crates` → repo root.
    fn live_repo_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("validate crate should live under crates/")
    }

    #[test]
    fn declarative_gate_flags_the_live_legacy_corpus() {
        // The BLANKET declarative-shape gate is not yet wired into `check_repo_static` (it
        // activates at the terminal migration increment). This test proves its PRODUCTION
        // semantics NOW: run it over the real corpus and confirm it reds on the coexisting legacy
        // shapes that carry no `logic:formalizes` back-reference.
        let mut report = RepoStaticReport::default();
        check_declarative_shape_purity(live_repo_root(), &mut report);
        assert!(
            !report.errors.is_empty(),
            "the blanket declarative-shape gate must red on the live legacy corpus"
        );
        for legacy in [
            "slices/core/inhabitation/shapes.ttl",
            "shapes/gmeow-shapes.ttl",
        ] {
            assert!(
                report.errors.iter().any(|e| e.contains(legacy)),
                "the gate must flag the known-legacy unbacked shapes in {legacy}; got {} errors",
                report.errors.len()
            );
        }
    }

    #[test]
    fn declarative_gate_flags_unbacked_node_shape_and_passes_backed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let module = root.join("slices/x/module.ttl");

        // An unbacked sh:NodeShape → flagged.
        write(
            &module,
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a sh:NodeShape ; sh:targetClass ex:Thing .\n",
        );
        let mut report = RepoStaticReport::default();
        check_declarative_shape_purity(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("module.ttl") && e.contains("declarative validation shape")),
            "an unbacked sh:NodeShape must be flagged; got {:?}",
            report.errors
        );

        // The SAME shape carrying logic:formalizes → legal.
        write(
            &module,
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:S a sh:NodeShape ; logic:formalizes ex:someConstraint ; sh:targetClass ex:Thing .\n",
        );
        let mut backed = RepoStaticReport::default();
        check_declarative_shape_purity(root, &mut backed);
        assert!(
            backed.errors.is_empty(),
            "a logic:formalizes-backed node shape must pass; got {:?}",
            backed.errors
        );
    }

    #[test]
    fn declarative_gate_walks_inline_property_shape_up_to_its_node_shape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let module = root.join("slices/y/module.ttl");

        // An inline sh:property whose owning node shape is UNBACKED → both flagged.
        write(
            &module,
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:property [ sh:path ex:p ; sh:minCount 1 ] .\n",
        );
        let mut report = RepoStaticReport::default();
        check_declarative_shape_purity(root, &mut report);
        assert!(
            report.errors.iter().any(|e| e.contains("module.ttl")),
            "an inline property shape under an unbacked node shape must be flagged; got {:?}",
            report.errors
        );

        // With logic:formalizes on the OWNING node shape → the upward walk legalizes the inline
        // property shape too, so nothing is flagged.
        write(
            &module,
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:S a sh:NodeShape ; logic:formalizes ex:someConstraint ;\n    \
                 sh:property [ sh:path ex:p ; sh:minCount 1 ] .\n",
        );
        let mut backed = RepoStaticReport::default();
        check_declarative_shape_purity(root, &mut backed);
        assert!(
            backed.errors.is_empty(),
            "a backed node shape must legalize its inline property shape (upward walk); got {:?}",
            backed.errors
        );
    }

    #[test]
    fn declarative_gate_catches_alternate_prefix_and_full_iri_bypass() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // Alternate prefix bound to the SHACL namespace — a substring scan for "sh:NodeShape"
        // misses it.
        write(
            &root.join("slices/altprefix/module.ttl"),
            "@prefix af: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a af:NodeShape ; af:targetClass ex:Thing .\n",
        );
        // Full-IRI form — no SHACL prefix at all.
        write(
            &root.join("slices/fulliri/module.ttl"),
            "@prefix ex: <https://example.org/> .\n\
             ex:T a <http://www.w3.org/ns/shacl#PropertyShape> ;\n    \
                 <http://www.w3.org/ns/shacl#path> ex:p .\n",
        );
        let mut report = RepoStaticReport::default();
        check_declarative_shape_purity(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("altprefix/module.ttl")),
            "an alternate-prefix declarative shape must be flagged; got {:?}",
            report.errors
        );
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("fulliri/module.ttl")),
            "a full-IRI declarative shape must be flagged; got {:?}",
            report.errors
        );
    }

    #[test]
    fn authored_shex_gate_flags_a_shex_file_and_passes_when_absent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // No .shex present → passes.
        write(
            &root.join("shapes/gmeow-shapes.ttl"),
            "@prefix ex: <https://example.org/> .\nex:a ex:b ex:c .\n",
        );
        let mut clean = RepoStaticReport::default();
        check_authored_shex_purity(root, &mut clean);
        assert!(
            clean.errors.is_empty(),
            "no authored .shex → gate passes; got {:?}",
            clean.errors
        );

        // A hand-authored .shex under shapes/ → flagged.
        write(&root.join("shapes/gmeow-shapes.shex"), "<S> { ex:p . }\n");
        let mut dirty = RepoStaticReport::default();
        check_authored_shex_purity(root, &mut dirty);
        assert!(
            dirty
                .errors
                .iter()
                .any(|e| e.contains("gmeow-shapes.shex") && e.contains("emit-only projection")),
            "an authored .shex surface must be flagged; got {:?}",
            dirty.errors
        );
    }

    // ── shrink-only shapes.ttl ratchet ───────────────────────────────────

    #[test]
    fn shapes_ratchet_hard_fails_when_slices_dir_is_missing() {
        // `slices/` is a REQUIRED source tree. The fail-open bug this guards against: an
        // absent/unreadable `slices/` used to make `hand_authored_shapes_ttl_census` return an
        // empty `Vec` silently, and an empty census is trivially a subset of
        // PINNED_HAND_AUTHORED_SHAPES_TTL, so the ratchet PASSED on a broken repo (.goals: "a
        // missing required thing is a HARD FAIL", never a silent pass).
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        // Deliberately no `slices/` dir at all.

        let mut report = RepoStaticReport::default();
        check_hand_authored_shapes_ratchet(root, &mut report);
        assert!(
            !report.ok(),
            "a missing required slices/ dir must hard-fail the gate, not pass silently"
        );
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("slices") && e.contains("cannot read required directory")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn shapes_ratchet_passes_when_census_is_empty_or_pinned() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // An empty (but present) slices/ dir → empty census, passes.
        fs::create_dir_all(root.join("slices")).unwrap();
        let mut none = RepoStaticReport::default();
        check_hand_authored_shapes_ratchet(root, &mut none);
        assert!(none.ok(), "{:?}", none.errors);

        // A shapes.ttl in a slice that IS on the pin → passes.
        write(&root.join("slices/core/ai/shapes.ttl"), "");
        let mut pinned = RepoStaticReport::default();
        check_hand_authored_shapes_ratchet(root, &mut pinned);
        assert!(pinned.ok(), "{:?}", pinned.errors);
    }

    #[test]
    fn shapes_ratchet_flags_a_shapes_ttl_outside_the_pin() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        // "affect" is not in PINNED_HAND_AUTHORED_SHAPES_TTL.
        write(&root.join("slices/core/affect/shapes.ttl"), "");

        let mut report = RepoStaticReport::default();
        check_hand_authored_shapes_ratchet(root, &mut report);
        assert!(
            report.errors.iter().any(|e| {
                e.contains("slices/core/affect/shapes.ttl")
                    && e.contains("MIGRATING-SHAPES-TO-LOGIC.md")
            }),
            "an unpinned shapes.ttl must be flagged and point at the migration doc; got {:?}",
            report.errors
        );
    }

    #[test]
    fn shapes_ratchet_permits_shrinkage_without_touching_the_pin() {
        // Deleting a pinned slice's shapes.ttl (a retirement, e.g. slices/grounding/math's) must
        // never fail the gate even though the pin itself was not trimmed — subset-or-equal, not
        // strict equality.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("slices/grounding")).unwrap();
        let mut report = RepoStaticReport::default();
        check_hand_authored_shapes_ratchet(root, &mut report);
        assert!(report.ok(), "{:?}", report.errors);
    }

    #[test]
    fn live_hand_authored_shapes_ttl_census_is_subset_or_equal_of_the_pin() {
        // Direct exercise of the invariant described on PINNED_HAND_AUTHORED_SHAPES_TTL: the live
        // repo's census may shrink relative to the pin (retirements land without a pin edit) but
        // must never grow beyond it.
        let pinned: BTreeSet<&str> = PINNED_HAND_AUTHORED_SHAPES_TTL.iter().copied().collect();
        let mut report = RepoStaticReport::default();
        let live = hand_authored_shapes_ttl_census(live_repo_root(), &mut report);
        assert!(
            report.ok(),
            "the live repo's slices/ tree must be readable: {:?}",
            report.errors
        );
        for rel in &live {
            assert!(
                pinned.contains(rel.as_str()),
                "{rel}: a hand-authored shapes.ttl exists outside the pinned shrink-only \
                 census — see docs/MIGRATING-SHAPES-TO-LOGIC.md before adding a new one",
            );
        }
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

    // ── honest-invariant #1: no first-party thiserror/anyhow dependency ──

    fn crate_manifest(root: &Path, crate_name: &str, extra: &str) {
        write(
            &root.join(format!("crates/{crate_name}/Cargo.toml")),
            &format!(
                "[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n{extra}\n"
            ),
        );
        write(
            &root.join(format!("crates/{crate_name}/src/lib.rs")),
            "// empty\n",
        );
    }

    #[test]
    fn minimal_repo_with_a_clean_crate_passes_error_crate_dep_check() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write_minimal_repo(root);
        crate_manifest(
            root,
            "gmeow-foo",
            "[dependencies]\nserde = \"1\"\n\n[dev-dependencies]\ntempfile = \"3\"\n",
        );
        let mut report = RepoStaticReport::default();
        check_no_first_party_error_crate_deps(root, &mut report);
        assert!(report.ok(), "{:?}", report.errors);
    }

    #[test]
    fn thiserror_dependency_string_form_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate_manifest(root, "gmeow-foo", "[dependencies]\nthiserror = \"1\"\n");
        let mut report = RepoStaticReport::default();
        check_no_first_party_error_crate_deps(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("gmeow-foo") && e.contains("thiserror")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn anyhow_workspace_dependency_form_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate_manifest(
            root,
            "gmeow-bar",
            "[dev-dependencies]\nanyhow = { workspace = true }\n",
        );
        let mut report = RepoStaticReport::default();
        check_no_first_party_error_crate_deps(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("gmeow-bar") && e.contains("anyhow")),
            "{:?}",
            report.errors
        );
    }

    #[test]
    fn thiserror_in_target_cfg_dependencies_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate_manifest(
            root,
            "gmeow-baz",
            "[target.'cfg(not(target_arch = \"wasm32\"))'.dependencies]\nthiserror = \"1\"\n",
        );
        let mut report = RepoStaticReport::default();
        check_no_first_party_error_crate_deps(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("gmeow-baz") && e.contains("thiserror")),
            "{:?}",
            report.errors
        );
    }

    // ── honest-invariant #2: String is never a Result error type ─────────

    fn crate_src(root: &Path, crate_name: &str, file: &str, body: &str) {
        write(&root.join(format!("crates/{crate_name}/src/{file}")), body);
    }

    fn string_result_errors(root: &Path) -> Vec<String> {
        let mut report = RepoStaticReport::default();
        check_no_string_result_error_type(root, &mut report);
        report.errors
    }

    #[test]
    fn result_unit_string_return_type_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate_src(
            root,
            "gmeow-foo",
            "lib.rs",
            "fn f() -> Result<(), String> {\n    Ok(())\n}\n",
        );
        let errs = string_result_errors(root);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].contains("Result<_, String>"), "{errs:?}");
    }

    #[test]
    fn result_u8_string_return_type_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate_src(
            root,
            "gmeow-foo",
            "lib.rs",
            "fn g() -> Result<u8, String> {\n    Ok(0)\n}\n",
        );
        let errs = string_result_errors(root);
        assert_eq!(errs.len(), 1, "{errs:?}");
    }

    #[test]
    fn std_result_fully_qualified_string_error_fails() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate_src(
            root,
            "gmeow-foo",
            "lib.rs",
            "fn h() -> std::result::Result<u8, String> {\n    Ok(0)\n}\n",
        );
        let errs = string_result_errors(root);
        assert_eq!(errs.len(), 1, "{errs:?}");
    }

    #[test]
    fn ok_position_and_single_arg_and_nested_string_do_not_false_positive() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate_src(
            root,
            "gmeow-foo",
            "lib.rs",
            "use std::collections::BTreeMap;\n\
             fn a() -> Result<String> { Ok(String::new()) }\n\
             fn b() -> Result<BTreeMap<String, String>, Diag> { Ok(BTreeMap::new()) }\n\
             fn c() -> io::Result<String> { Ok(String::new()) }\n\
             fn d() -> Result<T, MyErr<String>> { unimplemented!() }\n",
        );
        let errs = string_result_errors(root);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn string_result_in_comment_and_cfg_test_module_is_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        crate_src(
            root,
            "gmeow-foo",
            "lib.rs",
            "// fn old() -> Result<(), String> { unimplemented!() }\n\
             fn real() -> Result<(), Diag> { Ok(()) }\n\
             #[cfg(test)]\nmod tests {\n    fn t() -> Result<(), String> { Ok(()) }\n}\n",
        );
        let errs = string_result_errors(root);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn parse_top_level_generic_args_splits_only_top_level_commas() {
        let text: Vec<char> = "<BTreeMap<String, String>, Diag>".chars().collect();
        let (args, end) = parse_top_level_generic_args(&text, 0).expect("balanced");
        assert_eq!(args, vec!["BTreeMap<String, String>", "Diag"]);
        assert_eq!(end, text.len());
    }
}
