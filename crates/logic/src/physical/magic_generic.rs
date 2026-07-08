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
//! is an explicit, honest gap routed to the oracle, never a silent drop.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use super::generic::{GenericAtom, GenericRule, materialize_generic};
use super::magic::{magic_pred_iri, term_of};
use crate::facts::TypedFactSet;
use crate::oracle::{TypedProvenance, TypedRow};
use crate::physical::binding_pattern::BindingPattern;
use crate::physical::seminaive::{NativeOutcome, UnsupportedKind};
use crate::provenance::term_display;
use crate::query_ir::{
    AnswerSet, Binding, Budget, CompletionFrontier, QAtom, QBodyLit, QProgram, QTerm,
};
use crate::rule_ir::EvalTerm;
use crate::seam::{BudgetStatus, ScryerForeign};

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
fn ground_eval_term(t: &EvalTerm) -> Result<TermValue, String> {
    match t {
        EvalTerm::ConstNamed(iri) => Ok(TermValue::iri(iri.clone())),
        EvalTerm::ConstLit(lit) => Ok(lit.clone()),
        EvalTerm::Var(v) => Err(format!("generic magic seed term {v:?} is not ground")),
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
) -> Result<GenericMagicProgram, String> {
    let idb: BTreeSet<String> = rules.iter().map(|r| r.head.relation.clone()).collect();

    let mut out: Vec<GenericRule> = Vec::new();

    // (1) Seed: the goal's ground magic fact (none for an all-free goal).
    let seed = match magic_guard_atom(goal, goal_pattern) {
        Some(g) => {
            let args: Result<Vec<TermValue>, String> =
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
fn build_generic_edb(foreign: &dyn ScryerForeign, world: &str) -> TypedFactSet {
    let mut facts = TypedFactSet::new();
    for dq in foreign.in_world(world, None, None, None) {
        let s = facts.intern(&dq.subject);
        let p = facts.intern(&TermValue::iri(dq.predicate.as_str()));
        let o = facts.intern(&dq.object);
        let w = facts.intern(&TermValue::iri(world));
        facts.push_fact("triple", vec![s, p, o, w]);
    }
    facts
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
/// The generic core is unbudgeted; a `max_answers` cap applies as a sound post-fixpoint
/// truncation ([`BudgetStatus::Partial`]). `max_steps` composition is a later concern —
/// this path evaluates to the full demand-restricted fixpoint.
///
/// # Errors
///
/// Propagates a [`materialize_generic`] failure (e.g. an unbound head variable — a
/// malformed rule the positive-Datalog core cannot ground).
pub(super) fn resolve_native_generic(
    foreign: &dyn ScryerForeign,
    world: &str,
    program: &QProgram,
    budget: &Budget,
) -> Result<NativeOutcome<AnswerSet>, String> {
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
                // is an explicit, honest gap routed to the oracle (never a silent drop of
                // the negation).
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
    let transformed = magic_transform_generic(&rules, &goal_atom, pattern)?;

    // (3) Build the generic-triple EDB, insert the seed demand fact, and run the
    //     arity-generic positive-Datalog fixpoint.
    let mut facts = build_generic_edb(foreign, world);
    if let Some((relation, args)) = &transformed.seed {
        let ids: Vec<_> = args.iter().map(|a| facts.intern(a)).collect();
        facts.push_fact(relation, ids);
    }
    let result = materialize_generic(&facts, &transformed.rules)?;

    // (4) Project the goal relation's tuples into bindings.
    let mut bindings = project_generic(&result.rows, goal);

    // (5) Budget: `max_answers` as a sound post-fixpoint truncation (Partial).
    let mut status = BudgetStatus::Ok;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::QGoal;
    use crate::seam::WorldStoreForeign;
    use crate::store::WorldStore;

    const W: &str = "http://logic.test/world/magic-generic";
    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const P1: &str = "http://ex/p1";
    const P2: &str = "http://ex/p2";

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
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let prog = subprop_program();
        let answer = decided(
            super::super::magic::resolve_native(&foreign, &world, &prog, &Budget::default())
                .unwrap(),
        );
        assert!(answer.bindings.is_empty(), "no <p1> edge ⇒ no answers");
    }

    // ── (c) Provenance: the derived answer carries its demand antecedent ─────────

    #[test]
    fn generic_derived_row_carries_demand_and_rule_provenance() {
        let (store, world) = make_world(&[("http://ex/x", P1, "http://ex/y")]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
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
        let result = materialize_generic(&facts, &transformed.rules).unwrap();

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
}
