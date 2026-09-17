// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Build a REAL `stage-compile-logic` upstream product carrying a typed `Logic` handle
/// whose `reasoning_programs` are the AUTHORED demonstrator corpus
/// (`REASONING_PROGRAMS_EXAMPLE_PATH`), parsed through the exact same production
/// frontend entry point (`parse_logic_path`) the real stage uses — never a hand-built
/// fake corpus. Its `graph/logic` backing graph carries only a placeholder triple: the
/// handle pin only requires digest self-consistency (proven at construction), not full
/// program re-derivability, and this stage never reads that graph directly (only the
/// handle payload).
fn compile_logic_upstream_with_reasoning_programs() -> StageProduct {
    let path = repo_root().join(crate::stages::compile_logic::REASONING_PROGRAMS_EXAMPLE_PATH);
    let (parsed, _diags) = gmeow_logic_compile::frontend::parse_logic_path(&path, None)
        .expect("parse the authored reasoning-programs cell");
    assert!(
        !parsed.reasoning_programs.is_empty(),
        "the authored cell must carry at least one logic:ReasoningProgram"
    );
    let program = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None)
        .with_reasoning_programs(parsed.reasoning_programs);
    let dataset = crate::stages::carrier::parse_into_graph(
        b"<https://example.test/s> <https://example.test/p> <https://example.test/o> .\n",
        "application/n-triples",
        crate::stages::compile_logic::GRAPH_LOGIC,
    )
    .expect("route the placeholder graph/logic triple");
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        dataset,
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(crate::stages::compile_logic::GRAPH_LOGIC);
    bundle
        .pin_handle(
            crate::stages::compile_logic::GRAPH_LOGIC,
            PipelineHandle::Logic(Arc::new(program)),
            pinned,
        )
        .expect("pin the Logic handle to graph/logic");
    StageProduct::from_bundle("stage-compile-logic", Arc::new(bundle))
}

/// Build a REAL `stage-reason` upstream product carrying a typed `Reasoning` handle
/// whose `rdfs:subClassOf` closure includes the math: subsort tower — reasoned through
/// the exact same production `reason_artifacts` entry point the real stage uses, over a
/// tiny synthetic EDB carrying only the tower's TOLD edges (not the whole ontology, so
/// the test stays cheap). This proves `math:Integer ⊑ math:RealNumber` is genuinely
/// DERIVED by the reasoner's transitive closure, never hardcoded.
fn reason_upstream_with_math_tower() -> StageProduct {
    const NATURAL: &str = "https://blackcatinformatics.ca/math/NaturalNumber";
    const INTEGER: &str = "https://blackcatinformatics.ca/math/Integer";
    const RATIONAL: &str = "https://blackcatinformatics.ca/math/RationalNumber";
    const REAL: &str = "https://blackcatinformatics.ca/math/RealNumber";
    const COMPLEX: &str = "https://blackcatinformatics.ca/math/ComplexNumber";
    const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/authored-default";
    let nq = format!(
        "<{NATURAL}> <{RDFS_SUBCLASS_OF}> <{INTEGER}> <{WORLD}> .\n\
             <{INTEGER}> <{RDFS_SUBCLASS_OF}> <{RATIONAL}> <{WORLD}> .\n\
             <{RATIONAL}> <{RDFS_SUBCLASS_OF}> <{REAL}> <{WORLD}> .\n\
             <{REAL}> <{RDFS_SUBCLASS_OF}> <{COMPLEX}> <{WORLD}> .\n"
    );
    let reasoned = crate::stages::reason::reason_artifacts(nq.as_bytes())
        .expect("reason the synthetic math tower EDB");
    // Confirm the transitive derivation actually happened (the whole point of routing
    // subsort_edges through the REASONED closure rather than a hardcoded tower):
    // Integer ⊑ RealNumber must be a DERIVED (non-EDB) axiom, entailed from the told
    // Integer⊑Rational and Rational⊑Real edges.
    assert!(
        reasoned.result.inferred().iter().any(|axiom| {
            !axiom.is_edb
                && bare_iri(&axiom.predicate) == RDFS_SUBCLASS_OF
                && bare_iri(&axiom.subject) == INTEGER
                && axiom.object.as_iri() == Some(REAL)
        }),
        "math:Integer subClassOf math:RealNumber must be genuinely DERIVED by the reasoner"
    );
    let reasoning = gmeow_logic::result_rdf::project_reasoning_dataset(&reasoned.result)
        .expect("project the native reasoning result");
    let dataset = crate::stages::carrier::rooted_in_graph(&reasoning, GRAPH_REASONING)
        .expect("route the complete graph/reasoning projection");
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        dataset,
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(GRAPH_REASONING);
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(reasoned.result)),
            pinned,
        )
        .expect("pin the Reasoning handle to graph/reasoning");
    StageProduct::from_bundle("stage-reason", Arc::new(bundle))
}

fn real_upstream() -> BTreeMap<String, StageProduct> {
    let mut upstream = BTreeMap::new();
    upstream.insert(
        "stage-compile-logic".to_string(),
        compile_logic_upstream_with_reasoning_programs(),
    );
    upstream.insert(
        "stage-reason".to_string(),
        reason_upstream_with_math_tower(),
    );
    upstream
}

#[test]
fn goal_directed_stage_attaches_a_nonempty_goal_directed_graph() {
    let stage = GoalDirectedStage::new();
    let upstream = real_upstream();
    let root = std::path::Path::new(".");
    let out = stage
        .run(StageInput {
            root,
            upstream: &upstream,
        })
        .expect("goal-directed run");
    let dataset = out.product.bundle().dataset();
    let graph = dataset.project_named_graph(GRAPH_GOAL_DIRECTED);
    let quads: Vec<_> = graph.owned_quads().collect();
    assert!(
        !quads.is_empty(),
        "the stage attaches a non-empty graph/goal-directed"
    );
    // The authored examples' namespace (slices/grounding/logic/examples/reasoning-programs.ttl):
    // every relation/constant/function symbol is a REAL IRI (never a bare local name), so
    // the rendered answer atoms carry it in full.
    const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";
    // The graph carries the Peano demonstrator's ground answer atom + a
    // proof-derivation IRI (the proof reached the bundle, not just the answer).
    // PurRDF's `render` joins application arguments with `", "`.
    let peano_atom =
        format!("{EX}add({EX}s({EX}s({EX}zero)), {EX}s({EX}zero), {EX}s({EX}s({EX}s({EX}zero))))");
    let has_atom = quads
        .iter()
        .any(|q| matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == peano_atom));
    assert!(has_atom, "the ground answer atom is in graph/goal-directed");
    let has_derivation = quads
        .iter()
        .any(|q| q.predicate == "https://blackcatinformatics.ca/gmeow/goalDirectedDerivation");
    assert!(
        has_derivation,
        "a proof-derivation IRI is in graph/goal-directed"
    );

    // The structured member/append demonstrator's cons-list answer atom rode through.
    // PurRDF's `render` joins application arguments with `", "`.
    let member_atom =
        format!("{EX}member({EX}a, {EX}cons({EX}a, {EX}cons({EX}b, {EX}cons({EX}c, {EX}nil))))");
    let has_structured = quads
        .iter()
        .any(|q| matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == member_atom));
    assert!(
        has_structured,
        "a structured cons-list membership answer is in graph/goal-directed"
    );

    // The three-valued SLG-WFS negation demonstrator: an `undefined` loop verdict AND both
    // founded verdicts reached the graph — SLG-WFS is observable (non-dark).
    let verdict_pred = "https://blackcatinformatics.ca/gmeow/goalDirectedVerdict";
    let verdict_values: Vec<&str> = quads
        .iter()
        .filter(|q| q.predicate == verdict_pred)
        .filter_map(|q| match &q.object {
            purrdf::RdfTerm::Literal(l) => Some(l.lexical_form.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        verdict_values.contains(&"undefined"),
        "an undefined WFS verdict is in graph/goal-directed: {verdict_values:?}"
    );
    assert!(
        verdict_values.contains(&"true") && verdict_values.contains(&"false"),
        "founded true/false WFS verdicts are in graph/goal-directed: {verdict_values:?}"
    );

    // The order-sorted (ℤ ⊑ ℝ) demonstrator's subsort-unified answer atom rode through.
    let subsort_atom = format!("{EX}p({EX}one)");
    let has_subsort = quads.iter().any(
        |q| matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == subsort_atom),
    );
    assert!(
        has_subsort,
        "the order-sorted subsort-unified answer p(one) is in graph/goal-directed"
    );

    // R6: the math-subsort-control program (`ex:mathSubsortControl`) projects a query
    // node with status "ok" and ZERO answer atoms — a positive presence-of-absence: the
    // sort lattice actively refused the Integer-sorted constant for the incomparable
    // Set-sorted variable, rather than the program being silently skipped/empty.
    let name_pred = "https://blackcatinformatics.ca/gmeow/goalDirectedName";
    let status_pred = "https://blackcatinformatics.ca/gmeow/goalDirectedStatus";
    let has_answer_pred = "https://blackcatinformatics.ca/gmeow/hasGoalDirectedAnswer";
    let control_query = quads
            .iter()
            .find(|q| {
                q.predicate == name_pred
                    && matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == "mathSubsortControl")
            })
            .map(|q| q.subject.clone())
            .expect("the mathSubsortControl query node is in graph/goal-directed");
    let control_status = quads
        .iter()
        .find(|q| q.subject == control_query && q.predicate == status_pred)
        .and_then(|q| match &q.object {
            purrdf::RdfTerm::Literal(l) => Some(l.lexical_form.as_str()),
            _ => None,
        });
    assert_eq!(
        control_status,
        Some("ok"),
        "the control program's status is ok (not partial/exhausted)"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.subject == control_query && q.predicate == has_answer_pred),
        "the control program has ZERO hasGoalDirectedAnswer edges (presence-of-absence)"
    );
}

#[test]
fn run_hard_fails_when_stage_compile_logic_is_missing() {
    let stage = GoalDirectedStage::new();
    let mut upstream = BTreeMap::new();
    upstream.insert(
        "stage-reason".to_string(),
        reason_upstream_with_math_tower(),
    );
    let root = std::path::Path::new(".");
    let result = stage.run(StageInput {
        root,
        upstream: &upstream,
    });
    let Err(err) = result else {
        panic!("a missing stage-compile-logic product must HARD-fail, never fall back");
    };
    assert!(format!("{err:?}").contains("stage-compile-logic"));
}

#[test]
fn run_hard_fails_when_stage_reason_is_missing() {
    let stage = GoalDirectedStage::new();
    let mut upstream = BTreeMap::new();
    upstream.insert(
        "stage-compile-logic".to_string(),
        compile_logic_upstream_with_reasoning_programs(),
    );
    let root = std::path::Path::new(".");
    let result = stage.run(StageInput {
        root,
        upstream: &upstream,
    });
    let Err(err) = result else {
        panic!("a missing stage-reason product must HARD-fail, never fall back");
    };
    assert!(format!("{err:?}").contains("stage-reason"));
}

#[test]
fn run_hard_fails_when_the_logic_handle_carries_zero_reasoning_programs() {
    let stage = GoalDirectedStage::new();
    let program = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None);
    let dataset = crate::stages::carrier::parse_into_graph(
        b"<https://example.test/s> <https://example.test/p> <https://example.test/o> .\n",
        "application/n-triples",
        crate::stages::compile_logic::GRAPH_LOGIC,
    )
    .expect("route the placeholder graph/logic triple");
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        dataset,
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(crate::stages::compile_logic::GRAPH_LOGIC);
    bundle
        .pin_handle(
            crate::stages::compile_logic::GRAPH_LOGIC,
            PipelineHandle::Logic(Arc::new(program)),
            pinned,
        )
        .expect("pin the Logic handle to graph/logic");
    let mut upstream = BTreeMap::new();
    upstream.insert(
        "stage-compile-logic".to_string(),
        StageProduct::from_bundle("stage-compile-logic", Arc::new(bundle)),
    );
    upstream.insert(
        "stage-reason".to_string(),
        reason_upstream_with_math_tower(),
    );
    let root = std::path::Path::new(".");
    let result = stage.run(StageInput {
        root,
        upstream: &upstream,
    });
    let Err(err) = result else {
        panic!("zero authored reasoning programs must HARD-fail, never an empty result");
    };
    let msg = format!("{err:?}");
    assert!(msg.contains("reasoning_programs") || msg.contains("ReasoningProgram"));
}
