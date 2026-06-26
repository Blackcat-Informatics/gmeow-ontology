// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection tests — unit checks plus the **insta snapshot goldens** (T8,
//! #789): every projection of every `conformance/logic/cases/projections/*` case
//! is pinned by a committed `.snap` golden (text targets byte-for-byte; RDF
//! targets as a canonicalized sorted triple-set, since no golden uses blank
//! nodes). The `.snap` files ARE the byte-exact unit golden; cross-engine
//! semantic corpus parity over the same `expected/` files is owned by the native
//! `crates/conformance` harness (graph-isomorphism + bless), which is untouched.

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
    use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
    use gmeow_rdf::parse_dataset;
    // Native codec parse (#909) → frozen IR → oxigraph Store, so the existing
    // oxigraph-`Display` triple rendering below is unchanged.
    let dataset = parse_dataset(turtle.as_bytes(), "text/turtle", None)
        .unwrap_or_else(|e| panic!("turtle parse failed: {e}\n---\n{turtle}"));
    let store = store_from_dataset(dataset.as_ref(), GraphPolicy::PreserveNamedGraphs)
        .unwrap_or_else(|e| panic!("turtle materialize failed: {e}\n---\n{turtle}"));
    let mut lines: Vec<String> = store
        .iter()
        .map(|q| {
            let q = q.expect("store iteration must not fail");
            format!("{} {} {}", q.subject, q.predicate, q.object)
        })
        .collect();
    lines.sort();
    lines
}

/// Canonical sorted triple-set rendering of a Turtle document, as a single
/// newline-joined string suitable for a byte-stable insta snapshot. Reuses
/// [`triple_set`] (default graph, blank-node-free goldens) so quoted/reifier
/// terms of `canonical-rdf12` / the projection report are captured as object
/// terms in deterministic sorted order — NEVER raw oxigraph Turtle, whose blank
/// labels and statement order are non-deterministic.
fn rdf_snapshot(turtle: &str) -> String {
    triple_set(turtle).join("\n")
}

/// Pin every projection of one conformance case with insta snapshot goldens.
///
/// The `.snap` files are the byte-exact unit golden; the native
/// `crates/conformance` harness owns cross-engine semantic parity over the same
/// `expected/` corpus, so this only re-compiles the case `input.logic.ttl` (it
/// no longer reads the `expected/projections/` files).
fn run_case(case: &str) {
    let dir = conformance_dir().join(case);
    let input = std::fs::read_to_string(dir.join("input.logic.ttl")).expect("read input");
    let (program, diags) = parse_logic_str(&input, None).expect("parse conformance input");
    assert!(
        diags.is_empty(),
        "[{case}] unexpected parse diagnostics: {diags:?}"
    );
    let arts = compile_program(&program).expect("compile");

    // One `.snap` per (case, target). The per-case suffix keeps the goldens
    // discoverable and avoids a single mega-snapshot.
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_suffix(case);
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        // Text targets: byte-identical (the front-end canonicalizes blank labels).
        insta::assert_snapshot!("datalog", arts.datalog);
        insta::assert_snapshot!("n3", arts.n3);
        insta::assert_snapshot!("nemo", arts.nemo);

        // RDF targets: canonicalized sorted triple-set.
        insta::assert_snapshot!("owl-dl", rdf_snapshot(&arts.owl_dl));
        insta::assert_snapshot!("owl-el", rdf_snapshot(&arts.owl_el));
        insta::assert_snapshot!("gufo", rdf_snapshot(&arts.gufo));
        insta::assert_snapshot!("canonical-rdf12", rdf_snapshot(&arts.canonical_rdf12));
        insta::assert_snapshot!("projection-report", rdf_snapshot(&arts.report));
    });
}

// ── ReasoningContract round-trip (#767, Task 6) ──────────────────────────────
//
// The canonical RDF 1.2 projection of a ReasoningContract must be LOSSLESS:
// re-parsing the emitted triples through `extract_contracts` (via parse_logic_str)
// reconstructs the byte-identical contract (same `sort_key()`), whether the
// contract originated from a preset or from direct facets.

use crate::compile::ir::{ReasoningContract, SemanticProfileId};

/// Build a fully-loaded preset contract exercising every facet kind.
fn loaded_preset_contract() -> ReasoningContract {
    let mut c = ReasoningContract::from_preset(SemanticProfileId::ProceduralProlog);
    c.formula_fragment = Some("HornFragment".to_owned());
    c.model_semantics = Some("LeastModelSemantics".to_owned());
    c.truth_algebra = Some("BooleanAlgebra".to_owned());
    c.admissible_valuation = Some("ForbidGap".to_owned());
    c.designated_values = Some("TrueDesignated".to_owned());
    c.evolution = Some("StaticEvolution".to_owned());
    c.argumentation = Some("GroundedSemantics".to_owned());
    // MonotonicRevision (not EntrenchmentRevision) so the counterfactual-coupling
    // compatibility rules do not fire — this contract must be SUPPORTED so the
    // round-trip reparse is diagnostic-free.
    c.revision = Some("MonotonicRevision".to_owned());
    c.equality_policy = Some("UniqueNameAssumption".to_owned());
    c.default_closure = Some("OpenWorldClosure".to_owned());
    c.negation_operators.insert("DefaultNegation".to_owned());
    c.negation_operators.insert("ExplicitNegation".to_owned());
    c.context_axes.insert("StandpointContextAxis".to_owned());
    c.uncertainty_measures
        .insert("PossibilityMeasure".to_owned());
    c.resource_policies.insert("ProceduralExecution".to_owned());
    c.resource_policies
        .insert("BudgetBoundedResource".to_owned());
    c.projection_targets.insert("OwlProjection".to_owned());
    c.closure_entries
        .insert("ex:closedPred".to_owned(), "ClosedWorldClosure".to_owned());
    c.closure_entries
        .insert("ex:openPred".to_owned(), "OpenWorldClosure".to_owned());
    c.complexity = Some(crate::compile::ir::ComplexityClass::new("PTIME").unwrap());
    c
}

/// Reconstruct the program from the canonical RDF 1.2 projection bytes.
fn reparse_canonical(program: &LogicProgram) -> LogicProgram {
    let proj = rdf::project_canonical_rdf12(program).expect("project ok");
    let (reparsed, diags) = parse_logic_str(&proj.content, None).expect("reparse ok");
    assert!(
        diags.is_empty(),
        "round-trip reparse produced diagnostics: {diags:?}"
    );
    reparsed
}

#[test]
fn contract_round_trip_preset_with_full_facets() {
    let original = loaded_preset_contract();
    let program = LogicProgram::new(vec![], vec![], vec![original.clone()], None);
    let reparsed = reparse_canonical(&program);

    assert_eq!(reparsed.contracts.len(), 1, "exactly one contract survives");
    let got = &reparsed.contracts[0];
    assert_eq!(
        got.sort_key(),
        original.sort_key(),
        "preset contract sort_key must round-trip exactly"
    );
    assert_eq!(
        *got, original,
        "preset contract must reconstruct identically"
    );
}

#[test]
fn contract_round_trip_anonymous_faceted_contract() {
    // No preset — exercises the minted logic:contract/_NNNNNN subject node.
    let mut original = ReasoningContract::new();
    original.model_semantics = Some("WellFoundedSemantics".to_owned());
    original
        .negation_operators
        .insert("DefaultNegation".to_owned());
    original.default_closure = Some("OpenWorldClosure".to_owned());
    original
        .closure_entries
        .insert("ex:k".to_owned(), "ClosedWorldClosure".to_owned());
    original.complexity = Some(crate::compile::ir::ComplexityClass::new("PTIME").unwrap());

    let program = LogicProgram::new(vec![], vec![], vec![original.clone()], None);
    let reparsed = reparse_canonical(&program);

    assert_eq!(reparsed.contracts.len(), 1);
    let got = &reparsed.contracts[0];
    assert_eq!(
        got.sort_key(),
        original.sort_key(),
        "anonymous contract sort_key must round-trip exactly"
    );
    assert_eq!(*got, original);
}

#[test]
fn contract_round_trip_custom_iri_facet_value() {
    // #767, Gap 3: the open facet-value vocabulary admits a value that is a full
    // CUSTOM IRI (not under the logic: namespace). The projection must emit it
    // verbatim (never re-prefixed to a corrupt `…/logic/https://…`) and storage
    // must keep the full IRI, so project → reparse round-trips identically.
    let mut original = ReasoningContract::new();
    original.model_semantics = Some("https://example.org/MyModelSemantics".to_owned());

    let program = LogicProgram::new(vec![], vec![], vec![original.clone()], None);
    let reparsed = reparse_canonical(&program);

    assert_eq!(reparsed.contracts.len(), 1);
    let got = &reparsed.contracts[0];
    assert_eq!(
        got.model_semantics.as_deref(),
        Some("https://example.org/MyModelSemantics"),
        "custom-IRI facet value must round-trip verbatim (no double-prefix)"
    );
    assert_eq!(
        got.sort_key(),
        original.sort_key(),
        "custom-IRI facet value contract sort_key must round-trip exactly"
    );
    assert_eq!(*got, original);
}

#[test]
fn contract_round_trip_passes_ir_isomorphism_gate_on_contracts() {
    // Reviewer B1: exercise the IR-isomorphism gate's CONTRACT branch with a
    // NON-empty contract set (the adapter fixtures only ever carry empty
    // contracts). The gate keys contracts on `sort_key()` (the `contract_key`
    // helper), so a program built from only the original contracts and one built
    // from only the round-tripped contracts must compare contract-isomorphic.
    //
    // (The full program is intentionally NOT asserted equal: like the rule
    // projection, the canonical-RDF12 facet triples are themselves logic:-predicate
    // triples that the axiom extractor also reads, so the round-trip is an
    // EXACT reconstruction of the *contract*, not a no-op over the whole graph.)
    use crate::compile::adapter::assert_ir_isomorphic;
    let contracts = vec![loaded_preset_contract(), {
        let mut c = ReasoningContract::new();
        c.model_semantics = Some("WellFoundedSemantics".to_owned());
        c.negation_operators.insert("DefaultNegation".to_owned());
        c
    }];
    let program = LogicProgram::new(vec![], vec![], contracts, None);
    let reparsed = reparse_canonical(&program);

    let contracts_only = LogicProgram::new(vec![], vec![], program.contracts.clone(), None);
    let reparsed_contracts_only =
        LogicProgram::new(vec![], vec![], reparsed.contracts.clone(), None);
    assert_ir_isomorphic(&contracts_only, &reparsed_contracts_only)
        .expect("contracts must round-trip through the isomorphism gate's contract branch");
}

#[test]
fn contract_round_trip_is_exact_preservation_no_drops() {
    // The canonical-rdf12 target is ExactPreservation: projecting a contract-bearing
    // program must not trip the overclaim gate (zero actual drops on that target).
    let program = LogicProgram::new(vec![], vec![], vec![loaded_preset_contract()], None);
    let proj = rdf::project_canonical_rdf12(&program).expect("project ok");
    assert!(
        proj.actual_drops.is_empty(),
        "canonical-rdf12 must drop nothing: {:?}",
        proj.actual_drops
    );
    // The lossy targets, by contrast, RECORD the contract as a drop.
    let owl = rdf::project_owl_dl(&program).expect("owl-dl ok");
    assert!(
        owl.actual_drops
            .iter()
            .any(|d| d.contains("reasoning contract")),
        "owl-dl must record the dropped contract: {:?}",
        owl.actual_drops
    );
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

// ── G1: path-projection wiring ───────────────────────────────────────────────
//
// Verifies that `compile_program` genuinely populates `path_projections` (not
// dead code) and extends `preservation_ledger` with a `"property-path:<iri>"`
// entry for every declared `logic:PathShape`.  Uses the same nearbyOrgs fixture
// the per-function tests in paths/tests.rs use.

#[test]
fn compile_program_wires_path_projections_and_ledger() {
    let ttl = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:nearbyOrgs a logic:PathShape ;
    logic:pathWildcard true ;
    logic:pathNamespaceScope \"https://example.org/org/\"^^xsd:anyURI ;
    logic:pathMinDepth 1 ; logic:pathMaxDepth 2 ; logic:pathDepthParam \"maxDepth\" .";
    let (program, _diags) = parse_logic_str(ttl, None).expect("parse");
    let arts = compile_program(&program).expect("compile");

    // G1a: path_projections is non-empty and carries both surfaces.
    assert_eq!(
        arts.path_projections.len(),
        1,
        "one PathShape → one PathProjection"
    );
    let pp = &arts.path_projections[0];
    assert_eq!(pp.shape_iri, "https://example.org/test/nearbyOrgs");
    assert!(
        !pp.property_path.is_empty(),
        "property_path surface must be non-empty"
    );
    assert!(!pp.datalog.is_empty(), "datalog surface must be non-empty");

    // G1b: preservation_ledger contains a property-path row keyed by IRI.
    let expected_key = format!("property-path:{}", pp.shape_iri);
    let ledger_entry = arts
        .preservation_ledger
        .iter()
        .find(|e| e.target == expected_key)
        .unwrap_or_else(|| {
            panic!(
                "preservation_ledger must contain a \"{expected_key}\" entry; \
                 found: {:?}",
                arts.preservation_ledger
                    .iter()
                    .map(|e| &e.target)
                    .collect::<Vec<_>>()
            )
        });
    assert_eq!(ledger_entry.preservation, "SoundUnderApproximation");
    assert!(
        !ledger_entry.lossy_drops.is_empty(),
        "property-path ledger entry must declare lossy_drops"
    );
}

// ── CR5: the projection report includes the property-path targets ────────────
//
// build_projection_report runs over the SAME target list the preservation ledger
// is built from (path projections are folded into `owned` BEFORE the report), so
// the report Turtle and the ledger must AGREE — both carry a property-path row for
// every declared logic:PathShape (maximal information flow; no suppression).

#[test]
fn projection_report_includes_property_path_targets() {
    let ttl = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:nearbyOrgs a logic:PathShape ;
    logic:pathWildcard true ;
    logic:pathNamespaceScope \"https://example.org/org/\"^^xsd:anyURI ;
    logic:pathMinDepth 1 ; logic:pathMaxDepth 2 ; logic:pathDepthParam \"maxDepth\" .";
    let (program, _diags) = parse_logic_str(ttl, None).expect("parse");
    let arts = compile_program(&program).expect("compile");

    // The report Turtle must carry the property-path target as an rdfs:label, in
    // lock-step with the ledger row keyed `property-path:<iri>`.
    let pp = &arts.path_projections[0];
    let expected_label = format!("property-path:{}", pp.shape_iri);
    assert!(
        arts.report.contains(&expected_label),
        "the projection report must include a property-path target labelled {expected_label:?}; \
         report:\n{}",
        arts.report
    );

    // And it must agree with the preservation ledger (same target list).
    assert!(
        arts.preservation_ledger
            .iter()
            .any(|e| e.target == expected_label),
        "report and ledger must agree on the property-path target {expected_label:?}"
    );
}
