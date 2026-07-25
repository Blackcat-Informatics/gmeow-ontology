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
use gmeow_logic::conjecture::{ConjectureLifecycleState, conjecture_test};
use gmeow_logic::correspondence_exec::leg_pair_verdict;
use gmeow_logic::counterfactual::construct_and_resolve;
use gmeow_logic::explain::{Row, assert_faithful, explain_one};
use gmeow_logic::materialize::{
    ChaseAdmission, MaterializationLimits, StructuredAtom, StructuredExistentialRule,
    StructuredTerm, materialize_existential_rules,
};
use gmeow_logic::provenance::{ASSERT_RULE_IRI, term_display};
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::reason::reason_all;
use gmeow_logic::seam::DerivedQuad;
use gmeow_logic::store::WorldStore;
use gmeow_logic_compile::ir::{DischargeVerdict, Formula, LegPath, Term};
use gmeow_validate::store::parse_file_dataset;
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};

mod support;
use support::flagship_discharge::{
    CounterExampleExecution, Flagship, FlagshipCtx, SliceSpec, assert_counterexample_depth,
    flagship_error, parse_manifest, repo_root, run_flagship_discharge_with_counterexample,
};

/// The `logic:` grounding namespace — used for the SCANNED failure classes (`logic:<Class>`),
/// which stay slice-namespaced.
use gmeow_ns::LOGIC_NS;

/// Vocabulary IRIs for the closure / conjecture producers.
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const FIXTURE_NS: &str = "http://example.org/logic/";

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
    run_flagship_discharge_with_counterexample(
        &logic_spec(),
        5,
        &run_producer,
        &run_counterexample,
    );
}

fn write_manifest(marker_rows: &str) -> (tempfile::TempDir, SliceSpec) {
    let temp = tempfile::tempdir().expect("temporary manifest directory");
    std::fs::write(
        temp.path().join("manifest.ttl"),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:scenario a gmeow:FlagshipScenario ;\n\
               gmeow:demonstratedByExample \"example.ttl\" ;\n\
               gmeow:guardedByCounterExample \"counter.ttl\" ;\n\
               gmeow:enforcesFailureClass logic:IncompleteClosure ;\n\
               gmeow:demonstratedByProducer \"logic::reason::reason_all\" ;\n\
               {marker_rows}\n"
        ),
    )
    .expect("write temporary manifest");
    let spec = SliceSpec {
        slice_ns: LOGIC_NS,
        slice_prefix: "logic",
        slice_root: temp.path().to_path_buf(),
        manifest_rel: "manifest.ttl",
    };
    (temp, spec)
}

#[test]
#[should_panic(expected = "duplicate gmeow:counterExampleDischarge")]
fn duplicate_discharge_marker_hard_fails() {
    let (_temp, spec) = write_manifest(
        "gmeow:counterExampleDischarge gmeow:structuralDischarge ;\n\
         gmeow:counterExampleDischarge gmeow:reasonerDrivenDischarge .",
    );
    let _ = parse_manifest(&spec, 1);
}

#[test]
#[should_panic(expected = "unknown gmeow:counterExampleDischarge marker")]
fn unknown_discharge_marker_hard_fails() {
    let (_temp, spec) =
        write_manifest("gmeow:counterExampleDischarge <https://example.org/unknownDischarge> .");
    let _ = parse_manifest(&spec, 1);
}

#[test]
#[should_panic(expected = "missing gmeow:counterExampleDischarge")]
fn missing_discharge_marker_hard_fails() {
    let (_temp, spec) = write_manifest("<https://example.org/unrelated> true .");
    let _ = parse_manifest(&spec, 1);
}

#[test]
#[should_panic(expected = "counter-example marker/execution mismatch")]
fn reasoner_marker_with_structural_execution_hard_fails() {
    let flagships = parse_manifest(&logic_spec(), 5);
    assert_counterexample_depth(&flagships[0], CounterExampleExecution::StructuralProxy);
}

#[test]
#[should_panic(expected = "must observe EXACTLY")]
fn reasoner_execution_with_wrong_failure_set_hard_fails() {
    let flagships = parse_manifest(&logic_spec(), 5);
    assert_counterexample_depth(
        &flagships[0],
        CounterExampleExecution::ReasonerDriven(
            std::iter::once("WrongFailure".to_owned()).collect(),
        ),
    );
}

/// Run the malformed half of a flagship through the SAME native producer as its positive
/// fixture. Parse/capability/budget/infrastructure failures return `Err` and therefore can never
/// masquerade as the expected semantic failure class.
fn run_counterexample(
    flagship: &Flagship,
    _ctx: &FlagshipCtx<'_>,
) -> gmeow_errors::Result<CounterExampleExecution> {
    assert_fixture_producer(&flagship.example, &flagship.producer)?;
    assert_fixture_producer(&flagship.counter_example, &flagship.producer)?;
    assert_fixture_failure(&flagship.counter_example, &flagship.failure_class)?;
    let failure = match flagship.producer.as_str() {
        "logic::reason::reason_all" => run_incomplete_closure()?,
        "logic::correspondence_exec::leg_pair_verdict" => run_broken_section()?,
        "logic::counterfactual::construct_and_resolve" => run_unentrenched_counterfactual()?,
        "logic::conjecture::conjecture_test" => run_unwitnessed_refutation()?,
        "logic::materialize::materialize_existential_rules" => run_uncertified_chase()?,
        other => {
            return Err(flagship_error(format!(
                "unknown logic flagship producer identifier: {other}"
            )));
        }
    };
    Ok(CounterExampleExecution::ReasonerDriven(
        std::iter::once(failure.to_owned()).collect(),
    ))
}

/// Dispatch and RUN a `logic:` flagship's `gmeow:demonstratedByProducer`, asserting the
/// executed output equals the pinned falsifiable datum. The logic producers are
/// self-contained native functions, so (like `math:`) the callback needs no pipeline
/// catalog.
fn run_producer(flagship: &Flagship, _ctx: &FlagshipCtx<'_>) {
    assert_fixture_producer(&flagship.example, &flagship.producer)
        .unwrap_or_else(|detail| panic!("flagship {}: {detail}", flagship.subject));
    match flagship.producer.as_str() {
        // elRlDlClosure: the EL→RL→DL closure entails the transitive subClassOf(A, C) —
        // a DERIVED atom absent from the asserted EDB.
        "logic::reason::reason_all" => run_closure(),
        // correspondenceSection: a genuine inverse discharges the section law; a broken
        // section does not.
        "logic::correspondence_exec::leg_pair_verdict" => run_section(),
        // counterfactualStratumC: overriding the base assumption, the entrenchment-ranked
        // resolution selects the deterministic consequent.
        "logic::counterfactual::construct_and_resolve" => run_counterfactual(),
        // symmetricConjecture: a refutation yields Some(witness); a corroboration yields None.
        "logic::conjecture::conjecture_test" => run_conjecture(),
        // chaseTerminationCertificate: a weakly-acyclic existential chase surfaces a
        // non-empty ChaseAdmission Finding, and the derivation carries an assert_faithful trace.
        "logic::materialize::materialize_existential_rules" => run_chase(),
        other => panic!("unknown logic flagship producer identifier: {other}"),
    }
}

/// Read one IRI or literal value for a fixture predicate, rejecting missing and duplicate
/// contract fields. The fixture declarations are the ontology-data tie between the manifest and
/// the exact native producer run by this harness.
fn fixture_value(path: &std::path::Path, predicate: &str) -> gmeow_errors::Result<String> {
    let ds = parse_file_dataset(path)
        .map_err(|e| flagship_error(format!("fixture {} parses: {e}", path.display())))?;
    let mut values = Vec::new();
    for quad in ds.owned_quads() {
        if quad.predicate != predicate {
            continue;
        }
        match &quad.object {
            RdfTerm::Iri(iri) => values.push(iri.clone()),
            RdfTerm::Literal(lit) => values.push(lit.lexical_form.clone()),
            other => {
                return Err(flagship_error(format!(
                    "fixture {} predicate <{predicate}> has non-IRI/non-literal value {other:?}",
                    path.display()
                )));
            }
        }
    }
    match values.as_slice() {
        [value] => Ok(value.clone()),
        [] => Err(flagship_error(format!(
            "fixture {} is missing <{predicate}>",
            path.display()
        ))),
        _ => Err(flagship_error(format!(
            "fixture {} has duplicate <{predicate}> values {values:?}",
            path.display()
        ))),
    }
}

fn assert_fixture_producer(path: &std::path::Path, expected: &str) -> gmeow_errors::Result<()> {
    let actual = fixture_value(path, &format!("{FIXTURE_NS}executesNativeProducer"))?;
    if actual == expected {
        Ok(())
    } else {
        Err(flagship_error(format!(
            "fixture {} names native producer {actual:?}, manifest names {expected:?}",
            path.display()
        )))
    }
}

fn assert_fixture_failure(
    path: &std::path::Path,
    expected_local: &str,
) -> gmeow_errors::Result<()> {
    let actual = fixture_value(path, &format!("{FIXTURE_NS}observesRuntimeFailure"))?;
    let expected = format!("{LOGIC_NS}{expected_local}");
    if actual == expected {
        Ok(())
    } else {
        Err(flagship_error(format!(
            "fixture {} names runtime failure <{actual}>, manifest names <{expected}>",
            path.display()
        )))
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

/// FS1 negative — the closure completes normally but the demanded transitive atom is absent
/// because one premise is missing. A reasoner error is infrastructure; only the decided absence
/// is `logic:IncompleteClosure`.
fn run_incomplete_closure() -> gmeow_errors::Result<&'static str> {
    const W: &str = "https://ex/world";
    const A: &str = "https://ex/A";
    const B: &str = "https://ex/B";
    const C: &str = "https://ex/C";
    let store = edb(W, &[(A, SUBCLASS, B)]);
    let result = reason_all(store.as_ref()).map_err(|e| flagship_error(e.to_string()))?;
    let object_c = format!("<{C}>");
    if result
        .inferred()
        .iter()
        .any(|ax| ax.predicate == SUBCLASS && ax.subject == A && ax.object == object_c)
    {
        return Err(flagship_error(
            "malformed closure unexpectedly entailed the demanded A subClassOf C atom",
        ));
    }
    Ok("IncompleteClosure")
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

/// FS2 negative — execute the same native section-law evaluator over a broken put leg.
fn run_broken_section() -> gmeow_errors::Result<&'static str> {
    let get = LegPath::Step("https://ex/a".to_owned());
    let wrong_put = LegPath::Step("https://ex/WRONG".to_owned());
    match leg_pair_verdict(&get, &wrong_put) {
        DischargeVerdict::ObligationViolated => Ok("SectionObligationViolated"),
        other => Err(flagship_error(format!(
            "broken get/put section returned {other:?}, expected ObligationViolated"
        ))),
    }
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

/// FS3 negative — two incomparable antecedent values are a genuine unentrenched tie. The native
/// counterfactual engine must decide `unknown` with no selected binding; `incomplete` is a budget
/// failure and is deliberately rejected rather than translated to the expected failure class.
fn run_unentrenched_counterfactual() -> gmeow_errors::Result<&'static str> {
    const HORN: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    let store = WorldStore::new();
    store.insert_quad(
        "http://world/base",
        "https://ex/seed",
        "https://ex/p",
        "https://ex/o",
    );
    let prog = parse_query_program(
        ":- prefix(ex, 'https://ex/').\n\
         :- counterfactual('http://world/cf', 'http://world/base').\n\
         :- assume(ex:flag(ex:x, ex:blue)).\n\
         :- assume(ex:flag(ex:x, ex:green)).\n\
         ?- ex:flag(ex:x, Z).\n",
    )
    .map_err(|e| flagship_error(e.to_string()))?;
    let answer = construct_and_resolve(&store, &prog, HORN, &Budget::default(), 4, None)
        .map_err(|e| flagship_error(e.to_string()))?;
    match answer.status_str() {
        "unknown" if answer.bindings.is_empty() => Ok("UnentrenchedCounterfactual"),
        "incomplete" => Err(flagship_error("counterfactual negative exhausted a budget")),
        status => Err(flagship_error(format!(
            "unentrenched counterfactual returned status {status:?} and bindings {:?}",
            answer.bindings
        ))),
    }
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

/// FS4 negative — execute a claimed refutation against a consistent KB that provides no clash.
/// The native conjecture engine completes in the open state with no contradiction witness.
fn run_unwitnessed_refutation() -> gmeow_errors::Result<&'static str> {
    const SCN: &str = "http://world/scenario";
    const STANDPOINT: &str = "http://world/standpoint/alice";
    const IND_A: &str = "http://ex/a";
    const A_CLS: &str = "http://ex/A";
    const B_CLS: &str = "http://ex/B";
    let candidate = Formula::atom(
        Term::iri(TYPE.to_owned()).map_err(|e| flagship_error(e.to_string()))?,
        vec![
            Term::iri(IND_A.to_owned()).map_err(|e| flagship_error(e.to_string()))?,
            Term::iri(B_CLS.to_owned()).map_err(|e| flagship_error(e.to_string()))?,
        ],
    )
    .map_err(|e| flagship_error(e.to_string()))?;
    let kb = edb(SCN, &[(IND_A, TYPE, A_CLS)]);
    let answer = conjecture_test(
        kb.as_ref(),
        SCN,
        &candidate,
        STANDPOINT,
        &[],
        &Budget::default(),
    )
    .map_err(|e| flagship_error(e.to_string()))?;
    if answer.lifecycle == ConjectureLifecycleState::Open && answer.witness.is_none() {
        Ok("UnwitnessedRefutation")
    } else {
        Err(flagship_error(format!(
            "witness-free refutation negative returned lifecycle {:?} and witness {:?}",
            answer.lifecycle, answer.witness
        )))
    }
}

/// FS5 — a weakly-acyclic existential chase: the typed materializer surfaces the ChaseAdmission
/// termination certificate as a non-empty gmeow:Finding, and the derivation carries an
/// explanation trace that passes `assert_faithful` (its cited witness IRIs are present).
fn run_chase() {
    // A weakly-acyclic value-inventing program: for each typed individual, invent a fresh
    // ancestor. The existential head predicate never feeds a body position, so no existential
    // edge lies in a cycle ⇒ the chase is certified terminating.
    let input = "<https://ex/alice> <https://ex/hasType> <https://ex/Person> <https://world/W> .\n";
    let rules = vec![StructuredExistentialRule {
        rule_iri: "https://ex/rule/ancestor".to_owned(),
        body: vec![StructuredAtom::new(
            StructuredTerm::var("?x"),
            "https://ex/hasType",
            StructuredTerm::named("https://ex/Person"),
        )],
        head: vec![StructuredAtom::new(
            StructuredTerm::var("?x"),
            "https://ex/hasAncestor",
            StructuredTerm::var("?y"),
        )],
        distinct: Vec::new(),
        witness_frontier: None,
    }];
    let dataset = purrdf::parse_dataset(input.as_bytes(), "application/n-quads", None)
        .expect("FS5: input parses");
    let m = materialize_existential_rules(
        dataset.as_ref(),
        &rules,
        MaterializationLimits {
            max_steps: Some(64),
        },
    )
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

    // The derivation carries a faithful explanation trace over the materializer's OWN
    // output — never a fabricated stand-in trace. Convert the chase's real derived quads to
    // the explain surface's `Row`s (reproducing, not re-deriving, the SAME reifier
    // materializer already minted into `source_quad_ids`), locate the invented
    // `hasAncestor` quad the chase actually derived, and assert its explanation is non-empty
    // and faithful — assert_faithful rejects any fabricated witness.
    let rows: Vec<Row> = m.quads.iter().map(quad_to_row).collect();
    let target = rows
        .iter()
        .position(|row| row.rule_iri != ASSERT_RULE_IRI)
        .expect("FS5: the chase must have derived at least one non-asserted (IDB) quad");
    let expl = explain_one(&rows, target)
        .expect("FS5: the real chase-derived quad's explanation must reconstruct");
    assert!(
        !expl.cited_iris.is_empty(),
        "FS5: the explanation over the real chase output must cite witness IRIs"
    );
    assert_faithful(&expl, &rows).expect(
        "FS5: every witness IRI cited in the real chase's explanation must be present in the \
         derivation trace",
    );
}

/// FS5 negative — the existential position graph contains a cycle. The same routed materializer
/// returns the native `Uncertified` admission (while its terminating facts-only oracle leg keeps
/// the fixture executable); a parse failure, unsupported input, or missing admission is an error.
fn run_uncertified_chase() -> gmeow_errors::Result<&'static str> {
    let input = "<http://ex/a> \
                 <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                 <http://ex/C> <http://world/W> .\n";
    let rules = vec![
        StructuredExistentialRule {
            rule_iri: "http://ex/rule/invent".to_owned(),
            body: vec![StructuredAtom::new(
                StructuredTerm::var("?x"),
                TYPE,
                StructuredTerm::named("http://ex/C"),
            )],
            head: vec![StructuredAtom::new(
                StructuredTerm::var("?x"),
                "http://ex/p",
                StructuredTerm::var("?y"),
            )],
            distinct: Vec::new(),
            witness_frontier: None,
        },
        StructuredExistentialRule {
            rule_iri: "http://ex/rule/recur".to_owned(),
            body: vec![StructuredAtom::new(
                StructuredTerm::named("http://ex/a"),
                "http://ex/p",
                StructuredTerm::var("?z"),
            )],
            head: vec![StructuredAtom::new(
                StructuredTerm::var("?z"),
                TYPE,
                StructuredTerm::named("http://ex/C"),
            )],
            distinct: Vec::new(),
            witness_frontier: None,
        },
    ];
    let dataset = purrdf::parse_dataset(input.as_bytes(), "application/n-quads", None)
        .map_err(|e| flagship_error(e.to_string()))?;
    let result =
        materialize_existential_rules(dataset.as_ref(), &rules, MaterializationLimits::default())
            .map_err(|e| flagship_error(e.to_string()))?;
    match result.chase_admission.as_ref() {
        Some(ChaseAdmission::Uncertified { violations }) if !violations.is_empty() => {
            Ok("UncertifiedChase")
        }
        Some(other) => Err(flagship_error(format!(
            "cyclic existential chase returned the wrong admission: {other:?}"
        ))),
        None => Err(flagship_error(
            "cyclic existential chase returned no admission certificate",
        )),
    }
}

/// Convert a chase-materialized [`DerivedQuad`] into the `explain` surface's [`Row`]: the bare
/// subject/predicate IRIs plus the `term_display`-rendered object N3 form reproduce, byte for
/// byte, the SAME `mint_reifier` recipe the typed materializer used to mint this quad's
/// entries in `source_quad_ids` — this is a second READER of that one provenance scheme, not a
/// second scheme.
fn quad_to_row(dq: &DerivedQuad) -> Row {
    let subject = match &dq.subject {
        TermValue::Iri(iri) => iri.clone(),
        other => panic!(
            "FS5: a world-scoped chase quad's subject must be an IRI (or Skolem IRI); got {other:?}"
        ),
    };
    Row {
        graph: dq.graph.clone(),
        subject,
        predicate: dq.predicate.clone(),
        obj: term_display(&dq.object),
        derivation_id: dq.derivation_id.as_str().to_owned(),
        rule_iri: dq.rule_iri.clone(),
        source_quad_ids: dq.source_quad_ids.clone(),
    }
}
