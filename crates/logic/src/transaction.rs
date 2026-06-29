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
//! - **`logic:ConcurrentComposition`** — a HARD ERROR here: concurrent
//!   serializability/isolation/protocol evaluation is a separate concern, not part of the
//!   sequential transaction-path core.
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
///
/// `logic:ConcurrentComposition` is deliberately NOT a variant — [`parse_program`]
/// hard-fails on it.
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
/// more than one combinator type, a `ConcurrentComposition` type, or a malformed operand
/// set is a HARD ERROR.
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
        Some(CONCURRENT_COMPOSITION) => Err(format!(
            "logic:ConcurrentComposition {node:?} is not evaluable by the transaction-program \
             engine; concurrent serializability/isolation/protocol evaluation is a separate concern"
        )),
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
    }
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
/// (`ConcurrentComposition` roots that ARE marked executable are included so the parser
/// hard-fails on them rather than silently skipping.)  Sorted, deduped.
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
/// contract carrying more than one `logic:executionMode` value, or an `executionMode` value that
/// is neither `logic:CommittedExecution` nor `logic:HypotheticalExecution`.
pub(crate) fn root_execution_mode(facts: &WorldFacts, root: &str) -> Result<ExecutionMode, String> {
    let contracts = facts.objects(root, &logic(EXECUTED_UNDER_CONTRACT));
    let contract = match contracts.len() {
        0 => return Ok(ExecutionMode::Committed),
        1 => contracts[0].to_owned(),
        n => {
            return Err(format!(
                "transaction-program node {root:?} has {n} logic:executedUnderContract links (at most one governing contract allowed)"
            ))
        }
    };
    let modes = facts.objects(&contract, &logic(EXECUTION_MODE));
    match modes.len() {
        0 => Ok(ExecutionMode::Committed),
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
/// `ConcurrentComposition` root, a non-terminating program, or a malformed execution-mode
/// declaration) as a hard error.
pub(crate) fn emit_transaction_outcome(
    facts: &WorldFacts,
    world: &str,
    root: &str,
) -> Result<Vec<crate::teleology::TeleologyQuad>, String> {
    use crate::provenance::mint_derivation_id;
    use crate::teleology::{effect_quads, n3, triple_reifier, TeleologyQuad};

    let (start, sits) = root_start(facts, root)?;
    let program = parse_program(facts, root, 0)?;
    // The execution-commitment mode is read from the program's governing contract; the
    // verdict (plan_path) is computed identically under both modes — only emission differs.
    let mode = root_execution_mode(facts, root)?;
    let mut counter = StepCounter::new();
    let outcome = plan_path(facts, &program, &start, &sits, root, &mut counter)?;

    // Content-addressed outcome node, salted by (root, start, world).
    let outcome_iri = format!(
        "{LOGIC_NAMESPACE}outcome/{}",
        sha1_hex(&format!("{root}\n{start}\n{world}"))
    );
    // Grounding provenance: the program's type assertion is the link the outcome rests on.
    let source = triple_reifier(root, RDF_TYPE, &program_type_iri(&program))?;
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
    emit(&outcome_iri, logic(TRANSACTION_START), n3(&start));
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
        let key =
            crate::versioning::hypothetical_run_key(&crate::versioning::HypotheticalRunKeyInputs {
                start_state_hash: blake3_32(&sits.iter().cloned().collect::<Vec<_>>().join("\n")),
                program_hash: blake3_32(&format!("{program:?}")),
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
        // The executed path: temporallySucceeds edges + a minted logic:Path linked by the
        // reused logic:executedAlongPath (oldest → newest; the start may be the only state).
        let path_iri = format!(
            "{LOGIC_NAMESPACE}path/{}",
            sha1_hex(&format!("{root}\n{start}\n{world}\npath"))
        );
        emit(&path_iri, RDF_TYPE.to_owned(), n3(&logic(PATH_CLASS)));
        emit(&outcome_iri, logic(EXECUTED_ALONG_PATH), n3(&path_iri));
        for pair in outcome.path.windows(2) {
            // temporallySucceeds(successor, predecessor): "b succeeds a".
            emit(&pair[1], logic(TEMPORALLY_SUCCEEDS), n3(&pair[0]));
        }
        // Per-step substrate. Each runtime step is materialized as a first-class
        // logic:TransactionStep (the declared contract): typed, instantiating its action
        // schema, carrying the path edge from→to. Its provenance is grounded PER STEP on
        // the schema's own `logic:effect` triple — NOT the root program-type triple — so
        // every step's situationObtains + supersession quartet is attributed to the effect
        // that actually produced it. This is byte-isomorphic to the authored-step family
        // (emit_effect_application), differing only in the rule IRI.
        for step in &outcome.steps {
            let effect = facts.object(&step.schema, &logic(EFFECT)).ok_or_else(|| {
                format!(
                    "logic:ActionSchema {:?} names no logic:effect node",
                    step.schema
                )
            })?;
            let step_source = triple_reifier(&step.schema, &logic(EFFECT), effect)?;
            let step_deriv = mint_derivation_id(TRANSACTION_RULE_IRI, &[step_source.as_str()]);
            // Scope the borrow of `out` so it ends before the `effect_quads` extend below.
            {
                let mut push_step = |predicate: String, object: String| {
                    out.push(TeleologyQuad {
                        graph: world.to_owned(),
                        subject: step.attribution.clone(),
                        predicate,
                        object,
                        rule_iri: TRANSACTION_RULE_IRI.to_owned(),
                        source_quad_ids: vec![step_source.clone()],
                        derivation_id: step_deriv.clone(),
                    });
                };
                push_step(RDF_TYPE.to_owned(), n3(&logic(TRANSACTION_STEP)));
                push_step(logic(INSTANTIATES_SCHEMA), n3(&step.schema));
                push_step(logic(TRANSITION_FROM_STATE), n3(&step.from_state));
                push_step(logic(TRANSITION_TO_STATE), n3(&step.to_state));
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
        // A bare primitive is never a root (roots are combinator-typed), but be total.
        TransactionProgram::Primitive { .. } => logic(INSTANTIATES_SCHEMA),
    }
}

#[cfg(test)]
mod tests;
