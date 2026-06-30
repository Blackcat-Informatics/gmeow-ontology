// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! T6 — read-only audit of recorded agentic `gmeow:ToolCall` trajectories (issue #716).
//!
//! A recorded tool-call sequence is treated AS a Transaction-Logic transaction and verified
//! against the existing TR Evolution-facet engine ([`super`]): pre/postconditions (per-step
//! `logic:precondition` gating + `logic:effect` application), atomicity
//! (`ExecOutcome::succeeded` — executional entailment is all-or-nothing), and — under a
//! `logic:HypotheticalExecution` anchor — sandboxed hypothetical replay.  The audit is a
//! READ-ONLY consumer: it reads the world's facts and RETURNS derived
//! `logic:TransactionOutcome` substrate; it never mutates the recorded ToolCall facts
//! (Principle 8 — the reasoner is QA, never the consumer's prerequisite; Principle 10 holds
//! for free because nothing is written, so nothing is erased).
//!
//! # The bridge (no new vocabulary)
//!
//! A `gmeow:ToolCall` is a `gmeow:Event`; the existing `logic:instantiatesSchema` edge already
//! links any event occurrence to the `logic:ActionSchema` it instantiates.  The adapter:
//!
//! 1. discovers BOUND ToolCalls (those carrying `logic:instantiatesSchema`) grouped under a
//!    SINGLE canonical trajectory anchor — the `logic:properPartOf` whole that bears
//!    `logic:transitionFromState` (the start state).  `gmeow:calledByInvocation` is NOT a
//!    grouping key: `module.ttl` declares it OPTIONAL, and grouping on an optional property is
//!    a latent silent-skip.  A bound ToolCall not reachable from such an anchor is a HARD FAIL.
//! 2. orders a trajectory's calls by `gmeow:atTime` (ties broken by the call IRI — total and
//!    deterministic).  All calls MUST share one `gmeow:eventTemporalFrame` (Principle 11); a
//!    mixed-frame trajectory is a HARD FAIL (a lexical sort across frames is incoherent).
//! 3. right-folds the ordered primitives into the BINARY `logic:SerialConjunction`
//!    `Serial(P0, Serial(P1, …))` — one `Primitive` per ToolCall, hence one materialized
//!    `logic:TransactionStep` per ToolCall.
//! 4. delegates emission to the single [`super::emit_program_outcome`] path — no duplicated
//!    committed/hypothetical/concurrent branches.

use std::collections::{BTreeMap, BTreeSet};

use crate::provenance::{sha1_hex, LOGIC_NAMESPACE};
use crate::teleology::{TeleologyQuad, WorldFacts};

use super::{
    emit_program_outcome, logic, root_execution_mode, root_start, TransactionProgram,
    INSTANTIATES_SCHEMA, TRANSITION_FROM_STATE,
};

/// The `gmeow:` namespace.
const GMEOW_NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// Build a `gmeow:`-namespaced IRI string.
fn gmeow(local: &str) -> String {
    format!("{GMEOW_NAMESPACE}{local}")
}

// gmeow: vocabulary local names the audit reads.
const TOOL_CALL: &str = "ToolCall";
const AT_TIME: &str = "atTime";
const EVENT_TEMPORAL_FRAME: &str = "eventTemporalFrame";
// logic: the mereological spine the trajectory anchor groups its calls on.
const PROPER_PART_OF: &str = "properPartOf";

/// One recorded trajectory: the anchor (the `logic:properPartOf` whole bearing the start
/// state) and its bound `gmeow:ToolCall` steps, ordered by `gmeow:atTime`.
pub(crate) struct TrajectoryRoot {
    /// The anchor IRI — the program-identity root that salts the synthesized program.
    pub anchor: String,
    /// `(call_iri, schema_iri)` pairs in temporal order (one per bound ToolCall).
    pub steps: Vec<(String, String)>,
}

/// Discover the recorded trajectories of a world: bound `gmeow:ToolCall`s grouped under their
/// `logic:properPartOf` anchor, each anchor temporally ordered.
///
/// A "bound" ToolCall is one carrying `logic:instantiatesSchema` (an UNbound ToolCall is plain
/// provenance, not a transaction step, and is correctly ignored).  Returns the anchors in
/// content-sorted order so emission is deterministic.
///
/// # Errors
///
/// HARD FAIL on: a bound ToolCall with no (or more than one) `logic:properPartOf` anchor; an
/// anchor with no `logic:transitionFromState`; a call with no `gmeow:atTime` or no
/// `gmeow:eventTemporalFrame`; or a trajectory mixing `gmeow:eventTemporalFrame` values.
pub(crate) fn trajectory_roots(facts: &WorldFacts) -> Result<Vec<TrajectoryRoot>, String> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for call in facts.subjects_with_type(&gmeow(TOOL_CALL)) {
        // Only ToolCalls bound to an action schema participate in the audit.
        if facts.object(&call, &logic(INSTANTIATES_SCHEMA)).is_none() {
            continue;
        }
        let anchors = facts.objects(&call, &logic(PROPER_PART_OF));
        let anchor = match anchors.len() {
            1 => anchors[0].to_owned(),
            0 => {
                return Err(format!(
                    "bound gmeow:ToolCall {call:?} has no logic:properPartOf trajectory anchor \
                     (a bound ToolCall must be a proper part of an anchor bearing \
                     logic:transitionFromState)"
                ))
            }
            n => {
                return Err(format!(
                    "bound gmeow:ToolCall {call:?} has {n} logic:properPartOf anchors \
                     (exactly one trajectory anchor required)"
                ))
            }
        };
        if facts
            .object(&anchor, &logic(TRANSITION_FROM_STATE))
            .is_none()
        {
            return Err(format!(
                "trajectory anchor {anchor:?} (logic:properPartOf whole of bound gmeow:ToolCall \
                 {call:?}) has no logic:transitionFromState start state"
            ));
        }
        groups.entry(anchor).or_default().push(call);
    }

    let mut roots = Vec::with_capacity(groups.len());
    for (anchor, calls) in groups {
        let steps = order_steps(facts, &anchor, calls)?;
        roots.push(TrajectoryRoot { anchor, steps });
    }
    Ok(roots)
}

/// Order one anchor's bound calls by `gmeow:atTime` (tie-break by call IRI) and resolve each
/// call's `logic:instantiatesSchema`, enforcing a single shared `gmeow:eventTemporalFrame`.
fn order_steps(
    facts: &WorldFacts,
    anchor: &str,
    calls: Vec<String>,
) -> Result<Vec<(String, String)>, String> {
    // Enforce a single temporal frame across the trajectory (Principle 11): a lexical sort of
    // gmeow:atTime literals is coherent only within one frame.
    let mut frames: BTreeSet<String> = BTreeSet::new();
    for call in &calls {
        match facts.object(call, &gmeow(EVENT_TEMPORAL_FRAME)) {
            Some(frame) => {
                frames.insert(frame.to_owned());
            }
            None => {
                return Err(format!(
                    "gmeow:ToolCall {call:?} in trajectory {anchor:?} declares no \
                     gmeow:eventTemporalFrame (Principle 11: every crisp timestamp names its \
                     frame)"
                ))
            }
        }
    }
    if frames.len() > 1 {
        return Err(format!(
            "trajectory {anchor:?} mixes gmeow:eventTemporalFrame values {frames:?}; a single \
             frame is required to order gmeow:atTime coherently"
        ));
    }

    // (atTime literal, call IRI, schema IRI) — sorting on the leading two fields gives a total,
    // deterministic temporal order (the call IRI breaks equal-timestamp ties).
    let mut keyed: Vec<(String, String, String)> = Vec::with_capacity(calls.len());
    for call in calls {
        let at_time = facts
            .object_n3(&call, &gmeow(AT_TIME))
            .ok_or_else(|| {
                format!("gmeow:ToolCall {call:?} in trajectory {anchor:?} has no gmeow:atTime")
            })?
            .to_owned();
        let schema = facts
            .object(&call, &logic(INSTANTIATES_SCHEMA))
            .ok_or_else(|| {
                format!(
                    "gmeow:ToolCall {call:?} in trajectory {anchor:?} lost its \
                     logic:instantiatesSchema target"
                )
            })?
            .to_owned();
        keyed.push((at_time, call, schema));
    }
    keyed.sort();
    Ok(keyed
        .into_iter()
        .map(|(_, call, schema)| (call, schema))
        .collect())
}

/// Right-fold the ordered steps into a binary `logic:SerialConjunction` program (a single
/// `Primitive` when the trajectory has one step).  Synthetic serial nodes are content-addressed
/// off the anchor + the right-hand call so the minted state/step IRIs are stable run-to-run.
fn synthesize_program(anchor: &str, steps: &[(String, String)]) -> TransactionProgram {
    // Build from the tail so the fold nests right: Serial(P0, Serial(P1, … Pn)).
    let mut iter = steps.iter().rev();
    let (last_call, last_schema) = iter
        .next()
        .expect("trajectory_roots only yields trajectories with at least one step");
    let mut program = TransactionProgram::Primitive {
        node: last_call.clone(),
        schema: last_schema.clone(),
    };
    for (call, schema) in iter {
        let serial_node = format!(
            "{LOGIC_NAMESPACE}txserial/{}",
            sha1_hex(&format!("{anchor}\n{call}"))
        );
        program = TransactionProgram::Serial {
            node: serial_node,
            left: Box::new(TransactionProgram::Primitive {
                node: call.clone(),
                schema: schema.clone(),
            }),
            right: Box::new(program),
        };
    }
    program
}

/// Audit every recorded trajectory in a world and RETURN the derived `logic:TransactionOutcome`
/// substrate (verdict + path/step supersession, or — under a hypothetical anchor — the witness).
///
/// Read-only: the returned quads are DERIVED; the recorded ToolCall facts are never mutated.
/// Wired into the family-9 transaction-evaluation pass; emits nothing when a world carries no
/// bound ToolCall trajectory (production `module.ttl` carries none — slice examples are not
/// reasoned), so existing goldens stay byte-stable.
///
/// # Errors
///
/// Propagates discovery faults ([`trajectory_roots`]) and any structural emission fault from
/// [`super::emit_program_outcome`].
pub(crate) fn emit_trajectory_audits(
    facts: &WorldFacts,
    world: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    let mut out = Vec::new();
    for tr in trajectory_roots(facts)? {
        let (start, sits) = root_start(facts, &tr.anchor)?;
        let mode = root_execution_mode(facts, &tr.anchor)?;
        let program = synthesize_program(&tr.anchor, &tr.steps);
        out.extend(emit_program_outcome(
            facts, world, &tr.anchor, &program, mode, &start, &sits,
        )?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
