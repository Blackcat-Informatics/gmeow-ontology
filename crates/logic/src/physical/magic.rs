// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Magic-sets (demand) transformation + the backward `resolve_native` evaluator.
//!
//! # Why magic-sets
//!
//! The forward semi-naive core ([`crate::physical::seminaive`]) answers a query by
//! materializing the WHOLE least model and reading the goal predicate out of it.  For a
//! *backward* query `?- g(t0, t1)` that is wasteful: a top-down SLD resolver (the
//! reference oracle [`crate::reference_resolver::resolve`]) only ever explores the part
//! of the model reachable from the goal's bound arguments.  The **magic-sets** (a.k.a.
//! *demand*) transformation rewrites the program so that the SAME bottom-up engine
//! computes exactly that demand-restricted slice: a *magic* predicate per adorned IDB
//! atom carries the set of bound argument values that the top-down search would have
//! propagated, and a guard atom in front of each rule body restricts derivation to those
//! demanded instances.  Bottom-up evaluation of the transformed program then yields the
//! same goal answers as top-down SLD — which is the parity gate of this module.
//!
//! # Binary fragment, binary magic encoding
//!
//! The gmeow query fragment is binary (`pred(subject, object)`); the engine's
//! [`crate::physical::store::RelationStore`] only stores binary relations.  So the magic
//! predicates are themselves encoded as binary atoms:
//!
//! - adornment `bf`/`fb` (exactly one bound arg) → a self-loop `magic_p_<adorn>(v, v)`
//!   carrying the single bound value `v`.
//! - adornment `bb` (both bound) → `magic_p_bb(s, o)` carrying both values.
//! - adornment `ff` (none bound) → NO magic guard: the predicate is demanded unrestricted
//!   (every instance), so no guard atom is emitted for an `ff` occurrence.
//!
//! The magic-predicate IRIs are minted deterministically from the original predicate IRI
//! (`<base>magic/<localname>_<adorn>`), stable across runs.
//!
//! # The transformation (standard magic-sets, left-to-right SIPS)
//!
//! For a goal `g(t0, t1)` with adornment `a` (over `{b, f}`):
//!
//! 1. **Seed** — a bodyless rule deriving the goal's magic fact carrying the goal's bound
//!    constant(s) (or, for `ff`, no seed at all — `ff` is unrestricted).
//! 2. **Modified rules** — each original rule `h :- b1..bn` becomes, for the head
//!    adornment `a_h`, `h :- magic_h^{a_h}, b1, ..., bn` (the guard prepended; an `ff`
//!    head emits no guard).  Each IDB body atom is adorned per a left-to-right SIPS (a
//!    body-atom argument is *bound* iff it is a head-bound argument or was bound by an
//!    earlier body atom).
//! 3. **Magic rules** — for each adorned IDB body atom `bi^{a_i}`, a rule deriving its
//!    magic fact from the head's magic guard plus the preceding body atoms (the SIPS
//!    chain): `magic_bi^{a_i} :- magic_h^{a_h}, b1, ..., b(i-1)` (an `ff` body atom adds
//!    no magic rule — it demands nothing).
//!
//! The positive query corpus introduces no negation, so the transformed program is
//! stratifiable; a transform that WOULD break stratification surfaces as
//! [`crate::physical::seminaive::UnsupportedKind::DemandBreaksStratification`].

use std::collections::{BTreeMap, BTreeSet};

use oxigraph::model::NamedNode;

use crate::physical::seminaive::{evaluate, NativeOutcome, UnsupportedKind};
use crate::physical::store::extract_edb;
use crate::profile_gate;
use crate::query_ir::{AnswerSet, Binding, Budget, QAtom, QBodyLit, QProgram, QTerm};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};
use crate::seam::{BudgetStatus, ScryerForeign};

// ── Adornment ────────────────────────────────────────────────────────────────────

/// A two-position adornment over `{b, f}` for a binary atom: `b` = bound, `f` = free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Adorn {
    /// Subject position bound?
    subj_bound: bool,
    /// Object position bound?
    obj_bound: bool,
}

impl Adorn {
    /// The two-char adornment string (`"bb"`, `"bf"`, `"fb"`, `"ff"`).
    fn code(self) -> &'static str {
        match (self.subj_bound, self.obj_bound) {
            (true, true) => "bb",
            (true, false) => "bf",
            (false, true) => "fb",
            (false, false) => "ff",
        }
    }

    /// `true` if neither position is bound (no demand restriction — `ff`).
    fn is_free(self) -> bool {
        !self.subj_bound && !self.obj_bound
    }
}

// ── IR conversion (QProgram → EvalRule, binary fragment) ──────────────────────────

/// Convert one `QTerm` to an [`EvalTerm`], or report the gap.
///
/// A `Const("<iri>")` → [`EvalTerm::ConstNamed`] (angle brackets stripped); a `Var(v)` →
/// `EvalTerm::Var("?v")` (the engine's variable surface carries a leading `?`, matching
/// `parse_eval_rules`); a `Num` is an arithmetic operand the native core does not carry.
fn term_of(t: &QTerm) -> Result<EvalTerm, NativeOutcome<AnswerSet>> {
    match t {
        QTerm::Const(c) => {
            let iri = c
                .strip_prefix('<')
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(c);
            match NamedNode::new(iri) {
                Ok(nn) => Ok(EvalTerm::ConstNamed(nn)),
                // A non-IRI constant (e.g. a literal) is outside the binary IDB fragment
                // the magic transform targets; report it as a non-binary/unsupported gap
                // rather than papering over it.
                Err(_) => Err(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom)),
            }
        }
        QTerm::Var(v) => Ok(EvalTerm::Var(format!("?{v}"))),
        QTerm::Num(_) => Err(NativeOutcome::Unsupported(UnsupportedKind::Arithmetic)),
    }
}

/// Convert one binary `QAtom` to an [`EvalAtom`] (predicate angle brackets already absent
/// in `QAtom::pred`), or report the gap.
fn atom_of(atom: &QAtom) -> Result<EvalAtom, NativeOutcome<AnswerSet>> {
    if atom.args.len() != 2 {
        return Err(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom));
    }
    let predicate = NamedNode::new(&atom.pred)
        .map_err(|_| NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom))?;
    let subject = term_of(&atom.args[0])?;
    let object = term_of(&atom.args[1])?;
    Ok(EvalAtom {
        subject,
        predicate,
        object,
        negated: false,
    })
}

// ── Magic-predicate minting ───────────────────────────────────────────────────────

/// Mint the deterministic magic-predicate IRI for `pred` under adornment `adorn`.
///
/// Derived from the original predicate IRI: the base (everything up to and including the
/// last `/` or `#`) plus `magic/<localname>_<adorn>`.  Stable across runs.
fn magic_pred_iri(pred: &str, adorn: &str) -> NamedNode {
    let split = pred.rfind(['/', '#']).map_or(pred.len(), |i| i + 1);
    let (base, local) = pred.split_at(split);
    // `base` ends with the separator; nest the magic predicates under `magic/` so they
    // never collide with a real predicate in the source namespace.
    NamedNode::new(format!("{base}magic/{local}_{adorn}"))
        .expect("magic predicate IRI is well-formed (derived from a valid predicate IRI)")
}

/// Build a magic *guard* atom (a body literal) for an adorned IDB atom.
///
/// `bb` carries `(subject, object)`; `bf`/`fb` carry the single bound term as a self-loop
/// `magic(v, v)`.  `ff` has no guard (returns `None`).
fn magic_guard_atom(atom: &EvalAtom, adorn: Adorn) -> Option<EvalAtom> {
    if adorn.is_free() {
        return None;
    }
    let pred = magic_pred_iri(atom.predicate.as_str(), adorn.code());
    let (subject, object) = match (adorn.subj_bound, adorn.obj_bound) {
        (true, true) => (atom.subject.clone(), atom.object.clone()),
        // self-loop: carry the single bound term in both slots.
        (true, false) => (atom.subject.clone(), atom.subject.clone()),
        (false, true) => (atom.object.clone(), atom.object.clone()),
        (false, false) => unreachable!("ff handled above"),
    };
    Some(EvalAtom {
        subject,
        predicate: pred,
        object,
        negated: false,
    })
}

/// Build a magic *seed* fact atom carrying the goal's bound constants for `goal_atom`.
///
/// Same binary encoding as [`magic_guard_atom`]; returns `None` for an `ff` goal (no seed
/// — the predicate is demanded unrestricted).
fn magic_seed_atom(goal_atom: &EvalAtom, adorn: Adorn) -> Option<EvalAtom> {
    magic_guard_atom(goal_atom, adorn)
}

// ── SIPS adornment of an IDB body atom ────────────────────────────────────────────

/// The variable name of an [`EvalTerm::Var`], or `None` for a constant.
fn var_name(t: &EvalTerm) -> Option<&str> {
    match t {
        EvalTerm::Var(v) => Some(v.as_str()),
        EvalTerm::ConstNamed(_) | EvalTerm::ConstLit(_) => None,
    }
}

/// Adorn a body atom under a left-to-right SIPS, given the set of currently-bound
/// variable names: a position is bound iff it is a constant or a bound variable.
fn adorn_atom(atom: &EvalAtom, bound: &BTreeSet<String>) -> Adorn {
    let pos_bound = |t: &EvalTerm| match var_name(t) {
        Some(v) => bound.contains(v),
        None => true, // a constant is always bound
    };
    Adorn {
        subj_bound: pos_bound(&atom.subject),
        obj_bound: pos_bound(&atom.object),
    }
}

/// Add an atom's variable names to the bound set (used to thread SIPS bindings).
fn bind_atom_vars(atom: &EvalAtom, bound: &mut BTreeSet<String>) {
    if let Some(v) = var_name(&atom.subject) {
        bound.insert(v.to_owned());
    }
    if let Some(v) = var_name(&atom.object) {
        bound.insert(v.to_owned());
    }
}

/// The bound-variable set induced by the head's adornment (the head-bound arguments).
fn head_bound_vars(head: &EvalAtom, adorn: Adorn) -> BTreeSet<String> {
    let mut bound = BTreeSet::new();
    if adorn.subj_bound {
        if let Some(v) = var_name(&head.subject) {
            bound.insert(v.to_owned());
        }
    }
    if adorn.obj_bound {
        if let Some(v) = var_name(&head.object) {
            bound.insert(v.to_owned());
        }
    }
    bound
}

// ── The magic-sets transformation ─────────────────────────────────────────────────

/// An `EvalRule` with a synthesized rule IRI from the head predicate and a discriminator.
fn rule(head: EvalAtom, body: Vec<EvalAtom>, rule_iri: String) -> EvalRule {
    EvalRule {
        head,
        body,
        rule_iri,
        distinct_pairs: vec![],
    }
}

/// The output of the magic-sets transformation: the transformed binary program (modified
/// rules + magic rules) plus the ground seed fact (the goal's magic fact, inserted into
/// the EDB before evaluation).
///
/// The seed is inserted into the EDB rather than emitted as a bodyless rule because the
/// semi-naive engine never fires a zero-positive-body rule (a bodyless rule produces no
/// solution in a delta round); a magic seed is an asserted demand fact, so it belongs in
/// the EDB seed.  An `ff` goal carries no seed (`None` — the predicate is unrestricted).
struct MagicProgram {
    /// The transformed rules (modified original rules + magic rules).
    rules: Vec<EvalRule>,
    /// The goal's ground magic seed fact `(predicate, subject, object)`, or `None` for
    /// an `ff` goal (no demand restriction).
    seed: Option<EvalAtom>,
}

/// Magic-transform `rules` w.r.t. the goal atom `goal` and its `goal_adorn`.
///
/// Returns the transformed binary program (modified rules + magic rules) plus the ground
/// seed fact.  The IDB predicate set is the set of original rule-head predicates; only IDB
/// body atoms are adorned/guarded (an EDB body atom propagates SIPS bindings but carries
/// no magic).
fn magic_transform(rules: &[EvalRule], goal: &EvalAtom, goal_adorn: Adorn) -> MagicProgram {
    let idb: BTreeSet<String> = rules
        .iter()
        .map(|r| r.head.predicate.as_str().to_owned())
        .collect();

    let mut out: Vec<EvalRule> = Vec::new();

    // (1) Seed: the goal's magic fact (none for an ff goal). Inserted into the EDB by the
    // caller — a bodyless rule never fires in the semi-naive engine.
    let seed = magic_seed_atom(goal, goal_adorn);

    // The query corpus has at most one adornment per IDB predicate reachable from a
    // single goal (the goal binds the head pattern), so we adorn each rule by its head's
    // adornment derived from the goal demand.  For a predicate reached only as the goal,
    // the head adornment is the goal adornment; for an IDB predicate reached via a body
    // atom, its adornment is computed by the SIPS at that occurrence.  We compute the set
    // of (head_pred, adornment) demands by a fixpoint over the magic rules so every
    // reachable adorned IDB predicate gets its modified + magic rules.
    let mut demands: BTreeSet<(String, &'static str)> = BTreeSet::new();
    demands.insert((goal.predicate.as_str().to_owned(), goal_adorn.code()));

    // Fixpoint: expanding a demand (pred, adorn) over every rule whose head is `pred`
    // discovers the adorned IDB body atoms it demands.
    let mut frontier: Vec<(String, Adorn)> = vec![(goal.predicate.as_str().to_owned(), goal_adorn)];

    while let Some((head_pred, head_adorn)) = frontier.pop() {
        for r in rules
            .iter()
            .filter(|r| r.head.predicate.as_str() == head_pred)
        {
            // SIPS: bound vars start from the head-bound arguments.
            let mut bound = head_bound_vars(&r.head, head_adorn);
            for atom in &r.body {
                if idb.contains(atom.predicate.as_str()) {
                    let a = adorn_atom(atom, &bound);
                    let demand = (atom.predicate.as_str().to_owned(), a.code());
                    // `demands` doubles as the visited-set: insert returns true only the
                    // first time a demand is seen, so each frontier node expands once.
                    if demands.insert(demand) {
                        frontier.push((atom.predicate.as_str().to_owned(), a));
                    }
                }
                // Thread this atom's bindings for the next atom (SIPS).
                bind_atom_vars(atom, &mut bound);
            }
        }
    }

    // (2) Modified rules + (3) magic rules, for every demanded (head_pred, adorn).
    for (head_pred, adorn_code) in &demands {
        let head_adorn = adorn_from_code(adorn_code);
        for (ri, r) in rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.head.predicate.as_str() == head_pred.as_str())
        {
            let mut bound = head_bound_vars(&r.head, head_adorn);

            // (2) Modified rule body: head magic guard (if any) ++ original body.
            let mut mod_body: Vec<EvalAtom> = Vec::new();
            if let Some(guard) = magic_guard_atom(&r.head, head_adorn) {
                mod_body.push(guard);
            }

            // Walk the body, emitting per-IDB-atom magic rules along the SIPS chain.
            let head_guard = magic_guard_atom(&r.head, head_adorn);
            let mut prefix: Vec<EvalAtom> = Vec::new();
            for (bi, atom) in r.body.iter().enumerate() {
                if idb.contains(atom.predicate.as_str()) {
                    let a = adorn_atom(atom, &bound);
                    // (3) magic rule: magic_bi :- magic_head, b1..b(i-1)  (none for ff).
                    if let Some(magic_head) = magic_guard_atom(atom, a) {
                        let mut mbody: Vec<EvalAtom> = Vec::new();
                        if let Some(hg) = &head_guard {
                            mbody.push(hg.clone());
                        }
                        mbody.extend(prefix.iter().cloned());
                        let iri = format!(
                            "{}::magic/{}/{}#{ri}.{bi}",
                            atom.predicate.as_str(),
                            a.code(),
                            head_pred
                        );
                        out.push(rule(magic_head, mbody, iri));
                    }
                }
                // The modified rule keeps the ORIGINAL body atom (positive, unguarded);
                // the demand restriction comes from the head guard + the magic rules that
                // gate which instances are derived.
                mod_body.push(atom.clone());
                prefix.push(atom.clone());
                bind_atom_vars(atom, &mut bound);
            }

            let iri = format!("{}::mod/{}#{ri}", r.head.predicate.as_str(), adorn_code);
            out.push(rule(r.head.clone(), mod_body, iri));
        }
    }

    MagicProgram { rules: out, seed }
}

/// Convert a ground magic seed [`EvalAtom`] into a [`crate::rule_ir::Fact`] for EDB
/// insertion.  The seed is always ground (its terms are goal constants), so this never
/// hits an unbound variable.
fn seed_to_fact(seed: &EvalAtom) -> Result<crate::rule_ir::Fact, String> {
    let to_term = |t: &EvalTerm| match t {
        EvalTerm::ConstNamed(nn) => Ok(oxigraph::model::Term::NamedNode(nn.clone())),
        EvalTerm::ConstLit(term) => Ok(term.clone()),
        EvalTerm::Var(v) => Err(format!("magic seed term {v:?} is not ground")),
    };
    Ok(crate::rule_ir::Fact {
        subject: to_term(&seed.subject)?,
        predicate: seed.predicate.clone(),
        object: to_term(&seed.object)?,
    })
}

/// Reconstruct an [`Adorn`] from its two-char code (`"bb"`, `"bf"`, `"fb"`, `"ff"`).
fn adorn_from_code(code: &str) -> Adorn {
    let bytes = code.as_bytes();
    Adorn {
        subj_bound: bytes[0] == b'b',
        obj_bound: bytes[1] == b'b',
    }
}

// ── Backward entry: resolve_native ────────────────────────────────────────────────

/// Compute the goal atom's adornment from its `(subject, object)` terms.
fn goal_adornment(goal: &QAtom) -> Adorn {
    let bound = |t: &QTerm| matches!(t, QTerm::Const(_) | QTerm::Num(_));
    Adorn {
        subj_bound: bound(&goal.args[0]),
        obj_bound: bound(&goal.args[1]),
    }
}

/// Project the goal predicate's derived tuples into [`AnswerSet`] bindings, exactly as
/// [`crate::reference_resolver::resolve`] does: one binding per goal variable, mapping it
/// to the matched constant's canonical `<iri>` surface.
///
/// A goal atom `g(t0, t1)` selects every derived `g`-fact whose constant positions match;
/// each surviving fact yields a binding of the goal's variable position(s).  When the goal
/// has no variables (a `bb` ground goal) a single empty binding is produced iff the fact
/// is present — the "yes" answer, matching the oracle's empty-binding row.
fn project_answers(facts: &[crate::rule_ir::Fact], goal: &QAtom, goal_pred: &str) -> Vec<Binding> {
    // The goal's constant constraints (by position) and variable names (by position).
    let want_const = |t: &QTerm| match t {
        QTerm::Const(c) => Some(c.clone()),
        QTerm::Var(_) | QTerm::Num(_) => None,
    };
    let s_const = want_const(&goal.args[0]);
    let o_const = want_const(&goal.args[1]);
    let s_var = match &goal.args[0] {
        QTerm::Var(v) => Some(v.clone()),
        _ => None,
    };
    let o_var = match &goal.args[1] {
        QTerm::Var(v) => Some(v.clone()),
        _ => None,
    };

    let mut bindings: Vec<Binding> = Vec::new();
    for f in facts {
        if f.predicate.as_str() != goal_pred {
            continue;
        }
        let s_surface = f.subject.to_string();
        let o_surface = f.object.to_string();
        // Apply the goal's constant constraints.
        if let Some(c) = &s_const {
            if &s_surface != c {
                continue;
            }
        }
        if let Some(c) = &o_const {
            if &o_surface != c {
                continue;
            }
        }
        // Build the binding for this fact's goal variables. If the goal repeats a
        // variable across both positions, the two surfaces must agree.
        let mut binding: Binding = BTreeMap::new();
        if let Some(v) = &s_var {
            binding.insert(v.clone(), s_surface.clone());
        }
        if let Some(v) = &o_var {
            if let Some(existing) = binding.get(v) {
                if existing != &o_surface {
                    continue; // repeated-var disagreement
                }
            } else {
                binding.insert(v.clone(), o_surface.clone());
            }
        }
        bindings.push(binding);
    }
    bindings
}

/// Resolve `program` against `world` via the native bottom-up engine over a
/// magic-transformed program — the backward leg of the native execution core.
///
/// Parity sibling of [`crate::reference_resolver::resolve`]: the returned [`AnswerSet`]
/// (after `canonicalize`) carries the SAME goal-variable bindings and status as the
/// top-down oracle for the binary positive corpus.  A cut / arithmetic / non-binary input
/// is a declared gap ([`NativeOutcome::Unsupported`]); the caller routes such requests to
/// an oracle (no-optionality).
///
/// # Errors
///
/// Returns `Err` for an evaluator failure (e.g. an unbound head variable or a
/// provenance-recipe failure) propagated from the shared engine helpers.
pub(crate) fn resolve_native(
    foreign: &dyn ScryerForeign,
    world: &NamedNode,
    program: &QProgram,
    budget: &Budget,
) -> Result<NativeOutcome<AnswerSet>, String> {
    // (0) Gate cut / arithmetic (reuse the structural detectors the dispatch gate uses).
    if profile_gate::has_cut(program) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut));
    }
    if profile_gate::has_builtin(program) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::Arithmetic));
    }

    // The corpus goal is a single binary atom; the native backward leg handles exactly
    // that.  A multi-atom conjunctive goal (or a non-binary goal atom) is a declared gap.
    if program.goal.atoms.len() != 1 {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom));
    }
    let goal = &program.goal.atoms[0];
    if goal.args.len() != 2 {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom));
    }

    // (1) Convert program rules → binary EvalRules.
    let mut rules: Vec<EvalRule> = Vec::with_capacity(program.rules.len());
    for r in &program.rules {
        for lit in &r.body {
            match lit {
                QBodyLit::Atom(_) => {}
                QBodyLit::Cut => return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut)),
                QBodyLit::Builtin(_) => {
                    return Ok(NativeOutcome::Unsupported(UnsupportedKind::Arithmetic))
                }
            }
        }
        let head = match atom_of(&r.head) {
            Ok(a) => a,
            Err(gap) => return Ok(gap),
        };
        let mut body: Vec<EvalAtom> = Vec::new();
        for lit in &r.body {
            if let QBodyLit::Atom(a) = lit {
                match atom_of(a) {
                    Ok(ea) => body.push(ea),
                    Err(gap) => return Ok(gap),
                }
            }
        }
        // A synthesized stable rule IRI for the modified/original rule.
        let rule_iri = format!("{}::rule", head.predicate.as_str());
        rules.push(EvalRule {
            head,
            body,
            rule_iri,
            distinct_pairs: vec![],
        });
    }

    // (2) Compute the goal adornment and magic-transform.
    let goal_atom = match atom_of(goal) {
        Ok(a) => a,
        Err(gap) => return Ok(gap),
    };
    let adorn = goal_adornment(goal);
    let transformed = magic_transform(&rules, &goal_atom, adorn);

    // (3) Extract the world EDB columnar-form, seed the goal's magic fact, and run the
    //     bottom-up fixpoint.  The seed is an asserted demand fact, so it goes into the
    //     EDB seed (a bodyless rule never fires in the semi-naive engine).
    let mut edb = extract_edb(foreign, world);
    if let Some(seed) = &transformed.seed {
        let fact = seed_to_fact(seed)?;
        edb.insert(&fact.predicate, fact.subject, fact.object);
    }
    let facts = match evaluate(edb, &transformed.rules)? {
        NativeOutcome::Decided(facts) => facts,
        NativeOutcome::Unsupported(kind) => {
            // A demand transform that breaks stratification is the documented gap kind;
            // surface any non-stratifiable transform under that name.
            let kind = match kind {
                UnsupportedKind::NonStratifiable => UnsupportedKind::DemandBreaksStratification,
                other => other,
            };
            return Ok(NativeOutcome::Unsupported(kind));
        }
    };

    // (4) Project the goal predicate's derived tuples into bindings.
    let mut bindings = project_answers(&facts, goal, goal_atom.predicate.as_str());

    // (5) Budget semantics — max_answers truncation only.  The native engine runs to
    //     fixpoint; it has no step governor.  Step-budget queries (max_steps.is_some())
    //     are demoted at the dispatch layer before reaching here, so only max_answers
    //     applies at this point.
    let mut status = BudgetStatus::Ok;
    if let Some(max_a) = budget.max_answers {
        // Deterministic truncation: canonicalize first so the kept prefix is stable.
        let mut tmp = AnswerSet {
            bindings: bindings.clone(),
            status: BudgetStatus::Ok,
            preservation: crate::result::PreservationClaim::exact(),
        };
        tmp.canonicalize();
        if tmp.bindings.len() > max_a {
            tmp.bindings.truncate(max_a);
            status = BudgetStatus::Partial;
        } else if tmp.bindings.len() == max_a && !tmp.bindings.is_empty() {
            // The oracle stamps Partial the moment it reaches exactly max_answers.
            status = BudgetStatus::Partial;
        }
        bindings = tmp.bindings;
    }

    let mut answer = AnswerSet {
        bindings,
        status,
        preservation: crate::result::PreservationClaim::exact(),
    };
    answer.canonicalize();
    Ok(NativeOutcome::Decided(answer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::parse_query_program;
    use crate::reference_resolver;
    use crate::seam::WorldStoreForeign;
    use crate::store::WorldStore;

    const W: &str = "http://logic.test/world/magic";
    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const BASE: &str = "https://example.org/";

    fn make_world(triples: &[(&str, &str, &str)]) -> (WorldStore, NamedNode) {
        let store = WorldStore::new();
        for (s, p, o) in triples {
            store.insert_quad(W, s, p, o);
        }
        (store, NamedNode::new(W).unwrap())
    }

    fn decided(outcome: NativeOutcome<AnswerSet>) -> AnswerSet {
        match outcome {
            NativeOutcome::Decided(a) => a,
            NativeOutcome::Unsupported(k) => panic!("expected Decided, got Unsupported({k:?})"),
        }
    }

    // ── Test 1: non-recursive single-rule parity ─────────────────────────────────

    #[test]
    fn magic_non_recursive_matches_reference() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}alice"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}bob"),
        )]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestorOf(ex:alice, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

        assert_eq!(native.status, reference.status, "status parity");
        assert_eq!(
            native.bindings, reference.bindings,
            "bottom-up magic-sets must equal top-down SLD: native {native:?} vs ref {reference:?}"
        );
        assert_eq!(native.bindings.len(), 1);
        assert_eq!(native.bindings[0]["Y"], format!("<{BASE}bob>"));
    }

    // ── Test 2: recursive transitive-closure parity (bottom-up == top-down) ──────

    fn tc_program() -> QProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        parse_query_program(&src).unwrap()
    }

    fn tc_world() -> (WorldStore, NamedNode) {
        make_world(&[
            (
                &format!("{BASE}a"),
                &format!("{BASE}parentOf"),
                &format!("{BASE}b"),
            ),
            (
                &format!("{BASE}b"),
                &format!("{BASE}parentOf"),
                &format!("{BASE}c"),
            ),
            (
                &format!("{BASE}c"),
                &format!("{BASE}parentOf"),
                &format!("{BASE}d"),
            ),
        ])
    }

    #[test]
    fn magic_recursive_transitive_closure_matches_reference() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let prog = tc_program();
        let budget = Budget::default();

        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

        assert_eq!(native.status, reference.status, "status parity");
        assert_eq!(
            native.bindings, reference.bindings,
            "bottom-up magic-sets == top-down SLD on recursion: native {native:?} vs ref {reference:?}"
        );
        let ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert!(
            ys.contains(&format!("<{BASE}b>").as_str()),
            "missing b: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{BASE}c>").as_str()),
            "missing c: {ys:?}"
        );
        assert!(
            ys.contains(&format!("<{BASE}d>").as_str()),
            "missing d: {ys:?}"
        );
        assert_eq!(native.bindings.len(), 3);
    }

    // ── Test 3a: bb (both-bound) ground goal parity ──────────────────────────────

    #[test]
    fn magic_bb_ground_goal_matches_reference() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        // ?- ancestor(a, c)  → present (a→b→c); one "yes" (empty-binding) answer.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, ex:c).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

        assert_eq!(native.status, reference.status);
        assert_eq!(
            native.bindings, reference.bindings,
            "bb parity: native {native:?} vs ref {reference:?}"
        );
        // Present → exactly one empty-binding "yes" answer.
        assert_eq!(native.bindings.len(), 1);
        assert!(
            native.bindings[0].is_empty(),
            "ground yes is an empty binding"
        );
    }

    #[test]
    fn magic_bb_ground_goal_absent_matches_reference() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        // ?- ancestor(d, a)  → absent; zero answers.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:d, ex:a).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();
        assert_eq!(native.bindings, reference.bindings);
        assert!(
            native.bindings.is_empty(),
            "absent ground goal has no answers"
        );
    }

    // ── Test 3b: fb (object-bound) goal parity ───────────────────────────────────

    #[test]
    fn magic_fb_object_bound_goal_matches_reference() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        // ?- ancestor(X, d)  → X ∈ {a, b, c}.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(X, ex:d).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

        assert_eq!(native.status, reference.status);
        assert_eq!(
            native.bindings, reference.bindings,
            "fb parity: native {native:?} vs ref {reference:?}"
        );
        let xs: Vec<&str> = native.bindings.iter().map(|b| b["X"].as_str()).collect();
        assert!(
            xs.contains(&format!("<{BASE}a>").as_str()),
            "missing a: {xs:?}"
        );
        assert!(
            xs.contains(&format!("<{BASE}b>").as_str()),
            "missing b: {xs:?}"
        );
        assert!(
            xs.contains(&format!("<{BASE}c>").as_str()),
            "missing c: {xs:?}"
        );
        assert_eq!(native.bindings.len(), 3);
    }

    // ── Test 3c: ff (fully-free) goal parity ─────────────────────────────────────

    #[test]
    fn magic_ff_free_goal_matches_reference() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        // ?- ancestor(X, Y)  → the full transitive closure of a→b→c→d.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(X, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget::default();

        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();

        assert_eq!(native.status, reference.status);
        assert_eq!(
            native.bindings, reference.bindings,
            "ff parity: native {native:?} vs ref {reference:?}"
        );
        // Closure of a→b→c→d: 6 pairs.
        assert_eq!(native.bindings.len(), 6);
    }

    // ── Test 4: demand pruning evidence ──────────────────────────────────────────

    #[test]
    fn magic_bf_demand_does_not_evaluate_unrelated_starts() {
        // Two disjoint chains: a→b→c and p→q→r.  A bf goal ?- ancestor(a, Y) must
        // demand-restrict to the `a` chain; the magic transform seeds the demand only for
        // `a`, so the derived `ancestor` facts cover only {a→b, a→c}, never the p chain.
        let (store, world_nn) = make_world(&[
            (
                &format!("{BASE}a"),
                &format!("{BASE}parentOf"),
                &format!("{BASE}b"),
            ),
            (
                &format!("{BASE}b"),
                &format!("{BASE}parentOf"),
                &format!("{BASE}c"),
            ),
            (
                &format!("{BASE}p"),
                &format!("{BASE}parentOf"),
                &format!("{BASE}q"),
            ),
            (
                &format!("{BASE}q"),
                &format!("{BASE}parentOf"),
                &format!("{BASE}r"),
            ),
        ]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        // Build the transformed program and evaluate it directly to inspect the demanded
        // ancestor facts (the demand restriction is what we are asserting).
        let mut rules: Vec<EvalRule> = Vec::new();
        for r in &prog.rules {
            let head = atom_of(&r.head).unwrap();
            let body: Vec<EvalAtom> = r
                .body
                .iter()
                .filter_map(|l| match l {
                    QBodyLit::Atom(a) => Some(atom_of(a).unwrap()),
                    _ => None,
                })
                .collect();
            rules.push(EvalRule {
                head,
                body,
                rule_iri: format!("{}::rule", atom_of(&r.head).unwrap().predicate.as_str()),
                distinct_pairs: vec![],
            });
        }
        let goal = &prog.goal.atoms[0];
        let goal_atom = atom_of(goal).unwrap();
        let transformed = magic_transform(&rules, &goal_atom, goal_adornment(goal));
        let mut edb = extract_edb(&foreign, &world_nn);
        if let Some(seed) = &transformed.seed {
            let f = seed_to_fact(seed).unwrap();
            edb.insert(&f.predicate, f.subject, f.object);
        }
        let facts = match evaluate(edb, &transformed.rules).unwrap() {
            NativeOutcome::Decided(f) => f,
            other => panic!("expected Decided, got {other:?}"),
        };

        let anc = format!("{BASE}ancestor");
        let derived_anc: BTreeSet<(String, String)> = facts
            .iter()
            .filter(|f| f.predicate.as_str() == anc)
            .map(|f| (f.subject.to_string(), f.object.to_string()))
            .collect();
        // The bf demand seeds the goal `ancestor(a, _)`; the SIPS propagates the demand
        // forward only along edges reachable from `a` (`a` then its successor `b`), so the
        // derived `ancestor` facts are exactly the reachable closure rooted in the
        // a-chain: {(a,b),(b,c),(a,c)}.  Crucially the DISJOINT p→q→r chain is NEVER
        // demanded — no `p`/`q`-rooted ancestor fact is derived, even though p→q→r is in
        // the EDB.  A non-demand (full) evaluation would additionally derive
        // {(p,q),(q,r),(p,r)}; their absence is the demand-pruning evidence.
        let want: BTreeSet<(String, String)> = [("a", "b"), ("b", "c"), ("a", "c")]
            .into_iter()
            .map(|(s, o)| (format!("<{BASE}{s}>"), format!("<{BASE}{o}>")))
            .collect();
        assert_eq!(
            derived_anc, want,
            "bf demand must derive exactly the a-rooted reachable closure, pruning the \
             disjoint p-chain: {derived_anc:?}"
        );
        // Explicit pruning witnesses: none of the p-chain ancestor facts appear.
        for (s, o) in [("p", "q"), ("q", "r"), ("p", "r")] {
            assert!(
                !derived_anc.contains(&(format!("<{BASE}{s}>"), format!("<{BASE}{o}>"))),
                "demand must prune the unrelated p-chain fact ancestor({s},{o})"
            );
        }
    }

    // ── Test 5: cut / arithmetic / non-binary unsupported ────────────────────────

    #[test]
    fn magic_cut_is_unsupported() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}a"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}b"),
        )]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y), !, ex:ancestor(X, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
        assert!(
            matches!(outcome, NativeOutcome::Unsupported(UnsupportedKind::Cut)),
            "cut must be Unsupported(Cut): {outcome:?}"
        );
    }

    #[test]
    fn magic_arithmetic_is_unsupported() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}l0"),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
        )]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, 'http://www.w3.org/1999/02/22-rdf-syntax-ns#').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
        assert!(
            matches!(
                outcome,
                NativeOutcome::Unsupported(UnsupportedKind::Arithmetic)
            ),
            "arithmetic builtin must be Unsupported(Arithmetic): {outcome:?}"
        );
    }

    #[test]
    fn magic_non_binary_goal_is_unsupported() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}a"),
            &format!("{BASE}parentOf"),
            &format!("{BASE}b"),
        )]);
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        // get/3 is a non-binary (ternary) IDB atom — outside the binary native fragment.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:get(L, N, X) :- ex:parentOf(L, X).\n\
             ?- ex:get(ex:a, ex:b, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
        assert!(
            matches!(
                outcome,
                NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom)
            ),
            "non-binary goal atom must be Unsupported(NonBinaryAtom): {outcome:?}"
        );
    }

    // ── Budget: max_answers truncation parity ────────────────────────────────────

    #[test]
    fn magic_budget_max_answers_matches_reference() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let prog = tc_program();
        let budget = Budget {
            max_answers: Some(1),
            ..Default::default()
        };
        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        let reference = reference_resolver::resolve(&foreign, &world_nn, &prog, &budget).unwrap();
        assert_eq!(native.bindings.len(), 1, "capped at 1 answer");
        assert_eq!(native.status, BudgetStatus::Partial, "cap → Partial");
        assert_eq!(
            native.status, reference.status,
            "status parity under budget"
        );
    }
}
