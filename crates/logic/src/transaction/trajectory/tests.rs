// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the read-only ToolCall-trajectory audit (T6).
//!
//! Each test builds a small N-Quads world recording a `gmeow:ToolCall` trajectory (anchored by
//! `logic:properPartOf` to a node bearing `logic:transitionFromState`), runs the audit, and
//! asserts the executional-entailment verdict, the per-call step substrate, that a failed
//! trajectory commits NOTHING (atomicity), that the audit never rewrites the recording
//! (read-only), and that the no-optionality hard fails fire.

use super::*;
use crate::store::WorldStore;
use crate::teleology::WorldFacts;

const W: &str = "https://blackcatinformatics.ca/gmeow/examples/traj/world";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A bare `gmeow:` IRI string.
fn gns(local: &str) -> String {
    format!("https://blackcatinformatics.ca/gmeow/{local}")
}

/// A bare `logic:` IRI string.
fn lns(local: &str) -> String {
    format!("https://blackcatinformatics.ca/logic/{local}")
}

/// A bare example IRI string in world `W`.
fn exns(local: &str) -> String {
    format!("{W}#{local}")
}

// Angle-bracket N-Quads forms.
fn ge(local: &str) -> String {
    format!("<{}>", gns(local))
}
fn le(local: &str) -> String {
    format!("<{}>", lns(local))
}
fn xe(local: &str) -> String {
    format!("<{}>", exns(local))
}

/// An `xsd:dateTime` literal in N-Quads form.
fn dt(s: &str) -> String {
    format!("\"{s}\"^^<http://www.w3.org/2001/XMLSchema#dateTime>")
}

/// One N-Quad line in world `W`.
fn q(s: &str, p: &str, o: &str) -> String {
    format!("{s} {p} {o} <{W}> .\n")
}

/// Read the world's facts from N-Quads text.
fn facts_of(nq: &str) -> WorldFacts {
    let store = WorldStore::new();
    store.load_nquads(nq).expect("valid N-Quads");
    WorldFacts::read(&store, W)
}

/// One recorded `gmeow:ToolCall` `node`: typed, anchored to `traj`, bound to `schema`,
/// timestamped at `at` under temporal frame `frame`.
fn tool_call(node: &str, schema: &str, at: &str, frame: &str) -> String {
    let mut s = String::new();
    s += &q(&xe(node), &format!("<{RDF_TYPE}>"), &ge("ToolCall"));
    s += &q(&xe(node), &le("properPartOf"), &xe("traj"));
    s += &q(&xe(node), &le("instantiatesSchema"), &xe(schema));
    s += &q(&xe(node), &ge("atTime"), &dt(at));
    s += &q(&xe(node), &ge("eventTemporalFrame"), &ge(frame));
    s
}

/// A two-call trajectory: store (pre `sReady` → ins `sStored`, del `sReady`) then note
/// (pre `call1_precond` → ins `sNoted`).  `frame1` is the second call's temporal frame, so a
/// caller can force a mixed-frame world.
fn trajectory_world(call1_precond: &str, frame1: &str) -> String {
    let mut s = String::new();
    // Anchor + start state.
    s += &q(&xe("traj"), &le("transitionFromState"), &xe("start0"));
    s += &q(&xe("start0"), &le("situationObtains"), &xe("sReady"));
    // store schema: pre sReady, effect ins sStored / del sReady.
    s += &q(&xe("schemaStore"), &le("precondition"), &xe("sReady"));
    s += &q(&xe("schemaStore"), &le("effect"), &xe("effStore"));
    s += &q(&xe("effStore"), &le("ins"), &xe("sStored"));
    s += &q(&xe("effStore"), &le("del"), &xe("sReady"));
    // note schema: pre <call1_precond>, effect ins sNoted.
    s += &q(&xe("schemaNote"), &le("precondition"), &xe(call1_precond));
    s += &q(&xe("schemaNote"), &le("effect"), &xe("effNote"));
    s += &q(&xe("effNote"), &le("ins"), &xe("sNoted"));
    // Two recorded calls, store before note (by gmeow:atTime).
    s += &tool_call(
        "call0",
        "schemaStore",
        "2026-06-12T17:03:11Z",
        "temporalFrameUTCGregorian",
    );
    s += &tool_call("call1", "schemaNote", "2026-06-12T17:03:14Z", frame1);
    s
}

/// Count derived `logic:TransactionStep` records in the audit output.
fn step_count(out: &[crate::teleology::TeleologyQuad]) -> usize {
    let step_type = format!("<{}>", lns("TransactionStep"));
    out.iter()
        .filter(|qd| qd.predicate == RDF_TYPE && qd.object == step_type)
        .count()
}

/// The verdict (`logic:transactionSucceeds`) literal, if present.
fn verdict(out: &[crate::teleology::TeleologyQuad]) -> Option<&str> {
    out.iter()
        .find(|qd| qd.predicate == lns("transactionSucceeds"))
        .map(|qd| qd.object.as_str())
}

#[test]
fn audit_succeeds_and_emits_one_step_per_toolcall() {
    let nq = trajectory_world("sStored", "temporalFrameUTCGregorian");
    let out = emit_trajectory_audits(&facts_of(&nq), W).expect("audit runs");

    let v = verdict(&out).expect("a transactionSucceeds verdict");
    assert_eq!(
        v, "\"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>",
        "expected an exact xsd:boolean true verdict, got {v:?}"
    );
    assert_eq!(
        step_count(&out),
        2,
        "exactly one logic:TransactionStep per recorded ToolCall"
    );
}

#[test]
fn unmet_precondition_makes_the_trajectory_non_atomic() {
    // The second call's precondition never obtains: executional entailment fails, and an
    // all-or-nothing transaction commits NO step substrate.
    let nq = trajectory_world("sNeverHolds", "temporalFrameUTCGregorian");
    let out = emit_trajectory_audits(&facts_of(&nq), W).expect("audit runs");

    let v = verdict(&out).expect("a transactionSucceeds verdict");
    assert_eq!(
        v, "\"false\"^^<http://www.w3.org/2001/XMLSchema#boolean>",
        "expected an exact xsd:boolean false verdict, got {v:?}"
    );
    assert_eq!(
        step_count(&out),
        0,
        "a failed (non-atomic) trajectory commits no steps"
    );
}

#[test]
fn audit_is_read_only_over_the_recording() {
    let nq = trajectory_world("sStored", "temporalFrameUTCGregorian");
    let out = emit_trajectory_audits(&facts_of(&nq), W).expect("audit runs");

    // The audit emits DERIVED logic: substrate only — never a recording predicate, and never
    // a quad rewriting a recorded ToolCall as its subject.
    let recording_preds = [
        gns("atTime"),
        gns("toolResult"),
        gns("toolArguments"),
        gns("usedTool"),
        gns("eventTemporalFrame"),
        gns("calledByInvocation"),
    ];
    for qd in &out {
        assert!(
            !recording_preds.contains(&qd.predicate),
            "audit must not emit a recording predicate: {}",
            qd.predicate
        );
        assert_ne!(
            qd.subject,
            exns("call0"),
            "recorded call0 must not be rewritten"
        );
        assert_ne!(
            qd.subject,
            exns("call1"),
            "recorded call1 must not be rewritten"
        );
    }
    // Non-vacuous: the audit DID produce a derived verdict.
    assert!(verdict(&out).is_some(), "the audit produced a verdict");
}

#[test]
fn unanchored_bound_toolcall_is_a_hard_fail() {
    // A bound ToolCall (carries logic:instantiatesSchema) with no logic:properPartOf anchor.
    let mut nq = String::new();
    nq += &q(&xe("loose"), &format!("<{RDF_TYPE}>"), &ge("ToolCall"));
    nq += &q(&xe("loose"), &le("instantiatesSchema"), &xe("schemaStore"));
    nq += &q(&xe("loose"), &ge("atTime"), &dt("2026-06-12T17:03:11Z"));
    nq += &q(
        &xe("loose"),
        &ge("eventTemporalFrame"),
        &ge("temporalFrameUTCGregorian"),
    );
    nq += &q(&xe("schemaStore"), &le("effect"), &xe("effStore"));
    nq += &q(&xe("effStore"), &le("ins"), &xe("sStored"));

    let err = emit_trajectory_audits(&facts_of(&nq), W).unwrap_err();
    assert!(err.message().contains("no logic:properPartOf"), "{err}");
}

#[test]
fn anchor_without_transition_from_state_is_a_hard_fail() {
    // A bound ToolCall whose logic:properPartOf anchor EXISTS but bears no
    // logic:transitionFromState start state — the trajectory has no defined start, which is a
    // HARD FAIL (trajectory_roots hard-fails at lines 110-118).
    let mut nq = String::new();
    // The anchor exists (it is the properPartOf target) but has NO transitionFromState.
    nq += &q(&xe("orphan"), &format!("<{RDF_TYPE}>"), &ge("ToolCall"));
    nq += &q(&xe("orphan"), &le("properPartOf"), &xe("anchorNoState"));
    nq += &q(&xe("orphan"), &le("instantiatesSchema"), &xe("schemaStore"));
    nq += &q(&xe("orphan"), &ge("atTime"), &dt("2026-06-12T17:03:11Z"));
    nq += &q(
        &xe("orphan"),
        &ge("eventTemporalFrame"),
        &ge("temporalFrameUTCGregorian"),
    );
    nq += &q(&xe("schemaStore"), &le("effect"), &xe("effStore"));
    nq += &q(&xe("effStore"), &le("ins"), &xe("sStored"));
    // anchorNoState is referenced but has no logic:transitionFromState quad.

    let err = emit_trajectory_audits(&facts_of(&nq), W).unwrap_err();
    assert!(
        err.message().contains("no logic:transitionFromState"),
        "{err}"
    );
}

/// A well-formed single-call trajectory with ONE extra copy of `pred → obj` on the call, so the
/// audited field is multi-valued. Each must hard-fail rather than pick one value by content order.
fn trajectory_with_extra_call_quad(pred: &str, obj: &str) -> String {
    let mut nq = String::new();
    nq += &q(&xe("traj"), &le("transitionFromState"), &xe("start0"));
    nq += &q(&xe("start0"), &le("situationObtains"), &xe("sReady"));
    nq += &q(&xe("schemaStore"), &le("precondition"), &xe("sReady"));
    nq += &q(&xe("schemaStore"), &le("effect"), &xe("effStore"));
    nq += &q(&xe("effStore"), &le("ins"), &xe("sStored"));
    nq += &tool_call(
        "call0",
        "schemaStore",
        "2026-06-12T17:03:11Z",
        "temporalFrameUTCGregorian",
    );
    // The offending second value for the field under test.
    nq += &q(&xe("call0"), pred, obj);
    nq
}

#[test]
fn multi_valued_per_call_fields_are_hard_fails() {
    // A bound ToolCall with TWO gmeow:eventTemporalFrame / gmeow:atTime / logic:instantiatesSchema
    // values must hard-fail (no silent first-wins pick by content order — no-optionality).
    let frame = trajectory_with_extra_call_quad(&ge("eventTemporalFrame"), &ge("temporalFrameTAI"));
    let err = emit_trajectory_audits(&facts_of(&frame), W).unwrap_err();
    assert!(
        err.message()
            .contains("gmeow:eventTemporalFrame values (exactly one is required)"),
        "{err}"
    );

    let at = trajectory_with_extra_call_quad(&ge("atTime"), &dt("2026-06-12T17:03:55Z"));
    let err = emit_trajectory_audits(&facts_of(&at), W).unwrap_err();
    assert!(
        err.message()
            .contains("gmeow:atTime values (exactly one is required)"),
        "{err}"
    );

    let schema = trajectory_with_extra_call_quad(&le("instantiatesSchema"), &xe("schemaOther"));
    let err = emit_trajectory_audits(&facts_of(&schema), W).unwrap_err();
    assert!(
        err.message()
            .contains("logic:instantiatesSchema values (exactly one is required)"),
        "{err}"
    );
}

#[test]
fn mixed_temporal_frame_is_a_hard_fail() {
    // call0 is UTC-Gregorian; call1 is a different frame — a lexical gmeow:atTime sort across
    // frames is incoherent, so the audit hard-fails rather than ordering silently.
    let nq = trajectory_world("sStored", "temporalFrameTAI");
    let err = emit_trajectory_audits(&facts_of(&nq), W).unwrap_err();
    assert!(
        err.message().contains("mixes gmeow:eventTemporalFrame"),
        "{err}"
    );
}

// ── Helpers: concurrency, goals, hypothetical replay ─────────────────────────────

/// A start state `state` bearing the given obtaining situation locals.
fn start_with(state: &str, sits: &[&str]) -> String {
    let mut s = String::new();
    for sit in sits {
        s += &q(&xe(state), &le("situationObtains"), &xe(sit));
    }
    s
}

/// An action schema `name` with optional precondition locals and ins-effect locals.
fn schema(name: &str, pre: &[&str], ins: &[&str]) -> String {
    let eff = format!("{name}Eff");
    let mut s = String::new();
    s += &q(&xe(name), &le("effect"), &xe(&eff));
    for p in pre {
        s += &q(&xe(name), &le("precondition"), &xe(p));
    }
    for i in ins {
        s += &q(&xe(&eff), &le("ins"), &xe(i));
    }
    s
}

/// A ToolCall `node` anchored to `anchor`, bound to `sch`, at time `at` (UTC-Gregorian frame).
fn call_in(anchor: &str, node: &str, sch: &str, at: &str) -> String {
    let mut s = String::new();
    s += &q(&xe(node), &format!("<{RDF_TYPE}>"), &ge("ToolCall"));
    s += &q(&xe(node), &le("properPartOf"), &xe(anchor));
    s += &q(&xe(node), &le("instantiatesSchema"), &xe(sch));
    s += &q(&xe(node), &ge("atTime"), &dt(at));
    s += &q(
        &xe(node),
        &ge("eventTemporalFrame"),
        &ge("temporalFrameUTCGregorian"),
    );
    s
}

/// Whether the output carries a quad typing some subject `logic:<local>`.
fn has_type(out: &[crate::teleology::TeleologyQuad], local: &str) -> bool {
    let ty = format!("<{}>", lns(local));
    out.iter()
        .any(|qd| qd.predicate == RDF_TYPE && qd.object == ty)
}

#[test]
fn concurrent_trajectories_share_a_start_and_serialize_cleanly() {
    // Two trajectories starting from the SAME state with DISJOINT writes → a derived
    // logic:ConcurrentHistory and NO serialization anomaly.
    let mut nq = String::new();
    nq += &start_with("s0", &[]);
    nq += &q(&xe("trajA"), &le("transitionFromState"), &xe("s0"));
    nq += &q(&xe("trajB"), &le("transitionFromState"), &xe("s0"));
    nq += &schema("schA", &[], &["sitA"]);
    nq += &schema("schB", &[], &["sitB"]);
    nq += &call_in("trajA", "callA", "schA", "2026-06-12T17:00:00Z");
    nq += &call_in("trajB", "callB", "schB", "2026-06-12T17:00:00Z");

    let out = emit_trajectory_audits(&facts_of(&nq), W).expect("audit runs");
    assert!(
        has_type(&out, "ConcurrentHistory"),
        "two shared-start trajectories compose into a concurrent history"
    );
    assert!(
        !has_type(&out, "SerializationAnomaly"),
        "disjoint footprints are conflict-serializable"
    );
}

#[test]
fn concurrent_cross_dependency_trajectories_flag_a_serialization_anomaly() {
    // Mirror the engine's cross-dependency schedule via two two-call trajectories: left writes
    // sitX then reads sitY; right writes sitY then reads sitX → an opposing-order conflict cycle.
    let mut nq = String::new();
    nq += &start_with("s0", &["sitX", "sitY"]);
    nq += &q(&xe("trajL"), &le("transitionFromState"), &xe("s0"));
    nq += &q(&xe("trajR"), &le("transitionFromState"), &xe("s0"));
    nq += &schema("schL0", &[], &["sitX"]);
    nq += &schema("schL1", &["sitY"], &["sitZ1"]);
    nq += &schema("schR0", &[], &["sitY"]);
    nq += &schema("schR1", &["sitX"], &["sitZ2"]);
    nq += &call_in("trajL", "callL0", "schL0", "2026-06-12T17:00:00Z");
    nq += &call_in("trajL", "callL1", "schL1", "2026-06-12T17:00:01Z");
    nq += &call_in("trajR", "callR0", "schR0", "2026-06-12T17:00:00Z");
    nq += &call_in("trajR", "callR1", "schR1", "2026-06-12T17:00:01Z");

    let out = emit_trajectory_audits(&facts_of(&nq), W).expect("audit runs");
    assert!(
        has_type(&out, "SerializationAnomaly"),
        "cross-dependency trajectories are NOT conflict-serializable"
    );
    let precedes = out
        .iter()
        .filter(|qd| qd.predicate.ends_with("/precedes"))
        .count();
    assert!(
        precedes >= 2,
        "a two-transaction cycle has conflict edges in both directions, got {precedes}"
    );
}

#[test]
fn hypothetical_replay_emits_goal_reached_without_committing() {
    // A single trajectory under logic:HypotheticalExecution whose end-state reaches the goal.
    let mut nq = String::new();
    nq += &start_with("h0", &["sReady"]);
    nq += &q(&xe("htraj"), &le("transitionFromState"), &xe("h0"));
    nq += &q(&xe("htraj"), &le("executedUnderContract"), &xe("hContract"));
    nq += &q(
        &xe("hContract"),
        &le("executionMode"),
        &le("HypotheticalExecution"),
    );
    nq += &q(&xe("htraj"), &le("planGoal"), &xe("theGoal"));
    nq += &q(&xe("htraj"), &le("planGoalSituation"), &xe("sDone"));
    nq += &schema("schDo", &["sReady"], &["sDone"]);
    nq += &call_in("htraj", "callDo", "schDo", "2026-06-12T17:00:00Z");

    let out = emit_trajectory_audits(&facts_of(&nq), W).expect("audit runs");
    // Goal reached → a derived gmeow:satisfiedBy edge from the goal to the reached situation.
    assert!(
        out.iter().any(|qd| qd.predicate == gns("satisfiedBy")
            && qd.subject == exns("theGoal")
            && qd.object == format!("<{}>", exns("sDone"))),
        "an alternative continuation that reaches the goal emits the satisfiedBy verdict"
    );
    // Hypothetical isolation: a content-addressed witness, and NO committed step substrate.
    assert!(
        out.iter()
            .any(|qd| qd.predicate == lns("executedHypotheticallyAs")),
        "hypothetical run records its witness"
    );
    assert_eq!(
        step_count(&out),
        0,
        "a hypothetical run discards its committed effects"
    );
}

#[test]
fn goal_not_reached_emits_no_satisfiedby_verdict() {
    // The trajectory succeeds but never reaches the goal situation: no satisfiedBy verdict.
    let mut nq = String::new();
    nq += &start_with("g0", &["sReady"]);
    nq += &q(&xe("gtraj"), &le("transitionFromState"), &xe("g0"));
    nq += &q(&xe("gtraj"), &le("planGoal"), &xe("theGoal"));
    nq += &q(&xe("gtraj"), &le("planGoalSituation"), &xe("sUnreached"));
    nq += &schema("schDo", &["sReady"], &["sOther"]);
    nq += &call_in("gtraj", "callDo", "schDo", "2026-06-12T17:00:00Z");

    let out = emit_trajectory_audits(&facts_of(&nq), W).expect("audit runs");
    assert!(
        !out.iter().any(|qd| qd.predicate == gns("satisfiedBy")),
        "a goal that is not reached emits no satisfiedBy verdict"
    );
}

#[test]
fn plangoal_without_situation_is_a_hard_fail() {
    let mut nq = String::new();
    nq += &start_with("g0", &["sReady"]);
    nq += &q(&xe("gtraj"), &le("transitionFromState"), &xe("g0"));
    nq += &q(&xe("gtraj"), &le("planGoal"), &xe("theGoal"));
    nq += &schema("schDo", &["sReady"], &["sOther"]);
    nq += &call_in("gtraj", "callDo", "schDo", "2026-06-12T17:00:00Z");

    let err = emit_trajectory_audits(&facts_of(&nq), W).unwrap_err();
    assert!(err.message().contains("logic:planGoalSituation"), "{err}");
}

#[test]
fn three_trajectories_sharing_a_start_is_a_hard_fail() {
    // Conflict-serializability composes exactly two concurrent legs.
    let mut nq = String::new();
    nq += &start_with("s0", &[]);
    nq += &schema("sch", &[], &["sitA"]);
    for (anchor, call, at) in [
        ("t1", "c1", "2026-06-12T17:00:00Z"),
        ("t2", "c2", "2026-06-12T17:00:00Z"),
        ("t3", "c3", "2026-06-12T17:00:00Z"),
    ] {
        nq += &q(&xe(anchor), &le("transitionFromState"), &xe("s0"));
        nq += &call_in(anchor, call, "sch", at);
    }

    let err = emit_trajectory_audits(&facts_of(&nq), W).unwrap_err();
    assert!(err.message().contains("shared by 3 trajectories"), "{err}");
}
