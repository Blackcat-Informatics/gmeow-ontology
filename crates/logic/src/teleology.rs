// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native Rust evaluator for the canonical-process *teleology* layer.
//!
//! This module is the canonical native evaluator for the teleology / transaction
//! computations of the unified canonical process model: it EVALUATES and
//! CLASSIFIES given structure and RECORDS the verdict as `logic:GoalEvaluation`
//! quads and `logic:`-namespaced findings.  It mirrors [`crate::foundation`]'s
//! determinism contract verbatim so the conformance runner can wire it in
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

use gmeow_rdf::TermValue;

use crate::provenance::{mint_derivation_id, mint_reifier};
use crate::result::PreservationClaim;
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
pub(crate) fn n3(iri: &str) -> String {
    format!("<{iri}>")
}

/// Reifier IRI for an explicit `(s, p, o)` IRI triple, via the golden-pinned recipe.
pub(crate) fn triple_reifier(s: &str, p: &str, o: &str) -> Result<String, String> {
    let sn = TermValue::iri(s);
    let on = TermValue::iri(o);
    mint_reifier(&sn, p, &on)
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
/// deterministic and independent of the native store's internal iteration order — the
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
    pub(crate) fn objects(&self, subject: &str, predicate: &str) -> Vec<&str> {
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
    pub(crate) fn object(&self, subject: &str, predicate: &str) -> Option<&str> {
        self.objects(subject, predicate).into_iter().next()
    }

    /// The first object's N3 form (IRI or literal) for `(subject, predicate)`.
    pub(crate) fn object_n3(&self, subject: &str, predicate: &str) -> Option<&str> {
        self.sp_index
            .get(&(subject.to_owned(), predicate.to_owned()))
            .and_then(|idxs| idxs.first().map(|&i| self.triples[i].object_n3.as_str()))
    }

    /// Whether `(subject, predicate, object)` (object an IRI) is present.
    pub(crate) fn has(&self, subject: &str, predicate: &str, object: &str) -> bool {
        self.objects(subject, predicate).contains(&object)
    }

    /// All subjects asserted `(subject, rdf:type, class_iri)`, in content-sorted dedup
    /// order — the object-keyed counterpart of [`Self::objects`] for type discovery.
    pub(crate) fn subjects_with_type(&self, class_iri: &str) -> Vec<String> {
        let mut subs: Vec<String> = self
            .triples
            .iter()
            .filter(|t| t.predicate == RDF_TYPE && t.object_iri.as_deref() == Some(class_iri))
            .map(|t| t.subject.clone())
            .collect();
        subs.sort();
        subs.dedup();
        subs
    }

    /// Every `(subject, object IRI)` pair for `predicate`, in the content-sorted order of
    /// the underlying triples (literal objects skipped).
    pub(crate) fn subject_objects(&self, predicate: &str) -> Vec<(String, String)> {
        self.triples
            .iter()
            .filter(|t| t.predicate == predicate)
            .filter_map(|t| {
                t.object_iri
                    .as_ref()
                    .map(|o| (t.subject.clone(), o.clone()))
            })
            .collect()
    }

    /// Return a NEW content-sorted fact view extending `self` with the triples carried
    /// by `quads` (their `(subject, predicate, object)` shape; the per-quad provenance
    /// is dropped — facts are ground triples).
    ///
    /// Used by the dual-authority bridge post-pass so the forward direction can read the
    /// `logic:GoalEvaluation`s the driver itself just emitted, exactly as if they had
    /// been authored in the input — keeping the fold deterministic and store-free.
    fn extended_with(&self, quads: &[TeleologyQuad]) -> Self {
        let mut triples = self.triples.clone();
        for q in quads {
            let object_iri = strip_angle_opt(&q.object).map(str::to_owned);
            triples.push(Triple {
                subject: q.subject.clone(),
                predicate: q.predicate.clone(),
                object_iri,
                object_n3: q.object.clone(),
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
    // The situation/world judged against and the holding vantage complete the
    // identifying tuple, making a driver-emitted evaluation a well-formed,
    // vantage-indexed record the dual-authority bridge can project a flat
    // gmeow:satisfiedBy edge from. A path evaluation is judged against the world
    // (its path) under the default-of-silence vantage (gmeow:unspecifiedStandpoint).
    push(&logic(EVALUATED_AGAINST), &n3(world));
    push(
        &logic(EVALUATION_EVALUATOR),
        &n3(&gmeow(UNSPECIFIED_STANDPOINT)),
    );
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

// ── satisfiedBy ⟷ GoalEvaluation dual-authority bridge ──────────────────────────

/// The `gmeow:` vocabulary namespace (the surface vocabulary the flat
/// `gmeow:satisfiedBy` / `gmeow:accordingTo` edges live in).  Matches
/// [`crate::provenance::NAMESPACE`] verbatim.
const GMEOW_NS: &str = crate::provenance::NAMESPACE;

/// `gmeow:satisfiedBy` — the flat, conclusive projection of a satisfied+completed
/// `logic:GoalEvaluation`.
const SATISFIED_BY: &str = "satisfiedBy";

/// The IRI of the flat `gmeow:satisfiedBy` edge (used to detect the collapse in the output).
const SATISFIED_BY_IRI: &str = "https://blackcatinformatics.ca/gmeow/satisfiedBy";

/// The runtime preservation drop note emitted when `gmeow:satisfiedBy` edges appear in the
/// materialized output, indicating that the factored `logic:GoalEvaluation` axes were
/// collapsed to a flat binary edge.  Wording matches the compile-time disclosure in
/// `gmeow_logic_compile::projections::GOAL_EVAL_COLLAPSE_DROP` exactly.
const GOAL_EVAL_COLLAPSE_DROP: &str = concat!(
    "logic:GoalEvaluation factored axes (satisfaction/feasibility/lifecycle status, ",
    "satisfaction degree, criterion, evaluator/standpoint vantage multiplicity) ",
    "collapsed to flat binary gmeow:satisfiedBy edge"
);

/// `gmeow:accordingTo` — the vantage / standpoint a flat statement is asserted under
/// (the surface alias of `logic:evaluationEvaluator`; vantage ⊑ accordingTo).
const ACCORDING_TO: &str = "accordingTo";

/// `logic:evaluationEvaluator` — the vantage a reified `logic:GoalEvaluation` is
/// attributed to.
const EVALUATION_EVALUATOR: &str = "evaluationEvaluator";

/// `logic:evaluatedAgainst` — the situation / world a goal evaluation is judged against.
const EVALUATED_AGAINST: &str = "evaluatedAgainst";

/// `gmeow:unspecifiedStandpoint` — the documented default-of-silence vantage for a flat
/// `gmeow:satisfiedBy` edge authored with no explicit `gmeow:accordingTo` (unspecified,
/// NOT universal — see `slices/core/standpoint`).
const UNSPECIFIED_STANDPOINT: &str = "unspecifiedStandpoint";

/// Build a `gmeow:`-namespaced IRI string.
fn gmeow(local: &str) -> String {
    format!("{GMEOW_NS}{local}")
}

/// Reifier IRI for a `(subject, gmeow:satisfiedBy, object)` triple — the reified
/// statement node that carries the edge's vantage (`gmeow:accordingTo`), via the
/// golden-pinned [`mint_reifier`] recipe.
fn satisfied_by_reifier(goal: &str, situation: &str) -> Result<String, String> {
    triple_reifier(goal, &gmeow(SATISFIED_BY), situation)
}

/// A satisfied + completed evaluation, projected to the `(goal, situation, vantage)`
/// tuple that grounds the flat `gmeow:satisfiedBy` edge.
struct SatisfiedEval {
    /// The source `logic:GoalEvaluation` node IRI.
    eval_iri: String,
    /// `logic:evaluatesGoal`.
    goal: String,
    /// `logic:evaluatedAgainst`.
    situation: String,
    /// `logic:evaluationEvaluator` (the vantage; aligned with `gmeow:accordingTo`).
    vantage: String,
}

/// Enumerate every `logic:GoalEvaluation` in the world that is BOTH
/// `logic:satisfactionStatus = logic:Satisfied` AND
/// `logic:goalEvaluationStatus = logic:GoalEvaluationCompleted`, in deterministic
/// content order.  An evaluation missing its goal, situation, or evaluator is skipped
/// (it cannot ground a vantage-indexed flat edge) — the no-optionality contract on the
/// SHACL `logic:GoalEvaluationShape` governs well-formedness; the bridge is a pure
/// projection over the evaluations that ARE complete.
fn satisfied_completed_evals(facts: &WorldFacts) -> Vec<SatisfiedEval> {
    let satisfied = logic("Satisfied");
    let completed = logic("GoalEvaluationCompleted");
    // The distinct evaluation subjects, in sorted order (insertion-order enumeration).
    let mut evals: Vec<&str> = facts
        .triples
        .iter()
        .filter(|t| t.predicate == logic(SATISFACTION_STATUS))
        .filter(|t| t.object_iri.as_deref() == Some(satisfied.as_str()))
        .map(|t| t.subject.as_str())
        .collect();
    evals.sort_unstable();
    evals.dedup();

    let mut out: Vec<SatisfiedEval> = Vec::new();
    for e in evals {
        // Must ALSO be completed; satisfaction alone is not the projection condition.
        if !facts.has(e, &logic(GOAL_EVALUATION_STATUS), &completed) {
            continue;
        }
        let (Some(goal), Some(situation), Some(vantage)) = (
            facts.object(e, &logic(EVALUATES_GOAL)),
            facts.object(e, &logic(EVALUATED_AGAINST)),
            facts.object(e, &logic(EVALUATION_EVALUATOR)),
        ) else {
            // An incomplete (goal/situation/vantage missing) evaluation cannot project a
            // vantage-indexed edge — skip rather than mint a non-vantage-indexed one.
            continue;
        };
        out.push(SatisfiedEval {
            eval_iri: e.to_owned(),
            goal: goal.to_owned(),
            situation: situation.to_owned(),
            vantage: vantage.to_owned(),
        });
    }
    out
}

/// **Forward bridge.** Generate the flat, vantage-indexed `gmeow:satisfiedBy(goal,
/// situation)` edges that are the conclusive projection of every satisfied + completed
/// `logic:GoalEvaluation` in `facts`.
///
/// Per the source-of-truth rule (`LOGIC-TELEOLOGY.md` §"Goal evaluation is reified
/// and factored"): evaluations are canonical, and an edge is generated from each
/// evaluation whose `logic:satisfactionStatus` is `logic:Satisfied` AND whose
/// `logic:goalEvaluationStatus` is `logic:GoalEvaluationCompleted`.
///
/// The edge is **vantage-indexed** (Principle 9): it is emitted as the flat triple
/// `goal gmeow:satisfiedBy situation` PLUS a reified statement (the [`mint_reifier`]
/// node of that flat triple) carrying `gmeow:accordingTo vantage`, where `vantage` is
/// the source evaluation's `logic:evaluationEvaluator`.  Two contested evaluators that
/// both reach satisfied+completed therefore each generate their OWN edge under their
/// own vantage — there is never one global verdict; a single satisfied vantage among
/// dissenters generates exactly one edge.
///
/// Provenance: the derivation id is minted (via [`mint_derivation_id`]) from the source
/// evaluation's `logic:satisfactionStatus` reifier, so re-running over the same input
/// yields byte-identical quads and ids (determinism).
///
/// # Errors
///
/// Returns `Err` for an invalid IRI in a goal/situation/vantage (a malformed input the
/// reifier recipe rejects).
pub fn bridge_generate_satisfied_by(
    facts: &WorldFacts,
    world: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    let mut out: Vec<TeleologyQuad> = Vec::new();
    for ev in satisfied_completed_evals(facts) {
        // The antecedent: the satisfaction-status quad of the SOURCE evaluation.
        let source = triple_reifier(
            &ev.eval_iri,
            &logic(SATISFACTION_STATUS),
            &logic("Satisfied"),
        )?;
        let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
        // The reified statement node of the flat edge (carries the vantage).
        let stmt = satisfied_by_reifier(&ev.goal, &ev.situation)?;
        let mut push = |subject: &str, p: &str, o_n3: String| {
            out.push(TeleologyQuad {
                graph: world.to_owned(),
                subject: subject.to_owned(),
                predicate: p.to_owned(),
                object: o_n3,
                rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
                source_quad_ids: vec![source.clone()],
                derivation_id: deriv.clone(),
            });
        };
        // The flat, conclusive edge.
        push(&ev.goal, &gmeow(SATISFIED_BY), n3(&ev.situation));
        // The reified statement carrying its vantage — this is what makes the flat edge
        // vantage-indexed rather than a global verdict.
        push(&stmt, &gmeow(ACCORDING_TO), n3(&ev.vantage));
    }
    canonical_sort(&mut out);
    Ok(out)
}

/// An authored flat `gmeow:satisfiedBy(goal, situation)` edge and the vantage it is
/// asserted under.
struct AuthoredEdge {
    goal: String,
    situation: String,
    /// The vantage read off the edge's `gmeow:accordingTo`, or the documented default
    /// (`gmeow:unspecifiedStandpoint`) when the edge carries none.
    vantage: String,
}

/// Enumerate the authored flat `gmeow:satisfiedBy(goal, situation)` edges, resolving
/// each edge's vantage from the `gmeow:accordingTo` of its reified statement (the
/// [`mint_reifier`] node of the flat triple); an edge with no such index defaults to
/// `gmeow:unspecifiedStandpoint` (unspecified, NOT universal).
fn authored_satisfied_by(facts: &WorldFacts) -> Result<Vec<AuthoredEdge>, String> {
    let default_vantage = gmeow(UNSPECIFIED_STANDPOINT);
    let mut edges: Vec<(String, String)> = facts
        .triples
        .iter()
        .filter(|t| t.predicate == gmeow(SATISFIED_BY))
        .filter_map(|t| {
            t.object_iri
                .as_deref()
                .map(|o| (t.subject.clone(), o.to_owned()))
        })
        .collect();
    edges.sort();
    edges.dedup();

    let mut out: Vec<AuthoredEdge> = Vec::new();
    for (goal, situation) in edges {
        let stmt = satisfied_by_reifier(&goal, &situation)?;
        let vantages: Vec<String> = facts
            .objects(&stmt, &gmeow(ACCORDING_TO))
            .into_iter()
            .map(str::to_owned)
            .collect();
        if vantages.is_empty() {
            // No accordingTo present — fall back to the documented default.
            out.push(AuthoredEdge {
                goal,
                situation,
                vantage: default_vantage.clone(),
            });
        } else {
            // One AuthoredEdge per co-agreeing vantage so no vantage is dropped.
            for vantage in vantages {
                out.push(AuthoredEdge {
                    goal: goal.clone(),
                    situation: situation.clone(),
                    vantage,
                });
            }
        }
    }
    Ok(out)
}

/// Mint the content-addressed IRI of the DEFAULT `logic:GoalEvaluation` for an authored
/// edge, hashing `(goal, situation, vantage)` so re-running yields the SAME node
/// (idempotent, deterministic) — the reverse direction's analogue of [`mint_eval_iri`].
fn mint_default_eval_iri(goal: &str, situation: &str, vantage: &str) -> String {
    let payload = format!("{goal}\n{situation}\n{vantage}");
    format!("{LOGIC_NS}eval/{}", crate::provenance::sha1_hex(&payload))
}

/// **Reverse bridge.** Expand each AUTHORED flat `gmeow:satisfiedBy(goal, situation)`
/// edge that has NO backing `logic:GoalEvaluation` under its asserting vantage into a
/// DEFAULT evaluation carrying `logic:satisfactionStatus = logic:Satisfied`,
/// `logic:goalEvaluationStatus = logic:GoalEvaluationCompleted`,
/// `logic:evaluatesGoal = goal`, `logic:evaluatedAgainst = situation`, and
/// `logic:evaluationEvaluator = vantage` (the edge's `gmeow:accordingTo`, or
/// `gmeow:unspecifiedStandpoint` when none).
///
/// The minted evaluation node's IRI is content-addressed over `(goal, situation,
/// vantage)` ([`mint_default_eval_iri`]), so re-running yields the SAME node
/// (idempotent).  "Backing exists" is checked PER VANTAGE: an edge already backed by a
/// satisfied+completed evaluation under its own vantage is NOT re-expanded, while a
/// contested vantage that authored the same flat edge still gets its own default
/// evaluation.  This is the reverse of [`bridge_generate_satisfied_by`]; together they
/// keep the flat and reified records in agreement per-vantage.
///
/// Provenance: derivation id minted from the authored flat edge's reifier.
///
/// # Errors
///
/// Returns `Err` for an invalid IRI in a goal/situation (the reifier recipe rejects it).
pub fn bridge_expand_authored_satisfied_by(
    facts: &WorldFacts,
    world: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    // Index the (goal, situation, vantage) tuples ALREADY backed by a satisfied+completed
    // evaluation, so an authored edge with a real backing under its vantage is left alone.
    let backed: BTreeSet<(String, String, String)> = satisfied_completed_evals(facts)
        .into_iter()
        .map(|ev| (ev.goal, ev.situation, ev.vantage))
        .collect();

    let mut out: Vec<TeleologyQuad> = Vec::new();
    for edge in authored_satisfied_by(facts)? {
        let key = (
            edge.goal.clone(),
            edge.situation.clone(),
            edge.vantage.clone(),
        );
        if backed.contains(&key) {
            // Already backed under THIS vantage — the flat and reified records agree;
            // do not mint a duplicate default evaluation.
            continue;
        }
        let eval_iri = mint_default_eval_iri(&edge.goal, &edge.situation, &edge.vantage);
        let source = satisfied_by_reifier(&edge.goal, &edge.situation)?;
        let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
        let mut push = |p: &str, o_n3: String| {
            out.push(TeleologyQuad {
                graph: world.to_owned(),
                subject: eval_iri.clone(),
                predicate: p.to_owned(),
                object: o_n3,
                rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
                source_quad_ids: vec![source.clone()],
                derivation_id: deriv.clone(),
            });
        };
        push(RDF_TYPE, n3(&logic("GoalEvaluation")));
        push(&logic(EVALUATES_GOAL), n3(&edge.goal));
        push(&logic(EVALUATED_AGAINST), n3(&edge.situation));
        push(&logic(EVALUATION_EVALUATOR), n3(&edge.vantage));
        push(&logic(SATISFACTION_STATUS), n3(&logic("Satisfied")));
        push(
            &logic(GOAL_EVALUATION_STATUS),
            n3(&logic("GoalEvaluationCompleted")),
        );
    }
    canonical_sort(&mut out);
    Ok(out)
}

/// Run BOTH bridge directions over one world and return the union of their quads in
/// canonical order: forward (`logic:GoalEvaluation` → flat `gmeow:satisfiedBy`) and
/// reverse (authored flat `gmeow:satisfiedBy` → default `logic:GoalEvaluation`).
///
/// After this post-pass the flat and reified records agree PER VANTAGE — one is always
/// derived from the other (`LOGIC-TELEOLOGY.md`).  First-wins dedup on
/// `(graph, subject, predicate, object)` keeps a single record where both directions
/// would emit the same key.
///
/// # Errors
///
/// Returns `Err` for an invalid IRI in either direction.
pub fn bridge(facts: &WorldFacts, world: &str) -> Result<Vec<TeleologyQuad>, String> {
    let mut out = bridge_generate_satisfied_by(facts, world)?;
    out.extend(bridge_expand_authored_satisfied_by(facts, world)?);
    canonical_sort(&mut out);
    // First-wins dedup on the (graph, subject, predicate, object) key — the same
    // dedup discipline the foundation runner's fold applies.
    let mut seen: HashSet<(String, String, String, String)> = HashSet::new();
    out.retain(|q| {
        seen.insert((
            q.graph.clone(),
            q.subject.clone(),
            q.predicate.clone(),
            q.object.clone(),
        ))
    });
    Ok(out)
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
/// schema (the elementary case the conformance scenarios exercise).
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
///
/// # Errors
///
/// Returns `Err` if any accessible ideal world carries a non-linear path
/// (cyclic, forked, or disconnected).  The caller must surface this error;
/// degrading to `Neither` is not permitted.
pub fn evaluate_deontic(
    base_facts: &WorldFacts,
    base_world: &str,
    ideal_world_facts: &BTreeMap<String, WorldFacts>,
    goal_situation: &str,
    proscribed_situation: &str,
) -> Result<DeonticVerdict, String> {
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
        return Ok(DeonticVerdict::Undetermined);
    }
    let mut all_satisfied = true;
    let mut all_negation_supported = true;
    for w in &ideals {
        let wf = &ideal_world_facts[*w];
        let states = ordered_states(wf)?;
        let last = states.last().map(String::as_str);
        let satisfied = last.is_some_and(|s| obtains_at(wf, s, goal_situation));
        let neg_supported = last.is_some_and(|s| obtains_at(wf, s, proscribed_situation));
        if !satisfied {
            all_satisfied = false;
        }
        if !neg_supported {
            all_negation_supported = false;
        }
    }
    if all_satisfied {
        Ok(DeonticVerdict::ObligationHolds)
    } else if all_negation_supported {
        Ok(DeonticVerdict::ProhibitionHolds)
    } else {
        Ok(DeonticVerdict::Neither)
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
const INVARIANT: &str = "invariant";
const ACTION_RESOURCE: &str = "actionResource";
/// `logic:resourceSupply` — relates a state to a resource it supplies (availability).
const RESOURCE_SUPPLY: &str = "resourceSupply";
/// `logic:resourceExhausted` — a boolean marker on a resource that is depleted.
const RESOURCE_EXHAUSTED: &str = "resourceExhausted";

/// Whether a resource is supplied at `state` AND not marked exhausted.
///
/// Resource availability is a state fact (`state logic:resourceSupply resource`); a
/// resource the state does not supply, or one supplied but flagged
/// `logic:resourceExhausted "true"`, is unavailable.  This is the representation-level
/// seam to `logic:competesForResource` — two goals drawing on the same declared
/// resource whose supply is insufficient — NOT a real build engine-lock.
fn resource_available(facts: &WorldFacts, state: &str, resource: &str) -> bool {
    if !facts.has(state, &logic(RESOURCE_SUPPLY), resource) {
        return false;
    }
    !facts
        .object_n3(resource, &logic(RESOURCE_EXHAUSTED))
        .is_some_and(|v| v.starts_with("\"true\""))
}

/// Gate a proposed action under an action schema against the current state.
///
/// Admit iff, in deterministic order:
///
/// 1. every `logic:precondition` situation obtains at `state`;
/// 2. every `logic:capability` is in `available_capabilities`;
/// 3. every `logic:invariant` situation holds at `state` (the breach is a hard,
///    surfaced denial — an invariant the action would not preserve gates it, never a
///    silent pass); and
/// 4. every `logic:actionResource` the schema requires is available at `state` (the
///    state supplies it via `logic:resourceSupply` and it is not exhausted) — the
///    representation-level resource facet, not a real engine-lock.
///
/// On any failure, return the schema's `logic:compensation` (rollback) to run.  Pure
/// function over given structure (P12).
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
    // Invariant: every preserved condition must hold at the state across the action's
    // execution. A breach is a hard, surfaced denial — never a silent pass.
    let invariants: Vec<String> = facts
        .objects(schema, &logic(INVARIANT))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    for inv in &invariants {
        if !obtains_at(facts, state, inv) {
            return ActionGate::Deny {
                compensation,
                reason: format!("invariant {inv:?} is breached in state {state:?}"),
            };
        }
    }
    // Resource: every required resource must be available at the state (supplied and
    // not exhausted). A required resource the state does not supply gates the action.
    let resources: Vec<String> = facts
        .objects(schema, &logic(ACTION_RESOURCE))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    for res in &resources {
        if !resource_available(facts, state, res) {
            return ActionGate::Deny {
                compensation,
                reason: format!(
                    "resource {res:?} is unavailable (not supplied or exhausted) in state {state:?}"
                ),
            };
        }
    }
    ActionGate::Admit
}

// ── 6. Effect application as ins/del supersession ────────────────────────────────

/// `logic:transitionFromState` — the predecessor state a transaction step starts from.
const TRANSITION_FROM_STATE: &str = "transitionFromState";
/// `logic:transitionToState` — the successor state a transaction step materializes.
const TRANSITION_TO_STATE: &str = "transitionToState";
/// `logic:instantiatesSchema` — the action schema an occurrence/elementary update runs.
const INSTANTIATES_SCHEMA: &str = "instantiatesSchema";
/// `logic:ins` — a support the effect asserts into the successor state.
const INS: &str = "ins";
/// `logic:del` — a support the effect retires from the successor state.
const DEL: &str = "del";
/// `logic:activeInState` — the predecessor state a retired support was active in.
const ACTIVE_IN_STATE: &str = "activeInState";
/// `logic:validUntilState` — the successor state at which a support stops holding.
const VALID_UNTIL_STATE: &str = "validUntilState";
/// `logic:retiredByTransaction` — the update/step that retired a support.
const RETIRED_BY_TRANSACTION: &str = "retiredByTransaction";
/// `logic:supersededBy` — the update/successor support that superseded a retired one.
const SUPERSEDED_BY: &str = "supersededBy";

/// The computed support of a successor situation after an effect applies, kept as the
/// asserted set and the retired set so the caller can see both — `del` is recorded as
/// supersession, NEVER erased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuccessorSupport {
    /// Supports active in the successor state: the predecessor's carried-forward
    /// supports plus the effect's `ins`, minus the effect's `del`.
    pub asserted: BTreeSet<String>,
    /// Supports the effect retired — recorded as superseded (recoverable), not erased.
    pub retired: BTreeSet<String>,
}

/// Apply an action schema's `logic:effect` over a transaction step, computing the
/// successor situation's support as an ins/del SUPERSESSION over the predecessor.
///
/// The predecessor support is the set of situations obtaining at `from_state`
/// (`logic:situationObtains`).  The effect node (`schema logic:effect node`) carries
/// `logic:ins` supports (asserted into the successor) and `logic:del` supports (retired
/// from it).  The successor's asserted set is `predecessor ∪ ins \ del`; the retired set
/// is exactly the `del` supports that were active in the predecessor — these are NEVER
/// dropped from the historical record, they are returned as `retired` and emitted with
/// the supersession quartet by [`emit_effect_application`].
///
/// P12: pure computation over the given structure; no search, no mutation of the store.
///
/// # Errors
///
/// Returns `Err` if the schema names no `logic:effect` node (a malformed effect facet is
/// a hard error, never a silent empty effect).
pub fn apply_effect(
    facts: &WorldFacts,
    schema: &str,
    from_state: &str,
) -> Result<SuccessorSupport, String> {
    // The predecessor support of an authored step is the situations obtaining at the
    // authored `from_state` (`logic:situationObtains`).
    let predecessor: BTreeSet<String> = facts
        .objects(from_state, &logic(SITUATION_OBTAINS))
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();
    apply_effect_over(facts, schema, &predecessor)
}

/// Apply an action schema's `logic:effect` over an EXPLICIT predecessor support set,
/// computing the successor support as an ins/del supersession.
///
/// Identical in semantics to [`apply_effect`] except the predecessor support is supplied
/// directly rather than read from a `from_state`'s `logic:situationObtains` facts.  This
/// is what lets the transaction-program interpreter thread a *generated* support set
/// forward across engine-minted successor states (whose situations are not in the input
/// `WorldFacts`) — the same situation-level substrate, no store mutation.
///
/// `asserted = predecessor ∪ ins \ del`; `retired = del ∩ predecessor` (a `del` of a
/// support that was never active retires nothing — no phantom supersession).
///
/// # Errors
///
/// Returns `Err` if the schema names no `logic:effect` node.
pub(crate) fn apply_effect_over(
    facts: &WorldFacts,
    schema: &str,
    predecessor: &BTreeSet<String>,
) -> Result<SuccessorSupport, String> {
    let effect = facts
        .object(schema, &logic(EFFECT))
        .ok_or_else(|| format!("logic:ActionSchema {schema:?} names no logic:effect node"))?
        .to_owned();
    let ins: BTreeSet<String> = facts
        .objects(&effect, &logic(INS))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let del: BTreeSet<String> = facts
        .objects(&effect, &logic(DEL))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    // The retired set is the del supports that were ACTIVE in the predecessor — a del of
    // a support that was never active retires nothing (no phantom supersession).
    let retired: BTreeSet<String> = del.intersection(predecessor).cloned().collect();
    // asserted = predecessor ∪ ins \ del.
    let mut asserted: BTreeSet<String> = predecessor.union(&ins).cloned().collect();
    for d in &del {
        asserted.remove(d);
    }
    Ok(SuccessorSupport { asserted, retired })
}

/// Build the materialized quads for one effect application: the successor state's
/// asserted `logic:situationObtains` supports plus, for every retired support, the full
/// append-only supersession quartet (`activeInState` / `validUntilState` /
/// `retiredByTransaction` / `supersededBy`).
///
/// Shared by the authored-step family ([`emit_effect_application`], rule
/// `logic:rule/teleology`) and the transaction-program family (rule
/// `logic:rule/transaction`): the quad SHAPE is identical; only the grounding provenance
/// (`source`, `deriv`, `rule_iri`) and the retiring `attribution` node differ, so the
/// caller supplies them.  Keeping one emitter guarantees byte-identical supersession
/// substrate across both families.
#[allow(clippy::too_many_arguments)]
pub(crate) fn effect_quads(
    world: &str,
    support: &SuccessorSupport,
    from_state: &str,
    to_state: &str,
    attribution: &str,
    source: &str,
    deriv: &str,
    rule_iri: &str,
) -> Vec<TeleologyQuad> {
    let mut out: Vec<TeleologyQuad> = Vec::new();
    let mut push = |subject: &str, p: &str, o_n3: String| {
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: subject.to_owned(),
            predicate: p.to_owned(),
            object: o_n3,
            rule_iri: rule_iri.to_owned(),
            source_quad_ids: vec![source.to_owned()],
            derivation_id: deriv.to_owned(),
        });
    };
    // The successor state's asserted support — the effect advances the path by
    // materializing these supports in the successor snapshot.
    for sit in &support.asserted {
        push(to_state, &logic(SITUATION_OBTAINS), n3(sit));
    }
    // Every retired support is recorded as a SUPERSESSION (append-only), never erased:
    // the support remains recoverable via the quartet and the historical predecessor
    // state still carries it.
    for sit in &support.retired {
        push(sit, &logic(ACTIVE_IN_STATE), n3(from_state));
        push(sit, &logic(VALID_UNTIL_STATE), n3(to_state));
        push(sit, &logic(RETIRED_BY_TRANSACTION), n3(attribution));
        push(sit, &logic(SUPERSEDED_BY), n3(attribution));
    }
    out
}

/// Emit the materialized quads for an effect application over a transaction step.
///
/// The step (`step`) carries `logic:instantiatesSchema schema`,
/// `logic:transitionFromState from`, and `logic:transitionToState to`.  This emits, for
/// the computed successor support:
///
/// - the successor state's asserted supports as `to logic:situationObtains support`
///   (the effect's `ins` plus the carried-forward predecessor supports); and
/// - for every retired support, the full supersession quartet
///   (`logic:activeInState from`, `logic:validUntilState to`,
///   `logic:retiredByTransaction step`, `logic:supersededBy step`) so the retired
///   support stays recoverable/append-only — NEVER erased.
///
/// Provenance is content-addressed over the effect link the application grounded.
///
/// # Errors
///
/// Returns `Err` if the schema's effect facet is malformed, or for an invalid IRI in
/// the provenance recipe.
fn emit_effect_application(
    facts: &WorldFacts,
    world: &str,
    step: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    let schema = facts
        .object(step, &logic(INSTANTIATES_SCHEMA))
        .ok_or_else(|| {
            format!("transaction step {step:?} has no logic:instantiatesSchema action schema")
        })?
        .to_owned();
    let from_state = facts
        .object(step, &logic(TRANSITION_FROM_STATE))
        .ok_or_else(|| format!("transaction step {step:?} has no logic:transitionFromState"))?
        .to_owned();
    let to_state = facts
        .object(step, &logic(TRANSITION_TO_STATE))
        .ok_or_else(|| format!("transaction step {step:?} has no logic:transitionToState"))?
        .to_owned();
    let effect = facts
        .object(&schema, &logic(EFFECT))
        .ok_or_else(|| format!("logic:ActionSchema {schema:?} names no logic:effect node"))?
        .to_owned();
    let support = apply_effect(facts, &schema, &from_state)?;

    let source = triple_reifier(&schema, &logic(EFFECT), &effect)?;
    let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
    Ok(effect_quads(
        world,
        &support,
        &from_state,
        &to_state,
        step,
        &source,
        &deriv,
        TELEOLOGY_RULE_IRI,
    ))
}

// ── 7. Observation-conditioned policy ────────────────────────────────────────────

/// `logic:reveals` — relates an observation value to the situation it reveals.
const REVEALS: &str = "reveals";
/// `logic:branchObservation` — the observation a policy branch is conditioned on.
const BRANCH_OBSERVATION: &str = "branchObservation";
/// `logic:branchGuard` — the revealed situation that selects a policy branch.
const BRANCH_GUARD: &str = "branchGuard";
/// `logic:branchActionSchema` — the action schema a selected policy branch invokes.
const BRANCH_ACTION_SCHEMA: &str = "branchActionSchema";
/// `logic:selectedBranch` — the policy branch the engine selects from the observation.
const SELECTED_BRANCH: &str = "selectedBranch";
/// `logic:nextActionSchema` — the action schema the selected branch invokes (surfaced).
const NEXT_ACTION_SCHEMA: &str = "nextActionSchema";

/// Emit the observation an action schema reveals, and — when a policy conditions a branch
/// on it — the selected branch and its next action schema, so a policy reading is
/// possible from the materialized quads.
///
/// The schema's `logic:observation` value reveals a situation via `logic:reveals`.  A
/// policy branch (`branch logic:branchObservation observation`,
/// `branch logic:branchGuard revealedSituation`,
/// `branch logic:branchActionSchema nextSchema`) whose guard matches the revealed
/// situation is SELECTED: the engine surfaces `policy logic:selectedBranch branch` and
/// `policy logic:nextActionSchema nextSchema`.  This is what makes a plan a policy — its
/// next action is chosen from what an earlier action's observation revealed, not fixed in
/// advance.
///
/// P12: classification over the given structure; no search.
///
/// # Errors
///
/// Returns `Err` for an invalid IRI in the provenance recipe.
fn emit_observation_policy(
    facts: &WorldFacts,
    world: &str,
    schema: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    let observations: Vec<String> = facts
        .objects(schema, &logic(OBSERVATION))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let mut out: Vec<TeleologyQuad> = Vec::new();
    for obs in &observations {
        // What the observation reveals — surfaced verbatim so a policy can read it.
        let revealed: Vec<String> = facts
            .objects(obs, &logic(REVEALS))
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let source = triple_reifier(schema, &logic(OBSERVATION), obs)?;
        let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
        for rev in &revealed {
            out.push(TeleologyQuad {
                graph: world.to_owned(),
                subject: obs.clone(),
                predicate: logic(REVEALS),
                object: n3(rev),
                rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
                source_quad_ids: vec![source.clone()],
                derivation_id: deriv.clone(),
            });
            // Observation-conditioned branching: any policy branch whose guard matches
            // this revealed situation is selected, and its next action schema surfaced —
            // the policy reading of a plan.
            for branch in branches_conditioned_on(facts, obs) {
                if facts.has(&branch, &logic(BRANCH_GUARD), rev) {
                    let next = facts
                        .object(&branch, &logic(BRANCH_ACTION_SCHEMA))
                        .map(str::to_owned);
                    for policy in policies_with_branch(facts, &branch) {
                        let bsource = triple_reifier(&branch, &logic(BRANCH_GUARD), rev)?;
                        let bderiv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[bsource.as_str()]);
                        out.push(TeleologyQuad {
                            graph: world.to_owned(),
                            subject: policy.clone(),
                            predicate: logic(SELECTED_BRANCH),
                            object: n3(&branch),
                            rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
                            source_quad_ids: vec![bsource.clone()],
                            derivation_id: bderiv.clone(),
                        });
                        if let Some(next_schema) = &next {
                            out.push(TeleologyQuad {
                                graph: world.to_owned(),
                                subject: policy.clone(),
                                predicate: logic(NEXT_ACTION_SCHEMA),
                                object: n3(next_schema),
                                rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
                                source_quad_ids: vec![bsource.clone()],
                                derivation_id: bderiv.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The policy branches conditioned on `observation` via `logic:branchObservation`, sorted.
fn branches_conditioned_on(facts: &WorldFacts, observation: &str) -> Vec<String> {
    let mut subs: Vec<String> = facts
        .triples
        .iter()
        .filter(|t| t.predicate == logic(BRANCH_OBSERVATION))
        .filter(|t| t.object_iri.as_deref() == Some(observation))
        .map(|t| t.subject.clone())
        .collect();
    subs.sort();
    subs.dedup();
    subs
}

/// The policies (plans) that carry `branch` via `logic:planBranch`, sorted.
fn policies_with_branch(facts: &WorldFacts, branch: &str) -> Vec<String> {
    let mut subs: Vec<String> = facts
        .triples
        .iter()
        .filter(|t| t.predicate == logic(PLAN_BRANCH))
        .filter(|t| t.object_iri.as_deref() == Some(branch))
        .map(|t| t.subject.clone())
        .collect();
    subs.sort();
    subs.dedup();
    subs
}

/// `logic:effect` — the action schema's change facet.
const EFFECT: &str = "effect";
/// `logic:observation` — the action schema's observation facet.
const OBSERVATION: &str = "observation";
/// `logic:planBranch` — a policy/plan's observation-conditioned branch.
const PLAN_BRANCH: &str = "planBranch";

// ── 8. Gate-probe materialization (invariant + resource verdicts) ────────────────

/// `logic:GateProbe` — a node pairing an action schema with the state (and held
/// capabilities) the gate verdict is computed against.
const GATE_PROBE_CLASS: &str = "GateProbe";
/// `logic:probesSchema` — the action schema a gate probe gates.
const PROBES_SCHEMA: &str = "probesSchema";
/// `logic:probesState` — the state a gate probe gates the schema against.
const PROBES_STATE: &str = "probesState";
/// `logic:capabilityAvailable` — a capability the probed state makes available.
const CAPABILITY_AVAILABLE: &str = "capabilityAvailable";
/// `logic:gateVerdict` — the gate verdict (admit/deny) the probe records.
const GATE_VERDICT: &str = "gateVerdict";
/// `logic:gateDenialReason` — the human-readable denial reason on a denied probe.
const GATE_DENIAL_REASON: &str = "gateDenialReason";
/// `logic:gateCompensation` — the compensation the denied gate would run.
const GATE_COMPENSATION: &str = "gateCompensation";
/// `logic:GateAdmitted` — the admit verdict individual.
const GATE_ADMITTED: &str = "GateAdmitted";
/// `logic:GateDenied` — the deny verdict individual.
const GATE_DENIED: &str = "GateDenied";

/// Emit the gate verdict for one `logic:GateProbe`, surfacing the invariant-breach and
/// resource-exhaustion denials (and the precondition/capability ones) as materialized
/// quads with provenance.
///
/// The probe pairs a schema (`logic:probesSchema`) with a state (`logic:probesState`)
/// and the capabilities the state makes available (`logic:capabilityAvailable`).  The
/// emitted `logic:gateVerdict` is `logic:GateAdmitted` or `logic:GateDenied`; a denial
/// also carries `logic:gateDenialReason` and, when declared, `logic:gateCompensation`.
///
/// # Errors
///
/// Returns `Err` for a probe missing its schema or state, or an invalid provenance IRI.
fn emit_gate_probe(
    facts: &WorldFacts,
    world: &str,
    probe: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    let schema = facts
        .object(probe, &logic(PROBES_SCHEMA))
        .ok_or_else(|| format!("logic:GateProbe {probe:?} has no logic:probesSchema"))?
        .to_owned();
    let state = facts
        .object(probe, &logic(PROBES_STATE))
        .ok_or_else(|| format!("logic:GateProbe {probe:?} has no logic:probesState"))?
        .to_owned();
    let caps: BTreeSet<String> = facts
        .objects(&state, &logic(CAPABILITY_AVAILABLE))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let gate = gate_action(facts, &schema, &state, &caps);

    let source = triple_reifier(probe, &logic(PROBES_SCHEMA), &schema)?;
    let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
    let mut out: Vec<TeleologyQuad> = Vec::new();
    let mut push = |p: &str, o_n3: String| {
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: probe.to_owned(),
            predicate: p.to_owned(),
            object: o_n3,
            rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
            source_quad_ids: vec![source.clone()],
            derivation_id: deriv.clone(),
        });
    };
    match gate {
        ActionGate::Admit => {
            push(&logic(GATE_VERDICT), n3(&logic(GATE_ADMITTED)));
        }
        ActionGate::Deny {
            compensation,
            reason,
        } => {
            push(&logic(GATE_VERDICT), n3(&logic(GATE_DENIED)));
            push(
                &logic(GATE_DENIAL_REASON),
                format!("\"{}\"", reason.replace('\\', "\\\\").replace('"', "\\\"")),
            );
            if let Some(comp) = compensation {
                push(&logic(GATE_COMPENSATION), n3(&comp));
            }
        }
    }
    Ok(out)
}

/// `logic:TransactionStep` — an elementary update / step whose effect the driver applies.
const TRANSACTION_STEP_CLASS: &str = "TransactionStep";
/// `logic:ActionSchema` — the reusable action template the facets are declared on.
const ACTION_SCHEMA_CLASS: &str = "ActionSchema";

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

    // ── Dual-authority bridge post-pass ───────────────────────────────────────
    // Run BOTH directions over the input facts EXTENDED with the evaluations this
    // driver just emitted, so: (a) the forward direction projects a flat,
    // vantage-indexed `gmeow:satisfiedBy` edge from every satisfied+completed
    // evaluation (driver-emitted OR authored); and (b) the reverse direction expands
    // any authored `gmeow:satisfiedBy` edge that no vantage's evaluation backs into a
    // default satisfied+completed `logic:GoalEvaluation`. Afterwards the flat and
    // reified records agree per-vantage. First-wins dedup on (graph, subject,
    // predicate, object) keeps the bridge edges distinct from the driver's quads.
    let bridged_facts = facts.extended_with(&out);
    let bridged = bridge(&bridged_facts, world)?;
    let mut seen: HashSet<(String, String, String, String)> = out
        .iter()
        .map(|q| {
            (
                q.graph.clone(),
                q.subject.clone(),
                q.predicate.clone(),
                q.object.clone(),
            )
        })
        .collect();
    for q in bridged {
        let key = (
            q.graph.clone(),
            q.subject.clone(),
            q.predicate.clone(),
            q.object.clone(),
        );
        if seen.insert(key) {
            out.push(q);
        }
    }
    canonical_sort(&mut out);
    Ok(out)
}

// ── Whole-store materialization driver ──────────────────────────────────────────

/// Local name of `rdf:type`'s object for a `logic:Plan`.
const PLAN_CLASS: &str = "Plan";
/// `logic:planSchema` — the action schema whose nondeterministic outcomes a plan ranges over.
const PLAN_SCHEMA: &str = "planSchema";
/// `logic:planGoalSituation` — the situation type that counts as the plan reaching its goal.
const PLAN_GOAL_SITUATION: &str = "planGoalSituation";
/// `logic:planSuccessMode` — the selector the driver emits the classified mode under.
const PLAN_SUCCESS_MODE: &str = "planSuccessMode";
/// `logic:realizedOutcome` — the nondeterministic outcome branch that actually occurred.
const REALIZED_OUTCOME: &str = "realizedOutcome";
/// `logic:selectedCompensation` — the outcome-specific compensation the driver selects for a realized outcome.
const SELECTED_COMPENSATION: &str = "selectedCompensation";

/// `logic:planFlowEdge` — a reified flow edge (logic:ControlFlowEdge / logic:DataFlowEdge)
/// that constitutes a plan's flow graph (the certifier's input).
const PLAN_FLOW_EDGE: &str = "planFlowEdge";
/// `logic:planCertification` — links a plan to the logic:ReasoningResult recording its
/// DAG-workflow certification verdict.
const PLAN_CERTIFICATION: &str = "planCertification";
/// `logic:executedUnderContract` — the contract a plan declares it runs under.
const EXECUTED_UNDER_CONTRACT: &str = "executedUnderContract";
/// `logic:resourcePolicy` — the resource-execution policy a contract requests.
const RESOURCE_POLICY: &str = "resourcePolicy";
/// `logic:DagWorkflowResource` — the resource policy whose acyclicity this certifies.
const DAG_WORKFLOW_RESOURCE: &str = "DagWorkflowResource";
/// `logic:flowFrom` / `logic:flowTo` — the mediation legs of a reified flow edge.
const FLOW_FROM: &str = "flowFrom";
const FLOW_TO: &str = "flowTo";
/// `logic:dagCycleWitness` — the disclosed offending cycle member(s) of an unsupported verdict.
const DAG_CYCLE_WITNESS: &str = "dagCycleWitness";
/// `logic:resultEvaluation` / `logic:resultCompleteness` — the reasoning-result status fields.
const RESULT_EVALUATION: &str = "resultEvaluation";
const RESULT_COMPLETENESS: &str = "resultCompleteness";

/// The runtime preservation drop note emitted when a plan under the DAG-workflow profile
/// carries a cyclic flow graph: the loop is unsupported under the certified acyclic
/// fragment and is disclosed (logic:dagCycleWitness), never silently truncated.
const DAG_CYCLE_UNSUPPORTED_DROP: &str = concat!(
    "logic:Plan flow-graph cycle is unsupported under the DAG-workflow profile ",
    "(logic:DagWorkflowResource): the offending cycle members are disclosed by ",
    "logic:dagCycleWitness; the plan stays valid under a non-DAG contract"
);

/// `rdf:type` object for a `logic:DeonticContext`.
const DEONTIC_CONTEXT_CLASS: &str = "DeonticContext";
/// `logic:prescribedGoalSituation` — the atomic goal situation a deontic context prescribes.
const PRESCRIBED_GOAL_SITUATION: &str = "prescribedGoalSituation";
/// `logic:proscribedSituation` — the situation whose obtaining positively supports the goal's negation.
const PROSCRIBED_SITUATION: &str = "proscribedSituation";
/// `logic:prescribesGoal` — the (gmeow:Goal) the deontic context attributes the obligation/prohibition verdict to.
const PRESCRIBES_GOAL: &str = "prescribesGoal";

/// `rdf:type` object for a `logic:ConcurrentHistory`.
const CONCURRENT_HISTORY_CLASS: &str = "ConcurrentHistory";
/// `logic:precedes` — a conflict (precedence) edge `from precedes to` in a concurrent history.
const PRECEDES: &str = "precedes";
/// `logic:serializabilityCriterion` — the criterion a serialization-anomaly finding is recorded against.
const SERIALIZABILITY_CRITERION: &str = "serializabilityCriterion";
/// The default conflict-serializability criterion when a history names none.
const DEFAULT_SERIALIZABILITY_CRITERION: &str = "ConflictSerializability";

/// Materialize ALL applicable teleology computations over the worlds of `store`.
///
/// This is the conformance runner's single teleology entry point (the analogue of
/// [`crate::foundation::evaluate`]): given a [`WorldStore`] built from a case's
/// `input.nq`, it runs — per world, worlds in sorted order — every teleology
/// computation the world's facts call for and returns the union of emitted
/// [`TeleologyQuad`]s in canonical `(graph, subject, predicate, object)` order.
///
/// The five families, each a pure classification over the given structure (P12):
///
/// 1. **Goal-expression evaluation** — every `goal logic:hasGoalCondition expr` bound
///    to a `logic:GoalExpression`, evaluated over the world's `logic:Path`, emitted as a
///    factored `logic:GoalEvaluation` ([`evaluate_world_goals`], which ALSO runs the
///    satisfiedBy⟷GoalEvaluation bridge post-pass — family 5).
/// 2. **Plan-success classification** — every `logic:Plan` (with a `logic:planSchema`
///    and a `logic:planGoalSituation`), emitting `logic:planSuccessMode`; when the plan
///    names a `logic:realizedOutcome`, the outcome-specific `logic:selectedCompensation`
///    is selected from THAT branch.
/// 3. **Deontic obligation/prohibition** — every `logic:DeonticContext` (with a
///    `logic:prescribesGoal`, `logic:prescribedGoalSituation`, and the
///    `logic:deonticallyIdeal` accessibility into other worlds of the same store),
///    emitting a `logic:GoalEvaluation` whose status is `GoalEvaluationUndetermined`
///    when no accessible ideal world exists (never a vacuous obligation).
/// 4. **Serialization-anomaly detection** — every `logic:ConcurrentHistory` carrying
///    `logic:precedes` conflict edges, emitting a `logic:SerializationAnomaly` finding
///    when the precedence graph has a cycle (a finding, never a ⊥ witness).
/// 5. **satisfiedBy⟷GoalEvaluation bridge** — run inside [`evaluate_world_goals`]'s
///    post-pass over each world's facts (forward + reverse), keeping the flat and
///    reified records in agreement per-vantage.
///
/// Determinism is the foundation contract verbatim: worlds sorted, per-world structure
/// enumerated in content order, first-wins dedup on `(g, s, p, o)`, content-addressed
/// provenance, canonical final sort.
///
/// # Errors
///
/// Returns `Err` (no silent skip) for any malformed structure: a non-linear path, a
/// malformed goal expression, a plan with no outcome branch, an invalid IRI in a
/// conflict edge, or any provenance recipe failure.
pub fn materialize_teleology(
    store: &WorldStore,
) -> Result<(Vec<TeleologyQuad>, PreservationClaim), String> {
    let mut worlds = store.worlds();
    worlds.sort();
    worlds.dedup();

    // Per-world fact views, read ONCE up front so the deontic family can index the ideal
    // worlds accessible from any base world without re-reading the store.
    let mut world_facts: BTreeMap<String, WorldFacts> = BTreeMap::new();
    for w in &worlds {
        world_facts.insert(w.clone(), WorldFacts::read(store, w));
    }

    let mut out: Vec<TeleologyQuad> = Vec::new();
    // Tracks whether any plan's DAG-workflow certification resolved to the `unsupported`
    // verdict (a cyclic flow graph), so the materialization's preservation claim discloses
    // the dropped loop rather than claiming an exact fold over it.
    let mut dag_unsupported = false;
    for world in &worlds {
        let facts = &world_facts[world];

        // ── Family 0: echo the asserted EDB facts ────────────────────────────────
        // Mirror the foundation evaluator: every input triple is echoed as a
        // self-sourced asserted quad (rule = logic:assert). This is what makes the
        // derived quads' `source_quad_ids` reifiers resolve in the explanation index
        // (`explain_all` builds its reifier→quad map from ALL rows; a derived quad whose
        // antecedent is an authored fact would otherwise dangle). The echo carries the
        // object in its original N3 form so literals (degrees, recoverable flags) round
        // trip faithfully.
        for t in &facts.triples {
            out.push(asserted_quad(world, t));
        }

        // ── Family 1 + 5: goal-expression evaluation + bridge post-pass ──────────
        // `evaluate_world_goals` reads its own WorldFacts and runs the bridge; reuse it
        // wholesale so the conformance fold matches the unit-tested driver exactly.
        out.extend(evaluate_world_goals(store, world)?);

        // ── Family 2: plan-success classification (+ outcome compensation) +──────
        //    DAG-workflow certification for a plan run under logic:DagWorkflowResource.
        for plan in typed_subjects(facts, PLAN_CLASS) {
            out.extend(emit_plan_success(facts, world, &plan)?);
            let (dag_quads, cyclic) = emit_dag_certification(facts, world, &plan)?;
            out.extend(dag_quads);
            dag_unsupported |= cyclic;
        }

        // ── Family 3: deontic obligation / prohibition ───────────────────────────
        for ctx in typed_subjects(facts, DEONTIC_CONTEXT_CLASS) {
            out.extend(emit_deontic_evaluation(facts, world, &ctx, &world_facts)?);
        }

        // ── Family 4: serialization-anomaly detection ────────────────────────────
        for history in typed_subjects(facts, CONCURRENT_HISTORY_CLASS) {
            out.extend(emit_history_anomaly(facts, world, &history)?);
        }

        // ── Family 6: effect application as ins/del supersession ──────────────────
        // Every logic:TransactionStep applies the effect of the schema it instantiates,
        // computing the successor situation's support; retired supports are recorded as
        // supersession (recoverable, append-only), never erased.
        for step in typed_subjects(facts, TRANSACTION_STEP_CLASS) {
            out.extend(emit_effect_application(facts, world, &step)?);
        }

        // ── Family 7: observation-conditioned policy ─────────────────────────────
        // Every action schema that declares a logic:observation surfaces what it reveals
        // and, when a policy conditions a branch on it, the selected branch and next
        // action schema — the policy reading of a plan.
        for schema in typed_subjects(facts, ACTION_SCHEMA_CLASS) {
            out.extend(emit_observation_policy(facts, world, &schema)?);
        }

        // ── Family 8: gate-probe verdicts (invariant + resource + precond/cap) ────
        // Every logic:GateProbe pairs a schema with a state; the gate verdict surfaces
        // invariant-breach and resource-exhaustion denials as hard, recorded findings.
        for probe in typed_subjects(facts, GATE_PROBE_CLASS) {
            out.extend(emit_gate_probe(facts, world, &probe)?);
        }

        // ── Family 9: transaction-program executional entailment ──────────────────
        // Every executable transaction-program root (a combinator carrying
        // logic:transitionFromState) is EXECUTED under executional entailment: the
        // verdict + executed path surface as a logic:TransactionOutcome.  Placed AFTER
        // family 6 so on any situationObtains overlap the effect family's provenance
        // wins the first-wins dedup, keeping existing teleology goldens stable.
        for prog in crate::transaction::program_roots(facts) {
            out.extend(crate::transaction::emit_transaction_outcome(
                facts, world, &prog,
            )?);
        }
    }

    canonical_sort(&mut out);
    // First-wins dedup on (graph, subject, predicate, object) — the foundation fold's
    // discipline; the bridge already dedups within a world, this guards cross-family
    // overlap (e.g. a goal evaluation a deontic family would re-emit).
    let mut seen: HashSet<(String, String, String, String)> = HashSet::new();
    out.retain(|q| {
        seen.insert((
            q.graph.clone(),
            q.subject.clone(),
            q.predicate.clone(),
            q.object.clone(),
        ))
    });

    // Runtime preservation disclosure: when the forward bridge emitted one or more flat
    // `gmeow:satisfiedBy` edges (projecting a factored `logic:GoalEvaluation` to a
    // binary edge), the factored axes are absent from the materialized quads.  Disclose
    // the collapse as a non-exact (SoundUnder) claim so downstream consumers see what
    // was dropped.  When no `satisfiedBy` edge was generated the materialization is
    // exact — the full `logic:GoalEvaluation` structure is present in the quads.
    // Two independent runtime disclosures fold into one claim: the satisfiedBy collapse
    // (a factored GoalEvaluation projected to a flat binary edge) and a DAG-workflow
    // `unsupported` verdict (a cyclic plan whose loop the acyclic fragment cannot carry).
    // Either makes the fold non-exact (SoundUnder), naming exactly what was dropped; a run
    // with neither stays exact and reproduces the prior golden byte-for-byte.
    let mut drops: Vec<&str> = Vec::new();
    if out.iter().any(|q| q.predicate == SATISFIED_BY_IRI) {
        drops.push(GOAL_EVAL_COLLAPSE_DROP);
    }
    if dag_unsupported {
        drops.push(DAG_CYCLE_UNSUPPORTED_DROP);
    }
    let claim = if drops.is_empty() {
        PreservationClaim::exact()
    } else {
        PreservationClaim::for_unsupported(drops)
    };
    Ok((out, claim))
}

/// Echo one input `Triple` as a self-sourced asserted [`TeleologyQuad`] under the
/// `logic:assert` sentinel rule.
///
/// The reifier is the content-addressed identity of the `(s, p, o)` triple via the
/// shared [`crate::provenance::reifier_from_strings`] recipe (the same one
/// `triple_reifier` / the explanation engine agree on), so a derived quad citing this
/// fact's reifier resolves to this very echoed row. `derivation_id` hashes the assert
/// rule over the fact's own reifier (depth-0, self-rooted) exactly like the foundation
/// evaluator's asserted echo.
fn asserted_quad(world: &str, t: &Triple) -> TeleologyQuad {
    let reifier = crate::provenance::reifier_from_strings(&t.subject, &t.predicate, &t.object_n3);
    let deriv = mint_derivation_id(crate::provenance::ASSERT_RULE_IRI, &[reifier.as_str()]);
    TeleologyQuad {
        graph: world.to_owned(),
        subject: t.subject.clone(),
        predicate: t.predicate.clone(),
        object: t.object_n3.clone(),
        rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
        source_quad_ids: vec![reifier],
        derivation_id: deriv,
    }
}

/// The distinct subjects in `facts` typed `rdf:type logic:<class_local>`, sorted.
fn typed_subjects(facts: &WorldFacts, class_local: &str) -> Vec<String> {
    let class_iri = logic(class_local);
    let mut subs: Vec<String> = facts
        .triples
        .iter()
        .filter(|t| t.predicate == RDF_TYPE)
        .filter(|t| t.object_iri.as_deref() == Some(class_iri.as_str()))
        .map(|t| t.subject.clone())
        .collect();
    subs.sort();
    subs.dedup();
    subs
}

/// Classify one `logic:Plan` and emit `logic:planSuccessMode`; when the plan names a
/// realized outcome, also emit the outcome-specific `logic:selectedCompensation`.
fn emit_plan_success(
    facts: &WorldFacts,
    world: &str,
    plan: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    let schema = facts
        .object(plan, &logic(PLAN_SCHEMA))
        .ok_or_else(|| format!("logic:Plan {plan:?} has no logic:planSchema"))?
        .to_owned();
    let goal_situation = facts
        .object(plan, &logic(PLAN_GOAL_SITUATION))
        .ok_or_else(|| format!("logic:Plan {plan:?} has no logic:planGoalSituation"))?
        .to_owned();
    let mode = classify_plan_success(facts, &schema, &goal_situation)?;

    let source = triple_reifier(plan, &logic(PLAN_SCHEMA), &schema)?;
    let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
    let mut out: Vec<TeleologyQuad> = Vec::new();
    let mut push = |subject: &str, p: &str, o_n3: String| {
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: subject.to_owned(),
            predicate: p.to_owned(),
            object: o_n3,
            rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
            source_quad_ids: vec![source.clone()],
            derivation_id: deriv.clone(),
        });
    };
    // A plan whose outcome set reaches the goal under SOME mode records that mode; a
    // plan no outcome reaches records none (PlanSuccess::None.local() == None) — the
    // classification is honest about total failure rather than inventing a weak success.
    if let Some(local) = mode.local() {
        push(plan, &logic(PLAN_SUCCESS_MODE), n3(&logic(local)));
    }

    // Outcome-specific compensation: when the plan names the branch that actually
    // occurred, select THAT outcome's compensation (not a generic schema undo).
    if let Some(realized) = facts.object(plan, &logic(REALIZED_OUTCOME)) {
        let realized = realized.to_owned();
        let comp = compensation_for_outcome(facts, &realized)?;
        let csource = triple_reifier(&realized, &logic(COMPENSATION), &comp)?;
        let cderiv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[csource.as_str()]);
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: plan.to_owned(),
            predicate: logic(SELECTED_COMPENSATION),
            object: n3(&comp),
            rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
            source_quad_ids: vec![csource],
            derivation_id: cderiv,
        });
    }
    Ok(out)
}

/// Content-addressed IRI of a plan's DAG-workflow certification result, over `(plan,
/// DAG resource)` so re-running is idempotent (mirrors [`mint_deontic_eval_iri`]).
fn mint_dag_result_iri(plan: &str) -> String {
    let payload = format!("dag-cert\n{plan}");
    format!("{LOGIC_NS}result/{}", crate::provenance::sha1_hex(&payload))
}

/// Certify one `logic:Plan`'s flow graph under the DAG-workflow profile and emit the
/// verdict as a `logic:ReasoningResult` linked by `logic:planCertification`.
///
/// Fires ONLY for a plan that (a) gathers reified flow edges through `logic:planFlowEdge`
/// and (b) runs `logic:executedUnderContract` a contract requesting `logic:DagWorkflowResource`
/// (`logic:resourcePolicy`). The plan's `flowFrom -> flowTo` edges are run through the SAME
/// shared certifier the build pipeline delegates to ([`crate::dag_profile::certify_acyclic`]),
/// so there is one acyclicity authority. An acyclic plan is certified
/// `logic:EvaluationCompleted` + `logic:CompleteForFragment`; a cyclic plan resolves to the
/// `logic:EvaluationUnsupported` verdict with each offending cycle member disclosed by
/// `logic:dagCycleWitness` — never silently truncated. The plan stays valid canonically
/// (the build pipeline keeps its own load-time certification; this is the canonical
/// reified-flow-graph form a general `logic:Plan` carries).
///
/// Returns `(quads, cyclic)` — `cyclic` is `true` iff an `unsupported` verdict was emitted,
/// so the caller can fold the disclosure into the materialization's preservation claim.
///
/// # Errors
///
/// Returns `Err` (no silent skip) for a `logic:planFlowEdge` member missing a
/// `logic:flowFrom` or `logic:flowTo` mediation leg (a malformed flow edge).
fn emit_dag_certification(
    facts: &WorldFacts,
    world: &str,
    plan: &str,
) -> Result<(Vec<TeleologyQuad>, bool), String> {
    // A plan's flow graph is reachable only through logic:planFlowEdge; without edges
    // there is nothing to certify (the build pipeline uses its own gmeow:hasStage /
    // gmeow:dataflowConsumes form, certified at load time, not here).
    let edges: Vec<String> = facts
        .objects(plan, &logic(PLAN_FLOW_EDGE))
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    if edges.is_empty() {
        return Ok((Vec::new(), false));
    }
    // Certify only under a contract that requests the DAG-workflow resource policy.
    let under_dag = facts
        .objects(plan, &logic(EXECUTED_UNDER_CONTRACT))
        .iter()
        .any(|c| facts.has(c, &logic(RESOURCE_POLICY), &logic(DAG_WORKFLOW_RESOURCE)));
    if !under_dag {
        return Ok((Vec::new(), false));
    }

    // Build producer -> consumer string edges from each reified edge's flowFrom / flowTo.
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(edges.len());
    for e in &edges {
        let from = facts
            .object(e, &logic(FLOW_FROM))
            .ok_or_else(|| format!("logic flow edge {e:?} has no logic:flowFrom"))?
            .to_owned();
        let to = facts
            .object(e, &logic(FLOW_TO))
            .ok_or_else(|| format!("logic flow edge {e:?} has no logic:flowTo"))?
            .to_owned();
        pairs.push((from, to));
    }
    let cert =
        crate::dag_profile::certify_acyclic(pairs.iter().map(|(a, b)| (a.as_str(), b.as_str())));
    let (evaluation, completeness) = cert.result_status();
    let witness = cert.witness();
    let cyclic = !cert.is_certified();

    // The verdict node is content-addressed over the plan so re-running is idempotent;
    // it is provenance-rooted at the plan's executedUnderContract edge (the fact that
    // places the plan under the DAG profile).
    let contract = facts
        .object(plan, &logic(EXECUTED_UNDER_CONTRACT))
        .expect("under_dag implies an executedUnderContract object")
        .to_owned();
    let result_iri = mint_dag_result_iri(plan);
    let source = triple_reifier(plan, &logic(EXECUTED_UNDER_CONTRACT), &contract)?;
    let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
    let mut out: Vec<TeleologyQuad> = Vec::new();
    let mut push = |subject: &str, p: &str, o_n3: String| {
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: subject.to_owned(),
            predicate: p.to_owned(),
            object: o_n3,
            rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
            source_quad_ids: vec![source.clone()],
            derivation_id: deriv.clone(),
        });
    };
    // The plan reaches its verdict; the verdict carries the two status axes the shared
    // certifier maps the resource policy onto, and (when cyclic) the disclosed witness.
    push(plan, &logic(PLAN_CERTIFICATION), n3(&result_iri));
    push(&result_iri, RDF_TYPE, n3(&logic("ReasoningResult")));
    push(
        &result_iri,
        &logic(RESULT_EVALUATION),
        n3(&logic(evaluation.local_name())),
    );
    push(
        &result_iri,
        &logic(RESULT_COMPLETENESS),
        n3(&logic(completeness.local_name())),
    );
    // The witness names every offending cycle member (sorted, deterministic). Empty on an
    // acyclic plan — the certified verdict carries no witness.
    for member in &witness {
        push(&result_iri, &logic(DAG_CYCLE_WITNESS), n3(member));
    }
    Ok((out, cyclic))
}

/// Evaluate one `logic:DeonticContext` and emit a `logic:GoalEvaluation` over its
/// deontically-ideal accessible worlds.
///
/// The verdict maps to the factored axes: an obligation that holds → `Satisfied` +
/// `Completed`; a prohibition that holds → `Violated` + `Completed`; neither →
/// `Unsatisfied` + `Completed`; NO accessible ideal world → `Unsatisfied` +
/// `Undetermined` (never a vacuous obligation — the conclusiveness axis carries the
/// "no ideal world" signal, not a fabricated truth value).
fn emit_deontic_evaluation(
    facts: &WorldFacts,
    world: &str,
    ctx: &str,
    world_facts: &BTreeMap<String, WorldFacts>,
) -> Result<Vec<TeleologyQuad>, String> {
    let goal = facts
        .object(ctx, &logic(PRESCRIBES_GOAL))
        .ok_or_else(|| format!("logic:DeonticContext {ctx:?} has no logic:prescribesGoal"))?
        .to_owned();
    let goal_situation = facts
        .object(ctx, &logic(PRESCRIBED_GOAL_SITUATION))
        .ok_or_else(|| {
            format!("logic:DeonticContext {ctx:?} has no logic:prescribedGoalSituation")
        })?
        .to_owned();
    // A proscribed situation is optional: a context that names none can never witness
    // ProhibitionHolds (no positive support for the negation), which is correct.
    let proscribed = facts
        .object(ctx, &logic(PROSCRIBED_SITUATION))
        .unwrap_or("")
        .to_owned();

    let verdict = evaluate_deontic(facts, world, world_facts, &goal_situation, &proscribed)?;
    let (sat, status) = match verdict {
        DeonticVerdict::ObligationHolds => (Satisfaction::Satisfied, EvaluationStatus::Completed),
        DeonticVerdict::ProhibitionHolds => (Satisfaction::Violated, EvaluationStatus::Completed),
        DeonticVerdict::Neither => (Satisfaction::Unsatisfied, EvaluationStatus::Completed),
        DeonticVerdict::Undetermined => (Satisfaction::Unsatisfied, EvaluationStatus::Undetermined),
    };

    // The evaluation node is content-addressed over (context, goal, world) so re-running
    // is idempotent. The deontic verdict is judged against the BASE world under the
    // default-of-silence vantage, exactly like a path evaluation.
    let eval_iri = mint_deontic_eval_iri(ctx, &goal, world);
    let source = triple_reifier(ctx, &logic(PRESCRIBES_GOAL), &goal)?;
    let deriv = mint_derivation_id(TELEOLOGY_RULE_IRI, &[source.as_str()]);
    let mut out: Vec<TeleologyQuad> = Vec::new();
    let mut push = |p: &str, o_n3: String| {
        out.push(TeleologyQuad {
            graph: world.to_owned(),
            subject: eval_iri.clone(),
            predicate: p.to_owned(),
            object: o_n3,
            rule_iri: TELEOLOGY_RULE_IRI.to_owned(),
            source_quad_ids: vec![source.clone()],
            derivation_id: deriv.clone(),
        });
    };
    push(RDF_TYPE, n3(&logic("GoalEvaluation")));
    push(&logic(EVALUATES_GOAL), n3(&goal));
    push(&logic(EVALUATED_AGAINST), n3(world));
    push(
        &logic(EVALUATION_EVALUATOR),
        n3(&gmeow(UNSPECIFIED_STANDPOINT)),
    );
    if let Some(local) = sat.local() {
        push(&logic(SATISFACTION_STATUS), n3(&logic(local)));
    }
    push(&logic(GOAL_EVALUATION_STATUS), n3(&logic(status.local())));
    Ok(out)
}

/// Mint a deterministic deontic-evaluation node IRI (content-addressed over context,
/// goal, and base world), so re-running yields the same node.
fn mint_deontic_eval_iri(ctx: &str, goal: &str, world: &str) -> String {
    let payload = format!("deontic\n{ctx}\n{goal}\n{world}");
    format!("{LOGIC_NS}eval/{}", crate::provenance::sha1_hex(&payload))
}

/// Detect a serialization anomaly over one `logic:ConcurrentHistory`'s `logic:precedes`
/// conflict edges, emitting a `logic:SerializationAnomaly` finding for a cycle.
///
/// A serializable (acyclic) history emits nothing — there is no anomaly to record.
fn emit_history_anomaly(
    facts: &WorldFacts,
    world: &str,
    history: &str,
) -> Result<Vec<TeleologyQuad>, String> {
    let _ = history;
    // The conflict edges are the world's logic:precedes facts (a single history per
    // world in the conformance scenarios), enumerated in content order.
    let mut edges: Vec<ConflictEdge> = facts
        .triples
        .iter()
        .filter(|t| t.predicate == logic(PRECEDES))
        .filter_map(|t| {
            t.object_iri.as_deref().map(|to| ConflictEdge {
                from: t.subject.clone(),
                to: to.to_owned(),
            })
        })
        .collect();
    edges.sort();
    edges.dedup();

    let criterion = facts
        .object(history, &logic(SERIALIZABILITY_CRITERION))
        .map_or_else(|| logic(DEFAULT_SERIALIZABILITY_CRITERION), str::to_owned);

    match detect_serialization_anomaly(&edges) {
        SerializationVerdict::Serializable => Ok(Vec::new()),
        SerializationVerdict::Anomaly(cycle) => {
            // The finding node is content-addressed over (history, cycle) so re-running
            // is idempotent.
            let finding_iri = mint_anomaly_finding_iri(history, &cycle);
            emit_serialization_anomaly(world, &finding_iri, &cycle, &criterion, &edges)
        }
    }
}

/// Mint a deterministic serialization-anomaly finding IRI (content-addressed over the
/// history and the canonical cycle), so re-running yields the same finding node.
///
/// Exposed `pub(crate)` so the transaction family ([`crate::transaction`]) mints the finding
/// IRI for a DERIVED concurrent history through the SAME content-address recipe the authored
/// Family-4 path uses — one recipe, no drift.
pub(crate) fn mint_anomaly_finding_iri(history: &str, cycle: &[String]) -> String {
    let payload = format!("anomaly\n{history}\n{}", cycle.join("\n"));
    format!(
        "{LOGIC_NS}finding/{}",
        crate::provenance::sha1_hex(&payload)
    )
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
