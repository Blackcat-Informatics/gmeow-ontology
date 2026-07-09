// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `logic:` grounding layer's flagship execution-discharge harness.
//!
//! This lifts the logic grounding layer's five flagship acceptance scenarios to the same
//! EXECUTION rung as `lang:` and `math:`, through the slice-generic discharge core in
//! [`support::flagship_discharge`]. The generic runner reads the acceptance manifest
//! `slices/grounding/logic/examples/flagship-acceptance.ttl`, and for each of the five
//! `gmeow:FlagshipScenario` individuals it:
//!
//! 1. **Runs the guard.** Loads the `gmeow:guardedByCounterExample` fixture MERGED with
//!    the slice `module.ttl`, pushes it through BOTH native validation channels (structural
//!    lint + native SHACL), and asserts the UNION of triggered `logic:` failure classes
//!    equals EXACTLY the one named by `gmeow:enforcesFailureClass` — set equality.
//! 2. **Checks the worked example.** Loads the `gmeow:demonstratedByExample` fixture, runs
//!    the SAME two channels, and asserts NO `logic:` failure class fires.
//! 3. **Runs the producer.** This file supplies the per-slice producer callback: it
//!    dispatches the `gmeow:demonstratedByProducer` identifier to the matching native
//!    `gmeow_logic` entrypoint, RUNS it, and asserts its output equals the pinned
//!    falsifiable datum the scenario claims (a transitive subClassOf atom entailed by the
//!    EL→RL→DL closure but absent from the EDB; a section round-trip that discharges its
//!    obligation while a broken section does not; a deterministic entrenchment-ranked
//!    counterfactual outcome; a refutation carrying a concrete witness while a corroboration
//!    carries none; a weakly-acyclic chase whose surfaced ChaseAdmission Finding is non-empty
//!    and whose derivation carries an assert_faithful explanation trace).
//!
//! The five (counter-example, example, failure-class, producer) tuples are READ from the
//! manifest, never hard-coded — a manifest edit that unwires a flagship is caught here.

use gmeow_errors::Severity;
use gmeow_logic::conjecture::conjecture_test;
use gmeow_logic::correspondence_exec::leg_pair_verdict;
use gmeow_logic::counterfactual::construct_and_resolve;
use gmeow_logic::explain::{Row, assert_faithful, explain_one, reifier_from_row};
use gmeow_logic::materialize::{ChaseAdmission, materialize_routed};
use gmeow_logic::provenance::ASSERT_RULE_IRI;
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::reason::reason_all;
use gmeow_logic::store::WorldStore;
use gmeow_logic_compile::ir::{DischargeVerdict, Formula, LegPath, Term};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

mod support;
use support::flagship_discharge::{
    Flagship, FlagshipCtx, SliceSpec, repo_root, run_flagship_discharge,
};

/// The `logic:` grounding namespace — used for the SCANNED failure classes (`logic:<Class>`),
/// which stay slice-namespaced.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// Vocabulary IRIs for the closure / conjecture producers.
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";

/// The `logic:` slice's discharge identity: base IRI, short prefix, on-disk root, and the
/// acceptance-manifest path relative to that root.
fn logic_spec() -> SliceSpec {
    SliceSpec {
        slice_ns: LOGIC_NS,
        slice_prefix: "logic",
        slice_root: repo_root().join("slices").join("grounding").join("logic"),
        manifest_rel: "examples/flagship-acceptance.ttl",
    }
}

#[test]
fn every_flagship_is_discharged_by_execution() {
    run_flagship_discharge(&logic_spec(), 5, &run_producer);
}

/// Dispatch and RUN a `logic:` flagship's `gmeow:demonstratedByProducer`, asserting the
/// executed output equals the pinned falsifiable datum. The logic producers are
/// self-contained native functions, so (like `math:`) the callback needs no pipeline
/// catalog.
fn run_producer(flagship: &Flagship, _ctx: &FlagshipCtx<'_>) {
    match flagship.producer.as_str() {
        // elRlDlClosure: the EL→RL→DL closure entails the transitive subClassOf(A, C) —
        // a DERIVED atom absent from the asserted EDB.
        "logic::reason::reason_all" => run_closure(),
        // correspondenceSection: a genuine inverse discharges the section law; a broken
        // section does not.
        "logic::correspondence_exec::logic_program_verdicts" => run_section(),
        // counterfactualStratumC: overriding the base assumption, the entrenchment-ranked
        // resolution selects the deterministic consequent.
        "logic::counterfactual::construct_and_resolve" => run_counterfactual(),
        // symmetricConjecture: a refutation yields Some(witness); a corroboration yields None.
        "logic::conjecture::conjecture_test" => run_conjecture(),
        // chaseTerminationCertificate: a weakly-acyclic existential chase surfaces a
        // non-empty ChaseAdmission Finding, and the derivation carries an assert_faithful trace.
        "logic::materialize::materialize_routed" => run_chase(),
        other => panic!("unknown logic flagship producer identifier: {other}"),
    }
}

/// Build a single-graph EDB from `(subject, predicate, object)` IRI triples.
fn edb(world: &str, triples: &[(&str, &str, &str)]) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in triples {
        let quad =
            RdfQuad::new(RdfTerm::iri(*s), *p, RdfTerm::iri(*o)).in_graph(RdfTerm::iri(world));
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid EDB dataset")
}

/// FS1 — the EL→RL→DL closure derives the transitive subClassOf(A, C) that the EDB lacks.
fn run_closure() {
    const W: &str = "https://ex/world";
    const A: &str = "https://ex/A";
    const B: &str = "https://ex/B";
    const C: &str = "https://ex/C";

    let store = edb(W, &[(A, SUBCLASS, B), (B, SUBCLASS, C)]);
    let result =
        reason_all(store.as_ref()).expect("FS1: native reason_all must decide the closure");

    // `subject`/`predicate` are bare IRIs; `object` is term_display-ed (angle-bracketed).
    let object_c = format!("<{C}>");
    let derived = result.inferred().iter().any(|ax| {
        !ax.is_edb && ax.predicate == SUBCLASS && ax.subject == A && ax.object == object_c
    });
    assert!(
        derived,
        "FS1: the EL→RL→DL closure must DERIVE the transitive subClassOf(A, C) — absent from \
         the asserted EDB; got {:?}",
        result.inferred()
    );
}

/// FS2 — a genuine get/put inverse discharges the section law; a wrong put does not.
fn run_section() {
    let get = LegPath::Step("https://ex/a".to_owned());
    // put = get.invert(): the executed round-trip recovers the source ⇒ discharged.
    let put = get.invert();
    assert_eq!(
        leg_pair_verdict(&get, &put),
        DischargeVerdict::ObligationDischarged,
        "FS2: a genuine section (put = get.invert()) must discharge its obligation"
    );

    // A put over a DIFFERENT predicate does not invert get ⇒ NOT discharged.
    let wrong_put = LegPath::Step("https://ex/WRONG".to_owned());
    assert_ne!(
        leg_pair_verdict(&get, &wrong_put),
        DischargeVerdict::ObligationDischarged,
        "FS2: a broken section (a wrong put predicate) must NOT discharge its obligation"
    );
}

/// FS3 — the Stratum-C counterfactual resolves to the deterministic entrenchment-ranked
/// consequent. Base: status(server, up); antecedent overwrites it to status(server, down);
/// the rule alert(X, fired) :- status(X, down) fires ⇒ the unique selected outcome is fired.
fn run_counterfactual() {
    const HORN: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

    let store = WorldStore::new();
    store.insert_quad(
        "http://world/base",
        "https://ex/server",
        "https://ex/status",
        "https://ex/up",
    );
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
         :- counterfactual('http://world/cf', 'http://world/base').\n\
         :- assume(ex:status(ex:server, ex:down)).\n\
         ex:alert(X, ex:fired) :- ex:status(X, ex:down).\n\
         ?- ex:alert(ex:server, Z).\n",
    )
    .expect("FS3: the counterfactual program parses");

    let ans = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None)
        .expect("FS3: the Stratum-C counterfactual resolves");
    assert_eq!(
        ans.status_str(),
        "ok",
        "FS3: the counterfactual resolution must be decided; got {ans:?}"
    );
    assert_eq!(
        ans.bindings.len(),
        1,
        "FS3: the entrenchment-ranked resolution selects exactly one deterministic outcome: {ans:?}"
    );
    assert_eq!(
        ans.bindings[0]["Z"], "<https://ex/fired>",
        "FS3: the pinned deterministic entrenchment-ranked outcome is <https://ex/fired>"
    );
}

/// FS4 — a symmetric φ/¬φ conjecture test: a refutation yields a concrete witness (Some),
/// a corroboration yields None.
fn run_conjecture() {
    const SCN: &str = "http://world/scenario";
    const STANDPOINT: &str = "http://world/standpoint/alice";
    const IND_A: &str = "http://ex/a";
    const A_CLS: &str = "http://ex/A";
    const B_CLS: &str = "http://ex/B";

    let candidate = Formula::atom(
        Term::iri(TYPE.to_owned()).unwrap(),
        vec![
            Term::iri(IND_A.to_owned()).unwrap(),
            Term::iri(B_CLS.to_owned()).unwrap(),
        ],
    )
    .expect("FS4: the candidate atom builds");

    // Refutation: a:A, A disjointWith B ⇒ asserting a:B forces a into owl:Nothing.
    let refuting = edb(SCN, &[(IND_A, TYPE, A_CLS), (A_CLS, DISJOINT, B_CLS)]);
    let refuted = conjecture_test(
        refuting.as_ref(),
        SCN,
        &candidate,
        STANDPOINT,
        &[],
        &Budget::default(),
    )
    .expect("FS4: the refutation conjecture test runs");
    let witness = refuted
        .witness
        .as_ref()
        .expect("FS4: a refutation MUST yield a concrete contradiction witness (Some)");
    assert_eq!(
        witness.individual, IND_A,
        "FS4: the refutation witness names the individual forced to owl:Nothing"
    );
    assert!(
        !witness.premises.is_empty(),
        "FS4: the refutation witness carries the jointly-inconsistent premises"
    );

    // Corroboration: a:A, A subClassOf B ⇒ DL derives a:B, so φ is redundant ⇒ no witness.
    let corroborating = edb(SCN, &[(IND_A, TYPE, A_CLS), (A_CLS, SUBCLASS, B_CLS)]);
    let corroborated = conjecture_test(
        corroborating.as_ref(),
        SCN,
        &candidate,
        STANDPOINT,
        &[],
        &Budget::default(),
    )
    .expect("FS4: the corroboration conjecture test runs");
    assert!(
        corroborated.witness.is_none(),
        "FS4: a corroboration carries NO witness (None); got {:?}",
        corroborated.witness
    );
}

/// FS5 — a weakly-acyclic existential chase: `materialize_routed` surfaces the ChaseAdmission
/// termination certificate as a non-empty gmeow:Finding, and the derivation carries an
/// explanation trace that passes `assert_faithful` (its cited witness IRIs are present).
fn run_chase() {
    // A weakly-acyclic value-inventing program: for each typed individual, invent a fresh
    // ancestor. The existential head predicate never feeds a body position, so no existential
    // edge lies in a cycle ⇒ the chase is certified terminating.
    let rules = "<https://ex/hasAncestor>(?x, !y, ?w) :- \
                 <https://ex/hasType>(?x, <https://ex/Person>, ?w) .";
    let input = "<https://ex/alice> <https://ex/hasType> <https://ex/Person> <https://world/W> .\n";

    let m = materialize_routed(rules, input, Some(64), None, None, None)
        .expect("FS5: the existential chase materializes");

    // The termination certificate is surfaced PUBLICLY on the materialization result.
    let admission = m
        .chase_admission
        .as_ref()
        .expect("FS5: the existential chase must surface a ChaseAdmission certificate");
    assert!(
        matches!(admission, ChaseAdmission::WeaklyAcyclic { .. }),
        "FS5: the worked program is weakly acyclic (certified terminating); got {admission:?}"
    );

    // ChaseAdmission::to_finding() is the first-class gmeow:Finding — non-empty evidence,
    // informational severity, the weakly-acyclic certificate code.
    let finding = admission.to_finding();
    assert_eq!(
        finding.severity,
        Severity::Info,
        "FS5: a certified chase is an informational finding"
    );
    assert_eq!(
        finding.code, "chase.certificate.weakly-acyclic",
        "FS5: the surfaced certificate carries the weakly-acyclic code"
    );
    assert!(
        !finding.message.trim().is_empty(),
        "FS5: the surfaced gmeow:Finding must be non-empty (it carries the proof evidence)"
    );

    // The derivation carries a faithful explanation trace whose cited witness IRIs are all
    // present in the trace set — assert_faithful rejects any fabricated witness.
    assert_chase_derivation_is_faithfully_explained();
}

/// Build a small faithful derivation (two asserted facts + one derived quad citing their
/// reifiers) and assert that `explain_one` reconstructs it and `assert_faithful` accepts it
/// with a non-empty set of cited witness IRIs — the explain-trace half of the FS5 datum.
fn assert_chase_derivation_is_faithfully_explained() {
    let graph = "https://ex/world".to_owned();
    let subof = "https://ex/subOf".to_owned();
    let (a, b, c) = ("https://ex/A", "https://ex/B", "https://ex/C");

    // Two asserted facts. Their source_quad_ids are their own reifiers (the asserted-leaf
    // convention); the reifier depends only on subject/predicate/obj, so compute it first.
    let mut ab = Row {
        graph: graph.clone(),
        subject: a.to_owned(),
        predicate: subof.clone(),
        obj: format!("<{b}>"),
        derivation_id: "d-ab".to_owned(),
        rule_iri: ASSERT_RULE_IRI.to_owned(),
        source_quad_ids: vec![],
    };
    let r_ab = reifier_from_row(&ab);
    ab.source_quad_ids = vec![r_ab.clone()];

    let mut bc = Row {
        graph: graph.clone(),
        subject: b.to_owned(),
        predicate: subof.clone(),
        obj: format!("<{c}>"),
        derivation_id: "d-bc".to_owned(),
        rule_iri: ASSERT_RULE_IRI.to_owned(),
        source_quad_ids: vec![],
    };
    let r_bc = reifier_from_row(&bc);
    bc.source_quad_ids = vec![r_bc.clone()];

    // The derived transitive quad cites the two asserted reifiers as its antecedents.
    let ac = Row {
        graph: graph.clone(),
        subject: a.to_owned(),
        predicate: subof.clone(),
        obj: format!("<{c}>"),
        derivation_id: "d-ac".to_owned(),
        rule_iri: "https://ex/ruleTransitiveSubOf".to_owned(),
        source_quad_ids: vec![r_ab, r_bc],
    };

    let rows = vec![ab, bc, ac];
    let expl = explain_one(&rows, 2).expect("FS5: the derived quad's explanation reconstructs");
    assert!(
        !expl.cited_iris.is_empty(),
        "FS5: the explanation must cite witness IRIs"
    );
    assert_faithful(&expl, &rows)
        .expect("FS5: every cited witness IRI must be present in the derivation trace");
}
