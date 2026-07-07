// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native **arity-generic positive-Datalog** forward evaluator.
//!
//! # Why a second evaluator (not the binary [`crate::physical::seminaive`] core)
//!
//! The binary semi-naive engine ([`crate::rule_ir::EvalAtom`]) keys every relation
//! by a CONSTANT predicate NAME and drops the world slot — it is structurally
//! incapable of representing an OWL 2 RL/RDF meta-rule that binds the *property
//! position to a VARIABLE* (`prp-spo1`, `prp-dom`, `prp-trp`, …). [`crate::reason::rl`]
//! re-encodes those rules as the 4-ary generic-triple relation
//! `triple(?s, ?p, ?o, ?w)` with the predicate carried as a DATA term in `args[1]`.
//! This module evaluates exactly that shape natively: an atom is a relation NAME
//! applied to a positional term vector, and a variable in ANY position — including
//! the predicate-bearing `args[1]` — falls out for free.
//!
//! The EL/DL lane stays on the binary core UNCHANGED; the [`crate::oracle::NativeForwardOracle`]
//! dispatches to THIS evaluator only for the generic (non-ternary) EDB encoding.
//!
//! # The fragment (positive Datalog only)
//!
//! OWL 2 RL/RDF as [`crate::reason::rl::RL_RULES`] carries it is pure positive
//! Datalog over two arity-generic relations (`triple/4`, `list_member/3`): NO
//! negation, NO builtins/arithmetic, NO existentials, NO inequality guards. A parsed
//! rule that somehow carries a negated body atom is a rule-text bug and is a HARD
//! ERROR ([`parse_generic_rules`]) — never silently dropped or approximated.
//!
//! # Determinism
//!
//! Rows are interned-order-free: a relation's rows are stored insertion-ordered and
//! O(1)-deduped on their N3-surface tuple, rules fire in parse order, per-round
//! winners commit in sorted-key order, and the emitted [`crate::oracle::TypedChaseResult`]
//! is sorted canonically by `(relation, row-surface)`. The least model is unique, so
//! the FACT set is engine-order-independent; the sort makes the row ORDER stable too.
//!
//! # Semi-naive fixpoint
//!
//! Standard delta × full position-decomposition (mirroring
//! [`crate::rule_ir::least_model_of_reduct`], generalized to n-ary): for each positive
//! body atom position `p`, the union over `p` of
//! `{ a_p ∈ delta, a_{<p} ∈ full, a_{>p} ∈ store∖delta }` visits every delta-touching
//! solution exactly once, so each round joins only against newly-derived facts.

use std::collections::{BTreeMap, HashMap, HashSet};

use purrdf::TermValue;

use crate::facts::TypedFactSet;
use crate::oracle::{TypedChaseResult, TypedProvenance, TypedRow};
use crate::provenance::{LOGIC_NAMESPACE, term_display};
use crate::rule_ir::{EvalTerm, lower_nemo_term};

// ── Generic atom / rule IR ─────────────────────────────────────────────────────

/// One arity-generic atom: a relation NAME applied to a positional term vector.
///
/// Unlike [`crate::rule_ir::EvalAtom`] (which pins subject/predicate/object and
/// drops the world), this keeps EVERY term: the predicate position is simply
/// `args[1]`, and a [`EvalTerm::Var`] there is an ordinary variable the join binds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenericAtom {
    /// The relation name (a program-local symbol like `triple` / `list_member`, or
    /// a full predicate IRI — whatever the rule text writes in predicate position).
    pub(crate) relation: String,
    /// The positional argument terms (a var, a constant IRI, or a constant literal).
    pub(crate) args: Vec<EvalTerm>,
}

/// A lowered positive-Datalog rule: one head atom, a positive body, and the firing
/// rule IRI (`#[name("...")]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GenericRule {
    /// The single head atom.
    pub(crate) head: GenericAtom,
    /// The positive body atoms (positive Datalog — no negation).
    pub(crate) body: Vec<GenericAtom>,
    /// The firing rule IRI (the `#[name(...)]` value, or a synthesized anonymous IRI).
    pub(crate) rule_iri: String,
}

// ── Parsing (shared Nemo front-end, arity-preserving) ──────────────────────────

/// Lower one Nemo atom into a [`GenericAtom`], KEEPING ALL terms.
///
/// The relation name is `atom.predicate()`; every argument is lowered with the SAME
/// [`crate::rule_ir::lower_nemo_term`] codec the binary IR uses (with `slot =
/// "object"`, permissive — a generic relation may carry a literal in any position).
fn lower_generic_atom(
    atom: &nemo::rule_model::components::atom::Atom,
) -> Result<GenericAtom, String> {
    let relation = atom.predicate().to_string();
    let mut args: Vec<EvalTerm> = Vec::new();
    for term in atom.terms() {
        args.push(lower_nemo_term(term, "object")?);
    }
    Ok(GenericAtom { relation, args })
}

/// Parse `.rls`-style rule text into the arity-generic IR via the SAME Nemo parser
/// the binary [`crate::rule_ir::parse_eval_rules`] uses, so the predicate / variable
/// surface is byte-identical to the engine — but WITHOUT the arity-3 restriction, so
/// the 4-ary `triple` / 3-ary `list_member` relations and the variable
/// predicate-position survive.
///
/// # Errors
///
/// Returns the Nemo parse-error string, a lowering error, or — because this fragment
/// is pure positive Datalog — a HARD ERROR if any rule carries a negated body atom
/// (RL/RDF has none; a negated atom would be a rule-text bug, never approximated).
pub(crate) fn parse_generic_rules(rules: &str) -> Result<Vec<GenericRule>, String> {
    use crate::nemo_engine::NemoParsedRules;
    use nemo::rule_model::programs::ProgramRead;

    let program = NemoParsedRules::parse_unvalidated(rules)?.into_program();

    let mut out: Vec<GenericRule> = Vec::new();
    for rule in program.rules() {
        let head_atom = rule
            .head()
            .first()
            .ok_or("generic: rule has no head atom")?;
        let head = lower_generic_atom(head_atom)?;

        // Positive Datalog only: a negated body literal is a hard error (RL/RDF is
        // negation-free; this evaluator must never see one, and must never silently
        // treat it as positive).
        if rule.body_negative().count() != 0 {
            return Err(format!(
                "generic: rule {:?} carries a negated body atom, but the generic n-ary \
                 evaluator is pure positive Datalog (OWL 2 RL/RDF has no negation)",
                rule.name().unwrap_or_else(|| "<anonymous>".to_owned())
            ));
        }

        let mut body: Vec<GenericAtom> = Vec::new();
        for atom in rule.body_positive() {
            body.push(lower_generic_atom(atom)?);
        }

        let rule_iri = rule
            .name()
            .unwrap_or_else(|| format!("{LOGIC_NAMESPACE}rule/anonymous"));

        out.push(GenericRule {
            head,
            body,
            rule_iri,
        });
    }
    Ok(out)
}

// ── Generic store (relation-keyed, insertion-ordered, N3-surface deduped) ──────

/// The N3-surface tuple of a ground row — the dedup / delta key.
type RowSurface = Vec<String>;

/// A per-round winning derivation: `(head-row, firing rule IRI, matched
/// antecedent rows)`, keyed by `(relation, head-surface)` in the round map.
type RoundWinner = (Vec<TermValue>, String, Vec<GenericAntecedent>);

/// One ground antecedent row consumed by a firing: `(relation, row-terms)`.
///
/// The pre-reifier premise the [`crate::oracle::ForwardOracle`] seam re-exposes as a
/// [`crate::oracle::TypedRow`] so the materialize consumer can mint reifiers (and,
/// crucially, HARD-FAIL on a non-ternary antecedent of a ternary head — a binary
/// helper premise has no world-scoped reifier).
type GenericAntecedent = (String, Vec<TermValue>);

/// One ground stored row plus its cached surface and provenance.
struct GenericEntry {
    relation: String,
    row: Vec<TermValue>,
    surface: RowSurface,
    /// `None` for an asserted EDB row; `Some(rule_iri)` for a derived one.
    rule_iri: Option<String>,
    /// The matched positive body rows of the winning firing, in body order —
    /// empty for an EDB row (no antecedents).  Carried so the oracle seam can
    /// re-expose each as a decoded antecedent `TypedRow`.
    antecedents: Vec<GenericAntecedent>,
}

/// The arity-generic fact store: insertion-ordered entries, an O(1) dedup key set,
/// a per-relation index of entry positions (so a join scans only a relation's rows),
/// and — the join accelerator — a per-`(relation, column-position, N3-surface)` value
/// index mapping a bound term to the entry indices bearing it there.
///
/// Every index bucket keeps its entry indices in INSERTION ORDER (ascending): a join
/// that iterates a bucket instead of the full relation therefore visits exactly the
/// rows the full scan would have MATCHED, in the SAME order, so the produced solution
/// sequence — and hence every committed head, per-round winner, and antecedent list —
/// is byte-identical to a full scan. This is a pure speed change.
#[derive(Default)]
struct GenericStore {
    entries: Vec<GenericEntry>,
    keys: HashSet<(String, RowSurface)>,
    by_relation: HashMap<String, Vec<usize>>,
    /// `relation → (column-position, term N3-surface) → ascending entry indices`.
    by_pos_value: HashMap<String, HashMap<(usize, String), Vec<usize>>>,
}

impl GenericStore {
    fn contains(&self, relation: &str, surface: &RowSurface) -> bool {
        self.keys.contains(&(relation.to_owned(), surface.clone()))
    }

    /// Insert `relation(row)` if new; return the new entry's index, or `None` if it
    /// was already present.
    fn insert(
        &mut self,
        relation: String,
        row: Vec<TermValue>,
        rule_iri: Option<String>,
        antecedents: Vec<GenericAntecedent>,
    ) -> Option<usize> {
        let surface: RowSurface = row.iter().map(term_display).collect();
        if !self.keys.insert((relation.clone(), surface.clone())) {
            return None;
        }
        let idx = self.entries.len();
        self.by_relation
            .entry(relation.clone())
            .or_default()
            .push(idx);
        // Value index: register this entry's surface at every column position, each
        // bucket staying ascending (idx grows monotonically).
        let rel_index = self.by_pos_value.entry(relation.clone()).or_default();
        for (pos, s) in surface.iter().enumerate() {
            rel_index.entry((pos, s.clone())).or_default().push(idx);
        }
        self.entries.push(GenericEntry {
            relation,
            row,
            surface,
            rule_iri,
            antecedents,
        });
        Some(idx)
    }

    fn indices_for(&self, relation: &str) -> &[usize] {
        self.by_relation
            .get(relation)
            .map_or(&[][..], Vec::as_slice)
    }
}

// ── Join engine (arity-generic, positional unification) ────────────────────────

/// A partial solution: variable name → bound native term.
type Binding = Vec<(String, TermValue)>;

/// A partial solution paired with the antecedent rows it has matched so far, in
/// body order.  The join accumulates both in lockstep so a committed head can
/// record exactly the body rows that produced it (its immediate premises).
#[derive(Clone)]
struct GenSolution {
    binding: Binding,
    antecedents: Vec<GenericAntecedent>,
}

/// The per-position matching obligation for one atom under a fixed partial solution,
/// precomputed ONCE per solution (not per scanned row) so the hot loop never
/// re-formats a constant or re-resolves an already-bound variable.
enum PosMatch {
    /// A position pinned to an exact N3 surface — a constant, or a variable already
    /// bound in the partial solution. The row must carry this surface here.
    Expect(String),
    /// A position holding a variable not yet bound — the row's term here binds it
    /// (or, on a repeated occurrence within the atom, must agree with the first).
    Fresh(String),
}

/// Precompute the per-position match plan of `atom` under `base`: constants and
/// already-bound variables collapse to their fixed surface; still-free variables
/// stay `Fresh`. This mirrors [`match_generic_atom`]'s per-term dispatch but hoists
/// the constant-format / bound-var-resolve out of the row loop.
fn build_match_plan(atom: &GenericAtom, base: &Binding) -> Vec<PosMatch> {
    atom.args
        .iter()
        .map(|pat| match pat {
            EvalTerm::ConstNamed(iri) => PosMatch::Expect(format!("<{iri}>")),
            EvalTerm::ConstLit(lit) => PosMatch::Expect(term_display(lit)),
            EvalTerm::Var(name) => match base.iter().find(|(k, _)| k == name) {
                Some((_, existing)) => PosMatch::Expect(term_display(existing)),
                None => PosMatch::Fresh(name.clone()),
            },
        })
        .collect()
}

/// Match `entry` against a precomputed `plan` under `base`, returning the extended
/// binding or `None`. Constants / bound vars compare against the entry's CACHED
/// surface (no per-row `term_display`); a repeated free variable must agree
/// (byte-equal surface); arity must match. Byte-identical to the old
/// [`match_generic_atom`]: the merged binding is `base` followed by each newly-bound
/// variable in first-occurrence position order.
fn match_plan(plan: &[PosMatch], entry: &GenericEntry, base: &Binding) -> Option<Binding> {
    if plan.len() != entry.row.len() {
        return None;
    }
    // First position at which each fresh variable was bound in THIS row (so a
    // repeated occurrence can compare surfaces without re-formatting).
    let mut fresh: Vec<(&str, usize)> = Vec::new();
    for (pos, pm) in plan.iter().enumerate() {
        match pm {
            PosMatch::Expect(surface) => {
                if entry.surface[pos] != *surface {
                    return None;
                }
            }
            PosMatch::Fresh(name) => match fresh.iter().find(|(k, _)| *k == name.as_str()) {
                Some(&(_, first)) => {
                    if entry.surface[first] != entry.surface[pos] {
                        return None;
                    }
                }
                None => fresh.push((name.as_str(), pos)),
            },
        }
    }
    let mut binding = base.clone();
    for (name, pos) in fresh {
        binding.push((name.to_owned(), entry.row[pos].clone()));
    }
    Some(binding)
}

/// Which rows of a body atom's relation a scan position walks (semi-naive
/// decomposition).
enum Scan {
    /// Only rows added in the previous round (the "new at p" position).
    Delta,
    /// Any row in the store (the `j < p` positions).
    Full,
    /// Only rows NOT added last round (the `j > p` positions).
    OldOnly,
}

/// Extend each partial solution by matching `atom` against its relation's rows under
/// `scan`, gated by `delta` (a set of entry indices added in the previous round).
fn extend(
    atom: &GenericAtom,
    store: &GenericStore,
    delta: &HashSet<usize>,
    scan: &Scan,
    solutions: &[GenSolution],
) -> Vec<GenSolution> {
    // The relation's value index (positions → surface → ascending entry indices); the
    // whole-relation scan is the fallback when no atom position is bound.
    let rel_index = store.by_pos_value.get(&atom.relation);
    let mut next: Vec<GenSolution> = Vec::new();
    for sol in solutions {
        let plan = build_match_plan(atom, &sol.binding);

        // Index-selection: over the atom's BOUND positions (constants + already-bound
        // vars) pick the smallest value bucket — that is the tightest candidate set,
        // and iterating it in its ascending order visits exactly the rows a full scan
        // would have matched, in the same order. A bound position with an EMPTY bucket
        // means no row can match, so this solution contributes nothing.
        let mut selected: Option<&[usize]> = None;
        let mut any_bound = false;
        let mut empty = false;
        for (pos, pm) in plan.iter().enumerate() {
            if let PosMatch::Expect(surface) = pm {
                any_bound = true;
                match rel_index.and_then(|m| m.get(&(pos, surface.clone()))) {
                    None => {
                        empty = true;
                        break;
                    }
                    Some(bucket) => {
                        if selected.is_none_or(|cur| bucket.len() < cur.len()) {
                            selected = Some(bucket.as_slice());
                        }
                    }
                }
            }
        }
        if empty {
            continue;
        }
        let candidates: &[usize] = if any_bound {
            // `any_bound` with no `empty` guarantees a chosen bucket.
            selected.unwrap_or(&[])
        } else {
            // No bound position (e.g. RL's leading `triple(?s,?p,?o,?w)` all-free
            // atom): unavoidable full scan of the relation.
            store.indices_for(&atom.relation)
        };

        for &i in candidates {
            let in_delta = delta.contains(&i);
            let keep = match scan {
                Scan::Delta => in_delta,
                Scan::Full => true,
                Scan::OldOnly => !in_delta,
            };
            if !keep {
                continue;
            }
            let entry = &store.entries[i];
            if let Some(merged) = match_plan(&plan, entry, &sol.binding) {
                // Record the matched body row (relation + terms) as an antecedent,
                // appended in body order (only once a match commits).
                let mut antecedents = sol.antecedents.clone();
                antecedents.push((entry.relation.clone(), entry.row.clone()));
                next.push(GenSolution {
                    binding: merged,
                    antecedents,
                });
            }
        }
    }
    next
}

/// Join a rule's positive body against `store`, semi-naive: the union over each body
/// position `p` of `{ a_p ∈ delta, a_{<p} ∈ full, a_{>p} ∈ store∖delta }`. A bodyless
/// rule yields nothing (the empty solution never touches delta) — RL/RDF has no
/// bodyless rules, so this is inert there.
fn join_body(rule: &GenericRule, store: &GenericStore, delta: &HashSet<usize>) -> Vec<GenSolution> {
    let k = rule.body.len();
    if k == 0 {
        return Vec::new();
    }
    let mut all: Vec<GenSolution> = Vec::new();
    for p in 0..k {
        let mut partial: Vec<GenSolution> = vec![GenSolution {
            binding: Vec::new(),
            antecedents: Vec::new(),
        }];
        for (j, atom) in rule.body.iter().enumerate() {
            let scan = if j < p {
                Scan::Full
            } else if j == p {
                Scan::Delta
            } else {
                Scan::OldOnly
            };
            partial = extend(atom, store, delta, &scan, &partial);
            if partial.is_empty() {
                break;
            }
        }
        all.extend(partial);
    }
    all
}

/// Ground a head atom into a concrete row under `sol`, failing hard on an unbound
/// head variable.
fn ground_head(head: &GenericAtom, sol: &Binding) -> Result<Vec<TermValue>, String> {
    let mut row: Vec<TermValue> = Vec::with_capacity(head.args.len());
    for arg in &head.args {
        match arg {
            EvalTerm::ConstNamed(iri) => row.push(TermValue::iri(iri.clone())),
            EvalTerm::ConstLit(lit) => row.push(lit.clone()),
            EvalTerm::Var(name) => {
                let value = sol
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| {
                        format!("generic: head variable {name:?} unbound after body matching")
                    })?;
                row.push(value);
            }
        }
    }
    Ok(row)
}

// ── Materialization entry point ────────────────────────────────────────────────

/// Materialize the least model of `rules` over the generic n-ary EDB `facts`,
/// returning the same [`TypedChaseResult`] shape [`crate::oracle::NativeForwardOracle`]
/// coerces back — each row is `TypedRow { predicate: relation_name, args: row }`
/// (so `triple` rows stay arity-4 and `list_member` rows arity-3, exactly what
/// [`crate::reason::rl::rl_closure`] expects).
///
/// The output includes BOTH the echoed EDB rows (`is_edb = true`, `rule_name =
/// None`) and every derived row (`is_edb = false`, `rule_name = Some(rule_iri)`) —
/// mirroring Nemo, which `rl_closure` relies on (asserted ∪ derived). Each derived
/// row carries its matched body rows as antecedents (the production provenance the
/// materialize consumer mints reifiers from — full-arity, so a non-ternary premise
/// hard-fails rather than fabricating a reifier); parity still compares FACTS, not
/// provenance. Rows are sorted canonically by `(relation, row-surface)` for a
/// byte-stable result.
pub(crate) fn materialize_generic(
    facts: &TypedFactSet,
    rules: &[GenericRule],
) -> Result<TypedChaseResult, String> {
    let interner = facts.interner();
    let mut store = GenericStore::default();

    // Seed the EDB: every typed fact becomes a stored row `relation(args…)` with the
    // native term values resolved from the set's interner.
    let mut delta: HashSet<usize> = HashSet::new();
    for fact in facts.facts() {
        let row: Vec<TermValue> = fact
            .args
            .iter()
            .map(|&id| interner.resolve(id).clone())
            .collect();
        if let Some(idx) = store.insert(fact.predicate.clone(), row, None, Vec::new()) {
            delta.insert(idx);
        }
    }

    // Semi-naive least-fixpoint: each round derives new heads by joining only against
    // the previous round's delta, committing per-round winners in sorted-key order.
    loop {
        let mut round: BTreeMap<(String, RowSurface), RoundWinner> = BTreeMap::new();
        for rule in rules {
            for sol in join_body(rule, &store, &delta) {
                let row = ground_head(&rule.head, &sol.binding)?;
                let surface: RowSurface = row.iter().map(term_display).collect();
                let key = (rule.head.relation.clone(), surface);
                if store.contains(&key.0, &key.1) {
                    continue; // a prior round already derived it; earlier round wins
                }
                // First rule/solution (in rule then enumeration order) wins provenance
                // — its matched body rows become the head's recorded antecedents.
                round
                    .entry(key)
                    .or_insert_with(|| (row, rule.rule_iri.clone(), sol.antecedents));
            }
        }
        if round.is_empty() {
            break; // fixpoint
        }
        let mut new_delta: HashSet<usize> = HashSet::with_capacity(round.len());
        for ((relation, _surface), (row, rule_iri, antecedents)) in round {
            if let Some(idx) = store.insert(relation, row, Some(rule_iri), antecedents) {
                new_delta.insert(idx);
            }
        }
        delta = new_delta;
    }

    // Emit every stored row (EDB echo ∪ derived) as a TypedRow, sorted canonically.
    let mut order: Vec<usize> = (0..store.entries.len()).collect();
    order.sort_by(|&a, &b| {
        let ea = &store.entries[a];
        let eb = &store.entries[b];
        (ea.relation.as_str(), &ea.surface).cmp(&(eb.relation.as_str(), &eb.surface))
    });

    let rows = order
        .into_iter()
        .map(|i| {
            let entry = &store.entries[i];
            let is_edb = entry.rule_iri.is_none();
            // Re-expose each matched body row as an antecedent `TypedRow`, KEEPING
            // its full arity (a ternary `subClassOf(s,o,w)` premise stays arity-3;
            // a binary `helper(x,y)` premise stays arity-2 so the materialize
            // consumer hard-fails on it rather than minting a bogus reifier).
            let antecedents = entry
                .antecedents
                .iter()
                .map(|(relation, row)| TypedRow {
                    predicate: relation.clone(),
                    args: row.clone(),
                })
                .collect();
            (
                TypedRow {
                    predicate: entry.relation.clone(),
                    args: entry.row.clone(),
                },
                TypedProvenance {
                    is_edb,
                    rule_name: entry.rule_iri.clone(),
                    antecedents,
                    attributions: Vec::new(),
                },
            )
        })
        .collect();

    Ok(TypedChaseResult { rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reason::rl::RL_RULES;

    // ── generic-triple EDB helpers (the RL predicate-as-data encoding) ──────────

    const RL_W: &str = "urn:world:rl-generic";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const SUBPROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
    const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
    const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    const TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
    const SYMMETRIC: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
    const INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
    const ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
    const UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
    const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    const HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
    const LIT_SURROGATE: &str = "urn:gmeow-rl-lit:0";

    /// A tiny generic-triple EDB builder in the `triple(?s,?p,?o,?w)` encoding.
    #[derive(Default)]
    struct Edb {
        facts: TypedFactSet,
    }

    impl Edb {
        /// Push `triple(subject, predicate, object, world)` with IRI terms.
        fn t(&mut self, s: &str, p: &str, o: &str) -> &mut Self {
            self.push(s, p, TermValue::iri(o));
            self
        }

        /// Push a triple whose OBJECT is an already-interned literal-surrogate IRI —
        /// the shape [`crate::reason::rl::encode_generic_edb`] produces for a literal
        /// object (RL never inspects the literal value, only the surrogate IRI).
        fn t_lit(&mut self, s: &str, p: &str) -> &mut Self {
            self.push(s, p, TermValue::iri(LIT_SURROGATE));
            self
        }

        fn push(&mut self, s: &str, p: &str, o: TermValue) {
            let s = self.facts.intern(&TermValue::iri(s));
            let p = self.facts.intern(&TermValue::iri(p));
            let o = self.facts.intern(&o);
            let w = self.facts.intern(&TermValue::simple_literal(RL_W));
            self.facts.push_fact("triple", vec![s, p, o, w]);
        }

        /// Run the RL closure natively over this EDB, returning the derived triples.
        fn close(&self) -> TypedChaseResult {
            let rules = parse_generic_rules(RL_RULES).expect("RL rules parse");
            materialize_generic(&self.facts, &rules).expect("generic materialize")
        }
    }

    fn edb() -> Edb {
        Edb::default()
    }

    /// Whether the closure carries `triple(s, p, o)` (object an IRI) in any world.
    fn has(closure: &TypedChaseResult, s: &str, p: &str, o: &str) -> bool {
        has_obj(closure, s, p, &format!("<{o}>"))
    }

    /// Whether the closure carries `triple(s, p, <object-surface>)`.
    fn has_obj(closure: &TypedChaseResult, s: &str, p: &str, obj_surface: &str) -> bool {
        closure.rows.iter().any(|(row, _)| {
            row.predicate == "triple"
                && row.args.len() == 4
                && term_display(&row.args[0]) == format!("<{s}>")
                && term_display(&row.args[1]) == format!("<{p}>")
                && term_display(&row.args[2]) == obj_surface
        })
    }

    const A: &str = "http://ex/A";
    const B: &str = "http://ex/B";
    const C: &str = "http://ex/C";
    const P: &str = "http://ex/p";
    const P1: &str = "http://ex/p1";
    const P2: &str = "http://ex/p2";
    const X: &str = "http://ex/x";
    const Y: &str = "http://ex/y";
    const Z: &str = "http://ex/z";

    #[test]
    fn generic_cax_sco_propagates_type_through_subclass() {
        let mut e = edb();
        e.t(X, TYPE, A).t(A, SUBCLASS, B);
        let c = e.close();
        assert!(has(&c, X, TYPE, B), "x a B via cax-sco");
        // Echoed EDB present too.
        assert!(has(&c, X, TYPE, A), "asserted x a A echoed");
    }

    #[test]
    fn generic_scm_sco_is_transitive() {
        let mut e = edb();
        e.t(A, SUBCLASS, B).t(B, SUBCLASS, C);
        let c = e.close();
        assert!(has(&c, A, SUBCLASS, C), "A ⊑ C via scm-sco transitivity");
    }

    #[test]
    fn generic_prp_spo1_binds_a_variable_predicate() {
        // The load-bearing case: prp-spo1 quantifies over the PROPERTY position.
        let mut e = edb();
        e.t(P1, SUBPROP, P2).t(X, P1, Y);
        let c = e.close();
        assert!(
            has(&c, X, P2, Y),
            "x p2 y via prp-spo1 (variable predicate)"
        );
    }

    #[test]
    fn generic_prp_spo1_carries_a_literal_surrogate_object() {
        // prp-spo1 propagating a literal object (interned to a surrogate IRI): the
        // surrogate rides through the variable-predicate join unchanged.
        let mut e = edb();
        e.t(P1, SUBPROP, P2).t_lit(X, P1);
        let c = e.close();
        assert!(
            has_obj(&c, X, P2, &format!("<{LIT_SURROGATE}>")),
            "x p2 <lit-surrogate> via prp-spo1"
        );
    }

    #[test]
    fn generic_prp_trp_closes_a_transitive_chain() {
        let mut e = edb();
        e.t(P, TYPE, TRANSITIVE).t(X, P, Y).t(Y, P, Z);
        let c = e.close();
        assert!(has(&c, X, P, Z), "x p z via prp-trp");
    }

    #[test]
    fn generic_prp_symp_mirrors_a_symmetric_edge() {
        let mut e = edb();
        e.t(P, TYPE, SYMMETRIC).t(X, P, Y);
        let c = e.close();
        assert!(has(&c, Y, P, X), "y p x via prp-symp");
    }

    #[test]
    fn generic_prp_inv_derives_both_directions() {
        let mut e = edb();
        e.t(P1, INVERSE_OF, P2).t(X, P1, Y);
        let c = e.close();
        assert!(has(&c, Y, P2, X), "y p2 x via prp-inv1");
    }

    #[test]
    fn generic_prp_dom_and_rng_derive_types() {
        let mut e = edb();
        e.t(P, DOMAIN, A).t(P, RANGE, B).t(X, P, Y);
        let c = e.close();
        assert!(has(&c, X, TYPE, A), "x a A via prp-dom");
        assert!(has(&c, Y, TYPE, B), "y a B via prp-rng");
    }

    #[test]
    fn generic_cls_oneof_over_a_list_and_list_member() {
        // C oneOf ( x y ) ⇒ x a C, y a C — exercises list_member recursion + cls-oneOf.
        let l0 = "http://ex/l0";
        let l1 = "http://ex/l1";
        let mut e = edb();
        e.t(C, ONE_OF, l0)
            .t(l0, FIRST, X)
            .t(l0, REST, l1)
            .t(l1, FIRST, Y)
            .t(l1, REST, NIL);
        let c = e.close();
        assert!(has(&c, X, TYPE, C), "x a C via cls-oneOf");
        assert!(has(&c, Y, TYPE, C), "y a C via cls-oneOf");
    }

    #[test]
    fn generic_cls_union_member_subclasses_each_member() {
        let l0 = "http://ex/l0";
        let l1 = "http://ex/l1";
        let mut e = edb();
        e.t(C, UNION_OF, l0)
            .t(l0, FIRST, A)
            .t(l0, REST, l1)
            .t(l1, FIRST, B)
            .t(l1, REST, NIL);
        let c = e.close();
        assert!(has(&c, A, SUBCLASS, C), "A ⊑ C via cls-union-member");
        assert!(has(&c, B, SUBCLASS, C), "B ⊑ C via cls-union-member");
    }

    #[test]
    fn generic_cls_hasvalue_asserts_and_recognizes() {
        // R onProperty P ; R hasValue V ; x a R ⇒ x P V (cls-hv1);
        //                               z P V ⇒ z a R (cls-hv2).
        let r = "http://ex/R";
        let v = "http://ex/v";
        let mut e = edb();
        e.t(r, ON_PROPERTY, P)
            .t(r, HAS_VALUE, v)
            .t(X, TYPE, r)
            .t(Z, P, v);
        let c = e.close();
        assert!(has(&c, X, P, v), "x P V via cls-hv1");
        assert!(has(&c, Z, TYPE, r), "z a R via cls-hv2");
    }

    #[test]
    fn generic_parse_rejects_a_negated_body_atom() {
        // Positive Datalog only: a negated body atom is a hard error, never treated
        // as positive.
        let rls = "#[name(\"ex:neg\")]\n\
             <http://ex/q>(?x, ?y, ?w) :- <http://ex/p>(?x, ?y, ?w), ~<http://ex/r>(?x, ?y, ?w) .\n";
        let err = parse_generic_rules(rls).expect_err("a negated body atom must be rejected");
        assert!(err.contains("negated body atom"), "got: {err}");
    }
}
