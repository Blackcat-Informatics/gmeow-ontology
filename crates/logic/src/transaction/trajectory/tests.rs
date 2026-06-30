// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests for the read-only ToolCall-trajectory audit (T6, issue #716).
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
    assert!(v.contains("true"), "expected a success verdict, got {v:?}");
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
    assert!(v.contains("false"), "expected a failure verdict, got {v:?}");
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
    assert!(err.contains("no logic:properPartOf"), "{err}");
}

#[test]
fn mixed_temporal_frame_is_a_hard_fail() {
    // call0 is UTC-Gregorian; call1 is a different frame — a lexical gmeow:atTime sort across
    // frames is incoherent, so the audit hard-fails rather than ordering silently.
    let nq = trajectory_world("sStored", "temporalFrameTAI");
    let err = emit_trajectory_audits(&facts_of(&nq), W).unwrap_err();
    assert!(err.contains("mixes gmeow:eventTemporalFrame"), "{err}");
}
