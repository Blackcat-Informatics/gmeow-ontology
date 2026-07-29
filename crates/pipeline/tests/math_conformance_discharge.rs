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
//!    `math:<Class>:` literal message-token prefix in the native validator sources the
//!    task names (`crates/validate/src/lint.rs`, `crates/logic/src/math_expression.rs`,
//!    `crates/logic/src/physical/lower.rs`).
//! 3. **Tier-matched** — the charter's declared `Primary gate` column names one or more
//!    channels (`owl-axiom`, `shacl-core`, `shacl-sparql`, `source-lint`, `rust-validator`,
//!    `competency-query`, `projection-test`, plus the two ADDITIONAL channels the charter's
//!    own matrix cites verbatim for its Flagship/Bridges rows, `structural` and
//!    `native-test`/"Rust cross-check" — a documented generalization, not a narrowing of the
//!    task's seven); execution over the on-disk fixture corpus must actually FIRE the class
//!    through one of the declared channels, and the channel a native check is credited under
//!    is the channel ITS OWN CODE actually implements ([`native_check_channel`]) — never the
//!    charter's own declared tier fed back to itself.
//! 4. **Fixture-discharged** — a reachable class has a counter-example fixture on disk
//!    (`tests/counter-examples/*.ttl`) whose execution trips it.
//!
//! Every channel this file drives — the native structural lint, the reasoned-graph gate,
//! the native DL owl-axiom disjointness gate, the native SHACL engine over the generated
//! shape surface, the competency-query executor, the structural-assertion executor, and the
//! flagship manifest cross-check — is EXECUTED for real; none degrades to a non-failing
//! "unverified" placeholder. A missing or malformed `generated/shapes/*.ttl` is a hard
//! failure (run `make sync`), never a weaker verdict, and a class this harness cannot
//! execute through any channel is a hard gap, never a silently-accepted bucket.
//!
//! ## Judgment calls (documented, not hidden)
//!
//! * **The native structural lint (`structural_lint_dataset`) is one Rust function that
//!   implements MULTIPLE charter tiers at once** via the same `math:<Class>:` message-token
//!   convention. [`native_check_channel`] maps every class the native lint can raise to the
//!   ONE channel its OWN emitting function actually implements (read from the function's
//!   body, not inferred from the charter), so a native hit can report a genuine tier
//!   mismatch instead of trivially always matching whatever the charter happens to declare.
//!   A native-lint hit for an unregistered class is a hard panic (register its true
//!   channel), never a silent guess.
//! * **`owl-axiom` tier rows** (the `owl:disjointWith` conflation classes) are executed by
//!   reading the ACTUAL reasoned closure ([`owl_axiom_tripped`]): the module's own
//!   `?carrier owl:disjointWith ?target . ?carrier gmeow:enforcesFailureClass ?class`
//!   triples name every disjoint pair generically, a cheap structural prefilter finds any
//!   fixture individual co-typed both ways, and — only then — the native DL reasoner
//!   ([`gmeow_logic::reason::reason_all`]) decides whether that co-typing is genuinely
//!   forced into `owl:Nothing`, crediting the failure class from the witnessed individual's
//!   own asserted types. No fixture is exempted into an "unverified" bucket.
//! * **`projection-test` tier rows** (the `math:ProjectionRecord` join-requiring native
//!   checks) are additionally proven against REAL, EXECUTED producer output, not only
//!   against hand-authored counter-example/conformance-fixture testimony: see
//!   [`projection_rules_execute_real_producers_and_pass_acceptance_query`], which runs each
//!   producer in `crates/pipeline/tests/support/math_projection_producer.rs` and asserts the
//!   SAME native acceptance checks find its actual output clean.
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
//! * **A class the native sources EMIT but `module.ttl` never AUTHORS is a hard gap**
//!   ([`unauthored_reachable_classes`]), independent of whether the charter cites it: the
//!   per-charter-class loop below only ever looks at classes the charter itself cites, and
//!   the orphan check requires `authored ∩ reachable`, so an unauthored, uncited class is
//!   invisible to both by construction. This reconciliation set closes that hole.

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
    SliceSpec, enforcing_shape_paths, load_scoped_shapes, local_name, minimal_lint_config,
    native_failure_classes, repo_root, shape_class_map, shared_shapes_path,
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
// The `ConformanceSection` descriptor + the (generalized) registry of all 13 sections.
// ─────────────────────────────────────────────────────────────────────────────────────────

/// One `###`-headed gate-matrix section of a conformance charter, generalized over ANY
/// grounding slice's charter (not `math:`-specific machinery, by design —
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

/// One parsed row of a gate-matrix table: its `Primary gate` cell verbatim, every
/// `math:<Class>` local name extracted from its `Failure class` cell (empty when the cell
/// is prose rather than a class — the competency/structural/native-test rows), and the
/// class cell's raw text verbatim (surfaced in hard-gap/orphan diagnostics so a reader sees
/// the charter's own words, not only the derived local name — this field is READ, not
/// dead-code-placated).
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
///
/// Char-boundary-safe: the charter's cells carry multibyte prose (`ℚ⁷`, `μ`, `∂`, `≤`,
/// en-dashes) that can sit immediately after a `math:` mention with no recognized local-name
/// character between them. The old `local.len().max(1)` advance assumed a byte offset of at
/// least 1 was always a char boundary of `after`, which is false the instant the character
/// right after `math:` is multibyte — this advances by that character's OWN UTF-8
/// length instead, and stops rather than indexing past an empty remainder.
fn extract_math_classes(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = cell;
    while let Some(idx) = rest.find("math:") {
        let after = &rest[idx + "math:".len()..];
        if after.is_empty() {
            break;
        }
        let local: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        let advance = if local.is_empty() {
            after.chars().next().map_or(after.len(), char::len_utf8)
        } else {
            local.len()
        };
        if !local.is_empty() {
            out.push(local.clone());
        }
        rest = &after[advance..];
    }
    out
}

/// Parse one `###`-headed section's gate-matrix table into its rows, generalized over ANY
/// heading (not GMN- or `math:`-specific): locate the heading line, read forward to the next
/// `#`-prefixed line (any heading level) or EOF, and extract every markdown table row's
/// middle (`Primary gate`) and LAST (`Failure class`) cells — skipping the header row and the
/// `|---|` separator, exactly mirroring `gmn_conformance_discharge::matrix_gmn_rows`'s
/// boundary and skip logic. The failure-class cell is `cells.last()` (matching this
/// doc's own "last cell" wording, not a hardcoded `cells[2]` — every real row today has
/// exactly 3 cells, so this changes nothing observable, but a defensively-wider row shape
/// is read correctly rather than silently misaligned).
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
        let class_cell = *cells.last().expect("cells has at least 3 entries");
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

/// The execution channel taxonomy: the seven named channels, plus two the charter's
/// OWN matrix cites verbatim for its Flagship/Bridges rows (`structural`, `native-test`) —
/// documented above as a generalization, not a narrowing, of this harness's list.
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

/// `true` iff `marker` occurs in `haystack` at a real token boundary — the character
/// immediately before and after the match, if any, is never ASCII alphanumeric. A bare
/// substring test also matches a marker that is merely a PREFIX of a longer identifier
/// mentioned in the SAME cell (e.g. the marker `"structural"` inside a hypothetical gate
/// cell mentioning `math:structuralKey`/`math:structuralNormalization`), misrouting the row.
fn contains_word(haystack: &str, marker: &str) -> bool {
    haystack.match_indices(marker).any(|(idx, _)| {
        let before_ok = haystack[..idx]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        let after_ok = haystack[idx + marker.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        before_ok && after_ok
    })
}

/// Classify a charter `Primary gate` cell into every channel it names, by token-boundary
/// match (not a bare substring test) against the charter's own stable vocabulary (see
/// the exhaustive enumeration this was built from). A cell naming
/// NONE of these markers is a charter-drift bug (a new gate-kind vocabulary word) and PANICS
/// rather than silently classifying as empty.
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
        if contains_word(gate, marker) {
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

/// The `math:<Class>:` message-token classes recovered from the native validator sources the
/// task names, reusing [`native_failure_classes`]'s exact token-recognition algorithm by
/// feeding it the sources' own NON-COMMENT lines as pseudo-"errors".
///
/// Comment lines (`//`, `///`, `//!`) are excluded: a doc comment routinely mentions a
/// `logic:Constraint`'s own identifier (e.g. `math:StringOnlyComputableExpressionConstraint:`
/// — the SHACL-SPARQL derivation source, not itself a `math:MathConformanceFailure`
/// subclass) in the SAME `<prefix>:<Class>:` shape a genuine runtime message token has; only
/// a non-comment line can be part of an actual `push_error`/`format!` message literal, so a
/// class genuinely reachable through native code always has a non-comment occurrence too.
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
        let lines: Vec<String> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .map(str::to_owned)
            .collect();
        out.extend(native_failure_classes(&lines, "math"));
    }
    out
}

/// The full `https://blackcatinformatics.ca/math/<Class>` failure-class IRI constants
/// `crates/logic/src/physical/lower.rs`'s own `mod failure_class` block declares — the
/// SECOND reachability signal genuine native code carries, alongside the message-token scan
/// above.
///
/// Needed because `check_structural_key_on_rejected_expression`'s embedded second token
/// (`format!("math:{class_local}: {err}")`) is RUNTIME-interpolated: the literal SOURCE text
/// is never `math:CyclicExpressionGraph:` — only the value `class_local` resolves to at
/// runtime is — so the message-token scan alone cannot see it. `mod failure_class`'s own doc
/// comment confirms `math:CyclicExpressionGraph` / `math:ExpressionDepthExceeded` are
/// "reachable ONLY through this Rust decision". This scan is scoped to EXACTLY that module
/// block (never the whole file, which also carries ordinary domain-class IRIs unrelated to
/// any failure class — ApplicationExpression, ArgumentSlot, …) so it cannot manufacture a
/// false "reachable" claim for some unrelated class reference elsewhere in the lowering
/// engine; every constant in that block IS, by the block's own documented invariant, one of
/// [`crate::physical::lower::MathLoweringError::failure_class`]'s eight decided classes.
fn native_source_iri_constant_classes() -> BTreeSet<String> {
    let path = repo_root().join("crates/logic/src/physical/lower.rs");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read native validator source {}: {e}", path.display()));
    let start = text
        .find("mod failure_class {")
        .expect("physical/lower.rs declares mod failure_class");
    let body = &text[start..];
    let end = body
        .find("\n}\n")
        .expect("mod failure_class has a closing brace");
    let block = &body[..end];
    let mut out = BTreeSet::new();
    for line in block.lines() {
        if let Some(idx) = line.find(MATH_NS) {
            let rest = &line[idx + MATH_NS.len()..];
            let local: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if local.starts_with(|c: char| c.is_ascii_uppercase()) {
                out.insert(local);
            }
        }
    }
    out
}

/// The reconciliation set the previous harness omitted (the single most important fix in
/// this file): a class the native sources EMIT — `reachable` — but `module.ttl` never
/// AUTHORS as a proper `owl:Class` + `math:MathConformanceFailure` subclass is a PHANTOM
/// failure class, and it is invisible to two independent checks by construction:
///
/// * the per-charter-class loop only ever iterates classes the charter itself CITES, so an
///   uncited phantom is never looked at;
/// * the orphan check requires `authored ∩ reachable`, which excludes an unauthored class
///   by definition.
///
/// A `math:<Class>` token emitted by native Rust with no `module.ttl` declaration is exactly
/// this shape (proved experimentally by reverting `math:MalformedStructuralKey`'s
/// declaration while leaving the Rust emitter in place — the old harness returned PASS).
/// `cited` classes are excluded here because the main per-class loop already reports THEM as
/// "NOT AUTHORED" — this function targets only the classes invisible to every other check.
fn unauthored_reachable_classes(
    reachable: &BTreeSet<String>,
    authored: &BTreeSet<String>,
    cited: &BTreeSet<String>,
) -> Vec<String> {
    reachable
        .iter()
        .filter(|class| !authored.contains(*class) && !cited.contains(*class))
        .map(|class| {
            format!(
                "math:{class}: EMITTED BY NATIVE RUST (a `math:{class}:` message-token or \
                 gmeow:enforcesFailureClass target) but NOT AUTHORED as an owl:Class + \
                 (rdfs|logic):subClassOf math:MathConformanceFailure chain in module.ttl, and \
                 cited by NO section of the charter — a phantom failure class invisible to \
                 both the charter-class loop (uncited) and the orphan loop (requires authored \
                 ∩ reachable)"
            )
        })
        .collect()
}

#[cfg(test)]
mod unauthored_reachable_regression {
    use super::*;

    /// Proves [`unauthored_reachable_classes`] catches the exact hole this harness proved
    /// experimentally. Reverting the `!authored.contains` / `!cited.contains` guard back to
    /// the pre-fix shape (only ever checking `authored ∩ reachable`, i.e. deleting this
    /// function's body and inlining the old orphan-only logic) makes the first assertion
    /// below fail — this is the regression test this guard requires: it fails if the
    /// reconciliation is removed.
    #[test]
    fn catches_a_reachable_class_native_rust_emits_but_module_ttl_never_authors() {
        let reachable: BTreeSet<String> =
            ["MalformedStructuralKey".to_owned()].into_iter().collect();
        let authored: BTreeSet<String> = BTreeSet::new();
        let cited: BTreeSet<String> = BTreeSet::new();
        let gaps = unauthored_reachable_classes(&reachable, &authored, &cited);
        assert_eq!(
            gaps.len(),
            1,
            "an unauthored, uncited, but native-emitted class must be exactly one hard gap"
        );
        assert!(
            gaps[0].contains("MalformedStructuralKey"),
            "the hard gap must name the phantom class: {gaps:?}"
        );
    }

    /// The LOAD-BEARING half: the real native-source scan actually populates `reachable`.
    ///
    /// The set-difference check above can stay green while the wiring beneath it rots — if
    /// `native_source_message_classes` stopped finding anything (a renamed file, a changed
    /// message convention), `reachable` would be empty, no class could ever be flagged, and the
    /// phantom hole would silently reopen with every assertion still passing. That is exactly
    /// the shape of test this harness exists to distrust, so pin the scan itself: it runs over
    /// the REAL source files and must recover the classes native Rust actually emits.
    #[test]
    fn the_real_native_source_scan_recovers_the_classes_rust_actually_emits() {
        // Compose BOTH reachability signals exactly as the harness does: the message-token
        // scan, plus the failure-class IRI constants. The second exists because the typed
        // error algebra interpolates its class token at RUNTIME, so classes reachable only
        // through it never appear as literal source text — checking one half alone would
        // report a class as unreachable that production emits.
        let mut reachable = native_source_message_classes();
        reachable.extend(native_source_iri_constant_classes());
        assert!(
            reachable.len() > 5,
            "the native-source scan must recover the emitted classes; got {reachable:?}"
        );
        // Each of these is emitted from a DIFFERENT source file the scan must cover, so a
        // dropped path shows up here rather than as a silently smaller population.
        for expected in [
            "StringOnlyComputableExpression", // crates/validate/src/lint.rs
            "MalformedStructuralKey",         // crates/logic/src/math_expression.rs
            "CyclicExpressionGraph",          // crates/logic/src/physical/lower.rs
        ] {
            assert!(
                reachable.contains(expected),
                "the scan must recover {expected} from its own source file; got {reachable:?}"
            );
        }
    }

    /// End to end over the REAL populations: a class Rust emits but the ontology does not
    /// author is a hard gap. Uses the live scan and the live authored set rather than
    /// hand-built ones, so it exercises the reconciliation as the harness actually runs it.
    #[test]
    fn a_real_emitted_class_missing_from_the_authored_set_is_a_hard_gap() {
        let mut reachable = native_source_message_classes();
        reachable.extend(native_source_iri_constant_classes());
        let victim = "MalformedStructuralKey".to_owned();
        assert!(
            reachable.contains(&victim),
            "precondition: the scan recovers the class this test withholds from `authored`"
        );
        // Every reachable class IS authored on a healthy tree; withhold exactly one and the
        // reconciliation must name it.
        let authored: BTreeSet<String> = reachable
            .iter()
            .filter(|c| **c != victim)
            .cloned()
            .collect();
        let gaps = unauthored_reachable_classes(&reachable, &authored, &BTreeSet::new());
        assert!(
            gaps.iter().any(|g| g.contains(&victim)),
            "withholding {victim} from the authored set must produce a hard gap naming it: {gaps:?}"
        );
    }

    /// An authored class is never flagged even though reachable — this check targets ONLY
    /// the unauthored case, not a general "reachable" sweep.
    #[test]
    fn does_not_flag_an_authored_reachable_class() {
        let both: BTreeSet<String> = ["KnownClass".to_owned()].into_iter().collect();
        assert!(unauthored_reachable_classes(&both, &both, &BTreeSet::new()).is_empty());
    }

    /// A cited-but-unauthored class is left to the MAIN per-charter-class loop (which
    /// already reports it as "NOT AUTHORED") — citing it suppresses this specific,
    /// uncited-phantom check so the class is never double-reported by both.
    #[test]
    fn does_not_double_report_a_cited_unauthored_class() {
        let reachable: BTreeSet<String> = ["CitedClass".to_owned()].into_iter().collect();
        let cited: BTreeSet<String> = ["CitedClass".to_owned()].into_iter().collect();
        assert!(unauthored_reachable_classes(&reachable, &BTreeSet::new(), &cited).is_empty());
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Execution discharge: the native lint + reasoned-graph channels (no `generated/`
// dependency). each takes the ALREADY-PARSED, already-unioned per-fixture dataset —
// the caller parses and unions each fixture exactly ONCE and shares it across every channel.
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

/// The channel each native-lint check function in `crates/validate/src/lint.rs` ACTUALLY
/// implements, keyed by the `math:<Class>` local name it raises — read from what each
/// function DOES (the doctrine's three kinds: "a Rust source-level check over the slice TTL
/// before folding", "a native check for obligations that genuinely need specialized
/// execution", "an acceptance query over a generated lowering"), never inferred from the
/// charter's OWN declared tier. This is the EXHAUSTIVE set of `math:` classes
/// `structural_lint_dataset` can raise (`check_math_ingest_invariants`,
/// `check_math_expression_invariants`, `check_math_core_invariants`,
/// `check_math_probability_invariants`, `check_math_projection_invariants`); a native-lint
/// hit for anything else is a charter-drift bug this registry has not kept up with, and
/// [`native_lint_tripped`]'s caller panics rather than silently guessing a channel.
fn native_check_channel(class: &str) -> Option<Channel> {
    use Channel::{ProjectionTest, RustValidator, SourceLint};
    match class {
        // source-lint: "a Rust source-level check over the slice TTL before folding" —
        // `check_string_only_computable_expression`'s own doc comment names itself "the
        // source-lint half of math:StringOnlyComputableExpression", the doctrine's own
        // worked example of this tier.
        "StringOnlyComputableExpression" => Some(SourceLint),

        // Rust validator: "a native check for obligations that genuinely need specialized
        // execution" — arithmetic (outcome-mass summation, positivity/dimension
        // constraints), dependency-graph completeness, and cross-node joins with no SHACL
        // target shape.
        "UnliftableIngest" => Some(RustValidator),
        "UngroundedResultClaim" => Some(RustValidator),
        "ProbabilityOutOfBounds" => Some(RustValidator),
        "DistributionParameterConstraint" => Some(RustValidator),
        "MissingProbabilityModelLowering" => Some(RustValidator),
        "IncompleteDependencyModel" => Some(RustValidator),
        "ExactPreservationViolated" => Some(RustValidator),

        // projection test: "an acceptance query over a generated lowering that fails if
        // loss is unrecorded or preservation is mis-declared" — the join-requiring native
        // checks over `math:ProjectionRecord` (`check_math_projection_invariants`), each
        // ALSO proven against real producer output by
        // `projection_rules_execute_real_producers_and_pass_acceptance_query`.
        "MissingPreservationKind" => Some(ProjectionTest),
        "UndeclaredUnsupportedConstruct" => Some(ProjectionTest),
        "UnrecordedProjectionLoss" => Some(ProjectionTest),
        "ProjectionConfidenceAsProbability" => Some(ProjectionTest),
        "ProjectionDroppedParameterization" => Some(ProjectionTest),

        _ => None,
    }
}

/// The `math:` classes the native structural lint raises over the already-merged `ds`.
fn native_lint_tripped(ds: &RdfDataset) -> BTreeSet<String> {
    let report = structural_lint_dataset(ds, &minimal_lint_config());
    native_failure_classes(&report.errors(), "math")
        .into_iter()
        .collect()
}

/// The `math:` classes the two REASONED-GRAPH native gates raise over the already-merged
/// `ds` — `gmeow_logic::math_expression::check_math_expression_findings`
/// (`math:StructuralKeyDrift`, `math:SurfaceLeakInNormalForm`,
/// `math:StructuralKeyOnRejectedExpression`, and — via the rejection's own embedded
/// `math:<LocalName>:` token when a rejected root also carries an authored
/// `math:structuralKey` — `math:CyclicExpressionGraph`/`math:ExpressionDepthExceeded`/etc.)
/// and the reasoner-derived dimensional-homogeneity gate
/// (`gmeow_logic::reason::math_gate::dimension_gate_markers`,
/// `math:DimensionalInhomogeneity`) — neither of which the pre-fold structural lint or the
/// generated-SHACL surface can reach: both are genuine computations over the merged dataset
/// itself (no actual DL closure is needed for either gate's own asserted-fact reading, so
/// `dimension_gate_markers` is called with an empty derived-edge slice). Every class this
/// function can raise is declared `Rust validator` by the charter, so it is credited there
/// directly by the caller (no tier-selection ambiguity, unlike the native-lint registry
/// above which spans three tiers).
///
/// # Panics
///
/// `dimension_gate_markers`'s own contract (see its doc comment) is that an `Err` is a
/// genuine internal-invariant failure (non-stratifiable rules, or a declined native forward
/// chase) — never a missing-fixture condition. this propagates that as a hard panic
/// instead of silently dropping it via `if let Ok(...)`, which would misattribute a real
/// engine failure as "the gate found nothing".
fn reasoned_tripped(ds: &RdfDataset) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();

    let expr_messages: Vec<String> =
        gmeow_logic::math_expression::check_math_expression_findings(ds)
            .into_iter()
            .map(|f| f.message)
            .collect();
    classes.extend(native_failure_classes(&expr_messages, "math"));

    match gmeow_logic::reason::math_gate::dimension_gate_markers(ds, &[]) {
        Ok(markers) => {
            for (_, class_iri) in markers {
                classes.insert(local_name(&class_iri));
            }
        }
        Err(e) => panic!(
            "math dimension-gate chase failed as a genuine internal-invariant violation \
             (non-stratifiable rules or a declined native forward chase per its own \
             documented contract) — never a missing-fixture condition, so this is a hard \
             failure, not something the completeness gate may drop: {e}"
        ),
    }
    classes
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Execution discharge: the OWL-axiom disjointness channel. Reads the module's own
// `owl:disjointWith` + `gmeow:enforcesFailureClass` pairing generically, then consults the
// ACTUAL reasoned closure for the `owl:Nothing` witness the axiom promises — never a
// structural co-typing heuristic alone, and never an "unverified" bucket when no fixture
// happens to trip it (a class with no execution channel built is a hard gap instead, per
// this harness's own contract).
// ─────────────────────────────────────────────────────────────────────────────────────────

/// One authored `owl:disjointWith` pair the module ties to a failure class via
/// `gmeow:enforcesFailureClass` — e.g. `(math:InferenceRun, gmeow:Observation,
/// ProcessObservationConflation)`. `carrier_iri`/`target_iri` stay FULL IRIs (needed to
/// build the per-fixture co-typing query); `failure_class` is the local name.
struct OwlAxiomCarrier {
    carrier_iri: String,
    target_iri: String,
    failure_class: String,
}

/// Every `?carrier owl:disjointWith ?target . ?carrier gmeow:enforcesFailureClass ?class`
/// triple pair in `module.ttl` — generic over however many disjoint pairs the module
/// authors this way today or later, with no per-class hardcoding.
fn owl_axiom_carriers(ds: &Arc<RdfDataset>) -> Vec<OwlAxiomCarrier> {
    match gmeow_slicetest::native_query::query(
        ds,
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> \
         PREFIX owl: <http://www.w3.org/2002/07/owl#> \
         SELECT ?carrier ?target ?class WHERE { \
           ?carrier owl:disjointWith ?target . \
           ?carrier gmeow:enforcesFailureClass ?class \
         }",
    )
    .expect("query runs")
    {
        SparqlResult::Solutions {
            variables, rows, ..
        } => {
            let ci = variables
                .iter()
                .position(|v| v == "carrier")
                .expect("projects ?carrier");
            let ti = variables
                .iter()
                .position(|v| v == "target")
                .expect("projects ?target");
            let ki = variables
                .iter()
                .position(|v| v == "class")
                .expect("projects ?class");
            rows.iter()
                .filter_map(|row| {
                    let carrier = row.get(ci).cloned().flatten()?;
                    let target = row.get(ti).cloned().flatten()?;
                    let class = row.get(ki).cloned().flatten()?;
                    match (carrier, target, class) {
                        (
                            TermValue::Iri(carrier_iri),
                            TermValue::Iri(target_iri),
                            TermValue::Iri(class),
                        ) => Some(OwlAxiomCarrier {
                            carrier_iri,
                            target_iri,
                            failure_class: local_name(&class),
                        }),
                        _ => None,
                    }
                })
                .collect()
        }
        other => panic!("expected SELECT solutions, got {other:?}"),
    }
}

/// Cheap, reasoner-free prefilter: subjects in `ds` co-typed BOTH a carrier's `carrier_iri`
/// AND its `target_iri` — subject IRI -> the failure classes such co-typing implicates. A
/// targeted per-carrier query (bounded by the two IRIs' own extents) rather than a full
/// `?s a ?class` dump, so this stays cheap across every on-disk fixture even though only a
/// handful ever populate it.
fn owl_axiom_candidates(
    ds: &Arc<RdfDataset>,
    carriers: &[OwlAxiomCarrier],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for carrier in carriers {
        let query = format!(
            "SELECT ?s WHERE {{ ?s a <{}> . ?s a <{}> . }}",
            carrier.carrier_iri, carrier.target_iri
        );
        let subjects = match gmeow_slicetest::native_query::query(ds, &query).expect("query runs") {
            SparqlResult::Solutions {
                variables, rows, ..
            } => {
                let si = variables
                    .iter()
                    .position(|v| v == "s")
                    .expect("projects ?s");
                rows.iter()
                    .filter_map(|row| match row.get(si).cloned().flatten() {
                        Some(TermValue::Iri(s)) => Some(s),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            }
            other => panic!("expected SELECT solutions, got {other:?}"),
        };
        for subject in subjects {
            out.entry(subject)
                .or_default()
                .insert(carrier.failure_class.clone());
        }
    }
    out
}

/// The `math:` classes the OWL-axiom disjointness channel raises over the already-merged
/// `ds`: a cheap structural prefilter finds candidate co-typed individuals, and — only when
/// at least one exists — the REAL native DL reasoner decides whether the co-typing is
/// genuinely forced into `owl:Nothing`, crediting the failure class from the witnessed
/// individual's own candidate types (never crediting a class merely because SOME
/// inconsistency exists somewhere in the graph).
fn owl_axiom_tripped(ds: &Arc<RdfDataset>, carriers: &[OwlAxiomCarrier]) -> BTreeSet<String> {
    let candidates = owl_axiom_candidates(ds, carriers);
    if candidates.is_empty() {
        return BTreeSet::new();
    }
    let result = gmeow_logic::reason::reason_all(ds)
        .unwrap_or_else(|e| panic!("owl-axiom channel: native DL reasoning failed: {e}"));
    let mut classes = BTreeSet::new();
    for witness in &result.provenance.contradiction_witnesses {
        let individual = witness
            .individual
            .trim_start_matches('<')
            .trim_end_matches('>');
        if let Some(failures) = candidates.get(individual) {
            classes.extend(failures.iter().cloned());
        }
    }
    classes
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// Execution discharge: the SHACL channels (HARD-REQUIRED, never a best-effort degrade).
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

/// Silence the default panic-hook stderr spew while recovering the panic message, for a
/// caller that turns an assertion-style helper's panic into a [`Diag`] it can attribute to
/// the RIGHT prose-row hard-gap message (used by [`run_flagship_manifest_channel`] only —
/// NOT by shape loading, which hard-fails directly).
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
/// [`load_math_shapes`].
type MathShapeSurface = (
    Shapes,
    std::collections::HashMap<String, String>,
    std::collections::HashMap<String, Channel>,
);

/// Load the scoped `math:` shape surface + shape→class map + shape→tier map.
///
/// Both an ABSENT `generated/shapes/*.ttl` and a MALFORMED one are HARD FAILURES — the
/// repo doctrine is that a missing generated artifact means recompute-or-hard-fail, never a
/// weaker verdict ("a missing dependency or implementation is a HARD FAIL, never permission
/// to use a weaker parser, omit output, retain stale bytes, or otherwise degrade
/// semantics"), so this harness carries NO "unverified" bucket for either case. Absence
/// fails with an explicit "run `make sync`" instruction; malformation propagates
/// `load_scoped_shapes`'s own parse-error `.expect(...)` panic verbatim (Rust's `.expect`
/// renders the wrapped error's `Debug` output alongside the message, so the recovered
/// diagnostic is surfaced, not swallowed).
///
/// # Panics
///
/// Panics if any required shape path is missing, or if the shape surface fails to parse or
/// scope cleanly.
fn load_math_shapes() -> MathShapeSurface {
    let spec = math_spec();
    for path in enforcing_shape_paths(&spec) {
        assert!(
            path.is_file(),
            "required generated shape surface is missing: {} — run `make sync` to \
             regenerate `generated/shapes/*.ttl` before running this harness; a missing \
             generated artifact is a hard failure here, never a silently-accepted \
             \"unverified\" channel",
            path.display()
        );
    }
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
// the projection-test channel's real-producer acceptance query, over ACTUAL executed
// producer output — not only over hand-authored counter-example/conformance-fixture
// testimony. A SEPARATE test from the fixture-driven reconciliation below: this one proves
// each producer named in the charter's "### Projection rules" section rows actually runs
// and its actual output passes the SAME native acceptance checks.
// ─────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn projection_rules_execute_real_producers_and_pass_acceptance_query() {
    let conformance_modules = gmeow_slicetest::native_query::dataset_from_files(
        &gmeow_slicetest::paths::conformance_module_files(&math_root()),
    )
    .expect("math conformance module trio parses");

    const PROJECTION_CLASSES: &[&str] = &[
        "MissingPreservationKind",
        "UndeclaredUnsupportedConstruct",
        "UnrecordedProjectionLoss",
        "ProjectionConfidenceAsProbability",
        "ProjectionDroppedParameterization",
    ];

    type Producer = (&'static str, fn() -> String);
    let producers: &[Producer] = &[
        (
            "produce_expression_annotation_projection",
            support::math_projection_producer::produce_expression_annotation_projection,
        ),
        (
            "produce_distribution_scipy_projection",
            support::math_projection_producer::produce_distribution_scipy_projection,
        ),
        (
            "produce_confidence_probability_projection",
            support::math_projection_producer::produce_confidence_probability_projection,
        ),
    ];

    for (name, produce) in producers {
        let turtle = produce();
        let producer_ds = gmeow_slicetest::native_query::dataset_from_turtle(&turtle)
            .unwrap_or_else(|e| panic!("producer {name} emitted unparsable Turtle: {e}"));
        let ds =
            gmeow_slicetest::native_query::union(&[Arc::clone(&conformance_modules), producer_ds]);
        let tripped = native_lint_tripped(&ds);
        for class in PROJECTION_CLASSES {
            assert!(
                !tripped.contains(*class),
                "producer {name}'s REAL executed output must pass the math:{class} \
                 acceptance query cleanly (the projection-test channel proven over actual \
                 output, not fixture testimony), but it raised: {tripped:?}\n{turtle}"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────
// The TOTAL reconciliation test.
// ─────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn total_math_conformance_matrix_is_discharged() {
    let md = std::fs::read_to_string(charter_path()).expect("MATHEMATICS-CONFORMANCE.md readable");

    // ── Parse every section, generically. ────────────────────────────────────────────
    // class -> (declared channels, citing sections, raw class-cell texts observed for it)
    type MatrixClassEntry = (BTreeSet<Channel>, BTreeSet<&'static str>, BTreeSet<String>);
    let mut matrix_classes: BTreeMap<String, MatrixClassEntry> = BTreeMap::new();
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
                    .or_insert_with(|| (BTreeSet::new(), BTreeSet::new(), BTreeSet::new()));
                entry.0.extend(tiers.iter().copied());
                entry.1.insert(section.heading);
                entry.2.insert(row.raw_class_cell.clone());
            }
        }
    }

    // ── module.ttl-derived checks (sets 1 and 2) — no `generated/` dependency. ────────
    let module_ds = math_module_dataset();
    let authored = authored_conformance_failure_classes(&module_ds);
    let enforced = enforced_in_module(&module_ds);
    let mut native_source = native_source_message_classes();
    native_source.extend(native_source_iri_constant_classes());
    let reachable: BTreeSet<String> = enforced.union(&native_source).cloned().collect();
    let owl_carriers = owl_axiom_carriers(&module_ds);

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

    // ── The generated SHACL shape surface — HARD-REQUIRED, loaded once. ──────────
    let (shapes, shape_class, shape_tier) = load_math_shapes();

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

        // parse + union this fixture with the conformance modules EXACTLY ONCE,
        // sharing the result across every channel below (native lint, reasoned-graph,
        // owl-axiom, SHACL) instead of re-parsing/re-unioning per channel.
        let fixture_ds = gmeow_slicetest::native_query::dataset_from_file(fixture)
            .unwrap_or_else(|e| panic!("parse counter-example {}: {e}", fixture.display()));
        let ds =
            gmeow_slicetest::native_query::union(&[Arc::clone(&conformance_modules), fixture_ds]);

        // The native structural-lint channel — credited under the channel its OWN
        // emitting function actually implements, never the charter's declared tier fed
        // back to itself.
        for class in native_lint_tripped(&ds) {
            let credited = native_check_channel(&class).unwrap_or_else(|| {
                panic!(
                    "math:{class}: native lint raised this class over {} but no entry \
                     exists in native_check_channel's explicit tier registry — \
                     register its true channel by reading what the emitting check \
                     function actually does",
                    fixture.display()
                )
            });
            class_channel_fixtures
                .entry(class)
                .or_default()
                .entry(credited)
                .or_default()
                .insert(name.clone());
            fixture_used.insert(name.clone());
        }

        // The reasoned-graph channel: every class it can raise is declared `rust-validator`
        // in the charter, so it is credited there directly (no tier-selection ambiguity).
        for class in reasoned_tripped(&ds) {
            class_channel_fixtures
                .entry(class)
                .or_default()
                .entry(Channel::RustValidator)
                .or_default()
                .insert(name.clone());
            fixture_used.insert(name.clone());
        }

        // The OWL-axiom disjointness channel: reads the reasoned closure's verdict.
        for class in owl_axiom_tripped(&ds, &owl_carriers) {
            class_channel_fixtures
                .entry(class)
                .or_default()
                .entry(Channel::OwlAxiom)
                .or_default()
                .insert(name.clone());
            fixture_used.insert(name.clone());
        }

        // The SHACL channels (always executed, never best-effort).
        for (class, channel) in shacl_tripped(&ds, &shapes, &shape_class, &shape_tier) {
            class_channel_fixtures
                .entry(class)
                .or_default()
                .entry(channel)
                .or_default()
                .insert(name.clone());
            fixture_used.insert(name.clone());
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
    // Retained as a defensive gate, not a leniency valve: every push site that used
    // to feed this bucket (absent/malformed shapes, an unbuilt owl-axiom channel) is gone,
    // so this stays empty by construction — but the final assert still fails loudly if a
    // future regression ever pushes to it, rather than silently accepting a weaker verdict.
    let unverified: Vec<String> = Vec::new();
    let mut orphan_notes: Vec<String> = Vec::new();
    let mut prose_notes: Vec<String> = Vec::new();

    for (class, (declared, sections, raw_cells)) in &matrix_classes {
        let sections_str = sections.iter().copied().collect::<Vec<_>>().join(", ");
        let raw_cells_str = raw_cells.iter().cloned().collect::<Vec<_>>().join(" | ");
        if !authored.contains(class) {
            hard_gaps.push(format!(
                "[{sections_str}] math:{class}: NOT AUTHORED — no owl:Class + \
                 (rdfs|logic):subClassOf math:MathConformanceFailure chain in module.ttl \
                 (charter cell: {raw_cells_str})"
            ));
            continue;
        }
        if !reachable.contains(class) {
            hard_gaps.push(format!(
                "[{sections_str}] math:{class}: UNREACHABLE — no gmeow:enforcesFailureClass \
                 triple in module.ttl and no `math:{class}:` message-token in the native \
                 validator sources (charter cell: {raw_cells_str})"
            ));
            continue;
        }
        let observed = class_channel_fixtures.get(class);
        let observed_channels: BTreeSet<Channel> = observed
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();

        if observed_channels.is_empty() {
            hard_gaps.push(format!(
                "[{sections_str}] math:{class}: reachable, declared {}, but NO on-disk \
                 counter-example fixture tripped it via any executed channel (charter cell: \
                 {raw_cells_str})",
                channels_to_string(declared)
            ));
            continue;
        }
        if observed_channels.is_disjoint(declared) {
            hard_gaps.push(format!(
                "[{sections_str}] math:{class}: TIER MISMATCH — charter declares {}, but \
                 execution observed it fire only via {} (charter cell: {raw_cells_str})",
                channels_to_string(declared),
                channels_to_string(&observed_channels)
            ));
        }
        // else: at least one declared tier was actually observed — no gap, regardless of any
        // extra secondary-gate channel also observed (the charter explicitly allows secondary
        // gates: "a rule may have secondary gates, but one owns the failure").
    }

    // The unauthored-reachable-phantom sweep (the single most important reconciliation —
    // see the module doc and `unauthored_reachable_classes`'s own doc comment).
    let cited: BTreeSet<String> = matrix_classes.keys().cloned().collect();
    hard_gaps.extend(unauthored_reachable_classes(&reachable, &authored, &cited));

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

    // Unused fixtures: on-disk counter-examples that tripped NOTHING via any executed
    // channel (native lint, reasoned-graph, owl-axiom, or SHACL — all four always run).
    let all_fixture_names: BTreeSet<String> = fixtures
        .iter()
        .filter_map(|f| f.file_name().and_then(|n| n.to_str()))
        .map(str::to_owned)
        .collect();
    let unused: Vec<&String> = all_fixture_names.difference(&fixture_used).collect();
    if !unused.is_empty() {
        hard_gaps.push(format!(
            "{} on-disk counter-example fixture(s) tripped NOTHING via any executed channel: {}",
            unused.len(),
            unused
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
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
         on-disk counter-example fixtures: {}\n\n",
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
        "\n--- UNVERIFIED (must always be empty — defensive gate only, {}) ---\n",
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

    // printed UNCONDITIONALLY — a passing run's report is exactly as important as a
    // failing one's, and the previous version dropped it silently on the passing path
    // (only the `assert!` panic payload carried it, invisible unless the test failed).
    println!("{report}");

    // The gate: HARD GAPS, ORPHAN CLASSES, and a non-empty UNVERIFIED bucket are all
    // actionable and drive failure. There is no "environment-limited" carve-out:
    // every channel above is executed for real, so a class this harness cannot discharge is
    // always a hard gap, never a silently-accepted weaker verdict.
    assert!(
        hard_gaps.is_empty() && orphan_notes.is_empty() && unverified.is_empty(),
        "{report}"
    );
}
