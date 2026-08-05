// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The diagnostic meta-reasoning conformance lane.
//!
//! This proves — by the native reasoner, over the ACTUAL authored ontology — that
//! the `gmeow:DiagnosticMetaRule` fold (root-cause, cluster, cross-node-glut, and the
//! root-finding extensibility demonstrator) derives the meta-findings its schema
//! commits to, over world-scoped fixture EDBs. Every rule is loaded from
//! `slices/grounding/logic/module.ttl` and SELECTED BY TYPE (`?r a
//! gmeow:DiagnosticMetaRule`), never re-typed in Rust and never selected by a
//! hardcoded head predicate — so the extensibility claim (a new meta-finding is added
//! by tagging a rule, with zero engine change) is proved structurally.
//!
//! Four things are asserted:
//!
//!  1. **root-cause / cluster** — findings sharing one childless antecedent derive
//!     `gmeow:findingRootCause` pointing at the shared root, grouped by
//!     `gmeow:findingCluster` / `gmeow:clusterRoot`; independent findings derive
//!     nothing.
//!  2. **cross-node glut** — two opposing-polarity findings at one non-trivial anchor
//!     derive `gmeow:crossNodeGlutWith`; same-polarity, different-anchor, and
//!     trivial-anchor fixtures derive nothing.
//!  3. **extensibility** — a THIRD tagged rule (`logic:ruleRootFinding`) fires through
//!     the SAME `gmeow:DiagnosticMetaRule` class selection, with no engine change.
//!  4. **stratification hard-fail** — a deliberately unstratifiable program (a negative
//!     cycle) makes `reason_program` return a HARD ERROR, never silently under-derive.

use std::collections::BTreeSet;

use gmeow_logic::reason::reason_program;
use gmeow_logic_compile::frontend::parse_logic_dataset;
use gmeow_logic_compile::ir::{LogicProgram, LogicRule};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, SparqlEngine, SparqlRequest,
    SparqlResult, TermValue, dataset_from_bytes,
};

use gmeow_ns::GMEOW_NS;
use gmeow_ns::LOGIC_NS;
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const DIAGNOSTIC_META_RULE: &str = "https://blackcatinformatics.ca/gmeow/DiagnosticMetaRule";
const CATEGORY_POLARITY: &str = "https://blackcatinformatics.ca/gmeow/categoryPolarity";

/// The single named-graph world the fixture EDB is re-scoped into. The chase reads
/// facts out of named-graph worlds; a plain default-graph fact is invisible to it by
/// design, so the whole EDB must be world-scoped (exactly as the gate-morphism lane).
const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics-meta-conformance";

fn gmeow(local: &str) -> String {
    format!("{GMEOW_NS}{local}")
}
fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}

/// The absolute path of a slice module `.ttl`.
fn slice_module(group: &str, name: &str) -> std::path::PathBuf {
    gmeow_conformance::paths::repo_root()
        .join("slices")
        .join(group)
        .join(name)
        .join("module.ttl")
}

/// The absolute path of a diagnostics-slice test fixture.
fn fixture(rel: &str) -> std::path::PathBuf {
    gmeow_conformance::paths::repo_root()
        .join("slices/core/diagnostics/tests")
        .join(rel)
}

fn parse_ttl(path: &std::path::Path) -> std::sync::Arc<RdfDataset> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    dataset_from_bytes(&bytes, NativeRdfFormat::Turtle)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn iri(t: &TermValue) -> Option<String> {
    match t {
        TermValue::Iri(i) => Some(i.clone()),
        _ => None,
    }
}

/// Run a `SELECT` over `dataset` and return the solution rows keyed by variable name.
fn select(dataset: &std::sync::Arc<RdfDataset>, query: &str) -> Vec<Vec<Option<TermValue>>> {
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            dataset,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .unwrap_or_else(|e| panic!("query must evaluate: {e}"));
    match result {
        SparqlResult::Solutions { rows, .. } => rows,
        _ => panic!("query must be a SELECT"),
    }
}

/// The set of `gmeow:DiagnosticMetaRule` IRIs, read from the logic slice BY TYPE — the
/// class-based selection design-decision-2 mandates (never a hardcoded head predicate).
fn meta_rule_iris() -> BTreeSet<String> {
    let module = slice_module("grounding", "logic");
    let dataset = parse_ttl(&module);
    select(
        &dataset,
        &format!("SELECT ?r WHERE {{ ?r <{RDF_TYPE}> <{DIAGNOSTIC_META_RULE}> . }}"),
    )
    .iter()
    .filter_map(|row| iri(row[0].as_ref()?))
    .collect()
}

/// The program of ALL and ONLY the `gmeow:DiagnosticMetaRule` rules, discovered by TYPE
/// and matched to their parsed [`LogicRule`] via `logic:provenance` (each rule carries
/// its own IRI there). This is the class-based fold selection, isolated from the rest of
/// the logic slice so the chase runs exactly the meta rules.
fn meta_program() -> (LogicProgram, BTreeSet<String>) {
    let module = slice_module("grounding", "logic");
    let dataset = parse_ttl(&module);
    let (program, _diags) = parse_logic_dataset(dataset.as_ref(), None)
        .unwrap_or_else(|e| panic!("parse_logic_dataset {}: {e}", module.display()));
    let iris = meta_rule_iris();
    let rules: Vec<LogicRule> = program
        .rules
        .into_iter()
        .filter(|r| {
            r.scope
                .provenance
                .as_deref()
                .is_some_and(|p| iris.contains(p))
        })
        .collect();
    assert_eq!(
        rules.len(),
        iris.len(),
        "every gmeow:DiagnosticMetaRule (by type) must resolve to a parsed rule via \
         logic:provenance; got {} rules for {} typed IRIs",
        rules.len(),
        iris.len(),
    );
    (LogicProgram::new(Vec::new(), rules, Vec::new(), None), iris)
}

/// The authored `gmeow:categoryPolarity` wiring (category IRI → InformationState IRI),
/// read from the diagnostics slice — the SAME source the cross-node-glut rule reads.
fn authored_category_polarity() -> Vec<(String, String)> {
    let module = slice_module("core", "diagnostics");
    let dataset = parse_ttl(&module);
    select(
        &dataset,
        &format!("SELECT ?c ?p WHERE {{ ?c <{CATEGORY_POLARITY}> ?p . }}"),
    )
    .iter()
    .filter_map(|row| Some((iri(row[0].as_ref()?)?, iri(row[1].as_ref()?)?)))
    .collect()
}

/// Every IRI-object triple of a fixture `.ttl` (fixtures are authored in the default
/// graph; the chase needs them world-scoped, so the caller re-graphs them into `WORLD`).
fn fixture_triples(path: &std::path::Path) -> Vec<(String, String, String)> {
    let dataset = parse_ttl(path);
    select(&dataset, "SELECT ?s ?p ?o WHERE { ?s ?p ?o . }")
        .iter()
        .filter_map(|row| {
            Some((
                iri(row[0].as_ref()?)?,
                iri(row[1].as_ref()?)?,
                iri(row[2].as_ref()?)?,
            ))
        })
        .collect()
}

fn push(builder: &mut RdfDatasetBuilder, s: &str, p: &str, o: &str) {
    let quad = RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(WORLD));
    builder.push_owned_quad(&quad);
}

/// Build the world-scoped EDB from a fixture, unioning the authored
/// `gmeow:categoryPolarity` wiring so the glut rule reads the real projection.
fn edb_for(fixture_rel: &str) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in fixture_triples(&fixture(fixture_rel)) {
        push(&mut builder, &s, &p, &o);
    }
    for (c, p) in authored_category_polarity() {
        push(&mut builder, &c, CATEGORY_POLARITY, &p);
    }
    builder.freeze().expect("fixture EDB must freeze")
}

/// The set of `(subject, object-IRI)` pairs the reasoner DERIVED for `predicate`. A
/// derived row renders its object in N-Triples form (`<IRI>`); strip the brackets.
fn derived_pairs(
    result: &gmeow_logic::result::ReasoningResult,
    predicate: &str,
) -> BTreeSet<(String, String)> {
    result
        .inferred()
        .iter()
        .filter(|a| a.predicate == predicate)
        .filter_map(|a| {
            let obj = a.object.strip_prefix('<')?.strip_suffix('>')?.to_string();
            Some((a.subject.clone(), obj))
        })
        .collect()
}

/// The set of subjects the reasoner DERIVED `rdf:type <class>` for.
fn derived_typed(result: &gmeow_logic::result::ReasoningResult, class: &str) -> BTreeSet<String> {
    let want = format!("<{class}>");
    result
        .inferred()
        .iter()
        .filter(|a| a.predicate == RDF_TYPE && a.object == want && !a.is_edb)
        .map(|a| a.subject.clone())
        .collect()
}

#[test]
fn root_cause_and_cluster_derive_over_a_shared_root() {
    let (program, iris) = meta_program();
    // The three principal + helper + extensibility rules are all discovered by TYPE.
    assert!(
        iris.contains(&logic("ruleFindingRootCause")),
        "the root-cause rule must be discovered by gmeow:DiagnosticMetaRule type"
    );

    let edb = edb_for("conformance-fixtures/finding-root-cause-present.ttl");
    let result = reason_program(&program, edb.as_ref())
        .expect("native reason_program over the meta rules must succeed");

    let (f1, f2, f3, f4) = (
        gmeow("examples/diagnostics/tests/rcF1"),
        gmeow("examples/diagnostics/tests/rcF2"),
        gmeow("examples/diagnostics/tests/rcF3"),
        gmeow("examples/diagnostics/tests/rcF4"),
    );

    // findingRootCause: F2, F3, F4 all point at the shared childless root F1.
    let root_cause = derived_pairs(&result, &gmeow("findingRootCause"));
    for f in [&f2, &f3, &f4] {
        assert!(
            root_cause.contains(&(f.clone(), f1.clone())),
            "expected gmeow:findingRootCause({f}, {f1}); derived: {root_cause:?}"
        );
    }
    // The intermediate F2 is NOT a root of F4 (F2 has an antecedent): honest shared root.
    assert!(
        !root_cause.contains(&(f4.clone(), f2.clone())),
        "F2 has an antecedent, so it must never be F4's root cause"
    );

    // findingCluster + clusterRoot: the group keyed by F1 gathers F2, F3, F4; the
    // cluster node (F1) carries clusterRoot to itself and is typed gmeow:FindingCluster.
    let cluster = derived_pairs(&result, &gmeow("findingCluster"));
    for f in [&f2, &f3, &f4] {
        assert!(
            cluster.contains(&(f.clone(), f1.clone())),
            "expected gmeow:findingCluster({f}, {f1}); derived: {cluster:?}"
        );
    }
    let cluster_root = derived_pairs(&result, &gmeow("clusterRoot"));
    assert!(
        cluster_root.contains(&(f1.clone(), f1.clone())),
        "expected gmeow:clusterRoot({f1}, {f1}); derived: {cluster_root:?}"
    );
    assert!(
        derived_typed(&result, &gmeow("FindingCluster")).contains(&f1),
        "the root-keyed cluster node F1 must be typed gmeow:FindingCluster"
    );

    // Extensibility: the THIRD tagged rule (ruleRootFinding) fires through the SAME
    // class selection — F1 is typed gmeow:RootFinding — with no engine change.
    assert!(
        iris.contains(&logic("ruleRootFinding")),
        "the extensibility rule must be discovered by gmeow:DiagnosticMetaRule type"
    );
    assert!(
        derived_typed(&result, &gmeow("RootFinding")).contains(&f1),
        "the extensibility rule must type the witnessed root F1 as gmeow:RootFinding"
    );
}

#[test]
fn independent_findings_derive_no_root_cause() {
    let (program, _iris) = meta_program();
    let edb = edb_for("counter-examples/finding-root-cause-absent.ttl");
    let result =
        reason_program(&program, edb.as_ref()).expect("native reason_program must succeed");

    assert!(
        derived_pairs(&result, &gmeow("findingRootCause")).is_empty(),
        "independent findings (no antecedent chain) must derive NO gmeow:findingRootCause"
    );
    assert!(
        derived_pairs(&result, &gmeow("findingTraces")).is_empty(),
        "independent findings must derive NO gmeow:findingTraces"
    );
    assert!(
        derived_pairs(&result, &gmeow("findingCluster")).is_empty(),
        "independent findings must derive NO gmeow:findingCluster"
    );
}

#[test]
fn cross_node_glut_derives_at_a_non_trivial_anchor() {
    let (program, iris) = meta_program();
    assert!(
        iris.contains(&logic("ruleCrossNodeGlut")),
        "the cross-node-glut rule must be discovered by gmeow:DiagnosticMetaRule type"
    );

    let edb = edb_for("conformance-fixtures/cross-node-glut-present.ttl");
    let result =
        reason_program(&program, edb.as_ref()).expect("native reason_program must succeed");

    let supported = gmeow("examples/diagnostics/tests/glutSupported");
    let opposed = gmeow("examples/diagnostics/tests/glutOpposed");
    let glut = derived_pairs(&result, &gmeow("crossNodeGlutWith"));
    assert!(
        glut.contains(&(supported.clone(), opposed.clone())),
        "expected gmeow:crossNodeGlutWith({supported}, {opposed}); derived: {glut:?}"
    );
}

#[test]
fn cross_node_glut_never_fires_on_the_counter_examples() {
    let (program, _iris) = meta_program();
    for fixture_rel in [
        "counter-examples/cross-node-glut-same-polarity.ttl",
        "counter-examples/cross-node-glut-different-anchor.ttl",
        "counter-examples/cross-node-glut-trivial-anchor.ttl",
    ] {
        let edb = edb_for(fixture_rel);
        let result = reason_program(&program, edb.as_ref())
            .unwrap_or_else(|e| panic!("reason_program over {fixture_rel} must succeed: {e}"));
        assert!(
            derived_pairs(&result, &gmeow("crossNodeGlutWith")).is_empty(),
            "the cross-node-glut rule must derive NOTHING over {fixture_rel}"
        );
    }
}

/// An unstratifiable program (a negative cycle `pingA :- Seed, ~pingB` and
/// `pingB :- Seed, ~pingA`) MUST make `reason_program` hard-error, never silently
/// under-derive or loop. This is the single biggest correctness risk of the fold: NAF
/// is only sound under stratification, so an unstratifiable rule set must be refused.
#[test]
fn unstratifiable_program_hard_fails() {
    // Authored inline (never a repo artifact): two rules forming a negative cycle over
    // helper predicates each rule derives and the other negates.
    let ttl = format!(
        r#"@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix logic: <{LOGIC_NS}> .
@prefix gmeow: <{GMEOW_NS}> .

logic:ruleUnstratA a logic:Rule ;
    logic:provenance logic:ruleUnstratA ;
    logic:head [ rdf:subject "?x" ; rdf:predicate gmeow:pingA ; rdf:object "?x" ] ;
    logic:body [ rdf:subject "?x" ; rdf:predicate rdf:type ; rdf:object gmeow:UnstratSeed ] ;
    logic:negatedBody [ rdf:subject "?x" ; rdf:predicate gmeow:pingB ; rdf:object "?x" ] .

logic:ruleUnstratB a logic:Rule ;
    logic:provenance logic:ruleUnstratB ;
    logic:head [ rdf:subject "?x" ; rdf:predicate gmeow:pingB ; rdf:object "?x" ] ;
    logic:body [ rdf:subject "?x" ; rdf:predicate rdf:type ; rdf:object gmeow:UnstratSeed ] ;
    logic:negatedBody [ rdf:subject "?x" ; rdf:predicate gmeow:pingA ; rdf:object "?x" ] .
"#
    );
    let dataset =
        dataset_from_bytes(ttl.as_bytes(), NativeRdfFormat::Turtle).expect("inline rules parse");
    let (program, _diags) =
        parse_logic_dataset(dataset.as_ref(), None).expect("inline rules lower");
    assert_eq!(
        program.rules.len(),
        2,
        "the inline unstratifiable program must carry both cycle rules"
    );

    // A non-vacuous seed so the negative cycle is live.
    let mut builder = RdfDatasetBuilder::new();
    push(
        &mut builder,
        &gmeow("examples/diagnostics/tests/unstratX"),
        RDF_TYPE,
        &gmeow("UnstratSeed"),
    );
    let edb = builder.freeze().expect("seed EDB must freeze");

    let outcome = reason_program(&program, edb.as_ref());
    assert!(
        outcome.is_err(),
        "an unstratifiable (negative-cycle) program MUST hard-fail, got Ok: {:?}",
        outcome.map(|r| r.inferred().len())
    );
}
