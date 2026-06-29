// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the native transaction-program interpreter.
//!
//! Each test builds a small N-Quads world declaring a transaction program, parses it,
//! and executes it under executional entailment — asserting the executed path, the
//! supersession steps, and that every malformed program / non-terminating loop is a hard
//! error rather than a silent default.

use super::*;
use crate::store::WorldStore;
use crate::teleology::WorldFacts;

const W: &str = "https://blackcatinformatics.ca/gmeow/examples/tx/world";

/// A `logic:` IRI in N-Quads angle-bracket form.
fn l(local: &str) -> String {
    format!("<https://blackcatinformatics.ca/logic/{local}>")
}

/// An example-namespaced IRI in angle-bracket form.
fn e(local: &str) -> String {
    format!("<{W}#{local}>")
}

/// Read the world's facts from N-Quads text.
fn facts_of(nq: &str) -> WorldFacts {
    let store = WorldStore::new();
    store.load_nquads(nq).expect("valid N-Quads");
    WorldFacts::read(&store, W)
}

/// One N-Quad line in world `W`.
fn q(s: &str, p: &str, o: &str) -> String {
    format!("{s} {p} {o} <{W}> .\n")
}

/// A start state `start` bearing the given obtaining situation locals.
fn start_state(state: &str, obtains: &[&str]) -> String {
    let mut s = String::new();
    for sit in obtains {
        s.push_str(&q(&e(state), &l("situationObtains"), &e(sit)));
    }
    s
}

/// A primitive leaf `node` instantiating a schema with the given precondition locals and
/// ins/del effect locals.
fn primitive(node: &str, preconds: &[&str], ins: &[&str], del: &[&str]) -> String {
    let schema = format!("{node}Schema");
    let effect = format!("{node}Effect");
    let mut s = String::new();
    s.push_str(&q(&e(node), &l("instantiatesSchema"), &e(&schema)));
    s.push_str(&q(&e(&schema), &l("effect"), &e(&effect)));
    for p in preconds {
        s.push_str(&q(&e(&schema), &l("precondition"), &e(p)));
    }
    for i in ins {
        s.push_str(&q(&e(&effect), &l("ins"), &e(i)));
    }
    for d in del {
        s.push_str(&q(&e(&effect), &l("del"), &e(d)));
    }
    s
}

/// Parse and execute the program rooted at `root_local` from its declared start state.
fn run(nq: &str, root_local: &str) -> Result<ExecOutcome, String> {
    let facts = facts_of(nq);
    let root = format!("{W}#{root_local}");
    let (start, sits) = root_start(&facts, &root)?;
    let prog = parse_program(&facts, &root, 0)?;
    let mut counter = StepCounter::new();
    plan_path(&facts, &prog, &start, &sits, &root, &mut counter)
}

// ── Parser hard-fails (no-optionality) ──────────────────────────────────────────

#[test]
fn parse_rejects_unknown_node() {
    // A node with no combinator type and no instantiatesSchema is neither.
    let nq = q(&e("bare"), &l("guardSituation"), &e("g"));
    let facts = facts_of(&nq);
    let err = parse_program(&facts, &format!("{W}#bare"), 0).unwrap_err();
    assert!(
        err.contains("neither a recognized combinator nor a primitive"),
        "{err}"
    );
}

#[test]
fn parse_hard_fails_on_concurrent_composition() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}",
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        q(&e("cc"), &l("leftOperand"), &e("a")),
        q(&e("cc"), &l("rightOperand"), &e("b")),
    );
    let facts = facts_of(&nq);
    let err = parse_program(&facts, &format!("{W}#cc"), 0).unwrap_err();
    assert!(err.contains("ConcurrentComposition"), "{err}");
    assert!(err.contains("separate concern"), "{err}");
    // The hard-fail message names the concern, never an issue/PR number.
    assert!(!err.contains("711") && !err.contains("714"), "{err}");
}

#[test]
fn parse_rejects_doubled_left_operand() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        q(&e("ser"), ty, &l("SerialConjunction")),
        q(&e("ser"), &l("leftOperand"), &e("a")),
        q(&e("ser"), &l("leftOperand"), &e("b")), // doubled
        q(&e("ser"), &l("rightOperand"), &e("c")),
    );
    let facts = facts_of(&nq);
    let err = parse_program(&facts, &format!("{W}#ser"), 0).unwrap_err();
    assert!(err.contains("leftOperand"), "{err}");
    assert!(err.contains("exactly one required"), "{err}");
}

#[test]
fn parse_rejects_missing_right_operand() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}",
        q(&e("ser"), ty, &l("SerialConjunction")),
        q(&e("ser"), &l("leftOperand"), &e("a")),
    );
    let facts = facts_of(&nq);
    let err = parse_program(&facts, &format!("{W}#ser"), 0).unwrap_err();
    assert!(err.contains("rightOperand"), "{err}");
}

#[test]
fn parse_rejects_multiple_combinator_types() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}",
        q(&e("x"), ty, &l("SerialConjunction")),
        q(&e("x"), ty, &l("Fallback")),
    );
    let facts = facts_of(&nq);
    let err = parse_program(&facts, &format!("{W}#x"), 0).unwrap_err();
    assert!(err.contains("more than one combinator type"), "{err}");
}

#[test]
fn parse_bounds_operand_cycle_depth() {
    // A SerialConjunction whose left operand is itself: parse recurses until the depth
    // bound, then hard-fails (no stack overflow).
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}",
        q(&e("ser"), ty, &l("SerialConjunction")),
        q(&e("ser"), &l("leftOperand"), &e("ser")), // self-cycle
        q(&e("ser"), &l("rightOperand"), &e("ser")),
    );
    let facts = facts_of(&nq);
    let err = parse_program(&facts, &format!("{W}#ser"), 0).unwrap_err();
    assert!(err.contains("exceeds depth"), "{err}");
}

// ── Serial conjunction: path-splitting ──────────────────────────────────────────

#[test]
fn serial_conjunction_splits_the_path() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}{}",
        start_state("s0", &[]),
        q(&e("ser"), ty, &l("SerialConjunction")),
        q(&e("ser"), &l("transitionFromState"), &e("s0")),
        // φ: primA asserts sitX (no precondition); ψ: primB requires sitX, asserts sitY.
        [
            q(&e("ser"), &l("leftOperand"), &e("primA")),
            q(&e("ser"), &l("rightOperand"), &e("primB")),
        ]
        .concat(),
        [
            primitive("primA", &[], &["sitX"], &[]),
            primitive("primB", &["sitX"], &["sitY"], &[]),
        ]
        .concat(),
    );
    let out = run(&nq, "ser").expect("serial succeeds");
    assert!(out.succeeded());
    // Three states: start, mid (after φ), end (after ψ) — the split is at mid.
    assert_eq!(out.path.len(), 3, "path = {:?}", out.path);
    assert_eq!(out.steps.len(), 2);
    assert!(out.sits_end.contains(&format!("{W}#sitX")));
    assert!(out.sits_end.contains(&format!("{W}#sitY")));
}

#[test]
fn serial_fails_when_second_leg_precondition_unmet() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}{}{}",
        start_state("s0", &[]),
        q(&e("ser"), ty, &l("SerialConjunction")),
        q(&e("ser"), &l("transitionFromState"), &e("s0")),
        q(&e("ser"), &l("leftOperand"), &e("primA")),
        q(&e("ser"), &l("rightOperand"), &e("primB")),
        // primA asserts sitX; primB requires sitNeverSet → ψ fails → whole serial fails.
        [
            primitive("primA", &[], &["sitX"], &[]),
            primitive("primB", &["sitNeverSet"], &["sitY"], &[]),
        ]
        .concat(),
    );
    let out = run(&nq, "ser").expect("no hard error");
    assert!(
        !out.succeeded(),
        "serial must fail when ψ precondition is unmet"
    );
    assert!(out.path.is_empty());
}

// ── Choice: guarded dispatch ────────────────────────────────────────────────────

fn choice_world(guard_obtains: bool) -> String {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let start = if guard_obtains {
        start_state("s0", &["sitG"])
    } else {
        start_state("s0", &[])
    };
    format!(
        "{}{}{}{}{}{}{}",
        start,
        q(&e("ch"), ty, &l("Choice")),
        q(&e("ch"), &l("transitionFromState"), &e("s0")),
        q(&e("ch"), &l("guardSituation"), &e("sitG")),
        q(&e("ch"), &l("leftOperand"), &e("primL")),
        q(&e("ch"), &l("rightOperand"), &e("primR")),
        [
            primitive("primL", &[], &["sit_lft"], &[]),
            primitive("primR", &[], &["sit_rgt"], &[]),
        ]
        .concat(),
    )
}

#[test]
fn choice_guard_true_takes_left() {
    let out = run(&choice_world(true), "ch").expect("choice succeeds");
    assert!(out.succeeded());
    assert_eq!(out.steps.len(), 1);
    // The left branch ran: its schema was instantiated, and the step is a minted runtime
    // logic:TransactionStep (no longer the static program node).
    assert_eq!(out.steps[0].schema, format!("{W}#primLSchema"));
    assert!(out.steps[0]
        .attribution
        .starts_with("https://blackcatinformatics.ca/logic/step/"));
    assert!(out.sits_end.contains(&format!("{W}#sit_lft")));
    assert!(!out.sits_end.contains(&format!("{W}#sit_rgt")));
}

#[test]
fn choice_guard_false_takes_right() {
    let out = run(&choice_world(false), "ch").expect("choice succeeds");
    assert!(out.succeeded());
    assert_eq!(out.steps.len(), 1);
    assert_eq!(out.steps[0].schema, format!("{W}#primRSchema"));
    assert!(out.steps[0]
        .attribution
        .starts_with("https://blackcatinformatics.ca/logic/step/"));
    assert!(out.sits_end.contains(&format!("{W}#sit_rgt")));
}

// ── Fallback: try-else on executional-entailment failure ────────────────────────

#[test]
fn fallback_runs_backup_when_primary_fails() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}{}{}",
        start_state("s0", &[]),
        q(&e("fb"), ty, &l("Fallback")),
        q(&e("fb"), &l("transitionFromState"), &e("s0")),
        q(&e("fb"), &l("leftOperand"), &e("primPrimary")),
        q(&e("fb"), &l("rightOperand"), &e("primBackup")),
        // primary requires a missing precondition → fails; backup unconditional.
        [
            primitive("primPrimary", &["sitMissing"], &["sitNever"], &[]),
            primitive("primBackup", &[], &["sitOk"], &[]),
        ]
        .concat(),
    );
    let out = run(&nq, "fb").expect("fallback succeeds via backup");
    assert!(out.succeeded());
    // Only the backup emitted a step — the failed primary produced nothing (no rollback).
    assert_eq!(out.steps.len(), 1);
    assert_eq!(out.steps[0].schema, format!("{W}#primBackupSchema"));
    assert!(out.steps[0]
        .attribution
        .starts_with("https://blackcatinformatics.ca/logic/step/"));
    assert!(out.sits_end.contains(&format!("{W}#sitOk")));
}

#[test]
fn fallback_fails_when_both_branches_fail() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}{}{}",
        start_state("s0", &[]),
        q(&e("fb"), ty, &l("Fallback")),
        q(&e("fb"), &l("transitionFromState"), &e("s0")),
        q(&e("fb"), &l("leftOperand"), &e("primPrimary")),
        q(&e("fb"), &l("rightOperand"), &e("primBackup")),
        [
            primitive("primPrimary", &["sitMissing"], &[], &[]),
            primitive("primBackup", &["sitAlsoMissing"], &[], &[]),
        ]
        .concat(),
    );
    let out = run(&nq, "fb").expect("no hard error");
    assert!(
        !out.succeeded(),
        "fallback must fail when both branches fail"
    );
    assert!(
        out.path.is_empty(),
        "failure leaves the start untouched (empty path)"
    );
}

// ── Iteration: while-condition loop + termination bound ──────────────────────────

#[test]
fn iteration_runs_until_condition_clears() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    // Condition sitLoop holds at start; the body deletes it → exactly one pass, then stop.
    let nq = format!(
        "{}{}{}{}{}{}",
        start_state("s0", &["sitLoop"]),
        q(&e("it"), ty, &l("Iteration")),
        q(&e("it"), &l("transitionFromState"), &e("s0")),
        q(&e("it"), &l("iterationCondition"), &e("sitLoop")),
        q(&e("it"), &l("iterationBody"), &e("primBody")),
        primitive("primBody", &[], &[], &["sitLoop"]),
    );
    let out = run(&nq, "it").expect("iteration terminates");
    assert!(out.succeeded());
    assert_eq!(out.steps.len(), 1, "exactly one body pass");
    assert_eq!(out.path.len(), 2);
    assert!(!out.sits_end.contains(&format!("{W}#sitLoop")));
}

#[test]
fn iteration_zero_passes_when_condition_false_at_start() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}{}{}",
        start_state("s0", &[]), // sitLoop absent
        q(&e("it"), ty, &l("Iteration")),
        q(&e("it"), &l("transitionFromState"), &e("s0")),
        q(&e("it"), &l("iterationCondition"), &e("sitLoop")),
        q(&e("it"), &l("iterationBody"), &e("primBody")),
        primitive("primBody", &[], &[], &["sitLoop"]),
    );
    let out = run(&nq, "it").expect("zero-iteration succeeds");
    assert!(out.succeeded());
    assert!(out.steps.is_empty(), "no body pass");
    assert_eq!(out.path, vec![format!("{W}#s0")]);
}

#[test]
fn step_attribution_is_unique_per_pass_and_deterministic() {
    // The same primitive executed on two iteration passes starts from DISTINCT states, so
    // its minted logic:TransactionStep — the supersession-quartet attribution — must be
    // distinct, never collapsing two runtime passes onto one node.
    let pass_one = mint_step("root", "primBody", "s0");
    let pass_two = mint_step("root", "primBody", "s1");
    assert_ne!(
        pass_one, pass_two,
        "distinct from-states (passes) mint distinct step nodes"
    );
    // Deterministic / content-addressed: same salt → same IRI.
    assert_eq!(pass_one, mint_step("root", "primBody", "s0"));
    // And disjoint from the state IRIs minted over the same salt.
    assert!(pass_one.starts_with("https://blackcatinformatics.ca/logic/step/"));
    assert_ne!(pass_one, mint_state("root", "primBody", "s0"));
}

#[test]
fn iteration_step_bound_hard_fails_on_no_progress() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    // Condition sitLoop holds and the body re-asserts it (a content no-op) → the loop never
    // makes progress; the step bound must hard-fail rather than spin forever.
    let nq = format!(
        "{}{}{}{}{}{}",
        start_state("s0", &["sitLoop"]),
        q(&e("it"), ty, &l("Iteration")),
        q(&e("it"), &l("transitionFromState"), &e("s0")),
        q(&e("it"), &l("iterationCondition"), &e("sitLoop")),
        q(&e("it"), &l("iterationBody"), &e("primBody")),
        primitive("primBody", &[], &["sitLoop"], &[]), // re-asserts the loop condition
    );
    let err = run(&nq, "it").unwrap_err();
    assert!(err.contains("step bound"), "{err}");
    assert!(err.contains("non-terminating"), "{err}");
}

// ── Root discovery ──────────────────────────────────────────────────────────────

#[test]
fn program_roots_excludes_operands() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    // outer Fallback whose left is an inner Serial (used as an operand) → only outer is a root.
    let nq = format!(
        "{}{}{}{}{}{}{}",
        q(&e("outer"), ty, &l("Fallback")),
        q(&e("outer"), &l("transitionFromState"), &e("s0")),
        q(&e("outer"), &l("leftOperand"), &e("inner")),
        q(&e("outer"), &l("rightOperand"), &e("primB")),
        q(&e("inner"), ty, &l("SerialConjunction")),
        q(&e("inner"), &l("leftOperand"), &e("primA")),
        q(&e("inner"), &l("rightOperand"), &e("primC")),
    );
    let facts = facts_of(&nq);
    let roots = program_roots(&facts);
    // `inner` is an operand (excluded); `outer` carries a start marker (executable).
    assert_eq!(roots, vec![format!("{W}#outer")]);
}

// ── Determinism ─────────────────────────────────────────────────────────────────

#[test]
fn execution_is_deterministic() {
    let world = choice_world(true);
    let a = run(&world, "ch").unwrap();
    let b = run(&world, "ch").unwrap();
    assert_eq!(
        a, b,
        "same program + input must yield identical outcome (content-addressed)"
    );
}

// ── Materialization ─────────────────────────────────────────────────────────────

fn outcome_quads(nq: &str, root_local: &str) -> Vec<crate::teleology::TeleologyQuad> {
    let facts = facts_of(nq);
    let root = format!("{W}#{root_local}");
    emit_transaction_outcome(&facts, W, &root).expect("materialization succeeds")
}

#[test]
fn materializes_success_outcome_with_path() {
    // A succeeding choice: guard holds → left runs → outcome succeeds with a one-edge path.
    let quads = outcome_quads(&choice_world(true), "ch");
    let succeeds = quads
        .iter()
        .find(|q| q.predicate.ends_with("transactionSucceeds"));
    assert!(
        succeeds.is_some(),
        "an outcome must carry transactionSucceeds"
    );
    assert!(
        succeeds.unwrap().object.starts_with("\"true\""),
        "{:?}",
        succeeds.unwrap().object
    );
    // Outcome carries its program + start, and the executed path obtains a successor.
    assert!(quads
        .iter()
        .any(|q| q.predicate.ends_with("outcomeOfProgram") && q.object == format!("<{W}#ch>")));
    assert!(quads
        .iter()
        .any(|q| q.predicate.ends_with("transactionStart") && q.object == format!("<{W}#s0>")));
    assert!(
        quads
            .iter()
            .any(|q| q.predicate.ends_with("temporallySucceeds")),
        "executed path edge present"
    );
    assert!(
        quads
            .iter()
            .any(|q| q.predicate.ends_with("executedAlongPath")),
        "outcome links to its logic:Path"
    );
    assert!(
        quads
            .iter()
            .any(|q| q.predicate.ends_with("situationObtains")
                && q.object == format!("<{W}#sit_lft>"))
    );
    // Every emitted quad is stamped with the transaction rule IRI.
    assert!(quads.iter().all(|q| q.rule_iri == TRANSACTION_RULE_IRI));
}

#[test]
fn materializes_failure_outcome_leaves_start_untouched() {
    // A fallback whose both branches fail: outcome is recorded false, with NO path/substrate.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}{}{}",
        start_state("s0", &[]),
        q(&e("fb"), ty, &l("Fallback")),
        q(&e("fb"), &l("transitionFromState"), &e("s0")),
        q(&e("fb"), &l("leftOperand"), &e("primPrimary")),
        q(&e("fb"), &l("rightOperand"), &e("primBackup")),
        [
            primitive("primPrimary", &["sitMissing"], &[], &[]),
            primitive("primBackup", &["sitAlsoMissing"], &[], &[]),
        ]
        .concat(),
    );
    let quads = outcome_quads(&nq, "fb");
    let succeeds = quads
        .iter()
        .find(|q| q.predicate.ends_with("transactionSucceeds"))
        .unwrap();
    assert!(
        succeeds.object.starts_with("\"false\""),
        "{:?}",
        succeeds.object
    );
    // Failure leaves the start untouched: NO path edges, NO situationObtains substrate.
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("temporallySucceeds")),
        "no path on failure"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("situationObtains")),
        "no substrate on failure"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("executedAlongPath")),
        "no path link on failure"
    );
}

#[test]
fn program_roots_requires_executable_start_marker() {
    // A combinator without logic:transitionFromState is structural data, not an executable
    // root — it must NOT be evaluated (keeps vocabulary-only declarations inert).
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}",
        q(&e("decl"), ty, &l("Choice")),
        q(&e("decl"), &l("leftOperand"), &e("a")),
        q(&e("decl"), &l("rightOperand"), &e("b")),
    );
    let facts = facts_of(&nq);
    assert!(
        program_roots(&facts).is_empty(),
        "no executable root without a start marker"
    );
}
