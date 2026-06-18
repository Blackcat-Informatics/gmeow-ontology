// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection tests — unit checks plus the **parity gate**: every projection of
//! every `conformance/logic/cases/projections/*` case must match the committed
//! Python goldens (text targets byte-for-byte; RDF targets by triple-set, since
//! no golden uses blank nodes).

use std::path::PathBuf;

use super::*;
use crate::compile::frontend::parse_logic_str;

// ── Unit: helpers ────────────────────────────────────────────────────────────

#[test]
fn python_repr_matches_cpython() {
    assert_eq!(python_repr("0.9"), "'0.9'");
    assert_eq!(python_repr("hello"), "'hello'");
    // Contains a single quote but no double quote → switch to double quotes.
    assert_eq!(python_repr("it's"), "\"it's\"");
    // Contains both → stay single-quoted, escape the single quote.
    assert_eq!(python_repr("a'b\"c"), "'a\\'b\"c'");
    assert_eq!(python_repr("tab\there"), "'tab\\there'");
}

#[test]
fn overclaim_gate_fires_on_exact_with_drops() {
    use crate::compile::ir::PreservationKind;
    assert!(assert_no_overclaim("nemo", PreservationKind::Exact, &[]).is_ok());
    let err = assert_no_overclaim(
        "nemo",
        PreservationKind::Exact,
        &["dropped something".to_owned()],
    )
    .unwrap_err();
    assert!(err.0.contains("Overclaim"));
    // SoundUnder with drops is fine.
    assert!(assert_no_overclaim("owl-dl", PreservationKind::SoundUnder, &["x".to_owned()]).is_ok());
}

#[test]
fn extract_nemo_rules_section_finds_marker() {
    let nemo = text::project_nemo(&parse("ex:A logic:subClassOf ex:B .")).unwrap();
    let rules = text::extract_nemo_rules_section(&nemo.content).unwrap();
    assert!(rules.is_empty()); // no rules in this program
    assert!(text::extract_nemo_rules_section("no marker here").is_err());
}

// ── Unit: rule emission (text targets, not exercised by the axiom-only cases) ──

#[test]
fn nemo_rule_safety_violation_errors() {
    // Head variable ?z absent from body → safety violation.
    let prog = parse(
        "ex:r a logic:Rule ;
            logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:p ; rdf:object \"?z\" ] ;
            logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:q ; rdf:object \"?y\" ] .",
    );
    let err = text::project_nemo(&prog).unwrap_err();
    assert!(err.contains("safety violation"), "got: {err}");
}

#[test]
fn datalog_rule_emits_world_var_and_guard() {
    let prog = parse(
        "ex:r a logic:Rule ;
            logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
            logic:distinctBody [ rdf:subject \"?x\" ; rdf:object \"?y\" ] .",
    );
    let dl = text::project_datalog(&prog);
    assert!(dl.content.contains("rel(?x, ?y, ?C) :-"), "{}", dl.content);
    assert!(dl.content.contains("?x != ?y"), "{}", dl.content);
}

// ── The parity gate ──────────────────────────────────────────────────────────

fn conformance_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/logic/cases/projections")
}

fn parse(ttl: &str) -> LogicProgram {
    let prefixes = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";
    parse_logic_str(&format!("{prefixes}{ttl}"), None)
        .expect("parse ok")
        .0
}

/// Canonical sorted triple lines of a Turtle document (default graph), for
/// triple-set equality (valid because no golden uses blank nodes).
fn triple_set(turtle: &str) -> Vec<String> {
    use oxigraph::io::RdfFormat;
    use oxigraph::store::Store;
    let store = Store::new().unwrap();
    store
        .load_from_reader(RdfFormat::Turtle, turtle.as_bytes())
        .unwrap_or_else(|e| panic!("turtle parse failed: {e}\n---\n{turtle}"));
    let mut lines: Vec<String> = store
        .iter()
        .filter_map(Result::ok)
        .map(|q| format!("{} {} {}", q.subject, q.predicate, q.object))
        .collect();
    lines.sort();
    lines
}

fn assert_rdf_iso(case: &str, target: &str, got: &str, expected: &str) {
    let g = triple_set(got);
    let e = triple_set(expected);
    if g != e {
        let only_got: Vec<_> = g.iter().filter(|x| !e.contains(x)).collect();
        let only_exp: Vec<_> = e.iter().filter(|x| !g.contains(x)).collect();
        panic!(
            "[{case}/{target}] RDF triple-set mismatch\n  got-only: {only_got:#?}\n  \
             expected-only: {only_exp:#?}"
        );
    }
}

fn run_case(case: &str) {
    let dir = conformance_dir().join(case);
    let input = std::fs::read_to_string(dir.join("input.logic.ttl")).expect("read input");
    let (program, diags) = parse_logic_str(&input, None).expect("parse conformance input");
    assert!(
        diags.is_empty(),
        "[{case}] unexpected parse diagnostics: {diags:?}"
    );
    let arts = compile_program(&program).expect("compile");

    let exp = dir.join("expected/projections");
    let read = |name: &str| std::fs::read_to_string(exp.join(name)).expect(name);

    // Text targets: byte-identical.
    assert_eq!(arts.datalog, read("datalog.dl"), "[{case}] datalog bytes");
    assert_eq!(arts.n3, read("n3.n3"), "[{case}] n3 bytes");
    assert_eq!(arts.nemo, read("nemo.rls"), "[{case}] nemo bytes");

    // RDF targets: triple-set / isomorphism.
    assert_rdf_iso(case, "owl-dl", &arts.owl_dl, &read("owl-dl.ttl"));
    assert_rdf_iso(case, "owl-el", &arts.owl_el, &read("owl-el.ttl"));
    assert_rdf_iso(case, "gufo", &arts.gufo, &read("gufo.ttl"));
    assert_rdf_iso(
        case,
        "canonical-rdf12",
        &arts.canonical_rdf12,
        &read("canonical-rdf12.ttl"),
    );
    assert_rdf_iso(case, "report", &arts.report, &read("projection-report.ttl"));
}

#[test]
fn parity_confidence_scoped_axiom() {
    run_case("confidence-scoped-axiom");
}

#[test]
fn parity_kind_hierarchy() {
    run_case("kind-hierarchy");
}

#[test]
fn parity_relator_mediation() {
    run_case("relator-mediation");
}
