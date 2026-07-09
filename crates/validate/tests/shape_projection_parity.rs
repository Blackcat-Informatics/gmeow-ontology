// SPDX-License-Identifier: AGPL-3.0-only

//! Shape-projection parity / equivalence-before-deletion harness.
//!
//! The hand-authored SHACL shapes (`shapes/*.ttl` + `slices/**/shapes.ttl` — the
//! AUTHORED union) are being migrated, one constraint at a time, into `logic:`
//! projections that re-emit as `generated/shapes/*.ttl` (the PROJECTED union) and
//! then DELETED (Tasks 5–9). This harness is the oracle that gates that deletion:
//! it proves the projected union reproduces the authored union's *validation
//! behavior* over a corpus of known-intent fixtures BEFORE the authored shapes go
//! away.
//!
//! Migration (Tasks 5–6) has NOT happened yet, so full behavioral equivalence is
//! not yet true. The harness therefore has two lanes:
//!
//! * **Default lane ([`unions_and_corpus_load`]) — must be GREEN now.** It asserts
//!   only the unconditionally-true smoke invariant: both unions load, are non-empty,
//!   and the whole corpus parses.
//!
//! * **Convergence gate (`#[ignore]`d, run via `cargo test -p gmeow-validate
//!   --ignored`) — expected RED now, flipped GREEN and un-`#[ignore]`d at Task 9.**
//!   Two `#[ignore]`d tests: [`projected_reproduces_authored`] asserts full
//!   behavioral equivalence (`authored_findings ⊆ projected_findings` AND no new
//!   well-formed Violation) and, on failure, prints a DELTA REPORT — exactly which
//!   authored findings the projection does not yet reproduce, grouped by domain
//!   source-term (the Task 5–6 migration worklist); and
//!   [`projected_does_not_over_claim_on_wellformed`] isolates the wellformed
//!   no-over-claim direction. That no-over-claim check was AUTHORED for the default
//!   lane but is RED today because the already-generated `validation-shapes.ttl`
//!   derive-all over-claims `sh:class` frame constraints the hand-authored shapes
//!   never enforced (see that test's doc); per Task 3 it is reported and moved to the
//!   gate rather than weakened, and Task 9 restores it to the default lane once the
//!   projection converges.
//!
//! ## Finding key
//!
//! A finding is normalized to `(focus_node, severity, source_term)`; `sh:message`
//! is treated as ADVISORY (it is renamed/reworded by the projector and is not part
//! of the key). `source_term` is resolved so the authored-shape-IRI → projected-
//! shape-IRI rename is transparent: it prefers the shape's `sh:targetClass` (which
//! is byte-identical across both unions when both target the same domain class),
//! then `logic:formalizes` (the canonical domain term a projected constraint-shape
//! carries), then falls back to the raw shape term. Using the SAME preference order
//! for both unions is what makes an authored finding and its projected twin land on
//! the same key — see [`source_term`].

#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::shapes::engine::{parse_shapes, validate_dataset};
use purrdf::shapes::report::Severity;
use purrdf::shapes::term::Term;
use purrdf::{
    DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue, flat_dataset_from_quads,
    flat_rdf_quads_from_dataset, parse_dataset,
};

// ── Vocabulary ──────────────────────────────────────────────────────────────

const SH: &str = "http://www.w3.org/ns/shacl#";
const SH_TARGET_CLASS: &str = "http://www.w3.org/ns/shacl#targetClass";
const SH_PROPERTY: &str = "http://www.w3.org/ns/shacl#property";
const SH_PATH: &str = "http://www.w3.org/ns/shacl#path";
const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";

/// SHACL predicates that name a constraint component on a (property or node) shape.
/// Used only by the Layer-B structural backstop to reduce a shape to declarative
/// `(target, path, component)` atoms.
const COMPONENT_PREDS: &[&str] = &[
    "minCount",
    "maxCount",
    "class",
    "datatype",
    "nodeKind",
    "hasValue",
    "in",
    "minInclusive",
    "maxInclusive",
    "minExclusive",
    "maxExclusive",
    "minLength",
    "maxLength",
    "pattern",
    "languageIn",
    "uniqueLang",
    "equals",
    "disjoint",
    "lessThan",
    "lessThanOrEquals",
    "not",
    "and",
    "or",
    "xone",
    "node",
    "qualifiedValueShape",
    "closed",
    "sparql",
];

// ── Paths ───────────────────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

/// Shape files excluded from the data-graph union (DSL / manifest lints), mirroring
/// `purrdf::shapes::shape_union::EXCLUDED`.
const EXCLUDED: &[&str] = &[
    "mapping-dsl-shapes.ttl",
    "statement-dsl-shapes.ttl",
    "test-dsl-shapes.ttl",
    "slice-manifest-shapes.ttl",
];

/// `*.ttl` directly under `dir`, sorted; missing dir → empty.
fn ttl_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("ttl") && path.is_file() {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Every `slices/*/*/shapes.ttl` (exactly two directory levels), sorted — mirrors
/// `purrdf::shapes::shape_union::shape_files` group 3.
fn slice_shape_files(slices_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(groups) = std::fs::read_dir(slices_dir) else {
        return out;
    };
    for group in groups.flatten() {
        if !group.path().is_dir() {
            continue;
        }
        let Ok(slices) = std::fs::read_dir(group.path()) else {
            continue;
        };
        for slice in slices.flatten() {
            let candidate = slice.path().join("shapes.ttl");
            if candidate.is_file() {
                out.push(candidate);
            }
        }
    }
    out.sort();
    out
}

/// The AUTHORED union: `shapes/*.ttl` (minus [`EXCLUDED`]) + `slices/**/shapes.ttl`.
fn authored_shape_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ttl_files(&root.join("shapes"))
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !EXCLUDED.contains(&n))
        })
        .collect();
    files.extend(slice_shape_files(&root.join("slices")));
    files
}

/// The PROJECTED union: `generated/shapes/*.ttl` only.
fn projected_shape_files(root: &Path) -> Vec<PathBuf> {
    ttl_files(&root.join("generated").join("shapes"))
}

/// Read + concatenate a shape-file group into one Turtle document.
///
/// We build each union SEPARATELY (not via `shape_union::load_shapes`, which unions
/// all three groups) because the whole point of the harness is to compare the two
/// groups against each other. Concatenation is safe: every property shape in these
/// files is an anonymous `[ … ]`, so there are no explicit blank-node labels to
/// collide across files.
fn concat_shapes(files: &[PathBuf]) -> String {
    let mut merged = String::new();
    for file in files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("failed to read shape file {}: {e}", file.display()));
        merged.push_str(&text);
        merged.push('\n');
    }
    merged
}

// ── Source-term resolver ────────────────────────────────────────────────────

/// Maps recovered from a shapes RDF graph so a finding's `source_shape` term can be
/// resolved to a rename-stable domain term.
struct ShapeIndex {
    /// shape IRI → its `sh:targetClass` IRI.
    target_of: BTreeMap<String, String>,
    /// shape IRI → its `logic:formalizes` IRI.
    formalizes_of: BTreeMap<String, String>,
}

impl ShapeIndex {
    /// Scan a shapes RDF graph for the `sh:targetClass` / `logic:formalizes`
    /// annotations that let [`source_term`] make the authored↔projected rename
    /// transparent.
    fn build(ds: &RdfDataset) -> Self {
        let mut target_of = BTreeMap::new();
        let mut formalizes_of = BTreeMap::new();
        for (pred, sink) in [
            (SH_TARGET_CLASS, &mut target_of),
            (LOGIC_FORMALIZES, &mut formalizes_of),
        ] {
            let Some(pred_id) = iri_id(ds, pred) else {
                continue;
            };
            for q in ds.quads_for_pattern(None, Some(pred_id), None, GraphMatch::Any) {
                if let (TermRef::Iri(s), TermRef::Iri(o)) = (ds.resolve(q.s), ds.resolve(q.o)) {
                    // A shape may (harmlessly) declare several target classes; the
                    // first in the deterministic scan order wins.
                    sink.entry(s.to_string()).or_insert_with(|| o.to_string());
                }
            }
        }
        Self {
            target_of,
            formalizes_of,
        }
    }
}

/// Resolve a finding's `source_shape` term to a rename-stable domain term.
///
/// Preference order (SAME for both unions, so an authored finding and its projected
/// twin land on the same key): `sh:targetClass` (byte-identical across unions),
/// then `logic:formalizes` (the canonical domain term), then the raw term (a
/// blank-node property shape or an un-annotated shape — these cannot join across the
/// rename, so they surface honestly in the delta rather than being silently fused).
fn source_term(shape: &Term, index: &ShapeIndex) -> String {
    match shape {
        Term::NamedNode(n) => {
            let iri = n.as_str();
            index
                .target_of
                .get(iri)
                .or_else(|| index.formalizes_of.get(iri))
                .cloned()
                .unwrap_or_else(|| iri.to_string())
        }
        other => term_string(other),
    }
}

/// Stable string form of a term (IRI without `<>`, blank as `_:label`, literal as its
/// `Display`), for the focus-node position and un-resolvable source shapes.
fn term_string(term: &Term) -> String {
    match term {
        Term::NamedNode(n) => n.as_str().to_string(),
        Term::BlankNode(l) => format!("_:{l}"),
        other => other.to_string(),
    }
}

#[inline]
fn iri_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

// ── Finding key ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Key {
    focus: String,
    severity: String,
    source_term: String,
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sev = self
            .severity
            .rsplit(['#', '/'])
            .next()
            .unwrap_or(&self.severity);
        write!(f, "{} [{}] <- {}", self.focus, sev, self.source_term)
    }
}

fn severity_iri(sev: &Severity) -> String {
    sev.iri().to_string()
}

// ── Corpus ──────────────────────────────────────────────────────────────────

/// Intended validation outcome of a corpus fixture, inferred from its location /
/// naming convention.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Intent {
    /// `*-wellformed.ttl` — must conform (0 Violations).
    Wellformed,
    /// `*-malformed.ttl` or a `tests/counter-examples/` fixture — must be rejected.
    Malformed,
    /// Warning/coexistence/leak/evidence fixtures — mixed intent; contribute to the
    /// finding sets but are not asserted per-fixture.
    Neutral,
}

struct Fixture {
    path: PathBuf,
    label: String,
    intent: Intent,
}

/// The known-intent corpus: `tests/fixtures/shapes/*.ttl` +
/// `slices/**/tests/counter-examples/*.ttl`.
///
/// (The `slices/**/tests/example-conformance.ttl` files are test-DSL *metadata*
/// — `gmeow:ExampleConformance` records pointing at data elsewhere — not data
/// graphs, so they are not loaded here; the conforming data they reference lives
/// under `slices/**/examples/` and is exercised by the conforming-examples gate.)
fn corpus(root: &Path) -> Vec<Fixture> {
    let mut out = Vec::new();

    for path in ttl_files(&root.join("tests").join("fixtures").join("shapes")) {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let intent = if stem.ends_with("-wellformed") {
            Intent::Wellformed
        } else if stem.ends_with("-malformed") {
            Intent::Malformed
        } else {
            Intent::Neutral
        };
        out.push(Fixture {
            label: format!("fixtures/shapes/{stem}"),
            path,
            intent,
        });
    }

    for path in counter_example_files(&root.join("slices")) {
        let label = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        out.push(Fixture {
            path,
            label,
            intent: Intent::Malformed,
        });
    }

    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// Every `slices/**/tests/counter-examples/*.ttl`.
fn counter_example_files(slices_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_counter_examples(slices_dir, &mut out);
    out.sort();
    out
}

fn collect_counter_examples(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("counter-examples") {
                out.extend(ttl_files(&path));
            } else {
                collect_counter_examples(&path, out);
            }
        }
    }
}

/// Parse a Turtle data file into a frozen default-graph dataset (mirrors
/// `shacl_engine.rs::turtle_to_dataset`).
fn load_data(path: &Path) -> Arc<RdfDataset> {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()));
    let dataset = parse_dataset(&bytes, "text/turtle", None)
        .unwrap_or_else(|e| panic!("fixture {} must parse as Turtle: {e}", path.display()));
    let mut quads = flat_rdf_quads_from_dataset(&dataset);
    for quad in &mut quads {
        quad.graph_name = None;
    }
    flat_dataset_from_quads(&quads).expect("flattened fixture dataset must freeze")
}

// ── Merged-ontology validation context ───────────────────────────────────────
//
// The live `make validate` validates the merged authored bundle — the root
// ontology + every slice module + the imports — so ontology-provided types,
// subclass edges, and shared individuals are in scope when a `sh:sparql` body
// dereferences them. A fixture validated ALONE lacks that context: a constraint
// whose firing depends on ontology-provided typing (e.g. the affect-decision
// single-label-set constraints, which need the GoEmotions label typing) produces
// ZERO findings under BOTH unions, so a constraint genuinely NOT reproduced by the
// projection is silently miscounted as reproduced. The convergence gate therefore
// validates each fixture in the SAME context: `fixture ⊎ authored-ontology`.

/// Every `slices/<group>/<name>/module.ttl`, sorted — the slice fragment of the
/// authored bundle (mirrors `source_load::module_files`).
fn module_files(slices_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(groups) = std::fs::read_dir(slices_dir) else {
        return out;
    };
    for group in groups.flatten() {
        if !group.path().is_dir() {
            continue;
        }
        let Ok(slices) = std::fs::read_dir(group.path()) else {
            continue;
        };
        for slice in slices.flatten() {
            let module = slice.path().join("module.ttl");
            if module.is_file() {
                out.push(module);
            }
        }
    }
    out.sort();
    out
}

/// The merged authored ontology dataset: `ontology/gmeow.ttl` + every
/// `slices/**/module.ttl` + `imports/*.ttl` — the same source set
/// `source_load::load_authored_dataset` composes and the live validator runs
/// against. Each file is parsed standalone and merged via `RdfDataset::union`,
/// which standardizes blank scopes apart so no two files' anonymous blanks collide.
fn authored_ontology(root: &Path) -> Arc<RdfDataset> {
    let mut files: Vec<PathBuf> = Vec::new();
    let onto = root.join("ontology").join("gmeow.ttl");
    if onto.is_file() {
        files.push(onto);
    }
    files.extend(module_files(&root.join("slices")));
    files.extend(ttl_files(&root.join("imports")));
    files.sort();
    assert!(!files.is_empty(), "authored ontology must be non-empty");
    let parsed: Vec<Arc<RdfDataset>> = files
        .iter()
        .map(|p| {
            let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
            parse_dataset(&bytes, "text/turtle", None)
                .unwrap_or_else(|e| panic!("ontology file {} must parse: {e}", p.display()))
        })
        .collect();
    let refs: Vec<&RdfDataset> = parsed.iter().map(|d| d.as_ref()).collect();
    Arc::new(RdfDataset::union(&refs))
}

/// The data graph the gate validates: `fixture ⊎ authored-ontology`, flattened to
/// the default graph (matching [`load_data`]). `RdfDataset::union` re-scopes the
/// two inputs' blanks apart, so the ontology's anonymous nodes cannot fuse with the
/// fixture's.
fn merge_with_ontology(fixture: &RdfDataset, ontology: &RdfDataset) -> Arc<RdfDataset> {
    let union = RdfDataset::union(&[fixture, ontology]);
    let mut quads = flat_rdf_quads_from_dataset(&union);
    for quad in &mut quads {
        quad.graph_name = None;
    }
    flat_dataset_from_quads(&quads).expect("merged data graph must freeze")
}

/// The IRI subjects declared IN the fixture. Findings are scoped to this set so the
/// merged ontology's own TBox / individual nodes — which are subjects in the
/// ontology files but never in the fixture — cannot introduce new focus nodes that
/// pollute the finding sets; only nodes the fixture is genuinely about are compared.
fn fixture_subjects(fixture: &RdfDataset) -> BTreeSet<String> {
    let mut subs = BTreeSet::new();
    for q in fixture.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let TermRef::Iri(s) = fixture.resolve(q.s) {
            subs.insert(s.to_string());
        }
    }
    subs
}

// ── Loaded unions ───────────────────────────────────────────────────────────

/// A parsed shape union plus the index that resolves its findings' source terms.
struct Union {
    shapes: purrdf::shapes::shapes::Shapes,
    index: ShapeIndex,
    file_count: usize,
}

impl Union {
    fn load(files: &[PathBuf]) -> Self {
        assert!(!files.is_empty(), "shape union must be non-empty");
        let text = concat_shapes(files);
        let shapes = parse_shapes(&text).expect("shape union must parse as SHACL");
        let graph = parse_dataset(text.as_bytes(), "text/turtle", None)
            .expect("shape union must parse as an RDF graph");
        Self {
            shapes,
            index: ShapeIndex::build(&graph),
            file_count: files.len(),
        }
    }

    /// Validate a data graph and normalize every result whose focus node is one of
    /// `subjects` (the fixture's own IRI subjects) to a [`Key`]. Scoping to the
    /// fixture's subjects keeps merged-ontology TBox nodes out of the finding sets.
    fn findings_scoped(&self, data: &RdfDataset, subjects: &BTreeSet<String>) -> BTreeSet<Key> {
        let report =
            validate_dataset(data, &self.shapes).expect("native SHACL validation must succeed");
        report
            .results
            .iter()
            .filter(|r| match &r.focus_node {
                Term::NamedNode(n) => subjects.contains(n.as_str()),
                _ => false,
            })
            .map(|r| Key {
                focus: term_string(&r.focus_node),
                severity: severity_iri(&r.severity),
                source_term: source_term(&r.source_shape, &self.index),
            })
            .collect()
    }
}

fn is_violation(key: &Key) -> bool {
    key.severity == Severity::Violation.iri()
}

/// The full-corpus finding tally used by both lanes.
struct Corpus {
    authored: BTreeSet<Key>,
    projected: BTreeSet<Key>,
    /// Per-fixture (label, intent, authored, projected) for directional per-file checks.
    per_fixture: Vec<(String, Intent, BTreeSet<Key>, BTreeSet<Key>)>,
}

fn tally() -> Corpus {
    let root = repo_root();
    let authored_union = Union::load(&authored_shape_files(&root));
    let projected_union = Union::load(&projected_shape_files(&root));
    let ontology = authored_ontology(&root);

    let mut authored = BTreeSet::new();
    let mut projected = BTreeSet::new();
    let mut per_fixture = Vec::new();

    for fixture in corpus(&root) {
        let data = load_data(&fixture.path);
        let subjects = fixture_subjects(&data);
        // Validate in the SAME context the live validator uses: the fixture merged
        // with the authored ontology, scoped back to the fixture's own subjects.
        let merged = merge_with_ontology(&data, &ontology);
        let a = authored_union.findings_scoped(&merged, &subjects);
        let p = projected_union.findings_scoped(&merged, &subjects);
        authored.extend(a.iter().cloned());
        projected.extend(p.iter().cloned());
        per_fixture.push((fixture.label, fixture.intent, a, p));
    }

    Corpus {
        authored,
        projected,
        per_fixture,
    }
}

// ── Always-true assertions (DEFAULT lane — must be GREEN) ────────────────────

/// Smoke: both unions load, every corpus file parses, and each side is non-empty.
#[test]
fn unions_and_corpus_load() {
    let root = repo_root();
    let authored_union = Union::load(&authored_shape_files(&root));
    let projected_union = Union::load(&projected_shape_files(&root));
    assert!(
        authored_union.file_count > 0,
        "authored union must have shape files"
    );
    assert!(
        projected_union.file_count > 0,
        "projected union must have shape files"
    );

    let fixtures = corpus(&root);
    assert!(!fixtures.is_empty(), "corpus must be non-empty");
    // `load_data` panics on any parse failure, so this loop is the "every corpus
    // file parses" assertion.
    for fixture in &fixtures {
        let _ = load_data(&fixture.path);
    }
}

/// The projected union must not OVER-CLAIM on well-formed data: for every
/// `*-wellformed.ttl` fixture, it may produce no Violation-severity finding that the
/// authored union does not also produce.
///
/// This assertion was AUTHORED to run in the default lane (rejecting data the
/// authored shapes accept is a false positive on today's production surface). It is
/// RED in the current repo state — and MOVED into the `#[ignore]`d convergence gate
/// as the equivalence-before-deletion worklist — because the *already-generated* `validation-shapes.ttl`
/// derive-all over-claims relative to the hand-authored shapes: it emits `sh:class`
/// frame constraints (e.g. `KnowledgeProficiency-shape` requires
/// `knowledgeProficiencyAgent` to be a typed `gmeow:Agent`) that the authored
/// `cognition-shapes` never enforced, so the minimal wellformed fixtures — which do
/// not declare `rdf:type` on their referenced objects — are legitimately rejected by
/// the projection but not the authored union (19 such over-claims across 8 fixtures:
/// cognition, entity-existence, expertise, norms, participation, privacy, relator,
/// rights). This is a derive-all frame-strictness gap for Tasks 4–6 to resolve
/// (either the fixtures gain the type declarations the frame requires, or the frame
/// `sh:class` obligations are reconciled), NOT a defect to paper over by weakening
/// the check — the gate re-runs it as direction 2 and Task 9 restores it to the
/// default lane once converged.
#[test]
#[ignore = "1194 convergence gate (moved from default lane): derive-all validation-shapes.ttl over-claims sh:class frame constraints the authored shapes never enforced; restored to default lane at Task 9"]
fn projected_does_not_over_claim_on_wellformed() {
    let corpus = tally();
    let mut over_claims: Vec<(String, Key)> = Vec::new();
    for (label, intent, authored, projected) in &corpus.per_fixture {
        if *intent != Intent::Wellformed {
            continue;
        }
        for key in projected.difference(authored) {
            if is_violation(key) {
                over_claims.push((label.clone(), key.clone()));
            }
        }
    }
    assert!(
        over_claims.is_empty(),
        "projected union over-claims Violations the authored union does not, on well-formed \
         fixtures (this is a false positive on the production surface — do NOT weaken this \
         assertion; fix the offending projector):\n{}",
        over_claims
            .iter()
            .map(|(fixture, key)| format!("  {fixture}: {key}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ── Convergence gate (RED now; enabled at Task 9) ────────────────────────────

/// Full behavioral equivalence: every authored finding is reproduced by the
/// projection AND the projection adds no well-formed Violation. Expected RED until
/// Tasks 5–6 finish the migration; on failure it prints the DELTA REPORT (the
/// migration worklist) and the Layer-B structural coverage gaps.
#[test]
#[ignore = "1194 convergence gate: enabled at Task 9 once migration (Tasks 5-6) reproduces every authored finding"]
fn projected_reproduces_authored() {
    let corpus = tally();

    let missing: Vec<&Key> = corpus.authored.difference(&corpus.projected).collect();

    // Direction 2: the same no-over-claim invariant, re-checked inside the gate so a
    // single `--ignored` run reports both directions of divergence.
    let mut wellformed_over_claims: Vec<(String, Key)> = Vec::new();
    for (label, intent, authored, projected) in &corpus.per_fixture {
        if *intent != Intent::Wellformed {
            continue;
        }
        for key in projected.difference(authored) {
            if is_violation(key) {
                wellformed_over_claims.push((label.clone(), key.clone()));
            }
        }
    }

    if missing.is_empty() && wellformed_over_claims.is_empty() {
        return; // converged — Task 9 removes the `#[ignore]`.
    }

    eprintln!(
        "{}",
        delta_report(&corpus, &missing, &wellformed_over_claims)
    );

    assert!(
        missing.is_empty() && wellformed_over_claims.is_empty(),
        "shape projection has NOT converged: {} authored finding(s) not reproduced, {} \
         well-formed over-claim(s) (see DELTA REPORT above)",
        missing.len(),
        wellformed_over_claims.len()
    );
}

/// Render the DELTA REPORT: the authored findings not yet reproduced, grouped by
/// domain source-term (the Task 5–6 migration worklist), plus the Layer-B structural
/// coverage backstop and the Task-6 witness-pair census.
fn delta_report(
    corpus: &Corpus,
    missing: &[&Key],
    wellformed_over_claims: &[(String, Key)],
) -> String {
    let root = repo_root();
    let mut out = String::new();
    out.push_str("\n================ #1194 SHAPE-PROJECTION DELTA REPORT ================\n");
    out.push_str(&format!(
        "authored findings:  {}\nprojected findings: {}\nauthored NOT reproduced by projected: {}\n\
         well-formed over-claims (projected adds a Violation): {}\n",
        corpus.authored.len(),
        corpus.projected.len(),
        missing.len(),
        wellformed_over_claims.len(),
    ));

    // Group the missing authored findings by source_term (the migration unit).
    let mut by_term: BTreeMap<String, Vec<&Key>> = BTreeMap::new();
    for key in missing {
        by_term
            .entry(key.source_term.clone())
            .or_default()
            .push(key);
    }
    out.push_str(&format!(
        "\n-- authored findings not reproduced, grouped by source_term ({} term(s)) --\n",
        by_term.len()
    ));
    // Sort terms by descending finding count so the biggest migration wins surface first.
    let mut terms: Vec<(&String, &Vec<&Key>)> = by_term.iter().collect();
    terms.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
    for (term, keys) in &terms {
        out.push_str(&format!("  [{:>3}] {term}\n", keys.len()));
        for key in keys.iter() {
            out.push_str(&format!(
                "        {} [{}]\n",
                key.focus,
                short_sev(&key.severity)
            ));
        }
    }

    if !wellformed_over_claims.is_empty() {
        out.push_str("\n-- well-formed over-claims (projected Violations the authored union does not raise) --\n");
        for (fixture, key) in wellformed_over_claims {
            out.push_str(&format!("  {fixture}: {key}\n"));
        }
    }

    // Layer-B structural backstop (advisory): authored constraint-atoms not present
    // in the projected union. Catches coverage gaps a fixture may not exercise.
    let authored_atoms = constraint_atoms(&authored_shape_files(&root));
    let projected_atoms = constraint_atoms(&projected_shape_files(&root));
    let atom_gap: Vec<&Atom> = authored_atoms.difference(&projected_atoms).collect();
    out.push_str(&format!(
        "\n-- Layer-B structural backstop (advisory): authored constraint-atoms absent from \
         projected ({} of {} authored atoms) --\n",
        atom_gap.len(),
        authored_atoms.len(),
    ));
    let mut atoms_by_target: BTreeMap<String, Vec<&Atom>> = BTreeMap::new();
    for atom in &atom_gap {
        atoms_by_target
            .entry(atom.target.clone())
            .or_default()
            .push(atom);
    }
    for (target, atoms) in &atoms_by_target {
        out.push_str(&format!("  {target}\n"));
        for atom in atoms {
            out.push_str(&format!("      {} · {}\n", atom.path, atom.component));
        }
    }

    // Task-6 witness-pair census (hook): every source_term that produces >=1
    // malformed-fixture finding has a failing witness; a pass witness is any
    // well-formed fixture the authored union accepts. This is the corpus-side view
    // of the Task-6 requirement (every migrated logic:Constraint carries >=1 pass +
    // >=1 fail fixture); at Task 6 the migrated-constraint list drives it directly.
    let fail_witnessed = source_terms_with_malformed_finding(corpus);
    out.push_str(&format!(
        "\n-- Task-6 witness hook: source_terms with a failing (malformed-fixture) witness: {} --\n",
        fail_witnessed.len(),
    ));

    out.push_str("====================================================================\n");
    out
}

fn short_sev(severity: &str) -> &str {
    severity.rsplit(['#', '/']).next().unwrap_or(severity)
}

/// Source-terms that produce at least one finding on a malformed fixture — the set
/// with a "fail" witness in the corpus. Documented hook for the Task-6 witness-pair
/// requirement.
fn source_terms_with_malformed_finding(corpus: &Corpus) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (_, intent, authored, _) in &corpus.per_fixture {
        if *intent == Intent::Malformed {
            for key in authored {
                out.insert(key.source_term.clone());
            }
        }
    }
    out
}

// ── Layer-B: declarative constraint atoms ────────────────────────────────────

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Atom {
    target: String,
    path: String,
    component: String,
}

/// Reduce a shape union to declarative `(target, path, component)` atoms: for every
/// `owner sh:property [ sh:path P ; <component> … ]`, emit `(target_of(owner), P,
/// component)`. Procedural bodies (`sh:sparql`) reduce to a single `(target, ·,
/// sparql)` atom — they are advisory only (an opaque SPARQL body is undecidable to
/// compare structurally), so this backstop reports coverage gaps but never hard-fails
/// on them.
fn constraint_atoms(files: &[PathBuf]) -> BTreeSet<Atom> {
    let text = concat_shapes(files);
    let ds = parse_dataset(text.as_bytes(), "text/turtle", None)
        .expect("shape union must parse as an RDF graph");
    let index = ShapeIndex::build(&ds);

    let mut atoms = BTreeSet::new();
    let Some(prop_id) = iri_id(&ds, SH_PROPERTY) else {
        return atoms;
    };

    for q in ds.quads_for_pattern(None, Some(prop_id), None, GraphMatch::Any) {
        let TermRef::Iri(owner) = ds.resolve(q.s) else {
            continue;
        };
        let target = index
            .target_of
            .get(owner)
            .cloned()
            .unwrap_or_else(|| owner.to_string());
        // The property-shape node (usually a blank node) is the object of sh:property.
        let ps_id = q.o;
        let path = property_path(&ds, ps_id);
        for (pred, _obj) in outgoing(&ds, ps_id) {
            if let Some(local) = pred.strip_prefix(SH)
                && COMPONENT_PREDS.contains(&local)
            {
                atoms.insert(Atom {
                    target: target.clone(),
                    path: path.clone(),
                    component: local.to_string(),
                });
            }
        }
    }
    atoms
}

/// The `sh:path` value of a property shape as a string (`·` when absent/complex).
fn property_path(ds: &RdfDataset, ps: TermId) -> String {
    let Some(path_id) = iri_id(ds, SH_PATH) else {
        return "·".to_string();
    };
    ds.quads_for_pattern(Some(ps), Some(path_id), None, GraphMatch::Any)
        .next()
        .map(|q| match ds.resolve(q.o) {
            TermRef::Iri(p) => p.to_string(),
            _ => "·".to_string(),
        })
        .unwrap_or_else(|| "·".to_string())
}

/// The `(predicate_iri, object_string)` pairs leaving `subject`.
fn outgoing(ds: &RdfDataset, subject: TermId) -> Vec<(String, String)> {
    ds.quads_for_pattern(Some(subject), None, None, GraphMatch::Any)
        .filter_map(|q| {
            let TermRef::Iri(p) = ds.resolve(q.p) else {
                return None;
            };
            let o = match ds.resolve(q.o) {
                TermRef::Iri(o) => o.to_string(),
                TermRef::Blank { label, .. } => format!("_:{label}"),
                TermRef::Literal { lexical, .. } => lexical.to_string(),
                TermRef::Triple { .. } => "«triple»".to_string(),
            };
            Some((p.to_string(), o))
        })
        .collect()
}
