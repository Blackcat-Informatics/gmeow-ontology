// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
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

const EMPTY_DEFICIENCY_LEDGER: &str = concat!(
    "# DEFICIENCY EMERGENCY LEDGER\n",
    "\n",
    "ALL ITEMS BELOW THE MARKER ARE 100% UNAUTHORIZED, CRITICALLY UNDONE WORK,\n",
    "AND 100% BUGS. EACH ENTRY MEANS THE ISSUE OR PULL REQUEST THAT PRODUCED IT\n",
    "FAILED.\n",
    "\n",
    "This is the log of last resort for work misrepresented to pass PR gates, work\n",
    "misrepresented by an agent, or a discovery that an agent was fundamentally\n",
    "defective. An entry is literally a cry for help from a failing agent. It is\n",
    "never an accepted risk, authorized descope, backlog, or success with caveats.\n",
    "\n",
    "The normal state is this notice and marker with no entries below it. Any entry\n",
    "blocks completion, PR creation, and merge of the work that produced it until\n",
    "the defect is verified and has a durable, visible remediation owner. Removing\n",
    "the emergency entry does not resolve the bug or make the failed work successful.\n",
    "\n",
    "--- ENTRIES BELOW THIS LINE ---\n",
);
const DEFICIENCY_ENTRY_MARKER: &str = "--- ENTRIES BELOW THIS LINE ---";

// The ELK/HermiT/Docker OWL-reasoner lane AND the in-process `purrdf::entail`
// differential reasoning oracle (both since retired) have been DELETED
// entirely — the native `logic:` reasoner is the single reasoning authority, so
// no Makefile target reaches Docker/Java and none wires a live second reasoner
// on-gate. The invariant is now that the Makefile and required CI are ENTIRELY
// Docker-free AND carry no live differential reasoning oracle (see
// `check_differential_oracle_seal`).
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

static DOCKER_REGEXES: LazyLock<Result<Vec<Regex>, regex::Error>> = LazyLock::new(|| {
    DOCKER_PATTERNS
        .iter()
        .map(|pattern| Regex::new(&format!("(?i){pattern}")))
        .collect()
});

// The single-authority seal: after the live native-vs-`purrdf::entail`
// differential reasoning oracle was retired, the native `logic:` reasoner is the
// SOLE reasoner on-gate. A live external/second reasoner wired as an on-gate
// subsumption/entailment oracle — the shape the deleted `reason-crosscheck` lane
// had — must never silently regrow. This regex matches a Makefile TARGET NAME of
// that shape (`*-crosscheck`); it deliberately scans target names only, so the
// RETAINED committed engine-independent goldens — the offline `dl_oracle_gold`
// frozen corpus and the native gap-zero `dl-el-crosscheck-report.ttl` ledger,
// which appear as recipe artifact PATHS or `conformance` tests, never as gate
// targets — stay green.
static DIFFERENTIAL_ORACLE_TARGET: LazyLock<Result<Regex, regex::Error>> =
    LazyLock::new(|| Regex::new(r"(?i)cross-?check"));

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
    // Synthetic unit-test repositories intentionally omit repository-wide policy files. A real
    // checkout (including a linked worktree, whose `.git` is a file) must carry the canonical
    // notice-only emergency ledger; a fixture that explicitly creates the ledger is checked too.
    if root.join(".git").exists() || root.join(".deficiencies").exists() {
        check_deficiency_emergency_ledger(root, &mut report);
    }
    check_lane_purity(root, &mut report);
    check_projection_compute_purity(root, &mut report);
    check_projection_shape_purity(root, &mut report);
    // The BLANKET declarative-shape peer: the hand-authored shape corpus has been retired, so this
    // is now armed. It reds on any authored `sh:NodeShape`/`sh:PropertyShape` (in
    // slices/shapes/dsl/governance) lacking a `logic:formalizes` back-reference — the terminal
    // single-source-of-truth invariant (Principle 17).
    check_declarative_shape_purity(root, &mut report);
    check_authored_shex_purity(root, &mut report);
    check_hand_authored_shapes_ratchet(root, &mut report);
    check_gmeow_shapes_drained(root, &mut report);
    check_no_generated_read_in_pipeline_stages(root, &mut report);
    check_no_first_party_error_crate_deps(root, &mut report);
    check_no_string_result_error_type(root, &mut report);
    check_gts_authorship_seals(root, &mut report);
    check_diag_failure_class_binding(root, &mut report);
    check_no_unmanaged_temp_dir(root, &mut report);
    check_rdf_stack_is_purrdf_only(root, &mut report);
    check_purrdf_and_zstd_pins(root, &mut report);
    check_slice_ttl_trailing_newline(root, &mut report);
    check_wasm_bindgen_pin_parity(root, &mut report);
    check_gts_emit_chokepoint(root, &mut report);
    report
}

/// Fail closed unless `.deficiencies` is exactly the canonical notice and marker.
///
/// The ledger is an emergency signal, not a risk register. Any entry is proof that its
/// originating work failed, so the repository gate must red before another PR can present that
/// state as complete. Exact notice bytes also prevent moving an entry above the marker or quietly
/// weakening the doctrine while leaving the marker-shaped check green.
fn check_deficiency_emergency_ledger(root: &Path, report: &mut RepoStaticReport) {
    let path = root.join(".deficiencies");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            report.error(format!(
                ".deficiencies: the canonical emergency ledger is missing or unreadable: {err}"
            ));
            return;
        }
    };
    if text == EMPTY_DEFICIENCY_LEDGER {
        return;
    }

    let marker_count = text
        .lines()
        .filter(|line| *line == DEFICIENCY_ENTRY_MARKER)
        .count();
    if marker_count != 1 {
        report.error(format!(
            ".deficiencies: expected exactly one canonical entry marker, found {marker_count}"
        ));
        return;
    }

    let (_, after_marker) = text
        .split_once(DEFICIENCY_ENTRY_MARKER)
        .expect("the counted marker is present");
    if after_marker.lines().any(|line| !line.trim().is_empty()) {
        report.error(
            ".deficiencies contains unauthorized critically undone work below its marker; every \
             entry is a bug and blocks completion, PR creation, and merge"
                .to_owned(),
        );
        return;
    }

    report.error(
        ".deficiencies must equal the canonical notice-only emergency ledger byte-for-byte; its \
         doctrine cannot be weakened and entries cannot be moved above the marker"
            .to_owned(),
    );
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
    for sub in ["slices", "shapes", "dsl", "governance"] {
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
    for sub in ["shapes", "slices", "dsl", "governance"] {
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
///
/// One exemption applies: a file's construct subjects are skipped when the file itself is a
/// registered `gmeow:saFailWitness` for its enclosing slice ([`registered_fail_witnesses`]). A
/// `mustNot` structural assertion (e.g. `ex:saNoHandAuthoredShapes`) proves its ban is non-vacuous
/// by committing a fixture that DELIBERATELY hand-authors the banned shape; without the fixture the
/// ban would pass whether or not its pattern is even correct. The exemption is keyed on the
/// REGISTRATION, not a blanket `tests/counter-examples/` directory skip, so an unregistered
/// hand-authored shape parked in the same directory is still caught.
// Wired into `check_repo_static` once the hand-authored shape corpus is retired: reds on any
// authored `sh:NodeShape`/`sh:PropertyShape` (in slices/shapes/dsl/governance) lacking a
// `logic:formalizes` back-reference. Backed residue / boundary-kept shapes carry one and pass.
fn check_declarative_shape_purity(root: &Path, report: &mut RepoStaticReport) {
    let mut ttl_files = Vec::new();
    for sub in ["slices", "shapes", "dsl", "governance"] {
        let dir = root.join(sub);
        if dir.is_dir() {
            collect_ttl_files(&dir, report, &mut ttl_files);
        }
    }
    ttl_files.sort();
    let mut witness_cache: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    for path in ttl_files {
        if let Some(slice_dir) = enclosing_slice_dir(&path) {
            let witnesses = witness_cache
                .entry(slice_dir.clone())
                .or_insert_with(|| registered_fail_witnesses(&slice_dir));
            if witnesses.contains(&path) {
                continue;
            }
        }
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
        // Boolean-combinator members → their owning subject, so a `logic:formalizes` on a node
        // shape legalizes a construct nested inside its logical combinators — the gate's own
        // semantics ("on it or its owning shape"): the shape is the migration-tracking unit and
        // its back-reference covers its whole logical structure, not only direct `sh:property`
        // children. `sh:and`/`sh:or`/`sh:xone` take an RDF list of shapes; `sh:not` a single shape.
        let rdf_first = iri_id_static(&ds, "http://www.w3.org/1999/02/22-rdf-syntax-ns#first");
        let rdf_rest = iri_id_static(&ds, "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest");
        let rdf_nil = iri_id_static(&ds, "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil");
        let list_members = |head: TermId| -> Vec<TermId> {
            let (Some(first), Some(rest)) = (rdf_first, rdf_rest) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            let mut node = Some(head);
            let mut guard = 0usize;
            while let Some(n) = node {
                if Some(n) == rdf_nil || guard > 100_000 {
                    break;
                }
                guard += 1;
                if let Some(q) = ds
                    .quads_for_pattern(Some(n), Some(first), None, GraphMatch::Any)
                    .next()
                {
                    out.push(q.o);
                }
                node = ds
                    .quads_for_pattern(Some(n), Some(rest), None, GraphMatch::Any)
                    .next()
                    .map(|q| q.o);
            }
            out
        };
        for comb in ["and", "or", "xone"] {
            if let Some(cid) = iri_id_static(&ds, &format!("{SHACL_NS}{comb}")) {
                for q in ds.quads_for_pattern(None, Some(cid), None, GraphMatch::Any) {
                    for member in list_members(q.o) {
                        construct_subjects.insert(member);
                        parents.entry(member).or_default().insert(q.s);
                    }
                }
            }
        }
        if let Some(nid) = iri_id_static(&ds, &format!("{SHACL_NS}not")) {
            for q in ds.quads_for_pattern(None, Some(nid), None, GraphMatch::Any) {
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
    for sub in ["slices", "shapes", "dsl", "governance"] {
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
    "slices/core/agentic/shapes.ttl",
    "slices/core/ai/shapes.ttl",
    "slices/core/concepts/shapes.ttl",
    "slices/core/diagnostics/shapes.ttl",
    "slices/core/documentation/shapes.ttl",
    "slices/core/epistemics/shapes.ttl",
    "slices/core/inference/shapes.ttl",
    "slices/core/inhabitation/shapes.ttl",
    "slices/core/kernel/shapes.ttl",
    "slices/core/learning/shapes.ttl",
    "slices/core/notation/shapes.ttl",
    "slices/core/pipeline/shapes.ttl",
    "slices/core/rights/shapes.ttl",
    "slices/core/standpoint/shapes.ttl",
    "slices/core/temporal/shapes.ttl",
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
/// The root `shapes/gmeow-shapes.ttl` is now fully grounded in the `logic:` canon: every
/// validation obligation it once carried lives as a canonical constraint in an owning slice
/// `module.ttl` and is re-projected (Principle 17). This is a shrink-only zero-ratchet: the file
/// must still EXIST (its consumers still enumerate it and the terminal increment retires it), but
/// it must declare ZERO `sh:NodeShape` / `sh:PropertyShape` — a re-introduced shape is a forbidden
/// second source of validation truth. Ground the obligation in `logic:` and re-project instead.
fn check_gmeow_shapes_drained(root: &Path, report: &mut RepoStaticReport) {
    let rel = "shapes/gmeow-shapes.ttl";
    let path = root.join(rel);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            report.error(format!(
                "{rel}: the drained root shapes file must still exist (its consumers enumerate it \
                 and the terminal increment retires it) — cannot read it: {e}"
            ));
            return;
        }
    };
    let declared = text
        .lines()
        .filter(|l| {
            let l = l.trim_start();
            !l.starts_with('#') && (l.contains("sh:NodeShape") || l.contains("sh:PropertyShape"))
        })
        .count();
    if declared != 0 {
        report.error(format!(
            "{rel}: {declared} sh:NodeShape/sh:PropertyShape declaration(s) present — this file is \
             fully drained and shrink-only; every validation obligation lives in the logic: canon \
             and is re-projected (Principle 17). A hand-authored shape here is a forbidden second \
             source of truth: ground it in logic: and re-project — see \
             docs/MIGRATING-SHAPES-TO-LOGIC.md — rather than re-adding it"
        ));
    }
}

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

/// Every authored Turtle surface under `slices/` ends with exactly one newline.
///
/// A bulk authoring pass once appended records to 67 slice `module.ttl` files
/// without a terminating newline, and the state survived across several commits
/// because the ONLY thing that noticed was the pre-commit `end-of-file-fixer`
/// hook — which the offending commits bypassed, and which *rewrites* files
/// rather than failing a lane. Every branch that subsequently ran the lint
/// inherited 67 modified files it had not touched.
///
/// The renderer is not the culprit and needs no fix: `turtle_render::render`
/// emits every statement with `writeln!`, so `gmeow-dev normalize` always
/// terminates its output. The gap was purely the absence of an authority.
///
/// So: the pre-commit hook is the FIXER, this gate is the AUTHORITY. The scope
/// here is deliberately a SUPERSET of the hook's selection over `slices/` — it
/// covers every `.ttl` (not merely `module.ttl`), because the same bulk-edit
/// failure mode reaches `shapes.ttl`, `examples/`, `tests/`, and `mappings/`,
/// and a gate narrower than its fixer just relocates the recurrence into the
/// uncovered set. "Exactly one" matches what the fixer normalizes to, so the two
/// cannot disagree about what correct looks like.
fn check_slice_ttl_trailing_newline(root: &Path, report: &mut RepoStaticReport) {
    let slices_dir = root.join("slices");
    if !slices_dir.is_dir() {
        return;
    }
    let mut files: Vec<PathBuf> = Vec::new();
    collect_ttl_files(&slices_dir, report, &mut files);
    files.sort();
    for path in files {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                report.error(format!("{rel}: cannot read: {err}"));
                continue;
            }
        };
        if bytes.is_empty() {
            report.error(format!(
                "{rel}: is empty — an authored Turtle surface must carry content"
            ));
            continue;
        }
        if !bytes.ends_with(b"\n") {
            report.error(format!(
                "{rel}: has no trailing newline — authored Turtle surfaces end with exactly one, \
                 which is what the renderer emits and what the end-of-file hook normalizes to; a \
                 file without one makes every later branch that runs the lint inherit a \
                 modification it did not author"
            ));
            continue;
        }
        if bytes.ends_with(b"\n\n") {
            report.error(format!(
                "{rel}: ends with a blank line — authored Turtle surfaces end with EXACTLY one \
                 trailing newline, so the gate and the end-of-file hook agree on what correct is"
            ));
        }
    }
}

/// Resolve an IRI to its interned [`TermId`] in `ds`, if present.
fn iri_id_static(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// The predicate a `gmeow:StructuralAssertion` cell uses to name the fixture that DELIBERATELY
/// commits the pattern its `mustNot` polarity bans — the fail-witness that proves the ban is
/// non-vacuous (a ban over a module that, by construction, never contains the banned pattern
/// would otherwise pass whether or not the pattern is even correctly stated). Its object is a
/// literal path relative to the assertion's OWN slice root, e.g.
/// `"tests/counter-examples/hand-authored-shape.ttl"`.
const GMEOW_SA_FAIL_WITNESS_IRI: &str = "https://blackcatinformatics.ca/gmeow/saFailWitness";

/// The enclosing slice directory of `start` — the nearest ancestor carrying a `manifest.ttl` — or
/// `None` when `start` is outside any slice. A `gmeow:saFailWitness` registration is authored
/// relative to its OWN slice, so a registration lookup must first resolve which slice `start`
/// belongs to rather than assume a single repo-wide `tests/` root.
///
/// Shared by [`registered_fail_witnesses`]'s two callers — the `crate-check` projection-purity
/// scan ([`check_declarative_shape_purity`]) and `gmeow-validate`'s `shape_census` test — so the
/// two gates the same failing fixture must satisfy agree on exactly one exemption rule rather
/// than drifting into two independently-maintained ones.
pub fn enclosing_slice_dir(start: &Path) -> Option<PathBuf> {
    let mut cursor = if start.is_dir() {
        Some(start.to_path_buf())
    } else {
        start.parent().map(Path::to_path_buf)
    };
    while let Some(dir) = cursor {
        if dir.join("manifest.ttl").is_file() {
            return Some(dir);
        }
        cursor = dir.parent().map(Path::to_path_buf);
    }
    None
}

/// Every path `slice_dir`'s `tests/*.ttl` cells register as a `gmeow:saFailWitness`, resolved to
/// absolute paths under `slice_dir` so callers can test scanned-file membership directly.
///
/// This is the ONLY legitimate exemption from the shape-purity scans: keyed on the REGISTRATION,
/// never on a blanket `tests/counter-examples/` directory skip. A blanket directory exclusion
/// would make any unregistered hand-authored shape parked in that same directory invisible to the
/// gate; keying on the specific fixture a structural assertion actually cites as its witness
/// closes that hole while still letting the assertion prove its ban is exercised.
pub fn registered_fail_witnesses(slice_dir: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let Ok(entries) = fs::read_dir(slice_dir.join("tests")) else {
        return out;
    };
    let mut cells: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("ttl"))
        .collect();
    cells.sort();
    for cell in cells {
        let Ok(bytes) = fs::read(&cell) else {
            continue;
        };
        let Ok(ds) = purrdf::parse_dataset(&bytes, "text/turtle", None) else {
            continue;
        };
        let Some(p) = iri_id_static(&ds, GMEOW_SA_FAIL_WITNESS_IRI) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(p), None, GraphMatch::Any) {
            if let TermRef::Literal { lexical, .. } = ds.resolve(q.o) {
                out.insert(slice_dir.join(lexical));
            }
        }
    }
    out
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
    // The single-authority seal: no live differential reasoning oracle may regrow
    // as a gate target.
    check_differential_oracle_seal(root, report);
}

/// Anti-regrowth seal for the retired live differential reasoning oracle.
///
/// After the native-vs-`purrdf::entail` `reason-crosscheck` lane was removed, the
/// native `logic:` reasoner is the single reasoning authority. This seal HARD-FAILS
/// if any Makefile target re-introduces a live second-reasoner subsumption/entailment
/// oracle gate (a `*-crosscheck` gate target). It scans target NAMES only, so the
/// retained committed engine-independent goldens — the offline frozen `dl_oracle_gold`
/// corpus (proven under `make conformance`) and the native gap-zero
/// `dl-el-crosscheck-report.ttl` ledger (a recipe artifact PATH) — are ALLOWED and
/// stay green; only a live differential-oracle GATE is forbidden.
fn check_differential_oracle_seal(root: &Path, report: &mut RepoStaticReport) {
    let rel = "Makefile";
    let re = match &*DIFFERENTIAL_ORACLE_TARGET {
        Ok(re) => re,
        Err(e) => {
            report.error(format!(
                "{rel}: failed to compile differential-oracle target regex: {e}"
            ));
            return;
        }
    };
    let Some(text) = read_required(root, rel, report) else {
        return;
    };
    for target in makefile_recipes(&text).keys() {
        if re.is_match(target) {
            report.error(format!(
                "{rel}: target {target:?} re-introduces a live differential reasoning oracle \
                 gate — the native-vs-purrdf reason-crosscheck lane was retired; the native \
                 reasoner is the single authority and only committed engine-independent goldens \
                 are permitted, never a live second reasoner on-gate"
            ));
        }
    }
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

    let docker_regexes = match &*DOCKER_REGEXES {
        Ok(res) => res,
        Err(e) => {
            report.error(format!("{rel}: failed to compile docker lane regex: {e}"));
            return;
        }
    };

    let mut required_jobs = needs.clone();
    required_jobs.push("quality".to_owned());
    for job_name in &required_jobs {
        let Some(job) = yaml_map_get(jobs, job_name) else {
            report.error(format!("{rel}: quality needs missing job {job_name:?}"));
            continue;
        };
        let blob = recursive_yaml_text(job);
        let hits = forbidden_hits(&blob, docker_regexes);
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
            // The retired live native-vs-purrdf differential oracle lane.
            "reason-crosscheck",
            "run_entail_crosscheck",
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
    let docker_regexes = match &*DOCKER_REGEXES {
        Ok(res) => res,
        Err(e) => {
            report.error(format!("{rel}: failed to compile docker lane regex: {e}"));
            return;
        }
    };

    for (target, lines) in &recipes {
        if lane_targets.contains(target.as_str()) {
            continue;
        }
        let hits = forbidden_hits(&lines.join("\n"), docker_regexes);
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

fn forbidden_hits(text: &str, docker_regexes: &[Regex]) -> BTreeSet<String> {
    let mut hits = BTreeSet::new();
    for (pattern, re) in DOCKER_PATTERNS.iter().zip(docker_regexes.iter()) {
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
/// bundle), so update mode reports "unchanged" while strict sync reds
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
/// traceable const-indirected reads; the pipeline's update→strict-check fixed-point test
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

/// Return `text` blanked three ways in parallel, all replacing regions with spaces and
/// preserving newlines (so line numbers and column offsets are unchanged):
///
/// * `.0` — comments and `#[cfg(test)]` bodies blanked, string-literal contents KEPT (so
///   `.join("generated"…)` stays visible to the generated/-read-ban scanner).
/// * `.1` — comments, string/char literals, AND `#[cfg(test)]` bodies all blanked (CODE
///   ONLY, for the `Result<_, String>` scan — a mention inside a string literal is prose,
///   not a type occurrence).
/// * `.2` — comments and string/char literals blanked but `#[cfg(test)]` bodies KEPT (CODE
///   ONLY, tests included, for the unmanaged-temp-dir ban — the leaked scratch paths that
///   filled a developer host's `/tmp` lived almost entirely inside test modules).
///
/// A Rust-aware char scanner — handling
/// line/block comments, string / raw-string / byte-string literals, char literals
/// **distinguished from lifetimes** (`'a`, `'static`), and byte prefixes — builds `.1` (a
/// `skeleton`, strings + comments blanked) so the `#[cfg(test)]` item body can be brace-matched
/// without being fooled by braces inside strings/comments, and reuses the same brace-matched
/// span to blank both variants identically. Works entirely in CHAR indices (never byte
/// offsets), so multi-byte chars (→, ∪, ×) never misalign it.
fn blank_regions(text: &str) -> (String, String, String) {
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

    // Snapshot the code-only skeleton BEFORE test-gated bodies are blanked. The
    // unmanaged-temp-dir ban scans test bodies too — a leaked temp directory in a
    // `#[cfg(test)]` module is exactly the litter that filled the developer host's
    // `/tmp`, so exempting test code would exempt the entire defect.
    let skeleton_with_tests: Vec<char> = skeleton.clone();

    // On the skeleton, blank every TEST-GATED item body. After the attribute, the item's
    // brace-delimited body is the region from the next `{` to its matching `}` (fn / mod /
    // impl); items with no body before a `;` (a `use`/`const`) carry no read.
    //
    // "Test-gated" is any `#[cfg(…)]` whose predicate names the `test` identifier — the bare
    // `#[cfg(test)]` AND the composed forms real modules use, e.g.
    // `#[cfg(all(test, not(target_arch = "wasm32")))]`. Matching only the literal
    // `#[cfg(test)]` would leave a wasm-gated test module looking like production code to
    // every gate built on this view. `not(test)` is excluded (it gates the NON-test build),
    // and a `feature = "test"` cannot match because string contents are already blanked here.
    let open_marker: Vec<char> = "#[cfg(".chars().collect();
    let mut m = 0;
    while m + open_marker.len() <= skeleton.len() {
        if skeleton[m..m + open_marker.len()] != open_marker[..] {
            m += 1;
            continue;
        }
        // Balanced-paren scan of the cfg predicate, then the closing `]`.
        let predicate_start = m + open_marker.len();
        let mut depth = 1i32;
        let mut k = predicate_start;
        while k < skeleton.len() && depth > 0 {
            match skeleton[k] {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            k += 1;
        }
        let mut after = k;
        while after < skeleton.len() && skeleton[after].is_whitespace() {
            after += 1;
        }
        if depth != 0 || after >= skeleton.len() || skeleton[after] != ']' {
            m += open_marker.len();
            continue;
        }
        let predicate: String = skeleton[predicate_start..k - 1].iter().collect();
        if !cfg_predicate_is_test_gated(&predicate) {
            m += open_marker.len();
            continue;
        }
        // Find the item's opening brace, but stop at a `;` (a semicolon-terminated item has no
        // body to blank — e.g. `#[cfg(test)] use super::*;`).
        let mut j = after + 1;
        while j < skeleton.len() && skeleton[j] != '{' && skeleton[j] != ';' {
            j += 1;
        }
        if j >= skeleton.len() || skeleton[j] == ';' {
            m += open_marker.len();
            continue;
        }
        let mut body_depth = 0i32;
        let mut end = j;
        while end < skeleton.len() {
            match skeleton[end] {
                '{' => body_depth += 1,
                '}' => {
                    body_depth -= 1;
                    if body_depth == 0 {
                        end += 1;
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        for pos in j..end.min(out.len()) {
            out[pos] = blank(src[pos]);
            skeleton[pos] = blank(src[pos]);
        }
        m = end;
    }
    (
        out.iter().collect(),
        skeleton.iter().collect(),
        skeleton_with_tests.iter().collect(),
    )
}

/// True when a `#[cfg(…)]` predicate gates its item to a TEST build: the `test` identifier
/// appears as a whole word and is not negated by `not(test)`. Whitespace is stripped first so
/// `not( test )` is recognised identically to `not(test)`.
fn cfg_predicate_is_test_gated(predicate: &str) -> bool {
    let compact: String = predicate.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("not(test)") {
        return false;
    }
    let bytes = compact.as_bytes();
    let mut from = 0;
    while let Some(rel) = compact[from..].find("test") {
        let at = from + rel;
        let end = at + 4;
        let starts = at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        let ends =
            end >= compact.len() || !(bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_');
        if starts && ends {
            return true;
        }
        from = end;
    }
    false
}

/// Comments and TEST-GATED bodies blanked, string/char literal CONTENTS kept — the
/// generated/-read ban's view (it must still see `.join("generated"…)` string literals).
fn blank_comments_and_cfg_test_modules(text: &str) -> String {
    blank_regions(text).0
}

/// Comments and string/char literals blanked, test-gated bodies KEPT — CODE ONLY,
/// tests included. Used by the unmanaged-temp-dir ban: the hand-rolled
/// `env::temp_dir()` scratch paths that filled a developer host's `/tmp` with hundreds
/// of gigabytes lived almost entirely inside `#[cfg(test)]` modules, so a scanner that
/// blanks test bodies would see none of them.
fn blank_comments_and_strings_keeping_tests(text: &str) -> String {
    blank_regions(text).2
}

/// Comments, string/char literals, AND TEST-GATED bodies all blanked — CODE ONLY. Used by
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

/// Manifest-dependency ban list for competing RDF / SHACL / OWL / Turtle / SPARQL stacks. purrdf
/// is gmeow's SOLE RDF-1.2 + SHACL + SPARQL + GTS engine (SUBSUME / EXTEND / ENHANCE): a
/// first-party crate that declares one of these as a Cargo dependency would be pulling in a
/// second, competing, weaker engine alongside purrdf. A transitive occurrence pulled in BY
/// purrdf is fine; a first-party manifest entry is not. (`oxiri`/`oxigraph`-family and
/// `spargebra`/`sparesults` are the crates purrdf's S-series natively replaced.) This list, and
/// the gate below that walks it, govern Cargo.toml dependency declarations ONLY — they read no
/// source code and therefore cannot see, and make no claim to catch, a hand-rolled
/// reimplementation of RDF/Turtle/SHACL parsing written directly in first-party source instead
/// of calling purrdf.
const BANNED_RDF_STACK_CRATES: &[&str] = &[
    "oxrdf",
    "oxttl",
    "oxrdfio",
    "oxrdfxml",
    "oxigraph",
    "oxsdatatypes",
    "oxiri",
    "spargebra",
    "sparesults",
    "sophia",
    "sophia_api",
    "rio_api",
    "rio_turtle",
    "rio_xml",
    "rdftk_core",
    "rdftk_iri",
    "horned-owl",
    "hornedowl",
    "shacl",
    "shacl_ast",
    "shacl_validation",
];

/// Delegation-purity invariant (manifest-only): no `crates/*/Cargo.toml` may declare a competing
/// RDF / SHACL stack as a dependency (see [`BANNED_RDF_STACK_CRATES`]). purrdf is the single
/// RDF-1.2 / SHACL / SPARQL engine gmeow ships, so a first-party crate that imports one of the
/// banned crates would be pulling in a second, rival engine. Uses the same toml key-lookup as
/// the error-crate ban, so `oxrdf.workspace = true`, `oxrdf = "0.2"`, and `oxrdf = { … }` are
/// all caught identically — a dependency-table key lookup, not a source or comment scan.
///
/// Mechanism, precisely: this gate ONLY inspects the `[dependencies]` / `[dev-dependencies]` /
/// `[build-dependencies]` tables of each first-party `Cargo.toml`. It does not read crate
/// source, so it cannot detect — and makes no claim to prevent — a hand-rolled reimplementation
/// of RDF/Turtle/SHACL parse, validate, serialize, or subclass-closure logic written directly in
/// first-party Rust instead of calling purrdf. Catching that class of violation is a code-review
/// responsibility, not this gate's.
fn check_rdf_stack_is_purrdf_only(root: &Path, report: &mut RepoStaticReport) {
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
            for banned in BANNED_RDF_STACK_CRATES {
                if table.contains_key(*banned) {
                    report.error(format!(
                        "{rel}: first-party crate {crate_name:?} declares a `{banned}` \
                         dependency — purrdf is gmeow's sole RDF-1.2 / SHACL / SPARQL engine; \
                         delegate to it instead of importing a competing RDF stack"
                    ));
                }
            }
        }
    }
}

/// The purrdf-source and structured-zstd-floor invariants:
///
/// 1. The standalone `fuzz/` workspace MUST declare the SAME PurRDF source as
///    the root workspace. Both committed lockfiles must select identical PurRDF
///    package versions, sources, and checksums within that requirement.
/// 2. `structured-zstd` MUST resolve to >= 0.0.49 in `Cargo.lock`. Earlier releases' huff0
///    encoder panics compressing the gmeow.gts bundle (the reason a since-removed vendor patch
///    once existed), so a downgrade would reintroduce a hard bundle-write crash.
///
/// Absent inputs (minimal fixtures) are skipped: the invariant binds only where the files
/// exist, which on the live repo is always.
fn check_purrdf_and_zstd_pins(root: &Path, report: &mut RepoStaticReport) {
    let root_manifest = root.join("Cargo.toml");
    let fuzz_manifest = root.join("fuzz").join("Cargo.toml");
    if root_manifest.is_file() && fuzz_manifest.is_file() {
        let root_dep = purrdf_source_key(&root_manifest, true, report);
        let fuzz_dep = purrdf_source_key(&fuzz_manifest, false, report);
        if let (Some(root_key), Some(fuzz_key)) = (root_dep, fuzz_dep)
            && root_key != fuzz_key
        {
            report.error(format!(
                "fuzz/Cargo.toml pins purrdf as [{fuzz_key}] but the root \
                 [workspace.dependencies] pins [{root_key}] — the standalone fuzz workspace \
                 must declare the SAME purrdf source as production; the committed lockfiles \
                 separately enforce the exact selected substrate"
            ));
        }
    }

    let lock_path = root.join("Cargo.lock");

    // Compatibility requirements may be ranges. The two committed lockfiles
    // must select identical PurRDF package versions, sources, and checksums.
    if root_manifest.is_file()
        && let Some(declaration) = purrdf_source_key(&root_manifest, true, report)
        && declaration != "absent"
        && let Err(error) = crate::substrate::verify_fuzz_substrate(root)
    {
        report.error(error.message().to_owned());
    }

    if lock_path.is_file()
        && let Some(version) = locked_package_version(&lock_path, "structured-zstd", report)
    {
        const FLOOR: (u64, u64, u64) = (0, 0, 49);
        match parse_version_triple(&version) {
            Some(triple) if triple >= FLOOR => {}
            Some(_) => report.error(format!(
                "Cargo.lock resolves structured-zstd {version}, below the 0.0.49 floor — \
                 earlier huff0 encoders panic compressing the gmeow.gts bundle; do not downgrade"
            )),
            None => report.error(format!(
                "Cargo.lock structured-zstd version {version:?} is unparsable"
            )),
        }
    }
}

/// The CI-installed `wasm-bindgen-cli` and the binaryen pin CI provisions.
///
/// Split out and called on EVERY path of [`check_wasm_bindgen_pin_parity`], including the
/// ones that could not read the workspace pin. When it was inline, an unreadable or absent
/// root manifest returned early and took this with it — so a genuinely wrong CI version went
/// unreported because a DIFFERENT file was missing. `expected` is `None` exactly when the
/// library pin could not be established; the CI surface is still checked for its own sake.
fn check_wasm_bindgen_ci_pin(root: &Path, expected: Option<&str>, report: &mut RepoStaticReport) {
    let workflow = root.join(".github").join("workflows").join("ci.yml");
    let ci = match fs::read_to_string(&workflow) {
        Ok(ci) => ci,
        Err(err) => {
            report.error(format!(
                "{}: cannot read: {err} — the wasm-bindgen CLI version cannot be checked \
                 against the workspace pin, which is a failure, not agreement",
                workflow.display()
            ));
            return;
        }
    };
    // EVERY occurrence, not the first: a second install step pinned to another version is
    // exactly the drift this exists to catch, and `nth(1)` would never have seen it.
    let installs: Vec<String> = ci
        .split("wasm-bindgen-cli@")
        .skip(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect::<String>()
        })
        .collect();
    if installs.is_empty() {
        report.error(format!(
            "{}: installs no `wasm-bindgen-cli@<version>` — the Node parity lanes cannot \
             generate bindings without it",
            workflow.display()
        ));
    }
    for installed in &installs {
        if expected.is_some() && Some(installed.as_str()) != expected {
            report.error(format!(
                "{} installs wasm-bindgen-cli@{installed} but [workspace.dependencies] pins \
                 wasm-bindgen {} — the CLI refuses any library version it does not match \
                 byte-for-byte, so the wasm build fails naming a version mismatch rather than \
                 this drift",
                workflow.display(),
                expected.unwrap_or("<unreadable>")
            ));
        }
    }

    // The binaryen pin has ONE home (the Makefile's `BINARYEN_VER`), and CI must keep
    // READING it rather than repeating the literal. Those two constants drifted once
    // already: CI provisioned a release predating `--enable-bulk-memory-opt`, and every
    // wasm-opt rule died on an unknown option. Nothing stopped the literal coming back.
    if !ci.contains("print-binaryen-ver") {
        report.error(format!(
            "{}: does not read the binaryen pin from `make print-binaryen-ver` — the Makefile \
             is its single source of truth, and a literal `version_NNN` here is the two-constant \
             drift that broke the wasm lanes",
            workflow.display()
        ));
    }
    // ANY binaryen release literal is the drift, whatever idiom carries it. The first
    // version of this detector matched only the YAML-mapping form (`BINARYEN_VER:`) —
    // which this repository no longer contains, because the fix that introduced the
    // detector also replaced that mapping with a shell read of `print-binaryen-ver`. A
    // detector keyed to a retired idiom matches nothing; the literal itself is the
    // invariant.
    match Regex::new(r"version_[0-9]+") {
        Ok(literal) => {
            if let Some(m) = literal.find(&ci) {
                report.error(format!(
                    "{}: carries the binaryen release literal {:?} — read the pin from \
                     `make print-binaryen-ver` instead; two copies of this version drifted \
                     once and took every wasm parity lane with them",
                    workflow.display(),
                    m.as_str()
                ));
            }
        }
        Err(err) => report.error(format!("binaryen literal pattern failed to compile: {err}")),
    }
}

/// `purrdf::gts_compose::emit_gts` may be called from exactly ONE place: the mandated
/// authorship profile in `crates/gts-profile`.
///
/// That crate exists because the claim "`emit_gmeow_gts` is the only production entry to the
/// GTS writer" was false — three crates called the writer directly, so bundles shipped
/// without the mandated transform chain and compression level. Extracting the profile fixed
/// the instances; nothing stopped the next one. A chokepoint nothing enforces is a
/// convention, and this file already scans for three sibling structural invariants.
///
/// Test code is exempt: a test may legitimately build a deliberately non-conforming bundle
/// to prove the profile's audit rejects it. The scan walks `crates/` — the standalone
/// `fuzz/` workspace sits outside it by design: fuzz targets are harness code that drives
/// the writer deliberately, the same exemption test crates get.
fn check_gts_emit_chokepoint(root: &Path, report: &mut RepoStaticReport) {
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        return;
    }
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rust_files(&crates_dir, report, &mut files);
    for file in files {
        // The profile crate IS the permitted entry.
        if file.starts_with(crates_dir.join("gts-profile")) {
            continue;
        }
        // Integration-test crates and benches are test code: their files carry no
        // `#[cfg(test)]` module for the scrubber to blank, but a test may legitimately
        // build a deliberately non-conforming bundle to prove the audit rejects it.
        if file
            .components()
            .any(|c| matches!(c.as_os_str().to_str(), Some("tests" | "benches")))
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(&file) else {
            report.error(format!("{}: cannot read", file.display()));
            continue;
        };
        let scrubbed = blank_comments_strings_and_cfg_test_modules(&text);
        let lines: Vec<&str> = scrubbed.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            // `#[cfg(test)] use ...::emit_gts;` attaches the attribute to the item, not to a
            // module, so the module scrubber cannot see it.
            let test_gated = idx
                .checked_sub(1)
                .and_then(|prev| lines.get(prev))
                .is_some_and(|prev| prev.trim() == "#[cfg(test)]");
            if test_gated {
                continue;
            }
            if line.contains("gts_compose::emit_gts") || line.contains("emit_gts(") {
                let rel = file.strip_prefix(root).unwrap_or(&file);
                report.error(format!(
                    "{}:{}: calls the GTS writer directly — every production bundle must be \
                     emitted through `gmeow_gts_profile::emit_gmeow_gts`, the single entry \
                     that applies the mandated transform chain and compression level",
                    rel.display(),
                    idx + 1
                ));
            }
        }
    }
}

/// The `wasm-bindgen` library pin, the per-crate declarations, and the CLI CI installs must
/// all name ONE version.
///
/// The CLI that generates the JS bindings refuses to run against a library version it does
/// not match byte-for-byte, and one CLI is on PATH for the whole build. The pin therefore
/// lives in `[workspace.dependencies]` and every `crates/*-wasm` manifest must consume it
/// with `wasm-bindgen.workspace = true`; a crate that redeclares its own version reopens
/// the disagreement the single declaration exists to make unrepresentable.
///
/// The remaining live drift is the CLI: CI installs `wasm-bindgen-cli@<version>` by literal,
/// and a bump to the library pin that forgets that line produces a build failure naming a
/// version mismatch rather than the drift that caused it. Both halves are checked here.
fn check_wasm_bindgen_pin_parity(root: &Path, report: &mut RepoStaticReport) {
    let root_manifest = root.join("Cargo.toml");
    // The root manifest is a REQUIRED repository input: absent or unreadable, the guard
    // could not run, and reporting agreement it never established is the silent
    // degradation this file exists to prevent. The CI half is still checked on every
    // path, so a missing manifest can never swallow a real CLI drift.
    let text = match fs::read_to_string(&root_manifest) {
        Ok(text) => text,
        Err(err) => {
            report.error(format!(
                "{}: cannot read: {err} — the wasm-bindgen pin cannot be checked, which is a \
                 failure, not agreement",
                root_manifest.display()
            ));
            check_wasm_bindgen_ci_pin(root, None, report);
            return;
        }
    };
    let Ok(manifest) = text.parse::<toml::Value>() else {
        report.error(format!("{}: cannot parse", root_manifest.display()));
        check_wasm_bindgen_ci_pin(root, None, report);
        return;
    };
    let declared = manifest
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|ws| ws.get("dependencies"))
        .and_then(toml::Value::as_table)
        .and_then(|deps| deps.get("wasm-bindgen"))
        .and_then(|dep| match dep {
            toml::Value::String(version) => Some(version.clone()),
            toml::Value::Table(table) => table
                .get("version")
                .and_then(toml::Value::as_str)
                .map(str::to_owned),
            _ => None,
        });
    let Some(declared) = declared else {
        report.error(
            "the root [workspace.dependencies] declares no wasm-bindgen — every `crates/*-wasm` \
             crate must consume ONE pin from there, so the wasm-bindgen CLI on PATH can match \
             all of them"
                .to_owned(),
        );
        check_wasm_bindgen_ci_pin(root, None, report);
        return;
    };
    let Some(exact) = declared.strip_prefix('=') else {
        report.error(format!(
            "[workspace.dependencies] pins wasm-bindgen {declared:?}, a range — it must be an \
             EXACT pin (=x.y.z); the CLI refuses any library version it does not match \
             byte-for-byte"
        ));
        check_wasm_bindgen_ci_pin(root, None, report);
        return;
    };

    // No `crates/*-wasm` crate may redeclare the version.
    // An ABSENT crates/ directory means no members to check (minimal fixtures); a
    // directory that exists but cannot be read is a failed check.
    let crates_dir = root.join("crates");
    let entries = if crates_dir.exists() {
        match fs::read_dir(&crates_dir) {
            Ok(entries) => Some(entries),
            Err(err) => {
                report.error(format!(
                    "{}: cannot read: {err} — the `*-wasm` members cannot be checked against \
                     the workspace pin",
                    crates_dir.display()
                ));
                None
            }
        }
    } else {
        None
    };
    if let Some(entries) = entries {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with("-wasm") {
                continue;
            }
            let manifest_path = entry.path().join("Cargo.toml");
            if !manifest_path.is_file() {
                continue;
            }
            let member = match fs::read_to_string(&manifest_path) {
                Ok(member) => member,
                Err(err) => {
                    report.error(format!("{}: cannot read: {err}", manifest_path.display()));
                    continue;
                }
            };
            let parsed = match member.parse::<toml::Value>() {
                Ok(parsed) => parsed,
                Err(err) => {
                    report.error(format!("{}: cannot parse: {err}", manifest_path.display()));
                    continue;
                }
            };
            let dep = parsed
                .get("dependencies")
                .and_then(toml::Value::as_table)
                .and_then(|deps| deps.get("wasm-bindgen"));
            let uses_workspace = matches!(
                dep,
                Some(toml::Value::Table(table))
                    if table.get("workspace").and_then(toml::Value::as_bool) == Some(true)
            );
            match dep {
                None => report.error(format!(
                    "{}: a `*-wasm` crate that declares no wasm-bindgen dependency",
                    manifest_path.display()
                )),
                Some(_) if uses_workspace => {}
                Some(other) => report.error(format!(
                    "{}: redeclares wasm-bindgen as {other} — it must read the single \
                     workspace pin with `wasm-bindgen.workspace = true`, so the version cannot \
                     disagree with its sibling crates or with the CLI CI installs",
                    manifest_path.display()
                )),
            }
        }
    }

    check_wasm_bindgen_ci_pin(root, Some(exact), report);
}

/// A stable, order-independent key for a `purrdf` dependency declaration, so the root and fuzz
/// manifests compare equal iff they name the same source. Returns `None` (with a parse error
/// recorded) only when the manifest itself is unreadable/unparsable; a missing `purrdf` key
/// yields a distinct `absent` key so a drift to "no purrdf" is still caught.
fn purrdf_source_key(
    manifest_path: &Path,
    workspace: bool,
    report: &mut RepoStaticReport,
) -> Option<String> {
    match purrdf_source_key_of(manifest_path, workspace) {
        Ok(key) => Some(key),
        Err(diag) => {
            report.error(diag.message().to_owned());
            None
        }
    }
}

/// The same `purrdf` source key as [`purrdf_source_key`], as a hard failure rather than a
/// recorded finding.
///
/// This diagnostic comparison handles even invalid declarations; exact admission
/// uses [`crate::substrate::resolve_purrdf`]. It uses a real TOML parse, never a line
/// scanner, so a dotted key, a table-form `[workspace.dependencies.purrdf]`, or a commented
/// header cannot silently read as "no pin". The docs-asset substrate gate keys off the LOCK
/// via [`workspace_substrate_key`] rather than any manifest reader, so this stays private:
/// visibility it earns only when a consumer outside this report actually exists.
///
/// # Errors
///
/// Returns a diagnostic when the manifest cannot be read or parsed. A manifest that parses but
/// declares no `purrdf` is NOT an error — it yields the distinct `absent` key, so a drift to
/// "no purrdf at all" still compares unequal instead of vanishing.
fn purrdf_source_key_of(manifest_path: &Path, workspace: bool) -> gmeow_errors::Result<String> {
    let text = fs::read_to_string(manifest_path).map_err(|err| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("{}: cannot read: {err}", manifest_path.display()),
        })
    })?;
    let manifest = text.parse::<toml::Value>().map_err(|err| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("{}: cannot parse: {err}", manifest_path.display()),
        })
    })?;
    let deps = if workspace {
        manifest
            .get("workspace")
            .and_then(toml::Value::as_table)
            .and_then(|ws| ws.get("dependencies"))
    } else {
        manifest.get("dependencies")
    };
    let dep = deps
        .and_then(toml::Value::as_table)
        .and_then(|t| t.get("purrdf"));
    Ok(match dep {
        None => "absent".to_owned(),
        Some(toml::Value::String(version)) => format!("registry;version={version}"),
        Some(toml::Value::Table(table)) => {
            let field = |key: &str| {
                table
                    .get(key)
                    .and_then(toml::Value::as_str)
                    .unwrap_or("")
                    .to_owned()
            };
            format!(
                "git={};tag={};rev={};branch={};version={}",
                field("git"),
                field("tag"),
                field("rev"),
                field("branch"),
                field("version"),
            )
        }
        Some(other) => format!("other={other}"),
    })
}

/// The resolved version of `name` in a parsed `Cargo.lock`, if present.
fn locked_package_version(
    lock_path: &Path,
    name: &str,
    report: &mut RepoStaticReport,
) -> Option<String> {
    match locked_package_version_of(lock_path, name) {
        Ok(version) => Some(version),
        Err(diag) => {
            report.error(diag.message().to_owned());
            None
        }
    }
}

/// The RESOLVED version of `name` in `Cargo.lock`, as a hard failure rather than a recorded
/// finding, for callers outside this report.
///
/// The lock is the only place the version that actually got COMPILED is written down. A
/// manifest requirement is a range: `purrdf = "0.12.0"` admits `0.12.1`, so a key derived from
/// the manifest compares equal across a substrate change the lock can see.
///
/// # Errors
///
/// Returns a diagnostic when the lock cannot be read or parsed, or when it resolves no package
/// named `name` — an absent package is a hard failure here, not a silent skip, because the
/// caller asked for an identity it cannot substitute.
pub fn locked_package_version_of(lock_path: &Path, name: &str) -> gmeow_errors::Result<String> {
    let text = fs::read_to_string(lock_path).map_err(|err| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("{}: cannot read: {err}", lock_path.display()),
        })
    })?;
    let lock = text.parse::<toml::Value>().map_err(|err| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("{}: cannot parse: {err}", lock_path.display()),
        })
    })?;
    lock.get("package")
        .and_then(toml::Value::as_array)
        .and_then(|packages| {
            packages
                .iter()
                .find(|pkg| pkg.get("name").and_then(toml::Value::as_str) == Some(name))
        })
        .and_then(|pkg| pkg.get("version").and_then(toml::Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!(
                    "{}: resolves no package named {name:?}",
                    lock_path.display()
                ),
            })
        })
}

/// The identity of the substrate a wasm engine built from this workspace statically links.
///
/// Both components are read from `Cargo.lock`, never from a manifest requirement: the lock
/// records what was RESOLVED and therefore what was compiled into the `.wasm`, while a
/// manifest requirement is a range that compares equal across the very bump this key exists
/// to catch.
///
/// - `purrdf` is the RDF/SHACL/SPARQL core the browser and native engines must share.
/// - `wasm-bindgen` fixes the JS glue ABI; a glue/library skew produces bindings that load
///   but misbehave, which the digest manifest cannot see either.
///
/// # Errors
///
/// Returns a diagnostic when the lock cannot be read or parsed, or resolves neither package.
pub fn workspace_substrate_key(root: &Path) -> gmeow_errors::Result<String> {
    let lock_path = root.join("Cargo.lock");
    let resolution = crate::substrate::resolve_purrdf(&root.join("Cargo.toml"), &lock_path)?;
    let purrdf = resolution.version;
    let bindgen = locked_package_version_of(&lock_path, "wasm-bindgen")?;
    let binaryen = declared_binaryen_version(root)?;
    Ok(format!(
        "purrdf {purrdf}; wasm-bindgen {bindgen}; binaryen {binaryen}"
    ))
}

/// The binaryen release the Makefile pins, as `BINARYEN_VER`.
///
/// Read from the Makefile because that is where the pin lives — CI provisions exactly this
/// via `make print-binaryen-ver`, and every `wasm-opt` rule refuses a binary reporting a
/// different version.
///
/// # Errors
///
/// Returns a diagnostic when the Makefile cannot be read or declares no `BINARYEN_VER`.
fn declared_binaryen_version(root: &Path) -> gmeow_errors::Result<String> {
    let makefile = root.join("Makefile");
    let text = fs::read_to_string(&makefile).map_err(|err| {
        gmeow_errors::Diag::of_kind(crate::error::Io {
            detail: format!("{}: cannot read: {err}", makefile.display()),
        })
    })?;
    text.lines()
        .find_map(|line| {
            let rest = line.strip_prefix("BINARYEN_VER")?;
            let rest = rest.trim_start().strip_prefix(":=")?;
            Some(rest.trim().to_owned())
        })
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!(
                    "{}: declares no `BINARYEN_VER := <release>` — the wasm engines' bytes \
                     depend on the binaryen that optimized them, so the substrate they were \
                     built against cannot be stated without it",
                    makefile.display()
                ),
            })
        })
}

/// Parse a `major.minor.patch` version prefix (ignoring any `-pre`/`+build` suffix) into a
/// comparable tuple.
fn parse_version_triple(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

// ── GTS-authorship seals: one door to purrdf's writer surface ────────────────

/// The crate that owns the mandated GTS authorship profile. It is the ONLY
/// production caller of purrdf's GTS-authorship surface; everything else routes
/// through `emit_gmeow_gts` / `view_to_gmeow_gts` / `GmeowGtsWriter`.
const GTS_PROFILE_CRATE_SRC: &str = "crates/gts-profile/src/";

/// A pinned purrdf public entry point that hands the caller a `Writer` or GTS
/// bytes (returned, or authored into a caller-supplied `io::Write` sink). Each
/// one mints a GTS header, so each one decides the transform chain of every frame
/// that follows — which is exactly what the mandated profile fixes.
///
/// **Census method (re-verified against the pinned purrdf source, tag
/// `rust-v0.8.5`, rev `59c31dc`, under `~/.cargo/git/checkouts/`).** Every
/// `Writer::{new, deterministic, with_layout, with_options, appending}`
/// construction site in purrdf's
/// non-test source was enumerated, and each was traced up to the nearest `pub`
/// entry point. Two consequences worth recording, because a guessed list gets
/// them wrong:
///
/// * `files::build_entries_v2_prefix` is PRIVATE (`fn`, not `pub fn`) at this
///   pin, so it is not an entry point; its five public `pack_entries_v2*`
///   wrappers are pinned in its place.
/// * `agent_memory::Memory` exposes no `writer()` accessor at this pin. Its
///   `store` / `revise` / `record_tool_call` methods DO mint a header internally,
///   but they return a `Claim` / `()` and write to a path — they hand the caller
///   neither a `Writer` nor GTS bytes, and the pinned revision exposes no
///   transform hook on them, so they are outside this seal's stated subject. The
///   segments GMEOW itself appends to those files (`build_audit_segment`,
///   `build_nt_segment`) DO go through the profile crate and are audited by
///   `gmeow-pipeline`'s own `validate_mandated_frames` tests.
/// * `Writer::appending` continues an existing segment's `prev` chain instead of
///   minting a fresh header, so an append-only store pays for one header per FILE
///   rather than one per record. It hands the caller a `Writer`, so it decides the
///   transform chain of every frame it goes on to author exactly as the minting
///   constructors do, and it is pinned here for the same reason. Its ONE production
///   caller is the profile crate's `store_writer`, which is also the door that
///   decides between continuing a segment and opening a new one when the store's
///   medium changes.
struct GtsEntryPoint {
    /// The module path tail as written in a qualified call
    /// (`purrdf::gts_compose::emit_gts` → `gts_compose`).
    module: &'static str,
    /// A free function's name, or the type owning `constructors`.
    item: &'static str,
    /// Associated constructor names for a TYPE entry point; empty for a free fn.
    constructors: &'static [&'static str],
}

const PURRDF_GTS_ENTRY_POINTS: &[GtsEntryPoint] = &[
    GtsEntryPoint {
        module: "writer",
        item: "Writer",
        constructors: &[
            "new",
            "deterministic",
            "with_layout",
            "with_options",
            "appending",
        ],
    },
    GtsEntryPoint {
        module: "writer",
        item: "snapshot_from_graph",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "gts_write",
        item: "to_writer",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "gts_write",
        item: "to_gts",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "gts_compose",
        item: "emit_gts",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "compact",
        item: "compact_streamable",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "files",
        item: "pack",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "files",
        item: "pack_to_writer",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "files",
        item: "pack_entries_v2",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "files",
        item: "pack_entries_v2_to_writer",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "files",
        item: "pack_entries_v2_with_blob_bytes",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "files",
        item: "pack_entries_v2_with_blob_ranges",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "files",
        item: "pack_entries_v2_with_blob_paths",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "from_tar",
        item: "from_tar",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "from_tar",
        item: "from_tar_bytes",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "from_tar",
        item: "from_tar_to_writer",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "from_tar",
        item: "from_seekable_tar",
        constructors: &[],
    },
    GtsEntryPoint {
        module: "from_tar",
        item: "from_seekable_tar_to_writer",
        constructors: &[],
    },
];

/// One production call of a pinned purrdf GTS-authorship entry point.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GtsAuthorshipHit {
    /// Repo-relative, slash-separated path of the calling file.
    file: String,
    /// 1-indexed line of the call.
    line: usize,
    /// The pinned entry point, as `module::item` (`gts_compose::emit_gts`).
    entry_point: String,
    /// The exact call token matched (`GtsWriter::new(`), so an alias is visible.
    token: String,
}

/// Every occurrence of `needle` in `haystack` whose preceding character is not an
/// identifier character — i.e. `needle` starts a fresh identifier. `Writer::new(`
/// therefore matches inside `purrdf::gts::writer::Writer::new(` (preceded by `:`)
/// but NOT inside `OkfWriter::new(` (preceded by `f`).
fn identifier_starts(haystack: &str, needle: &str) -> usize {
    let mut count = 0;
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(needle) {
        let at = from + rel;
        let boundary = at == 0 || {
            let prev = bytes[at - 1];
            !(prev.is_ascii_alphanumeric() || prev == b'_')
        };
        if boundary {
            count += 1;
        }
        from = at + needle.len();
    }
    count
}

/// The local names a purrdf `use` statement binds `item` to in `code`.
///
/// Always includes nothing when `item` is never imported: the qualified call form
/// (`module::item(`) is scanned separately, so a file that neither imports nor
/// qualifies cannot be calling the entry point at all. `use … as Alias` is
/// followed, which is how `use purrdf::gts::writer::Writer as GtsWriter;` stays
/// visible to the seal.
fn purrdf_use_bindings(code: &str, item: &str) -> BTreeSet<String> {
    let mut bindings = BTreeSet::new();
    let mut rest = code;
    while let Some(at) = rest.find("use ") {
        let after = &rest[at + 4..];
        let stmt_end = after.find(';').unwrap_or(after.len());
        let stmt = &after[..stmt_end];
        rest = &after[stmt_end..];
        if !stmt.contains("purrdf") {
            continue;
        }
        // Walk each identifier-start occurrence of `item` in the statement and
        // read an `as Alias` rename directly after it.
        let bytes = stmt.as_bytes();
        let mut from = 0;
        while let Some(rel) = stmt[from..].find(item) {
            let at = from + rel;
            let end = at + item.len();
            let starts_ident = at == 0 || {
                let prev = bytes[at - 1];
                !(prev.is_ascii_alphanumeric() || prev == b'_')
            };
            let ends_ident = end >= stmt.len() || {
                let next = bytes[end];
                !(next.is_ascii_alphanumeric() || next == b'_')
            };
            from = end;
            if !(starts_ident && ends_ident) {
                continue;
            }
            let tail = stmt[end..].trim_start();
            if let Some(alias) = tail.strip_prefix("as ") {
                let alias: String = alias
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !alias.is_empty() {
                    bindings.insert(alias);
                    continue;
                }
            }
            bindings.insert(item.to_string());
        }
    }
    bindings
}

/// Census every PRODUCTION call of a pinned purrdf GTS-authorship entry point.
///
/// **Production** is `crates/*/src/**.rs` and nothing else. `crates/*/tests/**`
/// integration tests are NOT production (they carry no `#[cfg(test)]` marker at
/// all, so "the first `#[cfg(test)]` in the file" is not a valid classifier), and
/// inside a scanned file every `#[cfg(test)]`-attributed body is blanked. Comments
/// and string/char literal contents are blanked too, so a commented-out call, a
/// doc mention, or a diagnostic message naming an entry point is never a hit.
fn purrdf_gts_authorship_census(
    root: &Path,
    report: &mut RepoStaticReport,
) -> Vec<GtsAuthorshipHit> {
    let mut hits = Vec::new();
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        return hits;
    }
    let mut crate_dirs: Vec<PathBuf> = match fs::read_dir(&crates_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
        Err(err) => {
            report.error(format!(
                "gts-authorship seal: {}: cannot read directory: {err}",
                crates_dir.display()
            ));
            return hits;
        }
    };
    crate_dirs.sort();

    for crate_dir in crate_dirs {
        let src = crate_dir.join("src");
        if !src.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_rust_files(&src, report, &mut files);
        files.sort();
        for path in &files {
            let rel = slash_path(path.strip_prefix(root).unwrap_or(path));
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(err) => {
                    report.error(format!("gts-authorship seal: {rel}: cannot read: {err}"));
                    continue;
                }
            };
            let code = blank_comments_strings_and_cfg_test_modules(&text);
            for entry in PURRDF_GTS_ENTRY_POINTS {
                let bindings = purrdf_use_bindings(&code, entry.item);
                let mut tokens: BTreeSet<String> = BTreeSet::new();
                if entry.constructors.is_empty() {
                    tokens.insert(format!("{}::{}(", entry.module, entry.item));
                    for alias in &bindings {
                        tokens.insert(format!("{alias}("));
                    }
                } else {
                    for ctor in entry.constructors {
                        tokens.insert(format!("{}::{}::{ctor}(", entry.module, entry.item));
                        for alias in &bindings {
                            tokens.insert(format!("{alias}::{ctor}("));
                        }
                    }
                }
                for (idx, line) in code.lines().enumerate() {
                    for token in &tokens {
                        for _ in 0..identifier_starts(line, token) {
                            hits.push(GtsAuthorshipHit {
                                file: rel.clone(),
                                line: idx + 1,
                                entry_point: format!("{}::{}", entry.module, entry.item),
                                token: token.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

/// **Seal A** — the sole snapshot emitter consumes its builder and returns full evidence.
/// Upstream `emit_gts` is forbidden in production: the owned payload writer is the
/// only snapshot writer, guarded by native completion/poison admission.
fn check_owned_snapshot_publication(
    root: &Path,
    hits: &[GtsAuthorshipHit],
    report: &mut RepoStaticReport,
) {
    let emitters: Vec<&GtsAuthorshipHit> = hits
        .iter()
        .filter(|hit| hit.entry_point == "gts_compose::emit_gts")
        .collect();
    if !emitters.is_empty() {
        report.error(format!("gts-authorship Seal A: raw `purrdf::gts_compose::emit_gts` must have ZERO production callers; found {}: {}", emitters.len(), render_authorship_hits(&emitters)));
    }
    let path = root.join(GTS_PROFILE_CRATE_SRC).join("lib.rs");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            report.error(format!(
                "gts-authorship Seal A: cannot read {}: {e}",
                path.display()
            ));
            return;
        }
    };
    let code = blank_comments_strings_and_cfg_test_modules(&text);
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let Some((signature, body)) = snapshot_function(&compact, "emit_gmeow_gts") else {
        report.error(
            "gts-authorship Seal A: exactly one public consuming snapshot emitter is required"
                .to_string(),
        );
        return;
    };
    let native_admission = match (
        body.find("builder.poison()"),
        body.find("builder.ingest_totals()"),
        body.find("builder.snapshot_payload()"),
    ) {
        (Some(poison), Some(receipt), Some(payload)) => poison < payload && receipt < payload,
        _ => false,
    };
    let one_owned_writer = compact
        .matches("emit_snapshot_payload_with_medium(")
        .count()
        == 2
        && body.matches("emit_snapshot_payload_with_medium(").count() == 1
        && compact.contains("fnemit_snapshot_payload_with_medium(")
        && !compact.contains("pubfnemit_snapshot_payload_with_medium(");
    if !signature.starts_with("pubfnemit_gmeow_gts(builder:SnapshotBuilder,")
        || !signature.contains("Result<GmeowGtsEmission>")
        || !native_admission
        || !one_owned_writer
        || !body.contains("GmeowGtsEmission{bytes,ingestion,")
    {
        report.error("gts-authorship Seal A: snapshot publication must consume its builder, check native poison before payload extraction, retain the full ingestion receipt, and call one private owned payload writer".to_string());
    }
}

/// Locate one function in the existing comment/string/test-blanked source view.
fn snapshot_function<'a>(code: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    let marker = format!("pubfn{name}(");
    let start = code.find(&marker)?;
    if code[start + marker.len()..].contains(&marker) {
        return None;
    }
    let open = start + code[start..].find('{')?;
    let mut depth = 1usize;
    for (offset, byte) in code.as_bytes()[open + 1..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&code[start..open], &code[open + 1..open + 1 + offset]));
                }
            }
            _ => {}
        }
    }
    None
}

/// **Seal B** — ZERO production callers, outside the profile crate, of ANY pinned
/// purrdf entry point that hands back a `Writer` or GTS bytes.
///
/// Seal A alone is not enough: `gts_write::to_gts`, a bare `Writer::new`, and the
/// `files`/`from_tar` packers all mint headers WITHOUT going near `emit_gts`, and
/// each authors payload frames with no transform chain at all. Every one of them
/// is invisible to an `emit_gts`-only seal.
fn check_no_bypassing_gts_authorship(hits: &[GtsAuthorshipHit], report: &mut RepoStaticReport) {
    let bypasses: Vec<&GtsAuthorshipHit> = hits
        .iter()
        .filter(|hit| !hit.file.starts_with(GTS_PROFILE_CRATE_SRC))
        .collect();
    if bypasses.is_empty() {
        return;
    }
    report.error(format!(
        "gts-authorship Seal B: {} production call(s) of a purrdf GTS-authorship entry point \
         outside `{GTS_PROFILE_CRATE_SRC}` — every GMEOW-authored payload frame must carry the \
         one mandated transform, so author through `gmeow_gts_profile` \
         (`emit_gmeow_gts` / `view_to_gmeow_gts` / `GmeowGtsWriter`) instead: {}",
        bypasses.len(),
        render_authorship_hits(&bypasses)
    ));
}

fn render_authorship_hits(hits: &[&GtsAuthorshipHit]) -> String {
    hits.iter()
        .map(|hit| format!("{}:{} `{}`", hit.file, hit.line, hit.token))
        .collect::<Vec<_>>()
        .join(", ")
}

fn check_gts_authorship_seals(root: &Path, report: &mut RepoStaticReport) {
    // The seals bind only where the profile crate exists. A synthetic minimal-repo
    // fixture carries no `crates/` tree at all; the live repo always carries it,
    // and `live_repo_static_passes` runs both seals over it on-gate.
    if !root.join(GTS_PROFILE_CRATE_SRC).is_dir() {
        return;
    }
    let hits = purrdf_gts_authorship_census(root, report);
    check_owned_snapshot_publication(root, &hits, report);
    check_no_bypassing_gts_authorship(&hits, report);
    check_every_gts_producer_declares_a_medium(root, report);
}

// ── Seal C: every production GTS producer declares exactly one medium ────────
//
// Seals A and B prove that every GMEOW-authored frame goes through ONE door. They
// say nothing about which MEDIUM a producer writes through, and the medium check is
// split in three (`gmeow_pipeline::medium::audit::validate_declared_media` dispatches
// on `gmeow:mediumSourceKind`). A three-way split is a TOTAL FUNCTION over producers
// only if every producer is in its domain — otherwise "a producer with no declared
// kind is a hard fail" is a sentence that only ever fires on a fixture, and the split
// is an exemption list with three named exceptions.
//
// So the domain is CENSUSED off the source, not asserted: every production call of a
// `gmeow_gts_profile` door outside the profile crate is a producer, and every such
// producer's file must be claimed by exactly one `gmeow:GtsProducer` individual whose
// declared `gmeow:producerMedium` resolves to a `gmeow:Medium` carrying exactly one
// `gmeow:mediumSourceKind`.

/// The doors of the mandated authorship profile — the GMEOW-side entry points a
/// production producer reaches for. Seal B already proves nothing bypasses them, so
/// this list is the complete set of ways production code can author GTS bytes.
///
/// `GmeowGtsWriter` is a TYPE (its `new` constructor mints a segment); the rest are
/// free functions. `validate_mandated_frames` / `segment_dictionaries` /
/// `store_tail_pins` are deliberately absent: they READ an artifact rather than
/// author one, so a caller of those is not a producer.
const GMEOW_GTS_PRODUCER_DOORS: &[&str] = &[
    "GmeowGtsWriter::new",
    "compact_gmeow_gts",
    "view_to_gmeow_gts",
    "emit_gmeow_gts",
    "open_store_segment",
    "store_writer",
];

/// The shrink-only census of production GTS-producer files that carry NO
/// `gmeow:GtsProducer` declaration.
///
/// EMPTY, and it must stay that way or shrink — it can only shrink from empty by
/// staying empty, which is exactly the point: the split of the medium check into
/// three branches is a total function over producers, so there is no producer the
/// ontology may decline to classify. The constant exists (rather than a bare
/// `is_empty()` assertion) so the failure message can name the idiom, and so the one
/// permissible value is visible where the ratchet is read. There is no ledger-based
/// exception: an entry in `.deficiencies` is unauthorized failed work and cannot create
/// headroom or approve a producer without a declared medium.
const PINNED_GTS_PRODUCERS_WITHOUT_DECLARED_MEDIUM: &[&str] = &[];

/// The lower bound on the live producer census.
///
/// A census that silently returned nothing — an unreadable `crates/` tree, a renamed
/// door, a scanner regression — is a SUBSET of any pin and would let this seal pass
/// on a repo where it proved nothing. The live tree authors GTS bytes from the
/// terminal sink, the release lane, the runtime stores, and the whole-artifact
/// producers, so a healthy census is comfortably above this floor.
const MIN_GTS_PRODUCER_FILES: usize = 6;

/// One production file that authors GTS bytes through a profile door.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GtsProducerFile {
    /// Repo-relative, slash-separated path.
    file: String,
    /// The doors it calls, sorted — reported so a missing declaration names what to
    /// classify rather than merely that something is unclassified.
    doors: BTreeSet<String>,
}

/// Census every production file that calls a [`GMEOW_GTS_PRODUCER_DOORS`] door.
///
/// **Production** is `crates/*/src/**.rs` outside the profile crate itself, with
/// comments, string/char literals and `#[cfg(test)]` bodies blanked — the same
/// classifier [`purrdf_gts_authorship_census`] uses, so the two seals cannot disagree
/// about what "production" means.
fn gts_producer_census(root: &Path, report: &mut RepoStaticReport) -> Vec<GtsProducerFile> {
    let mut out: Vec<GtsProducerFile> = Vec::new();
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        return out;
    }
    let mut crate_dirs: Vec<PathBuf> = match fs::read_dir(&crates_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
        Err(err) => {
            report.error(format!(
                "gts-producer census: {}: cannot read directory: {err}",
                crates_dir.display()
            ));
            return out;
        }
    };
    crate_dirs.sort();

    for crate_dir in crate_dirs {
        let src = crate_dir.join("src");
        if !src.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_rust_files(&src, report, &mut files);
        files.sort();
        for path in &files {
            let rel = slash_path(path.strip_prefix(root).unwrap_or(path));
            if rel.starts_with(GTS_PROFILE_CRATE_SRC) {
                continue;
            }
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(err) => {
                    report.error(format!("gts-producer census: {rel}: cannot read: {err}"));
                    continue;
                }
            };
            let code = blank_comments_strings_and_cfg_test_modules(&text);
            let doors: BTreeSet<String> = GMEOW_GTS_PRODUCER_DOORS
                .iter()
                .filter(|door| identifier_starts(&code, &format!("{door}(")) > 0)
                .map(|door| (*door).to_string())
                .collect();
            if !doors.is_empty() {
                out.push(GtsProducerFile { file: rel, doors });
            }
        }
    }
    out
}

/// One authored `gmeow:GtsProducer` individual, as the seal reads it off the slice.
#[derive(Debug, Clone, Default)]
struct DeclaredGtsProducer {
    /// The declared `gmeow:mediumSourceKind` individuals its media resolve to.
    source_kinds: BTreeSet<String>,
    /// The declared `gmeow:producerMedium` IRIs.
    media: BTreeSet<String>,
}

/// Read every authored `gmeow:GtsProducer` out of the slice trees, keyed by the
/// repo-relative source file it claims through `gmeow:producerCallSite`, together with
/// the `gmeow:mediumSourceKind` its `gmeow:producerMedium` resolves to.
///
/// The source kind is resolved THROUGH the medium rather than redeclared on the
/// producer: a producer that carried its own copy of the resolution rule would be a
/// second source of truth for a fact the medium already states (Principle 4), and the
/// two could then disagree about the same artifact.
fn declared_gts_producers(
    root: &Path,
    report: &mut RepoStaticReport,
) -> BTreeMap<String, DeclaredGtsProducer> {
    const CALL_SITE: &str = "https://blackcatinformatics.ca/gmeow/producerCallSite";
    const PRODUCER_MEDIUM: &str = "https://blackcatinformatics.ca/gmeow/producerMedium";
    const MEDIUM_SOURCE_KIND: &str = "https://blackcatinformatics.ca/gmeow/mediumSourceKind";

    let mut out: BTreeMap<String, DeclaredGtsProducer> = BTreeMap::new();
    let slices_dir = root.join("slices");
    if !slices_dir.is_dir() {
        return out;
    }
    let mut ttl_files = Vec::new();
    collect_ttl_files(&slices_dir, report, &mut ttl_files);
    ttl_files.sort();
    for path in &ttl_files {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        if !text.contains("producerCallSite") {
            continue;
        }
        let rel = slash_path(path.strip_prefix(root).unwrap_or(path));
        let ds = match purrdf::parse_dataset(text.as_bytes(), "text/turtle", None) {
            Ok(ds) => ds,
            Err(err) => {
                report.error(format!("{rel}: does not parse as Turtle: {err}"));
                continue;
            }
        };
        let (Some(call_site), Some(producer_medium), Some(source_kind)) = (
            iri_id_static(&ds, CALL_SITE),
            iri_id_static(&ds, PRODUCER_MEDIUM),
            iri_id_static(&ds, MEDIUM_SOURCE_KIND),
        ) else {
            continue;
        };
        // subject -> the media it declares, and each medium -> its source kinds.
        for quad in ds.quads_for_pattern(None, Some(call_site), None, GraphMatch::Any) {
            let TermRef::Literal { lexical, .. } = ds.resolve(quad.o) else {
                report.error(format!(
                    "{rel}: a gmeow:producerCallSite object is not a literal source path"
                ));
                continue;
            };
            let entry = out.entry(lexical.to_string()).or_default();
            for medium_quad in
                ds.quads_for_pattern(Some(quad.s), Some(producer_medium), None, GraphMatch::Any)
            {
                let TermRef::Iri(medium) = ds.resolve(medium_quad.o) else {
                    continue;
                };
                entry.media.insert(medium.to_string());
                for kind_quad in ds.quads_for_pattern(
                    Some(medium_quad.o),
                    Some(source_kind),
                    None,
                    GraphMatch::Any,
                ) {
                    if let TermRef::Iri(kind) = ds.resolve(kind_quad.o) {
                        entry.source_kinds.insert(kind.to_string());
                    }
                }
            }
        }
    }
    out
}

/// **Seal C** — the medium check's three-way split is TOTAL over production GTS
/// producers.
fn check_every_gts_producer_declares_a_medium(root: &Path, report: &mut RepoStaticReport) {
    // The seal binds where the ontology it reads exists. A synthetic minimal-repo
    // fixture carries a `crates/` tree and no `slices/` one; an absent `slices/` in a
    // REAL repo is not silently tolerated here either — it is already a hard failure
    // in `hand_authored_shapes_ttl_census`, which treats the tree as required.
    if !root.join("slices").is_dir() {
        return;
    }
    let census = gts_producer_census(root, report);
    if census.len() < MIN_GTS_PRODUCER_FILES {
        report.error(format!(
            "gts-authorship Seal C: the production GTS-producer census found {} file(s), below \
             the non-vacuity floor of {MIN_GTS_PRODUCER_FILES} — an empty or truncated census is \
             a SUBSET of any pin, so the seal would pass while proving nothing",
            census.len()
        ));
        return;
    }
    let declared = declared_gts_producers(root, report);
    if declared.is_empty() {
        report.error(
            "gts-authorship Seal C: no gmeow:GtsProducer individual declares a \
             gmeow:producerCallSite — the producer→medium map did not reach the slices, so every \
             producer below would be reported unclassified for one shared reason",
        );
        return;
    }
    let pinned: BTreeSet<&str> = PINNED_GTS_PRODUCERS_WITHOUT_DECLARED_MEDIUM
        .iter()
        .copied()
        .collect();

    for producer in &census {
        let doors: Vec<&str> = producer.doors.iter().map(String::as_str).collect();
        let Some(entry) = declared.get(&producer.file) else {
            if pinned.contains(producer.file.as_str()) {
                continue;
            }
            report.error(format!(
                "gts-authorship Seal C: {} authors GTS bytes ({}) but no gmeow:GtsProducer \
                 declares it through gmeow:producerCallSite, and it is outside the shrink-only \
                 census (PINNED_GTS_PRODUCERS_WITHOUT_DECLARED_MEDIUM in \
                 crates/validate/src/repo_static.rs) — the medium audit dispatches on \
                 gmeow:mediumSourceKind, so an undeclared producer has NO branch and would be \
                 audited by nothing. Mint the gmeow:GtsProducer in slices/core/gts/module.ttl \
                 rather than adding it here",
                producer.file,
                doors.join(", ")
            ));
            continue;
        };
        if entry.media.len() != 1 {
            report.error(format!(
                "gts-authorship Seal C: {} is declared with {} gmeow:producerMedium value(s) \
                 {:?} — a producer writes through exactly one medium, so any other count leaves \
                 its audit branch underivable",
                producer.file,
                entry.media.len(),
                entry.media
            ));
            continue;
        }
        if entry.source_kinds.len() != 1 {
            report.error(format!(
                "gts-authorship Seal C: {} declares medium {:?}, which resolves to {} \
                 gmeow:mediumSourceKind value(s) {:?} — exactly one is required, because the \
                 kind IS the audit branch selector",
                producer.file,
                entry.media,
                entry.source_kinds.len(),
                entry.source_kinds
            ));
        }
    }

    // The reverse direction: a declared call site that names no production producer is
    // a STALE declaration, and a stale declaration is how a real producer's classifying
    // individual survives the file being renamed out from under it.
    let censused: BTreeSet<&str> = census.iter().map(|p| p.file.as_str()).collect();
    for call_site in declared.keys() {
        if !censused.contains(call_site.as_str()) {
            report.error(format!(
                "gts-authorship Seal C: gmeow:producerCallSite {call_site:?} names no production \
                 file that authors GTS bytes — a stale declaration classifies nothing while \
                 making the map look complete"
            ));
        }
    }
}

// ── the diagnostic-kind ↔ ontology failure-class binding ────────────────────
//
// `gmeow_errors::define_diag_kind!` carries an OPTIONAL `failure_class = "<IRI>";`
// clause binding a Rust kind to the `gmeow:enforcesFailureClass` individual it
// produces. Two gates keep that binding honest, and NEITHER is redundant:
//
// * [`check_diag_failure_class_bijection`] proves every declared IRI resolves to a
//   real failure-class individual and that every `gmeow:Medium*` failure class has
//   exactly one Rust producer — the correctness of the links that EXIST;
// * [`check_diag_failure_class_ratchet`] pins the census of kinds carrying NO
//   failure class and lets it only SHRINK — without it the annotation stays
//   permanently optional and the bijection is vacuous for every kind but the
//   annotated few, since a new unannotated kind would simply never be looked at.

/// The `gmeow:` term that links a gate to the failure class it raises.
const ENFORCES_FAILURE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";

/// The IRI stem of the medium axis's failure-class vocabulary. Every failure class
/// under it must have exactly one Rust producer (the six `pipeline.medium.*` kinds).
const MEDIUM_FAILURE_CLASS_STEM: &str = "https://blackcatinformatics.ca/gmeow/Medium";

/// One `define_diag_kind!` invocation, as the static census reads it off the source.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiagKindDecl {
    /// Repo-relative, forward-slash path of the declaring file.
    file: String,
    /// The `code = "…"` literal — the kind's stable registered code.
    code: String,
    /// The `failure_class = "…"` literal, when the invocation declares one.
    failure_class: Option<String>,
}

/// Every `define_diag_kind!` invocation under `crates/*/src/`, in `(file, code)` order.
///
/// Production kinds only: the scan is restricted to `src/` trees (a `tests/` support
/// harness mints throwaway kinds that must not enter the pin) and reads through
/// [`blank_comments_and_cfg_test_modules`], so a doc-comment example and a
/// `#[cfg(test)]`-gated kind are both invisible while the `code` / `failure_class`
/// string literals the census actually needs stay readable.
///
/// The block scan is line-based and deliberately strict: an invocation is opened by a
/// line ending in `define_diag_kind! {` and closed by the first line whose trimmed
/// content is exactly `}`. Every production invocation is a top-level item written in
/// that shape; an invocation the scanner cannot close before the file ends is reported
/// as a HARD FAIL rather than silently dropped, because a silently dropped kind is one
/// the ratchet would stop watching.
fn diag_kind_census(root: &Path, report: &mut RepoStaticReport) -> Vec<DiagKindDecl> {
    let mut found = Vec::new();
    let crates_dir = root.join("crates");
    if !crates_dir.is_dir() {
        return found;
    }
    let mut files = Vec::new();
    collect_rust_files(&crates_dir, report, &mut files);
    files.sort();
    for path in &files {
        let rel = slash_path(path.strip_prefix(root).unwrap_or(path));
        // `crates/<crate>/src/…` only — a `tests/`/`benches/` helper is not a
        // production kind and must not be pinned as one.
        if !rel.contains("/src/") {
            continue;
        }
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{rel}: cannot read: {err}"));
                continue;
            }
        };
        if !text.contains("define_diag_kind!") {
            continue;
        }
        // Two views of the SAME text, blanked in place so their lines stay aligned:
        // `literals` keeps the string contents the census reads, `code_only` blanks
        // them so brace depth is counted over real delimiters and never over a `{}`
        // inside a `message = "…"` format string.
        let literals = blank_comments_and_cfg_test_modules(&text);
        let code_only = blank_comments_strings_and_cfg_test_modules(&text);
        scan_diag_kind_decls(&rel, &literals, &code_only, report, &mut found);
    }
    found.sort_by(|a, b| (&a.file, &a.code).cmp(&(&b.file, &b.code)));
    found
}

/// Pull the `code` / `failure_class` literals out of every `define_diag_kind!` block
/// in one already-blanked source text.
///
/// `literals` and `code_only` are the same text under the two blanking views (see
/// [`diag_kind_census`]); they have identical line structure, so the scan walks them
/// in lockstep and reads delimiters from one and string values from the other. The
/// block is delimited by BRACE DEPTH, not by a bare closing line: an invocation whose
/// struct body spans several lines closes its field list with a line that also trims
/// to `}`, and treating that as the end of the block would silently drop the kind.
fn scan_diag_kind_decls(
    rel: &str,
    literals: &str,
    code_only: &str,
    report: &mut RepoStaticReport,
    out: &mut Vec<DiagKindDecl>,
) {
    struct Open {
        line_no: usize,
        depth: i32,
        code: Option<String>,
        failure_class: Option<String>,
    }
    let brace_delta = |line: &str| -> i32 {
        line.chars().filter(|c| *c == '{').count() as i32
            - line.chars().filter(|c| *c == '}').count() as i32
    };

    let mut open: Option<Open> = None;
    for (index, (literal_line, code_line)) in literals.lines().zip(code_only.lines()).enumerate() {
        let Some(state) = open.as_mut() else {
            if code_line.trim().ends_with("define_diag_kind! {") {
                open = Some(Open {
                    line_no: index + 1,
                    depth: brace_delta(code_line),
                    code: None,
                    failure_class: None,
                });
            }
            continue;
        };

        let trimmed = literal_line.trim();
        if let Some(value) = quoted_clause_value(trimmed, "code") {
            state.code = Some(value);
        } else if let Some(value) = quoted_clause_value(trimmed, "failure_class") {
            state.failure_class = Some(value);
        }

        state.depth += brace_delta(code_line);
        if state.depth > 0 {
            continue;
        }
        let closed = open.take().expect("the block was open on this branch");
        match closed.code {
            Some(code) => out.push(DiagKindDecl {
                file: rel.to_string(),
                code,
                failure_class: closed.failure_class,
            }),
            None => report.error(format!(
                "{rel}:{}: a define_diag_kind! invocation declares no `code = \"…\";` literal — \
                 every diagnostic kind carries a stable registered code",
                closed.line_no
            )),
        }
    }
    if let Some(open) = open {
        report.error(format!(
            "{rel}:{}: a define_diag_kind! invocation is never closed — the diagnostic-kind \
             census cannot read it, and an unreadable kind is one the shrink-only failure-class \
             ratchet stops watching",
            open.line_no
        ));
    }
}

/// The string value of a `<clause> = "<value>";` line, if the line is one.
fn quoted_clause_value(trimmed: &str, clause: &str) -> Option<String> {
    let rest = trimmed.strip_prefix(clause)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Every failure-class IRI the ontology declares, as the object of a
/// `gmeow:enforcesFailureClass` triple in any authored slice Turtle.
///
/// The gate-to-failure link is the ontology's OWN authority for "this is a failure
/// class someone raises", so the census reads exactly that rather than re-deriving a
/// class hierarchy: an `owl:Class` nobody points at through `enforcesFailureClass` is
/// documentation, not a gate (`gmeow:GtsConformanceFailure`'s own `avoidWhen` says so).
fn ontology_failure_classes(root: &Path, report: &mut RepoStaticReport) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    let slices_dir = root.join("slices");
    if !slices_dir.is_dir() {
        return classes;
    }
    let mut ttl_files = Vec::new();
    collect_ttl_files(&slices_dir, report, &mut ttl_files);
    ttl_files.sort();
    for path in &ttl_files {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        // Cheap pre-filter: only a file that mentions the term can bind it.
        if !text.contains("enforcesFailureClass") {
            continue;
        }
        let rel = slash_path(path.strip_prefix(root).unwrap_or(path));
        let ds = match purrdf::parse_dataset(text.as_bytes(), "text/turtle", None) {
            Ok(ds) => ds,
            Err(err) => {
                report.error(format!("{rel}: does not parse as Turtle: {err}"));
                continue;
            }
        };
        let Some(pid) = iri_id_static(&ds, ENFORCES_FAILURE_CLASS) else {
            continue;
        };
        for quad in ds.quads_for_pattern(None, Some(pid), None, GraphMatch::Any) {
            if let TermRef::Iri(iri) = ds.resolve(quad.o) {
                classes.insert(iri.to_string());
            }
        }
    }
    classes
}

/// The Rust-kind ↔ ontology failure-class BIJECTION, in both directions:
///
/// * every `failure_class = "<IRI>"` a Rust kind declares resolves to a real
///   `gmeow:enforcesFailureClass` individual — an IRI typo, or a kind bound to a
///   class the ontology never minted, is a claim about a gate that does not exist;
/// * every `gmeow:Medium*` failure class has EXACTLY ONE Rust producer — a class with
///   none is an unenforced failure (documentation, not a gate), and a class with two
///   makes "which code did this raise" unanswerable.
///
/// The medium axis is the direction that is pinned to exactly-one because it is the
/// axis whose producers this codebase owns end to end. The other direction (a
/// declared IRI must exist) binds EVERY annotated kind, whatever its axis.
fn check_diag_failure_class_bijection(
    decls: &[DiagKindDecl],
    root: &Path,
    report: &mut RepoStaticReport,
) {
    let declared = ontology_failure_classes(root, report);
    if declared.is_empty() {
        // No slices tree (a synthetic minimal-repo fixture): nothing to bind against.
        return;
    }

    let mut producers: BTreeMap<&str, Vec<&DiagKindDecl>> = BTreeMap::new();
    for decl in decls {
        let Some(iri) = decl.failure_class.as_deref() else {
            continue;
        };
        if !declared.contains(iri) {
            report.error(format!(
                "{}: diagnostic kind `{}` declares failure_class <{iri}>, which is not a \
                 gmeow:enforcesFailureClass individual in any slice — a Rust kind may only bind \
                 to a failure class the ontology actually mints and a gate actually raises; \
                 author the individual (and the logic: constraint that enforces it) or drop the \
                 clause",
                decl.file, decl.code
            ));
            continue;
        }
        producers.entry(iri).or_default().push(decl);
    }

    for iri in declared
        .iter()
        .filter(|i| i.starts_with(MEDIUM_FAILURE_CLASS_STEM))
    {
        match producers.get(iri.as_str()).map(Vec::as_slice) {
            None => report.error(format!(
                "<{iri}>: a gmeow:Medium* failure class with NO Rust producer — an unenforced \
                 failure class is documentation, not a gate. Mint the diagnostic kind that \
                 raises it with `failure_class = \"{iri}\";` in its define_diag_kind! block"
            )),
            Some([_]) => {}
            Some(many) => report.error(format!(
                "<{iri}>: {} Rust kinds declare this failure class ({}) — the producer must be \
                 UNIQUE, or 'which code raised this failure' has no answer",
                many.len(),
                many.iter()
                    .map(|d| format!("`{}` in {}", d.code, d.file))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

/// The shrink-only census of diagnostic kinds that carry NO `failure_class` clause.
///
/// Every entry is a kind whose defect the ontology names no typed failure class for.
/// The set is **shrink-only**: as a failure class is minted in a slice and its
/// producer annotated, the kind's code leaves this list (and the bijection gate above
/// starts binding it). What must NEVER happen is GROWTH — a new unannotated kind means
/// a new Rust-side defect with no ontological counterpart, which is exactly the
/// structural disconnect the `failure_class` clause exists to close. Subset-or-equal
/// (not strict equality) is deliberate, mirroring
/// [`PINNED_HAND_AUTHORED_SHAPES_TTL`]: annotating a kind without trimming its entry
/// here must still pass; only an unlisted ADDITION reds.
const PINNED_DIAG_KINDS_WITHOUT_FAILURE_CLASS: &[&str] = &[
    "affect.classify.coincident-prototypes",
    "affect.classify.dimension-mismatch",
    "affect.classify.distance-failed",
    "affect.classify.duplicate-axis",
    "affect.classify.empty-prototype-set",
    "affect.classify.value-out-of-range",
    "affect.classify.vantage-not-pd",
    "affect.classify.zero-norm-cosine",
    "affect.crosscheck.definiteness-absent",
    "affect.crosscheck.definiteness-mismatch",
    "affect.crosscheck.gram-no-entries",
    "affect.distance.metric-basis-mismatch",
    "affect.graph.empty-basis",
    "affect.graph.missing-property",
    "affect.graph.no-observations",
    "affect.graph.unrecognized-handle",
    "cli-core.diagnostics.empty-artifact-selection",
    "cli-core.diagnostics.unknown-artifact-kind",
    "cli-core.diagnostics.unknown-console-mode",
    "conformance.case.anatomy",
    "conformance.cli.args",
    "conformance.compare.json",
    "conformance.compare.rdf",
    "conformance.corpus.invalid",
    "conformance.corpus.license-not-vendorable",
    "conformance.io",
    "conformance.lower.nquads",
    "conformance.manifest.invalid",
    "conformance.manifest.parse",
    "conformance.profile.invalid",
    "conformance.run.failed",
    "conformance.serialize",
    "conformance.szs.status",
    "conformance.szs.unknown-status",
    "conformance.vendor",
    "docs-print.typst-render-failed",
    "docs.describe.gts-read",
    "docs.i18n.catalog-inconsistent",
    "docs.i18n.file-io",
    "docs.i18n.po-parse",
    "docs.i18n.rdf-format",
    "docs.i18n.rdf-parse",
    "docs.i18n.turtle-unescape",
    "docs.i18n.unsupported-source",
    "errors.model.unknown-finding-category",
    "errors.model.unknown-severity-label",
    "gmeow-cli-core.docs-export.io",
    "gmeow-cli.bundle.read-failed",
    "gmeow-cli.describe.ambiguous",
    "gmeow-cli.describe.unresolved",
    "gmeow-cli.explain.unknown-target",
    "gmeow-cli.explain.walk-failed",
    "gmeow-cli.hybrid-query.purremb-selection",
    "gmeow-cli.output.encoding-failed",
    "gmeow-cli.rdf.pipeline-failed",
    "gmeow-cli.source.read-failed",
    "gmeow-dev-cli.bundle.read-failed",
    "gmeow-dev-cli.feedback.bundle-failed",
    "gmeow-dev-cli.gates.vendored-corpus-descriptor-invalid",
    "gmeow-dev-cli.logic.query-failed",
    "gmeow-dev-cli.output.encoding-failed",
    "gmeow-dev-cli.project.target-refresh-failed",
    "gmeow-dev-cli.rdf.pipeline-failed",
    "gmeow-dev-cli.reason.failed",
    "gmeow-dev-cli.shapes.clearance-ungrounded",
    "gmeow-dev-cli.source.read-failed",
    "gmeow-dev-cli.sync.failed",
    "lang-bridge.emit.digest-collision",
    "lang-bridge.gmn1.graph-out-of-domain",
    "lang-bridge.gmn1.malformed-number",
    "lang-bridge.gmn1.non-canonical-order",
    "lang-bridge.gmn1.non-decodable-grammar",
    "lang-bridge.gmn1.non-nfc-literal",
    "lang-bridge.gmn1.uncovered-term",
    "lang-bridge.gmn1.undeclared-dialect-version",
    "lang-bridge.registry.class-not-listed",
    "lang-bridge.registry.missing-targets",
    "logic-compile.cgif",
    "logic-compile.clif",
    "logic-compile.compat",
    "logic-compile.correspondence",
    "logic-compile.edoal",
    "logic-compile.fno",
    "logic-compile.frontend",
    "logic-compile.get-leg",
    "logic-compile.graph",
    "logic-compile.ir",
    "logic-compile.opt-lift",
    "logic-compile.projection",
    "logic-compile.put",
    "logic-compile.relational-core",
    "logic-compile.roundtrip",
    "logic-compile.sparql",
    "logic-compile.sssom",
    "logic-compile.text",
    "logic-compile.validation",
    "logic-compile.xcl",
    "logic.certify",
    "logic.contract-drift",
    "logic.counterfactual",
    "logic.engine",
    "logic.foundation",
    "logic.ir",
    "logic.lower",
    "logic.obligation",
    "logic.oracle",
    "logic.physical",
    "logic.probabilistic",
    "logic.provenance",
    "logic.query",
    "logic.reason",
    "logic.reference",
    "logic.relational-core",
    "logic.result",
    "logic.store",
    "logic.teleology",
    "logic.transaction",
    "logic.transition",
    "logic.verify",
    "math.angle.bad-cosine",
    "math.clifford.blade-out-of-range",
    "math.clifford.grade-out-of-range",
    "math.clifford.invalid-signature",
    "math.decimal.parse",
    "math.dimension.malformed",
    "math.gram.non-square",
    "math.gram.not-positive-definite",
    "math.graph.missing-property",
    "math.graph.no-cells",
    "math.graph.read",
    "math.index.out-of-range",
    "math.rational.domain",
    "math.rational.overflow",
    "math.scale.degenerate",
    "math.space.zero-dimensional",
    "math.sqrt.negative",
    "math.vector.zero",
    "music.format.unsupported",
    "music.fraction.invalid",
    "music.gts.no-musical-entity",
    "music.gts.rdf-pipeline",
    "music.import.unsupported-suffix",
    "music.musicxml.parse",
    "music.musicxml.timeline-overflow",
    "bundle-view.bundle.decode",
    "bundle-view.bundle.json",
    "bundle-view.bundle.parse",
    "bundle-view.bundle.untar",
    "pipeline.cache.decode",
    "pipeline.cache.mismatch",
    "pipeline.contract.attach-decl-mismatch",
    "pipeline.contract.attach-drift",
    "pipeline.contract.capability-mismatch",
    "pipeline.contract.consumes-mismatch",
    "pipeline.contract.dataflow-mismatch",
    "pipeline.contract.expected-output",
    "pipeline.contract.fanout-bijection",
    "pipeline.contract.resource-mismatch",
    "pipeline.dag.invalid",
    "pipeline.dag.unknown-stage-impl",
    "pipeline.declaration.invalid",
    "pipeline.docs-distribution",
    "pipeline.docs-measure",
    "pipeline.eval.schema",
    "pipeline.generator",
    "pipeline.io",
    "mcp.error",
    "mcp.ambiguous-term",
    "pipeline.meta-fold",
    "pipeline.projection",
    "pipeline.put",
    "pipeline.rdf.parse",
    "pipeline.release",
    "pipeline.rule-severity.unknown",
    "pipeline.scoreboard",
    "pipeline.spans.consumed-after-drop",
    "pipeline.stage.failed",
    "pipeline.transcode.codec",
    "pipeline.transcode.non-invertible-source",
    "pipeline.transcode.undecodable-input",
    "pipeline.transcode.unknown-codec",
    "pipeline.transform",
    "pipeline.up-projection",
    "slice-brief.io",
    "slice-brief.partition",
    "slice-quality.gate",
    "slice-quality.io",
    "slice-quality.reason",
    "slice-quality.report",
    "slice-quality.rubric",
    "slicetest.dataset.read",
    "slicetest.exec.aggregate",
    "slicetest.exec.competency",
    "slicetest.exec.conformance",
    "slicetest.exec.example-discovery",
    "slicetest.exec.query-load",
    "slicetest.exec.shape-validation",
    "slicetest.exec.structural",
    "slicetest.sparql.eval",
    "slicetest.sparql.unexpected-form",
    "slicetest.spec.cell",
    "slicetest.spec.load",
    "slicetest.spec.result-shape",
    "slicetest.spec.typed-binding",
    "slicetest.store.logic-reasoning",
    "slicetest.store.merged-graph",
    "slicetest.store.rdfs-closure",
    "validate.argument",
    "validate.catalog",
    "validate.crossref",
    "validate.dataset",
    "validate.engine",
    "validate.format",
    "validate.io",
    "validate.language-tag",
    "validate.mapping",
    "validate.parse",
    "validate.self-description",
    "validate.serialize",
];

/// The shrink-only failure-class ratchet: every kind the live census finds WITHOUT a
/// `failure_class` must already be in [`PINNED_DIAG_KINDS_WITHOUT_FAILURE_CLASS`].
fn check_diag_failure_class_ratchet(decls: &[DiagKindDecl], report: &mut RepoStaticReport) {
    let pinned: BTreeSet<&str> = PINNED_DIAG_KINDS_WITHOUT_FAILURE_CLASS
        .iter()
        .copied()
        .collect();
    for decl in decls.iter().filter(|d| d.failure_class.is_none()) {
        if !pinned.contains(decl.code.as_str()) {
            report.error(format!(
                "{}: diagnostic kind `{}` declares no `failure_class` and is outside the pinned \
                 shrink-only census (PINNED_DIAG_KINDS_WITHOUT_FAILURE_CLASS in \
                 crates/validate/src/repo_static.rs) — the set of kinds with no ontological \
                 failure class only ever SHRINKS. Mint the failure class in the owning slice and \
                 bind the kind to it with `failure_class = \"<IRI>\";` rather than adding it here",
                decl.file, decl.code
            ));
        }
    }
}

fn check_diag_failure_class_binding(root: &Path, report: &mut RepoStaticReport) {
    let decls = diag_kind_census(root, report);
    if decls.is_empty() {
        // No `crates/` tree (a synthetic minimal-repo fixture) — nothing to bind.
        return;
    }
    check_diag_failure_class_bijection(&decls, root, report);
    check_diag_failure_class_ratchet(&decls, report);
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

// ── temp-directory hygiene: every temporary path is RAII-managed ─────────

/// The banned spellings of the system temp-directory accessor, as they appear in CODE
/// (comments and string literals are blanked before the scan, so a mention in prose — such
/// as the doc comment you are reading — never trips the gate). Both the fully-qualified
/// `std::env::temp_dir` and the `use std::env;`-shortened `env::temp_dir` are matched;
/// matching on the `temp_dir` call itself would also catch a bare `use std::env::temp_dir;`
/// import, which is why the import form is listed too.
const TEMP_DIR_ACCESSORS: &[&str] = &["std::env::temp_dir", "env::temp_dir"];

/// The `TempDir`/`NamedTempFile` escape hatches that convert an RAII guard back into a bare
/// path and thereby re-introduce the leak. `into_path` is the historical name and `keep` the
/// current one; both consume the guard and disarm its `Drop`.
const TEMP_GUARD_DISARMERS: &[&str] = &["into_path", "keep"];

/// Temp-directory hygiene invariant: no first-party Rust source may build a scratch path out
/// of the system temp directory by hand, and no source may disarm an RAII temp guard.
///
/// The defect this encodes is concrete and was paid for in disk: the repository used to
/// hand-roll `std::env::temp_dir().join(format!("gmeow-…-{}", std::process::id()))` in 72
/// places across 12 crates. Every one created the directory, wrote into it, and never removed
/// it; keying on the pid guaranteed a *fresh* directory on every invocation, so a full gate
/// run left hundreds of gigabytes of `gmeow-*` directories behind and the host's `/tmp` had to
/// be cleared by hand. A manual `remove_dir_all` at the end of the function is not a fix — it
/// does not run when an assertion fails, an `expect` fires, or the process is killed, which is
/// precisely when a test leaves the most litter.
///
/// The gate therefore bans the *pattern*, not the symptom, and scans `#[cfg(test)]` bodies
/// along with production code (via [`blank_comments_and_strings_keeping_tests`]) because the
/// leaks were overwhelmingly in test modules. `tempfile::TempDir` / `tempfile::NamedTempFile`
/// remove their path on `Drop` — on success, on early return, and while unwinding from a
/// panic — so they are the only sanctioned way to obtain a temporary path.
///
/// A path that must genuinely outlive the process (a durable lock, a cache) does not belong in
/// the system temp directory at all and is spelled as an explicit absolute path elsewhere; the
/// host gate lock at `/var/tmp/gmeow-task/host-runner.lock` is the live example, and it never
/// reaches for `temp_dir()`.
fn check_no_unmanaged_temp_dir(root: &Path, report: &mut RepoStaticReport) {
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
        let detect = blank_comments_and_strings_keeping_tests(&text);
        scan_unmanaged_temp_dir(&rel, &text, &detect, report);
    }
}

/// Scan one file's `detect` text (comments and string/char literals blanked, `#[cfg(test)]`
/// bodies KEPT, exactly char-aligned with `orig_text`) for a hand-rolled system-temp path or a
/// disarmed RAII temp guard.
fn scan_unmanaged_temp_dir(
    rel: &str,
    orig_text: &str,
    detect: &str,
    report: &mut RepoStaticReport,
) {
    let orig_lines: Vec<&str> = orig_text.lines().collect();
    let line_of = |byte_idx: usize| detect[..byte_idx].matches('\n').count();
    let snippet_at = |line_no: usize| {
        orig_lines
            .get(line_no)
            .map(|l| l.trim())
            .unwrap_or("")
            .to_owned()
    };

    for accessor in TEMP_DIR_ACCESSORS {
        for (idx, _) in detect.match_indices(accessor) {
            // `std::env::temp_dir` also contains the substring `env::temp_dir`; count the
            // longest match once by skipping a short match that is part of the long one.
            if *accessor == "env::temp_dir"
                && idx >= 5
                && detect.is_char_boundary(idx - 5)
                && &detect[idx - 5..idx] == "std::"
            {
                continue;
            }
            let line_no = line_of(idx);
            report.error(format!(
                "{rel}:{}: builds a scratch path from the system temp directory by hand — \
                 nothing removes it, so every run leaks a directory (this pattern left \
                 hundreds of GB of gmeow-* litter in the host's temp dir). Use \
                 `tempfile::TempDir` (via `tempfile::tempdir()`) or `tempfile::NamedTempFile`, \
                 which delete on Drop including while unwinding from a panic. Bind the guard \
                 to a NAMED variable (`let _tmp = tempfile::tempdir()?;`) — a bare `let _ =` \
                 drops it immediately and the directory is gone before you use it — and join \
                 any meaningful subdirectory or filename under `_tmp.path()`. A path that must \
                 outlive the process belongs at an explicit durable location, not in the \
                 system temp dir: {}",
                line_no + 1,
                snippet_at(line_no)
            ));
        }
    }

    for disarmer in TEMP_GUARD_DISARMERS {
        let needle = format!(".{disarmer}()");
        for (idx, _) in detect.match_indices(&needle) {
            let line_no = line_of(idx);
            let snippet = snippet_at(line_no);
            report.error(format!(
                "{rel}:{}: `.{disarmer}()` consumes an RAII temp guard and disarms its Drop, \
                 turning a self-cleaning temporary back into a permanent leak — the exact \
                 defect the unmanaged-temp-dir ban exists to prevent. Keep the `TempDir` / \
                 `NamedTempFile` alive in a named binding for as long as the path is needed \
                 instead: {snippet}",
                line_no + 1
            ));
        }
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

#[path = "repo_static.tests.rs"]
#[cfg(test)]
mod tests;
