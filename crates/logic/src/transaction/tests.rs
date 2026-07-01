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
fn parse_accepts_concurrent_composition() {
    // ConcurrentComposition is now EXECUTABLE (T4): it parses to a Concurrent variant with
    // its two operands, exactly like the other binary combinators — no longer a hard error.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("leftOperand"), &e("a")),
            q(&e("cc"), &l("rightOperand"), &e("b")),
        ]
        .concat(),
        primitive("a", &[], &["sitA"], &[]),
        primitive("b", &[], &["sitB"], &[]),
    );
    let facts = facts_of(&nq);
    let prog = parse_program(&facts, &format!("{W}#cc"), 0).expect("concurrent parses");
    assert!(
        matches!(prog, TransactionProgram::Concurrent { .. }),
        "expected a Concurrent variant, got {prog:?}"
    );
}

#[test]
fn parse_concurrent_rejects_missing_right_operand() {
    // No-optionality survives: a ConcurrentComposition missing an operand is a hard error
    // (reuses require_one, exactly like SerialConjunction / Fallback).
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}",
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        q(&e("cc"), &l("leftOperand"), &e("a")),
        primitive("a", &[], &["sitA"], &[]),
    );
    let facts = facts_of(&nq);
    let err = parse_program(&facts, &format!("{W}#cc"), 0).unwrap_err();
    assert!(err.contains("rightOperand"), "{err}");
}

#[test]
fn parse_concurrent_rejects_doubled_left_operand() {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        q(&e("cc"), &l("leftOperand"), &e("a")),
        q(&e("cc"), &l("leftOperand"), &e("b")), // doubled
        q(&e("cc"), &l("rightOperand"), &e("c")),
    );
    let facts = facts_of(&nq);
    let err = parse_program(&facts, &format!("{W}#cc"), 0).unwrap_err();
    assert!(err.contains("leftOperand"), "{err}");
    assert!(err.contains("exactly one required"), "{err}");
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
    assert_eq!(out.path().len(), 3, "path = {:?}", out.path());
    assert_eq!(out.steps().len(), 2);
    assert!(out.sits_end().contains(&format!("{W}#sitX")));
    assert!(out.sits_end().contains(&format!("{W}#sitY")));
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
    assert!(out.path().is_empty());
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
    assert_eq!(out.steps().len(), 1);
    // The left branch ran: its schema was instantiated, and the step is a minted runtime
    // logic:TransactionStep (no longer the static program node).
    assert_eq!(out.steps()[0].schema, format!("{W}#primLSchema"));
    assert!(out.steps()[0]
        .attribution
        .starts_with("https://blackcatinformatics.ca/logic/step/"));
    assert!(out.sits_end().contains(&format!("{W}#sit_lft")));
    assert!(!out.sits_end().contains(&format!("{W}#sit_rgt")));
}

#[test]
fn choice_guard_false_takes_right() {
    let out = run(&choice_world(false), "ch").expect("choice succeeds");
    assert!(out.succeeded());
    assert_eq!(out.steps().len(), 1);
    assert_eq!(out.steps()[0].schema, format!("{W}#primRSchema"));
    assert!(out.steps()[0]
        .attribution
        .starts_with("https://blackcatinformatics.ca/logic/step/"));
    assert!(out.sits_end().contains(&format!("{W}#sit_rgt")));
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
    assert_eq!(out.steps().len(), 1);
    assert_eq!(out.steps()[0].schema, format!("{W}#primBackupSchema"));
    assert!(out.steps()[0]
        .attribution
        .starts_with("https://blackcatinformatics.ca/logic/step/"));
    assert!(out.sits_end().contains(&format!("{W}#sitOk")));
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
        out.path().is_empty(),
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
    assert_eq!(out.steps().len(), 1, "exactly one body pass");
    assert_eq!(out.path().len(), 2);
    assert!(!out.sits_end().contains(&format!("{W}#sitLoop")));
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
    assert!(out.steps().is_empty(), "no body pass");
    assert_eq!(out.path(), vec![format!("{W}#s0")]);
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

// ── Notification-wait: PENDING is a first-class tri-state, distinct from failure ──

/// A notification-wait primitive `node`: an ordinary primitive whose action schema also
/// `logic:awaitsSignal` the given external-signal locals (the schema is typed a
/// `logic:NotificationWaitSchema` for good measure — the interpreter reads `awaitsSignal`
/// directly, but the type keeps the fixture faithful to the vocabulary).
fn wait_primitive(node: &str, preconds: &[&str], signals: &[&str], ins: &[&str]) -> String {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let schema = format!("{node}Schema");
    let mut s = primitive(node, preconds, ins, &[]);
    s.push_str(&q(&e(&schema), ty, &l("NotificationWaitSchema")));
    for sig in signals {
        s.push_str(&q(&e(&schema), &l("awaitsSignal"), &e(sig)));
    }
    s
}

/// A plan world whose executable root is a guard-TRUE `logic:Choice` selecting a
/// notification-wait step. The Choice supplies the `logic:transitionFromState` marker that
/// makes it an executable root; its left branch is the wait. When `signal_present`, the awaited
/// external signal `sigDone` obtains at the start so the wait fires; otherwise the wait is
/// pending.
fn wait_plan_world(signal_present: bool) -> String {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    // The start obtains the guard sitG (so the Choice takes the wait branch) and, when
    // `signal_present`, the awaited external signal sigDone.
    let obtains: &[&str] = if signal_present {
        &["sitG", "sigDone"]
    } else {
        &["sitG"]
    };
    format!(
        "{}{}{}{}{}{}{}",
        start_state("s0", obtains),
        q(&e("plan"), ty, &l("Choice")),
        q(&e("plan"), &l("transitionFromState"), &e("s0")),
        q(&e("plan"), &l("guardSituation"), &e("sitG")),
        // left branch = the wait; right branch = an unconditional success (never taken here).
        [
            q(&e("plan"), &l("leftOperand"), &e("waitStep")),
            q(&e("plan"), &l("rightOperand"), &e("elseStep")),
        ]
        .concat(),
        wait_primitive("waitStep", &[], &["sigDone"], &["sitDone"]),
        primitive("elseStep", &[], &["sitElse"], &[]),
    )
}

#[test]
fn wait_step_without_signal_is_pending_not_failed() {
    // The awaited external signal is NOT in the world: the wait halts PENDING. This is
    // UNDETERMINED — it must NOT read as a failure, and it names the signal it waits on.
    let out = run(&wait_plan_world(false), "plan").expect("no hard error");
    assert!(
        !out.succeeded(),
        "an un-signalled wait has not completed — not a success"
    );
    assert_eq!(
        out.pending_signal(),
        Some(format!("{W}#sigDone").as_str()),
        "the pending outcome names the exact external signal it is waiting on"
    );
    assert!(
        matches!(out, ExecOutcome::Pending { .. }),
        "an un-signalled wait is PENDING, never ExecOutcome::Failed: {out:?}"
    );
    assert!(
        !matches!(out, ExecOutcome::Failed),
        "PENDING must be distinct from a genuine precondition failure"
    );
}

#[test]
fn wait_step_with_signal_present_succeeds() {
    // The SAME plan, but the awaited signal now obtains in the world → the wait fires and the
    // plan succeeds.
    let out = run(&wait_plan_world(true), "plan").expect("wait succeeds once signalled");
    assert!(out.succeeded(), "a signalled wait completes");
    assert_eq!(out.pending_signal(), None, "a succeeded run is not pending");
    assert!(out.sits_end().contains(&format!("{W}#sitDone")));
}

#[test]
fn genuine_precondition_failure_is_failed_not_pending() {
    // A primitive whose ordinary precondition is unmet is a genuine FAILURE — it must be
    // ExecOutcome::Failed, NEVER Pending (there is no external signal to wait on).
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        start_state("s0", &[]),
        q(&e("plan"), ty, &l("Choice")),
        [
            q(&e("plan"), &l("transitionFromState"), &e("s0")),
            q(&e("plan"), &l("guardSituation"), &e("sitG")),
            q(&e("plan"), &l("leftOperand"), &e("a")),
            q(&e("plan"), &l("rightOperand"), &e("b")),
        ]
        .concat(),
        [
            // Guard sitG is ABSENT → right branch `b` runs; b requires a missing precondition.
            primitive("a", &[], &["sitA"], &[]),
            primitive("b", &["sitMissing"], &["sitB"], &[]),
        ]
        .concat(),
    );
    let out = run(&nq, "plan").expect("no hard error");
    assert!(
        matches!(out, ExecOutcome::Failed),
        "an unmet ordinary precondition is a genuine failure: {out:?}"
    );
    assert_eq!(
        out.pending_signal(),
        None,
        "a genuine failure is not pending on any signal"
    );
}

#[test]
fn pending_outcome_emits_awaiting_signal_witness_and_no_success_substrate() {
    // The transaction-path materialization of a pending run: the outcome carries a
    // logic:awaitingSignal witness naming the awaited external signal, records
    // transactionSucceeds false, and emits NO success substrate (no path / step / obtains).
    let quads = outcome_quads(&wait_plan_world(false), "plan");

    // The verdict is present and reads false (no completing path exists yet).
    let succeeds = quads
        .iter()
        .find(|q| q.predicate.ends_with("transactionSucceeds"))
        .expect("a pending run still records its verdict");
    assert!(
        succeeds.object.starts_with("\"false\""),
        "pending reads transactionSucceeds false: {:?}",
        succeeds.object
    );

    // The load-bearing distinction from a plain failure: the awaitingSignal witness naming
    // the exact external signal the run is still waiting on.
    let awaiting = quads
        .iter()
        .find(|q| q.predicate.ends_with("awaitingSignal"))
        .expect("a pending outcome must carry a logic:awaitingSignal witness");
    assert_eq!(
        awaiting.object,
        format!("<{W}#sigDone>"),
        "the witness names the awaited signal"
    );

    // No success substrate: the run did not complete, so the start state is untouched.
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("temporallySucceeds")),
        "no committed path on a pending outcome"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("executedAlongPath")),
        "no committed path link on a pending outcome"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("situationObtains")),
        "no committed effect substrate on a pending outcome"
    );
    assert!(
        !quads.iter().any(|q| q.object == l("TransactionStep")),
        "no committed step nodes on a pending outcome"
    );
}

#[test]
fn fallback_with_pending_primary_halts_pending_does_not_take_alternate() {
    // A Fallback whose PRIMARY is a wait pending its signal: the whole plan is PENDING and does
    // NOT fall through to the (succeeding) alternate — a pending primary may still complete, so
    // routing to the alternate would fabricate a decision the engine has not earned.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}{}",
        start_state("s0", &[]), // sigDone ABSENT → the primary wait is pending
        q(&e("fb"), ty, &l("Fallback")),
        [
            q(&e("fb"), &l("transitionFromState"), &e("s0")),
            q(&e("fb"), &l("leftOperand"), &e("waitStep")),
            q(&e("fb"), &l("rightOperand"), &e("altStep")),
        ]
        .concat(),
        wait_primitive("waitStep", &[], &["sigDone"], &["sitDone"]),
        // The alternate would succeed unconditionally — but must NOT be taken while pending.
        primitive("altStep", &[], &["sitAlt"], &[]),
    );
    let out = run(&nq, "fb").expect("no hard error");
    assert!(
        matches!(out, ExecOutcome::Pending { .. }),
        "a pending primary halts the fallback pending: {out:?}"
    );
    assert_eq!(
        out.pending_signal(),
        Some(format!("{W}#sigDone").as_str()),
        "the fallback forwards the primary's awaited signal"
    );
    assert!(
        !out.sits_end().contains(&format!("{W}#sitAlt")),
        "the alternate must not have run"
    );
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

// ── The hypothetical/sandbox operator: test without committing ────────────────────

/// Annotate a transaction world so `root_local` runs under `logic:HypotheticalExecution`,
/// via `executedUnderContract` → a contract carrying `executionMode HypotheticalExecution`.
fn hypo_annotated(world: String, root_local: &str) -> String {
    format!(
        "{world}{}{}",
        q(
            &e(root_local),
            &l("executedUnderContract"),
            &e("hypoContract")
        ),
        q(
            &e("hypoContract"),
            &l("executionMode"),
            &l("HypotheticalExecution")
        ),
    )
}

#[test]
fn hypothetical_run_records_verdict_but_discards_committed_path() {
    // A SUCCEEDING program run under HypotheticalExecution: the verdict is observable, but
    // none of the committed-path effect substrate is emitted — "test without committing".
    let quads = outcome_quads(&hypo_annotated(choice_world(true), "ch"), "ch");

    // (1) The verdict is present and TRUE — the program WOULD succeed.
    let succeeds = quads
        .iter()
        .find(|q| q.predicate.ends_with("transactionSucceeds"))
        .expect("a hypothetical run still records its verdict");
    assert!(
        succeeds.object.starts_with("\"true\""),
        "{:?}",
        succeeds.object
    );

    // (2) The committed-path EFFECT substrate is ABSENT (discarded, not erased).
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("temporallySucceeds")),
        "hypothetical: no committed path edges"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("executedAlongPath")),
        "hypothetical: no committed path link"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("situationObtains")),
        "hypothetical: no committed effect substrate"
    );
    assert!(
        !quads.iter().any(|q| q.object == l("TransactionStep")),
        "hypothetical: no committed step nodes"
    );

    // (3) The discarded run leaves a content-addressed witness (makes the run observable).
    let witness = quads
        .iter()
        .find(|q| q.predicate.ends_with("executedHypotheticallyAs"))
        .expect("hypothetical: the run's content address is recorded as provenance");
    assert!(
        witness.object.starts_with('"'),
        "witness is a string literal: {:?}",
        witness.object
    );
}

#[test]
fn verdict_is_identical_committed_vs_hypothetical_only_substrate_differs() {
    // The heart of "test without committing": the verdict is mode-invariant; only emission differs.
    let committed = outcome_quads(&choice_world(true), "ch");
    let hypo = outcome_quads(&hypo_annotated(choice_world(true), "ch"), "ch");

    let verdict = |qs: &[crate::teleology::TeleologyQuad]| {
        qs.iter()
            .find(|q| q.predicate.ends_with("transactionSucceeds"))
            .expect("verdict present")
            .object
            .clone()
    };
    assert_eq!(
        verdict(&committed),
        verdict(&hypo),
        "the verdict must not depend on the execution mode"
    );

    // Committed emits the path substrate; hypothetical does not.
    assert!(committed
        .iter()
        .any(|q| q.predicate.ends_with("temporallySucceeds")));
    assert!(!hypo
        .iter()
        .any(|q| q.predicate.ends_with("temporallySucceeds")));

    // The witness is exclusive to the hypothetical run.
    assert!(!committed
        .iter()
        .any(|q| q.predicate.ends_with("executedHypotheticallyAs")));
    assert!(hypo
        .iter()
        .any(|q| q.predicate.ends_with("executedHypotheticallyAs")));
}

#[test]
fn root_execution_mode_defaults_to_committed_when_unannotated() {
    let facts = facts_of(&choice_world(true));
    assert_eq!(
        root_execution_mode(&facts, &format!("{W}#ch")).unwrap(),
        ExecutionMode::Committed,
        "absence of a governing contract resolves to the default committed execution"
    );
}

#[test]
fn root_execution_mode_reads_explicit_committed_and_hypothetical() {
    let nq_committed = format!(
        "{}{}{}",
        choice_world(true),
        q(&e("ch"), &l("executedUnderContract"), &e("k")),
        q(&e("k"), &l("executionMode"), &l("CommittedExecution")),
    );
    assert_eq!(
        root_execution_mode(&facts_of(&nq_committed), &format!("{W}#ch")).unwrap(),
        ExecutionMode::Committed
    );

    assert_eq!(
        root_execution_mode(
            &facts_of(&hypo_annotated(choice_world(true), "ch")),
            &format!("{W}#ch")
        )
        .unwrap(),
        ExecutionMode::Hypothetical
    );
}

#[test]
fn root_execution_mode_hard_fails_on_two_contracts() {
    let nq = format!(
        "{}{}{}",
        choice_world(true),
        q(&e("ch"), &l("executedUnderContract"), &e("k1")),
        q(&e("ch"), &l("executedUnderContract"), &e("k2")),
    );
    let err = root_execution_mode(&facts_of(&nq), &format!("{W}#ch")).unwrap_err();
    assert!(err.contains("executedUnderContract"), "{err}");
}

#[test]
fn root_execution_mode_hard_fails_on_two_execution_modes() {
    let nq = format!(
        "{}{}{}{}",
        choice_world(true),
        q(&e("ch"), &l("executedUnderContract"), &e("k")),
        q(&e("k"), &l("executionMode"), &l("CommittedExecution")),
        q(&e("k"), &l("executionMode"), &l("HypotheticalExecution")),
    );
    let err = root_execution_mode(&facts_of(&nq), &format!("{W}#ch")).unwrap_err();
    assert!(err.contains("executionMode"), "{err}");
}

#[test]
fn root_execution_mode_hard_fails_on_unknown_value() {
    let nq = format!(
        "{}{}{}",
        choice_world(true),
        q(&e("ch"), &l("executedUnderContract"), &e("k")),
        q(&e("k"), &l("executionMode"), &e("BogusMode")),
    );
    let err = root_execution_mode(&facts_of(&nq), &format!("{W}#ch")).unwrap_err();
    assert!(err.contains("unknown logic:executionMode"), "{err}");
}

#[test]
fn root_execution_mode_defaults_to_committed_when_contract_has_no_mode() {
    // One governing contract, but it carries no logic:executionMode at all: the distinct
    // "one contract, zero modes" branch must resolve to the committed default.
    let nq = format!(
        "{}{}",
        choice_world(true),
        q(&e("ch"), &l("executedUnderContract"), &e("k")),
    );
    assert_eq!(
        root_execution_mode(&facts_of(&nq), &format!("{W}#ch")).unwrap(),
        ExecutionMode::Committed,
        "a governing contract without logic:executionMode still defaults to committed"
    );
}

#[test]
fn root_execution_mode_hard_fails_on_non_iri_contract() {
    // A literal logic:executedUnderContract is malformed sandbox input: it is invisible to
    // objects() (IRIs only), so it must be detected and hard-fail rather than silently commit.
    let nq = format!(
        "{}{}",
        choice_world(true),
        q(&e("ch"), &l("executedUnderContract"), "\"hypoContract\""),
    );
    let err = root_execution_mode(&facts_of(&nq), &format!("{W}#ch")).unwrap_err();
    assert!(err.contains("non-IRI logic:executedUnderContract"), "{err}");
}

#[test]
fn root_execution_mode_hard_fails_on_non_iri_execution_mode() {
    // A literal logic:executionMode must NOT be mistaken for an absent mode (which would
    // silently default to committed) — a run authored as a sandbox would otherwise commit.
    let nq = format!(
        "{}{}{}",
        choice_world(true),
        q(&e("ch"), &l("executedUnderContract"), &e("k")),
        q(&e("k"), &l("executionMode"), "\"HypotheticalExecution\""),
    );
    let err = root_execution_mode(&facts_of(&nq), &format!("{W}#ch")).unwrap_err();
    assert!(err.contains("non-IRI logic:executionMode"), "{err}");
}

#[test]
fn canonical_program_is_stable_and_distinguishes_structure() {
    // The hypothetical-run key hashes this encoding, so it is a committed content-address
    // contract: identical for equal programs, distinct for structurally different ones, and
    // never derived from Debug formatting.
    let a = TransactionProgram::Primitive {
        node: "n".into(),
        schema: "s".into(),
    };
    let b = TransactionProgram::Primitive {
        node: "n".into(),
        schema: "s".into(),
    };
    assert_eq!(canonical_program(&a), canonical_program(&b));

    let other_schema = TransactionProgram::Primitive {
        node: "n".into(),
        schema: "other".into(),
    };
    assert_ne!(canonical_program(&a), canonical_program(&other_schema));

    // Length-prefixing prevents adjacent leaf fields from colliding across a boundary
    // ("ns" + "" must not encode the same as "n" + "s").
    let boundary = TransactionProgram::Primitive {
        node: "ns".into(),
        schema: String::new(),
    };
    assert_ne!(canonical_program(&a), canonical_program(&boundary));
}

// ── Concurrent composition: interleaving + derived serializability ───────────────

/// Helper: count quads of a given local `rdf:type`.
fn has_type(quads: &[crate::teleology::TeleologyQuad], local: &str) -> bool {
    quads.iter().any(|q| {
        q.predicate == RDF_TYPE
            && q.object == format!("<https://blackcatinformatics.ca/logic/{local}>")
    })
}

#[test]
fn concurrent_serializable_emits_no_anomaly() {
    // Two legs touching DISJOINT situations: zero conflict edges → conflict-serializable →
    // NO anomaly, but the derived logic:ConcurrentHistory + verdict are still materialized.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        start_state("s0", &[]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("pa")),
            q(&e("cc"), &l("rightOperand"), &e("pb")),
        ]
        .concat(),
        [
            primitive("pa", &[], &["sitA"], &[]),
            primitive("pb", &[], &["sitB"], &[]),
        ]
        .concat(),
    );
    let quads = outcome_quads(&nq, "cc");

    // Verdict: both legs find a path → succeeds.
    let succeeds = quads
        .iter()
        .find(|q| q.predicate.ends_with("transactionSucceeds"))
        .expect("a concurrent run records its verdict");
    assert!(
        succeeds.object.starts_with("\"true\""),
        "{:?}",
        succeeds.object
    );

    // A derived ConcurrentHistory + its outcome link, criterion, and operand audit edges.
    assert!(
        has_type(&quads, "ConcurrentHistory"),
        "history node present"
    );
    assert!(quads
        .iter()
        .any(|q| q.predicate.ends_with("derivedHistory")));
    assert!(quads
        .iter()
        .any(|q| q.predicate.ends_with("serializabilityCriterion")
            && q.object.ends_with("ConflictSerializability>")));
    assert_eq!(
        quads
            .iter()
            .filter(|q| q.predicate.ends_with("concurrentComposedFrom"))
            .count(),
        2,
        "history names both composed-from operands"
    );

    // Serializable: NO anomaly, and NO conflict edges between disjoint legs.
    assert!(
        !has_type(&quads, "SerializationAnomaly"),
        "serializable → no anomaly"
    );
    assert!(
        !quads.iter().any(|q| q.predicate.ends_with("/precedes")),
        "disjoint footprints → no conflict edges"
    );
}

/// A two-step leg built as a SerialConjunction of two primitives.
fn serial2(node: &str, l0: &str, l1: &str) -> String {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    format!(
        "{}{}",
        q(&e(node), ty, &l("SerialConjunction")),
        [
            q(&e(node), &l("leftOperand"), &e(l0)),
            q(&e(node), &l("rightOperand"), &e(l1)),
        ]
        .concat(),
    )
}

/// The cross-dependency world: left writes sitX then reads sitY; right writes sitY then
/// reads sitX. The two legs conflict in OPPOSING index order → a precedes cycle → anomaly.
/// Start obtains sitX+sitY so each leg succeeds independently (the reads are satisfied).
fn cross_dependency_world() -> String {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    format!(
        "{}{}{}{}{}{}",
        start_state("s0", &["sitX", "sitY"]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("leftSer")),
            q(&e("cc"), &l("rightOperand"), &e("rightSer")),
        ]
        .concat(),
        [
            serial2("leftSer", "l0", "l1"),
            serial2("rightSer", "r0", "r1")
        ]
        .concat(),
        [
            // left: l0 writes sitX; l1 reads sitY, writes sitZ1.
            primitive("l0", &[], &["sitX"], &[]),
            primitive("l1", &["sitY"], &["sitZ1"], &[]),
        ]
        .concat(),
        [
            // right: r0 writes sitY; r1 reads sitX, writes sitZ2.
            primitive("r0", &[], &["sitY"], &[]),
            primitive("r1", &["sitX"], &["sitZ2"], &[]),
        ]
        .concat(),
    )
}

#[test]
fn concurrent_non_serializable_emits_anomaly_with_cycle() {
    let quads = outcome_quads(&cross_dependency_world(), "cc");

    // The run succeeds (both legs find a path) — a serialization anomaly is a HISTORY-level
    // finding, not an execution failure.
    let succeeds = quads
        .iter()
        .find(|q| q.predicate.ends_with("transactionSucceeds"))
        .unwrap();
    assert!(
        succeeds.object.starts_with("\"true\""),
        "{:?}",
        succeeds.object
    );

    // Opposing-order conflicts → a precedes cycle → a SerializationAnomaly finding.
    assert!(
        has_type(&quads, "SerializationAnomaly"),
        "cross-dependency → anomaly"
    );
    assert!(
        quads
            .iter()
            .any(|q| q.predicate.ends_with("violatedCriterion")
                && q.object.ends_with("ConflictSerializability>")),
        "anomaly names the violated criterion"
    );
    let cycle = quads
        .iter()
        .find(|q| q.predicate.ends_with("anomalyCycle"))
        .expect("anomaly carries its cycle description");
    assert!(cycle.object.contains("#leftSer"), "{:?}", cycle.object);
    assert!(cycle.object.contains("#rightSer"), "{:?}", cycle.object);

    // Edges in BOTH directions (the two-transaction cycle).
    assert!(quads.iter().any(|q| q.predicate.ends_with("/precedes")
        && q.subject.ends_with("#leftSer")
        && q.object.ends_with("#rightSer>")));
    assert!(quads.iter().any(|q| q.predicate.ends_with("/precedes")
        && q.subject.ends_with("#rightSer")
        && q.object.ends_with("#leftSer>")));

    // It is a FINDING, never a contradiction witness: the final state is consistent.
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("contradictionWitness")),
        "a serialization anomaly is NOT a logical contradiction"
    );
}

#[test]
fn concurrent_write_write_conflict_is_detected() {
    // Pure write-write conflicts (no reads): left writes {sitX, sitY} in two steps, right
    // writes {sitY, sitX} in two steps — opposing order on the two shared writes → a cycle.
    // Proves derive_conflict_edges does NOT depend on read sets to find conflicts.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}{}{}",
        start_state("s0", &[]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("leftSer")),
            q(&e("cc"), &l("rightOperand"), &e("rightSer")),
        ]
        .concat(),
        [
            serial2("leftSer", "l0", "l1"),
            serial2("rightSer", "r0", "r1")
        ]
        .concat(),
        [
            primitive("l0", &[], &["sitX"], &[]), // l0 writes sitX
            primitive("l1", &[], &["sitY"], &[]), // l1 writes sitY
        ]
        .concat(),
        [
            primitive("r0", &[], &["sitY"], &[]), // r0 writes sitY  (conflicts l1, i=1>j=0 → R→L)
            primitive("r1", &[], &["sitX"], &[]), // r1 writes sitX  (conflicts l0, i=0<=j=1 → L→R)
        ]
        .concat(),
    );
    let quads = outcome_quads(&nq, "cc");
    assert!(
        has_type(&quads, "SerializationAnomaly"),
        "write-write conflicts in opposing order must be detected"
    );
}

#[test]
fn concurrent_anomaly_is_deterministic() {
    // Content-addressed: the same world yields byte-identical quads (incl. the anomaly).
    let world = cross_dependency_world();
    let a = outcome_quads(&world, "cc");
    let b = outcome_quads(&world, "cc");
    assert_eq!(a, b, "concurrent materialization must be deterministic");
}

#[test]
fn concurrent_fails_when_either_leg_fails() {
    // One leg has an unmet precondition → the whole concurrent composition fails (empty
    // path), and NO history / substrate / anomaly is emitted (suppression-never-erasure).
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        start_state("s0", &[]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("pgood")),
            q(&e("cc"), &l("rightOperand"), &e("pbad")),
        ]
        .concat(),
        [
            primitive("pgood", &[], &["sitA"], &[]),
            primitive("pbad", &["sitMissing"], &["sitB"], &[]), // precondition never holds
        ]
        .concat(),
    );
    let quads = outcome_quads(&nq, "cc");
    let succeeds = quads
        .iter()
        .find(|q| q.predicate.ends_with("transactionSucceeds"))
        .unwrap();
    assert!(
        succeeds.object.starts_with("\"false\""),
        "{:?}",
        succeeds.object
    );
    assert!(
        !has_type(&quads, "ConcurrentHistory"),
        "no history on failure"
    );
    assert!(
        !has_type(&quads, "SerializationAnomaly"),
        "no anomaly on failure"
    );
    assert!(
        !quads
            .iter()
            .any(|q| q.predicate.ends_with("situationObtains")),
        "no committed substrate on failure"
    );
}

// ── View-serializability: the second criterion + read-from / happens-before ──────

/// Whether the history satisfies the given serializability criterion (logic:satisfiesCriterion).
fn satisfies(quads: &[crate::teleology::TeleologyQuad], criterion_local: &str) -> bool {
    quads.iter().any(|q| {
        q.predicate.ends_with("satisfiesCriterion")
            && q.object.ends_with(&format!("{criterion_local}>"))
    })
}

#[test]
fn concurrent_serializable_satisfies_view_and_conflict() {
    // Disjoint legs → conflict-serializable. A conflict-serializable schedule is ALWAYS
    // view-serializable, so the derived history satisfies BOTH criteria via satisfiesCriterion.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        start_state("s0", &[]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("pa")),
            q(&e("cc"), &l("rightOperand"), &e("pb")),
        ]
        .concat(),
        [
            primitive("pa", &[], &["sitA"], &[]),
            primitive("pb", &[], &["sitB"], &[]),
        ]
        .concat(),
    );
    let quads = outcome_quads(&nq, "cc");
    assert!(
        satisfies(&quads, "ConflictSerializability"),
        "acyclic conflict graph → conflict-serializable"
    );
    assert!(
        satisfies(&quads, "ViewSerializability"),
        "conflict-serializable ⟹ view-serializable"
    );
}

#[test]
fn concurrent_read_dependency_emits_view_surface() {
    // left writes sitA; right reads sitA (present at the shared start, so the leg succeeds
    // independently) then writes sitB. In the witnessed interleaving [l0, r0] the right leg's
    // read observes the left leg's write → a cross-leg logic:readsFrom edge (r0 → l0) and a
    // logic:happensBefore edge (l0 → r0). One-directional conflict → serializable (both criteria).
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        start_state("s0", &["sitA"]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("l0")),
            q(&e("cc"), &l("rightOperand"), &e("r0")),
        ]
        .concat(),
        [
            primitive("l0", &[], &["sitA"], &[]),
            primitive("r0", &["sitA"], &["sitB"], &[]),
        ]
        .concat(),
    );
    let quads = outcome_quads(&nq, "cc");
    assert!(
        quads.iter().any(|q| q.predicate.ends_with("/readsFrom")),
        "the cross-leg read must materialize a readsFrom edge"
    );
    assert!(
        quads
            .iter()
            .any(|q| q.predicate.ends_with("/happensBefore")),
        "the cross-leg conflict must materialize a happensBefore edge"
    );
    assert!(
        satisfies(&quads, "ConflictSerializability") && satisfies(&quads, "ViewSerializability"),
        "a one-directional read dependency is serializable under both criteria"
    );
}

#[test]
fn concurrent_non_serializable_satisfies_neither_criterion() {
    // The cross-dependency world is conflict-CYCLIC. For two transactions view-serializability
    // coincides with conflict-serializability, so it satisfies NEITHER criterion — and its
    // cross-leg reads still materialize readsFrom edges for audit.
    let quads = outcome_quads(&cross_dependency_world(), "cc");
    assert!(
        !satisfies(&quads, "ConflictSerializability"),
        "a conflict cycle is not conflict-serializable"
    );
    assert!(
        !satisfies(&quads, "ViewSerializability"),
        "for two transactions a conflict cycle is not view-serializable either"
    );
    assert!(
        quads.iter().any(|q| q.predicate.ends_with("/readsFrom")),
        "cross-leg reads (r1 reads sitX from l0; l1 reads sitY from r0) materialize readsFrom"
    );
}

#[test]
fn concurrent_self_composition_is_hard_error() {
    // Composing a program with ITSELF (both operands the same node) is malformed for
    // serializability analysis — a hard error, never a silent self-conflict.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        start_state("s0", &[]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("pa")),
            q(&e("cc"), &l("rightOperand"), &e("pa")),
        ]
        .concat(),
        primitive("pa", &[], &["sitA"], &[]),
    );
    let facts = facts_of(&nq);
    let err = emit_transaction_outcome(&facts, W, &format!("{W}#cc")).unwrap_err();
    assert!(err.contains("composes a program with itself"), "{err}");
}

// ── Protocol soundness: the declared control mechanism verified over recorded events ──

/// An `xsd:integer` N3 literal.
fn int_lit(n: i64) -> String {
    format!("\"{n}\"^^<http://www.w3.org/2001/XMLSchema#integer>")
}

/// Wire the concurrent root `cc` to a logic:ReasoningContract declaring `protocol_local`.
fn declares_protocol(protocol_local: &str) -> String {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    format!(
        "{}{}{}",
        q(&e("contract"), ty, &l("ReasoningContract")),
        q(&e("cc"), &l("executedUnderContract"), &e("contract")),
        q(&e("contract"), &l("declaredProtocol"), &l(protocol_local)),
    )
}

/// A per-leg `logic:transactionTimestamp`.
fn timestamp(node: &str, n: i64) -> String {
    q(&e(node), &l("transactionTimestamp"), &int_lit(n))
}

/// Declare lock acquire/release events on a primitive's action schema (`{node}Schema`).
fn locks(node: &str, acquired: &[&str], released: &[&str]) -> String {
    let schema = format!("{node}Schema");
    let mut s = String::new();
    for a in acquired {
        s.push_str(&q(&e(&schema), &l("lockAcquired"), &e(a)));
    }
    for r in released {
        s.push_str(&q(&e(&schema), &l("lockReleased"), &e(r)));
    }
    s
}

/// The boolean logic:protocolEnforced verdict on the history, if present.
fn protocol_enforced(quads: &[crate::teleology::TeleologyQuad]) -> Option<bool> {
    quads
        .iter()
        .find(|q| q.predicate.ends_with("protocolEnforced"))
        .map(|q| q.object.starts_with("\"true\""))
}

#[test]
fn no_declared_protocol_emits_no_protocol_verdict() {
    // The existing serializable world declares no contract → no protocol claim → no verdict.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let nq = format!(
        "{}{}{}{}",
        start_state("s0", &[]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("pa")),
            q(&e("cc"), &l("rightOperand"), &e("pb")),
        ]
        .concat(),
        [
            primitive("pa", &[], &["sitA"], &[]),
            primitive("pb", &[], &["sitB"], &[]),
        ]
        .concat(),
    );
    assert_eq!(protocol_enforced(&outcome_quads(&nq, "cc")), None);
}

/// A read-dependency concurrent world (left writes sitA, right reads sitA then writes sitB;
/// sitA obtains at the shared start) with an optional trailer declaring a protocol + events.
fn protocol_world(trailer: &str) -> String {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    format!(
        "{}{}{}{}{}",
        start_state("s0", &["sitA"]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("l0")),
            q(&e("cc"), &l("rightOperand"), &e("r0")),
        ]
        .concat(),
        [
            primitive("l0", &[], &["sitA"], &[]),
            primitive("r0", &["sitA"], &["sitB"], &[]),
        ]
        .concat(),
        trailer,
    )
}

#[test]
fn timestamp_ordering_enforced_when_edge_respects_timestamps() {
    // One conflict edge l0 → r0 (left writes what right reads); timestamps 1 < 2 respect it.
    let nq = protocol_world(&format!(
        "{}{}{}",
        declares_protocol("TimestampOrdering"),
        timestamp("l0", 1),
        timestamp("r0", 2),
    ));
    assert_eq!(protocol_enforced(&outcome_quads(&nq, "cc")), Some(true));
}

#[test]
fn timestamp_ordering_violated_by_a_conflict_cycle() {
    // The cross-dependency world is conflict-CYCLIC: with timestamps 1,2 one of the opposing
    // edges runs against timestamp order → not enforced, with a reason.
    let stamps = [timestamp("leftSer", 1), timestamp("rightSer", 2)].concat();
    let nq = format!(
        "{}{}{}",
        cross_dependency_world(),
        declares_protocol("TimestampOrdering"),
        stamps,
    );
    let quads = outcome_quads(&nq, "cc");
    assert_eq!(protocol_enforced(&quads), Some(false));
    assert!(
        quads
            .iter()
            .any(|q| q.predicate.ends_with("protocolViolationReason")),
        "a protocol failure carries its reason"
    );
}

#[test]
fn timestamp_ordering_missing_timestamp_is_hard_error() {
    // Declaring timestamp ordering without a leg timestamp is a hard error, never a silent pass.
    let nq = protocol_world(&declares_protocol("TimestampOrdering"));
    let facts = facts_of(&nq);
    let err = emit_transaction_outcome(&facts, W, &format!("{W}#cc")).unwrap_err();
    assert!(err.contains("logic:transactionTimestamp"), "{err}");
}

#[test]
fn strict_two_phase_locking_enforced_and_no_lock_events_is_hard_error() {
    // Each schema acquires then releases its lock within its step → two-phase holds → enforced.
    let nq = protocol_world(&format!(
        "{}{}{}",
        declares_protocol("StrictTwoPhaseLocking"),
        locks("l0", &["sitA"], &["sitA"]),
        locks("r0", &["sitA"], &["sitA"]),
    ));
    assert_eq!(protocol_enforced(&outcome_quads(&nq, "cc")), Some(true));

    // The SAME declaration with NO recorded lock events is a hard error.
    let bare = protocol_world(&declares_protocol("StrictTwoPhaseLocking"));
    let facts = facts_of(&bare);
    let err = emit_transaction_outcome(&facts, W, &format!("{W}#cc")).unwrap_err();
    assert!(err.contains("no logic:lockAcquired"), "{err}");
}

#[test]
fn strong_strict_two_phase_locking_requires_holding_locks_to_commit() {
    // A two-step left leg that RELEASES its lock at the first (non-final) step breaks the
    // held-to-commit discipline of strong-strict 2PL, but still satisfies plain strict 2PL.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let world = |protocol: &str| {
        format!(
            "{}{}{}{}{}{}{}",
            start_state("s0", &[]),
            q(&e("cc"), ty, &l("ConcurrentComposition")),
            [
                q(&e("cc"), &l("transitionFromState"), &e("s0")),
                q(&e("cc"), &l("leftOperand"), &e("leftSer")),
                q(&e("cc"), &l("rightOperand"), &e("pb")),
            ]
            .concat(),
            serial2("leftSer", "l0", "l1"),
            [
                primitive("l0", &[], &["sitA"], &[]),
                primitive("l1", &[], &["sitC"], &[]),
                primitive("pb", &[], &["sitB"], &[]),
            ]
            .concat(),
            // l0 (the FIRST, non-final step of leftSer) acquires AND releases → released before commit.
            locks("l0", &["sitA"], &["sitA"]),
            declares_protocol(protocol),
        )
    };
    assert_eq!(
        protocol_enforced(&outcome_quads(&world("StrongStrictTwoPhaseLocking"), "cc")),
        Some(false),
        "strong-strict forbids releasing a lock before the commit step"
    );
    assert_eq!(
        protocol_enforced(&outcome_quads(&world("StrictTwoPhaseLocking"), "cc")),
        Some(true),
        "plain strict two-phase locking only requires the two-phase order"
    );
}

#[test]
fn optimistic_validation_enforced_iff_no_cross_leg_read_write_conflict() {
    // Disjoint legs → no cross-leg read-write conflict → optimistic validation passes.
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let disjoint = format!(
        "{}{}{}{}{}",
        start_state("s0", &[]),
        q(&e("cc"), ty, &l("ConcurrentComposition")),
        [
            q(&e("cc"), &l("transitionFromState"), &e("s0")),
            q(&e("cc"), &l("leftOperand"), &e("pa")),
            q(&e("cc"), &l("rightOperand"), &e("pb")),
        ]
        .concat(),
        [
            primitive("pa", &[], &["sitA"], &[]),
            primitive("pb", &[], &["sitB"], &[]),
        ]
        .concat(),
        declares_protocol("OptimisticValidation"),
    );
    assert_eq!(
        protocol_enforced(&outcome_quads(&disjoint, "cc")),
        Some(true)
    );

    // The read-dependency world: right READS what left WRITES → a cross-leg read-write
    // conflict → optimistic validation would abort → not enforced.
    let conflicting = protocol_world(&declares_protocol("OptimisticValidation"));
    assert_eq!(
        protocol_enforced(&outcome_quads(&conflicting, "cc")),
        Some(false)
    );
}

// ── Isolation-level adequacy: does the schedule meet the declared strength? ──────

/// Wire the concurrent root `cc` to a contract declaring `level_local`.
fn declares_isolation(level_local: &str) -> String {
    let ty = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    format!(
        "{}{}{}",
        q(&e("contract"), ty, &l("ReasoningContract")),
        q(&e("cc"), &l("executedUnderContract"), &e("contract")),
        q(
            &e("contract"),
            &l("declaredIsolationLevel"),
            &l(level_local)
        ),
    )
}

/// The boolean logic:isolationLevelAdequacy verdict on the history, if present.
fn isolation_adequate(quads: &[crate::teleology::TeleologyQuad]) -> Option<bool> {
    quads
        .iter()
        .find(|q| q.predicate.ends_with("isolationLevelAdequacy"))
        .map(|q| q.object.starts_with("\"true\""))
}

#[test]
fn no_declared_isolation_level_emits_no_adequacy_verdict() {
    // The read-dependency world with no isolation declaration → no adequacy verdict.
    let nq = protocol_world("");
    assert_eq!(isolation_adequate(&outcome_quads(&nq, "cc")), None);
}

#[test]
fn snapshot_and_weaker_levels_are_always_adequate_even_on_a_cycle() {
    // The cross-dependency world is conflict-CYCLIC (a write-skew). The engine realizes snapshot
    // isolation, so every level up to snapshot is met by construction — snapshot ADMITS write skew.
    for level in [
        "ReadUncommittedIsolation",
        "ReadCommittedIsolation",
        "RepeatableReadIsolation",
        "SnapshotIsolation",
    ] {
        let nq = format!("{}{}", cross_dependency_world(), declares_isolation(level));
        assert_eq!(
            isolation_adequate(&outcome_quads(&nq, "cc")),
            Some(true),
            "{level} must be adequate under the snapshot-isolation model even on a cycle"
        );
    }
}

#[test]
fn serializable_isolation_is_inadequate_on_a_cycle_with_reason() {
    // Declared serializable over a conflict-cyclic (write-skew) schedule → inadequate + reason.
    let nq = format!(
        "{}{}",
        cross_dependency_world(),
        declares_isolation("SerializableIsolation")
    );
    let quads = outcome_quads(&nq, "cc");
    assert_eq!(isolation_adequate(&quads), Some(false));
    assert!(
        quads
            .iter()
            .any(|q| q.predicate.ends_with("isolationInadequacyReason")),
        "an inadequate level carries its reason"
    );
}

#[test]
fn serializable_and_opacity_are_adequate_on_a_serializable_schedule() {
    // The read-dependency world is conflict-serializable (one acyclic edge), so BOTH the
    // serializable and opacity strengths are met (opacity coincides with serializable here —
    // there are no aborted/in-flight transactions, only committed runs produce a history).
    for level in ["SerializableIsolation", "OpacityIsolation"] {
        let nq = protocol_world(&declares_isolation(level));
        assert_eq!(
            isolation_adequate(&outcome_quads(&nq, "cc")),
            Some(true),
            "{level} must be adequate for a conflict-serializable schedule"
        );
    }
}

#[test]
fn opacity_is_inadequate_on_a_cycle() {
    // Opacity coincides with serializable in this model: a cycle fails it.
    let nq = format!(
        "{}{}",
        cross_dependency_world(),
        declares_isolation("OpacityIsolation")
    );
    assert_eq!(isolation_adequate(&outcome_quads(&nq, "cc")), Some(false));
}

/// The boolean logic:protocolLevelAdequacy verdict on the history, if present.
fn protocol_level_adequate(quads: &[crate::teleology::TeleologyQuad]) -> Option<bool> {
    quads
        .iter()
        .find(|q| q.predicate.ends_with("protocolLevelAdequacy"))
        .map(|q| q.object.starts_with("\"true\""))
}

#[test]
fn protocol_level_adequacy_absent_without_both_declarations() {
    // Only a protocol (multiversion needs no events) and no declared level → no pairing to judge,
    // so no protocolLevelAdequacy verdict — the ABSENCE of a cross-claim, not a degraded fallback.
    let nq = protocol_world(&declares_protocol("MultiversionConcurrencyControl"));
    assert_eq!(protocol_level_adequate(&outcome_quads(&nq, "cc")), None);
}

#[test]
fn multiversion_protocol_is_enforced_structurally_without_events() {
    // The engine realizes snapshot isolation by construction, so multiversion concurrency control
    // needs no lock/timestamp events and its schedule respects the protocol structurally.
    let nq = protocol_world(&declares_protocol("MultiversionConcurrencyControl"));
    assert_eq!(protocol_enforced(&outcome_quads(&nq, "cc")), Some(true));
}

#[test]
fn multiversion_under_snapshot_is_adequate() {
    // Multiversion concurrency control's guaranteed ceiling IS snapshot isolation → adequate.
    let nq = protocol_world(&format!(
        "{}{}",
        declares_protocol("MultiversionConcurrencyControl"),
        declares_isolation("SnapshotIsolation"),
    ));
    assert_eq!(
        protocol_level_adequate(&outcome_quads(&nq, "cc")),
        Some(true)
    );
}

#[test]
fn multiversion_under_serializable_is_inadequate_with_reason() {
    // Multiversion tops out at snapshot (write skew remains), so it cannot guarantee serializable.
    let nq = protocol_world(&format!(
        "{}{}",
        declares_protocol("MultiversionConcurrencyControl"),
        declares_isolation("SerializableIsolation"),
    ));
    let quads = outcome_quads(&nq, "cc");
    assert_eq!(protocol_level_adequate(&quads), Some(false));
    assert!(
        quads
            .iter()
            .any(|q| q.predicate.ends_with("protocolLevelInadequacyReason")),
        "an inadequate protocol/level pairing carries its reason"
    );
}

#[test]
fn serializability_strength_protocol_reaches_serializable() {
    // A serializability-strength protocol (optimistic validation needs no lock/timestamp events)
    // has a serializable ceiling, so declaring serializable is adequate — INDEPENDENTLY of whether
    // the witnessed schedule happens to pass that protocol's own enforcement check.
    let nq = protocol_world(&format!(
        "{}{}",
        declares_protocol("OptimisticValidation"),
        declares_isolation("SerializableIsolation"),
    ));
    assert_eq!(
        protocol_level_adequate(&outcome_quads(&nq, "cc")),
        Some(true)
    );
}
