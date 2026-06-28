// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native Rust evaluator for the W1 canonical-process *teleology* layer (issue #1055).
//!
//! This module is the canonical native evaluator for the teleology / transaction
//! computations of the unified canonical process model: it EVALUATES and
//! CLASSIFIES given structure and RECORDS the verdict as `logic:GoalEvaluation`
//! quads and `logic:`-namespaced findings.  It mirrors [`crate::foundation`]'s
//! determinism contract verbatim so the conformance runner (Task 5) can wire it in
//! the same way it wires the foundation evaluator.
//!
//! # P12 boundary (constitutional)
//!
//! This evaluator does **no means–end search** — it never *finds* a plan.  Given a
//! goal expression + a path it decides the goal's status; given a plan + an outcome
//! set it classifies the success mode; given a concurrent history it detects
//! serialization anomalies; given an action schema + a proposed action + a state it
//! gates the action.  Every computation is a pure function over the given structure.
//!
//! # Determinism contract (golden-pinned, identical to [`crate::foundation`])
//!
//! 1. **Insertion-order enumeration** — every join / scan walks the world's quads in
//!    the deterministic, content-sorted order [`WorldFacts::read`] produces.
//! 2. **First-wins dedup** — a derived quad whose `(s, p, o, g)` key already exists is
//!    dropped, keeping the first record's provenance.
//! 3. **Provenance** — reifiers via [`mint_reifier`] and derivation IRIs via
//!    [`mint_derivation_id`], reused verbatim from [`crate::provenance`] (no new
//!    provenance scheme is invented).
//! 4. **Canonical row sort** — the emitted quads are sorted by `(graph, subject,
//!    predicate, object)` before return, matching the foundation runner's fold.
//!
//! Satisfaction status (`logic:Satisfied` / `logic:PartiallySatisfied` /
//! `logic:Violated` / `logic:Unsatisfied`) is kept strictly DISTINCT from any
//! confidence / uncertainty and from `logic:satisfactionDegree`: a degree is never
//! folded into a truth value and a confidence is never read as a degree.
//!
//! # No-optionality
//!
//! A malformed structure (a goal expression with no kind, an unknown kind IRI, a
//! conditional goal with no operand) is a HARD ERROR — there is no silent default and
//! no degraded fallback.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;

use oxigraph::model::{NamedNode, Term};

use crate::provenance::{mint_derivation_id, mint_reifier};
use crate::store::WorldStore;

// ── Namespace + vocabulary constants ───────────────────────────────────────────

/// The `logic:` vocabulary namespace.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The `rdf:type` predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Rule IRI stamped on every emitted teleology record (`logic:rule/teleology`).
const TELEOLOGY_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/rule/teleology";

/// Build a `logic:`-namespaced IRI string.
fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}

// ── Goal-expression kinds (closed nine-member value set) ────────────────────────

/// The closed nine-member value set of `logic:GoalExpressionKind`.
///
/// An unknown kind IRI is a HARD ERROR ([`GoalKind::from_iri`] returns `Err`) — the
/// set is closed, mirroring [`crate::foundation::AntiRigidityPolicy::from_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalKind {
    /// `logic:AtomicGoal` — a named situation type obtains at the relevant state.
    Atomic,
    /// `logic:ConjunctiveGoal` — all operand sub-expressions satisfied.
    Conjunctive,
    /// `logic:DisjunctiveGoal` — any operand sub-expression satisfied.
    Disjunctive,
    /// `logic:AchievementGoal` — the target first obtains along the path.
    Achievement,
    /// `logic:MaintenanceGoal` — the target holds at every state of the interval.
    Maintenance,
    /// `logic:AvoidanceGoal` — the proscribed target never obtains (dual of maintenance).
    Avoidance,
    /// `logic:OptimizationGoal` — directed at a measure; recorded as a degree.
    Optimization,
    /// `logic:ConditionalGoal` — the operand applies only when the guard holds.
    Conditional,
    /// `logic:DeadlineWindowGoal` — an achievement/maintenance target bounded by a window.
    DeadlineWindow,
}

impl GoalKind {
    /// Parse a `logic:GoalExpressionKind` IRI.  Unknown values are a hard error.
    ///
    /// # Errors
    ///
    /// Returns `Err` for any IRI outside the closed nine-member set.
    pub fn from_iri(iri: &str) -> Result<Self, String> {
        let local = iri.strip_prefix(LOGIC_NS).unwrap_or(iri);
        match local {
            "AtomicGoal" => Ok(Self::Atomic),
            "ConjunctiveGoal" => Ok(Self::Conjunctive),
            "DisjunctiveGoal" => Ok(Self::Disjunctive),
            "AchievementGoal" => Ok(Self::Achievement),
            "MaintenanceGoal" => Ok(Self::Maintenance),
            "AvoidanceGoal" => Ok(Self::Avoidance),
            "OptimizationGoal" => Ok(Self::Optimization),
            "ConditionalGoal" => Ok(Self::Conditional),
            "DeadlineWindowGoal" => Ok(Self::DeadlineWindow),
            other => Err(format!(
                "Unknown logic:GoalExpressionKind {other:?}; must be one of the nine \
                 closed variants (AtomicGoal, ConjunctiveGoal, DisjunctiveGoal, \
                 AchievementGoal, MaintenanceGoal, AvoidanceGoal, OptimizationGoal, \
                 ConditionalGoal, DeadlineWindowGoal)"
            )),
        }
    }
}

// ── Factored evaluation axes ────────────────────────────────────────────────────

/// The satisfaction axis (`logic:SatisfactionStatus`).  Kept strictly apart from any
/// confidence/uncertainty and from the degree quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Satisfaction {
    /// `logic:Satisfied` — fully met.
    Satisfied,
    /// `logic:PartiallySatisfied` — met in part (carries a degree).
    PartiallySatisfied,
    /// `logic:Violated` — a maintenance/avoidance target has failed.
    Violated,
    /// `logic:Unsatisfied` — not met, no positive partial progress.
    Unsatisfied,
    /// No satisfaction verdict is asserted (a conditional goal whose guard is false —
    /// "does not apply").  Carries no `logic:satisfactionStatus` quad.
    DoesNotApply,
}

impl Satisfaction {
    /// The `logic:SatisfactionStatus` individual local name, or `None` for
    /// [`Satisfaction::DoesNotApply`] (which asserts no status).
    fn local(self) -> Option<&'static str> {
        match self {
            Self::Satisfied => Some("Satisfied"),
            Self::PartiallySatisfied => Some("PartiallySatisfied"),
            Self::Violated => Some("Violated"),
            Self::Unsatisfied => Some("Unsatisfied"),
            Self::DoesNotApply => None,
        }
    }
}

/// The goal-evaluation conclusiveness axis (`logic:GoalEvaluationStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationStatus {
    /// `logic:GoalEvaluationCompleted` — a conclusive judgment.
    Completed,
    /// `logic:GoalEvaluationUndetermined` — not yet conclusive (open window / no ideal world).
    Undetermined,
    /// `logic:GoalEvaluationUnsupported` — no procedure for the judgment.
    Unsupported,
}

impl EvaluationStatus {
    fn local(self) -> &'static str {
        match self {
            Self::Completed => "GoalEvaluationCompleted",
            Self::Undetermined => "GoalEvaluationUndetermined",
            Self::Unsupported => "GoalEvaluationUnsupported",
        }
    }
}

/// A typed goal-evaluation verdict over the factored axes (P12: classification only).
///
/// Satisfaction and conclusiveness vary independently; `degree` is the
/// `logic:satisfactionDegree` quantity carried for an optimization or partial goal and
/// is NEVER a confidence value (the two are different questions and never substitute).
#[derive(Debug, Clone, PartialEq)]
pub struct GoalVerdict {
    /// The evaluated goal-expression IRI.
    pub goal_expression: String,
    /// The satisfaction-axis verdict.
    pub satisfaction: Satisfaction,
    /// The conclusiveness-axis verdict.
    pub evaluation_status: EvaluationStatus,
    /// The `logic:satisfactionDegree` quantity, when the verdict carries one
    /// (optimization or partial).  A unit-interval decimal lexical form.
    pub degree: Option<String>,
}

// ── Output quad type (mirrors [`crate::foundation::FoundationQuad`]) ─────────────

/// A materialized teleology quad with the full seam provenance contract.
///
/// Shape-identical to [`crate::foundation::FoundationQuad`] so the conformance runner
/// folds both evaluators the same way: `object` is canonical N3 (`<iri>` for an IRI,
/// `"lex"^^<dt>` for a typed literal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeleologyQuad {
    /// The world / named-graph IRI the record is recorded in.
    pub graph: String,
    /// The subject IRI (the evaluation, plan, or finding node).
    pub subject: String,
    /// The predicate IRI.
    pub predicate: String,
    /// The object term in canonical N3 form.
    pub object: String,
    /// The firing rule IRI (`logic:rule/teleology`).
    pub rule_iri: String,
    /// The reifier IRIs of the antecedent quads consumed by the firing.
    pub source_quad_ids: Vec<String>,
    /// The content-addressed derivation IRI.
    pub derivation_id: String,
}

/// N3 form of an IRI: `<iri>`.
fn n3(iri: &str) -> String {
    format!("<{iri}>")
}

/// Reifier IRI for an explicit `(s, p, o)` IRI triple, via the golden-pinned recipe.
fn triple_reifier(s: &str, p: &str, o: &str) -> Result<String, String> {
    let sn = Term::NamedNode(NamedNode::new(s).map_err(|e| format!("invalid subject IRI: {e}"))?);
    let pn = NamedNode::new(p).map_err(|e| format!("invalid predicate IRI: {e}"))?;
    let on = Term::NamedNode(NamedNode::new(o).map_err(|e| format!("invalid object IRI: {e}"))?);
    mint_reifier(&sn, &pn, &on)
}

// ── EDB: insertion-ordered, content-sorted fact view of one world ───────────────

/// A ground triple `(subject, predicate, object)` — `object` may be an IRI or a
/// literal in canonical N3 form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Triple {
    subject: String,
    predicate: String,
    /// `Some(iri)` for an IRI object; `None` when the object is a literal.
    object_iri: Option<String>,
    object_n3: String,
}

/// The content-sorted fact view of a single world.
///
/// Facts are sorted by `(subject, predicate, object_n3)` so all enumeration is
/// deterministic and independent of oxigraph's internal iteration order — the
/// teleology analogue of the foundation chase's `initial.sort_by_key(Fact::key)`.
pub struct WorldFacts {
    triples: Vec<Triple>,
    sp_index: HashMap<(String, String), Vec<usize>>,
}

impl WorldFacts {
    /// Read and content-sort the facts of one world from the store.
    pub fn read(store: &WorldStore, world: &str) -> Self {
        let raw = store.quads_in_world(world);
        let mut triples: Vec<Triple> = Vec::with_capacity(raw.len());
        for r in &raw {
            let subject = strip_angle(&r[0]).to_owned();
            let predicate = strip_angle(&r[1]).to_owned();
            let (object_iri, object_n3) = match strip_angle_opt(&r[2]) {
                Some(iri) => (Some(iri.to_owned()), n3(iri)),
                None => (None, r[2].clone()),
            };
            triples.push(Triple {
                subject,
                predicate,
                object_iri,
                object_n3,
            });
        }
        triples.sort();
        triples.dedup();
        let mut sp_index: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (i, t) in triples.iter().enumerate() {
            sp_index
                .entry((t.subject.clone(), t.predicate.clone()))
                .or_default()
                .push(i);
        }
        Self { triples, sp_index }
    }

    /// All object IRIs for `(subject, predicate)`, in sorted order; literals skipped.
    fn objects(&self, subject: &str, predicate: &str) -> Vec<&str> {
        self.sp_index
            .get(&(subject.to_owned(), predicate.to_owned()))
            .map(|idxs| {
                idxs.iter()
                    .filter_map(|&i| self.triples[i].object_iri.as_deref())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The first object IRI for `(subject, predicate)`, or `None`.
    fn object(&self, subject: &str, predicate: &str) -> Option<&str> {
        self.objects(subject, predicate).into_iter().next()
    }

    /// The first object's N3 form (IRI or literal) for `(subject, predicate)`.
    fn object_n3(&self, subject: &str, predicate: &str) -> Option<&str> {
        self.sp_index
            .get(&(subject.to_owned(), predicate.to_owned()))
            .and_then(|idxs| idxs.first().map(|&i| self.triples[i].object_n3.as_str()))
    }

    /// Whether `(subject, predicate, object)` (object an IRI) is present.
    fn has(&self, subject: &str, predicate: &str, object: &str) -> bool {
        self.objects(subject, predicate).contains(&object)
    }
}

/// Strip `<` … `>` if both delimiters are present.
fn strip_angle_opt(s: &str) -> Option<&str> {
    s.strip_prefix('<').and_then(|x| x.strip_suffix('>'))
}

/// Strip `<` … `>` if present; identity otherwise.
fn strip_angle(s: &str) -> &str {
    strip_angle_opt(s).unwrap_or(s)
}

// ── Path: an ordered run of states via logic:temporallySucceeds ─────────────────

/// Local name of `logic:temporallySucceeds`.
const TEMPORALLY_SUCCEEDS: &str = "temporallySucceeds";
/// Local name of `logic:situationObtains`.
const SITUATION_OBTAINS: &str = "situationObtains";

/// The ordered state sequence of a `logic:Path`, recovered from the
/// `logic:temporallySucceeds` strict order over its states.
///
/// `temporallySucceeds(b, a)` is read "b succeeds a", i.e. `a` precedes `b`; the
/// sequence is the topological chain from the unique minimum.  A cycle or a fork is a
/// hard error — a path is a linear run, not a branching tree.
///
/// # Errors
///
/// Returns `Err` for a non-linear path (a fork, a cycle, more than one start, or a
/// disconnected component).
pub fn ordered_states(facts: &WorldFacts) -> Result<Vec<String>, String> {
    let succ_iri = logic(TEMPORALLY_SUCCEEDS);
    let mut succ_of: BTreeMap<String, String> = BTreeMap::new();
    let mut pred_of: BTreeMap<String, String> = BTreeMap::new();
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    for t in &facts.triples {
        if t.predicate == succ_iri {
            if let Some(a) = t.object_iri.as_deref() {
                let b = t.subject.clone();
                if succ_of.insert(a.to_owned(), b.clone()).is_some() {
                    return Err(format!(
                        "logic:Path is not linear: state {a:?} has two successors"
                    ));
                }
                if pred_of.insert(b.clone(), a.to_owned()).is_some() {
                    return Err(format!(
                        "logic:Path is not linear: state {b:?} has two predecessors"
                    ));
                }
                nodes.insert(a.to_owned());
                nodes.insert(b);
            }
        }
    }
    // A path may consist of a SINGLE state with no temporallySucceeds edge: gather
    // any state that bears a logic:situationObtains fact as a candidate path position.
    let obtains_iri = logic(SITUATION_OBTAINS);
    for t in &facts.triples {
        if t.predicate == obtains_iri {
            nodes.insert(t.subject.clone());
        }
    }
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    // Degenerate single-state path: one state, no ordering edges.
    if succ_of.is_empty() && pred_of.is_empty() {
        if nodes.len() == 1 {
            return Ok(vec![nodes.into_iter().next().expect("len==1")]);
        }
        return Err(format!(
            "logic:Path has {} states but no logic:temporallySucceeds ordering edges              (cannot order a multi-state path)",
            nodes.len()
        ));
    }
    let starts: Vec<&String> = nodes.iter().filter(|n| !pred_of.contains_key(*n)).collect();
    if starts.len() != 1 {
        return Err(format!(
            "logic:Path must have exactly one start state (states with no predecessor); found {}",
            starts.len()
        ));
    }
    let mut seq = Vec::with_capacity(nodes.len());
    let mut cur = starts[0].clone();
    let mut seen: HashSet<String> = HashSet::new();
    loop {
        if !seen.insert(cur.clone()) {
            return Err("logic:Path contains a cycle".to_owned());
        }
        seq.push(cur.clone());
        match succ_of.get(&cur) {
            Some(next) => cur = next.clone(),
            None => break,
        }
    }
    if seq.len() != nodes.len() {
        return Err(
            "logic:Path is disconnected (not all states reachable from the start)".to_owned(),
        );
    }
    Ok(seq)
}

/// Whether a situation type obtains at a state (`logic:situationObtains`).
fn obtains_at(facts: &WorldFacts, state: &str, situation: &str) -> bool {
    facts.has(state, &logic(SITUATION_OBTAINS), situation)
}

// ── Goal-expression structure property local names ──────────────────────────────

const GOAL_EXPR_KIND: &str = "goalExpressionKind";
const OPERAND: &str = "operand";
const BOUND_SITUATION_TYPE: &str = "boundSituationType";
const GUARD_SITUATION: &str = "guardSituation";

/// The recursion-depth ceiling for compositional goal evaluation — a malformed cyclic
/// operand graph is bounded here rather than overflowing the stack.
const MAX_GOAL_DEPTH: usize = 256;

/// Evaluate a `logic:GoalExpression` over the ordered states of a `logic:Path`.
///
/// Pure classification over given structure (P12): reads the expression's kind,
/// operands, and bound/guard situation, decides the factored verdict, never searches.
///
/// # Errors
///
/// Returns `Err` for a malformed expression (no kind, unknown kind IRI, a
/// conditional/deadline goal with no operand, or an operand cycle beyond
/// [`MAX_GOAL_DEPTH`]).
pub fn evaluate_goal_over_path(
    facts: &WorldFacts,
    goal_expr: &str,
    states: &[String],
) -> Result<GoalVerdict, String> {
    let (sat, status, degree) = eval_goal(facts, goal_expr, states, 0)?;
    Ok(GoalVerdict {
        goal_expression: goal_expr.to_owned(),
        satisfaction: sat,
        evaluation_status: status,
        degree,
    })
}

/// Inner recursive evaluation returning the factored axes for one expression.
fn eval_goal(
    facts: &WorldFacts,
    goal_expr: &str,
    states: &[String],
    depth: usize,
) -> Result<(Satisfaction, EvaluationStatus, Option<String>), String> {
    if depth > MAX_GOAL_DEPTH {
        return Err(format!(
            "logic:GoalExpression operand graph exceeds depth {MAX_GOAL_DEPTH} \
             (malformed cyclic operands?) at {goal_expr:?}"
        ));
    }
    let kind_iri = facts
        .object(goal_expr, &logic(GOAL_EXPR_KIND))
        .ok_or_else(|| {
            format!("logic:GoalExpression {goal_expr:?} has no logic:goalExpressionKind")
        })?;
    let kind = GoalKind::from_iri(kind_iri)?;

    match kind {
        GoalKind::Atomic => eval_atomic(facts, goal_expr, states),
        GoalKind::Achievement => eval_achievement(facts, goal_expr, states),
        GoalKind::Maintenance => eval_maintenance(facts, goal_expr, states, false),
        GoalKind::Avoidance => eval_maintenance(facts, goal_expr, states, true),
        GoalKind::Conjunctive => eval_junction(facts, goal_expr, states, depth, true),
        GoalKind::Disjunctive => eval_junction(facts, goal_expr, states, depth, false),
        GoalKind::Conditional => eval_conditional(facts, goal_expr, states, depth),
        GoalKind::DeadlineWindow => eval_deadline_window(facts, goal_expr, states, depth),
        GoalKind::Optimization => eval_optimization(facts, goal_expr),
    }
}

/// Read the bound situation type of an atomic/achievement/maintenance/avoidance goal.
fn bound_situation(facts: &WorldFacts, goal_expr: &str) -> Result<String, String> {
    facts
        .object(goal_expr, &logic(BOUND_SITUATION_TYPE))
        .map(str::to_owned)
        .ok_or_else(|| format!("goal {goal_expr:?} has no logic:boundSituationType"))
}

/// Atomic: satisfied iff the bound situation obtains at the LAST (most recent) state.
fn eval_atomic(
    facts: &WorldFacts,
    goal_expr: &str,
    states: &[String],
) -> Result<(Satisfaction, EvaluationStatus, Option<String>), String> {
    let sit = bound_situation(facts, goal_expr)?;
    let met = states.last().is_some_and(|s| obtains_at(facts, s, &sit));
    Ok((
        if met {
            Satisfaction::Satisfied
        } else {
            Satisfaction::Unsatisfied
        },
        EvaluationStatus::Completed,
        None,
    ))
}

/// Achievement: satisfied once the target FIRST obtains anywhere along the path.
fn eval_achievement(
    facts: &WorldFacts,
    goal_expr: &str,
    states: &[String],
) -> Result<(Satisfaction, EvaluationStatus, Option<String>), String> {
    let sit = bound_situation(facts, goal_expr)?;
    let met = states.iter().any(|s| obtains_at(facts, s, &sit));
    Ok((
        if met {
            Satisfaction::Satisfied
        } else {
            Satisfaction::Unsatisfied
        },
        EvaluationStatus::Completed,
        None,
    ))
}

/// Maintenance / avoidance over the path interval.
///
/// Maintenance: the target must hold at EVERY state; violated at the FIRST failure;
/// while the window is still open holding-so-far is `Undetermined`, not conclusive.
/// Avoidance is the dual: the proscribed target must NEVER obtain.
fn eval_maintenance(
    facts: &WorldFacts,
    goal_expr: &str,
    states: &[String],
    avoidance: bool,
) -> Result<(Satisfaction, EvaluationStatus, Option<String>), String> {
    let sit = bound_situation(facts, goal_expr)?;
    for s in states {
        let obtains = obtains_at(facts, s, &sit);
        let failed = if avoidance { obtains } else { !obtains };
        if failed {
            return Ok((Satisfaction::Violated, EvaluationStatus::Completed, None));
        }
    }
    Ok((
        Satisfaction::Satisfied,
        EvaluationStatus::Undetermined,
        None,
    ))
}

/// Conjunctive / disjunctive junction over operands.
fn eval_junction(
    facts: &WorldFacts,
    goal_expr: &str,
    states: &[String],
    depth: usize,
    conjunctive: bool,
) -> Result<(Satisfaction, EvaluationStatus, Option<String>), String> {
    let operands_ref = facts.objects(goal_expr, &logic(OPERAND));
    if operands_ref.is_empty() {
        return Err(format!(
            "{} goal {goal_expr:?} has no logic:operand",
            if conjunctive {
                "conjunctive"
            } else {
                "disjunctive"
            }
        ));
    }
    let operands: Vec<String> = operands_ref.iter().map(|s| (*s).to_owned()).collect();
    let mut all_completed = true;
    let mut any_satisfied = false;
    let mut all_satisfied = true;
    let mut any_violated = false;
    for op in &operands {
        let (sat, st, _deg) = eval_goal(facts, op, states, depth + 1)?;
        if st != EvaluationStatus::Completed {
            all_completed = false;
        }
        match sat {
            Satisfaction::Satisfied => any_satisfied = true,
            Satisfaction::Violated => {
                any_violated = true;
                all_satisfied = false;
            }
            _ => all_satisfied = false,
        }
    }
    let satisfaction = if conjunctive {
        if all_satisfied {
            Satisfaction::Satisfied
        } else if any_violated {
            Satisfaction::Violated
        } else {
            Satisfaction::Unsatisfied
        }
    } else if any_satisfied {
        Satisfaction::Satisfied
    } else if any_violated {
        Satisfaction::Violated
    } else {
        Satisfaction::Unsatisfied
    };
    let status = if conjunctive {
        if all_completed {
            EvaluationStatus::Completed
        } else {
            EvaluationStatus::Undetermined
        }
    } else if any_satisfied || all_completed {
        EvaluationStatus::Completed
    } else {
        EvaluationStatus::Undetermined
    };
    Ok((satisfaction, status, None))
}

/// Conditional: if the guard situation does NOT hold (at the last state), the goal
/// prescribes nothing — `DoesNotApply`.
fn eval_conditional(
    facts: &WorldFacts,
    goal_expr: &str,
    states: &[String],
    depth: usize,
) -> Result<(Satisfaction, EvaluationStatus, Option<String>), String> {
    let guard = facts
        .object(goal_expr, &logic(GUARD_SITUATION))
        .ok_or_else(|| format!("conditional goal {goal_expr:?} has no logic:guardSituation"))?
        .to_owned();
    let operand = facts
        .object(goal_expr, &logic(OPERAND))
        .ok_or_else(|| format!("conditional goal {goal_expr:?} has no logic:operand (target)"))?
        .to_owned();
    let guard_holds = states.last().is_some_and(|s| obtains_at(facts, s, &guard));
    if !guard_holds {
        return Ok((
            Satisfaction::DoesNotApply,
            EvaluationStatus::Completed,
            None,
        ));
    }
    eval_goal(facts, &operand, states, depth + 1)
}

/// Deadline-window: an achievement/maintenance target indexed to a bounding interval.
///
/// Closure is signalled by the boolean marker `logic:deadlineWindowClosed "true"` on
/// the goal.  While open → `Undetermined`; once closed the operand's verdict becomes
/// conclusive.
fn eval_deadline_window(
    facts: &WorldFacts,
    goal_expr: &str,
    states: &[String],
    depth: usize,
) -> Result<(Satisfaction, EvaluationStatus, Option<String>), String> {
    let operand = facts
        .object(goal_expr, &logic(OPERAND))
        .ok_or_else(|| format!("deadline-window goal {goal_expr:?} has no logic:operand (target)"))?
        .to_owned();
    let closed = facts
        .object_n3(goal_expr, &logic("deadlineWindowClosed"))
        .is_some_and(|v| v.starts_with("\"true\""));
    let (sat, op_status, deg) = eval_goal(facts, &operand, states, depth + 1)?;
    if closed {
        let status = match op_status {
            EvaluationStatus::Undetermined => EvaluationStatus::Completed,
            other => other,
        };
        Ok((sat, status, deg))
    } else {
        Ok((sat, EvaluationStatus::Undetermined, deg))
    }
}

/// Optimization: directed at `logic:objectiveValue` read in `logic:objectiveDirection`.
///
/// Records the degree (`logic:satisfactionDegree`) rather than a crisp boolean: the
/// verdict is `PartiallySatisfied` carrying the degree, never folded into a truth
/// value.  The degree is read verbatim off the goal's `logic:satisfactionDegree`.
fn eval_optimization(
    facts: &WorldFacts,
    goal_expr: &str,
) -> Result<(Satisfaction, EvaluationStatus, Option<String>), String> {
    let degree = facts
        .object_n3(goal_expr, &logic("satisfactionDegree"))
        .map(|n| literal_lexical(n).to_owned());
    Ok((
        Satisfaction::PartiallySatisfied,
        EvaluationStatus::Completed,
        degree,
    ))
}

/// Extract the lexical form from an N3 literal (`"lex"^^<dt>` or `"lex"`), or pass an
/// IRI/bare form through unchanged.
fn literal_lexical(value: &str) -> &str {
    if let Some(rest) = value.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return &rest[..end];
        }
    }
    value
}

// ── Emitting a GoalEvaluation as quads with provenance ──────────────────────────

const SATISFACTION_STATUS: &str = "satisfactionStatus";
const GOAL_EVALUATION_STATUS: &str = "goalEvaluationStatus";
const SATISFACTION_DEGREE: &str = "satisfactionDegree";
const EVALUATES_GOAL: &str = "evaluatesGoal";
const HAS_GOAL_CONDITION: &str = "hasGoalCondition";

/// Emit a `logic:GoalEvaluation` for one verdict as provenance-carrying quads.
///
/// Each quad's provenance hashes `TELEOLOGY_RULE_IRI` over the reifier of the
/// goal-condition link that grounded the evaluation, so the same input yields the same
/// derivation IRIs (determinism).
fn emit_goal_evaluation(
    world: &str,
    eval_iri: &str,
    goal_iri: &str,
    verdict: &GoalVerdict,
    out: &mut Vec<TeleologyQuad>,
) -> Result<(), String> {
    let source = triple_reifier(
        goal_iri,
        &logic(HAS_GOAL_CONDITION),
        &verdict.goal_expression,
    )?;
    let mut push = |p: &str, o_n3: &str| {
        let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: eval_iri.to_owned(),
            predicate: p.to_owned(),
            object: o_n3.to_owned(),
            rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
            source_quad_ids: vec![source.clone()],
            derivation_id: deriv,
        });
    };
    push(RDF_TYPE, &n3(&logic("GoalEvaluation")));
    push(&logic(EVALUATES_GOAL), &n3(goal_iri));
    if let Some(local) = verdict.satisfaction.local() {
        push(&logic(SATISFACTION_STATUS), &n3(&logic(local)));
    }
    push(
        &logic(GOAL_EVALUATION_STATUS),
        &n3(&logic(verdict.evaluation_status.local())),
    );
    if let Some(deg) = &verdict.degree {
        let lit = format!("\"{deg}\"^^<http://www.w3.org/2001/XMLSchema#decimal>");
        push(&logic(SATISFACTION_DEGREE), &lit);
    }
    Ok(())
}

// ── 2. Plan-success classification ──────────────────────────────────────────────

/// The plan-success classification (`logic:PlanSuccessMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanSuccess {
    /// `logic:WeakPlanSuccess` — SOME outcome path reaches the goal.
    Weak,
    /// `logic:StrongPlanSuccess` — EVERY outcome path reaches the goal.
    Strong,
    /// `logic:StrongCyclicPlanSuccess` — every FAIR execution reaches it in finitely
    /// many steps (retried loops through recoverable failures allowed).
    StrongCyclic,
    /// No outcome reaches the goal — the plan fails under every mode.
    None,
}

impl PlanSuccess {
    /// The `logic:PlanSuccessMode` individual local name, or `None` when no outcome
    /// reaches the goal.
    #[must_use]
    pub fn local(self) -> Option<&'static str> {
        match self {
            Self::Weak => Some("WeakPlanSuccess"),
            Self::Strong => Some("StrongPlanSuccess"),
            Self::StrongCyclic => Some("StrongCyclicPlanSuccess"),
            Self::None => None,
        }
    }
}

const NONDETERMINISTIC_OUTCOME: &str = "nondeterministicOutcome";
const OUTCOME_SITUATION: &str = "outcomeSituation";
const COMPENSATION: &str = "compensation";

/// Classify a plan's success over the nondeterministic outcome set of one action
/// schema (the elementary case the W1 conformance scenarios exercise).
///
/// `goal_situation` is the situation type that counts as reaching the goal.
///
/// - every outcome reaches the goal → `Strong`
/// - some but not all reach it, every non-reaching outcome recoverable → `StrongCyclic`
/// - some reach it but a non-reaching outcome is NOT recoverable → `Weak`
/// - none reach it → `None`
///
/// P12: this CLASSIFIES a given plan + outcome set; it does not search for a plan.
///
/// # Errors
///
/// Returns `Err` if `schema` declares no `logic:nondeterministicOutcome` branch.
pub fn classify_plan_success(
    facts: &WorldFacts,
    schema: &str,
    goal_situation: &str,
) -> Result<PlanSuccess, String> {
    let outcomes_ref = facts.objects(schema, &logic(NONDETERMINISTIC_OUTCOME));
    if outcomes_ref.is_empty() {
        return Err(format!(
            "logic:ActionSchema {schema:?} declares no logic:nondeterministicOutcome branch"
        ));
    }
    let outcomes: Vec<String> = outcomes_ref.iter().map(|s| (*s).to_owned()).collect();
    let mut any_reaches = false;
    let mut all_reach = true;
    let mut all_nonreaching_recoverable = true;
    for o in &outcomes {
        let reaches = facts.has(o, &logic(OUTCOME_SITUATION), goal_situation);
        if reaches {
            any_reaches = true;
        } else {
            all_reach = false;
            let recoverable = facts
                .object_n3(o, &logic("recoverableOutcome"))
                .is_some_and(|v| v.starts_with("\"true\""));
            if !recoverable {
                all_nonreaching_recoverable = false;
            }
        }
    }
    Ok(if !any_reaches {
        PlanSuccess::None
    } else if all_reach {
        PlanSuccess::Strong
    } else if all_nonreaching_recoverable {
        PlanSuccess::StrongCyclic
    } else {
        PlanSuccess::Weak
    })
}

/// Resolve the outcome-specific compensation for a realized outcome branch.
///
/// Selects the `logic:compensation` named on THAT `logic:Outcome` (not a generic undo
/// on the schema).  P12: pure lookup over given structure.
///
/// # Errors
///
/// Returns `Err` if the outcome names no `logic:compensation`.
pub fn compensation_for_outcome(facts: &WorldFacts, outcome: &str) -> Result<String, String> {
    facts
        .object(outcome, &logic(COMPENSATION))
        .map(str::to_owned)
        .ok_or_else(|| format!("logic:Outcome {outcome:?} names no logic:compensation"))
}

// ── 3. Deontic obligation / prohibition evaluation ──────────────────────────────

const DEONTICALLY_IDEAL: &str = "deonticallyIdeal";

/// A deontic verdict over the ideal worlds accessible from a base world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeonticVerdict {
    /// The goal is satisfied in EVERY accessible ideal world — the obligation holds.
    ObligationHolds,
    /// The goal's NEGATION is supported in every accessible ideal world — the
    /// prohibition holds (support-for-negation, stronger than failure-of-support).
    ProhibitionHolds,
    /// The goal is neither obligatory nor prohibited across the ideal worlds.
    Neither,
    /// No accessible ideal world exists → `logic:GoalEvaluationUndetermined` (NOT a
    /// vacuously-true obligation).
    Undetermined,
}

/// Evaluate an obligation/prohibition for an atomic goal over the deontic-ideal worlds
/// accessible from `base_world` via `logic:deonticallyIdeal` (serial accessibility).
///
/// The goal is atomic on `goal_situation`: satisfied in an ideal world iff that
/// situation obtains at the ideal world's last state; its negation is SUPPORTED iff
/// `proscribed_situation` obtains there (a positive witness of the negation, kept
/// distinct from the goal merely failing to hold).
///
/// - obligation: satisfied in EVERY accessible ideal world
/// - prohibition: negation supported in EVERY accessible ideal world
/// - no accessible ideal world → `Undetermined` (never vacuously true)
///
/// P12: classification over the given accessibility structure; no search.
#[must_use]
pub fn evaluate_deontic(
    base_facts: &WorldFacts,
    base_world: &str,
    ideal_world_facts: &BTreeMap<String, WorldFacts>,
    goal_situation: &str,
    proscribed_situation: &str,
) -> DeonticVerdict {
    let _ = base_world;
    let mut ideals: Vec<&str> = base_facts
        .triples
        .iter()
        .filter(|t| t.predicate == logic(DEONTICALLY_IDEAL))
        .filter_map(|t| t.object_iri.as_deref())
        .filter(|w| ideal_world_facts.contains_key(*w))
        .collect();
    ideals.sort_unstable();
    ideals.dedup();
    if ideals.is_empty() {
        return DeonticVerdict::Undetermined;
    }
    let mut all_satisfied = true;
    let mut all_negation_supported = true;
    for w in &ideals {
        let wf = &ideal_world_facts[*w];
        let last = ordered_states(wf).ok().and_then(|s| s.last().cloned());
        let satisfied = last
            .as_deref()
            .is_some_and(|s| obtains_at(wf, s, goal_situation));
        let neg_supported = last
            .as_deref()
            .is_some_and(|s| obtains_at(wf, s, proscribed_situation));
        if !satisfied {
            all_satisfied = false;
        }
        if !neg_supported {
            all_negation_supported = false;
        }
    }
    if all_satisfied {
        DeonticVerdict::ObligationHolds
    } else if all_negation_supported {
        DeonticVerdict::ProhibitionHolds
    } else {
        DeonticVerdict::Neither
    }
}

// ── 4. Serialization-anomaly detection ──────────────────────────────────────────

/// A conflict edge in a transaction history: `from` precedes `to` on some item.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConflictEdge {
    /// The earlier transaction.
    pub from: String,
    /// The later transaction.
    pub to: String,
}

/// The result of conflict-serializability analysis over a transaction history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerializationVerdict {
    /// The conflict (precedence) graph is acyclic — conflict-serializable.
    Serializable,
    /// A cycle exists in the conflict graph — a serialization anomaly; the field is
    /// the cycle as a sequence of transaction IRIs (closing back to the first).
    Anomaly(Vec<String>),
}

/// Detect a serialization anomaly in a concurrent history given its conflict
/// (precedence) edges.
///
/// Conflict-serializability holds iff the precedence graph is acyclic; a cycle is a
/// serialization anomaly.  This is a HISTORY-LEVEL finding, NOT a contradiction
/// witness — no ⊥ is produced.  The returned cycle is canonical: rotated to start at
/// its lexicographically smallest member and reported as the smallest such cycle, so
/// the same history always yields the same reported cycle (determinism).
///
/// P12: classification over the given history; no search for a schedule.
#[must_use]
pub fn detect_serialization_anomaly(edges: &[ConflictEdge]) -> SerializationVerdict {
    let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut nodes: BTreeSet<String> = BTreeSet::new();
    for e in edges {
        adj.entry(e.from.clone()).or_default().insert(e.to.clone());
        nodes.insert(e.from.clone());
        nodes.insert(e.to.clone());
    }
    let mut best: Option<Vec<String>> = None;
    for start in &nodes {
        let mut stack: Vec<String> = vec![start.clone()];
        let mut on_path: HashSet<String> = HashSet::new();
        on_path.insert(start.clone());
        if let Some(cycle) = dfs_cycle(start, start, &adj, &mut stack, &mut on_path) {
            let canon = canonicalize_cycle(&cycle);
            best = Some(match best {
                Some(b) if b <= canon => b,
                _ => canon,
            });
        }
    }
    match best {
        Some(c) => SerializationVerdict::Anomaly(c),
        None => SerializationVerdict::Serializable,
    }
}

/// DFS searching for a path from `cur` back to `target` (a cycle through `target`).
fn dfs_cycle(
    target: &str,
    cur: &str,
    adj: &BTreeMap<String, BTreeSet<String>>,
    stack: &mut Vec<String>,
    on_path: &mut HashSet<String>,
) -> Option<Vec<String>> {
    if let Some(neighbours) = adj.get(cur) {
        for n in neighbours {
            if n == target {
                return Some(stack.clone());
            }
            if !on_path.contains(n) {
                stack.push(n.clone());
                on_path.insert(n.clone());
                if let Some(c) = dfs_cycle(target, n, adj, stack, on_path) {
                    return Some(c);
                }
                stack.pop();
                on_path.remove(n);
            }
        }
    }
    None
}

/// Rotate a cycle to start at its lexicographically smallest member, then close it.
fn canonicalize_cycle(cycle: &[String]) -> Vec<String> {
    if cycle.is_empty() {
        return Vec::new();
    }
    let min_idx = cycle
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .map_or(0, |(i, _)| i);
    let mut out: Vec<String> = Vec::with_capacity(cycle.len() + 1);
    for k in 0..cycle.len() {
        out.push(cycle[(min_idx + k) % cycle.len()].clone());
    }
    out.push(out[0].clone());
    out
}

/// Emit a `logic:SerializationAnomaly` finding quad for a detected cycle.
///
/// It is a finding, never a ⊥ witness; provenance is content-addressed over the
/// conflict edges that formed the cycle.
///
/// # Errors
///
/// Returns `Err` for an invalid transaction IRI in `edges`.
pub fn emit_serialization_anomaly(
    world: &str,
    finding_iri: &str,
    cycle: &[String],
    criterion_iri: &str,
    edges: &[ConflictEdge],
) -> Result<Vec<TeleologyQuad>, String> {
    let mut sources: Vec<String> = Vec::new();
    for e in edges {
        sources.push(triple_reifier(&e.from, &logic("precedes"), &e.to)?);
    }
    let src_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
    let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &src_refs);
    let mut out = Vec::new();
    let mut push = |p: &str, o_n3: String| {
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: finding_iri.to_owned(),
            predicate: p.to_owned(),
            object: o_n3,
            rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
            source_quad_ids: sources.clone(),
            derivation_id: deriv.clone(),
        });
    };
    push(RDF_TYPE, n3(&logic("SerializationAnomaly")));
    push(&logic("violatedCriterion"), n3(criterion_iri));
    let cycle_desc = cycle.join(" -> ");
    push(
        &logic("anomalyCycle"),
        format!(
            "\"{}\"",
            cycle_desc.replace('\\', "\\\\").replace('"', "\\\"")
        ),
    );
    Ok(out)
}

// ── 5. MCP action-policy evaluation ─────────────────────────────────────────────

/// The gate verdict for a proposed action under an action schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionGate {
    /// The action is admitted (precondition holds in the state AND capability available).
    Admit,
    /// The action is denied; carries the compensation/rollback action (if any) + reason.
    Deny {
        /// The compensation action to run on denial, if declared.
        compensation: Option<String>,
        /// Human-readable reason for the denial.
        reason: String,
    },
}

const PRECONDITION: &str = "precondition";
const CAPABILITY: &str = "capability";

/// Gate a proposed action under an action schema against the current state.
///
/// Admit iff every `logic:precondition` situation obtains at `state` AND every
/// `logic:capability` is available.  On failure, return the schema's
/// `logic:compensation` (rollback) to run.  Pure function over given structure (P12).
#[must_use]
pub fn gate_action(
    facts: &WorldFacts,
    schema: &str,
    state: &str,
    available_capabilities: &BTreeSet<String>,
) -> ActionGate {
    let compensation = facts
        .object(schema, &logic(COMPENSATION))
        .map(str::to_owned);
    let preconds: Vec<String> = facts
        .objects(schema, &logic(PRECONDITION))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    for sit in &preconds {
        if !obtains_at(facts, state, sit) {
            return ActionGate::Deny {
                compensation,
                reason: format!("precondition {sit:?} does not hold in state {state:?}"),
            };
        }
    }
    let caps: Vec<String> = facts
        .objects(schema, &logic(CAPABILITY))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    for cap in &caps {
        if !available_capabilities.contains(cap) {
            return ActionGate::Deny {
                compensation,
                reason: format!("capability {cap:?} is not available"),
            };
        }
    }
    ActionGate::Admit
}

// ── Top-level driver: evaluate all goals in a world & emit GoalEvaluations ───────

/// Evaluate every goal in `world` that carries a `logic:hasGoalCondition` over the
/// world's path, emitting one `logic:GoalEvaluation` per goal with full provenance.
///
/// The evaluation node IRI is minted deterministically so re-running over the same
/// input yields the same evaluation IRIs (and thus the same provenance).  Two
/// evaluators (two distinct criteria) are two coexisting evaluations — this driver
/// emits the default-vantage evaluation; a contested one is the caller's to add.
///
/// # Errors
///
/// Returns `Err` for any malformed goal expression or non-linear path.
pub fn evaluate_world_goals(store: &WorldStore, world: &str) -> Result<Vec<TeleologyQuad>, String> {
    let facts = WorldFacts::read(store, world);
    let states = ordered_states(&facts)?;
    let mut goal_links: Vec<(String, String)> = facts
        .triples
        .iter()
        .filter(|t| t.predicate == logic(HAS_GOAL_CONDITION))
        .filter_map(|t| {
            t.object_iri
                .as_deref()
                .map(|o| (t.subject.clone(), o.to_owned()))
        })
        .collect();
    goal_links.sort();
    goal_links.dedup();

    let mut out: Vec<TeleologyQuad> = Vec::new();
    for (goal, goal_expr) in &goal_links {
        let verdict = evaluate_goal_over_path(&facts, goal_expr, &states)?;
        let eval_iri = mint_eval_iri(goal, goal_expr, world);
        emit_goal_evaluation(world, &eval_iri, goal, &verdict, &mut out)?;
    }
    canonical_sort(&mut out);
    Ok(out)
}

/// Mint a deterministic evaluation node IRI for the default vantage.
fn mint_eval_iri(goal: &str, goal_expr: &str, world: &str) -> String {
    let payload = format!("{goal}\n{goal_expr}\n{world}");
    format!("{LOGIC_NS}eval/{}", crate::provenance::sha1_hex(&payload))
}

/// Canonical output sort: `(graph, subject, predicate, object)`.
fn canonical_sort(quads: &mut [TeleologyQuad]) {
    quads.sort_by(|a, b| {
        (&a.graph, &a.subject, &a.predicate, &a.object).cmp(&(
            &b.graph,
            &b.subject,
            &b.predicate,
            &b.object,
        ))
    });
}

/// The rule IRI stamped on every emitted teleology record (exposed for the seam).
#[must_use]
pub const fn rule_iri() -> &'static str {
    TELEOLOGY_RULE_IRI
}

#[cfg(test)]
mod tests;
