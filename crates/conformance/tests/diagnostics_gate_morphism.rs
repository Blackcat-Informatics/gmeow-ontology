// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The diagnostics gate-morphism agreement lane.
//!
//! This proves — by the native reasoner, over the ACTUAL authored ontology — that
//! the RDF-derived gate policy equals the single Rust `gate()` morphism. It is the
//! dogfooding centerpiece of the grading substrate: the gate that decides fatality
//! lives once in Rust (`gmeow_errors::grade::gate`), and once in the ontology as the
//! `logic:ruleGateFatalVerdict` derivation reading the truth-axis triple
//! (`gmeow:findingSeverity`, the `gmeow:categoryBlocking` projection of
//! `gmeow:findingCategory`, `gmeow:findingStandpoint`). If the two ever disagree,
//! the ontology's policy has drifted from the engine's — and this lane is red.
//!
//! ## The contract
//!
//! For **every** grade in the finite bilattice (the 96 = 4 severities × 8
//! categories × 3 standpoints `Grade` combinations `grade.rs` tests exhaustively):
//!
//! 1. the authored `gmeow:categoryBlocking` wiring is read from the diagnostics
//!    slice and asserted equal to Rust `FindingCategory::blocking()` — so the
//!    category→Blocking projection the reasoner reads is provably the same map
//!    `gate()` uses;
//! 2. each grade is encoded as an RDF finding pointing at the seeded severity /
//!    category / standpoint individuals;
//! 3. the native chase runs the AUTHORED `logic:ruleGateFatalVerdict` rule (loaded
//!    from `slices/grounding/logic/module.ttl`, never re-typed) over the encoded
//!    grades + authored wiring;
//! 4. the set of grades the reasoner derives `gmeow:findingGateVerdict
//!    gmeow:gateFatal` for is asserted equal — exactly, both directions — to the set
//!    of grades for which Rust `gate(grade) == GateVerdict::Fatal`.
//!
//! The rule and the category→Blocking map are read from the slice `.ttl` files and
//! the Rust `blocking()`, never fudged, so this is a real derived-vs-Rust
//! set-equality. The two never-gate theorems (advisory-standpoint and
//! permitted-epistemic-conflict grades never gate) are asserted as named
//! sub-theorems of the same reasoner-derived set.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::grade::{
    Blocking, FindingCategory, GateVerdict, Grade, Severity, Standpoint, gate,
};
use gmeow_logic::reason::reason_program;
use gmeow_logic_compile::frontend::parse_logic_dataset;
use gmeow_logic_compile::ir::{LogicProgram, LogicRule};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    NativeRdfFormat, RdfDatasetBuilder, RdfQuad, RdfTerm, SparqlEngine, SparqlRequest,
    SparqlResult, TermValue, dataset_from_bytes,
};

use gmeow_ns::GMEOW_NS;
use gmeow_ns::LOGIC_NS;
/// The derived predicate and its Fatal value — the head of `logic:ruleGateFatalVerdict`.
const FINDING_GATE_VERDICT: &str = "https://blackcatinformatics.ca/gmeow/findingGateVerdict";
const GATE_FATAL: &str = "https://blackcatinformatics.ca/gmeow/gateFatal";
const CATEGORY_BLOCKING: &str = "https://blackcatinformatics.ca/gmeow/categoryBlocking";
/// The single world (named graph) the encoded grades + wiring live in. The chase
/// reads facts out of named-graph worlds; a plain default-graph fact is invisible
/// to it by design, so the whole EDB must be world-scoped.
const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics-gate-conformance";

/// The seeded `gmeow:severity*` individual IRI a [`Severity`] projects to.
fn severity_iri(s: Severity) -> String {
    let local = match s {
        Severity::Info => "severityInfo",
        Severity::Note => "severityNote",
        Severity::Warning => "severityWarning",
        Severity::Error => "severityError",
    };
    format!("{GMEOW_NS}{local}")
}

/// The seeded `logic:Finding*` category individual IRI a [`FindingCategory`] projects to.
fn category_iri(c: FindingCategory) -> String {
    let local = match c {
        FindingCategory::DataShapeViolation => "FindingDataShapeViolation",
        FindingCategory::ModelingDisciplineViolation => "FindingModelingDisciplineViolation",
        FindingCategory::ContradictionWitness => "FindingContradictionWitness",
        FindingCategory::PermittedEpistemicConflict => "FindingPermittedEpistemicConflict",
        FindingCategory::UnsupportedSemanticFeature => "FindingUnsupportedSemanticFeature",
        FindingCategory::IncompleteCheck => "FindingIncompleteCheck",
        FindingCategory::ProjectionLoss => "FindingProjectionLoss",
        FindingCategory::PolicyWarning => "FindingPolicyWarning",
        FindingCategory::Corroboration => "FindingCorroboration",
        FindingCategory::Transient => "FindingTransientChatter",
    };
    format!("{LOGIC_NS}{local}")
}

/// The seeded `gmeow:standpoint*` individual IRI a [`Standpoint`] projects to.
fn standpoint_iri(p: Standpoint) -> String {
    let local = match p {
        Standpoint::Advisory => "standpointAdvisory",
        Standpoint::Perspectival => "standpointPerspectival",
        Standpoint::Binding => "standpointBinding",
    };
    format!("{GMEOW_NS}{local}")
}

/// The seeded `gmeow:blocking*` individual IRI a [`Blocking`] projects to.
fn blocking_iri(b: Blocking) -> String {
    let local = match b {
        Blocking::Coherent => "blockingCoherent",
        Blocking::Blocking => "blockingBlocking",
    };
    format!("{GMEOW_NS}{local}")
}

/// Every grade in the finite bilattice, each paired with the stable finding IRI
/// that encodes it.
fn all_grades() -> Vec<(Grade, String)> {
    let mut out = Vec::new();
    for &s in &Severity::ALL {
        for &c in &FindingCategory::ALL {
            for &p in &Standpoint::ALL {
                let iri = format!(
                    "{GMEOW_NS}examples/diagnostics/gate-conformance/g-{:?}-{:?}-{:?}",
                    s, c, p
                );
                out.push((Grade::new(s, c, p), iri));
            }
        }
    }
    out
}

/// The absolute path of a slice module `.ttl`.
fn slice_module(group: &str, name: &str) -> std::path::PathBuf {
    gmeow_conformance::paths::repo_root()
        .join("slices")
        .join(group)
        .join(name)
        .join("module.ttl")
}

/// The authored `logic:ruleGateFatalVerdict` rule, isolated from the logic slice
/// module so the chase runs ONLY the gate derivation under test. Reads the actual
/// authored `.ttl` (never re-typed), then filters to the single rule whose head
/// derives `gmeow:findingGateVerdict`.
fn authored_gate_rule() -> LogicRule {
    let module = slice_module("grounding", "logic");
    let bytes = std::fs::read(&module).unwrap_or_else(|e| panic!("read {}: {e}", module.display()));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::Turtle)
        .unwrap_or_else(|e| panic!("parse {}: {e}", module.display()));
    let (program, _diags) = parse_logic_dataset(dataset.as_ref(), None)
        .unwrap_or_else(|e| panic!("parse_logic_dataset {}: {e}", module.display()));
    program
        .rules
        .into_iter()
        .find(|r| r.head.predicate == FINDING_GATE_VERDICT)
        .expect(
            "the authored logic:ruleGateFatalVerdict (head gmeow:findingGateVerdict) must be \
             present in slices/grounding/logic/module.ttl",
        )
}

/// The authored `gmeow:categoryBlocking` wiring, read from the diagnostics slice by
/// SPARQL SELECT — the category IRI → Blocking-disposition IRI map, exactly as the
/// slice states it.
fn authored_category_blocking() -> BTreeMap<String, String> {
    let module = slice_module("core", "diagnostics");
    let bytes = std::fs::read(&module).unwrap_or_else(|e| panic!("read {}: {e}", module.display()));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::Turtle)
        .unwrap_or_else(|e| panic!("parse {}: {e}", module.display()));
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            &dataset,
            SparqlRequest {
                query: &format!("SELECT ?cat ?b WHERE {{ ?cat <{CATEGORY_BLOCKING}> ?b . }}"),
                base_iri: None,
                substitutions: &[],
            },
        )
        .expect("categoryBlocking wiring query must evaluate");
    let (variables, rows) = match result {
        SparqlResult::Solutions {
            variables, rows, ..
        } => (variables, rows),
        _ => panic!("categoryBlocking query must be a SELECT"),
    };
    let cat_idx = variables
        .iter()
        .position(|v| v == "cat")
        .expect("?cat column");
    let b_idx = variables.iter().position(|v| v == "b").expect("?b column");
    let iri = |t: &TermValue| match t {
        TermValue::Iri(i) => i.clone(),
        other => panic!("categoryBlocking wiring term must be an IRI, got {other:?}"),
    };
    let mut map = BTreeMap::new();
    for sol in &rows {
        let cat = iri(sol[cat_idx].as_ref().expect("?cat bound"));
        let b = iri(sol[b_idx].as_ref().expect("?b bound"));
        map.insert(cat, b);
    }
    map
}

/// Push one IRI triple into the world graph.
fn push_triple(builder: &mut RdfDatasetBuilder, s: &str, p: &str, o: &str) {
    let quad = RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(WORLD));
    builder.push_owned_quad(&quad);
}

#[test]
fn reasoner_gate_verdict_equals_rust_gate_over_every_grade() {
    let grades = all_grades();

    // (1) The authored category→Blocking projection MUST equal Rust blocking() for
    // every category — so the map the reasoner reads is provably the one gate() uses.
    let authored = authored_category_blocking();
    assert_eq!(
        authored.len(),
        FindingCategory::ALL.len(),
        "the diagnostics slice must wire gmeow:categoryBlocking for every FindingCategory"
    );
    for &c in &FindingCategory::ALL {
        let cat = category_iri(c);
        let expected = blocking_iri(c.blocking());
        let authored_b = authored
            .get(&cat)
            .unwrap_or_else(|| panic!("no authored gmeow:categoryBlocking for {cat}"));
        assert_eq!(
            authored_b, &expected,
            "authored gmeow:categoryBlocking for {cat} disagrees with Rust FindingCategory::blocking()"
        );
    }

    // The program is ONLY the authored gate rule (isolated from the logic slice).
    let rule = authored_gate_rule();
    let program = LogicProgram::new(Vec::new(), vec![rule], Vec::new(), None);

    // (2)+(3) The EDB: the authored wiring + the 96 encoded grade findings, all in
    // one named-graph world the chase can read.
    let mut builder = RdfDatasetBuilder::new();
    for (cat, b) in &authored {
        push_triple(&mut builder, cat, CATEGORY_BLOCKING, b);
    }
    for (grade, iri) in &grades {
        push_triple(
            &mut builder,
            iri,
            &format!("{GMEOW_NS}findingSeverity"),
            &severity_iri(grade.severity),
        );
        push_triple(
            &mut builder,
            iri,
            &format!("{GMEOW_NS}findingCategory"),
            &category_iri(grade.category),
        );
        push_triple(
            &mut builder,
            iri,
            &format!("{GMEOW_NS}findingStandpoint"),
            &standpoint_iri(grade.standpoint),
        );
    }
    let edb = builder.freeze().expect("world EDB must freeze");

    // Run the native chase: the authored gate rule alongside the fixed DL calculus.
    let result = reason_program(&program, edb.as_ref())
        .expect("native reason_program over the gate rule must succeed");

    // (4) The set of grade IRIs the reasoner DERIVED gmeow:findingGateVerdict gmeow:gateFatal for.
    // A derived row's IRI object renders in N-Triples display form (`<IRI>`); the
    // subject is the bare IRI. Match the object against the angle-bracketed form.
    let gate_fatal_display = format!("<{GATE_FATAL}>");
    let derived_fatal: BTreeSet<String> = result
        .inferred()
        .iter()
        .filter(|a| a.predicate == FINDING_GATE_VERDICT && a.object == gate_fatal_display)
        .map(|a| a.subject.clone())
        .collect();

    // The set of grade IRIs Rust gate() calls Fatal.
    let rust_fatal: BTreeSet<String> = grades
        .iter()
        .filter(|(g, _)| gate(*g) == GateVerdict::Fatal)
        .map(|(_, iri)| iri.clone())
        .collect();

    // Sanity: the derivation actually fired somewhere and did NOT fire everywhere,
    // so a degenerate rule cannot pass the equality vacuously.
    assert!(
        !rust_fatal.is_empty(),
        "the Rust gate() Fatal set must be non-empty (there ARE fatal grades)"
    );
    assert!(
        rust_fatal.len() < grades.len(),
        "the Rust gate() Fatal set must be a PROPER subset (most grades are not fatal)"
    );

    // The crux: the reasoner-derived Fatal set equals Rust gate()'s, EXACTLY, both
    // directions. This is the ontology's policy morphism proved equal to Rust's.
    let missed_by_reasoner: Vec<&String> = rust_fatal.difference(&derived_fatal).collect();
    let over_derived: Vec<&String> = derived_fatal.difference(&rust_fatal).collect();
    assert!(
        missed_by_reasoner.is_empty() && over_derived.is_empty(),
        "reasoner-derived gatesFatal set != Rust gate() Fatal set over {} grades.\n  \
         reasoner MISSED (gate()=Fatal, reasoner=Collected): {:?}\n  \
         reasoner OVER-DERIVED (gate()=Collected, reasoner=Fatal): {:?}",
        grades.len(),
        missed_by_reasoner,
        over_derived,
    );

    // Never-gate theorem (a): NO advisory-standpoint grade is ever derived fatal.
    for (grade, iri) in &grades {
        if grade.standpoint == Standpoint::Advisory {
            assert!(
                !derived_fatal.contains(iri),
                "advisory-standpoint grade must never be derived fatal: {grade:?}"
            );
        }
    }

    // Never-gate theorem (b): NO permitted-epistemic-conflict grade is ever derived fatal.
    for (grade, iri) in &grades {
        if grade.category == FindingCategory::PermittedEpistemicConflict {
            assert!(
                !derived_fatal.contains(iri),
                "permitted-epistemic-conflict grade must never be derived fatal: {grade:?}"
            );
        }
    }
}

/// Extract every `(finding, severity, category, standpoint)` grade tuple SPARQL sees in
/// a projected diagnostics graph — driving the SAME query surface a reasoner would.
fn grade_tuples(nq: &str) -> Vec<(String, String, String, String)> {
    let dataset = dataset_from_bytes(nq.as_bytes(), NativeRdfFormat::NQuads)
        .expect("to_gmeow_rdf output must parse as N-Quads");
    let engine = NativeSparqlEngine::new();
    let q = format!(
        "SELECT ?f ?sev ?cat ?sp WHERE {{ GRAPH ?g {{ \
           ?f <{GMEOW_NS}findingSeverity> ?sev ; \
              <{GMEOW_NS}findingCategory> ?cat ; \
              <{GMEOW_NS}findingStandpoint> ?sp . }} }}"
    );
    let SparqlResult::Solutions {
        variables, rows, ..
    } = engine
        .query(
            &dataset,
            SparqlRequest {
                query: &q,
                base_iri: None,
                substitutions: &[],
            },
        )
        .expect("grade-tuple query must evaluate")
    else {
        panic!("grade-tuple query must be a SELECT");
    };
    let col = |n: &str| variables.iter().position(|v| v == n).expect("column");
    let (fi, si, ci, pi) = (col("f"), col("sev"), col("cat"), col("sp"));
    let iri = |t: &TermValue| match t {
        TermValue::Iri(i) => i.clone(),
        other => panic!("grade term must be an IRI, got {other:?}"),
    };
    rows.iter()
        .map(|s| {
            (
                iri(s[fi].as_ref().expect("?f")),
                iri(s[si].as_ref().expect("?sev")),
                iri(s[ci].as_ref().expect("?cat")),
                iri(s[pi].as_ref().expect("?sp")),
            )
        })
        .collect()
}

/// Reason the AUTHORED gate rule over the grade tuples a `to_gmeow_rdf` projection emits
/// (co-worlded with the authored `categoryBlocking` map) and return the finding IRIs the
/// reasoner DERIVES `gmeow:findingGateVerdict gmeow:gateFatal` for.
fn derived_fatal_over_projection(nq: &str) -> BTreeSet<String> {
    let authored = authored_category_blocking();
    let program = LogicProgram::new(Vec::new(), vec![authored_gate_rule()], Vec::new(), None);

    let mut builder = RdfDatasetBuilder::new();
    for (cat, b) in &authored {
        push_triple(&mut builder, cat, CATEGORY_BLOCKING, b);
    }
    for (f, sev, cat, sp) in grade_tuples(nq) {
        push_triple(
            &mut builder,
            &f,
            &format!("{GMEOW_NS}findingSeverity"),
            &sev,
        );
        push_triple(
            &mut builder,
            &f,
            &format!("{GMEOW_NS}findingCategory"),
            &cat,
        );
        push_triple(
            &mut builder,
            &f,
            &format!("{GMEOW_NS}findingStandpoint"),
            &sp,
        );
    }
    let edb = builder.freeze().expect("projection EDB must freeze");
    let result = reason_program(&program, edb.as_ref())
        .expect("native reason_program over the projected finding graph must succeed");
    let gate_fatal_display = format!("<{GATE_FATAL}>");
    result
        .inferred()
        .iter()
        .filter(|a| a.predicate == FINDING_GATE_VERDICT && a.object == gate_fatal_display)
        .map(|a| a.subject.clone())
        .collect()
}

/// The production-surface demonstration (closes the TEST-ONLY gap): the gate verdict is a
/// genuine ENTAILMENT of the authored `logic:ruleGateFatalVerdict` over the ACTUAL
/// `gmeow_errors::render::to_gmeow_rdf` projection — not a Rust `gate()` hand-assertion the
/// projection pre-materializes. An up-set finding (Error / blocking category / Binding) is
/// derived `gateFatal`; a never-gate finding (Advisory standpoint) is derived NOTHING; and
/// the projection itself carries no pre-computed verdict.
#[test]
fn projected_finding_gate_verdict_is_reasoner_derived_not_hand_asserted() {
    use gmeow_errors::render::to_gmeow_rdf;
    use gmeow_errors::{Finding, Report};

    // (1) An up-set finding → the reasoner derives gateFatal over the real projection.
    let mut fatal = Report::new("validate");
    fatal.add_finding(
        Finding::new(Severity::Error, "x.upset", "up-set finding")
            .with_category(FindingCategory::DataShapeViolation)
            .with_standpoint(Standpoint::Binding),
    );
    let fatal_nq = to_gmeow_rdf(&fatal);
    // The projection emits the grade coordinates but NEVER pre-materializes the verdict.
    assert!(
        !fatal_nq.contains("findingGateVerdict"),
        "to_gmeow_rdf must not hand-assert the reasoner-derived verdict:\n{fatal_nq}"
    );
    let derived = derived_fatal_over_projection(&fatal_nq);
    assert_eq!(
        derived.len(),
        1,
        "the authored gate rule must derive exactly one gateFatal over the projected up-set finding, got {derived:?}"
    );

    // (2) A never-gate finding (Advisory) → the reasoner derives NOTHING (the up-set
    // construction structurally excludes it — the first never-gate theorem, over the real
    // projection this time).
    let mut advisory = Report::new("validate");
    advisory.add_finding(
        Finding::new(Severity::Error, "x.adv", "advisory never gates")
            .with_category(FindingCategory::DataShapeViolation)
            .with_standpoint(Standpoint::Advisory),
    );
    let advisory_nq = to_gmeow_rdf(&advisory);
    assert!(
        derived_fatal_over_projection(&advisory_nq).is_empty(),
        "an Advisory-standpoint finding must never be derived gateFatal, even over the real projection"
    );
}
