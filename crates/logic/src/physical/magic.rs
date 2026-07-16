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
//! # Subsumptive demand keying (Tekle & Liu, SIGMOD 2011)
//!
//! The demand keying is SUBSUMPTIVE, not variant: when a predicate is demanded at several
//! adornments, only the ⊑-MINIMAL (most-general — fewest bound positions) ones mint a magic
//! predicate. Under the adornment lattice `A ⊑ B iff bound(A) ⊆ bound(B)` (A more general), a
//! demand on the kept general `A` serves every more-specific `B` it subsumes — `A`'s answers
//! ⊇ `B`'s — so a more-specific call reads `A`'s table filtered by the residual on the extra
//! positions `bound(B) ∖ bound(A)`. On this binary path that residual is discharged for FREE:
//! each modified rule keeps its ORIGINAL body atoms (whose constants/variables carry the real
//! join, so the derived fact set stays a subset of the untransformed least model — never
//! spurious), and the goal projection re-imposes the goal's own bound positions. Widening a
//! magic guard to a more-general table therefore only derives a superset of a demand slice of
//! the SAME least model; the goal answer set is byte-identical to the per-adornment (variant)
//! keying, while fewer magic predicates and derivations are minted (the structural win). The `#[cfg(test)] magic_transform_variant` is the retained
//! byte-identity A/B oracle.
//!
//! # The transformation (standard magic-sets, left-to-right SIPS)
//!
//! For a goal `g(t0, t1)` with adornment `a` (over `{b, f}`):
//!
//! 1. **Seed** — the goal's ground magic fact carrying the goal's bound constant(s),
//!    asserted directly into the EDB as control state rather than retained as an
//!    unconditional demand rule. For `ff` there is no seed at all (`ff` is unrestricted).
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
//! # Stratified negation-as-failure
//!
//! The backward surface carries stratified NAF (`\+ p(s, o)`): a negated body atom lowers
//! to a negated binary [`EvalAtom`] the shared stratified evaluator decides by NAF against
//! the accumulated lower-stratum least model. Under the transform a negated atom is
//! demanded exactly like a positive one (its magic rules propagate the demand through its
//! own recursion, so the NAF test sees a sufficient slice of the negated predicate), is
//! kept negated in the modified rule, is carried (still negated) into the SIPS prefix, and
//! binds no SIPS variables. Every negated variable must be range-restricted by a positive
//! body atom; an unbound one flounders ([`UnsupportedKind::Floundering`]).
//!
//! A negative literal inside a magic (demand) rule can make the transformed program
//! non-stratifiable even when the base program is stratified. Standard magic-sets theory
//! guarantees the transform is answer-preserving *when its result is stratified*; when the
//! result is NOT stratifiable, [`eval_with_base_fallback`] evaluates the UNTRANSFORMED base
//! program — a full stratified materialization (sound and terminating over the finite
//! Herbrand base, no value invention) — dropping only the demand pruning, and the answer's
//! preservation is honestly downgraded from `{exact}` to record that. It stays native (no
//! external-engine demotion: the native core remains authoritative). A base program that is
//! ALSO non-stratifiable is a genuine gap returned to production dispatch.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use crate::physical::binding_pattern::BindingPattern;
use crate::physical::incremental::{IncrementalSession, SignedFact};
use crate::physical::seminaive::{NativeOutcome, UnsupportedKind, evaluate};
#[cfg(test)]
use crate::physical::store::extract_edb;
use crate::physical::store::{RelationStore, extract_edb_patterns};
use crate::profile_gate;
use crate::provenance::term_display;
use crate::query_ir::{
    AnswerSet, Binding, Budget, CompletionFrontier, QAtom, QBodyLit, QBuiltin, QProgram, QTerm,
};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact};
use crate::seam::{BudgetStatus, WorldFactPattern, WorldFactSource};

use crate::annotation::{
    AnnotatedAnswer, AnnotatedAnswerSet, AnnotatedFactKey, AnnotationDerivation, AnnotationFactRef,
    AnnotationRequest, TupleAnnotationAlgebra,
};

/// Wrap a physical-chase condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn physical_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical { detail })
}

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
/// typed lowering); a `Num` is an arithmetic operand the native core does not carry.
///
/// Shared with the n-ary generic backward path ([`super::magic_generic`]): the same
/// `QTerm → EvalTerm` codec lowers a generic atom's positional args.
pub(super) fn term_of(t: &QTerm) -> Result<EvalTerm, UnsupportedKind> {
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
        // A structured (function-symbol) argument never reaches the flat binary/generic
        // codec: `resolve_native_under` routes any program carrying a `Struct` term to the
        // full-FOL resolver BEFORE this lowering. Should one ever arrive here it is a
        // non-binary shape the flat store cannot represent — a typed gap, never a panic.
        QTerm::Struct(_) => Err(UnsupportedKind::NonBinaryAtom),
    }
}

/// Rewrite a builtin operand's variable to the engine's `?`-prefixed surface,
/// matching the [`EvalTerm::Var`] keys the body atoms carry (constants unchanged).
fn prefix_builtin_term(t: &QTerm) -> QTerm {
    match t {
        QTerm::Var(v) => QTerm::Var(format!("?{v}")),
        // A structured term never reaches the flat builtin surface (it is routed to the
        // full-FOL resolver upstream); carry it unchanged for exhaustiveness.
        QTerm::Const(_) | QTerm::Num(_) | QTerm::Struct(_) => t.clone(),
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

pub(super) fn source_term(term: &EvalTerm) -> Option<TermValue> {
    match term {
        EvalTerm::Var(_) => None,
        EvalTerm::ConstNamed(iri) => Some(TermValue::iri(iri)),
        EvalTerm::ConstLit(value) => Some(value.clone()),
    }
}

/// Build the minimal deterministic set of RDF source probes required by a binary
/// query. Source facts can share a predicate with rule heads, so the plan is based on
/// every relation *consumed* by the goal or a body atom rather than an EDB/IDB name
/// partition. A broad pattern subsumes narrower probes for the same predicate.
fn binary_source_patterns(rules: &[EvalRule], goal: &EvalAtom) -> Vec<WorldFactPattern> {
    let mut patterns = Vec::new();
    let atoms = std::iter::once(goal).chain(rules.iter().flat_map(|rule| rule.body.iter()));
    for atom in atoms {
        let pattern = WorldFactPattern::new(
            source_term(&atom.subject),
            Some(atom.predicate.clone()),
            source_term(&atom.object),
        );
        if patterns
            .iter()
            .any(|existing: &WorldFactPattern| existing.subsumes(&pattern))
        {
            continue;
        }
        patterns.retain(|existing| !pattern.subsumes(existing));
        patterns.push(pattern);
    }
    patterns.sort();
    patterns
}

/// Predicate-only source plan for a reusable incremental session.
///
/// A later signed transaction may retract a source tuple that did not match the
/// initial goal's constants (for example, replacing `status(up)` with
/// `status(down)`). The session therefore admits every tuple of each consumed
/// predicate while still excluding predicates the program can never inspect.
fn incremental_source_patterns(rules: &[EvalRule], goal: &EvalAtom) -> Vec<WorldFactPattern> {
    let predicates = std::iter::once(goal)
        .chain(rules.iter().flat_map(|rule| rule.body.iter()))
        .map(|atom| atom.predicate.clone())
        .collect::<BTreeSet<_>>();
    predicates
        .into_iter()
        .map(|predicate| WorldFactPattern::new(None, Some(predicate), None))
        .collect()
}

// ── Magic-predicate minting ───────────────────────────────────────────────────────

/// Mint the deterministic magic-predicate IRI for `pred` under adornment `adorn`.
///
/// Derived from the original predicate IRI: the base (everything up to and including the
/// last `/` or `#`) plus `magic/<localname>_<adorn>`.  Stable across runs.
///
/// A plain, arity-agnostic string transform: the n-ary generic backward path
/// ([`super::magic_generic`]) mints its magic-relation IRIs through the SAME function, so a
/// generic magic relation and a binary one share the identical minting rule.
pub(super) fn magic_pred_iri(pred: &str, adorn: &str) -> String {
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

/// Whether every variable position of `atom` is already in `bound` (constants count as
/// bound) — i.e. the atom is fully ground under the current SIPS bindings.
fn negated_atom_fully_bound(atom: &EvalAtom, bound: &BTreeSet<String>) -> bool {
    let pos_bound = |t: &EvalTerm| var_name(t).is_none_or(|v| bound.contains(v));
    pos_bound(&atom.subject) && pos_bound(&atom.object)
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

/// Whether any negated body atom carries a variable that no POSITIVE literal binds — the
/// floundering (NAF-safety / allowedness) test.
///
/// A rule is allowed for NAF iff every variable appearing in a negated body atom also
/// appears in a positive body atom (range restriction) or is bound by an arithmetic `is`
/// generator. When that holds, the positive join grounds the negated atom's variables
/// before the NAF membership test, so the test is decided against a fully-ground tuple.
/// A negated variable that no positive literal binds is still free at NAF time — the goal
/// flounders, and the caller returns [`UnsupportedKind::Floundering`] rather than a wrong
/// or empty answer.  Body order is irrelevant: the join computes all positive atoms before
/// applying negation, so a variable bound by ANY positive atom is bound at NAF time.
fn negated_body_flounders(body: &[EvalAtom], builtins: &[QBuiltin]) -> bool {
    let mut bound: BTreeSet<String> = BTreeSet::new();
    for atom in body.iter().filter(|a| !a.negated) {
        if let Some(v) = var_name(&atom.subject) {
            bound.insert(v.to_owned());
        }
        if let Some(v) = var_name(&atom.object) {
            bound.insert(v.to_owned());
        }
    }
    // An `is` generator binds its target variable, so a negated atom over it is range-
    // restricted; a comparison binds nothing.
    for b in builtins {
        if let QBuiltin::Is {
            target: QTerm::Var(v),
            ..
        } = b
        {
            bound.insert(v.clone());
        }
    }
    body.iter().filter(|a| a.negated).any(|neg| {
        [&neg.subject, &neg.object]
            .into_iter()
            .filter_map(var_name)
            .any(|v| !bound.contains(v))
    })
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

/// Route a transform-emitted rule: a bodyless positive rule is an unconditional GROUND
/// control fact, so materialize it directly as a demand seed; a conditional rule is emitted
/// normally.
///
/// A bodyless positive rule's head is always ground: an empty body means the head guard is
/// `None`, so every bound position of the emitted atom is a constant carried from the source
/// — never a variable. Deduping keeps the seed set deterministic when both emission sites
/// mint the same demand fact.
fn emit_or_seed(
    head: EvalAtom,
    body: Vec<EvalAtom>,
    rule_iri: String,
    out: &mut Vec<EvalRule>,
    seeds: &mut Vec<EvalAtom>,
) {
    if body.is_empty() {
        if !seeds.contains(&head) {
            seeds.push(head); // deterministic dedup
        }
    } else {
        out.push(rule(head, body, rule_iri));
    }
}

/// The output of the magic-sets transformation: the transformed binary program (modified
/// rules + magic rules) plus the SET of ground demand seed facts inserted into the EDB
/// before evaluation.
///
/// EVERY unconditional demand rule the transform would produce (the goal's magic seed AND
/// each per-atom/modified demand rule whose body collapses to empty) is lifted into this seed
/// set. Such a rule is definitionally a ground control fact — an asserted demand — so it
/// belongs in the EDB seed rather than the semantic rule program. An `ff` goal contributes no
/// goal seed (the predicate is unrestricted); the set is then whatever the demand/modified
/// sites lift.
struct MagicProgram {
    /// The transformed rules (modified original rules + magic rules), with no
    /// unconditional demand-control rule. Semantic NAF-only or builtin-only rules may
    /// have no positive atom and are evaluated from the relational identity.
    rules: Vec<EvalRule>,
    /// The ground demand seed facts to assert into the EDB before evaluation (the goal's
    /// magic seed plus every lifted bodyless-rule head), deduplicated and order-stable.
    seeds: Vec<EvalAtom>,
}

/// The full demanded adornment set of a magic-sets demand fixpoint: for each IDB predicate,
/// the set of adornment codes (`BindingPattern::code`) it is demanded at, discovered by the
/// standard left-to-right-SIPS demand fixpoint rooted at the goal.
///
/// This is the RAW variant-keyed demand set — the input the subsumptive collapse operates
/// on. Codes (not `BindingPattern`s) key the inner set so the map stays deterministic
/// without an arbitrary total order on the lattice, mirroring the code-string identity the
/// magic-predicate IRIs already carry.
fn demand_fixpoint(
    rules: &[EvalRule],
    idb: &BTreeSet<String>,
    goal: &EvalAtom,
    goal_adorn: BindingPattern,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut demanded: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    demanded
        .entry(goal.predicate.as_str().to_owned())
        .or_default()
        .insert(goal_adorn.code());

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
                    // A negated IDB atom is demanded exactly like a positive one (at its
                    // current-bindings adornment): the demand — propagated through the
                    // predicate's own recursion by its magic rules — materializes precisely
                    // the instances the NAF test needs, so `\+ p(s, o)` is decided against a
                    // sufficient slice of `p`. It only differs in NOT threading its vars into
                    // the SIPS `bound` set below (NAF binds nothing).
                    let a = adorn_atom(atom, &bound);
                    // The inner set doubles as the visited-set: insert returns true only the
                    // first time a demand is seen, so each frontier node expands once.
                    if demanded
                        .entry(atom.predicate.as_str().to_owned())
                        .or_default()
                        .insert(a.code())
                    {
                        frontier.push((atom.predicate.as_str().to_owned(), a));
                    }
                }
                // Thread this atom's bindings for the next atom (SIPS). A negated atom binds
                // nothing under negation-as-failure, so it never extends `bound`.
                if !atom.negated {
                    bind_atom_vars(atom, &mut bound);
                }
            }
        }
    }
    demanded
}

/// The ⊑-MINIMAL antichain of a predicate's demanded adornment set — the MOST-GENERAL
/// (fewest-bound-positions) patterns, keeping each pattern that no OTHER demanded pattern is
/// strictly more general than.
///
/// Under the lattice `A ⊑ B iff bound(A) ⊆ bound(B)` (A more general), a demand keyed on the
/// kept general `A` serves every more-specific `B` it subsumes: `A`'s answers ⊇ `B`'s, so the
/// specific call reads `A`'s table filtered by the residual on `bound(B) ∖ bound(A)` — which
/// on this binary path is discharged for free by the modified rule's ORIGINAL body atoms plus
/// the goal projection (see `magic_transform`). This is the subsumptive-tabling
/// collapse: keep only the most-general table per predicate.
fn minimal_antichain(codes: &BTreeSet<String>) -> Vec<BindingPattern> {
    let pats: Vec<BindingPattern> = codes.iter().map(|c| BindingPattern::from_code(c)).collect();
    pats.iter()
        .copied()
        .filter(|p| !pats.iter().any(|q| q != p && q.subsumes(p)))
        .collect()
}

/// The kept magic-table adornment that SERVES a demanded `pat`: the most-general kept
/// pattern that subsumes it. Ties (a `pat` subsumed by two incomparable kept minimals, e.g.
/// `bb` served by either `bf` or `fb`) are broken deterministically by the smallest `code()`,
/// so the transform output is stable run-to-run.
///
/// # Panics
///
/// Panics if no kept pattern subsumes `pat` — impossible when `kept` is the minimal antichain
/// of a demanded set that CONTAINS `pat` (every element of a finite poset is ≥ some minimal
/// element).
fn serve(kept: &[BindingPattern], pat: BindingPattern) -> BindingPattern {
    kept.iter()
        .copied()
        .filter(|a| a.subsumes(&pat))
        .min_by(|a, b| a.code().cmp(&b.code()))
        .expect("every demanded pattern is subsumed by a kept minimal (most-general) element")
}

/// The SUBSUMPTIVE magic-sets transformation — the PRODUCTION demand rewrite.
///
/// Runs the standard demand fixpoint ([`demand_fixpoint`]), then COLLAPSES each predicate's
/// demanded adornment set to its ⊑-minimal (most-general) antichain ([`minimal_antichain`]):
/// only those most-general adornments mint a magic predicate. A more-specific demanded
/// adornment `B` is NOT minted — it is SERVED from the kept general `A` that subsumes it
/// ([`serve`]) plus a residual filter on `bound(B) ∖ bound(A)`. On this binary path that
/// residual is discharged WITHOUT an extra atom: the modified rule keeps its ORIGINAL body
/// atoms (which carry the real join constants/variables, so the derived fact set stays a
/// subset of the untransformed least model — never spurious), and the goal projection
/// ([`project_answers`]) filters the goal's own bound positions. Widening a magic guard to a
/// more-general table therefore only DERIVES a superset of a demand-restricted slice of the
/// same least model, never a wrong answer — the goal answer set is byte-identical to the
/// variant transform ([`magic_transform_variant`], the `#[cfg(test)]` byte-identity oracle).
///
/// Returns the transformed program + seed.
fn magic_transform(
    rules: &[EvalRule],
    goal: &EvalAtom,
    goal_adorn: BindingPattern,
) -> MagicProgram {
    let idb: BTreeSet<String> = rules
        .iter()
        .map(|r| r.head.predicate.as_str().to_owned())
        .collect();

    // (1) Demand fixpoint: the full variant-keyed (pred → exact adornments) demand set.
    let demanded = demand_fixpoint(rules, &idb, goal, goal_adorn);

    // (2) Collapse: keep only the most-general (⊑-minimal) adornment per predicate. `serve`
    //     maps every demanded adornment to the kept table that answers it.
    let kept: BTreeMap<String, Vec<BindingPattern>> = demanded
        .iter()
        .map(|(pred, codes)| (pred.clone(), minimal_antichain(codes)))
        .collect();
    let served = |pred: &str, pat: BindingPattern| -> BindingPattern {
        // A predicate reached only as an EDB body atom is not in `kept` (it is never
        // demanded); such atoms are never guarded, so `served` is only ever asked about an
        // IDB predicate present in `kept`.
        serve(
            kept.get(pred)
                .expect("a guarded/adorned atom's predicate is a demanded IDB predicate"),
            pat,
        )
    };

    let mut out: Vec<EvalRule> = Vec::new();
    let mut seeds: Vec<EvalAtom> = Vec::new();

    // (3) Seed: the goal's magic fact, keyed on the KEPT table that serves the goal's
    //     adornment (the goal projection re-imposes the goal's own residual). None for an
    //     all-free served goal. This and every other unconditional demand rule below are
    //     asserted into the EDB by the caller as control facts.
    if let Some(s) = magic_seed_atom(goal, served(goal.predicate.as_str(), goal_adorn)) {
        seeds.push(s);
    }

    // (4) Modified rules + magic rules, iterating ONLY the KEPT (most-general) head demands.
    //     Processing the general demand yields the general body demands; every body magic
    //     guard is routed through `served`, so it references only kept magic predicates.
    for (head_pred, kept_pats) in &kept {
        for &head_adorn in kept_pats {
            for (ri, r) in rules
                .iter()
                .enumerate()
                .filter(|(_, r)| r.head.predicate.as_str() == head_pred.as_str())
            {
                let mut bound = head_bound_vars(&r.head, head_adorn);

                // The head guard is the kept head table (`head_adorn` is itself kept, so it
                // serves itself). (2) Modified rule body: head magic guard ++ original body.
                let head_guard = magic_guard_atom(&r.head, head_adorn);
                let mut mod_body: Vec<EvalAtom> = Vec::new();
                if let Some(guard) = &head_guard {
                    mod_body.push(guard.clone());
                }

                // Walk the body, emitting per-IDB-atom magic rules along the SIPS chain.
                let mut prefix: Vec<EvalAtom> = Vec::new();
                for (bi, atom) in r.body.iter().enumerate() {
                    if idb.contains(atom.predicate.as_str()) {
                        // The body atom's exact SIPS adornment, then the KEPT table that
                        // serves it (a superset demand — the residual is discharged by the
                        // original body atom + goal projection).
                        let a = adorn_atom(atom, &bound);
                        let served_a = served(atom.predicate.as_str(), a);
                        // (3) magic rule: magic_served_a :- magic_head, b1..b(i-1) (none when
                        // the served table is all-free — an unrestricted demand needs none).
                        if let Some(magic_head) = magic_guard_atom(atom, served_a) {
                            let mut mbody: Vec<EvalAtom> = Vec::new();
                            if let Some(hg) = &head_guard {
                                mbody.push(hg.clone());
                            }
                            mbody.extend(prefix.iter().cloned());
                            let iri = format!(
                                "{}::magic/{}/{}#{ri}.{bi}",
                                atom.predicate.as_str(),
                                served_a.code(),
                                head_pred
                            );
                            // A leading bound recursive-IDB atom under an all-free head
                            // yields an empty `mbody` (no head guard, empty prefix); its
                            // ground magic head is lifted to a seed rather than dropped.
                            emit_or_seed(magic_head, mbody, iri, &mut out, &mut seeds);
                        }
                    }
                    // The modified rule always keeps the ORIGINAL body atom (a negated atom
                    // stays negated — its `negated` flag rides through the `clone`); the
                    // demand restriction comes from the head guard + the magic rules that gate
                    // which instances are derived, and the original body atom's own
                    // constants/variables discharge any subsumptive residual.
                    mod_body.push(atom.clone());
                    if atom.negated {
                        // NAF binds no SIPS variables. Carry the negated atom (still negated)
                        // into the SIPS `prefix` — so a LATER atom's magic (demand) rule sees
                        // the NAF condition under which it is reached, the negative literal
                        // that can make the transformed program non-stratifiable (recovered
                        // soundly by `eval_with_base_fallback`) — ONLY when it is fully ground
                        // given the bindings so far. A partially-bound negated guard inside a
                        // magic rule is existential NAF, strictly STRONGER than the per-tuple
                        // test, and would UNDER-demand a later atom (dropping needed
                        // instances); omitting it instead only WIDENS demand, always sound.
                        if negated_atom_fully_bound(atom, &bound) {
                            prefix.push(atom.clone());
                        }
                    } else {
                        prefix.push(atom.clone());
                        bind_atom_vars(atom, &mut bound);
                    }
                }

                // The modified rule carries the ORIGINAL rule's builtins: the shared
                // constraint stage evaluates them post-join, generating the head's
                // arithmetic answer (or filtering).  The magic (demand) rules carry NO
                // builtins — magic-sets is sound and complete under ANY sideways-
                // information-passing strategy, so adorning a builtin-bound variable as
                // free merely loosens demand (never changes the goal answers), and for
                // the binary arithmetic fragment the builtin is terminal, so the
                // adornment is in fact exact.
                let iri = format!(
                    "{}::mod/{}#{ri}",
                    r.head.predicate.as_str(),
                    head_adorn.code()
                );
                // A ground fact-rule (empty original body) under an all-free head yields an
                // empty `mod_body` with a ground head — an unconditional fact. Lift this
                // transform control fact to a seed. A builtin-bearing rule is semantic, so
                // retain it for relational-identity evaluation.
                if mod_body.is_empty() && r.builtins.is_empty() {
                    seeds.push(r.head.clone());
                } else {
                    let mut modified = rule(r.head.clone(), mod_body, iri);
                    modified.builtins = r.builtins.clone();
                    out.push(modified);
                }
            }
        }
    }

    // Unconditional transform control facts are seeds, never executable rules. Semantic
    // NAF-only and builtin-only rules are valid: the semi-naive core starts them from the
    // relational identity, so require semantic content rather than a positive driver.
    assert!(
        out.iter()
            .all(|r| !r.body.is_empty() || !r.builtins.is_empty()),
        "magic_transform must lift every unconditional demand-control rule into the seed set"
    );
    // The goal seed and the per-atom/modified demand lifts above can mint the same ground
    // demand fact from more than one emission site; dedup ONCE here, order-preservingly
    // (first-seen kept), rather than guarding every push with an O(N) `contains` scan.
    // `EvalAtom` derives `Debug` but not `Hash`/`Ord` (its `TermValue` operand is external
    // to this crate), so the dedup key is the atom's deterministic `Debug` rendering.
    let mut seen = std::collections::HashSet::new();
    seeds.retain(|s| seen.insert(format!("{s:?}")));
    MagicProgram { rules: out, seeds }
}

/// The VARIANT (per-exact-adornment) magic-sets transformation — the `#[cfg(test)]`
/// byte-identity reference the production [`magic_transform`] is checked against.
///
/// Mints a SEPARATE magic predicate per distinct demanded adornment (no subsumptive collapse):
/// this is the pre-upgrade demand keying, kept only as the A/B oracle proving the subsumptive
/// collapse leaves the answer set byte-identical. It is NOT a production path (greenfield: one
/// production demand rewrite, the subsumptive one).
#[cfg(test)]
fn magic_transform_variant(
    rules: &[EvalRule],
    goal: &EvalAtom,
    goal_adorn: BindingPattern,
) -> MagicProgram {
    let idb: BTreeSet<String> = rules
        .iter()
        .map(|r| r.head.predicate.as_str().to_owned())
        .collect();

    let mut out: Vec<EvalRule> = Vec::new();
    let mut seeds: Vec<EvalAtom> = Vec::new();

    // (1) Seed: the goal's magic fact (none for an ff goal). Every unconditional demand
    //     rule below is likewise lifted into the seed set as control state.
    if let Some(s) = magic_seed_atom(goal, goal_adorn) {
        seeds.push(s);
    }

    // The full variant-keyed (pred → exact adornments) demand set.
    let demanded = demand_fixpoint(rules, &idb, goal, goal_adorn);

    // (2) Modified rules + (3) magic rules, for every demanded (head_pred, exact adorn).
    for (head_pred, codes) in &demanded {
        for adorn_code in codes {
            let head_adorn = BindingPattern::from_code(adorn_code);
            for (ri, r) in rules
                .iter()
                .enumerate()
                .filter(|(_, r)| r.head.predicate.as_str() == head_pred.as_str())
            {
                let mut bound = head_bound_vars(&r.head, head_adorn);

                let head_guard = magic_guard_atom(&r.head, head_adorn);
                let mut mod_body: Vec<EvalAtom> = Vec::new();
                if let Some(guard) = &head_guard {
                    mod_body.push(guard.clone());
                }

                let mut prefix: Vec<EvalAtom> = Vec::new();
                for (bi, atom) in r.body.iter().enumerate() {
                    if idb.contains(atom.predicate.as_str()) {
                        let a = adorn_atom(atom, &bound);
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
                            emit_or_seed(magic_head, mbody, iri, &mut out, &mut seeds);
                        }
                    }
                    mod_body.push(atom.clone());
                    if atom.negated {
                        if negated_atom_fully_bound(atom, &bound) {
                            prefix.push(atom.clone());
                        }
                    } else {
                        prefix.push(atom.clone());
                        bind_atom_vars(atom, &mut bound);
                    }
                }

                let iri = format!("{}::mod/{}#{ri}", r.head.predicate.as_str(), adorn_code);
                if mod_body.is_empty() && r.builtins.is_empty() {
                    seeds.push(r.head.clone());
                } else {
                    let mut modified = rule(r.head.clone(), mod_body, iri);
                    modified.builtins = r.builtins.clone();
                    out.push(modified);
                }
            }
        }
    }

    // The variant mirrors the production transform exactly: unconditional demand-control
    // rules are lifted, while semantic NAF-only and builtin-only rules remain executable.
    assert!(
        out.iter()
            .all(|r| !r.body.is_empty() || !r.builtins.is_empty()),
        "magic_transform_variant must lift every unconditional demand-control rule into the seed set"
    );
    // Identical order-preserving end-of-transform dedup as `magic_transform` — required so
    // the A/B byte-identity oracle test comparing the two transforms' seed sets holds.
    let mut seen = std::collections::HashSet::new();
    seeds.retain(|s| seen.insert(format!("{s:?}")));
    MagicProgram { rules: out, seeds }
}

/// Convert a ground magic seed [`EvalAtom`] into a [`crate::rule_ir::Fact`] for EDB
/// insertion.  The seed is always ground (its terms are goal constants), so this never
/// hits an unbound variable.
fn seed_to_fact(seed: &EvalAtom) -> gmeow_errors::Result<crate::rule_ir::Fact> {
    let to_term = |t: &EvalTerm| match t {
        EvalTerm::ConstNamed(nn) => Ok(TermValue::iri(nn.clone())),
        EvalTerm::ConstLit(term) => Ok(term.clone()),
        EvalTerm::Var(v) => Err(physical_err(format!("magic seed term {v:?} is not ground"))),
    };
    Ok(crate::rule_ir::Fact {
        subject: to_term(&seed.subject)?,
        predicate: seed.predicate.clone(),
        object: to_term(&seed.object)?,
    })
}

// ── Value-generating-recursion termination guard ──────────────────────────────────

/// The transitive (≥1-step) reachability closure of the IDB predicate-dependency graph.
///
/// A node is `head_pred`; an edge `p → q` exists iff some rule with head `p` carries a
/// POSITIVE body atom over the IDB predicate `q`.  The returned map sends each IDB
/// predicate to the set of IDB predicates reachable from it in one or more edges — so
/// `reach[p]` contains `p` exactly when `p` lies on a directed cycle (a self-loop or a
/// larger SCC), and `q ∈ reach[p] ∧ p ∈ reach[q]` iff `p` and `q` are mutually recursive
/// (share an SCC).  Only positive edges count: a negated body atom binds nothing and
/// drives no derivation, so it cannot carry the recursion.
fn idb_reachability<'a>(
    rules: &'a [EvalRule],
    idb: &BTreeSet<&'a str>,
) -> BTreeMap<&'a str, BTreeSet<&'a str>> {
    let mut adj: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for r in rules {
        let h = r.head.predicate.as_str();
        for a in r.body.iter().filter(|a| !a.negated) {
            let p = a.predicate.as_str();
            if idb.contains(p) {
                adj.entry(h).or_default().insert(p);
            }
        }
    }
    let mut reach: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for &n in idb {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut stack: Vec<&str> = adj.get(n).into_iter().flatten().copied().collect();
        while let Some(x) = stack.pop() {
            if seen.insert(x)
                && let Some(succ) = adj.get(x)
            {
                for &y in succ {
                    if !seen.contains(y) {
                        stack.push(y);
                    }
                }
            }
        }
        reach.insert(n, seen);
    }
    reach
}

/// Whether `rule` carries an arithmetic value-generating `is` builtin whose target
/// variable reaches (contributes a value to) the rule head.
///
/// The set of variables that reach the head is seeded with the head's own variables and
/// closed BACKWARD over the `is` builtins: if a builtin's target reaches the head, its
/// operands reach the head too (they are consumed to compute a head-reaching value).  A
/// value-generating `is` whose target lands in that set drives a fresh term into the head
/// — the only way a binary backward rule can invent an unbounded Herbrand value.  A
/// `Compare` builtin has no target and generates nothing, so it never qualifies.
fn is_generator_reaches_head(rule: &EvalRule) -> bool {
    let mut reach: BTreeSet<String> = BTreeSet::new();
    for t in [&rule.head.subject, &rule.head.object] {
        if let EvalTerm::Var(v) = t {
            reach.insert(v.clone());
        }
    }
    loop {
        let mut changed = false;
        for b in &rule.builtins {
            if let QBuiltin::Is {
                target: QTerm::Var(t),
                lhs,
                rhs,
                ..
            } = b
                && reach.contains(t)
            {
                for op in [lhs, rhs] {
                    if let QTerm::Var(v) = op
                        && reach.insert(v.clone())
                    {
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    rule.builtins.iter().any(|b| {
        matches!(
            b,
            QBuiltin::Is {
                target: QTerm::Var(t),
                ..
            } if reach.contains(t)
        )
    })
}

/// Whether any rule in `rules` is potentially non-terminating via arithmetic self-drive.
///
/// A rule is flagged iff ALL of:
///
/// 1. **Its head predicate lies on a dependency cycle** (`reach[head]` contains `head`) —
///    the recursion that a value-generator could feed forever.
/// 2. **It carries a value-generating `is` builtin whose target reaches the head**
///    ([`is_generator_reaches_head`]) — the source of fresh Herbrand terms.
/// 3. **It has NO finite driver**: every POSITIVE body atom is over a relation in the
///    head's own cycle (an IDB predicate mutually recursive with the head).  A body atom
///    over an EDB relation, or over a strictly-lower-stratum IDB predicate (one that
///    cannot reach the head back), is a FINITE driver — it ranges over an already-settled
///    finite set, so the recursion is bounded and terminates.
///
/// This is precise and SOUND: it never flags the terminating list-length shape
/// `len(L,N) :- rest(L,R), len(R,M), N is M+1`, because its `rest(L,R)` body atom is an
/// EDB (non-cyclic) finite driver — condition 3 is false.  It DOES flag a pure self-drive
/// `count(X,S) :- count(X,Y), S is Y+1` whose only body atom is the cyclic head predicate.
/// Over-flagging a comparison-bounded terminating program is acceptable (it routes to the
/// oracle — incomplete, never wrong); under-flagging a genuine hang is not, and the
/// finite-driver test rules out exactly the terminating cases.
fn potentially_nonterminating_arithmetic(rules: &[EvalRule]) -> bool {
    let idb: BTreeSet<&str> = rules.iter().map(|r| r.head.predicate.as_str()).collect();
    let reach = idb_reachability(rules, &idb);
    let in_cycle = |p: &str| reach.get(p).is_some_and(|s| s.contains(p));
    for r in rules {
        let h = r.head.predicate.as_str();
        // (1) head on a cycle.
        if !in_cycle(h) {
            continue;
        }
        // (2) a value-generating `is` reaching the head.
        if !is_generator_reaches_head(r) {
            continue;
        }
        // (3) no finite driver: every positive body atom is cyclic with the head.
        let has_finite_driver = r.body.iter().filter(|a| !a.negated).any(|a| {
            let p = a.predicate.as_str();
            // A finite driver is an EDB relation (not IDB) or a strictly-lower IDB
            // predicate that cannot reach the head back (not mutually recursive).
            !idb.contains(p) || !reach.get(p).is_some_and(|s| s.contains(h))
        });
        if !has_finite_driver {
            return true;
        }
    }
    false
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
        // A structured argument is not a flat constant constraint (structured goals route to
        // the full-FOL resolver, never here).
        QTerm::Var(_) | QTerm::Num(_) | QTerm::Struct(_) => None,
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

/// A reusable, fixed-program incremental query state.
///
/// Counterfactual/conjecture loops clone this base state and apply a small signed EDB
/// revision instead of rebuilding the stable world's demand-restricted least model.
/// The session is deliberately facts-only: backward answer projection consumes only
/// the goal relation and never fabricates provenance.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalQuerySession {
    state: IncrementalSession,
    goal: QAtom,
    goal_predicate: String,
}

impl IncrementalQuerySession {
    /// Apply IRI-only signed changes and project the resulting goal answers.
    ///
    /// This path is unbounded by construction; [`prepare_incremental_query`] declines
    /// a request carrying `max_steps`, leaving it on the existing inline-governed
    /// scratch evaluator. `max_answers` remains a deterministic output cap.
    pub(crate) fn apply_iri_changes(
        &mut self,
        changes: impl IntoIterator<Item = (String, String, String, i64)>,
        max_answers: Option<usize>,
    ) -> gmeow_errors::Result<AnswerSet> {
        self.state.apply(
            changes
                .into_iter()
                .map(|(subject, predicate, object, weight)| SignedFact {
                    fact: Fact {
                        subject: TermValue::iri(subject),
                        predicate,
                        object: TermValue::iri(object),
                    },
                    weight,
                }),
        )?;

        let closure = self.state.closure();
        let mut bindings = project_answers(&closure, &self.goal, &self.goal_predicate);
        let mut answer = AnswerSet {
            bindings: Vec::new(),
            status: BudgetStatus::Ok,
            preservation: crate::result::PreservationClaim::exact(),
            // No StepGovernor ran: this is an unbounded, fully-settled transaction.
            // Empty is the established ungoverned-frontier convention.
            frontier: CompletionFrontier::empty(),
        };
        answer.bindings.append(&mut bindings);
        answer.canonicalize();
        if let Some(max_answers) = max_answers
            && answer.bindings.len() >= max_answers
            && !answer.bindings.is_empty()
        {
            answer.bindings.truncate(max_answers);
            answer.status = BudgetStatus::Partial;
        }
        Ok(answer)
    }
}

/// Prepare the reusable incremental form of an eligible binary positive query.
///
/// `Ok(None)` is an explicit optimization-boundary result, not a semantic refusal:
/// the ordinary native scratch path still decides the request.  The session declines
/// cut, n-ary atoms, NAF, builtins, and step-bounded runs; those paths retain their
/// existing governed or fragment-specific implementation. A leading-bound recursive-IDB
/// body IS supported — its demand is a lifted seed, not a bodyless transformed rule.
pub(crate) fn prepare_incremental_query(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    contract_hash: &str,
    budget: &Budget,
) -> gmeow_errors::Result<Option<IncrementalQuerySession>> {
    if budget.max_steps.is_some() || profile_gate::has_cut(program) || program.goal.atoms.len() != 1
    {
        return Ok(None);
    }
    let goal = &program.goal.atoms[0];
    let binary_eligible = goal.args.len() == 2
        && program.rules.iter().all(|rule| {
            rule.head.args.len() == 2
                && !rule.body.is_empty()
                && rule.body.iter().all(|literal| match literal {
                    QBodyLit::Atom(atom) => atom.args.len() == 2,
                    QBodyLit::Neg(_) | QBodyLit::Builtin(_) | QBodyLit::Cut => false,
                })
        });
    if !binary_eligible {
        // An EDB-only program has no rules and is still eligible.
        if !program.rules.is_empty() {
            return Ok(None);
        }
        if goal.args.len() != 2 {
            return Ok(None);
        }
    }

    let mut rules = Vec::with_capacity(program.rules.len());
    for rule in &program.rules {
        let Ok(head) = atom_of(&rule.head) else {
            return Ok(None);
        };
        let mut body = Vec::with_capacity(rule.body.len());
        for literal in &rule.body {
            let QBodyLit::Atom(atom) = literal else {
                return Ok(None);
            };
            let Ok(atom) = atom_of(atom) else {
                return Ok(None);
            };
            body.push(atom);
        }
        rules.push(EvalRule {
            rule_iri: format!("{}::rule", head.predicate.as_str()),
            head,
            body,
            distinct_pairs: Vec::new(),
            builtins: Vec::new(),
        });
    }

    let Ok(goal_atom) = atom_of(goal) else {
        return Ok(None);
    };
    let transformed = magic_transform(&rules, &goal_atom, goal_adornment(goal));
    // A bodyless positive rule no longer survives the transform — every such demand is
    // lifted into `transformed.seeds` and asserted into the EDB below — so the incremental
    // path now SUPPORTS a leading-bound recursive-IDB body instead of declining it. NAF and
    // builtins remain out of the incremental fragment.
    if transformed
        .rules
        .iter()
        .any(|rule| rule.body.iter().any(|atom| atom.negated) || !rule.builtins.is_empty())
    {
        return Ok(None);
    }

    let source_patterns = incremental_source_patterns(&rules, &goal_atom);
    let mut edb = extract_edb_patterns(foreign, world, &source_patterns)?.facts_sorted();
    for seed in &transformed.seeds {
        edb.push(seed_to_fact(seed)?);
    }
    let state = IncrementalSession::new(contract_hash, edb, &transformed.rules)?;
    Ok(Some(IncrementalQuerySession {
        state,
        goal: goal.clone(),
        goal_predicate: goal_atom.predicate,
    }))
}

/// Resolve `program` against `world` via the native bottom-up engine over a
/// magic-transformed program — the backward leg of the native execution core.
///
/// Parity sibling of [`crate::reference_resolver::resolve`]: the returned [`AnswerSet`]
/// (after `canonicalize`) carries the SAME goal-variable bindings and status as the
/// retained top-down reference for the binary positive corpus. A cut / arithmetic /
/// non-binary input is a declared gap ([`NativeOutcome::Unsupported`]); production dispatch
/// surfaces that typed refusal because no fallback evaluator remains.
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
    /// The (transformed or base) program was decided. `demand_pruning_dropped` is `true`
    /// iff the DEMAND transform was non-stratifiable and the answer came from evaluating
    /// the untransformed base program (full materialization, no demand pruning) — the
    /// honest signal the caller uses to downgrade the answer's preservation claim.
    Decided {
        facts: Vec<Fact>,
        status: BudgetStatus,
        frontier: CompletionFrontier,
        demand_pruning_dropped: bool,
    },
    /// A declared native gap the caller must surface as a refusal.
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
    contract_hash: &str,
    edb: RelationStore,
    transformed_rules: Vec<EvalRule>,
    base_rules: Vec<EvalRule>,
    max_steps: Option<u64>,
    base_edb: impl FnOnce() -> gmeow_errors::Result<RelationStore>,
) -> gmeow_errors::Result<FallbackOutcome> {
    // Enter the type-state plan pipeline for the demand-transformed program.  A magic
    // (demand) transform threads a magic guard — and, under stratified NAF, a negated
    // guard — through the program; a negative edge in that guarded cycle can make the
    // transformed program non-stratifiable even though the UNTRANSFORMED program is
    // stratified.  `stratify()` → `None` is exactly that trigger: fall back to the base
    // rules over a freshly extracted EDB (without the demand seed the base rules never
    // reference); the answer is exact but the demand pruning was dropped, so the caller
    // downgrades the preservation claim.
    let transformed_lookup = super::plan::compile_cached(contract_hash, transformed_rules);
    let Some(transformed_exe) = transformed_lookup.executable else {
        let base_lookup = super::plan::compile_cached(contract_hash, base_rules);
        let Some(base_exe) = base_lookup.executable else {
            // If the BASE program is also non-stratifiable, the program genuinely is — a
            // real declared gap returned to the caller.
            return Ok(FallbackOutcome::Unsupported(
                UnsupportedKind::NonStratifiable,
            ));
        };
        return match evaluate(base_edb()?, base_exe.as_ref(), max_steps)? {
            NativeOutcome::Decided(budgeted) => {
                let frontier = budgeted.frontier();
                Ok(FallbackOutcome::Decided {
                    facts: budgeted.rows,
                    status: budgeted.status,
                    frontier,
                    demand_pruning_dropped: true,
                })
            }
            // A builtin gap in the base program passes through to production dispatch as a
            // typed refusal.
            NativeOutcome::Unsupported(other) => Ok(FallbackOutcome::Unsupported(other)),
        };
    };

    match evaluate(edb, transformed_exe.as_ref(), max_steps)? {
        NativeOutcome::Decided(budgeted) => {
            let frontier = budgeted.frontier();
            Ok(FallbackOutcome::Decided {
                facts: budgeted.rows,
                status: budgeted.status,
                frontier,
                demand_pruning_dropped: false,
            })
        }
        // Any other declared native gap (cut / arithmetic / non-binary) passes through to
        // production dispatch unchanged.
        NativeOutcome::Unsupported(other) => Ok(FallbackOutcome::Unsupported(other)),
    }
}

/// The preservation claim for an answer produced by the base fallback because the demand
/// transform was non-stratifiable.
///
/// The base-fallback answer is complete AND sound (a full stratified materialization of the
/// untransformed program, projected to the goal), so its ANSWERS are exact. What changed is
/// the mechanism: the demand pruning was dropped and the evaluation WIDENED to the full
/// least model. The honest, conservative disclosure of that widening is
/// [`PreservationKind::CompleteOver`] — a complete over-approximation (every true answer is
/// present; the evaluation may have materialized more than the demand slice). It is the
/// correct polarity direction: never `{sound-under}` (which would falsely imply an answer
/// could be MISSING), and no longer a bare `{exact}` (which would hide that the intended
/// demand transform did not run). No new global ledger is invented — this downgrade IS the
/// required honest signal at this layer.
fn demand_pruning_dropped_claim() -> crate::result::PreservationClaim {
    let mut claim = crate::result::PreservationClaim::default();
    claim
        .insert(gmeow_logic_compile::ir::PreservationKind::CompleteOver)
        .expect("CompleteOver is a valid answer-preservation polarity (not ValidationOnly)");
    claim
}

/// Native binary fact evaluation retained for both plain and annotation-carrying
/// answer projection. Keeping the demand transformation and tuple fixpoint here means
/// `dispatch_query` and `dispatch_query_annotated` cannot drift into separate reasoners.
struct BinaryEvaluation {
    facts: Vec<Fact>,
    status: BudgetStatus,
    frontier: CompletionFrontier,
    demand_pruning_dropped: bool,
    goal_atom: EvalAtom,
    base_rules: Vec<EvalRule>,
    executed_rules: Vec<EvalRule>,
    base_edb_facts: Vec<Fact>,
    control_predicates: BTreeSet<String>,
}

type AnnotatedRows<E> = BTreeMap<Binding, (E, Vec<AnnotationDerivation<E>>)>;

fn evaluate_binary_under(
    contract_hash: &str,
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    budget: &Budget,
    goal: &QAtom,
) -> gmeow_errors::Result<NativeOutcome<BinaryEvaluation>> {
    let mut rules: Vec<EvalRule> = Vec::with_capacity(program.rules.len());
    for source_rule in &program.rules {
        if source_rule
            .body
            .iter()
            .any(|literal| matches!(literal, QBodyLit::Cut))
        {
            return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut));
        }
        let head = match atom_of(&source_rule.head) {
            Ok(atom) => atom,
            Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
        };
        let mut body = Vec::new();
        let mut builtins = Vec::new();
        for literal in &source_rule.body {
            match literal {
                QBodyLit::Atom(atom) => match atom_of(atom) {
                    Ok(atom) => body.push(atom),
                    Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
                },
                QBodyLit::Neg(atom) => match atom_of(atom) {
                    Ok(atom) => body.push(EvalAtom {
                        negated: true,
                        ..atom
                    }),
                    Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
                },
                QBodyLit::Builtin(builtin) => builtins.push(builtin_of(builtin)),
                QBodyLit::Cut => unreachable!("cut handled before binary rule lowering"),
            }
        }
        if negated_body_flounders(&body, &builtins) {
            return Ok(NativeOutcome::Unsupported(UnsupportedKind::Floundering));
        }
        let rule_iri = format!("{}::rule", head.predicate.as_str());
        rules.push(EvalRule {
            head,
            body,
            rule_iri,
            distinct_pairs: Vec::new(),
            builtins,
        });
    }

    if budget.max_steps.is_none() && potentially_nonterminating_arithmetic(&rules) {
        return Ok(NativeOutcome::Unsupported(
            UnsupportedKind::NonTerminatingArithmetic,
        ));
    }

    let goal_atom = match atom_of(goal) {
        Ok(atom) => atom,
        Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
    };
    let transformed = magic_transform(&rules, &goal_atom, goal_adornment(goal));
    let mut control_predicates: BTreeSet<String> = transformed
        .rules
        .iter()
        .filter(|rule| rule.rule_iri.contains("::magic/"))
        .map(|rule| rule.head.predicate.clone())
        .collect();
    for seed in &transformed.seeds {
        control_predicates.insert(seed.predicate.clone());
    }

    let source_patterns = binary_source_patterns(&rules, &goal_atom);
    let mut edb = extract_edb_patterns(foreign, world, &source_patterns)?;
    let base_edb_facts = edb.facts_sorted();
    for seed in &transformed.seeds {
        let fact = seed_to_fact(seed)?;
        edb.insert(&fact.predicate, &fact.subject, &fact.object);
    }
    let transformed_rules = transformed.rules;
    let base_rules = rules;
    let outcome = eval_with_base_fallback(
        contract_hash,
        edb,
        transformed_rules.clone(),
        base_rules.clone(),
        budget.max_steps,
        || extract_edb_patterns(foreign, world, &source_patterns),
    )?;
    match outcome {
        FallbackOutcome::Decided {
            facts,
            status,
            frontier,
            demand_pruning_dropped,
        } => Ok(NativeOutcome::Decided(BinaryEvaluation {
            facts,
            status,
            frontier,
            demand_pruning_dropped,
            goal_atom,
            base_rules: base_rules.clone(),
            executed_rules: if demand_pruning_dropped {
                base_rules
            } else {
                transformed_rules
            },
            base_edb_facts,
            control_predicates: if demand_pruning_dropped {
                BTreeSet::new()
            } else {
                control_predicates
            },
        })),
        FallbackOutcome::Unsupported(kind) => Ok(NativeOutcome::Unsupported(kind)),
    }
}

/// Resolve `program`'s single backward goal against `world` via the native magic-sets core.
///
/// # Native-authority contract
///
/// The native core is AUTHORITATIVE for every request it decides: a
/// [`NativeOutcome::Decided`] answer is the whole answer (exact, or an honestly-downgraded
/// complete over-approximation on the native base fallback). A
/// [`NativeOutcome::Unsupported`] gap — cut, arithmetic residue, a non-binary shape, a
/// genuinely non-stratifiable program, or a floundering NAF goal — is surfaced by production
/// [`crate::dispatch::dispatch_query`] as a typed hard failure. There is no external oracle,
/// secondary evaluator, or demotion route. Stratified negation stays entirely inside this
/// native path (decided or a declared gap); it is never a silent drop.
pub(crate) fn resolve_native(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    budget: &Budget,
) -> gmeow_errors::Result<NativeOutcome<AnswerSet>> {
    // A bare-`QProgram` entry owns no structured-term arena; a parsed program is flat, so the
    // fresh DAG is unused. A caller holding a STRUCTURED program interned into a live DAG calls
    // `resolve_native_under` directly, passing that owning arena so the `Struct` nodes resolve.
    let mut dag = super::term_dag::TermDag::new();
    resolve_native_under(
        "gmeow-backward-unscoped-v1",
        foreign,
        world,
        program,
        budget,
        &mut dag,
    )
}

/// Contract-scoped form used by production dispatch.
///
/// The contract hash participates in the immutable plan identity; callers that change
/// profile/resource semantics cannot accidentally reuse a plan compiled under an older
/// contract even when their lowered rule text happens to match.
///
/// `dag` is the structured-term arena a STRUCTURED program's `Struct` nodes were interned into
/// — the caller's OWNING arena, so the full-FOL resolver resolves against genuine nodes rather
/// than a fresh (empty) arena that rejects every node. A flat program never touches `dag`.
pub(crate) fn resolve_native_under(
    contract_hash: &str,
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    budget: &Budget,
    dag: &mut super::term_dag::TermDag,
) -> gmeow_errors::Result<NativeOutcome<AnswerSet>> {
    // (0) Gate cut (reuse the structural detector the dispatch gate uses).  Arithmetic
    // is no longer a whole-program gap — the closed builtin set is evaluated natively;
    // any residual (unbound operand / ÷0 / overflow) surfaces as a gap DURING the
    // fixpoint (see `seminaive::evaluate`).  Profile confinement is upstream in
    // `dispatch::dispatch_query` (`profile_gate::check_builtin_profile`), unchanged.
    if profile_gate::has_cut(program) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut));
    }

    // ── Structured (full-FOL) routing ────────────────────────────────────────────────
    //
    // A program carrying ANY structured (`QTerm::Struct`) argument — a function-symbol
    // (compound) term the flat binary/generic store cannot represent — routes to the
    // full-FOL resolver (`resolve_fol`): SLG tabling over compound terms with three-valued
    // well-founded negation, proof-carrying answers. The parser produces only flat terms, so
    // this branch never fires for a parsed production program — the flat path below stays
    // byte-identical. A structured program travels with the DAG its `Struct` nodes were
    // interned into, and the caller passes that OWNING arena as `dag`; `resolve_native_fol`
    // validates arena identity and resolves against the genuine nodes (a foreign arena is a
    // typed gap, never a fabricated answer).
    if super::resolve_fol::program_is_structured(program) {
        return super::resolve_fol::resolve_native_fol(dag, program, budget);
    }

    // The backward leg handles a SINGLE goal atom; a multi-atom conjunctive goal is a
    // declared gap on either path.
    if program.goal.atoms.len() != 1 {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom));
    }
    let goal = &program.goal.atoms[0];

    // ── Arity-eligibility dispatch (mirrors the forward oracle's binary/generic split
    //    at `crate::oracle`'s `binary_eligible`) ────────────────────────────────────
    //
    // The binary fragment stays on the byte-identical binary magic path below (it carries
    // the arithmetic-builtin seminaive constraint stage the binary corpus depends on).
    // ANY atom of arity != 2 — the goal, a rule head, or a rule body atom — routes to the
    // arity-generic n-ary evaluator, which resolves the real predicate-as-data
    // `triple(s, p, o, w)` shape the binary store cannot query.  A builtin literal is not
    // an atom (it never carries an argument position) and never disqualifies the binary
    // path: the binary arithmetic corpus stays binary.
    let binary_eligible = goal.args.len() == 2
        && program.rules.iter().all(|r| {
            r.head.args.len() == 2
                && r.body.iter().all(|lit| match lit {
                    QBodyLit::Atom(a) | QBodyLit::Neg(a) => a.args.len() == 2,
                    QBodyLit::Builtin(_) | QBodyLit::Cut => true,
                })
        });
    if !binary_eligible {
        return super::magic_generic::resolve_native_generic(foreign, world, program, budget);
    }

    let evaluation =
        match evaluate_binary_under(contract_hash, foreign, world, program, budget, goal)? {
            NativeOutcome::Decided(evaluation) => evaluation,
            NativeOutcome::Unsupported(kind) => return Ok(NativeOutcome::Unsupported(kind)),
        };
    let BinaryEvaluation {
        facts,
        status: fixpoint_status,
        frontier,
        demand_pruning_dropped,
        goal_atom,
        ..
    } = evaluation;

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

    // Preservation: `{exact}` on the demand-transformed happy path; a downgraded claim
    // when the transform was non-stratifiable and the answer came from the base fallback
    // (the answer set is still complete and sound, but the demand pruning was dropped —
    // recorded honestly rather than left as a silent free-transform assumption).
    let preservation = if demand_pruning_dropped {
        demand_pruning_dropped_claim()
    } else {
        crate::result::PreservationClaim::exact()
    };
    let mut answer = AnswerSet {
        bindings,
        status,
        preservation,
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

fn public_derivations<E: Clone>(
    world: &str,
    derivations: &[super::annotation::PhysicalAnnotationDerivation<E>],
    control_predicates: &BTreeSet<String>,
) -> Vec<AnnotationDerivation<E>> {
    derivations
        .iter()
        .map(|derivation| AnnotationDerivation {
            rule_iri: derivation.rule_iri.clone(),
            sources: derivation
                .sources
                .iter()
                .filter(|(_, predicate, _)| !control_predicates.contains(predicate))
                .map(|(subject, predicate, object)| AnnotatedFactKey {
                    graph: world.to_owned(),
                    subject: subject.clone(),
                    predicate: predicate.clone(),
                    object: object.clone(),
                })
                .collect(),
            tuple_sources: Vec::new(),
            annotation: derivation.annotation.clone(),
        })
        .collect()
}

/// Contract-scoped score-carrying counterpart of [`resolve_native_under`].
///
/// Tuple membership and opaque annotation equations are produced by one
/// demand-transformed physical fixpoint. Magic predicates remain unit-valued control
/// tuples so the demand rewrite never double-counts a scored premise.
pub(crate) fn resolve_native_annotated_under<A, F>(
    contract_hash: &str,
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
    if profile_gate::has_cut(program) {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut));
    }
    if program.goal.atoms.len() != 1 {
        return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonBinaryAtom));
    }
    let goal = &program.goal.atoms[0];
    let binary_eligible = goal.args.len() == 2
        && program.rules.iter().all(|rule| {
            rule.head.args.len() == 2
                && rule.body.iter().all(|literal| match literal {
                    QBodyLit::Atom(atom) | QBodyLit::Neg(atom) => atom.args.len() == 2,
                    QBodyLit::Builtin(_) | QBodyLit::Cut => true,
                })
        });
    if !binary_eligible {
        return super::magic_generic::resolve_native_generic_annotated(
            foreign, world, program, budget, annotation,
        );
    }

    let mut base_rules = Vec::with_capacity(program.rules.len());
    for source_rule in &program.rules {
        if source_rule
            .body
            .iter()
            .any(|literal| matches!(literal, QBodyLit::Cut))
        {
            return Ok(NativeOutcome::Unsupported(UnsupportedKind::Cut));
        }
        let head = match atom_of(&source_rule.head) {
            Ok(atom) => atom,
            Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
        };
        let mut body = Vec::new();
        let mut builtins = Vec::new();
        for literal in &source_rule.body {
            match literal {
                QBodyLit::Atom(atom) => match atom_of(atom) {
                    Ok(atom) => body.push(atom),
                    Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
                },
                QBodyLit::Neg(atom) => match atom_of(atom) {
                    Ok(atom) => body.push(EvalAtom {
                        negated: true,
                        ..atom
                    }),
                    Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
                },
                QBodyLit::Builtin(builtin) => builtins.push(builtin_of(builtin)),
                QBodyLit::Cut => unreachable!("cut handled above"),
            }
        }
        if negated_body_flounders(&body, &builtins) {
            return Ok(NativeOutcome::Unsupported(UnsupportedKind::Floundering));
        }
        let rule_iri = format!("{}::rule", head.predicate.as_str());
        base_rules.push(EvalRule {
            head,
            body,
            rule_iri,
            distinct_pairs: Vec::new(),
            builtins,
        });
    }
    if budget.max_steps.is_none() && potentially_nonterminating_arithmetic(&base_rules) {
        return Ok(NativeOutcome::Unsupported(
            UnsupportedKind::NonTerminatingArithmetic,
        ));
    }
    let goal_atom = match atom_of(goal) {
        Ok(atom) => atom,
        Err(kind) => return Ok(NativeOutcome::Unsupported(kind)),
    };
    let transformed = magic_transform(&base_rules, &goal_atom, goal_adornment(goal));
    let mut control_predicates = transformed
        .rules
        .iter()
        .filter(|rule| rule.rule_iri.contains("::magic/"))
        .map(|rule| rule.head.predicate.clone())
        .collect::<BTreeSet<_>>();
    for seed in &transformed.seeds {
        control_predicates.insert(seed.predicate.clone());
    }
    let source_patterns = binary_source_patterns(&base_rules, &goal_atom);
    let base_edb_facts = extract_edb_patterns(foreign, world, &source_patterns)?.facts_sorted();
    let mut edb = base_edb_facts.clone();
    for seed in &transformed.seeds {
        edb.push(seed_to_fact(seed)?);
    }
    edb.sort_by_key(Fact::key);

    let (executed_rules, demand_pruning_dropped) = {
        let lookup = super::plan::compile_cached(contract_hash, transformed.rules.clone());
        if lookup.executable.is_some() {
            (transformed.rules, false)
        } else {
            let base_lookup = super::plan::compile_cached(contract_hash, base_rules.clone());
            if base_lookup.executable.is_none() {
                return Ok(NativeOutcome::Unsupported(UnsupportedKind::NonStratifiable));
            }
            (base_rules.clone(), true)
        }
    };
    let lookup = super::plan::compile_cached(contract_hash, executed_rules);
    let executable = lookup
        .executable
        .expect("the selected annotated binary plan was checked executable");
    let certification = super::annotation::certify_query(&base_rules, annotation.contract)?;
    let mut seed_annotations = BTreeMap::new();
    for fact in &base_edb_facts {
        let fact_annotation = (annotation.annotation_for)(AnnotationFactRef {
            world,
            subject: &fact.subject,
            predicate: &fact.predicate,
            object: &fact.object,
        })
        .unwrap_or_else(|| annotation.algebra.one());
        seed_annotations.insert(fact.key(), fact_annotation);
    }
    for fact in &edb {
        if control_predicates.contains(&fact.predicate) {
            seed_annotations.insert(fact.key(), annotation.algebra.one());
        }
    }
    let annotated = super::annotation::evaluate_annotations(
        world,
        &edb,
        executable.as_ref(),
        super::annotation::AnnotationExecution::new(
            budget.max_steps,
            &seed_annotations,
            &control_predicates,
            annotation.algebra,
            annotation.contract,
        ),
    )?;

    let mut rows: AnnotatedRows<A::Element> = BTreeMap::new();
    for fact in &annotated.facts {
        let Some(binding) = project_answers(
            std::slice::from_ref(fact),
            goal,
            goal_atom.predicate.as_str(),
        )
        .into_iter()
        .next() else {
            continue;
        };
        let key = fact.key();
        let fact_annotation = annotated
            .annotations
            .get(&key)
            .cloned()
            .unwrap_or_else(|| annotation.algebra.zero());
        let derivations = public_derivations(
            world,
            annotated
                .derivations
                .get(&key)
                .map_or(&[][..], Vec::as_slice),
            &control_predicates,
        );
        match rows.entry(binding) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert((fact_annotation, derivations));
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let (combined, lineage) = entry.get_mut();
                *combined = annotation.algebra.add(combined, &fact_annotation)?;
                lineage.extend(derivations);
            }
        }
    }

    let mut answers: Vec<AnnotatedAnswer<A::Element>> = rows
        .into_iter()
        .map(|(binding, (annotation, derivations))| AnnotatedAnswer {
            binding,
            annotation,
            derivations,
        })
        .collect();
    let mut status = annotated.status;
    if let Some(max_answers) = budget.max_answers
        && answers.len() >= max_answers
        && !answers.is_empty()
    {
        answers.truncate(max_answers);
        status = BudgetStatus::Partial;
    }
    if status == BudgetStatus::Exhausted
        && answers.is_empty()
        && annotated
            .frontier
            .saturated_preds
            .contains(goal_atom.predicate.as_str())
    {
        status = BudgetStatus::Ok;
    }

    Ok(NativeOutcome::Decided(AnnotatedAnswerSet {
        answers,
        status,
        preservation: if demand_pruning_dropped {
            demand_pruning_dropped_claim()
        } else {
            crate::result::PreservationClaim::exact()
        },
        frontier: annotated.frontier,
        certification,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::plan::Parsed;
    use crate::query_ir::parse_query_program;
    use crate::reference_resolver;
    use crate::seam::WorldFactSnapshot;
    use crate::store::WorldStore;

    const W: &str = "http://logic.test/world/magic";
    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const BASE: &str = "https://example.org/";

    /// Drive the type-state plan pipeline for a stratifiable test program — the only path
    /// to the `Executable` the backward `evaluate` executor accepts.
    fn exe(rules: &[EvalRule]) -> crate::physical::plan::Executable {
        Parsed::uncached(rules)
            .stratify()
            .expect("stratifiable test program")
            .plan()
            .into_executable()
    }

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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();

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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let mut edb = extract_edb(&foreign, &world_nn).unwrap();
        for seed in &transformed.seeds {
            let f = seed_to_fact(seed).unwrap();
            edb.insert(&f.predicate, &f.subject, &f.object);
        }
        let facts = match evaluate(edb, &exe(&transformed.rules), None).unwrap() {
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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

    // NOTE: a non-binary goal atom is NO LONGER a binary-path `Unsupported(NonBinaryAtom)`
    // gap — the arity-eligibility dispatch in `resolve_native` routes it to the arity-generic
    // n-ary evaluator instead (see `super::magic_generic`, where the `triple(s, p, o, w)`
    // predicate-as-data resolution and its demand-provenance coverage live). Only a
    // multi-atom conjunctive goal remains a declared gap on the binary leg.

    // ── Value-generating-recursion termination guard ─────────────────────────────
    //
    // Over the finite triple EDB a pure-Datalog backward program always terminates; the
    // ONLY divergence source is an arithmetic `is` value-generator inside an IDB cycle
    // with no finite driver.  `potentially_nonterminating_arithmetic` flags EXACTLY that
    // shape, and `resolve_native` returns a typed refusal when `max_steps` is None (no
    // hang possible), evaluating normally when a step budget can cut the recursion.

    /// A binary self-drive `count(X,S) :- count(X,Y), S is Y+1` (seeded from an EDB
    /// `seed(a,a)` via a base rule) has NO finite driver in its recursive rule — its only
    /// positive body atom is the cyclic head predicate `count`, and the `is` generates a
    /// fresh successor forever.  With no step budget that is an unbounded hang, so the
    /// native core refuses it as `NonTerminatingArithmetic`.
    fn self_drive_program() -> String {
        format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:count(X, 0) :- ex:seed(X, X).\n\
             ex:count(X, S) :- ex:count(X, Y), S is Y + 1.\n\
             ?- ex:count(ex:a, N).\n"
        )
    }

    #[test]
    fn magic_value_generating_self_drive_is_unsupported_without_budget() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}a"),
            &format!("{BASE}seed"),
            &format!("{BASE}a"),
        )]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = parse_query_program(&self_drive_program()).unwrap();
        // No max_steps ⇒ the guard fires (an unbounded hang would otherwise occur).
        let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
        assert!(
            matches!(
                outcome,
                NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingArithmetic)
            ),
            "a value-generating self-drive with no finite driver and no budget must be \
             Unsupported(NonTerminatingArithmetic): {outcome:?}"
        );
    }

    #[test]
    fn magic_value_generating_self_drive_is_budgeted_partial_prefix() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}a"),
            &format!("{BASE}seed"),
            &format!("{BASE}a"),
        )]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = parse_query_program(&self_drive_program()).unwrap();
        // WITH a step budget the guard is bypassed: the StepGovernor cuts the otherwise-
        // infinite recursion deterministically, yielding a SOUND partial prefix.
        let budget = Budget {
            max_steps: Some(3),
            ..Default::default()
        };
        let cut = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
        assert_eq!(
            cut.status,
            BudgetStatus::Exhausted,
            "a budgeted value-generator is cut mid-recursion ⇒ Exhausted: {cut:?}"
        );
        // Every answer is a genuine `count(a, k)` for a distinct integer k — sound
        // (present in the infinite least model), and the governor cut it to a finite set.
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for b in &cut.bindings {
            let n = &b["N"];
            assert!(
                n.contains("XMLSchema#integer"),
                "each answer binds N to an integer successor: {n}"
            );
            assert!(seen.insert(n.clone()), "successors are distinct: {n}");
        }
        assert!(
            !cut.bindings.is_empty() && cut.bindings.len() <= 4,
            "a 3-step budget admits a small finite prefix, not the infinite model: {cut:?}"
        );
    }

    #[test]
    fn magic_finite_driver_arithmetic_is_not_flagged() {
        // The list-length program is in an IDB cycle (len→len) WITH arithmetic, but its
        // recursive rule carries the non-cyclic EDB body atom `rdf:rest(L,R)` — a finite
        // driver.  The guard must NOT flag it (condition 3 is false), so it is decided
        // natively even with no step budget.  This is the direct guard-precision check
        // complementing `magic_binary_arithmetic_is_decided_natively`.
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}l0"),
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
        )]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, 'http://www.w3.org/1999/02/22-rdf-syntax-ns#').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        // Budget::default() ⇒ max_steps None ⇒ the guard is ACTIVE. A finite-driver
        // program must survive it and decide.
        let outcome = resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap();
        assert!(
            matches!(outcome, NativeOutcome::Decided(_)),
            "the finite-driver len program must NOT be flagged non-terminating: {outcome:?}"
        );
    }

    // ── Budget: max_answers truncation parity ────────────────────────────────────

    #[test]
    fn magic_budget_max_answers_matches_reference() {
        let (store, world_nn) = tc_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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

    /// The step budget is charged at the SINGLE committed-derivation counting point
    /// (`StepGovernor::charge` in `eval_stratum_fixpoint`): `max_steps = n` charges EXACTLY
    /// `n` committed derivations before stamping `Exhausted`, and the returned bindings are
    /// a SOUND prefix — a strict subset of the unbudgeted answer set, every member of which
    /// is genuinely in the least model.  Asserted across several `n`.
    #[test]
    fn magic_budget_single_counting_point_exact_charge_and_sound_prefix() {
        let (store, world_nn) = tc_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = tc_program();

        let full: BTreeSet<String> =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap())
                .bindings
                .iter()
                .map(|b| b["Y"].clone())
                .collect();
        assert_eq!(full.len(), 3);

        for n in 1..=3u64 {
            let budget = Budget {
                max_steps: Some(n),
                ..Default::default()
            };
            let cut = decided(resolve_native(&foreign, &world_nn, &prog, &budget).unwrap());
            // Every cut answer is sound (present in the full least model).
            let got: BTreeSet<String> = cut.bindings.iter().map(|b| b["Y"].clone()).collect();
            assert!(
                got.is_subset(&full),
                "n={n}: cut answers must be a sound subset of the full model: {got:?} ⊄ {full:?}"
            );
            // The tc closure needs more than 3 committed derivations (magic seeds +
            // ancestor facts), so every n in 1..=3 cuts mid-fixpoint.
            assert_eq!(
                cut.status,
                BudgetStatus::Exhausted,
                "n={n}: below the completion cost ⇒ Exhausted"
            );
            // The single counting point charged EXACTLY n derivations: on `Exhausted` the
            // governor stopped the instant `consumed == n` (spent-before-commit).
            assert_eq!(
                cut.frontier.consumed_steps, n,
                "n={n}: exactly n committed derivations charged at the single counting point"
            );
        }
    }

    /// Budget composition is a stable status matrix at the `resolve_native` surface,
    /// unchanged by the arity-generic dispatch rewrites: a generous budget completes
    /// (`Ok`), a tight `max_steps` cuts (`Exhausted`), and a reached `max_answers` cap is
    /// `Partial` (taking precedence over any concurrent step cut).  This locks the budget
    /// transfer through the profile-gated backward leg.
    #[test]
    fn magic_budget_composition_status_matrix() {
        let (store, world_nn) = tc_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = tc_program();

        // Ok: a generous step budget, no answer cap ⇒ the fixpoint completes.
        let ok = decided(
            resolve_native(
                &foreign,
                &world_nn,
                &prog,
                &Budget {
                    max_steps: Some(1_000_000),
                    max_answers: None,
                },
            )
            .unwrap(),
        );
        assert_eq!(ok.status, BudgetStatus::Ok, "generous budget ⇒ Ok");
        assert_eq!(ok.bindings.len(), 3);

        // Exhausted: a tight step budget cuts before the fixpoint settles.
        let exhausted = decided(
            resolve_native(
                &foreign,
                &world_nn,
                &prog,
                &Budget {
                    max_steps: Some(1),
                    max_answers: None,
                },
            )
            .unwrap(),
        );
        assert_eq!(
            exhausted.status,
            BudgetStatus::Exhausted,
            "tight max_steps ⇒ Exhausted"
        );

        // Partial: a reached answer cap overrides even a concurrent step cut.
        let partial = decided(
            resolve_native(
                &foreign,
                &world_nn,
                &prog,
                &Budget {
                    max_steps: Some(1),
                    max_answers: Some(1),
                },
            )
            .unwrap(),
        );
        assert_eq!(
            partial.status,
            BudgetStatus::Partial,
            "a reached max_answers cap ⇒ Partial (precedence over the step cut)"
        );
        assert_eq!(partial.bindings.len(), 1);
    }

    /// A step cut is DETERMINISTIC on the backward leg: the same intermediate budget
    /// yields byte-identical bindings and status run-to-run (the fixpoint cut is the Nth
    /// FactKey-sorted committed winner, and `project_answers`+`canonicalize` is a
    /// deterministic function of the fact cut).
    #[test]
    fn magic_budget_max_steps_is_deterministic() {
        let (store, world_nn) = tc_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
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
                &TermValue::iri(x.clone()),
                &TermValue::iri(y.clone()),
            );
            Ok(edb)
        };
        // The transformed EDB is irrelevant: the transform is non-stratifiable, so
        // `evaluate` short-circuits before touching it.
        let out = eval_with_base_fallback(
            "fallback-test",
            RelationStore::new(),
            transformed,
            base,
            None,
            base_edb,
        )
        .expect("fallback must not error");
        let FallbackOutcome::Decided {
            facts,
            status,
            demand_pruning_dropped,
            ..
        } = out
        else {
            panic!("expected the base fallback to decide, got a declared gap");
        };
        assert_eq!(
            status,
            BudgetStatus::Ok,
            "the base fixpoint runs to its natural end"
        );
        // The base fallback fired: the demand pruning was dropped, and the caller must be
        // told so it can downgrade the answer's preservation claim from `{exact}`.
        assert!(
            demand_pruning_dropped,
            "a base-fallback decision must flag that demand pruning was dropped"
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
            "fallback-both-gap-test",
            RelationStore::new(),
            transformed,
            base,
            None,
            || Ok(RelationStore::new()),
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

    // ── Stratified negation-as-failure (backward surface) ────────────────────────

    /// The `unsupported` kind of a native outcome, or a panic if it decided.
    fn unsupported_kind(outcome: NativeOutcome<AnswerSet>) -> UnsupportedKind {
        match outcome {
            NativeOutcome::Unsupported(k) => k,
            NativeOutcome::Decided(a) => panic!("expected Unsupported, got Decided({a:?})"),
        }
    }

    /// A small reachability world: edges a→b, b→c (so a reaches b and c, b reaches c), plus
    /// a `node(v, v)` self-loop domain marker for a, b, c (a binary encoding of the vertex
    /// set so the whole program stays on the binary backward path).
    fn reachability_world() -> (WorldStore, String) {
        make_world(&[
            (
                &format!("{BASE}a"),
                &format!("{BASE}edge"),
                &format!("{BASE}b"),
            ),
            (
                &format!("{BASE}b"),
                &format!("{BASE}edge"),
                &format!("{BASE}c"),
            ),
            (
                &format!("{BASE}a"),
                &format!("{BASE}node"),
                &format!("{BASE}a"),
            ),
            (
                &format!("{BASE}b"),
                &format!("{BASE}node"),
                &format!("{BASE}b"),
            ),
            (
                &format!("{BASE}c"),
                &format!("{BASE}node"),
                &format!("{BASE}c"),
            ),
        ])
    }

    // (a) A stratified-negation program whose BASE is stratifiable: the native core decides
    // it with the correct hand-computed answer set. `reachable` is the transitive closure of
    // `edge`; `unreachable(X, Y)` holds for domain vertices with no path X ⇝ Y.
    #[test]
    fn magic_stratified_negation_reachability_decides_correctly() {
        let (store, world_nn) = reachability_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:reachable(X, Y) :- ex:edge(X, Y).\n\
             ex:reachable(X, Y) :- ex:edge(X, Z), ex:reachable(Z, Y).\n\
             ex:unreachable(X, Y) :- ex:node(X, X), ex:node(Y, Y), \\+ ex:reachable(X, Y).\n\
             ?- ex:unreachable(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let native =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());

        // `a` reaches {b, c}; the domain is {a, b, c}; so `a` is unreachable only to `a`
        // itself (no self-loop edge). The sole answer is Y = a.
        let ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert_eq!(
            ys,
            vec![format!("<{BASE}a>").as_str()],
            "unreachable(a, Y) must be exactly {{a}}: {native:?}"
        );
        assert_eq!(native.status, BudgetStatus::Ok);
        // The demand transform of THIS program stays stratifiable (`unreachable` negates
        // `reachable`, which does not reach back), so the answer is fully demand-pruned and
        // its preservation is `{exact}` — no base fallback, nothing dropped.
        assert!(
            native
                .preservation
                .polarities
                .contains(&gmeow_logic_compile::ir::PreservationKind::Exact),
            "a stratifiable-transform answer is exact: {:?}",
            native.preservation
        );
    }

    #[test]
    fn annotated_stratified_naf_scores_positive_support_and_treats_absence_as_unit() {
        let (store, world_nn) = reachability_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:reachable(X, Y) :- ex:edge(X, Y).\n\
             ex:reachable(X, Y) :- ex:edge(X, Z), ex:reachable(Z, Y).\n\
             ex:unreachable(X, Y) :- ex:node(X, X), ex:node(Y, Y), \\+ ex:reachable(X, Y).\n\
             ?- ex:unreachable(ex:a, Y).\n"
        );
        let program = parse_query_program(&src).unwrap();
        let contract = crate::annotation::AnnotationContract::exact();
        let request = AnnotationRequest::new(
            &crate::provenance::ZWeightSemiring,
            &contract,
            |fact: AnnotationFactRef<'_>| (fact.predicate == format!("{BASE}node")).then_some(2),
        );
        let answer = match resolve_native_annotated_under(
            "annotated-naf-one-pass",
            &foreign,
            &world_nn,
            &program,
            &Budget::default(),
            &request,
        )
        .unwrap()
        {
            NativeOutcome::Decided(answer) => answer,
            NativeOutcome::Unsupported(kind) => panic!("unexpected NAF refusal: {kind:?}"),
        };

        assert_eq!(
            answer.certification.query_class,
            crate::annotation::AnnotationQueryClass::StratifiedNaf
        );
        assert_eq!(answer.answers.len(), 1);
        assert_eq!(answer.answers[0].binding["Y"], format!("<{BASE}a>"));
        assert_eq!(
            answer.answers[0].annotation, 4,
            "two positive node premises: 2*2"
        );
        let direct = answer.answers[0]
            .derivations
            .iter()
            .find(|derivation| derivation.sources.len() == 2)
            .expect("NAF answer keeps positive support only");
        assert_eq!(direct.annotation, 4);
    }

    // (a, downgrade) A stratified-negation program whose BASE is stratifiable but whose
    // DEMAND transform is NOT: a negated recursive IDB atom placed before its positive use
    // puts a negative literal inside a magic (demand) rule, breaking the transform's
    // stratification. `eval_with_base_fallback` recovers the SOUND answer from the base
    // program (full materialization), and the answer's preservation is honestly downgraded
    // from `{exact}` to `{complete-over}` to record that the demand pruning was dropped.
    //
    // `asym(X, Y)`: X reaches Y but Y does not reach X (asymmetric reachability). Over the
    // chain a→b→c, `asym(a, Y)` = {b, c}. Correctness of this answer set is the primary
    // assertion; the preservation downgrade is the honest re-stratify signal.
    #[test]
    fn magic_negation_transform_nonstratifiable_falls_back_correctly_and_downgrades() {
        let (store, world_nn) = reachability_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:r(X, Y) :- ex:edge(X, Y).\n\
             ex:r(X, Y) :- ex:edge(X, Z), ex:r(Z, Y).\n\
             ex:asym(X, Y) :- ex:node(X, X), ex:node(Y, Y), \\+ ex:r(Y, X), ex:r(X, Y).\n\
             ?- ex:asym(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let native =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());

        let mut ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
        ys.sort_unstable();
        assert_eq!(
            ys,
            [format!("<{BASE}b>"), format!("<{BASE}c>")]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            "asym(a, Y) must be exactly {{b, c}} (full-materialization ground truth): {native:?}"
        );
        assert_eq!(native.status, BudgetStatus::Ok);
        // The transform was non-stratifiable ⇒ base fallback ⇒ demand pruning dropped ⇒ the
        // preservation is downgraded from `{exact}` to the conservative `{complete-over}`.
        assert_eq!(
            native.preservation.polarities,
            std::iter::once(gmeow_logic_compile::ir::PreservationKind::CompleteOver).collect(),
            "a base-fallback answer downgrades to a single {{complete-over}} polarity: {:?}",
            native.preservation
        );
        assert!(
            !native
                .preservation
                .polarities
                .contains(&gmeow_logic_compile::ir::PreservationKind::Exact),
            "the downgraded claim must NOT still assert exact"
        );
    }

    // (b) A genuinely non-stratifiable program (a negative cycle p ⇄ q at the BASE level,
    // over binary atoms with the negated vars range-restricted by `e`): both the demand
    // transform AND the base are non-stratifiable, so the native core declares the gap and
    // production dispatch surfaces the typed refusal.
    #[test]
    fn magic_negative_cycle_is_unsupported_nonstratifiable() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}a"),
            &format!("{BASE}e"),
            &format!("{BASE}b"),
        )]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- ex:e(X, Y), \\+ ex:q(X, Y).\n\
             ex:q(X, Y) :- ex:e(X, Y), \\+ ex:p(X, Y).\n\
             ?- ex:p(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        assert_eq!(
            unsupported_kind(
                resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap()
            ),
            UnsupportedKind::NonStratifiable,
            "a base negative cycle is a genuine non-stratifiable gap"
        );
    }

    // (c) A floundering program: the negated atom carries a variable (`Z`) that no positive
    // body atom binds, so it is still free when NAF fires. NAF over an unbound goal is
    // unsound — the native core refuses it as a declared gap rather than answer wrongly.
    #[test]
    fn magic_floundering_negation_is_unsupported() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}a"),
            &format!("{BASE}e"),
            &format!("{BASE}b"),
        )]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- ex:e(X, Y), \\+ ex:q(Y, Z).\n\
             ?- ex:p(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        assert_eq!(
            unsupported_kind(
                resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap()
            ),
            UnsupportedKind::Floundering,
            "an unbound variable under NAF flounders"
        );
    }

    // Soundness under a negated atom whose variable is bound only by a LATER positive atom:
    // `\+ q(X, Y)` precedes the recursive `r(X, Y)` that binds `Y`. The negated guard must
    // NOT leak into `r`'s magic (demand) rule as existential NAF (`\+ q(X, _)`), which would
    // wrongly prune `r(a, ·)` whenever `a` has ANY `q` edge and drop valid answers. The
    // correct answer for the chain a→b→c is `p(a, Y) = {c}` (a↛directly-c but a⇝c).
    #[test]
    fn magic_negation_var_bound_by_later_atom_stays_sound() {
        let (store, world_nn) = reachability_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:r(X, Y) :- ex:edge(X, Y).\n\
             ex:r(X, Y) :- ex:edge(X, Z), ex:r(Z, Y).\n\
             ex:q(X, Y) :- ex:edge(X, Y).\n\
             ex:p(X, Y) :- ex:node(X, X), \\+ ex:q(X, Y), ex:r(X, Y).\n\
             ?- ex:p(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let native =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        let ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert_eq!(
            ys,
            vec![format!("<{BASE}c>").as_str()],
            "p(a, Y) must be exactly {{c}} — the later-bound negated var must not under-demand \
             r: {native:?}"
        );
    }

    // The `not` keyword is an accepted synonym for `\+` on the query surface, decided
    // identically by the native stratified core.
    #[test]
    fn magic_not_keyword_negation_decides_like_backslash_plus() {
        let (store, world_nn) = reachability_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:reachable(X, Y) :- ex:edge(X, Y).\n\
             ex:reachable(X, Y) :- ex:edge(X, Z), ex:reachable(Z, Y).\n\
             ex:unreachable(X, Y) :- ex:node(X, X), ex:node(Y, Y), not ex:reachable(X, Y).\n\
             ?- ex:unreachable(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let native =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        let ys: Vec<&str> = native.bindings.iter().map(|b| b["Y"].as_str()).collect();
        assert_eq!(ys, vec![format!("<{BASE}a>").as_str()]);
    }

    // An n-ary (non-binary) program that also carries negation is an explicit, honest gap:
    // stratified NAF lives only on the binary backward path, so the generic n-ary path
    // returns a typed refusal rather than silently dropping the negation.
    #[test]
    fn magic_nary_with_negation_is_unsupported() {
        let (store, world_nn) = make_world(&[(
            &format!("{BASE}a"),
            &format!("{BASE}e"),
            &format!("{BASE}b"),
        )]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        // `t(X, Y, Z)` is arity 3 ⇒ the whole program routes to the generic n-ary path,
        // which declares negation unsupported.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- ex:t(X, Y, Z), \\+ ex:q(X, Y).\n\
             ?- ex:p(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        assert!(
            matches!(
                resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap(),
                NativeOutcome::Unsupported(_)
            ),
            "n-ary + negation is a declared gap"
        );
    }

    // ── Subsumptive demand keying: A/B byte-identity vs the variant transform ─────
    //
    // Tekle & Liu (SIGMOD 2011): subsumptive tabling keeps only the most-general demanded
    // adornment per predicate and serves the more-specific calls from it. The perf win must
    // be answer-preserving: the subsumptive transform's goal answer set is BYTE-IDENTICAL to
    // the per-adornment (variant) transform's. `magic_transform_variant` is the retained
    // reference; these tests are the load-bearing correctness gate for the collapse.

    /// Lower a positive/negated-atom program to binary `EvalRule`s (no builtins) — the shared
    /// setup for the A/B transform-parity tests.
    fn eval_rules_of(prog: &QProgram) -> Vec<EvalRule> {
        prog.rules
            .iter()
            .map(|r| {
                let head = atom_of(&r.head).unwrap();
                let body = r
                    .body
                    .iter()
                    .filter_map(|l| match l {
                        QBodyLit::Atom(a) => Some(atom_of(a).unwrap()),
                        QBodyLit::Neg(a) => Some(EvalAtom {
                            negated: true,
                            ..atom_of(a).unwrap()
                        }),
                        _ => None,
                    })
                    .collect();
                let rule_iri = format!("{}::rule", head.predicate.as_str());
                EvalRule {
                    head,
                    body,
                    rule_iri,
                    distinct_pairs: vec![],
                    builtins: vec![],
                }
            })
            .collect()
    }

    /// Resolve `prog` through the given magic transform, returning the canonicalized goal
    /// binding set — the A/B comparison surface. Evaluates the transformed program directly
    /// (the stratifiable-transform corpus), so it never needs the base fallback.
    fn answers_via(
        transform: impl Fn(&[EvalRule], &EvalAtom, BindingPattern) -> MagicProgram,
        foreign: &dyn WorldFactSource,
        world: &str,
        prog: &QProgram,
    ) -> Vec<Binding> {
        let rules = eval_rules_of(prog);
        let goal = &prog.goal.atoms[0];
        let goal_atom = atom_of(goal).unwrap();
        let transformed = transform(&rules, &goal_atom, goal_adornment(goal));
        let mut edb = extract_edb(foreign, world).unwrap();
        for seed in &transformed.seeds {
            let f = seed_to_fact(seed).unwrap();
            edb.insert(&f.predicate, &f.subject, &f.object);
        }
        let facts = match evaluate(edb, &exe(&transformed.rules), None).unwrap() {
            NativeOutcome::Decided(b) => b.rows,
            other => panic!("expected Decided, got {other:?}"),
        };
        let bindings = project_answers(&facts, goal, goal_atom.predicate.as_str());
        let mut answer = AnswerSet {
            bindings,
            status: BudgetStatus::Ok,
            preservation: crate::result::PreservationClaim::exact(),
            frontier: crate::query_ir::CompletionFrontier::empty(),
        };
        answer.canonicalize();
        answer.bindings
    }

    /// Assert the subsumptive transform produces the byte-identical goal answer set to the
    /// variant transform for `prog`.
    fn assert_ab_identical(
        foreign: &dyn WorldFactSource,
        world: &str,
        prog: &QProgram,
        label: &str,
    ) {
        let variant = answers_via(magic_transform_variant, foreign, world, prog);
        let subsumptive = answers_via(magic_transform, foreign, world, prog);
        assert_eq!(
            variant, subsumptive,
            "A/B byte-identity failed on {label}: variant {variant:?} vs subsumptive {subsumptive:?}"
        );
    }

    /// The distinct magic-predicate IRIs (`.../magic/...`) appearing in a transformed program
    /// — the count of magic predicates actually MINTED.
    fn minted_magic_preds(mp: &MagicProgram) -> BTreeSet<String> {
        let mut preds = BTreeSet::new();
        for r in &mp.rules {
            for p in std::iter::once(&r.head).chain(r.body.iter()) {
                if p.predicate.contains("/magic/") {
                    preds.insert(p.predicate.clone());
                }
            }
        }
        preds
    }

    /// The multi-adornment program: `p` is demanded at BOTH `bf` (from `q`'s first rule and
    /// `p(c, Y)` in the second) and `bb` (from `p(X, c)` in the second rule). `bf ⊑ bb`, so
    /// the subsumptive collapse keeps only `magic_p_bf` and serves the `bb` demand from it.
    fn multi_adornment_program() -> QProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- ex:e(X, Y).\n\
             ex:q(X, Y) :- ex:p(X, Y).\n\
             ex:q(X, Y) :- ex:p(X, ex:c), ex:p(ex:c, Y).\n\
             ?- ex:q(ex:a, W).\n"
        );
        parse_query_program(&src).unwrap()
    }

    /// The multi-adornment world: `e(a,b), e(a,c), e(c,d), e(b,z)`. The `e(b,z)` edge is the
    /// LEAK TRAP: it is reachable only if the general `bf` demand's over-derived `p(a,b)` were
    /// wrongly used in the `p(X, c)` (bb) join slot of `q`'s second rule.
    fn multi_adornment_world() -> (WorldStore, String) {
        make_world(&[
            (
                &format!("{BASE}a"),
                &format!("{BASE}e"),
                &format!("{BASE}b"),
            ),
            (
                &format!("{BASE}a"),
                &format!("{BASE}e"),
                &format!("{BASE}c"),
            ),
            (
                &format!("{BASE}c"),
                &format!("{BASE}e"),
                &format!("{BASE}d"),
            ),
            (
                &format!("{BASE}b"),
                &format!("{BASE}e"),
                &format!("{BASE}z"),
            ),
        ])
    }

    // ── Test 1: byte-identity over the full existing corpus + the multi-adornment program ─

    #[test]
    fn magic_subsumptive_matches_variant_over_corpus() {
        // Single-adornment corpus (subsumptive ≡ variant trivially — each predicate is
        // demanded at ONE adornment, so nothing collapses) over the tc / reachability worlds.
        let (tc_store, tc_w) = tc_world();
        let tc = WorldFactSnapshot::from_world(&tc_store, W, PROFILE).unwrap();
        let bf = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        let bb = bf.replace("?- ex:ancestor(ex:a, Y).", "?- ex:ancestor(ex:a, ex:c).");
        let fb = bf.replace("?- ex:ancestor(ex:a, Y).", "?- ex:ancestor(X, ex:d).");
        let ff = bf.replace("?- ex:ancestor(ex:a, Y).", "?- ex:ancestor(X, Y).");
        let nonrec = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\n\
             ?- ex:ancestorOf(ex:a, Y).\n"
        );
        for (label, src) in [
            ("tc-bf", &bf),
            ("tc-bb", &bb),
            ("tc-fb", &fb),
            ("tc-ff", &ff),
            ("non-recursive", &nonrec),
        ] {
            let prog = parse_query_program(src).unwrap();
            assert_ab_identical(&tc, &tc_w, &prog, label);
        }

        // Stratified-negation corpus (transform stays stratifiable) over the reachability
        // world: `unreachable` and the later-bound-negated-var soundness shape.
        let (rw_store, rw_w) = reachability_world();
        let rw = WorldFactSnapshot::from_world(&rw_store, W, PROFILE).unwrap();
        let unreachable = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:reachable(X, Y) :- ex:edge(X, Y).\n\
             ex:reachable(X, Y) :- ex:edge(X, Z), ex:reachable(Z, Y).\n\
             ex:unreachable(X, Y) :- ex:node(X, X), ex:node(Y, Y), \\+ ex:reachable(X, Y).\n\
             ?- ex:unreachable(ex:a, Y).\n"
        );
        let later_bound = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:r(X, Y) :- ex:edge(X, Y).\n\
             ex:r(X, Y) :- ex:edge(X, Z), ex:r(Z, Y).\n\
             ex:q(X, Y) :- ex:edge(X, Y).\n\
             ex:p(X, Y) :- ex:node(X, X), \\+ ex:q(X, Y), ex:r(X, Y).\n\
             ?- ex:p(ex:a, Y).\n"
        );
        for (label, src) in [
            ("unreachable", &unreachable),
            ("later-bound-neg", &later_bound),
        ] {
            let prog = parse_query_program(src).unwrap();
            assert_ab_identical(&rw, &rw_w, &prog, label);
        }

        // The multi-adornment program — where the collapse actually FIRES — must stay
        // byte-identical too.
        let (ma_store, ma_w) = multi_adornment_world();
        let ma = WorldFactSnapshot::from_world(&ma_store, W, PROFILE).unwrap();
        assert_ab_identical(&ma, &ma_w, &multi_adornment_program(), "multi-adornment");
    }

    // ── Test 2: the collapse fires — strictly fewer magic predicates on multi-adornment ──

    #[test]
    fn magic_subsumptive_collapses_multi_adornment_demand() {
        let (store, world_nn) = multi_adornment_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = multi_adornment_program();

        // (a) Byte-identical goal answers: q(a, W) = {b, c, d}. The leak-trap `z` is absent.
        let variant = answers_via(magic_transform_variant, &foreign, &world_nn, &prog);
        let subsumptive = answers_via(magic_transform, &foreign, &world_nn, &prog);
        assert_eq!(
            variant, subsumptive,
            "collapse must preserve the answer set"
        );
        let mut ws: Vec<&str> = subsumptive.iter().map(|b| b["W"].as_str()).collect();
        ws.sort_unstable();
        assert_eq!(
            ws,
            [
                format!("<{BASE}b>"),
                format!("<{BASE}c>"),
                format!("<{BASE}d>")
            ]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
            "q(a, W) must be exactly {{b, c, d}}: {subsumptive:?}"
        );

        // (b) Strictly fewer magic predicates minted than the variant transform.
        let rules = eval_rules_of(&prog);
        let goal_atom = atom_of(&prog.goal.atoms[0]).unwrap();
        let adorn = goal_adornment(&prog.goal.atoms[0]);
        let variant_mp = magic_transform_variant(&rules, &goal_atom, adorn);
        let subsumptive_mp = magic_transform(&rules, &goal_atom, adorn);

        let variant_preds = minted_magic_preds(&variant_mp);
        let subsumptive_preds = minted_magic_preds(&subsumptive_mp);
        assert!(
            subsumptive_preds.len() < variant_preds.len(),
            "subsumptive must mint strictly fewer magic predicates: subsumptive {subsumptive_preds:?} vs variant {variant_preds:?}"
        );
        // The variant mints `magic/p_bb`; the subsumptive folds it into `magic/p_bf` and mints
        // NO `p_bb` table (the bb demand is served from the more-general bf table).
        assert!(
            variant_preds.iter().any(|p| p.ends_with("p_bb")),
            "variant mints the separate p_bb table: {variant_preds:?}"
        );
        assert!(
            !subsumptive_preds.iter().any(|p| p.ends_with("p_bb")),
            "subsumptive must NOT mint p_bb (served by the general p_bf): {subsumptive_preds:?}"
        );
        assert!(
            subsumptive_preds.iter().any(|p| p.ends_with("p_bf")),
            "subsumptive keeps the most-general p_bf table: {subsumptive_preds:?}"
        );
    }

    // ── Test 3: residual no-leak — the general demand over-derives, the answer stays exact ─

    #[test]
    fn magic_subsumptive_residual_no_leak() {
        let (store, world_nn) = multi_adornment_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let prog = multi_adornment_program();
        let rules = eval_rules_of(&prog);
        let goal_atom = atom_of(&prog.goal.atoms[0]).unwrap();
        let adorn = goal_adornment(&prog.goal.atoms[0]);

        // Evaluate the SUBSUMPTIVE transformed program and inspect the derived `p` facts.
        let mp = magic_transform(&rules, &goal_atom, adorn);
        let mut edb = extract_edb(&foreign, &world_nn).unwrap();
        for seed in &mp.seeds {
            let f = seed_to_fact(seed).unwrap();
            edb.insert(&f.predicate, &f.subject, &f.object);
        }
        let facts = match evaluate(edb, &exe(&mp.rules), None).unwrap() {
            NativeOutcome::Decided(b) => b.rows,
            other => panic!("expected Decided, got {other:?}"),
        };
        let p = format!("{BASE}p");
        let derived_p: BTreeSet<(String, String)> = facts
            .iter()
            .filter(|f| f.predicate.as_str() == p)
            .map(|f| (term_display(&f.subject), term_display(&f.object)))
            .collect();

        // The general `bf` demand for `p(a, _)` OVER-DERIVES: it materializes BOTH `p(a, b)`
        // and `p(a, c)` (a superset of the `bb` request `p(a, c)`). This is the widened demand
        // the collapse produces — the residual on the extra bound position is NOT enforced at
        // the magic table.
        assert!(
            derived_p.contains(&(format!("<{BASE}a>"), format!("<{BASE}b>"))),
            "the general bf demand over-derives p(a, b): {derived_p:?}"
        );
        assert!(
            derived_p.contains(&(format!("<{BASE}a>"), format!("<{BASE}c>"))),
            "the general bf demand derives the bb-requested p(a, c): {derived_p:?}"
        );

        // Despite the over-derivation, the goal answer is EXACT: the `p(X, ex:c)` (bb) body
        // atom in q's second rule carries the constant `c`, so the over-derived `p(a, b)` is
        // filtered out of the join — it can NEVER reach `p(c, Y)` and drag in the `e(b, z)`
        // trap edge. The residual is discharged by the ORIGINAL body atom's own constant.
        let answers = answers_via(magic_transform, &foreign, &world_nn, &prog);
        let ws: BTreeSet<&str> = answers.iter().map(|b| b["W"].as_str()).collect();
        assert!(
            !ws.contains(format!("<{BASE}z>").as_str()),
            "the over-derived p(a, b) must NOT leak the z trap into the answer: {answers:?}"
        );
        assert_eq!(
            ws,
            [
                format!("<{BASE}b>"),
                format!("<{BASE}c>"),
                format!("<{BASE}d>")
            ]
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
            "the specific request yields exactly the correct instances, no leak: {answers:?}"
        );
    }

    // ── Leading-bound recursive-IDB demand-seed repro ─────────────────────────────
    //
    // A conjunctive rule body that LEADS with a recursive IDB atom carrying a bound
    // argument (`reach(self, P)`) must resolve the join — never silently return empty+Ok.
    // The magic transform identifies the `reach_bf(self,self)` demand for that leading atom
    // as an unconditional control fact. It must be materialized in the EDB seed set so
    // `reach` is demanded before the semantic program runs.

    /// The base recursive `reach` program + a trailing goal/rule snippet.
    fn leading_idb_src(tail: &str) -> String {
        format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:reach(X, Y) :- ex:knows(X, Y).\n\
             ex:reach(X, Y) :- ex:knows(X, Z), ex:reach(Z, Y).\n\
             {tail}"
        )
    }

    /// The repro world: `self →knows a →knows b`; `b` carries an EDB name and mention. The
    /// name object here is an IRI (`nameB`); [`leading_idb_world_literal_name`] below carries
    /// the SAME shape with the name object as a string literal instead, which is the issue's
    /// exact repro term (`nameMatch(b, "b")`) — both constant kinds are now exercised by the
    /// leading-IDB regression suite.
    fn leading_idb_world() -> (WorldStore, String) {
        make_world(&[
            (
                &format!("{BASE}self"),
                &format!("{BASE}knows"),
                &format!("{BASE}a"),
            ),
            (
                &format!("{BASE}a"),
                &format!("{BASE}knows"),
                &format!("{BASE}b"),
            ),
            (
                &format!("{BASE}b"),
                &format!("{BASE}nameMatch"),
                &format!("{BASE}nameB"),
            ),
            (
                &format!("{BASE}b"),
                &format!("{BASE}mentioned"),
                &format!("{BASE}engines"),
            ),
        ])
    }

    /// The SAME repro world as [`leading_idb_world`], except the `nameMatch` triple's
    /// object is a genuine `xsd:string` literal `"b"` rather than an IRI — the issue's exact
    /// repro term `ex:nameMatch(ex:b, "b")`. The goal-rule's `S` position is a free variable
    /// (never a parsed literal in the query text), so the literal only needs to exist in the
    /// EDB store: it is inserted via [`WorldStore::insert_quad_terms`] (term-preserving),
    /// while the other three triples still go through the IRI-only [`WorldStore::insert_quad`].
    fn leading_idb_world_literal_name() -> (WorldStore, String) {
        let store = WorldStore::new();
        store.insert_quad(
            W,
            &format!("{BASE}self"),
            &format!("{BASE}knows"),
            &format!("{BASE}a"),
        );
        store.insert_quad(
            W,
            &format!("{BASE}a"),
            &format!("{BASE}knows"),
            &format!("{BASE}b"),
        );
        store
            .insert_quad_terms(
                W,
                TermValue::iri(format!("{BASE}b")),
                TermValue::iri(format!("{BASE}nameMatch")),
                TermValue::simple_literal("b"),
            )
            .unwrap();
        store.insert_quad(
            W,
            &format!("{BASE}b"),
            &format!("{BASE}mentioned"),
            &format!("{BASE}engines"),
        );
        (store, W.to_owned())
    }

    #[test]
    fn magic_leading_bound_recursive_idb_body_resolves() {
        let (store, world_nn) = leading_idb_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let budget = Budget::default();

        // (label, goal/rule tail, expected binding count). The bare-reach and EDB-first rows
        // are the controls (correct even on the pre-fix engine); the two leading-IDB rows are
        // the repro (empty + Ok before the seed-set fix).
        let rows: [(&str, &str, usize); 4] = [
            ("bare-reach", "?- ex:reach(ex:self, P).\n", 2),
            (
                "edb-first-control",
                "ex:c(P, S) :- ex:nameMatch(P, S), ex:reach(ex:self, P), ex:mentioned(P, ex:engines).\n\
                 ?- ex:c(P, S).\n",
                1,
            ),
            (
                "leading-idb-name",
                "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
                 ?- ex:c(P, S).\n",
                1,
            ),
            (
                "leading-idb-name-mention",
                "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S), ex:mentioned(P, ex:engines).\n\
                 ?- ex:c(P, S).\n",
                1,
            ),
        ];
        for (label, tail, want) in rows {
            let prog = parse_query_program(&leading_idb_src(tail)).unwrap();
            let ans = crate::dispatch::dispatch_query(&foreign, &world_nn, &prog, PROFILE, &budget)
                .unwrap();
            assert_eq!(
                ans.status,
                BudgetStatus::Ok,
                "{label}: status must be Ok (never a silent empty drop): {ans:?}"
            );
            assert_eq!(
                ans.bindings.len(),
                want,
                "{label}: expected {want} bindings, got {ans:?}"
            );
        }
    }

    #[test]
    fn magic_leading_bound_recursive_idb_literal_name_object_resolves() {
        // The issue's exact repro term: `nameMatch(b, "b")` with a STRING LITERAL object,
        // not an IRI. The leading-IDB seed-set fix must not be sensitive to the constant
        // kind carried by the trailing EDB atom — this drives the same demand-seed path as
        // `magic_leading_bound_recursive_idb_body_resolves`'s "leading-idb-name" row, but
        // over `leading_idb_world_literal_name()`.
        let (store, world_nn) = leading_idb_world_literal_name();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let budget = Budget::default();
        let src = leading_idb_src(
            "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
             ?- ex:c(P, S).\n",
        );
        let prog = parse_query_program(&src).unwrap();
        let ans =
            crate::dispatch::dispatch_query(&foreign, &world_nn, &prog, PROFILE, &budget).unwrap();
        assert_eq!(
            ans.status,
            BudgetStatus::Ok,
            "literal-object leading-IDB: status must be Ok (never a silent empty drop): {ans:?}"
        );
        assert_eq!(
            ans.bindings.len(),
            1,
            "literal-object leading-IDB: expected 1 binding, got {ans:?}"
        );
        assert_eq!(
            ans.bindings[0]["S"], "\"b\"",
            "S must bind to the string literal \"b\": {ans:?}"
        );
    }

    #[test]
    fn magic_ff_goal_ground_fact_rule_resolves() {
        // Site B: a ground fact-rule `pf(a, b).` under an all-free goal `?- pf(X, Y)` lowers
        // the modified rule to an EMPTY body — an unconditional fact that belongs in the
        // demand seed set rather than the transformed semantic program.
        let (store, world_nn) = make_world(&[]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let budget = Budget::default();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:pf(ex:a, ex:b).\n\
             ?- ex:pf(X, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans =
            crate::dispatch::dispatch_query(&foreign, &world_nn, &prog, PROFILE, &budget).unwrap();
        assert_eq!(
            ans.status,
            BudgetStatus::Ok,
            "Site B status must be Ok: {ans:?}"
        );
        assert_eq!(
            ans.bindings.len(),
            1,
            "Site B ff-goal + ground fact-rule must return the asserted fact: {ans:?}"
        );
    }

    #[test]
    fn magic_leading_bound_recursive_idb_incremental_capable() {
        // The incremental path previously DECLINED (returned None) a leading-bound
        // recursive-IDB program because the transform produced a bodyless demand rule. With
        // that demand lifted to a seed, the session is prepared and yields the correct
        // non-empty answer end-to-end.
        let (store, world_nn) = leading_idb_world();
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let budget = Budget::default();
        let src = leading_idb_src(
            "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
             ?- ex:c(P, S).\n",
        );
        let prog = parse_query_program(&src).unwrap();
        let mut session =
            prepare_incremental_query(&foreign, &world_nn, &prog, "test-contract-1511", &budget)
                .unwrap()
                .expect(
                    "a leading-bound recursive-IDB program must now prepare an incremental \
                     session (its demand is a lifted seed, not a bodyless rule)",
                );
        // Apply no changes: the base least model already carries the demanded join.
        let ans = session
            .apply_iri_changes(std::iter::empty::<(String, String, String, i64)>(), None)
            .unwrap();
        assert_eq!(ans.status, BudgetStatus::Ok, "incremental status: {ans:?}");
        assert_eq!(
            ans.bindings.len(),
            1,
            "incremental leading-IDB answer must be non-empty: {ans:?}"
        );
        assert_eq!(
            ans.bindings[0]["P"],
            format!("<{BASE}b>"),
            "P must bind to b: {ans:?}"
        );
        assert_eq!(
            ans.bindings[0]["S"],
            format!("<{BASE}nameB>"),
            "S must bind to nameB: {ans:?}"
        );
    }

    #[test]
    fn magic_seeds_are_exactly_the_bodyless_rule_heads() {
        // Demand-completeness certificate (Beeri–Ramakrishnan): the materialized seed set is
        // EXACTLY the set of ground heads of the bodyless positive rules the transform would
        // emit. For the leading-IDB program the sole such demand is `magic_reach_bf(self,
        // self)` — the goal `c` is ff, so it contributes no goal seed.
        let src = leading_idb_src(
            "ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
             ?- ex:c(P, S).\n",
        );
        let prog = parse_query_program(&src).unwrap();
        let rules = eval_rules_of(&prog);
        let goal = &prog.goal.atoms[0];
        let goal_atom = atom_of(goal).unwrap();
        let transformed = magic_transform(&rules, &goal_atom, goal_adornment(goal));

        // No unconditional rule survives (the invariant the transform asserts), and every
        // seed is ground. Semantic NAF-only/builtin-only rules remain valid because they
        // carry body or builtin content.
        assert!(
            transformed
                .rules
                .iter()
                .all(|r| !r.body.is_empty() || !r.builtins.is_empty()),
            "no transformed rule may be unconditional: {:?}",
            transformed.rules
        );
        for s in &transformed.seeds {
            assert!(seed_to_fact(s).is_ok(), "every seed must be ground: {s:?}");
        }

        // Re-derive the expected demand independently: the only leading bound recursive-IDB
        // atom is `reach(self, _)` adorned bf, so the single lifted demand seed is the
        // self-loop `magic_reach_bf(self, self)`.
        let reach_pred = rules
            .iter()
            .map(|r| r.head.predicate.as_str())
            .find(|p| p.ends_with("reach"))
            .expect("the program defines reach")
            .to_owned();
        let self_iri = EvalTerm::ConstNamed(format!("{BASE}self"));
        let expected = EvalAtom {
            subject: self_iri.clone(),
            predicate: magic_pred_iri(&reach_pred, "bf"),
            object: self_iri,
            negated: false,
        };
        assert_eq!(
            transformed.seeds.len(),
            1,
            "exactly one lifted demand seed: {:?}",
            transformed.seeds
        );
        assert_eq!(
            transformed.seeds[0], expected,
            "the seed set must equal the bodyless-rule-head demand set {{magic_reach_bf(self, self)}}"
        );
    }

    // ── Empty-positive-body identity: NAF-only and builtin-only rules evaluate ──
    //
    // The empty conjunction contributes one empty substitution. NAF then filters that row
    // against the frozen lower-stratum store, while sequential `is` builtins extend it.

    #[test]
    fn resolve_native_ground_naf_only_body_evaluates_absence_and_presence() {
        let (store, world_nn) = make_world(&[]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(ex:a, ex:b) :- \\+ ex:q(ex:a, ex:b).\n\
             ?- ex:p(X, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let absent =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        assert_eq!(
            absent.bindings.len(),
            1,
            "absent q must let p fire: {absent:?}"
        );
        assert_eq!(absent.bindings[0]["X"], format!("<{BASE}a>"));
        assert_eq!(absent.bindings[0]["Y"], format!("<{BASE}b>"));

        let q_subject = format!("{BASE}a");
        let q_predicate = format!("{BASE}q");
        let q_object = format!("{BASE}b");
        let (store, world_nn) = make_world(&[(&q_subject, &q_predicate, &q_object)]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let present =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        assert!(
            present.bindings.is_empty(),
            "present q must block the ground NAF-only rule: {present:?}"
        );
    }

    #[test]
    fn resolve_native_builtin_only_body_evaluates_adjacent_assignments() {
        let (store, world_nn) = make_world(&[]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(X, Y) :- X is 1, Y is 2.\n\
             ?- ex:p(A, B).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let answer =
            decided(resolve_native(&foreign, &world_nn, &prog, &Budget::default()).unwrap());
        assert_eq!(
            answer.bindings.len(),
            1,
            "builtin-only rule must fire once: {answer:?}"
        );
        assert_eq!(
            answer.bindings[0]["A"],
            "\"1\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
        assert_eq!(
            answer.bindings[0]["B"],
            "\"2\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
    }

    #[test]
    fn prepare_incremental_query_declines_ground_naf() {
        // The incremental path already declines any body carrying a `Neg` literal
        // (`binary_eligible`'s per-literal match at the top of
        // `prepare_incremental_query`), UPSTREAM of `magic_transform` — so it is
        // panic-safe on this shape with no additional gate needed there.
        let (store, world_nn) = make_world(&[]);
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).unwrap();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(ex:a, ex:b) :- \\+ ex:q(ex:a, ex:b).\n\
             ?- ex:p(X, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let session = prepare_incremental_query(
            &foreign,
            &world_nn,
            &prog,
            "test-contract-1511-unpositive",
            &Budget::default(),
        )
        .unwrap();
        assert!(
            session.is_none(),
            "a ground-NAF-only body must decline incremental preparation, not panic: \
             {:?}",
            session.is_some()
        );
    }
}
