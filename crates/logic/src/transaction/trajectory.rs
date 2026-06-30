// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! T6 — read-only audit of recorded agentic `gmeow:ToolCall` trajectories.
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

use crate::provenance::{mint_derivation_id, sha1_hex, LOGIC_NAMESPACE};
use crate::teleology::{n3, triple_reifier, TeleologyQuad, WorldFacts};

use super::{
    emit_program_outcome, logic, plan_path, root_execution_mode, root_start, ExecutionMode,
    StepCounter, TransactionProgram, INSTANTIATES_SCHEMA, TRANSACTION_RULE_IRI,
    TRANSITION_FROM_STATE,
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
const SATISFIED_BY: &str = "satisfiedBy";
// logic: the mereological spine the trajectory anchor groups its calls on, plus the
// plan→goal link the goal-reachability check reads (reused, never minted).
const PROPER_PART_OF: &str = "properPartOf";
const PLAN_GOAL: &str = "planGoal";
const PLAN_GOAL_SITUATION: &str = "planGoalSituation";

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
    let mut frames: BTreeSet<&str> = BTreeSet::new();
    for call in &calls {
        let call_frames = facts.objects(call, &gmeow(EVENT_TEMPORAL_FRAME));
        match call_frames.len() {
            1 => {
                frames.insert(call_frames[0]);
            }
            0 => {
                return Err(format!(
                    "gmeow:ToolCall {call:?} in trajectory {anchor:?} declares no \
                     gmeow:eventTemporalFrame (Principle 11: every crisp timestamp names its \
                     frame)"
                ))
            }
            n => {
                return Err(format!(
                    "gmeow:ToolCall {call:?} in trajectory {anchor:?} has {n} \
                     gmeow:eventTemporalFrame values (exactly one is required)"
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
        let at_times = facts.objects_n3(&call, &gmeow(AT_TIME));
        let at_time = match at_times.len() {
            1 => at_times[0].to_owned(),
            0 => {
                return Err(format!(
                    "gmeow:ToolCall {call:?} in trajectory {anchor:?} has no gmeow:atTime"
                ))
            }
            n => {
                return Err(format!(
                    "gmeow:ToolCall {call:?} in trajectory {anchor:?} has {n} gmeow:atTime \
                     values (exactly one is required)"
                ))
            }
        };
        let schemas = facts.objects(&call, &logic(INSTANTIATES_SCHEMA));
        let schema = match schemas.len() {
            1 => schemas[0].to_owned(),
            0 => {
                return Err(format!(
                    "gmeow:ToolCall {call:?} in trajectory {anchor:?} lost its \
                     logic:instantiatesSchema target"
                ))
            }
            n => {
                return Err(format!(
                    "gmeow:ToolCall {call:?} in trajectory {anchor:?} has {n} \
                     logic:instantiatesSchema values (exactly one is required)"
                ))
            }
        };
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

/// One discovered trajectory resolved to an executable program from its start state.
struct ResolvedTrajectory {
    anchor: String,
    sits: BTreeSet<String>,
    program: TransactionProgram,
}

/// Audit every recorded trajectory in a world and RETURN the derived `logic:TransactionOutcome`
/// substrate (verdict + path/step supersession, or — under a hypothetical anchor — the witness).
///
/// Trajectories are grouped by their shared `logic:transitionFromState` START STATE: a start
/// with ONE trajectory is audited as a standalone serial transaction; a start shared by TWO
/// trajectories is audited as their `logic:ConcurrentComposition` — the deliberate, mint-free
/// way to record concurrency (an explicit `logic:ConcurrentComposition` node over the anchors
/// would be picked up by [`super::program_roots`] and mis-parsed).  Reusing a start-state IRI is
/// the author's explicit concurrency declaration.  A standalone trajectory whose anchor names a
/// `logic:planGoal` additionally gets a goal-reachability verdict (see [`goal_reachability`]).
///
/// Read-only: the returned quads are DERIVED; the recorded ToolCall facts are never mutated.
/// Wired into the family-9 transaction-evaluation pass; emits nothing when a world carries no
/// bound ToolCall trajectory (production `module.ttl` carries none — slice examples are not
/// reasoned), so existing goldens stay byte-stable.
///
/// # Errors
///
/// Propagates discovery faults ([`trajectory_roots`]); a start state shared by MORE than two
/// trajectories (the conflict-serializability engine composes two legs) is a HARD FAIL; and any
/// structural emission fault from [`super::emit_program_outcome`] propagates.
pub(crate) fn emit_trajectory_audits(
    facts: &WorldFacts,
    world: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    // Resolve each trajectory and group by its shared start state (content-sorted keys for
    // deterministic emission order).
    let mut by_start: BTreeMap<String, Vec<ResolvedTrajectory>> = BTreeMap::new();
    for tr in trajectory_roots(facts)? {
        let (start, sits) = root_start(facts, &tr.anchor)?;
        let program = synthesize_program(&tr.anchor, &tr.steps);
        by_start.entry(start).or_default().push(ResolvedTrajectory {
            anchor: tr.anchor,
            sits,
            program,
        });
    }

    let mut out = Vec::new();
    for (start, mut members) in by_start {
        // Deterministic leg order: by anchor IRI.
        members.sort_by(|a, b| a.anchor.cmp(&b.anchor));
        match members.as_slice() {
            [solo] => {
                // Standalone serial trajectory — committed or hypothetical per its anchor.
                // Ground the outcome on the anchor's logic:transitionFromState quad (a REAL
                // input quad — the synthesized program type is not in the facts, so the explain
                // engine could not resolve a type-based reifier).
                let mode = root_execution_mode(facts, &solo.anchor)?;
                let source = triple_reifier(&solo.anchor, &logic(TRANSITION_FROM_STATE), &start)?;
                out.extend(emit_program_outcome(
                    facts,
                    world,
                    &solo.anchor,
                    &solo.program,
                    mode,
                    &start,
                    &solo.sits,
                    &source,
                )?);
                out.extend(goal_reachability(
                    facts,
                    world,
                    &solo.anchor,
                    &solo.program,
                    &start,
                    &solo.sits,
                )?);
            }
            [left, right] => {
                // Two trajectories from one start = concurrent composition; conflict-
                // serializability is a COMMITTED-history property (emit_concurrent_history
                // re-runs both legs from the shared start and classifies the conflict graph).
                let conc_node = format!(
                    "{LOGIC_NAMESPACE}txconcurrent/{}",
                    sha1_hex(&format!("{}\n{}\n{start}", left.anchor, right.anchor))
                );
                let program = TransactionProgram::Concurrent {
                    node: conc_node.clone(),
                    left: Box::new(left.program.clone()),
                    right: Box::new(right.program.clone()),
                };
                // Ground on the left leg's logic:transitionFromState quad (real input quad).
                let source = triple_reifier(&left.anchor, &logic(TRANSITION_FROM_STATE), &start)?;
                out.extend(emit_program_outcome(
                    facts,
                    world,
                    &conc_node,
                    &program,
                    ExecutionMode::Committed,
                    &start,
                    &left.sits,
                    &source,
                )?);
            }
            many => {
                return Err(format!(
                    "start state {start:?} is shared by {} trajectories; conflict-serializability \
                     composes exactly two concurrent legs (split a >2-way schedule into pairs)",
                    many.len()
                ))
            }
        }
    }
    Ok(out)
}

/// Emit the goal-reachability verdict for a trajectory whose anchor names a `logic:planGoal`.
///
/// Reuses the EXISTING flat `gmeow:satisfiedBy` projection (no new vocabulary): re-runs the
/// program (the verdict is computed identically in committed and HYPOTHETICAL mode — only
/// emission of effects differs, so "would an alternative continuation reach the goal?" is
/// answered without committing it) and, when the end-state support contains the anchor's
/// `logic:planGoalSituation`, emits `<goal> gmeow:satisfiedBy <goalSituation>`.  Presence of that
/// edge IS the verdict; its absence means the goal was not reached.  Emitting a
/// `gmeow:satisfiedBy` edge deliberately trips the family-1 `GOAL_EVAL_COLLAPSE_DROP` disclosure,
/// flipping the run's preservation to `SoundUnder` (correct lossy-projection disclosure).
///
/// # Errors
///
/// An anchor naming `logic:planGoal` but no `logic:planGoalSituation` is a HARD FAIL (the
/// success criterion would be unstated); a structural fault from [`plan_path`] propagates.
fn goal_reachability(
    facts: &WorldFacts,
    world: &str,
    anchor: &str,
    program: &TransactionProgram,
    start: &str,
    sits: &BTreeSet<String>,
) -> Result<Vec<TeleologyQuad>, String> {
    let goal = match facts.object(anchor, &logic(PLAN_GOAL)) {
        Some(g) => g,
        None => return Ok(Vec::new()),
    };
    let goal_situation = facts
        .object(anchor, &logic(PLAN_GOAL_SITUATION))
        .ok_or_else(|| {
            format!(
                "trajectory anchor {anchor:?} names a logic:planGoal but no \
                 logic:planGoalSituation (the situation that counts as reaching the goal)"
            )
        })?;

    // The verdict (path existence + the situations obtaining at the end) is mode-independent —
    // [`plan_path`] is pure, so re-running it here under a fresh counter yields the same
    // end-state support whether the run commits its effects or discards them.
    let mut counter = StepCounter::new();
    let outcome = plan_path(facts, program, start, sits, anchor, &mut counter)?;
    if !(outcome.succeeded() && outcome.sits_end.contains(goal_situation)) {
        return Ok(Vec::new());
    }

    // Grounding provenance: the verdict rests on the anchor's logic:planGoal link.
    let source = triple_reifier(anchor, &logic(PLAN_GOAL), goal)?;
    let deriv = mint_derivation_id(TRANSACTION_RULE_IRI, &[source.as_str()]);
    Ok(vec![TeleologyQuad {
        graph: world.to_owned(),
        subject: goal.to_owned(),
        predicate: gmeow(SATISFIED_BY),
        object: n3(goal_situation),
        rule_iri: TRANSACTION_RULE_IRI.to_owned(),
        source_quad_ids: vec![source],
        derivation_id: deriv,
    }])
}

#[cfg(test)]
mod tests;
