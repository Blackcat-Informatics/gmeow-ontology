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

/// One ground stored row plus its cached surface and provenance.
struct GenericEntry {
    relation: String,
    row: Vec<TermValue>,
    surface: RowSurface,
    /// `None` for an asserted EDB row; `Some(rule_iri)` for a derived one.
    rule_iri: Option<String>,
}

/// The arity-generic fact store: insertion-ordered entries, an O(1) dedup key set,
/// and a per-relation index of entry positions (so a join scans only a relation's
/// rows).
#[derive(Default)]
struct GenericStore {
    entries: Vec<GenericEntry>,
    keys: HashSet<(String, RowSurface)>,
    by_relation: HashMap<String, Vec<usize>>,
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
        self.entries.push(GenericEntry {
            relation,
            row,
            surface,
            rule_iri,
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

/// Match `atom` against ground `row` under `base`, returning the extended binding or
/// `None`. A repeated variable must agree (byte-equal N3 surface); a constant must
/// equal the row term's surface exactly; arity must match.
fn match_generic_atom(atom: &GenericAtom, row: &[TermValue], base: &Binding) -> Option<Binding> {
    if atom.args.len() != row.len() {
        return None;
    }
    let mut binding = base.clone();
    for (pat, value) in atom.args.iter().zip(row.iter()) {
        match pat {
            EvalTerm::ConstNamed(iri) => {
                if term_display(value) != format!("<{iri}>") {
                    return None;
                }
            }
            EvalTerm::ConstLit(lit) => {
                if term_display(value) != term_display(lit) {
                    return None;
                }
            }
            EvalTerm::Var(name) => match binding.iter().find(|(k, _)| k == name) {
                Some((_, existing)) => {
                    if term_display(existing) != term_display(value) {
                        return None;
                    }
                }
                None => binding.push((name.clone(), value.clone())),
            },
        }
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
    solutions: &[Binding],
) -> Vec<Binding> {
    let indices = store.indices_for(&atom.relation);
    let mut next: Vec<Binding> = Vec::new();
    for sol in solutions {
        for &i in indices {
            let in_delta = delta.contains(&i);
            let keep = match scan {
                Scan::Delta => in_delta,
                Scan::Full => true,
                Scan::OldOnly => !in_delta,
            };
            if !keep {
                continue;
            }
            if let Some(merged) = match_generic_atom(atom, &store.entries[i].row, sol) {
                next.push(merged);
            }
        }
    }
    next
}

/// Join a rule's positive body against `store`, semi-naive: the union over each body
/// position `p` of `{ a_p ∈ delta, a_{<p} ∈ full, a_{>p} ∈ store∖delta }`. A bodyless
/// rule yields nothing (the empty solution never touches delta) — RL/RDF has no
/// bodyless rules, so this is inert there.
fn join_body(rule: &GenericRule, store: &GenericStore, delta: &HashSet<usize>) -> Vec<Binding> {
    let k = rule.body.len();
    if k == 0 {
        return Vec::new();
    }
    let mut all: Vec<Binding> = Vec::new();
    for p in 0..k {
        let mut partial: Vec<Binding> = vec![Vec::new()];
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
/// mirroring Nemo, which `rl_closure` relies on (asserted ∪ derived). Antecedents are
/// left empty at this seam (parity compares FACTS, not provenance). Rows are sorted
/// canonically by `(relation, row-surface)` for a byte-stable result.
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
        if let Some(idx) = store.insert(fact.predicate.clone(), row, None) {
            delta.insert(idx);
        }
    }

    // Semi-naive least-fixpoint: each round derives new heads by joining only against
    // the previous round's delta, committing per-round winners in sorted-key order.
    loop {
        let mut round: BTreeMap<(String, RowSurface), (Vec<TermValue>, String)> = BTreeMap::new();
        for rule in rules {
            for sol in join_body(rule, &store, &delta) {
                let row = ground_head(&rule.head, &sol)?;
                let surface: RowSurface = row.iter().map(term_display).collect();
                let key = (rule.head.relation.clone(), surface);
                if store.contains(&key.0, &key.1) {
                    continue; // a prior round already derived it; earlier round wins
                }
                // First rule/solution (in rule then enumeration order) wins provenance.
                round
                    .entry(key)
                    .or_insert_with(|| (row, rule.rule_iri.clone()));
            }
        }
        if round.is_empty() {
            break; // fixpoint
        }
        let mut new_delta: HashSet<usize> = HashSet::with_capacity(round.len());
        for ((relation, _surface), (row, rule_iri)) in round {
            if let Some(idx) = store.insert(relation, row, Some(rule_iri)) {
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
            (
                TypedRow {
                    predicate: entry.relation.clone(),
                    args: entry.row.clone(),
                },
                TypedProvenance {
                    is_edb,
                    rule_name: entry.rule_iri.clone(),
                    antecedents: Vec::new(),
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
