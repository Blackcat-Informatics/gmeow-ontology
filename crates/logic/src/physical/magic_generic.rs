// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **n-ary** backward leg: magic-sets (demand) transformation generalized to
//! arity-generic atoms, evaluated through the arity-generic positive-Datalog forward
//! core ([`super::generic::materialize_generic`]).
//!
//! # Why a second backward path
//!
//! The binary backward leg ([`super::magic`]) encodes every magic predicate over the
//! binary [`super::store::RelationStore`] — a predicate NAME keyed relation that drops
//! the world slot and cannot carry the property position as a DATA term. A backward goal
//! over the real n-ary shape — the `triple(?s, ?p, ?o, ?w)` predicate-as-data relation
//! that OWL 2 RL/RDF meta-rules bind a VARIABLE property against — is structurally
//! outside that store. [`super::magic::resolve_native`] dispatches such a goal (ANY atom
//! of arity != 2) HERE, mirroring the forward oracle's arity-eligibility split.
//!
//! # The transformation (standard magic-sets, arity-generic, left-to-right SIPS)
//!
//! Identical in structure to the binary [`super::magic::magic_transform`], generalized
//! from `(subject, object)` to a positional term vector:
//!
//! - A **guard** atom for an adorned atom carries the atom's **bound sub-tuple** — the
//!   args at the pattern's bound positions, in position order (arity = #bound positions).
//!   This is the REAL bound sub-tuple, not the binary store's self-loop hack: the generic
//!   store is arity-generic, so a single-bound-position guard is an arity-1 relation, a
//!   two-bound-position guard an arity-2 relation, and so on.
//! - The **magic predicate** IRI is minted by the SAME arity-agnostic
//!   [`super::magic::magic_pred_iri`] the binary leg uses.
//! - The **demand fixpoint** and **SIPS** thread bound variables left-to-right, keying
//!   demands on `(relation, pattern.code())`.
//!
//! # The fragment
//!
//! Positive Datalog only ([`materialize_generic`] is unbudgeted and negation-free); an
//! arithmetic builtin, a cut, or a negated body atom in the goal program is a declared gap
//! ([`NativeOutcome::Unsupported`]). Stratified negation-as-failure is supported ONLY on the
//! binary backward path ([`super::magic`]); an n-ary program carrying a `\+`/`not` literal
//! is an explicit, honest gap that production dispatch rejects, never a silent drop.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use super::generic::{GenericAtom, GenericRule, materialize_generic_budgeted};
use super::magic::{magic_pred_iri, term_of};
use crate::annotation::{
    AnnotatedAnswer, AnnotatedAnswerSet, AnnotatedTupleKey, AnnotationDerivation,
    AnnotationFactRef, AnnotationLineageContract, AnnotationQueryClass, AnnotationRequest,
    TupleAnnotationAlgebra,
};
use crate::facts::TypedFactSet;
use crate::oracle::{TypedProvenance, TypedRow};
use crate::physical::binding_pattern::BindingPattern;
use crate::physical::seminaive::{NativeOutcome, UnsupportedKind};
use crate::provenance::term_display;
use crate::query_ir::{
    AnswerSet, Binding, Budget, CompletionFrontier, GENERIC_TRIPLE_RELATION, QAtom, QBodyLit,
    QProgram, QTerm,
};
use crate::rule_ir::EvalTerm;
use crate::seam::{BudgetStatus, WorldFactSource};

// ── Lowering (QProgram → arity-generic IR, ALL terms kept) ───────────────────────

/// Lower a `QAtom` into a [`GenericAtom`], KEEPING every term (the predicate stays the
/// relation name; each arg is lowered with the shared [`term_of`] codec). A `Num` arg
/// lowers to its canonical typed-integer literal (a constant), NOT a gap — arithmetic is
/// only a gap when it appears as a BUILTIN literal (handled in [`resolve_native_generic`]).
fn generic_atom_of(atom: &QAtom) -> Result<GenericAtom, UnsupportedKind> {
    let mut args: Vec<EvalTerm> = Vec::with_capacity(atom.args.len());
    for t in &atom.args {
        args.push(term_of(t)?);
    }
    Ok(GenericAtom {
        relation: atom.pred.clone(),
        args,
    })
}

/// The goal atom's adornment: a position is bound iff its term is a constant.
fn goal_pattern(goal: &QAtom) -> BindingPattern {
    BindingPattern::from_bools(
        goal.args
            .iter()
            .map(|t| matches!(t, QTerm::Const(_) | QTerm::Num(_))),
    )
}

// ── SIPS adornment of a generic atom ─────────────────────────────────────────────

/// Adorn a generic body atom under a left-to-right SIPS: a position is bound iff its
/// term is a constant or an already-bound variable.
fn adorn_generic_atom(atom: &GenericAtom, bound: &BTreeSet<String>) -> BindingPattern {
    BindingPattern::from_bools(atom.args.iter().map(|t| match t {
        EvalTerm::Var(v) => bound.contains(v),
        EvalTerm::ConstNamed(_) | EvalTerm::ConstLit(_) => true,
    }))
}

/// Thread a generic atom's variable names into the bound set (the SIPS step).
fn bind_generic_atom_vars(atom: &GenericAtom, bound: &mut BTreeSet<String>) {
    for t in &atom.args {
        if let EvalTerm::Var(v) = t {
            bound.insert(v.clone());
        }
    }
}

/// The bound-variable set induced by the head's adornment (the head-bound arguments).
fn head_bound_vars(head: &GenericAtom, pattern: BindingPattern) -> BTreeSet<String> {
    let mut bound = BTreeSet::new();
    for pos in pattern.bound_positions() {
        if let EvalTerm::Var(v) = &head.args[pos] {
            bound.insert(v.clone());
        }
    }
    bound
}

// ── Magic guard / seed (the bound sub-tuple) ─────────────────────────────────────

/// Build a magic *guard* atom for an adorned generic atom: the guard carries the atom's
/// **bound sub-tuple** — the args at the pattern's bound positions, in position order
/// (arity = #bound positions). An all-free pattern → NO guard (`None`): the predicate is
/// demanded unrestricted.
fn magic_guard_atom(atom: &GenericAtom, pattern: BindingPattern) -> Option<GenericAtom> {
    if pattern.is_all_free() {
        return None;
    }
    let relation = magic_pred_iri(&atom.relation, &pattern.code());
    let args: Vec<EvalTerm> = pattern
        .bound_positions()
        .map(|p| atom.args[p].clone())
        .collect();
    Some(GenericAtom { relation, args })
}

/// Convert a ground [`EvalTerm`] into a [`TermValue`] for seed insertion. The seed's
/// terms are goal constants, so this never hits an unbound variable.
fn ground_eval_term(t: &EvalTerm) -> gmeow_errors::Result<TermValue> {
    match t {
        EvalTerm::ConstNamed(iri) => Ok(TermValue::iri(iri.clone())),
        EvalTerm::ConstLit(lit) => Ok(lit.clone()),
        EvalTerm::Var(v) => Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
            detail: format!("generic magic seed term {v:?} is not ground"),
        })),
    }
}

// ── The arity-generic magic-sets transformation ──────────────────────────────────

/// The output of [`magic_transform_generic`]: the transformed generic program plus the
/// ground seed fact `(relation, args)` (inserted into the EDB before evaluation), or
/// `None` for an all-free goal (no demand restriction).
struct GenericMagicProgram {
    rules: Vec<GenericRule>,
    seed: Option<(String, Vec<TermValue>)>,
}

/// A [`GenericRule`] with a synthesized deterministic rule IRI.
fn generic_rule(head: GenericAtom, body: Vec<GenericAtom>, rule_iri: String) -> GenericRule {
    GenericRule {
        head,
        body,
        rule_iri,
    }
}

/// Magic-transform `rules` w.r.t. the goal atom `goal` under `goal_pattern`.
///
/// Mirrors [`super::magic::magic_transform`] exactly, generalized to positional args: the
/// IDB is the set of rule-head relations; a demand fixpoint over the SIPS chain discovers
/// every reachable adorned IDB atom; each demand yields a modified rule (head guard ++
/// original body) and, per IDB body atom, a magic rule along the SIPS chain.
fn magic_transform_generic(
    rules: &[GenericRule],
    goal: &GenericAtom,
    goal_pattern: BindingPattern,
) -> gmeow_errors::Result<GenericMagicProgram> {
    let idb: BTreeSet<String> = rules.iter().map(|r| r.head.relation.clone()).collect();

    let mut out: Vec<GenericRule> = Vec::new();

    // (1) Seed: the goal's ground magic fact (none for an all-free goal).
    let seed = match magic_guard_atom(goal, goal_pattern) {
        Some(g) => {
            let args: gmeow_errors::Result<Vec<TermValue>> =
                g.args.iter().map(ground_eval_term).collect();
            Some((g.relation, args?))
        }
        None => None,
    };

    // Demand fixpoint keyed on (relation, pattern.code()); `demands` doubles as the
    // visited set so each frontier node expands once.
    let mut demands: BTreeSet<(String, String)> = BTreeSet::new();
    demands.insert((goal.relation.clone(), goal_pattern.code()));
    let mut frontier: Vec<(String, BindingPattern)> = vec![(goal.relation.clone(), goal_pattern)];

    while let Some((head_rel, head_pat)) = frontier.pop() {
        for r in rules.iter().filter(|r| r.head.relation == head_rel) {
            let mut bound = head_bound_vars(&r.head, head_pat);
            for atom in &r.body {
                if idb.contains(&atom.relation) {
                    let a = adorn_generic_atom(atom, &bound);
                    let demand = (atom.relation.clone(), a.code());
                    if demands.insert(demand) {
                        frontier.push((atom.relation.clone(), a));
                    }
                }
                bind_generic_atom_vars(atom, &mut bound);
            }
        }
    }

    // (2) Modified rules + (3) magic rules, for every demanded (relation, pattern).
    for (head_rel, adorn_code) in &demands {
        let head_pat = BindingPattern::from_code(adorn_code);
        for (ri, r) in rules
            .iter()
            .enumerate()
            .filter(|(_, r)| &r.head.relation == head_rel)
        {
            let mut bound = head_bound_vars(&r.head, head_pat);
            let head_guard = magic_guard_atom(&r.head, head_pat);

            // (2) Modified rule body: head magic guard (if any) ++ original body.
            let mut mod_body: Vec<GenericAtom> = Vec::new();
            if let Some(g) = &head_guard {
                mod_body.push(g.clone());
            }

            let mut prefix: Vec<GenericAtom> = Vec::new();
            for (bi, atom) in r.body.iter().enumerate() {
                if idb.contains(&atom.relation) {
                    let a = adorn_generic_atom(atom, &bound);
                    // (3) magic rule: magic_bi :- magic_head, b1..b(i-1) (none if all-free).
                    if let Some(magic_head) = magic_guard_atom(atom, a) {
                        let mut mbody: Vec<GenericAtom> = Vec::new();
                        if let Some(hg) = &head_guard {
                            mbody.push(hg.clone());
                        }
                        mbody.extend(prefix.iter().cloned());
                        let iri = format!(
                            "{}::magic/{}/{}#{ri}.{bi}",
                            atom.relation,
                            a.code(),
                            head_rel
                        );
                        out.push(generic_rule(magic_head, mbody, iri));
                    }
                }
                mod_body.push(atom.clone());
                prefix.push(atom.clone());
                bind_generic_atom_vars(atom, &mut bound);
            }

            let iri = format!("{}::mod/{}#{ri}", r.head.relation, adorn_code);
            out.push(generic_rule(r.head.clone(), mod_body, iri));
        }
    }

    Ok(GenericMagicProgram { rules: out, seed })
}

// ── The n-ary generic-triple EDB ─────────────────────────────────────────────────

/// Build the arity-4 generic-triple EDB `triple(subject, predicate, object, world)` by
/// scanning the world — the REAL n-ary data (the predicate carried as a DATA term) the
/// binary store cannot represent.
fn build_generic_edb(foreign: &dyn WorldFactSource, world: &str) -> TypedFactSet {
    let mut facts = TypedFactSet::new();
    for dq in foreign.in_world(world, None, None, None) {
        let s = facts.intern(&dq.subject);
        let p = facts.intern(&TermValue::iri(dq.predicate.as_str()));
        let o = facts.intern(&dq.object);
        let w = facts.intern(&TermValue::iri(world));
        facts.push_fact(GENERIC_TRIPLE_RELATION, vec![s, p, o, w]);
    }
    facts
}

// ── Servability gate (close the silent-empty for un-loadable EDB relations) ──────

/// Decide whether the generic evaluator can SOUNDLY serve every EDB relation the
/// program references.
///
/// The generic-triple EDB ([`build_generic_edb`]) loads world facts under EXACTLY ONE
/// relation — the arity-4 reserved [`GENERIC_TRIPLE_RELATION`].  A relation that is not
/// a rule head (not IDB) is therefore satisfiable ONLY if it is that reserved arity-4
/// relation; any OTHER non-IDB relation (e.g. a binary EDB predicate named `edge`) has
/// NO facts in the generic EDB, so a demand-restricted fixpoint over it derives nothing.
/// Returning that empty set as a decided answer would be a SILENT WRONG ANSWER — the
/// worst outcome.  Such a program is an honest gap: it is reported as
/// [`UnsupportedKind::NonBinaryAtom`] so dispatch routes it to the oracle, never a
/// silent-empty `Ok`.
///
/// A reserved-`triple` atom of the wrong arity is likewise un-servable (the EDB rows are
/// arity 4), so it is a gap too — never silently no-matched.
fn generic_program_servable(rules: &[GenericRule], goal: &GenericAtom) -> bool {
    let idb: BTreeSet<&str> = rules.iter().map(|r| r.head.relation.as_str()).collect();

    // A non-IDB relation is servable iff it is the reserved arity-4 generic-triple
    // relation; a reserved-`triple` atom (IDB or not) must be arity 4.
    let atom_ok = |atom: &GenericAtom| -> bool {
        if atom.relation == GENERIC_TRIPLE_RELATION {
            return atom.args.len() == 4;
        }
        idb.contains(atom.relation.as_str())
    };

    if !atom_ok(goal) {
        return false;
    }
    for r in rules {
        // Heads are IDB by construction, but a reserved-`triple` head still owes arity 4.
        if !atom_ok(&r.head) {
            return false;
        }
        if !r.body.iter().all(atom_ok) {
            return false;
        }
    }
    true
}

// ── Projection ───────────────────────────────────────────────────────────────────

/// Project the goal relation's derived tuples into [`AnswerSet`] bindings, arity-generic
/// mirror of [`super::magic`]'s `project_answers`: select rows on the goal relation,
/// apply the goal's constant-position constraints (compare `term_display`), and bind each
/// goal variable position — a repeated variable across positions must agree.
fn project_generic(rows: &[(TypedRow, TypedProvenance)], goal: &QAtom) -> Vec<Binding> {
    let mut bindings: Vec<Binding> = Vec::new();
    for (row, _) in rows {
        if row.predicate != goal.pred || row.args.len() != goal.args.len() {
            continue;
        }
        let surfaces: Vec<String> = row.args.iter().map(term_display).collect();

        // Apply the goal's constant-position constraints.
        let mut satisfies = true;
        for (pos, t) in goal.args.iter().enumerate() {
            let want = match t {
                QTerm::Const(c) => Some(c.clone()),
                QTerm::Num(n) => Some(term_display(&TermValue::typed_literal(
                    n.to_string(),
                    crate::physical::XSD_INTEGER,
                ))),
                QTerm::Var(_) => None,
            };
            if let Some(w) = want
                && surfaces[pos] != w
            {
                satisfies = false;
                break;
            }
        }
        if !satisfies {
            continue;
        }

        // Bind each goal variable; a repeated variable must agree across positions.
        let mut binding: Binding = BTreeMap::new();
        let mut agree = true;
        for (pos, t) in goal.args.iter().enumerate() {
            if let QTerm::Var(v) = t {
                match binding.get(v) {
                    Some(existing) if existing != &surfaces[pos] => {
                        agree = false;
                        break;
                    }
                    Some(_) => {}
                    None => {
                        binding.insert(v.clone(), surfaces[pos].clone());
                    }
                }
            }
        }
        if agree {
            bindings.push(binding);
        }
    }
    bindings
}

// ── Backward entry (n-ary) ───────────────────────────────────────────────────────

/// Resolve a NON-binary backward goal via the arity-generic magic-sets transform
/// evaluated through [`materialize_generic`]. The parity sibling of
/// [`super::magic::resolve_native`] for the n-ary fragment.
///
/// A cut or arithmetic builtin is a declared gap ([`NativeOutcome::Unsupported`]) — the
/// generic core is positive Datalog only. The goal atom is the single goal established by
/// the caller (`program.goal.atoms.len() == 1` is checked upstream).
///
/// # Budget
///
/// `max_steps` is threaded into the demand-restricted fixpoint through the single step
/// governor ([`materialize_generic_budgeted`]): `max_steps = Some(n)` cuts at `n`
/// committed derivations with a sound prefix ([`BudgetStatus::Exhausted`]), matching the
/// binary leg's deterministic-cut contract. A `max_answers` cap then applies as a sound
/// post-fixpoint truncation ([`BudgetStatus::Partial`]); a reached answer cap stamps
/// `Partial` even if the step budget also fired.
///
/// # Errors
///
/// Propagates a [`materialize_generic`] failure (e.g. an unbound head variable — a
/// malformed rule the positive-Datalog core cannot ground).
pub(super) fn resolve_native_generic(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    budget: &Budget,
) -> gmeow_errors::Result<NativeOutcome<AnswerSet>> {
    let goal = &program.goal.atoms[0];

    // (1) Lower rules to arity-generic IR, rejecting cut / arithmetic builtins.
    let mut rules: Vec<GenericRule> = Vec::with_capacity(program.rules.len());
    for r in &program.rules {
        let head = match generic_atom_of(&r.head) {
            Ok(a) => a,
            Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
        };
        let mut body: Vec<GenericAtom> = Vec::new();
        for lit in &r.body {
            match lit {
                QBodyLit::Atom(a) => match generic_atom_of(a) {
                    Ok(g) => body.push(g),
                    Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
                },
                // The generic core is positive Datalog only — negation / arithmetic / cut
                // are declared gaps. Stratified NAF is supported ONLY on the binary backward
                // path (`super::magic`); an n-ary program that carries a negated body atom
                // is an explicit, honest gap (never a silent drop of the negation).
                QBodyLit::Neg(_) => {
                    return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable));
                }
                QBodyLit::Builtin(_) => {
                    return Ok(NativeOutcome::Unsupported(UnsupportedKind::Arithmetic));
                }
                QBodyLit::Cut => return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut)),
            }
        }
        let rule_iri = format!("{}::rule", head.relation);
        rules.push(generic_rule(head, body, rule_iri));
    }

    // (2) Lower the goal, compute its adornment, and magic-transform.
    let goal_atom = match generic_atom_of(goal) {
        Ok(a) => a,
        Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
    };
    let pattern = goal_pattern(goal);

    // (2a) Servability gate: the generic-triple EDB loads facts under EXACTLY the
    //      reserved arity-4 `triple` relation, so a program that references any OTHER
    //      non-IDB relation (a binary EDB predicate like `edge`, or a mis-arity
    //      `triple`) cannot be soundly evaluated here — its demand-restricted fixpoint
    //      would derive nothing.  Emitting that empty set as `Decided` is a SILENT
    //      WRONG ANSWER; instead declare an honest gap so dispatch routes it to the
    //      oracle.  (The reserved `triple/4` shape passes this gate and decides below.)
    if !generic_program_servable(&rules, &goal_atom) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom));
    }

    let transformed = magic_transform_generic(&rules, &goal_atom, pattern)?;

    // (3) Build the generic-triple EDB, insert the seed demand fact, and run the
    //     arity-generic positive-Datalog fixpoint.
    let mut facts = build_generic_edb(foreign, world);
    if let Some((relation, args)) = &transformed.seed {
        let ids: Vec<_> = args.iter().map(|a| facts.intern(a)).collect();
        facts.push_fact(relation, ids);
    }
    // The generic core over the finite triple EDB (positive Datalog, no arithmetic) always
    // terminates, so no hang guard is needed; the step budget is threaded ONLY to honor the
    // deterministic-cut contract — `max_steps = Some(n)` cuts at `n` committed derivations
    // with a sound prefix (`Exhausted`), matching the binary leg.
    let (result, fixpoint_status) =
        materialize_generic_budgeted(&facts, &transformed.rules, budget.max_steps)?;

    // (4) Project the goal relation's tuples into bindings.
    let mut bindings = project_generic(&result.rows, goal);

    // (5) Budget: compose the step governor (fixpoint `Exhausted`) with a `max_answers`
    //     post-fixpoint truncation (`Partial`).  Precedence mirrors the binary leg: a
    //     reached answer cap stamps `Partial` even if the step budget also fired.
    let mut status = fixpoint_status;
    if let Some(max_a) = budget.max_answers {
        let mut tmp = AnswerSet {
            bindings: bindings.clone(),
            status: BudgetStatus::Ok,
            preservation: crate::result::PreservationClaim::exact(),
            frontier: CompletionFrontier::empty(),
        };
        tmp.canonicalize();
        if tmp.bindings.len() >= max_a && !tmp.bindings.is_empty() {
            tmp.bindings.truncate(max_a);
            status = BudgetStatus::Partial;
        }
        bindings = tmp.bindings;
    }

    let mut answer = AnswerSet {
        bindings,
        status,
        preservation: crate::result::PreservationClaim::exact(),
        frontier: CompletionFrontier::empty(),
    };
    answer.canonicalize();
    Ok(NativeOutcome::Decided(answer))
}

type GenericAnnotationKey = (String, Vec<String>);

#[derive(Clone)]
struct GenericAnnotatedRow<E> {
    relation: String,
    args: Vec<TermValue>,
    annotation: E,
    derivations: Vec<AnnotationDerivation<E>>,
}

fn generic_key(relation: &str, args: &[TermValue]) -> GenericAnnotationKey {
    (relation.to_owned(), args.iter().map(term_display).collect())
}

fn bind_generic(
    atom: &GenericAtom,
    row: &GenericAnnotatedRow<impl Clone>,
    base: &BTreeMap<String, TermValue>,
) -> Option<BTreeMap<String, TermValue>> {
    if atom.relation != row.relation || atom.args.len() != row.args.len() {
        return None;
    }
    let mut binding = base.clone();
    for (term, value) in atom.args.iter().zip(&row.args) {
        match term {
            EvalTerm::ConstNamed(iri) => {
                if term_display(value) != format!("<{iri}>") {
                    return None;
                }
            }
            EvalTerm::ConstLit(literal) => {
                if term_display(value) != term_display(literal) {
                    return None;
                }
            }
            EvalTerm::Var(name) => match binding.get(name) {
                Some(existing) if term_display(existing) != term_display(value) => return None,
                Some(_) => {}
                None => {
                    binding.insert(name.clone(), value.clone());
                }
            },
        }
    }
    Some(binding)
}

fn ground_generic(
    atom: &GenericAtom,
    binding: &BTreeMap<String, TermValue>,
) -> gmeow_errors::Result<Vec<TermValue>> {
    atom.args
        .iter()
        .map(|term| match term {
            EvalTerm::ConstNamed(iri) => Ok(TermValue::iri(iri.clone())),
            EvalTerm::ConstLit(literal) => Ok(literal.clone()),
            EvalTerm::Var(name) => binding.get(name).cloned().ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Physical {
                    detail: format!("generic annotated head variable {name:?} is unbound"),
                })
            }),
        })
        .collect()
}

fn generic_solutions<E: Clone>(
    rule: &GenericRule,
    rows: &BTreeMap<GenericAnnotationKey, GenericAnnotatedRow<E>>,
) -> Vec<(BTreeMap<String, TermValue>, Vec<GenericAnnotationKey>)> {
    let mut solutions = vec![(BTreeMap::new(), Vec::new())];
    for atom in &rule.body {
        let mut next = Vec::new();
        for (binding, sources) in solutions {
            for (key, row) in rows.iter().filter(|(_, row)| row.relation == atom.relation) {
                if let Some(merged) = bind_generic(atom, row, &binding) {
                    let mut lineage = sources.clone();
                    lineage.push(key.clone());
                    next.push((merged, lineage));
                }
            }
        }
        solutions = next;
        if solutions.is_empty() {
            break;
        }
    }
    solutions
}

fn generic_annotation_class(rules: &[GenericRule]) -> AnnotationQueryClass {
    // Predicate-as-data programs intentionally use one physical relation (`triple`) for
    // many logical predicates. A relation-name-only dependency graph therefore invents
    // recursion for `triple(_, <p2>, _, _) :- triple(_, <p1>, _, _)`. Refine the graph
    // by rule-head unifiability: a body atom depends on a producer only when their
    // relation, arity, and constant positions can actually unify.
    let compatible = |body: &GenericAtom, head: &GenericAtom| {
        body.relation == head.relation
            && body.args.len() == head.args.len()
            && body.args.iter().zip(&head.args).all(|(left, right)| {
                matches!(left, EvalTerm::Var(_))
                    || matches!(right, EvalTerm::Var(_))
                    || left == right
            })
    };
    let mut edges = vec![BTreeSet::new(); rules.len()];
    let mut indegree = vec![0usize; rules.len()];
    for (consumer, rule) in rules.iter().enumerate() {
        for body in &rule.body {
            for (producer, candidate) in rules.iter().enumerate() {
                if compatible(body, &candidate.head) && edges[consumer].insert(producer) {
                    indegree[producer] += 1;
                }
            }
        }
    }
    let mut ready = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, &degree)| (degree == 0).then_some(index))
        .collect::<BTreeSet<_>>();
    let mut visited = 0usize;
    while let Some(index) = ready.pop_first() {
        visited += 1;
        for &next in &edges[index] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.insert(next);
            }
        }
    }
    if visited != rules.len() {
        AnnotationQueryClass::PositiveNaryRecursive
    } else {
        AnnotationQueryClass::PositiveNaryAcyclic
    }
}

/// Score-carrying n-ary magic evaluation. Tuple membership, `oplus`/`otimes`, and
/// positional lineage are committed by one arity-generic fixpoint.
pub(super) fn resolve_native_generic_annotated<A, F>(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    budget: &Budget,
    annotation: &AnnotationRequest<'_, A, F>,
) -> gmeow_errors::Result<NativeOutcome<AnnotatedAnswerSet<A::Element>>>
where
    A: TupleAnnotationAlgebra,
    F: for<'fact> Fn(AnnotationFactRef<'fact>) -> Option<A::Element>,
{
    let goal = &program.goal.atoms[0];
    let mut rules = Vec::with_capacity(program.rules.len());
    for source in &program.rules {
        let head = match generic_atom_of(&source.head) {
            Ok(atom) => atom,
            Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
        };
        let mut body = Vec::new();
        for literal in &source.body {
            match literal {
                QBodyLit::Atom(atom) => match generic_atom_of(atom) {
                    Ok(atom) => body.push(atom),
                    Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
                },
                QBodyLit::Neg(_) => {
                    return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable));
                }
                QBodyLit::Builtin(_) => {
                    return Ok(NativeOutcome::Unsupported(UnsupportedKind::Arithmetic));
                }
                QBodyLit::Cut => return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut)),
            }
        }
        let rule_iri = format!("{}::rule", head.relation);
        rules.push(generic_rule(head, body, rule_iri));
    }
    let goal_atom = match generic_atom_of(goal) {
        Ok(atom) => atom,
        Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
    };
    if !generic_program_servable(&rules, &goal_atom) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom));
    }
    let certification = annotation.contract.certify_physical_class(
        generic_annotation_class(&rules),
        AnnotationLineageContract::AllPhysicalDerivations,
    )?;
    let transformed = magic_transform_generic(&rules, &goal_atom, goal_pattern(goal))?;
    let mut facts = build_generic_edb(foreign, world);
    let mut control_relations = transformed
        .rules
        .iter()
        .filter(|rule| rule.rule_iri.contains("::magic/"))
        .map(|rule| rule.head.relation.clone())
        .collect::<BTreeSet<_>>();
    if let Some((relation, args)) = &transformed.seed {
        let ids = args.iter().map(|arg| facts.intern(arg)).collect::<Vec<_>>();
        facts.push_fact(relation, ids);
        control_relations.insert(relation.clone());
    }

    let interner = facts.interner();
    let mut rows = BTreeMap::<GenericAnnotationKey, GenericAnnotatedRow<A::Element>>::new();
    let mut seeds = BTreeMap::<GenericAnnotationKey, A::Element>::new();
    for fact in facts.facts() {
        let args = fact
            .args
            .iter()
            .map(|&id| interner.resolve(id).clone())
            .collect::<Vec<_>>();
        let key = generic_key(&fact.predicate, &args);
        let value = if control_relations.contains(&fact.predicate) {
            annotation.algebra.one()
        } else if fact.predicate == GENERIC_TRIPLE_RELATION && args.len() == 4 {
            let predicate = match &args[1] {
                TermValue::Iri(iri) => iri.as_str(),
                _ => return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom)),
            };
            (annotation.annotation_for)(AnnotationFactRef {
                world,
                subject: &args[0],
                predicate,
                object: &args[2],
            })
            .unwrap_or_else(|| annotation.algebra.one())
        } else {
            annotation.algebra.one()
        };
        seeds.insert(key.clone(), value.clone());
        rows.insert(
            key,
            GenericAnnotatedRow {
                relation: fact.predicate.clone(),
                args,
                annotation: value.clone(),
                derivations: vec![AnnotationDerivation {
                    rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                    sources: Vec::new(),
                    tuple_sources: Vec::new(),
                    annotation: value,
                }],
            },
        );
    }

    let mut consumed = 0_u64;
    let mut status = BudgetStatus::Ok;
    let mut converged = false;
    for _round in 0..annotation.contract.max_fixpoint_rounds {
        let mut contributions = BTreeMap::<
            GenericAnnotationKey,
            Vec<(
                String,
                Vec<TermValue>,
                Vec<GenericAnnotationKey>,
                A::Element,
            )>,
        >::new();
        for rule in &transformed.rules {
            for (binding, sources) in generic_solutions(rule, &rows) {
                let args = ground_generic(&rule.head, &binding)?;
                let key = generic_key(&rule.head.relation, &args);
                let mut product = annotation.algebra.one();
                for source in &sources {
                    let factor = if control_relations.contains(&source.0) {
                        annotation.algebra.one()
                    } else {
                        rows.get(source)
                            .map(|row| row.annotation.clone())
                            .unwrap_or_else(|| annotation.algebra.zero())
                    };
                    product = annotation.algebra.multiply(&product, &factor)?;
                }
                contributions.entry(key).or_default().push((
                    rule.rule_iri.clone(),
                    args,
                    sources,
                    product,
                ));
            }
        }

        let mut inserted = false;
        for (key, direct) in &contributions {
            if rows.contains_key(key) {
                continue;
            }
            if budget.max_steps.is_some_and(|limit| consumed >= limit) {
                status = BudgetStatus::Exhausted;
                break;
            }
            let (_, args, _, _) = direct.first().expect("a contribution is non-empty");
            rows.insert(
                key.clone(),
                GenericAnnotatedRow {
                    relation: key.0.clone(),
                    args: args.clone(),
                    annotation: annotation.algebra.zero(),
                    derivations: Vec::new(),
                },
            );
            consumed = consumed.saturating_add(1);
            inserted = true;
        }

        let mut annotation_changed = false;
        for (key, row) in &mut rows {
            let mut value = seeds
                .get(key)
                .cloned()
                .unwrap_or_else(|| annotation.algebra.zero());
            let mut derivations = seeds.get(key).map_or_else(Vec::new, |seed| {
                vec![AnnotationDerivation {
                    rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                    sources: Vec::new(),
                    tuple_sources: Vec::new(),
                    annotation: seed.clone(),
                }]
            });
            if control_relations.contains(&row.relation) {
                value = annotation.algebra.one();
            } else if let Some(direct) = contributions.get(key) {
                for (rule_iri, _, sources, product) in direct {
                    value = annotation.algebra.add(&value, product)?;
                    derivations.push(AnnotationDerivation {
                        rule_iri: rule_iri.clone(),
                        sources: Vec::new(),
                        tuple_sources: sources
                            .iter()
                            .filter(|(relation, _)| !control_relations.contains(relation))
                            .map(|(relation, arguments)| AnnotatedTupleKey {
                                graph: world.to_owned(),
                                relation: relation.clone(),
                                arguments: arguments.clone(),
                            })
                            .collect(),
                        annotation: product.clone(),
                    });
                }
            }
            annotation_changed |= value != row.annotation;
            row.annotation = value;
            row.derivations = derivations;
        }
        if status == BudgetStatus::Exhausted {
            break;
        }
        if !inserted && !annotation_changed {
            converged = true;
            break;
        }
    }
    if status != BudgetStatus::Exhausted && !converged {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
            detail: format!(
                "generic annotation fixed point did not converge within {} deterministic rounds",
                annotation.contract.max_fixpoint_rounds
            ),
        }));
    }

    let mut grouped =
        BTreeMap::<Binding, (A::Element, Vec<AnnotationDerivation<A::Element>>)>::new();
    for row in rows.values() {
        let typed = TypedRow {
            predicate: row.relation.clone(),
            args: row.args.clone(),
        };
        let provenance = TypedProvenance {
            is_edb: false,
            rule_name: None,
            antecedents: Vec::new(),
            proof_height: None,
            attributions: Vec::new(),
        };
        let Some(binding) = project_generic(&[(typed, provenance)], goal)
            .into_iter()
            .next()
        else {
            continue;
        };
        match grouped.entry(binding) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert((row.annotation.clone(), row.derivations.clone()));
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let (value, lineage) = entry.get_mut();
                *value = annotation.algebra.add(value, &row.annotation)?;
                lineage.extend(row.derivations.clone());
            }
        }
    }
    let mut answers = grouped
        .into_iter()
        .map(|(binding, (annotation, derivations))| AnnotatedAnswer {
            binding,
            annotation,
            derivations,
        })
        .collect::<Vec<_>>();
    if let Some(max_answers) = budget.max_answers
        && answers.len() >= max_answers
        && !answers.is_empty()
    {
        answers.truncate(max_answers);
        status = BudgetStatus::Partial;
    }
    Ok(NativeOutcome::Decided(AnnotatedAnswerSet {
        answers,
        status,
        preservation: crate::result::PreservationClaim::exact(),
        frontier: CompletionFrontier {
            completed: usize::from(status != BudgetStatus::Exhausted),
            total: 1,
            saturated_preds: if status == BudgetStatus::Exhausted {
                BTreeSet::new()
            } else {
                rows.values().map(|row| row.relation.clone()).collect()
            },
            consumed_steps: consumed,
        },
        certification,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::QGoal;
    use crate::seam::WorldFactSnapshot;
    use crate::store::WorldStore;

    const W: &str = "http://logic.test/world/magic-generic";
    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const P1: &str = "http://ex/p1";
    const P2: &str = "http://ex/p2";

    struct SumProduct;

    impl TupleAnnotationAlgebra for SumProduct {
        type Element = f64;

        fn zero(&self) -> Self::Element {
            0.0
        }

        fn one(&self) -> Self::Element {
            1.0
        }

        fn add(
            &self,
            left: &Self::Element,
            right: &Self::Element,
        ) -> gmeow_errors::Result<Self::Element> {
            Ok(left + right)
        }

        fn multiply(
            &self,
            left: &Self::Element,
            right: &Self::Element,
        ) -> gmeow_errors::Result<Self::Element> {
            Ok(left * right)
        }
    }

    fn make_world(triples: &[(&str, &str, &str)]) -> (WorldStore, String) {
        let store = WorldStore::new();
        for (s, p, o) in triples {
            store.insert_quad(W, s, p, o);
        }
        (store, W.to_owned())
    }

    /// A `triple(?s, <pred>, ?o, ?w)` atom over the generic-triple encoding: the relation
    /// is the BARE `triple` symbol (matching `build_generic_edb`'s `push_fact("triple",…)`),
    /// the predicate position pinned to `<pred>` and the rest variables `s`/`o`/`w`.
    fn triple_atom(pred: &str) -> QAtom {
        QAtom {
            pred: "triple".to_owned(),
            args: vec![
                QTerm::Var("s".to_owned()),
                QTerm::Const(format!("<{pred}>")),
                QTerm::Var("o".to_owned()),
                QTerm::Var("w".to_owned()),
            ],
        }
    }

    /// The sub-property propagation program: `triple(?s,<p2>,?o,?w) :- triple(?s,<p1>,?o,?w)`
    /// with the arity-4 backward goal `triple(?s,<p2>,?o,?w)` — NOT binary-eligible.
    fn subprop_program() -> QProgram {
        QProgram {
            rules: vec![crate::query_ir::QRule {
                head: triple_atom(P2),
                body: vec![QBodyLit::Atom(triple_atom(P1))],
            }],
            goal: QGoal {
                atoms: vec![triple_atom(P2)],
            },
            counterfactual: None,
            prob_facts: vec![],
            prob_model: None,
            confidences: vec![],
        }
    }

    fn decided(outcome: NativeOutcome<AnswerSet>) -> AnswerSet {
        match outcome {
            NativeOutcome::Decided(a) => a,
            NativeOutcome::Unsupported(k) => panic!("expected Decided, got Unsupported({k:?})"),
        }
    }

    // ── (b) N-ary decides: genuine predicate-as-data backward resolution ─────────

    #[test]
    fn generic_subproperty_propagation_decides_nary_goal() {
        // A single <p1> edge x→y; the sub-property rule must derive x <p2> y, and the
        // arity-4 backward goal must return that derived edge — resolution the binary
        // store cannot express (the predicate rides in a DATA position).
        let (store, world) = make_world(&[("http://ex/x", P1, "http://ex/y")]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = subprop_program();

        // The dispatch signal: the goal atom is arity 4, so resolve_native routes here.
        assert_eq!(prog.goal.atoms[0].args.len(), 4, "arity != 2 ⇒ n-ary path");

        let answer = decided(
            super::super::magic::resolve_native(&foreign, &world, &prog, &Budget::default())
                .unwrap(),
        );
        assert_eq!(answer.status, BudgetStatus::Ok);
        assert_eq!(
            answer.bindings.len(),
            1,
            "exactly one derived <p2> edge: {answer:?}"
        );
        let b = &answer.bindings[0];
        assert_eq!(b["s"], "<http://ex/x>", "subject binding");
        assert_eq!(b["o"], "<http://ex/y>", "object binding");
        assert_eq!(b["w"], format!("<{W}>"), "world binding");
    }

    #[test]
    fn generic_goal_with_no_matching_edge_decides_empty() {
        // Only a <p2>-unrelated edge under a DIFFERENT predicate: no <p1> edge to
        // propagate, so the demand-restricted fixpoint derives nothing.
        let (store, world) = make_world(&[("http://ex/x", "http://ex/other", "http://ex/y")]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = subprop_program();
        let answer = decided(
            super::super::magic::resolve_native(&foreign, &world, &prog, &Budget::default())
                .unwrap(),
        );
        assert!(answer.bindings.is_empty(), "no <p1> edge ⇒ no answers");
    }

    // ── N-ary backward budget: the step governor threads the single counting point ─

    /// A transitive `<trans>` relation in the `triple(?s,?p,?o,?w)` encoding: the recursive
    /// rule `triple(?s,<trans>,?o,?w) :- triple(?s,<trans>,?m,?w), triple(?m,<trans>,?o,?w)`
    /// with the arity-4 backward goal `triple(?s,<trans>,?o,?w)` — n-ary (routes here).
    fn transitive_program() -> QProgram {
        let trans = "http://ex/trans";
        let edge = |s: &str, o: &str, w: &str| QAtom {
            pred: "triple".to_owned(),
            args: vec![
                QTerm::Var(s.to_owned()),
                QTerm::Const(format!("<{trans}>")),
                QTerm::Var(o.to_owned()),
                QTerm::Var(w.to_owned()),
            ],
        };
        QProgram {
            rules: vec![crate::query_ir::QRule {
                head: edge("s", "o", "w"),
                body: vec![
                    QBodyLit::Atom(edge("s", "m", "w")),
                    QBodyLit::Atom(edge("m", "o", "w")),
                ],
            }],
            goal: QGoal {
                atoms: vec![edge("s", "o", "w")],
            },
            counterfactual: None,
            prob_facts: vec![],
            prob_model: None,
            confidences: vec![],
        }
    }

    #[test]
    fn generic_backward_max_steps_exhausts_with_sound_prefix() {
        // A 4-node <trans> chain a→b→c→d; the transitive closure adds a→c, b→d, a→d.
        let trans = "http://ex/trans";
        let (store, world) = make_world(&[
            ("http://ex/a", trans, "http://ex/b"),
            ("http://ex/b", trans, "http://ex/c"),
            ("http://ex/c", trans, "http://ex/d"),
        ]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = transitive_program();

        // Unbudgeted: the full demand-restricted closure, complete (`Ok`).
        let full = decided(
            super::super::magic::resolve_native(&foreign, &world, &prog, &Budget::default())
                .unwrap(),
        );
        assert_eq!(full.status, BudgetStatus::Ok, "unbudgeted ⇒ Ok complete");
        // 3 EDB edges echoed + 3 derived transitive edges = 6 goal answers.
        assert_eq!(full.bindings.len(), 6, "full closure: {full:?}");
        let full_set: BTreeSet<(String, String)> = full
            .bindings
            .iter()
            .map(|b| (b["s"].clone(), b["o"].clone()))
            .collect();

        // Budgeted: a 1-step cut stamps `Exhausted` with a sound prefix (a strict subset).
        let budget = Budget {
            max_steps: Some(1),
            ..Default::default()
        };
        let cut =
            decided(super::super::magic::resolve_native(&foreign, &world, &prog, &budget).unwrap());
        assert_eq!(
            cut.status,
            BudgetStatus::Exhausted,
            "a 1-step budget cannot reach the closure ⇒ Exhausted: {cut:?}"
        );
        assert!(
            cut.bindings.len() < full.bindings.len(),
            "the cut answer set is a strict subset of the full closure: {cut:?}"
        );
        for b in &cut.bindings {
            assert!(
                full_set.contains(&(b["s"].clone(), b["o"].clone())),
                "every budget-cut answer is sound (present in the full closure): {b:?}"
            );
        }
    }

    // ── (c) Provenance: the derived answer carries its demand antecedent ─────────

    #[test]
    fn generic_derived_row_carries_demand_and_rule_provenance() {
        let (store, world) = make_world(&[("http://ex/x", P1, "http://ex/y")]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = subprop_program();

        // Drive the transform + generic materialization directly to inspect provenance.
        let rules: Vec<GenericRule> = prog
            .rules
            .iter()
            .map(|r| {
                let head = generic_atom_of(&r.head).unwrap();
                let body: Vec<GenericAtom> = r
                    .body
                    .iter()
                    .map(|l| match l {
                        QBodyLit::Atom(a) => generic_atom_of(a).unwrap(),
                        other => panic!("unexpected body literal {other:?}"),
                    })
                    .collect();
                let iri = format!("{}::rule", head.relation);
                generic_rule(head, body, iri)
            })
            .collect();
        let goal_atom = generic_atom_of(&prog.goal.atoms[0]).unwrap();
        let pattern = goal_pattern(&prog.goal.atoms[0]);
        let transformed = magic_transform_generic(&rules, &goal_atom, pattern).unwrap();

        // The seed is the goal's ground magic fact carrying the single bound sub-tuple
        // <p2> (the property position) — arity 1, NOT a self-loop.
        let (seed_rel, seed_args) = transformed.seed.clone().expect("bf/fbff goal ⇒ a seed");
        assert_eq!(seed_rel, magic_pred_iri("triple", "fbff"));
        assert_eq!(
            seed_args.len(),
            1,
            "one bound position ⇒ arity-1 bound sub-tuple"
        );
        assert_eq!(term_display(&seed_args[0]), format!("<{P2}>"));

        let mut facts = build_generic_edb(&foreign, &world);
        let ids: Vec<_> = seed_args.iter().map(|a| facts.intern(a)).collect();
        facts.push_fact(&seed_rel, ids);
        let (result, _status) =
            materialize_generic_budgeted(&facts, &transformed.rules, None).unwrap();

        // The derived triple(x, p2, y, w) row carries a firing rule name and its demand
        // antecedent (the magic guard) plus the antecedent <p1> edge.
        let derived = result
            .rows
            .iter()
            .find(|(row, prov)| {
                !prov.is_edb
                    && row.predicate == "triple"
                    && term_display(&row.args[1]) == format!("<{P2}>")
            })
            .expect("the derived <p2> edge must be present");
        let (_, prov) = derived;
        assert!(
            prov.rule_name.is_some(),
            "derived row names its firing rule"
        );
        let magic_rel = magic_pred_iri("triple", "fbff");
        assert!(
            prov.antecedents.iter().any(|a| a.predicate == magic_rel),
            "the derived answer carries its demand antecedent {magic_rel:?}: {:?}",
            prov.antecedents
        );
        assert!(
            prov.antecedents
                .iter()
                .any(|a| a.predicate == "triple" && term_display(&a.args[1]) == format!("<{P1}>")),
            "the derived answer carries its antecedent <p1> edge: {:?}",
            prov.antecedents
        );
    }

    #[test]
    fn annotated_generic_dispatch_carries_score_and_positional_lineage_in_one_fixpoint() {
        let (store, world) = make_world(&[("http://ex/x", P1, "http://ex/y")]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let program = subprop_program();
        let contract = crate::annotation::AnnotationContract::exact();
        let request =
            AnnotationRequest::new(&SumProduct, &contract, |fact: AnnotationFactRef<'_>| {
                (fact.predicate == P1).then_some(2.5)
            });

        let answer = match resolve_native_generic_annotated(
            &foreign,
            &world,
            &program,
            &Budget::default(),
            &request,
        )
        .unwrap()
        {
            NativeOutcome::Decided(answer) => answer,
            NativeOutcome::Unsupported(kind) => panic!("unexpected n-ary refusal: {kind:?}"),
        };

        assert_eq!(
            answer.certification.query_class,
            AnnotationQueryClass::PositiveNaryAcyclic
        );
        assert_eq!(
            answer.certification.lineage_contract,
            AnnotationLineageContract::AllPhysicalDerivations
        );
        assert_eq!(answer.answers.len(), 1);
        assert_eq!(answer.answers[0].annotation, 2.5);
        let rule = answer.answers[0]
            .derivations
            .iter()
            .find(|derivation| !derivation.tuple_sources.is_empty())
            .expect("derived answer retains positional tuple lineage");
        assert_eq!(
            rule.tuple_sources.len(),
            1,
            "magic control tuples are unit/hidden"
        );
        assert_eq!(rule.tuple_sources[0].relation, GENERIC_TRIPLE_RELATION);
        assert_eq!(rule.tuple_sources[0].arguments[1], format!("<{P1}>"));
    }
}
