// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixed-arity n-ary predication → reified-binary lowering, and the native n-ary
//! forward-chase ingestion entry.
//!
//! # Doctrine: n-ary is REIFIED BINARY, never a wide data plane
//!
//! `LOGIC-IR.md` §RelationalCore mandates that a fixed-arity atom `R(a₀,…,aₙ)` is NOT
//! carried as a wide tuple in the physical engine; at the lowering boundary it is
//! **reified** into a conjunction of ordinary binary atoms over a single content-addressed
//! reifier node:
//!
//! ```text
//! logic:instanceOf(R, Rel) ∧ logic:naryArg0(R, a₀) ∧ … ∧ logic:naryArgN(R, aₙ)
//! ```
//!
//! A body atom binds a fresh reifier variable (matched against the reified EDB); a head
//! atom *invents* a new tuple whose reifier node the restricted chase mints by **tuple
//! identity** — [`crate::provenance::mint_nary_reifier`], content-addressed on the relation
//! and the ordered argument VALUES — so an ingested EDB tuple and a chase-derived tuple that are
//! the SAME tuple share ONE reifier node (`exact`, not residue). This module is the
//! single place that performs that lowering for the native engine and the inverse
//! de-reification of the chase's output, so the native path stays a pure binary
//! [`EvalAtom`] engine (`EvalAtom` is NEVER widened).
//!
//! # What is REFUSED, not mis-lowered (`LOGIC-IR.md` §222-225, `LOGIC-SEMANTICS.md`)
//!
//! Only the fixed-arity, range-restricted, conjunctive-head fragment lowers `exact`. The
//! genuinely-unsupported shapes are hard-failed here (named), never silently evaluated as
//! one disjunct or with a guard dropped:
//!
//! * a **disjunctive head** ([`lower_nary_rules`] refuses — a head is a conjunction);
//! * a **non-range-restricted head argument** — an existential that appears as a tuple
//!   ARGUMENT the body does not bind is a Skolem-*function* obligation, refused (only a
//!   fresh reifier SUBJECT may be existential; a value null must still be frontier-shared);
//! * (the ≥n interchangeable-witness distinctness and negated-body cases stay refused by
//!   the underlying [`crate::physical`] chase parser, which this module does not bypass).
//!
//! # Termination is a certificate, computed on the RELATION-QUALIFIED model
//!
//! The reified encoding shares the `naryArg{i}` predicates across every relation, so the
//! generic constant-refined weak-acyclicity certifier ([`ChaseAdmission::certify`]) would
//! see a SPURIOUS cycle (m1's `naryArg0` object position and m0's `naryArg0` object
//! position collapse to one node). [`certify_nary_termination`] therefore certifies on a
//! **relation-qualified** reification (each `naryArg{i}` predicate is tagged by its
//! relation). This is a FAITHFUL termination model, not an under-approximation: in the
//! real chase every reified body atom is joined together with its `instanceOf(R, Rel)`
//! typing atom, so a `naryArg` fact is only ever consumed for the relation its reifier is
//! typed to — the shared predicate never crosses relations at runtime, exactly as the
//! qualified model asserts.

use std::collections::BTreeMap;

use purrdf::TermValue;

use crate::physical::{ChaseAdmission, ExistentialRule, NativeOutcome, chase_world};
use crate::provenance::{
    instance_of_iri, mint_nary_reifier, nary_arg_index, nary_arg_predicate, term_display,
};
use crate::rule_ir::{DerivedRow, EvalAtom, EvalTerm, Fact};

/// The single world the ingestion entry chases the reified EDB under. The reified-n-ary
/// fragment is world-agnostic (the reifier IRIs are content-addressed and world-independent),
/// so one world suffices for the ingestion/parity seam.
const NARY_WORLD: &str = "https://blackcatinformatics.ca/gmeow/world/nary";

/// Wrap an n-ary lowering condition message as a typed diagnostic on the shared substrate.
fn nary_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Engine { detail })
}

// ── Public n-ary surface ────────────────────────────────────────────────────────

/// One fixed-arity n-ary ground tuple `relation(args…)` — the engine-neutral n-ary shape
/// this module lowers into reified binary facts and de-reifies the chase output back into.
#[derive(Debug, Clone, PartialEq)]
pub struct NaryTuple {
    /// The relation IRI (un-bracketed).
    pub relation: String,
    /// The ordered argument terms.
    pub args: Vec<TermValue>,
}

/// One argument position of an n-ary ATOM in a rule: a variable, a constant IRI, or a
/// constant literal.
#[derive(Debug, Clone, PartialEq)]
pub enum NaryArg {
    /// A variable (the string includes the leading `?`, matching the engine surface).
    Var(String),
    /// A constant IRI (the full IRI string).
    Named(String),
    /// A constant literal.
    Lit(TermValue),
}

/// One fixed-arity n-ary atom `relation(args…)` in a rule body or head.
#[derive(Debug, Clone, PartialEq)]
pub struct NaryAtom {
    /// The relation IRI (un-bracketed).
    pub relation: String,
    /// The ordered argument positions.
    pub args: Vec<NaryArg>,
}

/// A fixed-arity n-ary multi-head TGD: a conjunctive body implies a conjunctive head that
/// may invent fresh tuples (each with its own existential reifier) and share value nulls.
///
/// An existential head variable is any variable occurring in the head that no body atom
/// binds — detected structurally, exactly as [`ExistentialRule::existentials`] does.
#[derive(Debug, Clone, PartialEq)]
pub struct NaryRule {
    /// The firing rule IRI (`#[name(...)]` value).
    pub name: String,
    /// The conjunctive body atoms.
    pub body: Vec<NaryAtom>,
    /// The conjunctive head atoms.
    pub head: Vec<NaryAtom>,
}

impl NaryAtom {
    /// The variable names occurring in this atom's argument positions.
    fn vars(&self) -> impl Iterator<Item = &str> {
        self.args.iter().filter_map(|a| match a {
            NaryArg::Var(v) => Some(v.as_str()),
            NaryArg::Named(_) | NaryArg::Lit(_) => None,
        })
    }
}

// ── Fact lowering ─────────────────────────────────────────────────────────────

/// Lower ONE fixed-arity n-ary ground tuple `relation(args…)` into its reified binary
/// facts: `instanceOf(R, relation) ∧ naryArg0(R, a₀) ∧ … ∧ naryArgN(R, aₙ)`, where `R` is
/// the content-addressed reifier ([`mint_nary_reifier`]) for `(relation, ordered args)`.
///
/// The reifier `R` is IDENTICAL to the node the restricted chase mints for a DERIVED tuple
/// of the same relation + arguments, so a pre-reified EDB tuple and a chase-invented tuple
/// that denote the same tuple share ONE reifier node (provenance/identity parity).
///
/// # Errors
///
/// Propagates the [`mint_nary_reifier`] failure (an RDF-star triple argument is out of
/// scope for the n-ary encoding).
pub(crate) fn lower_nary_fact(
    relation: &str,
    args: &[TermValue],
) -> gmeow_errors::Result<Vec<Fact>> {
    let reifier = mint_nary_reifier(relation, args)?;
    let mut facts = Vec::with_capacity(args.len() + 1);
    facts.push(Fact {
        subject: TermValue::iri(reifier.clone()),
        predicate: instance_of_iri(),
        object: TermValue::iri(relation.to_owned()),
    });
    for (i, arg) in args.iter().enumerate() {
        facts.push(Fact {
            subject: TermValue::iri(reifier.clone()),
            predicate: nary_arg_predicate(i),
            object: arg.clone(),
        });
    }
    Ok(facts)
}

/// Lower a set of n-ary tuples into ONE reified binary EDB (a flat `Vec<Fact>` in a single
/// world), via [`lower_nary_fact`] tuple-by-tuple. Duplicate tuples reify onto the SAME
/// reifier (content addressing), so the downstream chase's fact dedup collapses them.
///
/// # Errors
///
/// Propagates [`lower_nary_fact`] failures.
pub(crate) fn lower_nary_edb(tuples: &[NaryTuple]) -> gmeow_errors::Result<Vec<Fact>> {
    let mut out = Vec::new();
    for t in tuples {
        out.extend(lower_nary_fact(&t.relation, &t.args)?);
    }
    Ok(out)
}

// ── Rule lowering ─────────────────────────────────────────────────────────────

/// Whether to reify with the CANONICAL shared `naryArg{i}` predicates (the actual chase
/// encoding), or the RELATION-QUALIFIED predicates used only for the termination certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArgScheme {
    /// `logic:naryArg{i}` — the doctrinal shared surface the chase actually runs on.
    Canonical,
    /// `logic:naryArgQ/{relation}/{i}` — relation-tagged, so the weak-acyclicity position
    /// graph keeps each relation's argument positions distinct (faithful, since the real
    /// chase never crosses relations through a shared `naryArg` predicate — see module doc).
    RelationQualified,
}

/// The argument predicate IRI for `relation` position `i` under `scheme`.
fn arg_predicate(scheme: ArgScheme, relation: &str, i: usize) -> String {
    match scheme {
        ArgScheme::Canonical => nary_arg_predicate(i),
        ArgScheme::RelationQualified => {
            format!(
                "{}naryArgQ/{relation}/{i}",
                crate::provenance::LOGIC_NAMESPACE
            )
        }
    }
}

/// An [`NaryArg`] as a binary [`EvalTerm`].
fn arg_term(arg: &NaryArg) -> EvalTerm {
    match arg {
        NaryArg::Var(v) => EvalTerm::Var(v.clone()),
        NaryArg::Named(iri) => EvalTerm::ConstNamed(iri.clone()),
        NaryArg::Lit(t) => EvalTerm::ConstLit(t.clone()),
    }
}

/// Reify ONE n-ary atom onto reifier variable `reifier`, appending its binary atoms
/// (`instanceOf(reifier, relation)` + `naryArg{i}(reifier, argᵢ)`) to `out`.
fn reify_atom_into(atom: &NaryAtom, reifier: &str, scheme: ArgScheme, out: &mut Vec<EvalAtom>) {
    out.push(EvalAtom {
        subject: EvalTerm::Var(reifier.to_owned()),
        predicate: instance_of_iri(),
        object: EvalTerm::ConstNamed(atom.relation.clone()),
        negated: false,
    });
    for (i, a) in atom.args.iter().enumerate() {
        out.push(EvalAtom {
            subject: EvalTerm::Var(reifier.to_owned()),
            predicate: arg_predicate(scheme, &atom.relation, i),
            object: arg_term(a),
            negated: false,
        });
    }
}

/// The fresh reifier variable minted for the `idx`-th body / head n-ary atom. The prefix is
/// deliberately unusable as a user-authored variable so it never collides with a program var.
fn body_reifier_var(idx: usize) -> String {
    format!("?__nary_reif_body_{idx}")
}
fn head_reifier_var(idx: usize) -> String {
    format!("?__nary_reif_head_{idx}")
}

/// Lower a set of n-ary multi-head TGD rules into reified binary [`ExistentialRule`]s (the
/// CANONICAL `naryArg{i}` encoding the native chase runs).
///
/// Each body n-ary atom reifies onto a fresh reifier variable that the body join BINDS
/// (against the reified EDB), so it is a frontier — not existential — variable. Each head
/// n-ary atom reifies onto a fresh reifier variable that no body atom binds, so it is an
/// EXISTENTIAL reifier the chase mints by tuple identity. A value variable that occurs in
/// the head but no body is a shared value null (a frontier-addressed Skolem witness, shared
/// across every head atom it appears in — a genuine restricted-chase shared null).
///
/// # Errors (doctrinal refusals — `LOGIC-IR.md` §222-225)
///
/// * an **empty head** (nothing to derive) — refused;
/// * a **head n-ary atom with fewer than one argument** — a fixed-arity tuple has ≥1 slot;
/// * a **non-range-restricted VALUE argument** whose only occurrence is a single head atom
///   (it can never be a shared null, so it would demand a Skolem *function* over the
///   frontier per argument position) — refused rather than mis-lowered as `exact`.
pub(crate) fn lower_nary_rules(rules: &[NaryRule]) -> gmeow_errors::Result<Vec<ExistentialRule>> {
    lower_nary_rules_with(rules, ArgScheme::Canonical)
}

/// The scheme-parameterized core of [`lower_nary_rules`] — shared by the canonical chase
/// lowering and the relation-qualified certification lowering.
fn lower_nary_rules_with(
    rules: &[NaryRule],
    scheme: ArgScheme,
) -> gmeow_errors::Result<Vec<ExistentialRule>> {
    let mut out = Vec::with_capacity(rules.len());
    for rule in rules {
        out.push(lower_one_rule(rule, scheme)?);
    }
    Ok(out)
}

/// Lower one n-ary rule, enforcing the doctrinal refusals.
fn lower_one_rule(rule: &NaryRule, scheme: ArgScheme) -> gmeow_errors::Result<ExistentialRule> {
    if rule.head.is_empty() {
        return Err(nary_err(format!(
            "n-ary rule {:?} has an empty head — nothing to derive; a conjunctive TGD head \
             has at least one atom",
            rule.name
        )));
    }

    // Body-bound variables (the frontier source). A value var occurring in the body is
    // range-restricted; a head var absent from the body is existential.
    let body_vars: std::collections::BTreeSet<&str> =
        rule.body.iter().flat_map(NaryAtom::vars).collect();

    // Every value variable's head occurrences, to detect a non-range-restricted argument
    // that can never be a shared null (occurs in exactly one head atom and no body atom).
    let mut head_var_atom_count: BTreeMap<&str, usize> = BTreeMap::new();
    for atom in &rule.head {
        // Count DISTINCT head atoms a var occurs in (a var used twice in one atom still
        // shares within that atom, so it is the ACROSS-atom sharing that makes it a null).
        let mut seen_here: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for v in atom.vars() {
            if seen_here.insert(v) {
                *head_var_atom_count.entry(v).or_insert(0) += 1;
            }
        }
    }
    for (var, count) in &head_var_atom_count {
        if !body_vars.contains(var) && *count < 2 {
            // Existential value var occurring in a single head atom: it is a fresh witness
            // an argument position DEMANDS, not shared with any other head atom, so it is a
            // Skolem-FUNCTION obligation (a distinct witness per argument slot), which the
            // restricted reified chase does not carry as `exact`.
            return Err(nary_err(format!(
                "n-ary rule {:?} head argument variable {var:?} is not range-restricted (the \
                 body does not bind it) and is not shared across head atoms — a non-frontier, \
                 non-shared existential argument is a Skolem-function obligation the reified \
                 restricted chase refuses rather than mis-lowering as exact",
                rule.name
            )));
        }
    }

    let mut body: Vec<EvalAtom> = Vec::new();
    for (idx, atom) in rule.body.iter().enumerate() {
        reify_atom_into(atom, &body_reifier_var(idx), scheme, &mut body);
    }
    let mut head: Vec<EvalAtom> = Vec::new();
    for (idx, atom) in rule.head.iter().enumerate() {
        if atom.args.is_empty() {
            return Err(nary_err(format!(
                "n-ary rule {:?} head atom for relation {:?} has zero arguments — a \
                 fixed-arity n-ary tuple has at least one argument",
                rule.name, atom.relation
            )));
        }
        reify_atom_into(atom, &head_reifier_var(idx), scheme, &mut head);
    }

    Ok(ExistentialRule {
        rule_iri: rule.name.clone(),
        body,
        head,
        // The reified n-ary head carries no ≥n interchangeable-witness distinctness (each
        // invented tuple has its OWN reifier subject, minted by tuple identity), so the
        // structural distinctness guard is empty here by construction.
        distinct: Vec::new(),
        witness_frontier: None,
        witness_policy: crate::physical::WitnessPolicy::FrontierSkolem,
    })
}

// ── Termination certificate ───────────────────────────────────────────────────

/// Certify termination of an n-ary program by constant-refined weak acyclicity of its
/// **relation-qualified** reification (see the module doc for why the qualification is a
/// faithful, non-under-approximating termination model).
///
/// # Errors
///
/// Propagates a rule-lowering refusal ([`lower_nary_rules`]).
pub fn certify_nary_termination(rules: &[NaryRule]) -> gmeow_errors::Result<ChaseAdmission> {
    let qualified = lower_nary_rules_with(rules, ArgScheme::RelationQualified)?;
    Ok(ChaseAdmission::certify(&qualified))
}

// ── Ingestion entry ───────────────────────────────────────────────────────────

/// Run the native restricted chase over an n-ary EDB + n-ary multi-head TGD program,
/// returning the closure (asserted EDB ∪ derived tuples) DE-REIFIED back to n-ary tuples.
///
/// The program is lowered to the reified binary encoding ([`lower_nary_edb`] +
/// [`lower_nary_rules`]) and run through the EXISTING native chase
/// ([`crate::physical::chase_world`]); the derived reified triples are then re-assembled
/// into `(relation, args)` tuples for native consumers.
///
/// Termination is a CERTIFICATE, not a hope: the program is certified weakly acyclic on the
/// relation-qualified model first; an uncertified program is REFUSED (a hard error naming
/// the weak-acyclicity violation) rather than chased unbudgeted into a possible loop.
///
/// # Errors
///
/// Returns `Err` if the program is not certified terminating, if a lowering refusal fires,
/// if the chase declines the rule set, or if the chase output cannot be de-reified.
pub fn run_native_nary_forward(
    edb: &[NaryTuple],
    rules: &[NaryRule],
) -> gmeow_errors::Result<Vec<NaryTuple>> {
    Ok(run_native_nary_forward_run(edb, rules)?.tuples)
}

/// Re-assemble the chase's derived reified triples back into n-ary tuples.
///
/// Rows are grouped by their reifier SUBJECT; each group yields one tuple whose relation is
/// its `instanceOf` object and whose arguments are its `naryArg{i}` objects in positional
/// order. The output is sorted `(relation, term-display of each arg)` — a total, engine-
/// independent order, so two runs (or two engines) compare byte-for-byte up to null naming.
///
/// # Errors
///
/// Hard-fails on any reified-shape violation — a non-IRI `instanceOf` object, a reifier
/// with argument atoms but no typing atom, a positional-index gap or duplicate, or a
/// chase output row outside the reified vocabulary (no-optionality: a malformed closure is
/// a lowering/engine defect, never silently dropped).
fn dereify_rows(rows: &[DerivedRow]) -> gmeow_errors::Result<Vec<NaryTuple>> {
    let instance_of = instance_of_iri();
    // reifier subject surface → relation IRI.
    let mut relation_of: BTreeMap<String, String> = BTreeMap::new();
    // reifier subject surface → (positional index → argument value).
    let mut args_of: BTreeMap<String, BTreeMap<usize, TermValue>> = BTreeMap::new();

    for row in rows {
        let reifier = term_display(&row.subject);
        if row.predicate == instance_of {
            let TermValue::Iri(rel) = &row.object else {
                return Err(nary_err(format!(
                    "de-reify: instanceOf object for reifier {reifier} is not an IRI relation: {:?}",
                    row.object
                )));
            };
            if let Some(prev) = relation_of.insert(reifier.clone(), rel.clone())
                && &prev != rel
            {
                return Err(nary_err(format!(
                    "de-reify: reifier {reifier} typed to two relations ({prev:?} and {rel:?})"
                )));
            }
        } else if let Some(i) = nary_arg_index(&row.predicate) {
            let slot = args_of.entry(reifier.clone()).or_default();
            if let Some(prev) = slot.insert(i, row.object.clone())
                && term_display(&prev) != term_display(&row.object)
            {
                return Err(nary_err(format!(
                    "de-reify: reifier {reifier} argument position {i} bound to two values \
                     ({} and {})",
                    term_display(&prev),
                    term_display(&row.object)
                )));
            }
        } else {
            return Err(nary_err(format!(
                "de-reify: chase output row carries a non-reified predicate <{}> — the n-ary \
                 chase output must be fully reified (instanceOf / naryArg{{i}} only)",
                row.predicate
            )));
        }
    }

    let mut out: Vec<NaryTuple> = Vec::with_capacity(relation_of.len());
    for (reifier, relation) in &relation_of {
        let slots = args_of.get(reifier).ok_or_else(|| {
            nary_err(format!(
                "de-reify: reifier {reifier} typed {relation:?} has no naryArg arguments"
            ))
        })?;
        let arity = slots.len();
        let mut args = Vec::with_capacity(arity);
        for i in 0..arity {
            let value = slots.get(&i).ok_or_else(|| {
                nary_err(format!(
                    "de-reify: reifier {reifier} typed {relation:?} has a positional gap at \
                     argument {i} (indices {:?}, expected 0..{arity})",
                    slots.keys().collect::<Vec<_>>()
                ))
            })?;
            args.push(value.clone());
        }
        out.push(NaryTuple {
            relation: relation.clone(),
            args,
        });
    }
    out.sort_by_key(tuple_sort_key);
    Ok(out)
}

/// The canonical, engine-independent sort key of an n-ary tuple.
fn tuple_sort_key(t: &NaryTuple) -> (String, Vec<String>) {
    (
        t.relation.clone(),
        t.args.iter().map(term_display).collect(),
    )
}

// ── Steps-carrying ingestion entry ────────────────────────────────────────────

/// The de-reified n-ary closure plus the chase's consumed step count — the decomposable
/// signal a benchmark harness folds into its deterministic artifact next to the
/// verdict-agreement token (mirroring the ternary [`crate::cost::NativeForwardRun`]).
#[derive(Debug, Clone, PartialEq)]
pub struct NativeNaryRun {
    /// The closure (asserted EDB ∪ derived tuples), de-reified back to n-ary tuples,
    /// in the canonical `(relation, arg-displays)` order.
    pub tuples: Vec<NaryTuple>,
    /// The restricted chase's committed-derivation count (the single deterministic
    /// counting point — one charge per committed reified fact).
    pub consumed_steps: u64,
}

/// Run the native restricted chase over an n-ary EDB + program like
/// [`run_native_nary_forward`], but ALSO surface the chase's `consumed_steps` — the
/// decomposable cost signal the engine-bench harness records alongside the closure.
///
/// # Errors
///
/// Identical to [`run_native_nary_forward`]: an uncertified program, a lowering refusal,
/// a declined rule set, or a de-reification failure.
pub fn run_native_nary_forward_run(
    edb: &[NaryTuple],
    rules: &[NaryRule],
) -> gmeow_errors::Result<NativeNaryRun> {
    let admission = certify_nary_termination(rules)?;
    if !admission.admits_native() {
        return Err(nary_err(format!(
            "run_native_nary_forward: n-ary program is not certified terminating (weak \
             acyclicity); refusing to chase unbudgeted rather than risk non-termination: {}",
            admission.to_finding().message
        )));
    }

    let edb_facts = lower_nary_edb(edb)?;
    let reified_rules = lower_nary_rules(rules)?;
    let outcome = chase_world(NARY_WORLD, &edb_facts, &reified_rules, None)?;
    let budgeted = match outcome {
        NativeOutcome::Decided(budgeted) => budgeted,
        NativeOutcome::Unsupported(kind) => {
            return Err(nary_err(format!(
                "run_native_nary_forward: native chase declined the reified program \
                 ({kind:?}) — a certified-terminating program must be decidable natively"
            )));
        }
    };
    let consumed_steps = budgeted.consumed_steps;
    let tuples = dereify_rows(&budgeted.rows)?;
    Ok(NativeNaryRun {
        tuples,
        consumed_steps,
    })
}

// ── Null-blind canonicalization (colour refinement) ───────────────────────────

/// Whether a term's rendered display string is a native chase Skolem IRI.
fn is_null_display(d: &str) -> bool {
    d.contains("/skolem/")
}

/// Canonicalize an n-ary tuple set to a null-blind MULTISET by colour refinement of the
/// null-labeled tuple hypergraph: a null's colour is the fixpoint of the multiset of
/// `(relation, its position, the colours of every argument)` contexts it occurs in,
/// grounded in the named terms. Isomorphic null structures converge to equal colours,
/// non-isomorphic ones never do, and witness MULTIPLICITY is preserved by the count.
#[must_use]
pub fn canonical_null_blind_multiset(
    tuples: &[NaryTuple],
) -> BTreeMap<(String, Vec<String>), usize> {
    // Pre-render every tuple's per-argument display string (and its null-ness) exactly ONCE.
    // The colour-refinement fixpoint below re-scans every tuple's arguments once per null per
    // iteration; without this cache that re-ran `term_display` O(iterations × N × T × A) times.
    // `disp[ti][ai]` / `isnull[ti][ai]` are indexed identically to `tuples[ti].args[ai]`, so
    // every lookup below reads the SAME string `term_display` would have produced in place.
    let disp: Vec<Vec<String>> = tuples
        .iter()
        .map(|t| t.args.iter().map(term_display).collect())
        .collect();
    let isnull: Vec<Vec<bool>> = disp
        .iter()
        .map(|row| row.iter().map(|s| is_null_display(s)).collect())
        .collect();

    let nulls: std::collections::BTreeSet<String> = disp
        .iter()
        .zip(&isnull)
        .flat_map(|(row, nrow)| row.iter().zip(nrow))
        .filter(|&(_, &n)| n)
        .map(|(s, _)| s.clone())
        .collect();

    // Seed colours: a named term anchors on its own surface; a null starts uniform.
    let mut colour: BTreeMap<String, String> = BTreeMap::new();
    for (row, nrow) in disp.iter().zip(&isnull) {
        for (s, &n) in row.iter().zip(nrow) {
            colour
                .entry(s.clone())
                .or_insert_with(|| if n { "\u{0}".to_owned() } else { s.clone() });
        }
    }

    if !nulls.is_empty() {
        for _ in 0..=nulls.len() {
            let mut next = colour.clone();
            let mut changed = false;
            for n in &nulls {
                let mut sig: Vec<String> = Vec::new();
                for (ti, t) in tuples.iter().enumerate() {
                    let ctx: Vec<String> = disp[ti].iter().map(|s| colour[s].clone()).collect();
                    for (p, s) in disp[ti].iter().enumerate() {
                        if s == n {
                            sig.push(format!(
                                "{}\u{1f}{p}\u{1f}{}",
                                t.relation,
                                ctx.join("\u{1f}")
                            ));
                        }
                    }
                }
                sig.sort();
                let refined = crate::provenance::sha1_hex(&sig.join("\u{1e}"));
                if next[n] != refined {
                    changed = true;
                    next.insert(n.clone(), refined);
                }
            }
            colour = next;
            if !changed {
                break;
            }
        }
    }

    let distinct: std::collections::BTreeSet<String> =
        nulls.iter().map(|n| colour[n].clone()).collect();
    let token: BTreeMap<String, String> = distinct
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, format!("gmeow:null#{i}")))
        .collect();

    let mut ms: BTreeMap<(String, Vec<String>), usize> = BTreeMap::new();
    for (ti, t) in tuples.iter().enumerate() {
        let args: Vec<String> = disp[ti]
            .iter()
            .zip(&isnull[ti])
            .map(|(s, &n)| {
                if n {
                    token[&colour[s]].clone()
                } else {
                    s.clone()
                }
            })
            .collect();
        *ms.entry((t.relation.clone(), args)).or_insert(0) += 1;
    }
    ms
}

/// A deterministic, null-blind fingerprint of an n-ary closure. Built from
/// [`canonical_null_blind_multiset`] and hashed via the shared SHA-1 hex digest, so it is
/// a pure function of the closure and drift-gate-stable.
#[must_use]
pub fn nary_canonical_fingerprint(tuples: &[NaryTuple]) -> String {
    let ms = canonical_null_blind_multiset(tuples);
    let mut payload = String::new();
    for ((relation, args), count) in &ms {
        payload.push_str(relation);
        payload.push('\u{1f}');
        payload.push_str(&args.join("\u{1f}"));
        payload.push('\u{1f}');
        payload.push_str(&count.to_string());
        payload.push('\u{1e}');
    }
    crate::provenance::sha1_hex(&payload)
}

#[cfg(test)]
mod tests;
