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
//! always stratifiable.  A transform that WOULD break stratification (only possible
//! once native rules carry negation) falls back to a full stratified evaluation of
//! the UNTRANSFORMED base program — correct, without the demand pruning — rather than
//! demoting to an external engine (no-optionality: the native core stays authoritative).

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use crate::physical::binding_pattern::BindingPattern;
use crate::physical::seminaive::{NativeOutcome, UnsupportedKind, evaluate};
use crate::physical::store::{RelationStore, extract_edb};
use crate::profile_gate;
use crate::provenance::term_display;
use crate::query_ir::{
    AnswerSet, Binding, Budget, CompletionFrontier, QAtom, QBodyLit, QBuiltin, QProgram, QTerm,
};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact};
use crate::seam::{BudgetStatus, ScryerForeign};

// ── Adornment ────────────────────────────────────────────────────────────────────
//
// The adornment lattice is the arity-generic [`BindingPattern`] (a bitset over
// argument positions), shared with the forward generic evaluator. Its `code()` is the
// per-position `{b, f}` string; at arity 2 it is exactly the legacy `"bb"`/`"bf"`/
// `"fb"`/`"ff"` an `Adorn{subj_bound, obj_bound}` produced, so the minted magic
// predicate IRIs are byte-identical (the binary parity gate).

// ── IR conversion (QProgram → EvalRule, binary fragment) ──────────────────────────

/// Convert one `QTerm` to an [`EvalTerm`], or report the gap.
///
/// A `Const("<iri>")` → [`EvalTerm::ConstNamed`] (angle brackets stripped); a `Var(v)` →
/// `EvalTerm::Var("?v")` (the engine's variable surface carries a leading `?`, matching
/// `parse_eval_rules`); a `Num` is an arithmetic operand the native core does not carry.
fn term_of(t: &QTerm) -> Result<EvalTerm, UnsupportedKind> {
    match t {
        QTerm::Const(c) => {
            let iri = c
                .strip_prefix('<')
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(c);
            // The seam predicate is already a validated IRI string; carry it directly.
            Ok(EvalTerm::ConstNamed(iri.to_owned()))
        }
        QTerm::Var(v) => Ok(EvalTerm::Var(format!("?{v}"))),
        // An integer constant in an atom argument (e.g. the `0` in `len(nil, 0)` or a
        // list index) lowers to the canonical typed-integer literal — byte-identical
        // to a computed arithmetic answer's surface, so a fact-carried constant and a
        // builtin-generated value unify.
        QTerm::Num(n) => Ok(EvalTerm::ConstLit(TermValue::typed_literal(
            n.to_string(),
            crate::physical::XSD_INTEGER,
        ))),
    }
}

/// Rewrite a builtin operand's variable to the engine's `?`-prefixed surface,
/// matching the [`EvalTerm::Var`] keys the body atoms carry (constants unchanged).
fn prefix_builtin_term(t: &QTerm) -> QTerm {
    match t {
        QTerm::Var(v) => QTerm::Var(format!("?{v}")),
        QTerm::Const(_) | QTerm::Num(_) => t.clone(),
    }
}

/// Lower a `QBuiltin` into the engine surface: every variable operand `?`-prefixed
/// so the seminaive constraint stage's `lookup` resolves it against the solution
/// bindings.  The shared evaluator is namespace-neutral, so only the variable
/// surface changes.
fn builtin_of(b: &QBuiltin) -> QBuiltin {
    match b {
        QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        } => QBuiltin::Is {
            target: prefix_builtin_term(target),
            lhs: prefix_builtin_term(lhs),
            op: *op,
            rhs: prefix_builtin_term(rhs),
        },
        QBuiltin::Compare { lhs, op, rhs } => QBuiltin::Compare {
            lhs: prefix_builtin_term(lhs),
            op: *op,
            rhs: prefix_builtin_term(rhs),
        },
    }
}

/// Convert one binary `QAtom` to an [`EvalAtom`] (predicate angle brackets already absent
/// in `QAtom::pred`), or report the gap.
///
/// The `Err` carries just the [`UnsupportedKind`]; the caller wraps it in the
/// `NativeOutcome::Unsupported` gap it returns (keeping the answer-sized outcome off the
/// `Err` path, which would otherwise bloat every `?`-returning result).
fn atom_of(atom: &QAtom) -> Result<EvalAtom, UnsupportedKind> {
    if atom.args.len() != 2 {
        return Err(UnsupportedKind::NonBinaryAtom);
    }
    // `atom.pred` is already a validated predicate IRI surface; carry it directly.
    let predicate = atom.pred.clone();
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
fn magic_pred_iri(pred: &str, adorn: &str) -> String {
    let split = pred.rfind(['/', '#']).map_or(pred.len(), |i| i + 1);
    let (base, local) = pred.split_at(split);
    // `base` ends with the separator; nest the magic predicates under `magic/` so they
    // never collide with a real predicate in the source namespace.
    format!("{base}magic/{local}_{adorn}")
}

/// Build a magic *guard* atom (a body literal) for an adorned IDB atom.
///
/// The general model of a magic guard is *the bound sub-tuple*: the guard atom carries
/// exactly the values at `atom`'s bound positions, keyed on the pattern's `code()`.
/// The engine's [`RelationStore`] is binary, so that bound sub-tuple is packed into the
/// binary `magic(subject, object)` carrier:
///
/// - all-free (`ff`) → NO guard (`None`): the predicate is demanded unrestricted.
/// - both bound (`bb`) → `magic(subject, object)` — the two-value bound sub-tuple.
/// - exactly one bound (`bf`/`fb`) → a self-loop `magic(v, v)` carrying the single
///   bound value `v` in both slots.
///
/// This is the arity-2 specialization of the bound-sub-tuple encoding. `atom` is an
/// [`EvalAtom`], which is structurally binary (subject/predicate/object), so `pattern`
/// is always arity-2 here — `resolve_native` rejects any non-binary atom before the
/// transform runs, and no arity != 2 pattern can reach this path. The assertion pins
/// that invariant; the arity>2 bound-sub-tuple carrier is unreachable until the generic
/// n-ary evaluator supplies a non-binary store (a later rung), so it is not emitted.
fn magic_guard_atom(atom: &EvalAtom, pattern: BindingPattern) -> Option<EvalAtom> {
    assert_eq!(
        pattern.arity(),
        2,
        "magic_guard_atom encodes over the binary RelationStore; EvalAtom is binary so \
         its adornment is arity-2 (non-binary atoms are rejected before the transform)"
    );
    if pattern.is_all_free() {
        return None;
    }
    let pred = magic_pred_iri(atom.predicate.as_str(), &pattern.code());
    let (subject, object) = match (pattern.is_bound(0), pattern.is_bound(1)) {
        (true, true) => (atom.subject.clone(), atom.object.clone()),
        // self-loop: carry the single bound term in both slots.
        (true, false) => (atom.subject.clone(), atom.subject.clone()),
        (false, true) => (atom.object.clone(), atom.object.clone()),
        (false, false) => unreachable!("all-free handled above"),
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
/// Same binary encoding as [`magic_guard_atom`]; returns `None` for an all-free (`ff`)
/// goal (no seed — the predicate is demanded unrestricted).
fn magic_seed_atom(goal_atom: &EvalAtom, pattern: BindingPattern) -> Option<EvalAtom> {
    magic_guard_atom(goal_atom, pattern)
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
fn adorn_atom(atom: &EvalAtom, bound: &BTreeSet<String>) -> BindingPattern {
    let pos_bound = |t: &EvalTerm| match var_name(t) {
        Some(v) => bound.contains(v),
        None => true, // a constant is always bound
    };
    BindingPattern::from_bools([pos_bound(&atom.subject), pos_bound(&atom.object)])
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
fn head_bound_vars(head: &EvalAtom, pattern: BindingPattern) -> BTreeSet<String> {
    let mut bound = BTreeSet::new();
    if pattern.is_bound(0)
        && let Some(v) = var_name(&head.subject)
    {
        bound.insert(v.to_owned());
    }
    if pattern.is_bound(1)
        && let Some(v) = var_name(&head.object)
    {
        bound.insert(v.to_owned());
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
        builtins: vec![],
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
fn magic_transform(
    rules: &[EvalRule],
    goal: &EvalAtom,
    goal_adorn: BindingPattern,
) -> MagicProgram {
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
    let mut demands: BTreeSet<(String, String)> = BTreeSet::new();
    demands.insert((goal.predicate.as_str().to_owned(), goal_adorn.code()));

    // Fixpoint: expanding a demand (pred, adorn) over every rule whose head is `pred`
    // discovers the adorned IDB body atoms it demands.
    let mut frontier: Vec<(String, BindingPattern)> =
        vec![(goal.predicate.as_str().to_owned(), goal_adorn)];

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
        let head_adorn = BindingPattern::from_code(adorn_code);
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

            // The modified rule carries the ORIGINAL rule's builtins: the shared
            // constraint stage evaluates them post-join, generating the head's
            // arithmetic answer (or filtering).  The magic (demand) rules carry NO
            // builtins — magic-sets is sound and complete under ANY sideways-
            // information-passing strategy, so adorning a builtin-bound variable as
            // free merely loosens demand (never changes the goal answers), and for
            // the binary arithmetic fragment the builtin is terminal, so the
            // adornment is in fact exact.
            let iri = format!("{}::mod/{}#{ri}", r.head.predicate.as_str(), adorn_code);
            let mut modified = rule(r.head.clone(), mod_body, iri);
            modified.builtins = r.builtins.clone();
            out.push(modified);
        }
    }

    MagicProgram { rules: out, seed }
}

/// Convert a ground magic seed [`EvalAtom`] into a [`crate::rule_ir::Fact`] for EDB
/// insertion.  The seed is always ground (its terms are goal constants), so this never
/// hits an unbound variable.
fn seed_to_fact(seed: &EvalAtom) -> Result<crate::rule_ir::Fact, String> {
    let to_term = |t: &EvalTerm| match t {
        EvalTerm::ConstNamed(nn) => Ok(TermValue::iri(nn.clone())),
        EvalTerm::ConstLit(term) => Ok(term.clone()),
        EvalTerm::Var(v) => Err(format!("magic seed term {v:?} is not ground")),
    };
    Ok(crate::rule_ir::Fact {
        subject: to_term(&seed.subject)?,
        predicate: seed.predicate.clone(),
        object: to_term(&seed.object)?,
    })
}

// ── Backward entry: resolve_native ────────────────────────────────────────────────

/// Compute the goal atom's adornment from its `(subject, object)` terms.
fn goal_adornment(goal: &QAtom) -> BindingPattern {
    let bound = |t: &QTerm| matches!(t, QTerm::Const(_) | QTerm::Num(_));
    BindingPattern::from_bools([bound(&goal.args[0]), bound(&goal.args[1])])
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
        let s_surface = term_display(&f.subject);
        let o_surface = term_display(&f.object);
        // Apply the goal's constant constraints.
        if let Some(c) = &s_const
            && &s_surface != c
        {
            continue;
        }
        if let Some(c) = &o_const
            && &o_surface != c
        {
            continue;
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
/// # Budget semantics
///
/// This engine governs BOTH budget fields:
///
/// - `budget.max_steps` — a step/derivation budget honoured DURING the bottom-up fixpoint
///   ([`crate::physical::seminaive::evaluate`]).  Exhaustion stamps
///   [`BudgetStatus::Exhausted`] on the answer; the returned bindings are a sound
///   (FactKey-ordered) partial slice, never a wrong verdict.  The demand-transformed goal
///   predicate is the TOP stratum (everything is demanded toward it), so a step cut always
///   leaves the goal unsaturated — `max_steps` exhaustion is `Exhausted`, and the goal is
///   `Ok`-complete precisely when the fixpoint runs to its natural end (including the
///   pure-EDB case, where no derivation fires and the answer is complete under any budget).
/// - `budget.max_answers` — a sound post-fixpoint truncation stamping [`BudgetStatus::Partial`].
///
/// When BOTH fire, the answer cap takes precedence (`Partial`), matching the reference
/// oracle ([`crate::reference_resolver`]'s `budget_exceeded`/`resolve_conjunct`).
///
/// # Errors
///
/// Returns `Err` for an evaluator failure (e.g. an unbound head variable or a
/// provenance-recipe failure) propagated from the shared engine helpers.
/// The outcome of [`eval_with_base_fallback`]: a decided fixpoint (facts, budget
/// status, completion frontier) or a declared native gap.
///
/// A private mirror of [`NativeOutcome`] specialized to the fallback decision, so the
/// two-tier evaluate/fall-back-to-base logic lives in one testable place.
enum FallbackOutcome {
    /// The (transformed or base) program was decided.
    Decided(Vec<Fact>, BudgetStatus, CompletionFrontier),
    /// A declared native gap the caller routes to the oracle.
    Unsupported(UnsupportedKind),
}

/// Evaluate `transformed_rules` over `edb`; on a `NonStratifiable` gap, fall back to a
/// full stratified evaluation of `base_rules` over the base EDB.
///
/// Because the query IR carries no negation, a demand transform is always stratifiable,
/// so on the current fragment the fallback branch is taken only if a transformed program
/// is non-stratifiable for another reason. It falls back to a full stratified evaluation
/// of the base rules — correct, without the demand pruning (it materializes more than the
/// query strictly needs) — and stays native (no external-engine demotion; the native core
/// stays authoritative). `base_edb` is a closure so the base EDB is extracted lazily, ONLY
/// when the fallback fires: the happy path never pays for it, and the untransformed base
/// rules never reference the demand seed the transformed EDB carries.
///
/// # Errors
///
/// Propagates an [`evaluate`] failure (unbound head/guard variable or provenance-recipe
/// failure) from either the transformed or the base evaluation.
fn eval_with_base_fallback(
    edb: RelationStore,
    transformed_rules: &[EvalRule],
    base_rules: &[EvalRule],
    max_steps: Option<u64>,
    base_edb: impl FnOnce() -> RelationStore,
) -> Result<FallbackOutcome, String> {
    match evaluate(edb, transformed_rules, max_steps)? {
        NativeOutcome::Decided(budgeted) => {
            let frontier = budgeted.frontier();
            Ok(FallbackOutcome::Decided(
                budgeted.rows,
                budgeted.status,
                frontier,
            ))
        }
        // A magic (demand) transform threads a magic guard through the program; a negative
        // edge in that guarded cycle could make the transformed program non-stratifiable
        // even though the UNTRANSFORMED program is stratified by construction. Fall back to
        // the base rules over a freshly extracted EDB (without the demand seed the base
        // rules never reference).
        NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable) => {
            match evaluate(base_edb(), base_rules, max_steps)? {
                NativeOutcome::Decided(budgeted) => {
                    let frontier = budgeted.frontier();
                    Ok(FallbackOutcome::Decided(
                        budgeted.rows,
                        budgeted.status,
                        frontier,
                    ))
                }
                // If the BASE program is also non-stratifiable, the program genuinely is —
                // a real declared gap the caller routes to the oracle.
                NativeOutcome::Unsupported(other) => Ok(FallbackOutcome::Unsupported(other)),
            }
        }
        // Any other declared native gap (cut / arithmetic / non-binary) passes through to
        // the caller's oracle route unchanged.
        NativeOutcome::Unsupported(other) => Ok(FallbackOutcome::Unsupported(other)),
    }
}

pub(crate) fn resolve_native(
    foreign: &dyn ScryerForeign,
    world: &str,
    program: &QProgram,
    budget: &Budget,
) -> Result<NativeOutcome<AnswerSet>, String> {
    // (0) Gate cut (reuse the structural detector the dispatch gate uses).  Arithmetic
    // is no longer a whole-program gap — the closed builtin set is evaluated natively;
    // any residual (unbound operand / ÷0 / overflow) surfaces as a gap DURING the
    // fixpoint (see `seminaive::evaluate`).  Profile confinement is upstream in
    // `dispatch::dispatch_query` (`profile_gate::check_builtin_profile`), unchanged.
    if profile_gate::has_cut(program) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut));
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

    // (1) Convert program rules → binary EvalRules, splitting each body into its atoms
    // (the join structure) and its arithmetic/comparison builtins (the post-join
    // constraint stage, evaluated in the modified rules by the shared moded evaluator).
    let mut rules: Vec<EvalRule> = Vec::with_capacity(program.rules.len());
    for r in &program.rules {
        // Cut is procedural — still a declared gap.
        if r.body.iter().any(|lit| matches!(lit, QBodyLit::Cut)) {
            return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut));
        }
        let head = match atom_of(&r.head) {
            Ok(a) => a,
            Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
        };
        let mut body: Vec<EvalAtom> = Vec::new();
        let mut builtins: Vec<QBuiltin> = Vec::new();
        for lit in &r.body {
            match lit {
                QBodyLit::Atom(a) => match atom_of(a) {
                    Ok(ea) => body.push(ea),
                    Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
                },
                QBodyLit::Builtin(b) => builtins.push(builtin_of(b)),
                // Cut already returned above.
                QBodyLit::Cut => unreachable!("cut handled above"),
            }
        }
        // A synthesized stable rule IRI for the modified/original rule.
        let rule_iri = format!("{}::rule", head.predicate.as_str());
        rules.push(EvalRule {
            head,
            body,
            rule_iri,
            distinct_pairs: vec![],
            builtins,
        });
    }

    // (2) Compute the goal adornment and magic-transform.
    let goal_atom = match atom_of(goal) {
        Ok(a) => a,
        Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
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
    // The step/derivation budget is honoured DURING the fixpoint: `Exhausted` on a cut,
    // `Ok` on a natural fixpoint (including the pure-EDB case, where no rule fires).  The
    // decided arm surfaces the governor's completion frontier (which strata / predicates
    // are settled) on the answer instead of dropping it: an `Exhausted` backward goal is
    // incomplete, and the caller reads `completed < total` to tell that from a conclusive
    // result.  On a non-stratifiable transformed program the helper falls back to the base
    // rules over a lazily re-extracted EDB (see `eval_with_base_fallback`).
    let (facts, fixpoint_status, frontier) =
        match eval_with_base_fallback(edb, &transformed.rules, &rules, budget.max_steps, || {
            extract_edb(foreign, world)
        })? {
            FallbackOutcome::Decided(f, s, fr) => (f, s, fr),
            FallbackOutcome::Unsupported(k) => return Ok(NativeOutcome::Unsupported(k)),
        };

    // (4) Project the goal predicate's derived tuples into bindings.
    let mut bindings = project_answers(&facts, goal, goal_atom.predicate.as_str());

    // (5) Budget semantics — compose the step governor (fixpoint `Exhausted`) with the
    //     post-fixpoint `max_answers` truncation (`Partial`).  Precedence follows the
    //     reference oracle: when the answer cap is reached, `Partial` takes precedence
    //     even if the step budget also fired; otherwise a step cut stays `Exhausted`.
    let mut status = fixpoint_status;
    if let Some(max_a) = budget.max_answers {
        // Deterministic truncation: canonicalize first so the kept prefix is stable.
        let mut tmp = AnswerSet {
            bindings: bindings.clone(),
            status: BudgetStatus::Ok,
            preservation: crate::result::PreservationClaim::exact(),
            frontier: crate::query_ir::CompletionFrontier::empty(),
        };
        tmp.canonicalize();
        if tmp.bindings.len() >= max_a && !tmp.bindings.is_empty() {
            // The oracle stamps Partial the moment it reaches (or exceeds) max_answers;
            // the answer cap overrides a concurrent step `Exhausted`.
            tmp.bindings.truncate(max_a);
            status = BudgetStatus::Partial;
        }
        bindings = tmp.bindings;
    }

    let mut answer = AnswerSet {
        bindings,
        status,
        preservation: crate::result::PreservationClaim::exact(),
        frontier,
    };

    // (6) Frontier-aware conclusive-`neither` consult. When the fixpoint was step-cut
    //     (`Exhausted`) yet the GOAL predicate's stratum reached its natural fixpoint —
    //     its least-model extension is FINAL, recorded in `saturated_preds` under the
    //     bare-IRI head name `seminaive` inserts (`rule.head.predicate.as_str()`), which
    //     is exactly `goal_atom.predicate.as_str()` for the goal relation's modified
    //     rules — an EMPTY witness is a sound negative answer: the conclusive four-valued
    //     `neither`, NOT the `undetermined` of an unfinished search. So the answer is
    //     complete-for-fragment and its status collapses to `Ok`.
    //
    //     In the present positive-Horn / stratified-negation backward fragment the goal
    //     predicate is the ROOT of the demand transform (every demanded predicate is
    //     reachable FROM it), so it is at the maximal demanded stratum: it saturates only
    //     when the whole demanded run completes, at which point `status` is already `Ok`
    //     and this guard is a correct no-op. The consult is nonetheless PRESENT and
    //     correct so that any future fragment which can settle the goal predicate under a
    //     global (multi-world) cut yields the sound `neither` rather than over-claiming
    //     `undetermined`. A `Partial` (answer-cap) status is only ever set on a NON-empty
    //     witness, so the `is_empty()` guard also keeps this from touching `Partial`.
    if answer.status == BudgetStatus::Exhausted
        && answer.bindings.is_empty()
        && answer
            .frontier
            .saturated_preds
            .contains(goal_atom.predicate.as_str())
    {
        answer.status = BudgetStatus::Ok;
    }

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

    fn make_world(triples: &[(&str, &str, &str)]) -> (WorldStore, String) {
        let store = WorldStore::new();
        for (s, p, o) in triples {
            store.insert_quad(W, s, p, o);
        }
        (store, W.to_owned())
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

    fn tc_world() -> (WorldStore, String) {
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

    // ── GAP B: the frontier-aware conclusive-`neither` consult ───────────────────
    //
    // A 0-step budget cuts the `ancestor` fixpoint before ANY derivation: the goal
    // predicate's stratum never saturates. An empty witness is therefore genuinely
    // UNDETERMINED (the search was cut mid-fixpoint), NOT the conclusive four-valued
    // `neither`. The consult must NOT fire — the answer stays `Exhausted`. This is the
    // usual recursive-goal case the doc calls out: the goal predicate is the ROOT of the
    // demand transform, so it settles only when the whole run completes (then the status
    // is already `Ok`), and can never be settled on an `Exhausted` run. The guard is
    // present and correct, and this test proves it correctly does NOT over-collapse.

    #[test]
    fn magic_backward_exhausted_recursive_goal_is_not_over_collapsed() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let prog = tc_program(); // ?- ancestor(a, Y)
        let budget = Budget {
            max_answers: None,
            max_steps: Some(0), // cut before any ancestor derivation
        };

        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());

        assert_eq!(
            native.status,
            BudgetStatus::Exhausted,
            "a 0-step cut mid-search is Exhausted (undetermined), not a conclusive Ok"
        );
        assert!(
            native.bindings.is_empty(),
            "no derivation committed ⇒ empty witness"
        );
        // The consult's precondition is the goal predicate being SETTLED. It is the root of
        // the demand transform and its stratum was cut, so it is NOT in the settled
        // frontier — the guard therefore correctly does not fire.
        let goal_pred = format!("{BASE}ancestor");
        assert!(
            !native.frontier.saturated_preds.contains(&goal_pred),
            "the cut goal predicate must NOT be reported settled: {:?}",
            native.frontier.saturated_preds
        );
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
                builtins: vec![],
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
        let facts = match evaluate(edb, &transformed.rules, None).unwrap() {
            NativeOutcome::Decided(budgeted) => budgeted.rows,
            other => panic!("expected Decided, got {other:?}"),
        };

        let anc = format!("{BASE}ancestor");
        let derived_anc: BTreeSet<(String, String)> = facts
            .iter()
            .filter(|f| f.predicate.as_str() == anc)
            .map(|f| (term_display(&f.subject), term_display(&f.object)))
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
    fn magic_binary_arithmetic_is_decided_natively() {
        // The binary arithmetic list-length program is now DECIDED by the native
        // magic core (no longer an Arithmetic gap): the builtin `N is M + 1` is
        // evaluated as a post-join generator in the modified rules.  Over the
        // single-cell list l0→rest→nil, len(l0) = 1.
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
        let NativeOutcome::Decided(answer) = outcome else {
            panic!("binary arithmetic must be Decided natively, not a gap: {outcome:?}");
        };
        assert_eq!(answer.bindings.len(), 1, "one length answer: {answer:?}");
        assert_eq!(
            answer.bindings[0]["N"],
            "\"1\"^^<http://www.w3.org/2001/XMLSchema#integer>"
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

    // ── Budget: max_steps (step/derivation governor) ─────────────────────────────

    /// A step budget below the completion cost stamps `Exhausted` and returns a SOUND
    /// SUBSET of the unbounded answers — never a wrong verdict, never an answer the full
    /// model does not contain.
    #[test]
    fn magic_budget_max_steps_exhausts_with_sound_subset() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let prog = tc_program();

        let unbounded =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        assert_eq!(unbounded.status, BudgetStatus::Ok);
        let full: BTreeSet<String> = unbounded.bindings.iter().map(|b| b["Y"].clone()).collect();
        assert_eq!(full.len(), 3, "a→b→c→d yields ancestors {{b,c,d}}");

        let budget = Budget {
            max_steps: Some(1),
            ..Default::default()
        };
        let cut = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        assert_eq!(
            cut.status,
            BudgetStatus::Exhausted,
            "a 1-step budget cannot reach the 3-answer fixpoint ⇒ Exhausted"
        );
        for b in &cut.bindings {
            assert!(
                full.contains(&b["Y"]),
                "every budget-cut answer must be sound (present in the full model): {b:?}"
            );
        }
        assert!(
            cut.bindings.len() < full.len(),
            "the cut answer set is a strict subset of the full model"
        );
    }

    /// A step cut is DETERMINISTIC on the backward leg: the same intermediate budget
    /// yields byte-identical bindings and status run-to-run (the fixpoint cut is the Nth
    /// FactKey-sorted committed winner, and `project_answers`+`canonicalize` is a
    /// deterministic function of the fact cut).
    #[test]
    fn magic_budget_max_steps_is_deterministic() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let prog = tc_program();
        let budget = Budget {
            max_steps: Some(2),
            ..Default::default()
        };
        let run1 = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        let run2 = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        assert_eq!(run1.status, run2.status, "status is deterministic");
        assert_eq!(
            run1.bindings, run2.bindings,
            "the backward-leg budget cut is byte-identical run-to-run"
        );
    }

    /// When BOTH budgets fire, the answer cap takes precedence (`Partial`), matching the
    /// reference oracle — a step `Exhausted` does not override a reached `max_answers`.
    #[test]
    fn magic_budget_max_steps_and_max_answers_partial_precedence() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        let prog = tc_program();
        // A generous step budget so the fixpoint completes, then a max_answers cap of 1.
        let budget = Budget {
            max_steps: Some(1_000_000),
            max_answers: Some(1),
        };
        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        assert_eq!(native.bindings.len(), 1, "capped at 1 answer");
        assert_eq!(
            native.status,
            BudgetStatus::Partial,
            "the answer cap takes precedence over any step budget"
        );
    }

    /// A pure-EDB goal is `Ok`-complete under ANY step budget, including `max_steps = 0`:
    /// no rule fires (the goal predicate is EDB, i.e. the settled stratum 0), so the
    /// answer needs no derivation.  This is the frontier win at the query surface — the
    /// reference oracle would stamp `Exhausted` at 0 (it counts the EDB lookup as a step),
    /// but native honestly reports a complete answer.
    #[test]
    fn magic_pure_edb_goal_is_ok_under_zero_step_budget() {
        let (store, world_nn) = tc_world();
        let foreign = WorldStoreForeign::from_world(&store, W, PROFILE).unwrap();
        // Goal is the EDB predicate parentOf; the program carries NO rules.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let budget = Budget {
            max_steps: Some(0),
            ..Default::default()
        };
        let native = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        assert_eq!(
            native.status,
            BudgetStatus::Ok,
            "a pure-EDB goal derives nothing ⇒ complete under any budget"
        );
        assert_eq!(native.bindings.len(), 1, "parentOf(a, b)");
        assert_eq!(native.bindings[0]["Y"], format!("<{BASE}b>"));
    }

    // ── Native base-rule fallback (the extracted fallback decision) ───────────────
    //
    // `eval_with_base_fallback` is the fallback DECISION extracted from `resolve_native`.
    // The public query-IR fragment carries no negation, so a demand transform is always
    // stratifiable and this arm is not reachable through `resolve_native`'s query inputs;
    // exercise it directly with a hand-built non-stratifiable transformed program and a
    // distinct, stratifiable base program.

    /// A binary body/head atom `pred(?s, ?o)`, negated iff `neg`.
    fn fb_atom(subject: &str, pred: &str, object: &str, neg: bool) -> EvalAtom {
        EvalAtom {
            subject: EvalTerm::Var(subject.to_owned()),
            predicate: format!("{BASE}{pred}"),
            object: EvalTerm::Var(object.to_owned()),
            negated: neg,
        }
    }

    /// A rule `head :- body` with no builtins/guards.
    fn fb_rule(head: EvalAtom, body: Vec<EvalAtom>) -> EvalRule {
        let rule_iri = format!("{}::rule", head.predicate);
        EvalRule {
            head,
            body,
            rule_iri,
            distinct_pairs: vec![],
            builtins: vec![],
        }
    }

    /// A structurally non-stratifiable pair `a :- ~b`, `b :- ~a` (a negative cycle):
    /// `stratify` returns `None`, so `evaluate` yields `Unsupported(NonStratifiable)`.
    fn non_stratifiable_rules() -> Vec<EvalRule> {
        vec![
            fb_rule(
                fb_atom("?X", "a", "?Y", false),
                vec![fb_atom("?X", "b", "?Y", true)],
            ),
            fb_rule(
                fb_atom("?X", "b", "?Y", false),
                vec![fb_atom("?X", "a", "?Y", true)],
            ),
        ]
    }

    #[test]
    fn eval_with_base_fallback_fires_on_nonstratifiable_transform() {
        let transformed = non_stratifiable_rules();
        // A DIFFERENT, stratifiable base program: derived(?X, ?Y) :- src(?X, ?Y).
        let base = vec![fb_rule(
            fb_atom("?X", "derived", "?Y", false),
            vec![fb_atom("?X", "src", "?Y", false)],
        )];
        let x = format!("{BASE}x");
        let y = format!("{BASE}y");
        // The base EDB `src(x, y)` — extracted lazily ONLY when the fallback fires.
        let base_edb = || {
            let mut edb = RelationStore::new();
            edb.insert(
                &format!("{BASE}src"),
                TermValue::iri(x.clone()),
                TermValue::iri(y.clone()),
            );
            edb
        };
        // The transformed EDB is irrelevant: the transform is non-stratifiable, so
        // `evaluate` short-circuits before touching it.
        let out =
            eval_with_base_fallback(RelationStore::new(), &transformed, &base, None, base_edb)
                .expect("fallback must not error");
        let FallbackOutcome::Decided(facts, status, _frontier) = out else {
            panic!("expected the base fallback to decide, got a declared gap");
        };
        assert_eq!(
            status,
            BudgetStatus::Ok,
            "the base fixpoint runs to its natural end"
        );
        let keys: BTreeSet<_> = facts.iter().map(Fact::key).collect();
        // The base rule derived exactly derived(x, y) — proof the arm executed the BASE
        // rules, not the (non-stratifiable) transformed ones.
        assert!(
            keys.contains(&(format!("<{x}>"), format!("{BASE}derived"), format!("<{y}>"))),
            "base materialization must contain derived(x, y): {keys:?}"
        );
        // No transformed-only predicate appears (the transformed program never evaluated).
        assert!(
            keys.iter()
                .all(|(_, p, _)| p != &format!("{BASE}a") && p != &format!("{BASE}b")),
            "no transformed-program predicate may appear: {keys:?}"
        );
        // Exactly the base EDB fact plus the single derived fact.
        assert_eq!(
            keys.len(),
            2,
            "base fact set = {{src(x, y), derived(x, y)}}: {keys:?}"
        );
    }

    #[test]
    fn eval_with_base_fallback_passes_through_when_base_also_nonstratifiable() {
        let transformed = non_stratifiable_rules();
        let base = non_stratifiable_rules();
        // The base EDB never matters here: the base program is also non-stratifiable, so
        // its `evaluate` short-circuits at stratification.
        let out = eval_with_base_fallback(
            RelationStore::new(),
            &transformed,
            &base,
            None,
            RelationStore::new,
        )
        .expect("fallback must not error");
        assert!(
            matches!(
                out,
                FallbackOutcome::Unsupported(UnsupportedKind::NonStratifiable)
            ),
            "both transformed and base non-stratifiable ⇒ the genuine gap passes through"
        );
    }
}
