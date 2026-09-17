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

#[cfg(test)]
mod test_support;
#[cfg(test)]
use test_support::magic_transform_variant;

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
        // A ground RDF 1.2 quoted-triple goal argument lowers to a flat constant literal
        // carrying the reconstructed `TermValue::Triple`, so it unifies byte-identically
        // with a provider-returned or fact-carried triple term and reaches a provider as a
        // bound query term.
        QTerm::Triple { .. } => Ok(EvalTerm::ConstLit(qterm_to_value(t)?)),
    }
}

/// Lower a **ground** `QTerm` to its `purrdf::TermValue` (an IRI, a literal, or a nested
/// quoted-triple). Used to materialize a `QTerm::Triple` goal argument. A parser-produced
/// ground triple always lowers cleanly (the parser validates IRI predicate + ground
/// components); a malformed component (only reachable by constructing a `QTerm` directly,
/// bypassing the parser) is a non-representable shape, reported as the same typed gap the
/// `Struct` arm uses rather than a panic.
pub(crate) fn qterm_to_value(t: &QTerm) -> Result<TermValue, UnsupportedKind> {
    match t {
        QTerm::Const(c) => {
            crate::term_codec::decode_term(c).map_err(|_| UnsupportedKind::NonBinaryAtom)
        }
        QTerm::Num(n) => Ok(TermValue::typed_literal(
            n.to_string(),
            crate::physical::XSD_INTEGER,
        )),
        QTerm::Triple { s, p, o } => Ok(TermValue::Triple {
            s: Box::new(qterm_to_value(s)?),
            p: Box::new(qterm_to_value(p)?),
            o: Box::new(qterm_to_value(o)?),
        }),
        QTerm::Var(_) | QTerm::Struct(_) => Err(UnsupportedKind::NonBinaryAtom),
    }
}

/// Rewrite a builtin operand's variable to the engine's `?`-prefixed surface,
/// matching the [`EvalTerm::Var`] keys the body atoms carry (constants unchanged).
fn prefix_builtin_term(t: &QTerm) -> QTerm {
    match t {
        QTerm::Var(v) => QTerm::Var(format!("?{v}")),
        // A structured or quoted-triple term never reaches the flat builtin operand surface
        // (arithmetic operands are `Var`/`Num`; a triple term is a type error caught by the
        // builtin evaluator, not here); carry it unchanged for exhaustiveness.
        QTerm::Const(_) | QTerm::Num(_) | QTerm::Struct(_) | QTerm::Triple { .. } => t.clone(),
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
        // Only the TARGET is a value the constraint stage may generate/bind; the
        // gram/x/y operands are IRI inputs (a `Var` bound to an IRI is `?`-prefixed so
        // the solution lookup resolves it, a `Const` IRI is carried unchanged).
        QBuiltin::BilinearSqDist { target, gram, x, y } => QBuiltin::BilinearSqDist {
            target: prefix_builtin_term(target),
            gram: prefix_builtin_term(gram),
            x: prefix_builtin_term(x),
            y: prefix_builtin_term(y),
        },
        // LOWERING-ONLY (`crate::relational_core::lower_constraint_violation_rules`),
        // never authored on the `.logic` query surface this backward transform reads —
        // carried for exhaustiveness with the same operand-prefixing treatment.
        QBuiltin::DimEqual { d1, d2 } => QBuiltin::DimEqual {
            d1: prefix_builtin_term(d1),
            d2: prefix_builtin_term(d2),
        },
        QBuiltin::DimProduct { d_f, d_m, d_r } => QBuiltin::DimProduct {
            d_f: prefix_builtin_term(d_f),
            d_m: prefix_builtin_term(d_m),
            d_r: prefix_builtin_term(d_r),
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

/// The additional selective world probes a bilinear-form squared-distance builtin
/// needs: its exact-rational `math:` Gram/vector cells appear in NO body atom, so the
/// atom-derived `binary_source_patterns` never probes them. When any lowered rule
/// carries a `BilinearSqDist` builtin, probe every `math:` cell predicate
/// ([`MATH_CELL_PREDICATES`]) predicate-only, so the cell facts reach the columnar EDB
/// the seminaive constraint stage's resolver reads. An empty result when no such
/// builtin is present keeps every other program's probe plan byte-identical.
fn math_cell_source_patterns(rules: &[EvalRule]) -> Vec<WorldFactPattern> {
    let needs_cells = rules.iter().any(|rule| {
        rule.builtins
            .iter()
            .any(|b| matches!(b, QBuiltin::BilinearSqDist { .. }))
    });
    if !needs_cells {
        return Vec::new();
    }
    crate::physical::MATH_CELL_PREDICATES
        .iter()
        .map(|pred| WorldFactPattern::new(None, Some((*pred).to_owned()), None))
        .collect()
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
    // restricted; a comparison binds nothing. A bilinear-form squared-distance builtin
    // likewise binds its target (the exact squared distance).
    for b in builtins {
        match b {
            QBuiltin::Is {
                target: QTerm::Var(v),
                ..
            }
            | QBuiltin::BilinearSqDist {
                target: QTerm::Var(v),
                ..
            } => {
                bound.insert(v.clone());
            }
            // A pure filter binds nothing: `Compare` and the two LOWERING-ONLY dimension-
            // gate builtins (`DimEqual`/`DimProduct`, never authored on the query
            // surface this backward-magic analysis covers) never generate a value.
            QBuiltin::Is { .. }
            | QBuiltin::Compare { .. }
            | QBuiltin::BilinearSqDist { .. }
            | QBuiltin::DimEqual { .. }
            | QBuiltin::DimProduct { .. } => {}
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
        numeric: Vec::new(),
        head,
        body,
        rule_iri,
        distinct_pairs: vec![],
        builtins: vec![],
        reduction: None,
        constraint_tag: None,
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

/// Match a prepared native goal before rendering its public variable bindings.
/// Constants were lowered once at admission; literal and repeated-variable equality
/// compare complete terms. Rejected rows allocate no RDF presentation strings.
fn project_answer(fact: &Fact, goal: &EvalAtom) -> Option<Binding> {
    if fact.predicate != goal.predicate {
        return None;
    }
    let matches = |pattern: &EvalTerm, value: &TermValue| match pattern {
        EvalTerm::Var(_) => true,
        EvalTerm::ConstNamed(iri) => value.as_iri() == Some(iri.as_str()),
        EvalTerm::ConstLit(expected) => value == expected,
    };
    if !matches(&goal.subject, &fact.subject) || !matches(&goal.object, &fact.object) {
        return None;
    }
    if let (EvalTerm::Var(subject), EvalTerm::Var(object)) = (&goal.subject, &goal.object)
        && subject == object
        && fact.subject != fact.object
    {
        return None;
    }
    let mut binding = Binding::new();
    for (pattern, value) in [(&goal.subject, &fact.subject), (&goal.object, &fact.object)] {
        if let EvalTerm::Var(name) = pattern {
            let name = name
                .strip_prefix('?')
                .expect("admitted query variables carry the engine prefix");
            if !binding.contains_key(name) {
                binding.insert(name.to_owned(), term_display(value));
            }
        }
    }
    Some(binding)
}

/// Project the admitted goal's matching rows into public bindings.
fn project_answers(facts: &[Fact], goal: &EvalAtom) -> Vec<Binding> {
    facts
        .iter()
        .filter_map(|fact| project_answer(fact, goal))
        .collect()
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
    goal: EvalAtom,
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
        let mut bindings = project_answers(&closure, &self.goal);
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
            numeric: Vec::new(),
            rule_iri: format!("{}::rule", head.predicate.as_str()),
            head,
            body,
            distinct_pairs: Vec::new(),
            builtins: Vec::new(),
            reduction: None,
            constraint_tag: None,
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
        goal: goal_atom,
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
            numeric: Vec::new(),
            head,
            body,
            rule_iri,
            distinct_pairs: Vec::new(),
            builtins,
            reduction: None,
            constraint_tag: None,
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

    let mut source_patterns = binary_source_patterns(&rules, &goal_atom);
    // A metric-form builtin's Gram/vector cells ride in no body atom, so probe the
    // `math:` cell predicates explicitly when one is present (a no-op otherwise).
    source_patterns.extend(math_cell_source_patterns(&rules));
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
    let mut dag = gmeow_term_arena::engine::TermDag::new();
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
    dag: &mut gmeow_term_arena::engine::TermDag,
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
    let mut bindings = project_answers(&facts, &goal_atom);

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
                    subject: crate::provenance::term_display(subject),
                    predicate: predicate.clone(),
                    object: crate::provenance::term_display(object),
                })
                .collect(),
            tuple_sources: Vec::new(),
            provider_sources: Vec::new(),
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
            numeric: Vec::new(),
            head,
            body,
            rule_iri,
            distinct_pairs: Vec::new(),
            builtins,
            reduction: None,
            constraint_tag: None,
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
        let Some(binding) = project_answer(fact, &goal_atom) else {
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

/// Provider-aware annotated resolution through the single arity-generic fixpoint.
///
/// Explicit query-scoped relation registration selects this route even for an all-binary
/// program, because provider atoms and ordinary RDF EDB atoms must share one authored-SIPS
/// evaluation. There is no scalar callback, scratch-world materialization, or second score
/// pass.
pub(crate) fn resolve_native_annotated_with_relations_under<A, F>(
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    budget: &Budget,
    annotation: &AnnotationRequest<'_, A, F>,
    relation_execution: &mut crate::external_relation::RelationExecution<'_, '_, '_, A>,
) -> Result<
    NativeOutcome<AnnotatedAnswerSet<A::Element>>,
    super::magic_generic::ExternalRelationEvaluationError,
>
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
    let outcome = super::magic_generic::resolve_native_generic_annotated_with_relations(
        foreign,
        world,
        program,
        budget,
        annotation,
        relation_execution,
    )?;
    match outcome {
        NativeOutcome::Decided(mut answer) => {
            relation_execution
                .merge_preservation(&mut answer.preservation)
                .map_err(super::magic_generic::ExternalRelationEvaluationError::Query)?;
            Ok(NativeOutcome::Decided(answer))
        }
        NativeOutcome::Unsupported(kind) => Ok(NativeOutcome::Unsupported(kind)),
    }
}

#[path = "magic.tests.rs"]
#[cfg(test)]
mod tests;
