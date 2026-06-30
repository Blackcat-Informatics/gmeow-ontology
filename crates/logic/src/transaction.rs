// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native Rust interpreter for the transaction-program combinators of the
//! transaction-path Evolution facet — Transaction Logic (Bonner–Kifer).
//!
//! Where [`crate::teleology`] CLASSIFIES given structure (goal satisfaction, plan
//! success, deontic ideals) over an authored path, this module EXECUTES a
//! transaction *program* and decides its **executional entailment**: a transaction
//! succeeds iff there exists a path from the start state along which the program
//! holds, and failure leaves the start state untouched.
//!
//! # Pure, situation-level, store-free
//!
//! The interpreter is a PURE function over an evolving situation-set — it never
//! mutates a store. Every elementary step is computed with
//! [`crate::teleology::apply_effect_over`] (the situation substrate family 6 uses),
//! threading the support set `(sits ∪ ins) \ del` forward across engine-minted
//! successor states. Because nothing is ever written, "failure" is simply "return an
//! empty path and emit nothing" — there is nothing to roll back, so the
//! constitutional suppression-never-erasure discipline (Principle 10) holds for free.
//!
//! # The combinators
//!
//! - **Primitive** — a program node carrying `logic:instantiatesSchema`; executable iff
//!   the schema's `logic:precondition` situations all hold in the current support set;
//!   its effect is an ins/del supersession over that set.
//! - **`logic:SerialConjunction` (φ ⊗ ψ)** — path-SPLITTING: execute φ from the start to
//!   some mid state, then ψ from the mid state; the split point is forced (φ's end).
//! - **`logic:Choice`** — dispatch on whether `logic:guardSituation` holds at the current
//!   state; left when it holds, right when it does not (NOT a fallback).
//! - **`logic:Fallback`** — try the primary (left); if its executional entailment does not
//!   hold, execute the alternate (right).
//! - **`logic:Iteration`** — while `logic:iterationCondition` holds, execute
//!   `logic:iterationBody`; a step bound keeps it terminating.
//! - **`logic:ConcurrentComposition` (φ ∥ ψ)** — both sub-programs execute and their
//!   elementary steps are interleaved over a shared support; the engine DERIVES a
//!   `logic:ConcurrentHistory` (its `logic:precedes` conflict edges) from the two legs'
//!   read/write footprints under a deterministic index-order interleaving and reuses
//!   [`crate::teleology::detect_serialization_anomaly`] to classify it. A cyclic conflict
//!   graph surfaces as a `logic:SerializationAnomaly` finding — a HISTORY-LEVEL result,
//!   never a contradiction witness and never silently linearized. The serializability
//!   *criterion* (a history property), *isolation level* (a guarantee strength), and
//!   *concurrency-control protocol* (a mechanism) stay three separately-declared concerns;
//!   this evaluator decides conflict-serializability only.
//!
//! # No-optionality
//!
//! A malformed program (no recognized program type, more than one, a binary combinator
//! missing or doubling an operand, an operand graph beyond [`MAX_PROGRAM_DEPTH`], a
//! primitive schema with no effect, or a non-terminating program beyond
//! [`MAX_TRANSACTION_STEPS`]) is a HARD ERROR — never a silent default. A
//! *precondition* that does not hold is NOT an error: it is a normal execution failure
//! that `Fallback` / `Choice` route on.

use std::collections::BTreeSet;

use crate::provenance::{sha1_hex, LOGIC_NAMESPACE};
use crate::teleology::{apply_effect_over, SuccessorSupport, WorldFacts};

// ── Namespace + vocabulary local names ──────────────────────────────────────────

/// The `rdf:type` predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Build a `logic:`-namespaced IRI string.
fn logic(local: &str) -> String {
    format!("{LOGIC_NAMESPACE}{local}")
}

// Combinator class local names.
const SERIAL_CONJUNCTION: &str = "SerialConjunction";
const CHOICE: &str = "Choice";
const FALLBACK: &str = "Fallback";
const ITERATION: &str = "Iteration";
const CONCURRENT_COMPOSITION: &str = "ConcurrentComposition";

/// The closed set of program-combinator class local names — the nodes
/// [`program_roots`] recognizes as transaction-program operators.
const COMBINATOR_CLASSES: &[&str] = &[
    SERIAL_CONJUNCTION,
    CHOICE,
    FALLBACK,
    ITERATION,
    CONCURRENT_COMPOSITION,
];

// Structure property local names.
const LEFT_OPERAND: &str = "leftOperand";
const RIGHT_OPERAND: &str = "rightOperand";
const GUARD_SITUATION: &str = "guardSituation";
const ITERATION_BODY: &str = "iterationBody";
const ITERATION_CONDITION: &str = "iterationCondition";
const INSTANTIATES_SCHEMA: &str = "instantiatesSchema";
const TRANSITION_FROM_STATE: &str = "transitionFromState";
const TRANSITION_TO_STATE: &str = "transitionToState";
const SITUATION_OBTAINS: &str = "situationObtains";
const PRECONDITION: &str = "precondition";
const EFFECT: &str = "effect";
// Effect-footprint local names (the situations an elementary step writes).
const INS: &str = "ins";
const DEL: &str = "del";

// Execution-commitment facet local names (the hypothetical/sandbox operator).
const EXECUTED_UNDER_CONTRACT: &str = "executedUnderContract";
const EXECUTION_MODE: &str = "executionMode";
const COMMITTED_EXECUTION: &str = "CommittedExecution";
const HYPOTHETICAL_EXECUTION: &str = "HypotheticalExecution";

// ── Determinism / termination bounds ────────────────────────────────────────────

/// The recursion-depth ceiling for parsing a program's operand graph — a malformed
/// cyclic operand graph is bounded here rather than overflowing the stack.  Mirrors
/// `teleology::MAX_GOAL_DEPTH`.
const MAX_PROGRAM_DEPTH: usize = 256;

/// The hard non-termination backstop on total execution work units (elementary steps +
/// loop passes).  Set far above any conformance fixture: it is a guard against a
/// non-terminating program, NOT a semantic limit.  Exceeding it is a hard error, never a
/// silent truncation — keeping the evaluator terminating / PTIME-data.
const MAX_TRANSACTION_STEPS: usize = 4096;

/// The firing rule IRI stamped on every materialized transaction record
/// (`logic:rule/transaction`), mirroring `teleology::TELEOLOGY_RULE_IRI`.
pub(crate) const TRANSACTION_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/transaction";

// ── The transaction-program IR ──────────────────────────────────────────────────

/// A transaction-program IR node parsed from the RDF of one world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransactionProgram {
    /// A primitive elementary step: a program node carrying
    /// `logic:instantiatesSchema schema`.  Executable iff the schema's preconditions
    /// hold; its effect is an ins/del supersession.
    Primitive { node: String, schema: String },
    /// `logic:SerialConjunction` (φ ⊗ ψ): `left` over a prefix, `right` over the suffix.
    Serial {
        node: String,
        left: Box<TransactionProgram>,
        right: Box<TransactionProgram>,
    },
    /// `logic:Choice`: dispatch on `guard`; `left` when it holds, `right` when it does not.
    Choice {
        node: String,
        guard: String,
        left: Box<TransactionProgram>,
        right: Box<TransactionProgram>,
    },
    /// `logic:Fallback`: try `left`; on executional-entailment failure, execute `right`.
    Fallback {
        node: String,
        left: Box<TransactionProgram>,
        right: Box<TransactionProgram>,
    },
    /// `logic:Iteration`: while `condition` holds at the current state, execute `body`.
    Iteration {
        node: String,
        condition: String,
        body: Box<TransactionProgram>,
    },
    /// `logic:ConcurrentComposition` (φ ∥ ψ): `left` and `right` advance together, their
    /// elementary steps interleaved over a shared support; the derived conflict graph
    /// (`logic:ConcurrentHistory`) is classified for serialization anomalies.
    Concurrent {
        node: String,
        left: Box<TransactionProgram>,
        right: Box<TransactionProgram>,
    },
}

impl TransactionProgram {
    /// The RDF node this program was parsed from — the transaction-program identity used
    /// to name conflict edges and the audit trail back to the authored sub-programs.
    fn node(&self) -> &str {
        match self {
            Self::Primitive { node, .. }
            | Self::Serial { node, .. }
            | Self::Choice { node, .. }
            | Self::Fallback { node, .. }
            | Self::Iteration { node, .. }
            | Self::Concurrent { node, .. } => node,
        }
    }
}

// ── The execution outcome ───────────────────────────────────────────────────────

/// One elementary step actually applied along the executed path: the situation-level
/// support to materialize as a supersession.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedStep {
    /// The predecessor state the step started from.
    pub from_state: String,
    /// The engine-minted successor state the step produced.
    pub to_state: String,
    /// The engine-minted runtime `logic:TransactionStep` node — UNIQUE per pass — that
    /// this step materializes and that the supersession quartet attributes to.
    pub attribution: String,
    /// The action schema whose effect this step applied (grounds the provenance).
    pub schema: String,
    /// The computed successor support (asserted + retired).
    pub support: SuccessorSupport,
}

/// The result of executing a transaction program from a start state.
///
/// `path` carries the executed states (start..=end); an **empty** path means **failure**
/// (`succeeded()` is `false`). A zero-move success (e.g. an iteration whose condition is
/// false at the start) carries the single start state. `sits_end` is the situation
/// support obtaining at the end state (meaningful only on success).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecOutcome {
    pub path: Vec<String>,
    pub steps: Vec<PlannedStep>,
    pub sits_end: BTreeSet<String>,
}

impl ExecOutcome {
    /// A failed execution: empty path, nothing emitted, start untouched.
    fn failure() -> Self {
        Self {
            path: Vec::new(),
            steps: Vec::new(),
            sits_end: BTreeSet::new(),
        }
    }

    /// Whether the program succeeded (a path from the start exists).
    pub(crate) fn succeeded(&self) -> bool {
        !self.path.is_empty()
    }

    /// The end state of the executed path, if any.
    fn end_state(&self) -> Option<&String> {
        self.path.last()
    }
}

/// A global work-unit governor that makes execution terminating: every elementary step
/// and every loop pass consumes one unit; exceeding [`MAX_TRANSACTION_STEPS`] is a hard
/// error.
pub(crate) struct StepCounter {
    count: usize,
}

impl StepCounter {
    pub(crate) fn new() -> Self {
        Self { count: 0 }
    }

    /// Consume one work unit; hard-fail on a non-terminating program.
    fn tick(&mut self) -> Result<(), String> {
        self.count += 1;
        if self.count > MAX_TRANSACTION_STEPS {
            return Err(format!(
                "transaction exceeds step bound {MAX_TRANSACTION_STEPS} \
                 (non-terminating program)"
            ));
        }
        Ok(())
    }
}

impl Default for StepCounter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Parser ──────────────────────────────────────────────────────────────────────

/// Require exactly one object IRI for `(node, logic:<prop>)`; a missing or doubled
/// structural link is a HARD ERROR (never first-wins, which would hide a malformed
/// program).
fn require_one(facts: &WorldFacts, node: &str, prop: &str) -> Result<String, String> {
    let objs = facts.objects(node, &logic(prop));
    match objs.len() {
        1 => Ok(objs[0].to_owned()),
        0 => Err(format!(
            "transaction-program node {node:?} has no logic:{prop}"
        )),
        n => Err(format!(
            "transaction-program node {node:?} has {n} logic:{prop} links (exactly one required)"
        )),
    }
}

/// Parse the transaction program rooted at `node` from the world's facts.
///
/// Dispatch is on the node's `rdf:type`: a recognized combinator class, else a primitive
/// (a node carrying `logic:instantiatesSchema`).  A node with no recognized program type,
/// more than one combinator type, or a malformed operand set is a HARD ERROR.
///
/// # Errors
///
/// See module-level no-optionality contract.
pub(crate) fn parse_program(
    facts: &WorldFacts,
    node: &str,
    depth: usize,
) -> Result<TransactionProgram, String> {
    if depth > MAX_PROGRAM_DEPTH {
        return Err(format!(
            "transaction-program operand graph exceeds depth {MAX_PROGRAM_DEPTH} \
             (malformed cyclic operands?) at {node:?}"
        ));
    }
    // The recognized combinator classes among this node's rdf:type.
    let types: Vec<String> = facts
        .objects(node, RDF_TYPE)
        .into_iter()
        .filter_map(|t| t.strip_prefix(LOGIC_NAMESPACE).map(str::to_owned))
        .filter(|local| COMBINATOR_CLASSES.contains(&local.as_str()))
        .collect();
    if types.len() > 1 {
        return Err(format!(
            "transaction-program node {node:?} carries more than one combinator type: {types:?} \
             (a program node has exactly one operator type)"
        ));
    }

    match types.first().map(String::as_str) {
        Some(SERIAL_CONJUNCTION) => {
            let left = require_one(facts, node, LEFT_OPERAND)?;
            let right = require_one(facts, node, RIGHT_OPERAND)?;
            Ok(TransactionProgram::Serial {
                node: node.to_owned(),
                left: Box::new(parse_program(facts, &left, depth + 1)?),
                right: Box::new(parse_program(facts, &right, depth + 1)?),
            })
        }
        Some(CHOICE) => {
            let guard = require_one(facts, node, GUARD_SITUATION)?;
            let left = require_one(facts, node, LEFT_OPERAND)?;
            let right = require_one(facts, node, RIGHT_OPERAND)?;
            Ok(TransactionProgram::Choice {
                node: node.to_owned(),
                guard,
                left: Box::new(parse_program(facts, &left, depth + 1)?),
                right: Box::new(parse_program(facts, &right, depth + 1)?),
            })
        }
        Some(FALLBACK) => {
            let left = require_one(facts, node, LEFT_OPERAND)?;
            let right = require_one(facts, node, RIGHT_OPERAND)?;
            Ok(TransactionProgram::Fallback {
                node: node.to_owned(),
                left: Box::new(parse_program(facts, &left, depth + 1)?),
                right: Box::new(parse_program(facts, &right, depth + 1)?),
            })
        }
        Some(ITERATION) => {
            let condition = require_one(facts, node, ITERATION_CONDITION)?;
            let body = require_one(facts, node, ITERATION_BODY)?;
            Ok(TransactionProgram::Iteration {
                node: node.to_owned(),
                condition,
                body: Box::new(parse_program(facts, &body, depth + 1)?),
            })
        }
        Some(CONCURRENT_COMPOSITION) => {
            let left = require_one(facts, node, LEFT_OPERAND)?;
            let right = require_one(facts, node, RIGHT_OPERAND)?;
            Ok(TransactionProgram::Concurrent {
                node: node.to_owned(),
                left: Box::new(parse_program(facts, &left, depth + 1)?),
                right: Box::new(parse_program(facts, &right, depth + 1)?),
            })
        }
        Some(other) => Err(format!(
            "transaction-program node {node:?} has unhandled combinator type {other:?}"
        )),
        None => {
            // A primitive leaf invokes an action schema.
            let schema = facts
                .object(node, &logic(INSTANTIATES_SCHEMA))
                .ok_or_else(|| {
                    format!(
                    "transaction-program node {node:?} is neither a recognized combinator nor a \
                     primitive (no logic:instantiatesSchema)"
                )
                })?;
            Ok(TransactionProgram::Primitive {
                node: node.to_owned(),
                schema: schema.to_owned(),
            })
        }
    }
}

// ── Pure executional-entailment planner ─────────────────────────────────────────

/// Mint the content-addressed IRI of a successor state, salted by the program ROOT so
/// two distinct roots that invoke a shared sub-program node from the same base state
/// never collide on state IRIs.
fn mint_state(root: &str, node: &str, base_state: &str) -> String {
    let digest = sha1_hex(&format!("{root}\n{node}\n{base_state}"));
    format!("{LOGIC_NAMESPACE}txstate/{digest}")
}

/// Mint the content-addressed IRI of a RUNTIME `logic:TransactionStep`, salted by the
/// program ROOT, the primitive node, and the state the step starts FROM.  The `from`
/// salt makes every pass of a `logic:Iteration` over the same primitive a DISTINCT step
/// (each pass starts from a different state), so the supersession quartet it attributes
/// (`logic:retiredByTransaction` / `logic:supersededBy`) never collapses two runtime
/// passes onto one node.  The `step` discriminator keeps step IRIs disjoint from the
/// `txstate` IRIs minted over the same salt.
fn mint_step(root: &str, node: &str, from_state: &str) -> String {
    let digest = sha1_hex(&format!("{root}\n{node}\n{from_state}\nstep"));
    format!("{LOGIC_NAMESPACE}step/{digest}")
}

/// Whether the schema's preconditions all hold in the current support set.
fn preconditions_hold(facts: &WorldFacts, schema: &str, sits: &BTreeSet<String>) -> bool {
    facts
        .objects(schema, &logic(PRECONDITION))
        .into_iter()
        .all(|p| sits.contains(p))
}

/// Execute `program` from `state` (with support `sits`) under executional entailment.
///
/// Returns an [`ExecOutcome`]; an empty `path` means the program failed from here. Pure:
/// no store is mutated; the successor support is threaded via [`apply_effect_over`].
///
/// `root` salts minted state IRIs; `counter` bounds total work for termination.
///
/// # Errors
///
/// Returns `Err` only for a STRUCTURAL fault (a primitive schema with no effect, or a
/// non-terminating program beyond [`MAX_TRANSACTION_STEPS`]). A precondition that does
/// not hold is a normal failure (empty path), not an error.
pub(crate) fn plan_path(
    facts: &WorldFacts,
    program: &TransactionProgram,
    state: &str,
    sits: &BTreeSet<String>,
    root: &str,
    counter: &mut StepCounter,
) -> Result<ExecOutcome, String> {
    match program {
        TransactionProgram::Primitive { node, schema } => {
            // Executability gate: every precondition must hold in the current support.
            if !preconditions_hold(facts, schema, sits) {
                return Ok(ExecOutcome::failure());
            }
            counter.tick()?;
            // Effect as ins/del supersession over the THREADED support (a malformed
            // schema with no effect node is a hard error).
            let support = apply_effect_over(facts, schema, sits)?;
            let succ = mint_state(root, node, state);
            let sits_end = support.asserted.clone();
            Ok(ExecOutcome {
                path: vec![state.to_owned(), succ.clone()],
                steps: vec![PlannedStep {
                    from_state: state.to_owned(),
                    to_state: succ,
                    attribution: mint_step(root, node, state),
                    schema: schema.clone(),
                    support,
                }],
                sits_end,
            })
        }
        TransactionProgram::Serial { left, right, .. } => {
            // φ over a prefix, then ψ over the suffix — the split point is φ's end state.
            let l = plan_path(facts, left, state, sits, root, counter)?;
            if !l.succeeded() {
                return Ok(ExecOutcome::failure());
            }
            let mid = l.end_state().expect("non-empty success path").clone();
            let r = plan_path(facts, right, &mid, &l.sits_end, root, counter)?;
            if !r.succeeded() {
                return Ok(ExecOutcome::failure());
            }
            let mut path = l.path;
            path.extend_from_slice(&r.path[1..]); // drop the duplicated mid state
            let mut steps = l.steps;
            steps.extend(r.steps);
            Ok(ExecOutcome {
                path,
                steps,
                sits_end: r.sits_end,
            })
        }
        TransactionProgram::Choice {
            guard, left, right, ..
        } => {
            // Dispatch on the guard at the CURRENT support; the chosen branch's outcome is
            // returned as-is (a guard-false dispatch that itself fails makes Choice fail).
            let branch = if sits.contains(guard) { left } else { right };
            plan_path(facts, branch, state, sits, root, counter)
        }
        TransactionProgram::Fallback { left, right, .. } => {
            // Try the primary; on failure NOTHING was emitted (pure) — execute the alternate.
            let l = plan_path(facts, left, state, sits, root, counter)?;
            if l.succeeded() {
                return Ok(l);
            }
            plan_path(facts, right, state, sits, root, counter)
        }
        TransactionProgram::Iteration {
            condition, body, ..
        } => {
            let mut cur = state.to_owned();
            let mut cur_sits = sits.clone();
            let mut path = vec![state.to_owned()];
            let mut steps: Vec<PlannedStep> = Vec::new();
            while cur_sits.contains(condition) {
                // Tick once per pass so a no-progress loop hits the step bound rather than
                // spinning forever (a body of zero primitives would not otherwise tick).
                counter.tick()?;
                let b = plan_path(facts, body, &cur, &cur_sits, root, counter)?;
                if !b.succeeded() {
                    // A body that cannot execute mid-loop is a hard stop, not a silent break.
                    return Ok(ExecOutcome::failure());
                }
                path.extend_from_slice(&b.path[1..]);
                steps.extend(b.steps);
                cur = b.path.last().expect("non-empty success path").clone();
                cur_sits = b.sits_end;
            }
            // Condition false (possibly at entry → zero passes, path == [state]) — success.
            Ok(ExecOutcome {
                path,
                steps,
                sits_end: cur_sits,
            })
        }
        TransactionProgram::Concurrent { left, right, .. } => {
            // Both legs advance over the shared start; the verdict is "both legs find a
            // path from here". The conflict graph + serialization classification are
            // DERIVED at materialization ([`emit_concurrent_history`]), so `plan_path`
            // stays a pure path computation and never produces a single merged linear
            // chain that would silently linearize the schedule (forbidden by the design).
            let l = plan_path(facts, left, state, sits, root, counter)?;
            if !l.succeeded() {
                return Ok(ExecOutcome::failure());
            }
            let r = plan_path(facts, right, state, sits, root, counter)?;
            if !r.succeeded() {
                return Ok(ExecOutcome::failure());
            }
            // A merged outcome for the verdict and for any ENCLOSING combinator only: the
            // union of both legs' end supports and an index-interleaved step list. It is
            // NOT presented as an authoritative serial path — the load-bearing concurrency
            // result is the derived `logic:ConcurrentHistory`, not this merged support.
            let mut path = vec![state.to_owned()];
            path.extend(l.path.into_iter().skip(1));
            path.extend(r.path.into_iter().skip(1));
            let steps = interleave_steps(l.steps, r.steps);
            let mut sits_end = l.sits_end;
            sits_end.extend(r.sits_end);
            Ok(ExecOutcome {
                path,
                steps,
                sits_end,
            })
        }
    }
}

/// Index-order interleaving of two legs' elementary steps (left wins ties at equal index):
/// `left[0], right[0], left[1], right[1], …`. This is the single deterministic schedule the
/// conflict-edge derivation reads its operation order from.
fn interleave_steps(left: Vec<PlannedStep>, right: Vec<PlannedStep>) -> Vec<PlannedStep> {
    let mut out = Vec::with_capacity(left.len() + right.len());
    let n = left.len().max(right.len());
    let mut left_iter = left.into_iter();
    let mut right_iter = right.into_iter();
    for _ in 0..n {
        if let Some(s) = left_iter.next() {
            out.push(s);
        }
        if let Some(s) = right_iter.next() {
            out.push(s);
        }
    }
    out
}

// ── Root discovery ──────────────────────────────────────────────────────────────

/// The EXECUTABLE transaction-program ROOTS of a world: combinator-typed nodes that
///
/// 1. are not used as an operand/body of another program, AND
/// 2. carry a `logic:transitionFromState` — the explicit marker of an executable program
///    (the state it starts from).
///
/// Requirement (2) is what makes the engine evaluate exactly the programs an author
/// declared runnable: a combinator node without a start is structural data (a
/// sub-program fragment or a bare vocabulary declaration), not a transaction to execute.
/// (`ConcurrentComposition` roots marked executable are included so the engine executes
/// their interleaving and derives the concurrent history.)  Sorted, deduped.
pub(crate) fn program_roots(facts: &WorldFacts) -> Vec<String> {
    let mut typed: BTreeSet<String> = BTreeSet::new();
    for class in COMBINATOR_CLASSES {
        let class_iri = logic(class);
        for t in facts.subjects_with_type(&class_iri) {
            typed.insert(t);
        }
    }
    let mut used: BTreeSet<String> = BTreeSet::new();
    for prop in [LEFT_OPERAND, RIGHT_OPERAND, ITERATION_BODY] {
        for (_subj, obj) in facts.subject_objects(&logic(prop)) {
            used.insert(obj);
        }
    }
    typed
        .difference(&used)
        .filter(|&n| facts.object(n, &logic(TRANSITION_FROM_STATE)).is_some())
        .cloned()
        .collect()
}

/// The start state of a program root (`logic:transitionFromState`) and the situation
/// support obtaining there, read from the input facts.
///
/// # Errors
///
/// Hard-fails if the root has no (or more than one) `logic:transitionFromState`.
pub(crate) fn root_start(
    facts: &WorldFacts,
    root: &str,
) -> Result<(String, BTreeSet<String>), String> {
    let start = require_one(facts, root, TRANSITION_FROM_STATE)?;
    let sits: BTreeSet<String> = facts
        .objects(&start, &logic(SITUATION_OBTAINS))
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();
    Ok((start, sits))
}

// ── Execution-commitment facet: the hypothetical/sandbox operator ─────────────────

/// The execution-commitment mode a transaction run is evaluated under — the value of the
/// `logic:ExecutionMode` facet, reached from a program root through its governing
/// `logic:ReasoningContract` (`logic:executedUnderContract` → `logic:executionMode`).
///
/// Orthogonal to the program structure: [`plan_path`] computes the **verdict** identically
/// under both modes — the path-existence question is the same whether or not the effects are
/// kept. Only emission differs (see [`emit_transaction_outcome`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionMode {
    /// `logic:CommittedExecution` — the effect substrate (path, steps, supersession) is
    /// materialized; the default.
    Committed,
    /// `logic:HypotheticalExecution` — the sandbox operator: the verdict is recorded and a
    /// content-addressed witness is emitted, but the effect substrate is discarded.
    Hypothetical,
}

/// The execution mode a program `root` runs under.
///
/// Resolves `root --logic:executedUnderContract--> contract --logic:executionMode--> value`.
/// Absence of either link resolves to the constitutionally-named default
/// [`ExecutionMode::Committed`] (`logic:CommittedExecution` is documented as the default), so an
/// unannotated program commits exactly as before — this is a named default, NOT optionality.
///
/// # Errors
///
/// A MALFORMED declaration is a HARD ERROR: a root naming more than one governing contract, a
/// non-IRI (literal) `logic:executedUnderContract` or `logic:executionMode` value, a contract
/// carrying more than one `logic:executionMode` value, or an `executionMode` value that is
/// neither `logic:CommittedExecution` nor `logic:HypotheticalExecution`.
pub(crate) fn root_execution_mode(facts: &WorldFacts, root: &str) -> Result<ExecutionMode, String> {
    let contracts = facts.objects(root, &logic(EXECUTED_UNDER_CONTRACT));
    let contract = match contracts.len() {
        // `WorldFacts::objects` returns only IRI-valued objects, so a present-but-non-IRI link
        // is invisible here. That is malformed sandbox input — NOT an absent contract — and must
        // hard-fail rather than silently default to committed (which would commit a run authored
        // to be discarded).
        0 => {
            if facts
                .object_n3(root, &logic(EXECUTED_UNDER_CONTRACT))
                .is_some()
            {
                return Err(format!(
                    "transaction-program node {root:?} names a non-IRI logic:executedUnderContract value (a governing contract must be an IRI)"
                ));
            }
            return Ok(ExecutionMode::Committed);
        }
        1 => contracts[0],
        n => {
            return Err(format!(
                "transaction-program node {root:?} has {n} logic:executedUnderContract links (at most one governing contract allowed)"
            ))
        }
    };
    let modes = facts.objects(contract, &logic(EXECUTION_MODE));
    match modes.len() {
        // Same fail-closed discipline for the mode value: a non-IRI literal is malformed, not absent.
        0 => {
            if let Some(value) = facts.object_n3(contract, &logic(EXECUTION_MODE)) {
                return Err(format!(
                    "logic:ReasoningContract {contract:?} names a non-IRI logic:executionMode value {value:?} (expected logic:CommittedExecution or logic:HypotheticalExecution)"
                ));
            }
            Ok(ExecutionMode::Committed)
        }
        1 => {
            let value = modes[0];
            if value == logic(COMMITTED_EXECUTION) {
                Ok(ExecutionMode::Committed)
            } else if value == logic(HYPOTHETICAL_EXECUTION) {
                Ok(ExecutionMode::Hypothetical)
            } else {
                Err(format!(
                    "logic:ReasoningContract {contract:?} names an unknown logic:executionMode {value:?} (expected logic:CommittedExecution or logic:HypotheticalExecution)"
                ))
            }
        }
        n => Err(format!(
            "logic:ReasoningContract {contract:?} has {n} logic:executionMode values (exactly one required)"
        )),
    }
}

/// BLAKE3 digest (32 raw bytes) of a string, for content-addressing a hypothetical run.
fn blake3_32(s: &str) -> [u8; 32] {
    *blake3::hash(s.as_bytes()).as_bytes()
}

/// A canonical, layout-independent serialization of a transaction program — the stable
/// content-address input for the hypothetical-run key.
///
/// Unlike `format!("{program:?}")`, this encoding is a committed contract: each combinator is
/// named and every operand is length-prefixed, so a refactor of the IR's field order or its
/// `Debug` impl cannot silently renumber persisted witnesses. This mirrors the canonical-
/// serialization discipline the counterfactual keys use (never `Debug`).
fn canonical_program(program: &TransactionProgram) -> String {
    /// Length-prefix a leaf string so no concatenation boundary can collide.
    fn field(out: &mut String, s: &str) {
        out.push_str(&s.len().to_string());
        out.push(':');
        out.push_str(s);
        out.push(';');
    }
    fn encode(out: &mut String, program: &TransactionProgram) {
        match program {
            TransactionProgram::Primitive { node, schema } => {
                out.push_str("(primitive ");
                field(out, node);
                field(out, schema);
                out.push(')');
            }
            TransactionProgram::Serial { node, left, right } => {
                out.push_str("(serial ");
                field(out, node);
                encode(out, left);
                encode(out, right);
                out.push(')');
            }
            TransactionProgram::Choice {
                node,
                guard,
                left,
                right,
            } => {
                out.push_str("(choice ");
                field(out, node);
                field(out, guard);
                encode(out, left);
                encode(out, right);
                out.push(')');
            }
            TransactionProgram::Fallback { node, left, right } => {
                out.push_str("(fallback ");
                field(out, node);
                encode(out, left);
                encode(out, right);
                out.push(')');
            }
            TransactionProgram::Iteration {
                node,
                condition,
                body,
            } => {
                out.push_str("(iteration ");
                field(out, node);
                field(out, condition);
                encode(out, body);
                out.push(')');
            }
            TransactionProgram::Concurrent { node, left, right } => {
                out.push_str("(concurrent ");
                field(out, node);
                encode(out, left);
                encode(out, right);
                out.push(')');
            }
        }
    }
    let mut out = String::new();
    encode(&mut out, program);
    out
}

// ── Materialization: the executional-entailment outcome surface ──────────────────

// Outcome-surface vocabulary local names (declared in slices/core/logic/module.ttl).
const TRANSACTION_OUTCOME: &str = "TransactionOutcome";
const TRANSACTION_STEP: &str = "TransactionStep";
const OUTCOME_OF_PROGRAM: &str = "outcomeOfProgram";
const TRANSACTION_START: &str = "transactionStart";
const TRANSACTION_SUCCEEDS: &str = "transactionSucceeds";
const TEMPORALLY_SUCCEEDS: &str = "temporallySucceeds";
const EXECUTED_ALONG_PATH: &str = "executedAlongPath";
const PATH_CLASS: &str = "Path";
const EXECUTED_HYPOTHETICALLY_AS: &str = "executedHypotheticallyAs";
// Concurrent-history surface local names (the derived conflict graph + its links).
const CONCURRENT_HISTORY: &str = "ConcurrentHistory";
const PRECEDES: &str = "precedes";
const SERIALIZABILITY_CRITERION_PROP: &str = "serializabilityCriterion";
const DERIVED_HISTORY: &str = "derivedHistory";
const CONCURRENT_COMPOSED_FROM: &str = "concurrentComposedFrom";
/// The default history criterion when a `logic:ConcurrentHistory` declares none — the
/// engine decides conflict-serializability (mirrors `teleology::DEFAULT_SERIALIZABILITY_CRITERION`).
const CONFLICT_SERIALIZABILITY: &str = "ConflictSerializability";

/// The `xsd:boolean` N3 literal form.
fn xsd_bool(v: bool) -> String {
    format!("\"{v}\"^^<http://www.w3.org/2001/XMLSchema#boolean>")
}

/// Execute every transaction-program root of `world` and materialize its
/// executional-entailment outcome as derivation-graph quads.
///
/// For each root: parse, execute from the declared start state, then emit a
/// `logic:TransactionOutcome` carrying `logic:outcomeOfProgram`, `logic:transactionStart`,
/// and `logic:transactionSucceeds` (`true` iff the executed path is non-empty) — emitted in
/// BOTH execution modes.  Under `logic:CommittedExecution` (the default) a successful run also
/// emits the executed path (`logic:temporallySucceeds` edges + a minted `logic:Path` linked by
/// the reused `logic:executedAlongPath`) and, per elementary step, the situation-level
/// supersession substrate via [`crate::teleology::effect_quads`].  On failure ONLY the outcome
/// node is emitted — the start state is untouched.
///
/// Under `logic:HypotheticalExecution` (the sandbox operator, [`root_execution_mode`]) that
/// effect substrate is the discarded effect: it is SUPPRESSED, and only a content-addressed
/// witness (`logic:executedHypotheticallyAs`) is emitted alongside the verdict.  Hypothetical
/// success is, at the emission layer, isomorphic to committed failure — verdict present,
/// substrate absent, start state untouched — so suppression-never-erasure (Principle 10) holds
/// for the same reason it already holds for failure: nothing is asserted, so nothing is erased.
///
/// Pure over `facts` (no store mutation).  Every quad is stamped with
/// [`TRANSACTION_RULE_IRI`]; provenance reuses the shared content-addressed minters.
///
/// # Errors
///
/// Propagates any STRUCTURAL fault from [`parse_program`] / [`plan_path`] / [`root_start`] /
/// [`root_execution_mode`] (a malformed program, a primitive schema with no effect, a
/// non-terminating program, or a malformed execution-mode declaration) as a hard error.
///
/// A `logic:ConcurrentComposition` root additionally emits a DERIVED `logic:ConcurrentHistory`
/// (its conflict edges + serializability classification) — see [`emit_concurrent_history`].
pub(crate) fn emit_transaction_outcome(
    facts: &WorldFacts,
    world: &str,
    root: &str,
) -> Result<Vec<crate::teleology::TeleologyQuad>, String> {
    let (start, sits) = root_start(facts, root)?;
    let program = parse_program(facts, root, 0)?;
    // The execution-commitment mode is read from the program's governing contract; the
    // verdict (plan_path) is computed identically under both modes — only emission differs.
    let mode = root_execution_mode(facts, root)?;
    emit_program_outcome(facts, world, root, &program, mode, &start, &sits)
}

/// Emit the outcome substrate for an ALREADY-PARSED program from a resolved start state —
/// the single committed / hypothetical / concurrent emission path.
///
/// Factored out of [`emit_transaction_outcome`] so a SYNTHESIZED program (a recorded
/// `gmeow:ToolCall` trajectory mapped to a serial conjunction by
/// [`trajectory::emit_trajectory_audits`]) runs through the SAME emission as an authored
/// program — one path, no duplicated branches. `root` is the program-identity IRI that salts
/// every minted state/step and the content-addressed outcome node.
///
/// Pure over `facts` (no store mutation); every quad is stamped with [`TRANSACTION_RULE_IRI`].
///
/// # Errors
///
/// Propagates any STRUCTURAL fault from [`plan_path`] (a primitive schema with no effect, or a
/// non-terminating program) as a hard error.
pub(crate) fn emit_program_outcome(
    facts: &WorldFacts,
    world: &str,
    root: &str,
    program: &TransactionProgram,
    mode: ExecutionMode,
    start: &str,
    sits: &BTreeSet<String>,
) -> Result<Vec<crate::teleology::TeleologyQuad>, String> {
    use crate::provenance::mint_derivation_id;
    use crate::teleology::{n3, triple_reifier, TeleologyQuad};

    let mut counter = StepCounter::new();
    let outcome = plan_path(facts, program, start, sits, root, &mut counter)?;

    // Content-addressed outcome node, salted by (root, start, world).
    let outcome_iri = format!(
        "{LOGIC_NAMESPACE}outcome/{}",
        sha1_hex(&format!("{root}\n{start}\n{world}"))
    );
    // Grounding provenance: the program's type assertion is the link the outcome rests on.
    let source = triple_reifier(root, RDF_TYPE, &program_type_iri(program))?;
    let deriv = mint_derivation_id(TRANSACTION_RULE_IRI, &[source.as_str()]);

    let mut out: Vec<TeleologyQuad> = Vec::new();
    let mut emit = |subject: &str, predicate: String, object: String| {
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: subject.to_owned(),
            predicate,
            object,
            rule_iri: TRANSACTION_RULE_IRI.to_owned(),
            source_quad_ids: vec![source.clone()],
            derivation_id: deriv.clone(),
        });
    };

    // The outcome node + its verdict (always emitted).
    emit(
        &outcome_iri,
        RDF_TYPE.to_owned(),
        n3(&logic(TRANSACTION_OUTCOME)),
    );
    emit(&outcome_iri, logic(OUTCOME_OF_PROGRAM), n3(root));
    emit(&outcome_iri, logic(TRANSACTION_START), n3(start));
    emit(
        &outcome_iri,
        logic(TRANSACTION_SUCCEEDS),
        xsd_bool(outcome.succeeded()),
    );

    // Hypothetical (sandbox) run: emit the content-addressed witness — the sole standing trace
    // of a run whose effect substrate is deliberately never emitted (suppression-never-erasure
    // for free). The verdict above is observable in BOTH modes; the substrate below is the
    // committed effect that the sandbox operator discards.
    if mode == ExecutionMode::Hypothetical {
        // `sits` is a BTreeSet, so iteration is already lexically ordered — the hash input is
        // deterministic without an explicit sort; borrow the elements rather than cloning them.
        let key =
            crate::versioning::hypothetical_run_key(&crate::versioning::HypotheticalRunKeyInputs {
                start_state_hash: blake3_32(
                    &sits
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                program_hash: blake3_32(&canonical_program(program)),
                world: world.to_owned(),
                solver_version: crate::counterfactual::SOLVER_VERSION.to_owned(),
            });
        emit(
            &outcome_iri,
            logic(EXECUTED_HYPOTHETICALLY_AS),
            format!("\"{key}\""),
        );
    }

    if outcome.succeeded() && mode == ExecutionMode::Committed {
        // `emit` (the verdict closure) is no longer used past here, so its borrow of `out`
        // has ended (NLL) and the helpers below may extend `out`.
        if let TransactionProgram::Concurrent { left, right, .. } = program {
            // A concurrent root materializes EACH leg's path faithfully (no merged linear
            // chain) plus the DERIVED concurrent history + serialization classification.
            out.extend(emit_concurrent_history(
                facts,
                world,
                &outcome_iri,
                root,
                start,
                sits,
                left,
                right,
                &source,
                &deriv,
            )?);
        } else {
            // The executed path: temporallySucceeds edges + a minted logic:Path linked by the
            // reused logic:executedAlongPath, plus the per-step supersession substrate.
            let path_iri = format!(
                "{LOGIC_NAMESPACE}path/{}",
                sha1_hex(&format!("{root}\n{start}\n{world}\npath"))
            );
            out.extend(emit_committed_run(
                facts,
                world,
                &outcome_iri,
                &path_iri,
                &outcome.path,
                &outcome.steps,
                &source,
                &deriv,
            )?);
        }
    }
    Ok(out)
}

/// Emit the committed effect substrate of ONE executed run: a `logic:Path` (typed and
/// linked from `link_subject` by `logic:executedAlongPath`), the `logic:temporallySucceeds`
/// edges over `path` (oldest → newest), and per elementary `step` the first-class
/// `logic:TransactionStep` record + its situation-level supersession via
/// [`crate::teleology::effect_quads`].
///
/// Shared by the sequential outcome block and the concurrent per-leg emission so a
/// concurrent run materializes each leg's real path WITHOUT a merged cross-leg linear chain
/// (which would silently linearize the schedule). Path/edge quads carry the run's grounding
/// (`source`/`deriv`); each step's substrate is grounded PER STEP on the schema's own
/// `logic:effect` triple.
///
/// # Errors
///
/// Hard-fails if a step's action schema names no `logic:effect` node.
#[allow(clippy::too_many_arguments)]
fn emit_committed_run(
    facts: &WorldFacts,
    world: &str,
    link_subject: &str,
    path_iri: &str,
    path: &[String],
    steps: &[PlannedStep],
    source: &str,
    deriv: &str,
) -> Result<Vec<crate::teleology::TeleologyQuad>, String> {
    use crate::provenance::mint_derivation_id;
    use crate::teleology::{effect_quads, n3, triple_reifier, TeleologyQuad};

    let mut out: Vec<TeleologyQuad> = Vec::new();
    let mk = |subject: &str, predicate: String, object: String| TeleologyQuad {
        graph: world.to_owned(),
        subject: subject.to_owned(),
        predicate,
        object,
        rule_iri: TRANSACTION_RULE_IRI.to_owned(),
        source_quad_ids: vec![source.to_owned()],
        derivation_id: deriv.to_owned(),
    };
    out.push(mk(path_iri, RDF_TYPE.to_owned(), n3(&logic(PATH_CLASS))));
    out.push(mk(link_subject, logic(EXECUTED_ALONG_PATH), n3(path_iri)));
    for pair in path.windows(2) {
        // temporallySucceeds(successor, predecessor): "b succeeds a".
        out.push(mk(&pair[1], logic(TEMPORALLY_SUCCEEDS), n3(&pair[0])));
    }
    for step in steps {
        let effect = facts.object(&step.schema, &logic(EFFECT)).ok_or_else(|| {
            format!(
                "logic:ActionSchema {:?} names no logic:effect node",
                step.schema
            )
        })?;
        let step_source = triple_reifier(&step.schema, &logic(EFFECT), effect)?;
        let step_deriv = mint_derivation_id(TRANSACTION_RULE_IRI, &[step_source.as_str()]);
        for (predicate, object) in [
            (RDF_TYPE.to_owned(), n3(&logic(TRANSACTION_STEP))),
            (logic(INSTANTIATES_SCHEMA), n3(&step.schema)),
            (logic(TRANSITION_FROM_STATE), n3(&step.from_state)),
            (logic(TRANSITION_TO_STATE), n3(&step.to_state)),
        ] {
            out.push(TeleologyQuad {
                graph: world.to_owned(),
                subject: step.attribution.clone(),
                predicate,
                object,
                rule_iri: TRANSACTION_RULE_IRI.to_owned(),
                source_quad_ids: vec![step_source.clone()],
                derivation_id: step_deriv.clone(),
            });
        }
        out.extend(effect_quads(
            world,
            &step.support,
            &step.from_state,
            &step.to_state,
            &step.attribution,
            &step_source,
            &step_deriv,
            TRANSACTION_RULE_IRI,
        ));
    }
    Ok(out)
}

/// The (read, write) situation footprint of one elementary step: `read` is the schema's
/// `logic:precondition` situations (what the step depended on), `write` is the effect's
/// `logic:ins ∪ logic:del` situations (what the step changed). The write set is read from
/// the effect node — NOT from `step.support.retired`, which omits pure inserts and dels of
/// not-yet-present situations and would under-count write-write / read-write conflicts.
///
/// # Errors
///
/// Hard-fails if the step's action schema names no `logic:effect` node.
fn step_footprint(
    facts: &WorldFacts,
    step: &PlannedStep,
) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    let read: BTreeSet<String> = facts
        .objects(&step.schema, &logic(PRECONDITION))
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();
    let effect = facts.object(&step.schema, &logic(EFFECT)).ok_or_else(|| {
        format!(
            "logic:ActionSchema {:?} names no logic:effect node",
            step.schema
        )
    })?;
    let mut write: BTreeSet<String> = BTreeSet::new();
    for s in facts.objects(effect, &logic(INS)) {
        write.insert(s.to_owned());
    }
    for s in facts.objects(effect, &logic(DEL)) {
        write.insert(s.to_owned());
    }
    Ok((read, write))
}

/// Derive the conflict (precedence) edges between the two legs of a concurrent composition
/// from their executed step sequences, under the deterministic index-order interleaving
/// (`left[i]` at schedule position `2i`, `right[j]` at `2j+1` — left wins ties).
///
/// Two steps CONFLICT when their footprints share a situation that at least one of them
/// WRITES (read-write, write-read, or write-write). The edge points from the earlier to the
/// later transaction in that interleaving: `i <= j` orders the left step first (left
/// precedes right), `i > j` orders the right step first (right precedes left). Conflicts at
/// different situations may therefore point in OPPOSITE directions — the two-transaction
/// cycle (write-skew / lost-update) [`crate::teleology::detect_serialization_anomaly`]
/// classifies as a serialization anomaly. Result is sorted + deduped.
///
/// # Errors
///
/// Propagates a [`step_footprint`] hard-fail (a schema with no effect node).
fn derive_conflict_edges(
    facts: &WorldFacts,
    left_tx: &str,
    left: &[PlannedStep],
    right_tx: &str,
    right: &[PlannedStep],
) -> Result<Vec<crate::teleology::ConflictEdge>, String> {
    use crate::teleology::ConflictEdge;
    let left_fp: Vec<(BTreeSet<String>, BTreeSet<String>)> = left
        .iter()
        .map(|s| step_footprint(facts, s))
        .collect::<Result<_, _>>()?;
    let right_fp: Vec<(BTreeSet<String>, BTreeSet<String>)> = right
        .iter()
        .map(|s| step_footprint(facts, s))
        .collect::<Result<_, _>>()?;
    let mut edges: Vec<ConflictEdge> = Vec::new();
    for (i, (lr, lw)) in left_fp.iter().enumerate() {
        for (j, (rr, rw)) in right_fp.iter().enumerate() {
            // A conflict: a shared situation that at least one side writes.
            let conflict = lw.iter().any(|s| rr.contains(s) || rw.contains(s))
                || rw.iter().any(|s| lr.contains(s));
            if !conflict {
                continue;
            }
            if i <= j {
                edges.push(ConflictEdge {
                    from: left_tx.to_owned(),
                    to: right_tx.to_owned(),
                });
            } else {
                edges.push(ConflictEdge {
                    from: right_tx.to_owned(),
                    to: left_tx.to_owned(),
                });
            }
        }
    }
    edges.sort();
    edges.dedup();
    Ok(edges)
}

/// Emit the DERIVED `logic:ConcurrentHistory` of a succeeded concurrent composition: each
/// leg's committed path substrate (via [`emit_committed_run`], so neither leg is collapsed
/// into a bogus merged chain), the history node + its `logic:derivedHistory` link from the
/// outcome, the `logic:concurrentComposedFrom` audit edges to the two operands, the derived
/// `logic:precedes` conflict edges + the `logic:serializabilityCriterion`, and — when the
/// conflict graph cycles — the `logic:SerializationAnomaly` finding via the reused
/// [`crate::teleology::emit_serialization_anomaly`].
///
/// Re-executes each leg with a fresh [`StepCounter`] to capture the per-leg outcomes; the
/// caller's `outcome.succeeded()` already established both legs succeed. A serializable
/// (acyclic) history emits no anomaly — there is nothing to record.
///
/// # Errors
///
/// Propagates a structural fault from [`plan_path`] / [`emit_committed_run`] /
/// [`derive_conflict_edges`] / [`crate::teleology::emit_serialization_anomaly`].
#[allow(clippy::too_many_arguments)]
fn emit_concurrent_history(
    facts: &WorldFacts,
    world: &str,
    outcome_iri: &str,
    root: &str,
    start: &str,
    sits: &BTreeSet<String>,
    left: &TransactionProgram,
    right: &TransactionProgram,
    source: &str,
    deriv: &str,
) -> Result<Vec<crate::teleology::TeleologyQuad>, String> {
    use crate::teleology::{
        detect_serialization_anomaly, emit_serialization_anomaly, mint_anomaly_finding_iri, n3,
        SerializationVerdict, TeleologyQuad,
    };

    // Re-run each leg independently from the shared start (fresh counter — the verdict run
    // already proved termination); capture per-leg outcomes for substrate + footprints.
    let mut counter = StepCounter::new();
    let l = plan_path(facts, left, start, sits, root, &mut counter)?;
    let r = plan_path(facts, right, start, sits, root, &mut counter)?;
    if !l.succeeded() || !r.succeeded() {
        // Defensive: the caller only invokes this on a succeeded outcome.
        return Ok(Vec::new());
    }

    let left_tx = left.node();
    let right_tx = right.node();

    let mut out: Vec<TeleologyQuad> = Vec::new();

    // Each leg's path materializes faithfully (its own start→… chain), linked from the
    // outcome by logic:executedAlongPath — two paths, never one linearized merge.
    let left_path_iri = format!(
        "{LOGIC_NAMESPACE}path/{}",
        sha1_hex(&format!("{root}\n{start}\n{world}\n{left_tx}\npath"))
    );
    out.extend(emit_committed_run(
        facts,
        world,
        outcome_iri,
        &left_path_iri,
        &l.path,
        &l.steps,
        source,
        deriv,
    )?);
    let right_path_iri = format!(
        "{LOGIC_NAMESPACE}path/{}",
        sha1_hex(&format!("{root}\n{start}\n{world}\n{right_tx}\npath"))
    );
    out.extend(emit_committed_run(
        facts,
        world,
        outcome_iri,
        &right_path_iri,
        &r.path,
        &r.steps,
        source,
        deriv,
    )?);

    // The derived history node + its links, content-addressed by (root, start, world).
    let history_iri = format!(
        "{LOGIC_NAMESPACE}history/{}",
        sha1_hex(&format!("{root}\n{start}\n{world}"))
    );
    let criterion = logic(CONFLICT_SERIALIZABILITY);
    {
        let mut push = |subject: &str, predicate: String, object: String| {
            out.push(TeleologyQuad {
                graph: world.to_owned(),
                subject: subject.to_owned(),
                predicate,
                object,
                rule_iri: TRANSACTION_RULE_IRI.to_owned(),
                source_quad_ids: vec![source.to_owned()],
                derivation_id: deriv.to_owned(),
            });
        };
        push(
            &history_iri,
            RDF_TYPE.to_owned(),
            n3(&logic(CONCURRENT_HISTORY)),
        );
        push(outcome_iri, logic(DERIVED_HISTORY), n3(&history_iri));
        push(&history_iri, logic(CONCURRENT_COMPOSED_FROM), n3(left_tx));
        push(&history_iri, logic(CONCURRENT_COMPOSED_FROM), n3(right_tx));
        push(
            &history_iri,
            logic(SERIALIZABILITY_CRITERION_PROP),
            n3(&criterion),
        );
    }

    // The DERIVED conflict edges (the novel content: a conflict graph from execution, not an
    // authored one) — emitted so the anomaly finding's reifiers resolve in the explanation
    // index, and so a consumer can inspect the precedence structure.
    let edges = derive_conflict_edges(facts, left_tx, &l.steps, right_tx, &r.steps)?;
    {
        let mut push = |subject: &str, predicate: String, object: String| {
            out.push(TeleologyQuad {
                graph: world.to_owned(),
                subject: subject.to_owned(),
                predicate,
                object,
                rule_iri: TRANSACTION_RULE_IRI.to_owned(),
                source_quad_ids: vec![source.to_owned()],
                derivation_id: deriv.to_owned(),
            });
        };
        for e in &edges {
            push(&e.from, logic(PRECEDES), n3(&e.to));
        }
    }

    // Classify the derived history; a cycle is surfaced as a SerializationAnomaly finding
    // (reusing the shipped emitter), NEVER a contradiction witness and never linearized.
    if let SerializationVerdict::Anomaly(cycle) = detect_serialization_anomaly(&edges) {
        let finding_iri = mint_anomaly_finding_iri(&history_iri, &cycle);
        out.extend(emit_serialization_anomaly(
            world,
            &finding_iri,
            &cycle,
            &criterion,
            &edges,
        )?);
    }
    Ok(out)
}

/// The combinator/primitive class IRI a program node was parsed as — the grounding the
/// outcome's provenance reifies.
fn program_type_iri(program: &TransactionProgram) -> String {
    match program {
        TransactionProgram::Serial { .. } => logic(SERIAL_CONJUNCTION),
        TransactionProgram::Choice { .. } => logic(CHOICE),
        TransactionProgram::Fallback { .. } => logic(FALLBACK),
        TransactionProgram::Iteration { .. } => logic(ITERATION),
        TransactionProgram::Concurrent { .. } => logic(CONCURRENT_COMPOSITION),
        // A bare primitive is never a root (roots are combinator-typed), but be total.
        TransactionProgram::Primitive { .. } => logic(INSTANTIATES_SCHEMA),
    }
}

// T6 — read-only audit of recorded `gmeow:ToolCall` trajectories (issue #716).  A child
// module so it reaches the engine's private emission helpers through ONE shared path
// (`emit_program_outcome`); it mints no RDF vocabulary of its own.
pub(crate) mod trajectory;

#[cfg(test)]
mod tests;
