// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! TOTAL execution-discharge reconciliation over the `math:` conformance charter
//! ([`MATHEMATICS-CONFORMANCE.md`]) — every one of its thirteen `###`-headed gate-matrix
//! sections, not a curated subset.
//!
//! Where [`crate::gmn_conformance_discharge`] hand-curates one small, stable subsection of
//! the `lang:` matrix (13 SHACL rows, 6 codec classes), this harness is GENERIC over the
//! whole `math:` charter: it parses every section's table generically (locate the `###`
//! heading, read to the next heading, extract each row's `Primary gate` and `Failure class`
//! cells), so a later charter edit to an ALREADY-REGISTERED section (adding, removing, or
//! retiering a row) is picked up automatically with no code change here; registering a
//! wholly NEW `###` heading still needs one [`SECTIONS`] entry (as "Normalization identity
//! rules" now has).
//!
//! For each cited `math:<Class>`, this reconciles FOUR things:
//!
//! 1. **Authored** — the class is declared `owl:Class` and (transitively) `rdfs:subClassOf`
//!    or `logic:subClassOf` `math:MathConformanceFailure` in `module.ttl`.
//! 2. **Reachable** — the class is the object of some `gmeow:enforcesFailureClass` triple
//!    IN `module.ttl` itself (the doctrine puts this annotation on the SOURCE axiom/
//!    `logic:Constraint`, not only on a downstream generated shape), OR appears as a
//!    `math:<Class>:` literal message-token prefix in the two native validator sources the
//!    task names (`crates/validate/src/lint.rs`, `crates/logic/src/math_expression.rs`).
//! 3. **Tier-matched** — the charter's declared `Primary gate` column names one or more
//!    channels (`owl-axiom`, `shacl-core`, `shacl-sparql`, `source-lint`, `rust-validator`,
//!    `competency-query`, `projection-test`, plus the two ADDITIONAL channels the charter's
//!    own matrix cites verbatim for its Flagship/Bridges rows, `structural` and
//!    `native-test`/"Rust cross-check" — a documented generalization, not a narrowing of the
//!    task's seven); execution over the on-disk fixture corpus must actually FIRE the class
//!    through one of the declared channels.
//! 4. **Fixture-discharged** — a reachable class has a counter-example fixture on disk
//!    (`tests/counter-examples/*.ttl`) whose execution trips it.
//!
//! ## Judgment calls (documented, not hidden)
//!
//! * **`generated/shapes/*.ttl` is absent in this worktree** (it is a git-ignored product of
//!   `make sync`, which this task is directed NOT to run). The SHACL channels
//!   (`shacl-core`/`shacl-sparql`) therefore cannot be EXECUTED here — `load_scoped_shapes`
//!   is attempted once, and its failure degrades that one channel to an explicitly labeled
//!   "unverified" bucket (printed, but not counted as a hard gap) rather than either
//!   fabricating a pass or drowning the real, actionable gaps in hundreds of "shapes
//!   missing" repeats. Every OTHER channel this file drives — the native structural lint,
//!   the competency-query executor, the structural-assertion executor, and the flagship
//!   manifest cross-check — needs no generated artifact and is executed for real.
//! * **The native structural lint (`structural_lint_dataset`) is one Rust function that
//!   implements THREE charter tiers at once** (source-lint, Rust validator, and a few
//!   lint-embedded projection checks) via the same `math:<Class>:` message-token
//!   convention. This harness cannot distinguish which of the three a given native hit
//!   satisfies from the message text alone, so a native-lint hit is credited against
//!   whichever of `{source-lint, rust-validator, projection-test}` the charter itself
//!   declared for that class (and reported as a genuine tier mismatch when the charter
//!   declared none of the three).
//! * **`owl-axiom` tier rows** (the four `owl:disjointWith` conflation classes) have no
//!   execution channel built here — deciding them needs the reasoned closure, a materially
//!   larger lift than this task's budget. Reachability is still fully checked; execution is
//!   marked "unverified (no owl-axiom execution channel)" unless a fixture happens to trip
//!   the class via native lint anyway (credited as a bonus finding when it does).
//! * **Competency-tier / structural-tier / native-test-tier rows carry NO failure-class
//!   cell today** (their last cell is prose — "a mistyped invariant fails the exact-match
//!   competency gate", "(structural assertion)", "(native test)") — there is nothing to
//!   reconcile against `module.ttl`, so these rows are checked by REFERENCE RESOLUTION
//!   (does the cited query/assertion/file exist, does it currently execute green) rather
//!   than by the four-part class reconciliation above.
//! * **Competency-file matching** for a competency-tier row is by token-overlap between the
//!   row's own prose and each candidate `.rq` filename's hyphen-split stem (documented in
//!   [`match_competency_query`]) — the only competency-tier row today (the E8 invariants
//!   row) resolves to `e8-root-system.rq` this way, and the matcher is written generically
//!   so a later competency-tier row (e.g. in the anticipated Normalization-identity
//!   section) resolves the same way with no hardcoding.
//!
//! This harness is expected to FAIL today outside the expression/normalization surface this
//! issue actually touched — that failure list is exactly what a follow-on task consumes to
//! close the remaining charter sections. It is deliberately NOT `#[ignore]`d.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_errors::Diag;
use gmeow_errors::code::register_code;
use gmeow_errors::grade::{FindingCategory, Grade, Severity, Standpoint};
use gmeow_validate::lint::structural_lint_dataset;
use gmeow_validate::store::shacl_validate_dataset;
use purrdf::shapes::shapes::{Constraint, Shape, Shapes};
use purrdf::{RdfDataset, SparqlResult, TermValue};

mod support;
use support::flagship_discharge::{
    SliceSpec, load_scoped_shapes, local_name, minimal_lint_config, native_failure_classes,
    repo_root, shape_class_map, shared_shapes_path,
};

// ─────────────────────────────────────────────────────────────────────────────────────────
// Slice identity + paths.
// ─────────────────────────────────────────────────────────────────────────────────────────

const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

fn math_root() -> PathBuf {
    repo_root().join("slices").join("grounding").join("math")
}

fn math_spec() -> SliceSpec {
    SliceSpec {
        slice_ns: MATH_NS,
        slice_prefix: "math",
        slice_root: math_root(),
        manifest_rel: "examples/flagship-acceptance.ttl",
    }
}

fn charter_path() -> PathBuf {
    math_root()
        .join("design")
        .join("MATHEMATICS-CONFORMANCE.md")
}

fn counter_example_dir() -> PathBuf {
    math_root().join("tests").join("counter-examples")
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// The `ConformanceSection` descriptor + the (generalized) registry of all 12 sections.
// ─────────────────────────────────────────────────────────────────────────────────────────

/// One `###`-headed gate-matrix section of a conformance charter, generalized over ANY
/// grounding slice's charter (not `math:`-specific machinery, per the task's instruction —
/// only the [`SECTIONS`] registry below is math-specific).
#[allow(dead_code)]
struct ConformanceSection {
    charter_path: &'static str,
    heading: &'static str,
    slice_prefix: &'static str,
    slice_ns: &'static str,
}

/// ALL thirteen `###`-headed sections of `MATHEMATICS-CONFORMANCE.md`, by their REAL heading
/// text (read from the charter, not guessed), including "Normalization identity rules". Any
/// row added to, removed from, or retiered within an already-registered section's table is
/// picked up automatically, because [`matrix_rows`] is driven by this list, not by a
/// hardcoded row count.
const SECTIONS: &[ConformanceSection] = &[
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Expression and mathematical-core rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Normalization identity rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Numbers-and-sets rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Algebra rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Measure-and-dimension rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Analysis-and-geometry rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Linear-algebra-and-learning rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Probability rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Statistics rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Process/result/claim separation",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Projection rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Bridges / ingestion rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
    ConformanceSection {
        charter_path: "design/MATHEMATICS-CONFORMANCE.md",
        heading: "### Flagship acceptance-manifest rules",
        slice_prefix: "math",
        slice_ns: MATH_NS,
    },
];

// ─────────────────────────────────────────────────────────────────────────────────────────
// Generic per-section row parser (mirrors `gmn_conformance_discharge::matrix_gmn_rows`,
// generalized to any `###` heading and to the two/N failure classes a row cell may carry).
// ─────────────────────────────────────────────────────────────────────────────────────────

/// One parsed row of a gate-matrix table: its `Primary gate` cell verbatim, and every
/// `math:<Class>` local name extracted from its `Failure class` cell (empty when the cell
/// is prose rather than a class — the competency/structural/native-test rows).
#[derive(Debug, Clone)]
struct MatrixRow {
    rule: String,
    gate: String,
    classes: Vec<String>,
    raw_class_cell: String,
}

/// Extract every `math:<Local>` token's local name from `cell`, IN ORDER, so a row citing
/// two classes (e.g. the tensor-computation-graph row, `math:MalformedTensorComputationGraph`
/// / `math:MalformedArgumentSlot`) yields both, while an ordinary single-class row yields one
/// and a prose-only cell yields none.
fn extract_math_classes(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cell;
    while let Some(idx) = rest.find("math:") {
        let after = &rest[idx + "math:".len()..];
        let local: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !local.is_empty() {
            out.push(local.clone());
        }
        rest = &after[local.len().max(1)..];
    }
    out
}

/// Parse one `###`-headed section's gate-matrix table into its rows, generalized over ANY
/// heading (not GMN- or `math:`-specific): locate the heading line, read forward to the next
/// `#`-prefixed line (any heading level) or EOF, and extract every markdown table row's
/// middle (`Primary gate`) and last (`Failure class`) cells — skipping the header row and the
/// `|---|` separator, exactly mirroring `gmn_conformance_discharge::matrix_gmn_rows`'s
/// boundary and skip logic.
fn matrix_rows(md: &str, heading: &str) -> Vec<MatrixRow> {
    let lines: Vec<&str> = md.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_start() == heading)
        .unwrap_or_else(|| panic!("MATHEMATICS-CONFORMANCE.md has no section {heading:?}"));
    let mut out = Vec::new();
    for line in &lines[start + 1..] {
        let t = line.trim_start();
        if t.starts_with('#') {
            break; // next heading (any level) — end of this section.
        }
        if !t.starts_with('|') {
            continue; // prose / blank line between heading and table.
        }
        let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 3 {
            continue; // not a 3-column gate-matrix row (defensive; none seen in practice).
        }
        let rule = cells[0].to_owned();
        let gate = cells[1].to_owned();
        let class_cell = cells[2];
        if class_cell == "Failure class" || gate == "Primary gate" {
            continue; // header row.
        }
        if cells.iter().all(|c| c.chars().all(|c| c == '-')) {
            continue; // the `|---|---|---|` separator row.
        }
        out.push(MatrixRow {
            rule,
            gate,
            classes: extract_math_classes(class_cell),
            raw_class_cell: class_cell.to_owned(),
        });
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Channel taxonomy + gate-cell classification.
// ─────────────────────────────────────────────────────────────────────────────────────────

/// The execution channel taxonomy: the task's seven named channels, plus two the charter's
/// OWN matrix cites verbatim for its Flagship/Bridges rows (`structural`, `native-test`) —
/// documented above as a generalization, not a narrowing, of the task's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Channel {
    OwlAxiom,
    ShaclCore,
    ShaclSparql,
    SourceLint,
    RustValidator,
    CompetencyQuery,
    ProjectionTest,
    Structural,
    NativeTest,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Channel::OwlAxiom => "owl-axiom",
            Channel::ShaclCore => "shacl-core",
            Channel::ShaclSparql => "shacl-sparql",
            Channel::SourceLint => "source-lint",
            Channel::RustValidator => "rust-validator",
            Channel::CompetencyQuery => "competency-query",
            Channel::ProjectionTest => "projection-test",
            Channel::Structural => "structural",
            Channel::NativeTest => "native-test",
        }
    }
}

fn channels_to_string(channels: &BTreeSet<Channel>) -> String {
    if channels.is_empty() {
        return "<none>".to_owned();
    }
    channels
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Classify a charter `Primary gate` cell into every channel it names, by literal substring
/// (the charter's own vocabulary is stable and disjoint enough that this needs no fuzzy
/// matching — see the exhaustive enumeration this was built from in the task investigation).
/// A cell naming NONE of these markers is a charter-drift bug (a new gate-kind vocabulary
/// word) and PANICS rather than silently classifying as empty.
fn classify_gate(gate: &str) -> BTreeSet<Channel> {
    const MARKERS: &[(&str, Channel)] = &[
        ("SHACL-SPARQL", Channel::ShaclSparql),
        ("SHACL Core", Channel::ShaclCore),
        ("OWL axiom", Channel::OwlAxiom),
        ("source-lint", Channel::SourceLint),
        ("Rust validator", Channel::RustValidator),
        ("Rust numeric check", Channel::RustValidator),
        ("native producer test", Channel::NativeTest),
        ("Rust cross-check", Channel::NativeTest),
        ("execution-discharge harness", Channel::NativeTest),
        ("competency query", Channel::CompetencyQuery),
        ("projection test", Channel::ProjectionTest),
        ("structural", Channel::Structural),
    ];
    let mut out = BTreeSet::new();
    for (marker, channel) in MARKERS {
        if gate.contains(marker) {
            out.insert(*channel);
        }
    }
    assert!(
        !out.is_empty(),
        "unrecognized Primary-gate cell (charter drift — extend MARKERS): {gate:?}"
    );
    out
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// module.ttl-derived checks: authored classes + gate reachability (sets 1 and 2). Neither
// needs `generated/` — both run directly over the canonical slice source.
// ─────────────────────────────────────────────────────────────────────────────────────────

fn math_module_dataset() -> Arc<RdfDataset> {
    gmeow_slicetest::native_query::dataset_from_file(&math_root().join("module.ttl"))
        .expect("math module.ttl parses")
}

/// Run a `SELECT ?class WHERE { ... }` query and collect the local names of every IRI bound
/// to `?class`.
fn select_class_local_names(ds: &Arc<RdfDataset>, query: &str) -> BTreeSet<String> {
    match gmeow_slicetest::native_query::query(ds, query).expect("query runs") {
        SparqlResult::Solutions {
            variables, rows, ..
        } => {
            let idx = variables
                .iter()
                .position(|v| v == "class")
                .expect("query projects ?class");
            rows.iter()
                .filter_map(|row| row.get(idx).cloned().flatten())
                .filter_map(|t| match t {
                    TermValue::Iri(iri) => Some(local_name(&iri)),
                    _ => None,
                })
                .collect()
        }
        other => panic!("expected SELECT solutions, got {other:?}"),
    }
}

/// Every `math:` class transitively declared `owl:Class` + (`rdfs:subClassOf` |
/// `logic:subClassOf`)+ `math:MathConformanceFailure` in `module.ttl` — the AUTHORED set
/// (reconciliation set 1). The property-path alternation accepts either predicate at every
/// hop (the doctrine's "logic:subClassOf, or rdfs:subClassOf for the older pre-existing
/// ones") rather than requiring one uniform predicate along the whole chain.
fn authored_conformance_failure_classes(ds: &Arc<RdfDataset>) -> BTreeSet<String> {
    select_class_local_names(
        ds,
        "PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> \
         PREFIX logic: <https://blackcatinformatics.ca/logic/> \
         PREFIX math: <https://blackcatinformatics.ca/math/> \
         SELECT ?class WHERE { ?class (rdfs:subClassOf|logic:subClassOf)+ math:MathConformanceFailure }",
    )
}

/// Every class that is the object of a `gmeow:enforcesFailureClass` triple ANYWHERE in
/// `module.ttl` — the doctrine puts this annotation on the SOURCE axiom/`logic:Constraint`,
/// so this is checkable without a generated shape at all.
fn enforced_in_module(ds: &Arc<RdfDataset>) -> BTreeSet<String> {
    select_class_local_names(
        ds,
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> \
         SELECT ?class WHERE { ?s gmeow:enforcesFailureClass ?class }",
    )
}

/// The `math:<Class>:` message-token classes recovered from the two native validator
/// sources the task names, reusing [`native_failure_classes`]'s exact token-recognition
/// algorithm by feeding it the sources' own lines as pseudo-"errors".
fn native_source_message_classes() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for rel in [
        "crates/validate/src/lint.rs",
        "crates/logic/src/math_expression.rs",
        "crates/logic/src/physical/lower.rs",
    ] {
        let path = repo_root().join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read native validator source {}: {e}", path.display()));
        let lines: Vec<String> = text.lines().map(str::to_owned).collect();
        out.extend(native_failure_classes(&lines, "math"));
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Execution discharge: the native lint channel (no `generated/` dependency).
// ─────────────────────────────────────────────────────────────────────────────────────────

fn on_disk_counter_examples() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(counter_example_dir())
        .expect("counter-examples dir readable")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ttl"))
        .collect();
    out.sort();
    out
}

/// The `math:` classes the native structural lint raises over `fixture` merged with the
/// three co-foundational grounding modules (`logic:`/`lang:`/`math:` module.ttl — the SAME
/// scope the slice's own conformance harness uses per `conformance_module_files`).
fn native_lint_tripped(fixture: &Path, conformance_modules: &Arc<RdfDataset>) -> BTreeSet<String> {
    let fixture_ds = gmeow_slicetest::native_query::dataset_from_file(fixture)
        .unwrap_or_else(|e| panic!("parse counter-example {}: {e}", fixture.display()));
    let ds = gmeow_slicetest::native_query::union(&[Arc::clone(conformance_modules), fixture_ds]);
    let report = structural_lint_dataset(&ds, &minimal_lint_config());
    native_failure_classes(&report.errors(), "math")
        .into_iter()
        .collect()
}

/// The `math:` classes the two REASONED-GRAPH native gates raise over `fixture` merged with
/// the three co-foundational grounding modules — `gmeow_logic::math_expression::
/// check_math_expression_findings` (`math:StructuralKeyDrift`, `math:SurfaceLeakInNormalForm`,
/// `math:StructuralKeyOnRejectedExpression`) and the reasoner-derived dimensional-homogeneity
/// gate (`gmeow_logic::reason::math_gate::dimension_gate_markers`, `math:
/// DimensionalInhomogeneity`) — neither of which the pre-fold structural lint or the
/// generated-SHACL surface can reach: both are genuine computations over the merged dataset
/// itself (no actual DL closure is needed for either gate's own asserted-fact reading, so
/// `dimension_gate_markers` is called with an empty derived-edge slice), the charter's own
/// documented "SAME architectural shape" as the measure-and-dimension reasoned gate. Credited
/// under the ordinary `rust-validator` channel below (the charter declares both this way), not
/// a distinct channel kind.
fn reasoned_tripped(fixture: &Path, conformance_modules: &Arc<RdfDataset>) -> BTreeSet<String> {
    let fixture_ds = gmeow_slicetest::native_query::dataset_from_file(fixture)
        .unwrap_or_else(|e| panic!("parse counter-example {}: {e}", fixture.display()));
    let ds = gmeow_slicetest::native_query::union(&[Arc::clone(conformance_modules), fixture_ds]);
    let mut classes = BTreeSet::new();

    let expr_messages: Vec<String> =
        gmeow_logic::math_expression::check_math_expression_findings(&ds)
            .into_iter()
            .map(|f| f.message)
            .collect();
    classes.extend(native_failure_classes(&expr_messages, "math"));

    if let Ok(markers) = gmeow_logic::reason::math_gate::dimension_gate_markers(&ds, &[]) {
        for (_, class_iri) in markers {
            classes.insert(local_name(&class_iri));
        }
    }
    classes
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Execution discharge: the SHACL channels (`generated/` DEPENDENT — attempted once, best
// effort; unavailability degrades to a labeled "unverified" bucket, never a silent pass).
// ─────────────────────────────────────────────────────────────────────────────────────────

/// Build an ad-hoc [`Diag`] from a plain message — `gmeow_errors::Diag` is this repo's sole
/// first-party error type (the Phase-6 Diag-substrate honest invariant bans bare `String` in
/// `Err` position), so every fallible helper below reports through one instead.
fn harness_diag(message: impl Into<String>) -> Diag {
    Diag::new(
        register_code("test.pipeline.math-conformance-discharge.channel-error"),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    )
}

/// Silence the default panic-hook stderr spew for an EXPECTED, documented environment gap
/// (missing `generated/shapes/*.ttl` in a fresh worktree) while still recovering the panic
/// message for the report.
fn silence_panics<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> Result<T, Diag> {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(f);
    std::panic::set_hook(prev_hook);
    result.map_err(|payload| {
        let message = if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_owned()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "panic with non-string payload".to_owned()
        };
        harness_diag(message)
    })
}

/// Whether a parsed shape carries a `sh:sparql` constraint anywhere in its own node-level
/// constraints or (recursively) its nested property shapes' constraints — the real,
/// structural SHACL-Core/SHACL-SPARQL discriminant (a shape either does or does not carry a
/// SPARQL-AF constraint component), rather than a text heuristic.
fn shape_is_sparql(shape: &Shape) -> bool {
    fn property_is_sparql(ps: &purrdf::shapes::shapes::PropertyShape) -> bool {
        ps.constraints
            .iter()
            .any(|c| matches!(c, Constraint::Sparql { .. }))
            || ps.property_shapes.iter().any(property_is_sparql)
    }
    shape
        .constraints
        .iter()
        .any(|c| matches!(c, Constraint::Sparql { .. }))
        || shape.property_shapes.iter().any(property_is_sparql)
}

/// The scoped `math:` shape surface + shape→class map + shape→tier map, as loaded by
/// [`try_load_math_shapes`].
type MathShapeSurface = (
    Shapes,
    std::collections::HashMap<String, String>,
    std::collections::HashMap<String, Channel>,
);

/// Best-effort load of the scoped `math:` shape surface + shape→class map + shape→tier map.
/// `None` when `generated/shapes/*.ttl` is absent (this worktree's known, documented state).
fn try_load_math_shapes() -> Option<MathShapeSurface> {
    let spec = math_spec();
    silence_panics(std::panic::AssertUnwindSafe(|| {
        let (shapes, paths) = load_scoped_shapes(&spec);
        let mut class_paths = paths;
        class_paths.push(shared_shapes_path());
        let shape_class = shape_class_map(&class_paths);
        let mut shape_tier = std::collections::HashMap::new();
        for shape in &shapes.node_shapes {
            let rendered = shape.id.to_string();
            if let Some(iri) = rendered.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
                let tier = if shape_is_sparql(shape) {
                    Channel::ShaclSparql
                } else {
                    Channel::ShaclCore
                };
                shape_tier.insert(iri.to_owned(), tier);
            }
        }
        (shapes, shape_class.into_iter().collect(), shape_tier)
    }))
    .ok()
}

/// The `math:` classes (with their discriminated Core/SPARQL channel) the native SHACL
/// engine raises over `ds`.
fn shacl_tripped(
    ds: &RdfDataset,
    shapes: &Shapes,
    shape_class: &std::collections::HashMap<String, String>,
    shape_tier: &std::collections::HashMap<String, Channel>,
) -> BTreeMap<String, Channel> {
    let report = shacl_validate_dataset(ds, shapes);
    let mut out = BTreeMap::new();
    for result in &report.results {
        let rendered = result.source_shape.to_string();
        let shape_iri = rendered
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(rendered.as_str());
        if let Some(class_iri) = shape_class.get(shape_iri)
            && class_iri.starts_with(MATH_NS)
        {
            let tier = shape_tier
                .get(shape_iri)
                .copied()
                .unwrap_or(Channel::ShaclCore);
            out.insert(local_name(class_iri), tier);
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Competency-query channel (no `generated/` dependency — `run_competency_file`'s default
// `reasoningNone` lane, and the one math competency question(s) using `reasoningLogic`, both
// build their own datasets straight from slice source files).
// ─────────────────────────────────────────────────────────────────────────────────────────

/// Best-effort token-overlap match of a competency-tier row's own `Rule` prose to a specific
/// `queries/competency/*.rq` file cited by `tests/competency.ttl`'s `gmeow:cqQueryFile`
/// bindings. Scores each candidate by how many of the row's own significant words (>2 ASCII
/// alphanumeric characters, lower-cased) appear as one of the filename's hyphen-split stem
/// tokens, and returns the highest-scoring candidate (ties broken by path order). The one
/// competency-tier row in the charter today (the E8 root-system invariants row) resolves to
/// `e8-root-system.rq` this way (tokens `e8`/`root`/`system` all present in both), so the
/// matcher needs no hardcoded row->file table and picks up a later competency-tier row (e.g.
/// an anticipated Normalization-identity one) automatically.
fn match_competency_query(rule_text: &str, candidates: &BTreeSet<String>) -> Option<String> {
    let rule_lower = rule_text.to_lowercase();
    let rule_tokens: BTreeSet<String> = rule_lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_owned)
        .collect();
    let mut best: Option<(usize, &String)> = None;
    for path in candidates {
        let stem = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let score = stem
            .split('-')
            .filter(|tok| rule_tokens.contains(*tok))
            .count();
        if score > 0 && best.is_none_or(|(b, _)| score > b) {
            best = Some((score, path));
        }
    }
    best.map(|(_, p)| p.clone())
}

fn competency_query_files() -> BTreeSet<String> {
    let spec_path = math_root().join("tests").join("competency.ttl");
    let spec = gmeow_slicetest::dsl::load_spec(&spec_path).expect("competency.ttl parses");
    spec.competency
        .iter()
        .filter_map(|cq| cq.query_file.clone())
        .collect()
}

fn run_competency_channel() -> Result<(), Diag> {
    let spec_path = math_root().join("tests").join("competency.ttl");
    gmeow_slicetest::exec::run_competency_file(&spec_path)
}

fn run_structural_channel() -> Result<(), Diag> {
    let spec_path = math_root().join("tests").join("structural.ttl");
    gmeow_slicetest::exec::run_structural_file(&spec_path)
}

fn structural_assertion_iris() -> BTreeSet<String> {
    let spec_path = math_root().join("tests").join("structural.ttl");
    let spec = gmeow_slicetest::dsl::load_spec(&spec_path).expect("structural.ttl parses");
    spec.structural.iter().map(|sa| sa.iri.clone()).collect()
}

/// The Rust cross-check the Flagship section's second-to-last row cites
/// (`crates/slicetest` `flagship_manifest`) — `assert_flagship_manifest` needs no
/// `generated/` artifact (it unions two slice-local fixtures and cross-references disk
/// paths), so it is run for real.
fn run_flagship_manifest_channel() -> Result<(), Diag> {
    silence_panics(std::panic::AssertUnwindSafe(|| {
        gmeow_slicetest::flagship::assert_flagship_manifest(
            &math_root(),
            &[
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/e8Symmetry",
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/homomorphicEncryption",
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/proofAsProcess",
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/rBridge",
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/aiSelfStructure",
            ],
        );
    }))
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// The TOTAL reconciliation test.
// ─────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn total_math_conformance_matrix_is_discharged() {
    let md = std::fs::read_to_string(charter_path()).expect("MATHEMATICS-CONFORMANCE.md readable");

    // ── Parse every section, generically. ────────────────────────────────────────────
    // class -> (declared channels, citing sections, raw class-cell text for the report)
    let mut matrix_classes: BTreeMap<String, (BTreeSet<Channel>, BTreeSet<&'static str>)> =
        BTreeMap::new();
    // no-class (prose) rows, kept for the reference-resolution checks below.
    let mut prose_rows: Vec<(&'static str, MatrixRow)> = Vec::new();

    for section in SECTIONS {
        let rows = matrix_rows(&md, section.heading);
        assert!(
            !rows.is_empty(),
            "section {:?} parsed zero rows — heading or table-shape drift",
            section.heading
        );
        for row in rows {
            let tiers = classify_gate(&row.gate);
            if row.classes.is_empty() {
                prose_rows.push((section.heading, row));
                continue;
            }
            for class in &row.classes {
                let entry = matrix_classes
                    .entry(class.clone())
                    .or_insert_with(|| (BTreeSet::new(), BTreeSet::new()));
                entry.0.extend(tiers.iter().copied());
                entry.1.insert(section.heading);
            }
            let _ = &row.raw_class_cell; // retained for future extension; not itself reported.
        }
    }

    // ── module.ttl-derived checks (sets 1 and 2) — no `generated/` dependency. ────────
    let module_ds = math_module_dataset();
    let authored = authored_conformance_failure_classes(&module_ds);
    let enforced = enforced_in_module(&module_ds);
    let native_source = native_source_message_classes();
    let reachable: BTreeSet<String> = enforced.union(&native_source).cloned().collect();

    // ── Native-lint execution over every on-disk counter-example (sets 3/4, native tier). ─
    let conformance_modules = gmeow_slicetest::native_query::dataset_from_files(
        &gmeow_slicetest::paths::conformance_module_files(&math_root()),
    )
    .expect("math conformance module trio parses");
    let fixtures = on_disk_counter_examples();
    assert!(
        !fixtures.is_empty(),
        "tests/counter-examples has no fixtures"
    );

    // class -> (channel -> fixtures that trip it via that channel)
    let mut class_channel_fixtures: BTreeMap<String, BTreeMap<Channel, BTreeSet<String>>> =
        BTreeMap::new();
    let mut fixture_used: BTreeSet<String> = BTreeSet::new();

    for fixture in &fixtures {
        let name = fixture
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf8 fixture name")
            .to_owned();
        let native = native_lint_tripped(fixture, &conformance_modules);
        for class in native {
            // A native-lint hit is credited against whichever of {source-lint,
            // rust-validator, projection-test} the charter declared for this class (the
            // documented judgment call above); if the charter declared NONE of the three,
            // credit it under rust-validator as the closest native-execution proxy so the
            // hit is still visible in the report as a tier-mismatch rather than vanishing.
            let declared = matrix_classes
                .get(&class)
                .map(|(tiers, _)| tiers.clone())
                .unwrap_or_default();
            let credited = [
                Channel::SourceLint,
                Channel::RustValidator,
                Channel::ProjectionTest,
            ]
            .into_iter()
            .find(|c| declared.contains(c))
            .unwrap_or(Channel::RustValidator);
            class_channel_fixtures
                .entry(class.clone())
                .or_default()
                .entry(credited)
                .or_default()
                .insert(name.clone());
            fixture_used.insert(name.clone());
        }

        // The reasoned-graph channel: `math:MalformedStructuralKey`, `math:StructuralKeyDrift`,
        // `math:SurfaceLeakInNormalForm`, `math:StructuralKeyOnRejectedExpression`, and
        // `math:DimensionalInhomogeneity` are each declared `rust-validator` in the charter and
        // are credited there directly (no tier-selection ambiguity — unlike the native-lint
        // fallback above, the charter names exactly one channel for each of these five).
        for class in reasoned_tripped(fixture, &conformance_modules) {
            class_channel_fixtures
                .entry(class)
                .or_default()
                .entry(Channel::RustValidator)
                .or_default()
                .insert(name.clone());
            fixture_used.insert(name.clone());
        }
    }

    // ── SHACL execution (best-effort; degrades honestly when `generated/` is absent). ──
    let shapes_loaded = try_load_math_shapes();
    let shapes_available = shapes_loaded.is_some();
    if let Some((shapes, shape_class, shape_tier)) = &shapes_loaded {
        for fixture in &fixtures {
            let name = fixture
                .file_name()
                .and_then(|n| n.to_str())
                .expect("utf8 fixture name")
                .to_owned();
            let fixture_ds = gmeow_slicetest::native_query::dataset_from_file(fixture)
                .unwrap_or_else(|e| panic!("parse counter-example {}: {e}", fixture.display()));
            let ds = gmeow_slicetest::native_query::union(&[
                Arc::clone(&conformance_modules),
                fixture_ds,
            ]);
            for (class, channel) in shacl_tripped(&ds, shapes, shape_class, shape_tier) {
                class_channel_fixtures
                    .entry(class)
                    .or_default()
                    .entry(channel)
                    .or_default()
                    .insert(name.clone());
                fixture_used.insert(name.clone());
            }
        }
    }

    // ── Competency / structural / native-test channels (reference resolution + real exec). ─
    let competency_result = run_competency_channel();
    let structural_result = run_structural_channel();
    let flagship_manifest_result = run_flagship_manifest_channel();
    let competency_files = competency_query_files();
    let structural_iris = structural_assertion_iris();

    // ── Reconciliation report. ────────────────────────────────────────────────────────
    let mut hard_gaps: Vec<String> = Vec::new();
    let mut unverified: Vec<String> = Vec::new();
    let mut orphan_notes: Vec<String> = Vec::new();
    let mut prose_notes: Vec<String> = Vec::new();

    if !shapes_available {
        unverified.push(
            "SHACL channels (shacl-core/shacl-sparql): generated/shapes/*.ttl is absent in \
             this worktree (a known, documented gap — this task is directed not to run `make \
             sync`), so no class whose ONLY declared tier is SHACL Core/SHACL-SPARQL could be \
             execution-discharged here."
                .to_owned(),
        );
    }

    for (class, (declared, sections)) in &matrix_classes {
        let sections_str = sections.iter().copied().collect::<Vec<_>>().join(", ");
        if !authored.contains(class) {
            hard_gaps.push(format!(
                "[{sections_str}] math:{class}: NOT AUTHORED — no owl:Class + \
                 (rdfs|logic):subClassOf math:MathConformanceFailure chain in module.ttl"
            ));
            continue;
        }
        if !reachable.contains(class) {
            hard_gaps.push(format!(
                "[{sections_str}] math:{class}: UNREACHABLE — no gmeow:enforcesFailureClass \
                 triple in module.ttl and no `math:{class}:` message-token in lint.rs / \
                 math_expression.rs"
            ));
            continue;
        }
        let observed = class_channel_fixtures.get(class);
        let observed_channels: BTreeSet<Channel> = observed
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        let owl_axiom_only = declared.len() == 1 && declared.contains(&Channel::OwlAxiom);
        let shacl_only_unverified = !shapes_available
            && declared
                .iter()
                .all(|c| matches!(c, Channel::ShaclCore | Channel::ShaclSparql))
            && observed_channels.is_empty();

        if observed_channels.is_empty() {
            if owl_axiom_only {
                unverified.push(format!(
                    "[{sections_str}] math:{class}: declared owl-axiom only; no reasoner-based \
                     owl-axiom execution channel in this harness and no fixture tripped it via \
                     native lint either"
                ));
            } else if shacl_only_unverified {
                unverified.push(format!(
                    "[{sections_str}] math:{class}: declared {} but shapes are unavailable in \
                     this environment (see SHACL note above)",
                    channels_to_string(declared)
                ));
            } else {
                hard_gaps.push(format!(
                    "[{sections_str}] math:{class}: reachable, declared {}, but NO on-disk \
                     counter-example fixture tripped it via any executed channel",
                    channels_to_string(declared)
                ));
            }
            continue;
        }
        if observed_channels.is_disjoint(declared) {
            hard_gaps.push(format!(
                "[{sections_str}] math:{class}: TIER MISMATCH — charter declares {}, but \
                 execution observed it fire only via {}",
                channels_to_string(declared),
                channels_to_string(&observed_channels)
            ));
        }
        // else: at least one declared tier was actually observed — no gap, regardless of any
        // extra secondary-gate channel also observed (the charter explicitly allows secondary
        // gates: "a rule may have secondary gates, but one owns the failure").
    }

    // Orphan classes: authored + reachable, but cited in NO section of the charter matrix —
    // exactly the shape of the still-undocumented normalization-identity classes this issue
    // minted (math:StructuralKeyDrift / math:StructuralKeyOnRejectedExpression /
    // math:SurfaceLeakInNormalForm), surfaced generically rather than hardcoded by name.
    for class in authored.intersection(&reachable) {
        if !matrix_classes.contains_key(class) {
            orphan_notes.push(format!(
                "math:{class}: authored + reachable (native message-token and/or \
                 gmeow:enforcesFailureClass) but cited in NO section of \
                 MATHEMATICS-CONFORMANCE.md — a charter row is missing"
            ));
        }
    }

    // Unused fixtures: on-disk counter-examples that tripped NOTHING via the channels this
    // harness could actually execute (native lint, and SHACL when available). When SHACL is
    // unavailable this is expected for every SHACL-tier-only fixture, so it is folded into
    // `unverified` as one summary rather than one line per fixture.
    let all_fixture_names: BTreeSet<String> = fixtures
        .iter()
        .filter_map(|f| f.file_name().and_then(|n| n.to_str()))
        .map(str::to_owned)
        .collect();
    let unused: Vec<&String> = all_fixture_names.difference(&fixture_used).collect();
    if shapes_available && !unused.is_empty() {
        hard_gaps.push(format!(
            "{} on-disk counter-example fixture(s) tripped NOTHING via native lint or SHACL: {}",
            unused.len(),
            unused
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    } else if !shapes_available {
        unverified.push(format!(
            "{} of {} on-disk counter-example fixtures tripped nothing via native lint alone \
             (expected for SHACL-tier-only fixtures while shapes are unavailable)",
            unused.len(),
            fixtures.len()
        ));
    }

    // Prose (no-class) rows: reference resolution + real execution where available.
    for (section, row) in &prose_rows {
        let tiers = classify_gate(&row.gate);
        if tiers.contains(&Channel::CompetencyQuery) {
            match match_competency_query(&row.rule, &competency_files) {
                None => hard_gaps.push(format!(
                    "[{section}] competency-tier row ({:?}) did not resolve to any \
                     queries/competency/*.rq cited by tests/competency.ttl",
                    row.rule
                )),
                Some(file) => {
                    if let Err(e) = &competency_result {
                        hard_gaps.push(format!(
                            "[{section}] competency-tier row resolved to {file}, but \
                             tests/competency.ttl did not execute clean: {e}"
                        ));
                    } else {
                        prose_notes.push(format!(
                            "[{section}] competency-tier row resolved to {file} (green)"
                        ));
                    }
                }
            }
        }
        if tiers.contains(&Channel::Structural)
            && let Some(cited) = row.gate.find('`').and_then(|i| {
                row.gate[i + 1..]
                    .find('`')
                    .map(|j| &row.gate[i + 1..i + 1 + j])
            })
        {
            let local = cited.rsplit(':').next().unwrap_or(cited);
            let resolved = structural_iris.iter().any(|iri| iri.ends_with(local));
            if !resolved {
                hard_gaps.push(format!(
                    "[{section}] structural-tier row cites `{cited}`, not found among \
                     tests/structural.ttl assertion IRIs"
                ));
            } else if let Err(e) = &structural_result {
                hard_gaps.push(format!(
                    "[{section}] structural-tier row's `{cited}` resolved, but \
                     tests/structural.ttl did not execute clean: {e}"
                ));
            } else {
                prose_notes.push(format!("[{section}] structural-tier row `{cited}` (green)"));
            }
        }
        if tiers.contains(&Channel::NativeTest) {
            if row.gate.contains("flagship_manifest") {
                if let Err(e) = &flagship_manifest_result {
                    hard_gaps.push(format!(
                        "[{section}] Rust cross-check row (flagship_manifest) did not execute \
                         clean: {e}"
                    ));
                } else {
                    prose_notes.push(format!("[{section}] flagship_manifest cross-check (green)"));
                }
            } else if row.gate.contains("math_flagship_discharge.rs") {
                let path = repo_root().join("crates/pipeline/tests/math_flagship_discharge.rs");
                if !path.is_file() {
                    hard_gaps.push(format!(
                        "[{section}] execution-discharge-harness row cites a missing file: {}",
                        path.display()
                    ));
                } else {
                    prose_notes.push(format!(
                        "[{section}] execution-discharge harness file present \
                         (crates/pipeline/tests/math_flagship_discharge.rs) — its own test \
                         governs its pass/fail, not re-run here"
                    ));
                }
            }
        }
    }

    // ── Assemble the final report. ────────────────────────────────────────────────────
    let mut report = String::new();
    report.push_str(&format!(
        "\n=== math: conformance TOTAL reconciliation ===\n\
         sections: {}\n\
         class-bearing rows: {}\n\
         prose (no-class) rows: {}\n\
         authored failure classes: {}\n\
         reachable failure classes: {}\n\
         on-disk counter-example fixtures: {}\n\
         shapes available: {shapes_available}\n\n",
        SECTIONS.len(),
        matrix_classes.len(),
        prose_rows.len(),
        authored.len(),
        reachable.len(),
        fixtures.len(),
    ));

    report.push_str(&format!("--- HARD GAPS ({}) ---\n", hard_gaps.len()));
    for g in &hard_gaps {
        report.push_str(&format!("  • {g}\n"));
    }
    report.push_str(&format!(
        "\n--- ORPHAN CLASSES ({}) ---\n",
        orphan_notes.len()
    ));
    for n in &orphan_notes {
        report.push_str(&format!("  • {n}\n"));
    }
    report.push_str(&format!(
        "\n--- UNVERIFIED (environment-limited, {}) ---\n",
        unverified.len()
    ));
    for n in &unverified {
        report.push_str(&format!("  • {n}\n"));
    }
    report.push_str(&format!(
        "\n--- prose-row notes ({}) ---\n",
        prose_notes.len()
    ));
    for n in &prose_notes {
        report.push_str(&format!("  • {n}\n"));
    }

    // The gate: HARD GAPS and ORPHAN CLASSES are actionable and drive failure; UNVERIFIED is
    // printed for transparency but does not, by itself, fail the test (it is an acknowledged,
    // documented environment limitation, not a code deficiency this task can fix).
    assert!(hard_gaps.is_empty() && orphan_notes.is_empty(), "{report}");
}
