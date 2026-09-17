// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared evaluable rule IR + Gelfond-Lifschitz reduct engine.
//!
//! This module is the native substrate for the two *non-stratifiable*
//! semantics the foundation chase cannot express: the well-founded model
//! (alternating fixpoint, [`crate::wellfounded`]) and the stable-model / answer-set
//! semantics ([`crate::stablemodel`]). Both evaluators consume the same typed
//! [`EvalRule`] IR as the stratified native chase.
//!
//! # Why native terms, not bare strings
//!
//! Unlike [`crate::foundation`] (whose facts are all-IRI and stored as bare
//! strings), the IR here works over the native [`TermValue`] (with predicate IRIs
//! as plain `String`) so literal object constants and the golden-pinned provenance
//! recipe ([`crate::provenance::mint_reifier`]) are handled for free.  The dedup key
//! is the exact native `(subject, predicate, object)` triple. Output rendering
//! never participates in deduplication, negation or model comparison.
//!
//! # The reduct least model (the crux)
//!
//! [`least_model_of_reduct`] is the generalized semi-naive join from
//! `foundation.rs`, with ONE change: a negated body atom blocks the rule iff its
//! grounded form is PRESENT in a **separate reference store**, NOT in the growing
//! store.  That is precisely the Gelfond-Lifschitz reduct: every NAF literal is
//! evaluated against a fixed guess `reference`, turning the program positive, and
//! the positive least model of that reduct is returned.  Both the well-founded
//! alternating fixpoint and the stable-model stability test are built on top of it.
//!
//! # Determinism
//!
//! Mirrors `foundation.rs`: EDB facts are seeded in sorted-key order, rules fire in
//! parse order, facts iterate in insertion order, and a head whose key already
//! exists is dropped (first-wins).  Provenance for each derived fact is the FIRST
//! firing's `(rule_iri, source_reifiers)`.
//!
//! # Internal execution surface
//!
//! Production materializers consume typed [`EvalRule`] values and emit
//! [`DerivedRow`] values. A few lower-level constructors and comparison helpers
//! remain intentionally available only to scratch-parity tests, so this module
//! keeps a crate-internal `dead_code` allowance rather than exporting them.
#![allow(dead_code)]

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::hash::{BuildHasher, Hash, Hasher};

use foldhash::fast::FixedState;
use hashbrown::HashTable;
use purrdf::TermValue;

use crate::facts::{PredId, PredInterner};
use crate::physical::bitset::DenseBitset;
use crate::physical::id::RowId;
use crate::provenance::{
    ASSERT_RULE_IRI, MinProofHeightSemiring, ProofHeight, mint_derivation_id, mint_reifier,
    term_display,
};
use crate::query_ir::QBuiltin;

/// Bridge an admitted complete RDF literal to the native value algebra.
/// No lexical rendering, parsing, datatype coercion or language/direction loss.
pub(crate) fn literal_value(literal: &purrdf::RdfLiteral) -> TermValue {
    TermValue::Literal {
        lexical_form: literal.lexical_form.clone(),
        datatype: literal.datatype_iri().to_owned(),
        language: literal
            .language
            .as_ref()
            .map(|tag| tag.to_ascii_lowercase()),
        direction: literal.direction,
    }
}

/// A set of [`FactKey`]s — the whole-model comparison form used by the well-founded
/// alternating fixpoint ([`crate::wellfounded`]) and the stable-model stability test
/// ([`crate::stablemodel`]).
///
/// This is a COLD-PATH structure: it is materialized only for coarse model-equality
/// checks (`k2.key_set() == k.key_set()`) and skeptical-intersection sweeps, never
/// probed on the per-candidate join path — that hot delta membership moved to a dense
/// [`DenseBitset`] over the store's insertion-order [`RowId`]s.  A `BTreeSet` gives
/// order-independent equality with no hasher, so no SipHash-keyed owned-key set
/// survives on any path; determinism still derives from the sorted round commit, never
/// from this set's iteration.
pub(crate) type FactKeySet = BTreeSet<FactKey>;

/// Fixed-seed hash of a [`FactKey`], for the borrowed-key `HashTable` probes in
/// [`FactStore`] and the per-round winner map — mirrors `facts::fact_key_hash` /
/// `physical::generic`'s borrowed-key probe and never clones the key to hash it.
///
/// The seed is fixed (`FixedState::default()`, never random) and NEVER persisted: it
/// backs pure membership probes, never an emission-order source.
pub(crate) fn fact_key_hash(key: &FactKey) -> u64 {
    fact_parts_hash(&key.0, &key.1, &key.2)
}

fn fact_parts_hash(subject: &TermValue, predicate: &str, object: &TermValue) -> u64 {
    let mut hasher = FixedState::default().build_hasher();
    subject.hash(&mut hasher);
    predicate.hash(&mut hasher);
    object.hash(&mut hasher);
    hasher.finish()
}

/// Wrap a runtime-IR condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn ir_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Ir { detail })
}

// ── Evaluable term / atom / rule ────────────────────────────────────────────────

/// A head/body term: a `?var` reference, a constant IRI, or a constant literal.
///
/// RDF ingress admits literals only in object position. Internal query relations are
/// positional rather than RDF triples, however, so a builtin-generated head value may be
/// a [`ConstLit`](EvalTerm::ConstLit) in either argument. Predicate identity remains an IRI.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum EvalTerm {
    /// A variable, e.g. `?X` (the string includes the leading `?`).
    Var(String),
    /// A constant IRI (the full IRI string).
    ConstNamed(String),
    /// A native non-IRI constant, including literals, scoped blanks and triple terms.
    ConstLit(#[serde(with = "crate::term_serde")] TermValue),
}

impl EvalTerm {
    pub(crate) fn var(name: &str) -> Self {
        Self::Var(name.to_owned())
    }

    pub(crate) fn named(iri: &str) -> Self {
        Self::ConstNamed(iri.to_owned())
    }
}

/// A single arity-3-derived atom, with the world slot dropped (subject, object).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct EvalAtom {
    /// The subject term (slot 0).
    pub(crate) subject: EvalTerm,
    /// The predicate IRI string (constant in the gmeow fragment).
    pub(crate) predicate: String,
    /// The object term (slot 1).
    pub(crate) object: EvalTerm,
    /// `true` iff this is a negation-as-failure body literal.
    pub(crate) negated: bool,
}

impl EvalAtom {
    pub(crate) fn positive(subject: EvalTerm, predicate: &str, object: EvalTerm) -> Self {
        Self {
            subject,
            predicate: predicate.to_owned(),
            object,
            negated: false,
        }
    }
}

/// A lowered rule: one head atom, an ordered body (positive atoms then negated
/// atoms), the firing rule IRI (from `#[name("...")]`), and inequality guards.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct EvalRule {
    /// Canonical finite RDF numeric operators, prepared once with the rule plan.
    pub(crate) numeric: Vec<gmeow_logic_compile::relational_core::RcNumeric>,
    /// Complete-group reduction, executed after the input stratum has completed.
    pub(crate) reduction: Option<Reduction>,
    /// The single head atom.
    pub(crate) head: EvalAtom,
    /// The body atoms, positive first then negated.
    pub(crate) body: Vec<EvalAtom>,
    /// The firing rule IRI (the `#[name(...)]` value, or a synthesized anonymous IRI).
    pub(crate) rule_iri: String,
    /// Inequality guards `(?A, ?B)`: a `distinctBody` (`?A != ?B`) constraint that
    /// blocks the rule's firing when the two variables bind to the same value.
    /// Populated by canonical-AST lowering directly from
    /// `LogicRule::distinct_pairs`, so the chase's guard is preserved structurally.
    pub(crate) distinct_pairs: Vec<(String, String)>,
    /// Arithmetic / comparison builtins, in body order, with variable operands in
    /// the engine's `?`-prefixed surface (matching the body atoms' [`EvalTerm::Var`]
    /// keys).  Evaluated as a post-join constraint stage: a generator (`is` with a
    /// free target) binds its target before the head is grounded, a filter prunes
    /// the solution. Empty for ordinary forward rules — populated solely by the
    /// backward magic transform ([`crate::physical::magic`]) and a constraint
    /// violation-rule lowering (see [`Self::constraint_tag`]).
    pub(crate) builtins: Vec<QBuiltin>,
    /// `Some(constraint_iri)` when this rule was lowered from a `logic:Constraint`'s
    /// `logic:integrity` formula whose consequent names a BUILTIN-BOUND reflection
    /// relation ([`crate::relational_core::lower_constraint_violation_rules`]) —
    /// `None` for every ordinary DL/Horn/formula-derived forward rule.
    ///
    /// The ONLY provenance tag that licenses a builtin inside the forward semi-naive
    /// chase (`physical::seminaive`'s `builtin_gap` invariant is keyed on it, never on
    /// "is this a Dim builtin") and that inverts a builtin's `Filter` outcome to
    /// VIOLATION-EMITTING semantics: the head marker materializes when the law's
    /// consequent builtin does NOT hold, and is suppressed when it does.
    pub(crate) constraint_tag: Option<String>,
}

mod reduction;
pub(crate) use reduction::Reduction;

impl EvalRule {
    pub(crate) fn positive(rule_iri: &str, head: EvalAtom, body: Vec<EvalAtom>) -> Self {
        Self {
            numeric: Vec::new(),
            head,
            body,
            rule_iri: rule_iri.to_owned(),
            distinct_pairs: Vec::new(),
            builtins: Vec::new(),
            reduction: None,
            constraint_tag: None,
        }
    }
}

// ── Ground fact + store (oxigraph-term based, insertion-ordered, first-wins) ─────

/// A fully-ground fact `(subject, predicate, object)` over native terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fact {
    /// The first binary-relation argument. RDF-sourced rows contain an IRI/blank node;
    /// facts-only backward evaluation may carry a generated literal here.
    pub(crate) subject: TermValue,
    /// The predicate IRI string.
    pub(crate) predicate: String,
    /// The object term (IRI or literal).
    pub(crate) object: TermValue,
}

/// The exact native dedup key of `(subject, predicate, object)`.
pub(crate) type FactKey = (TermValue, String, TermValue);

impl Fact {
    /// The owned native key for model sets and retained candidate records.
    pub(crate) fn key(&self) -> FactKey {
        (
            self.subject.clone(),
            self.predicate.clone(),
            self.object.clone(),
        )
    }

    /// The reifier IRI for this fact, via the golden-pinned recipe.
    pub(crate) fn reifier(&self) -> gmeow_errors::Result<String> {
        mint_reifier(&self.subject, &self.predicate, &self.object)
    }
}

/// Insertion-ordered fact store with O(1) dedup — mirrors `foundation.rs::FactStore`.
///
/// Dedup borrows native fields directly from the stored fact. The index owns only
/// row handles, with no duplicate native key or rendered surface per row. Delta
/// membership remains a dense [`DenseBitset`] over insertion-order row indices.
#[derive(Debug, Clone, Default)]
pub(crate) struct FactStore {
    facts: Vec<Fact>,
    /// Row handles hashed by their borrowed native fields.
    keys: HashTable<usize>,
    /// The store's predicate dictionary: predicate surface → dense [`PredId`].
    predicates: PredInterner,
    /// [`PredId`] (interned predicate surface) → row indices into `facts`, in
    /// insertion order.  Maintained in lockstep with `facts` so each bucket's order
    /// equals insertion order; this lets the join scan only the rows for a
    /// constant-predicate atom while returning exactly the subsequence (same relative
    /// order) a full scan would.
    ///
    /// Keyed by `PredId` (a `Copy` niche integer) instead of an owned predicate
    /// `String` — the surface is interned once in `predicates`, never re-cloned per
    /// bucket.  Fixed-seed (`FixedState`, off std SipHash); never an emission-order
    /// source.
    predicate_index: HashMap<PredId, Vec<usize>, FixedState>,
}

impl FactStore {
    /// A fresh, empty store.
    pub(crate) fn new() -> Self {
        Self {
            facts: Vec::new(),
            keys: HashTable::new(),
            predicates: PredInterner::new(),
            predicate_index: HashMap::default(),
        }
    }

    /// Insert `fact` if its key is new; return the newly-assigned insertion-order row
    /// index on success, `None` if a fact with the same key already exists.
    ///
    /// The returned index is the store-global dense slot of the new row (equal to
    /// `facts().len()` before the push), so callers keeping a parallel per-row column
    /// (a depth `Vec`, a delta [`DenseBitset`]) can address the new row directly.
    pub(crate) fn insert(&mut self, fact: Fact) -> Option<usize> {
        let hash = fact_parts_hash(&fact.subject, &fact.predicate, &fact.object);
        if self.keys.find(hash, |&i| self.facts[i] == fact).is_some() {
            return None;
        }
        let pid = self.predicates.intern(&fact.predicate);
        let idx = self.facts.len();
        self.facts.push(fact);
        self.predicate_index.entry(pid).or_default().push(idx);
        let facts = &self.facts;
        self.keys.insert_unique(hash, idx, |&i| {
            let fact = &facts[i];
            fact_parts_hash(&fact.subject, &fact.predicate, &fact.object)
        });
        Some(idx)
    }

    /// Whether a fact with this key exists.
    pub(crate) fn contains_key(&self, key: &FactKey) -> bool {
        self.row_index(key).is_some()
    }

    /// Resolve a retained native key to its insertion-order row index.
    pub(crate) fn row_index(&self, key: &FactKey) -> Option<usize> {
        self.row_index_parts(&key.0, &key.1, &key.2)
    }

    fn row_index_parts(
        &self,
        subject: &TermValue,
        predicate: &str,
        object: &TermValue,
    ) -> Option<usize> {
        self.keys
            .find(fact_parts_hash(subject, predicate, object), |&i| {
                let fact = &self.facts[i];
                fact.subject == *subject && fact.predicate == predicate && fact.object == *object
            })
            .copied()
    }

    /// The set of all fact keys (for fixpoint comparison — a cold, whole-model path).
    pub(crate) fn key_set(&self) -> FactKeySet {
        self.facts.iter().map(Fact::key).collect()
    }

    /// The number of stored rows (insertion-order slots `0..row_count`).
    pub(crate) fn row_count(&self) -> usize {
        self.facts.len()
    }

    /// The facts in insertion order.
    pub(crate) fn facts(&self) -> &[Fact] {
        &self.facts
    }

    /// Row indices (into [`facts`](Self::facts), insertion-ordered) of facts whose
    /// predicate surface (`predicate.as_str()`) equals `pred`; empty slice if none.
    pub(crate) fn facts_for_predicate(&self, pred: &str) -> &[usize] {
        self.predicates
            .lookup(pred)
            .and_then(|pid| self.predicate_index.get(&pid))
            .map_or(&[][..], Vec::as_slice)
    }
}

// ── Output row (the seam-contract provenance for one derived/asserted fact) ──────

/// A materialized quad with full content-addressed provenance.
///
/// `graph` is filled by the caller (per world).  `object` is a native [`TermValue`];
/// its N3 surface (`term_display`) is what the seam stamps, matching
/// `foundation.rs` and `py.rs`.
#[derive(Debug, Clone)]
pub(crate) struct DerivedRow {
    /// Present only for an actual context-owned modal application.
    pub(crate) cross_world: Option<crate::modal::native::NativeCrossWorldEvidence>,
    /// The world IRI (named-graph component).
    pub(crate) graph: String,
    /// The subject term.
    pub(crate) subject: TermValue,
    /// The predicate IRI string.
    pub(crate) predicate: String,
    /// The object term.
    pub(crate) object: TermValue,
    /// The firing rule IRI (`logic:assert` for EDB, else the rule's `#[name(...)]`).
    pub(crate) rule_iri: String,
    /// The reifier IRIs of the antecedent quads consumed by the firing.
    pub(crate) source_quad_ids: Vec<String>,
    /// The content-addressed derivation IRI.
    pub(crate) derivation_id: String,
    /// Height of the selected minimal proof tree (`0` for an asserted leaf).
    ///
    /// Record mode carries exactly this bounded annotation per fact; full trees are
    /// reconstructed only when an explanation query descends the selected premises.
    pub(crate) proof_height: ProofHeight,
    /// The matched positive body facts of the winning firing, in body order —
    /// the pre-reifier `(subject, predicate, object)` antecedents whose reifiers
    /// are exactly [`source_quad_ids`](Self::source_quad_ids).
    ///
    /// Carried so the [`crate::oracle::NativeForwardOracle`] seam can re-expose each
    /// antecedent as a decoded [`crate::oracle::TypedRow`] (the production
    /// provenance the reason/explain/materialize consumers require — they cannot
    /// invert a reifier hash back to its triple).  Empty for an echoed EDB row
    /// (an asserted fact has no antecedents) and on the facts-only (Skip) lane
    /// (which records no provenance at all).
    pub(crate) antecedents: Vec<Fact>,
}

/// Sort rows canonically by `(graph, subject, predicate, object)` N3 surfaces —
/// the same deterministic order the native paths and `foundation.rs` emit. Shared by
/// the well-founded and stable-model materializers.
///
/// Uses `sort_by_cached_key` so each row's string key is materialized once (O(n)
/// allocations) rather than on every comparison.
pub(crate) fn sort_rows(rows: &mut [DerivedRow]) {
    rows.sort_by_cached_key(|r| {
        (
            r.graph.clone(),
            term_display(&r.subject),
            r.predicate.clone(),
            term_display(&r.object),
        )
    });
}

/// The result of a reduct least-model computation: the final store plus the
/// first-wins provenance of every derived (non-EDB) fact.
#[derive(Debug, Clone)]
pub(crate) struct ReductResult {
    /// The least model of the reduct (EDB ∪ derived).
    pub(crate) store: FactStore,
    /// One row per DERIVED (non-EDB) fact, in first-derivation order, with the
    /// FIRST firing's provenance.  `graph` is left empty for the caller to fill.
    pub(crate) derivations: Vec<DerivedRow>,
}

// ── Join engine// ── Join engine (semi-naive, NAF against a SEPARATE reference store) ─────────────

/// A candidate solution: variable→native-term bindings plus the matched positive
/// body facts (their full [`Fact`]s, for provenance recovery).
#[derive(Clone)]
pub(crate) struct Solution {
    pub(crate) bindings: Vec<(String, TermValue)>,
    pub(crate) source_facts: Vec<Fact>,
}

impl Solution {
    pub(crate) fn get(&self, var_name: &str) -> Option<&TermValue> {
        self.bindings
            .iter()
            .find(|(k, _)| k == var_name)
            .map(|(_, v)| v)
    }
}

/// Borrow a grounded binding or literal; an IRI constant needs one owned wrapper.
fn ground<'a>(term: &'a EvalTerm, sol: &'a Solution) -> Option<Cow<'a, TermValue>> {
    match term {
        EvalTerm::ConstNamed(iri) => Some(Cow::Owned(TermValue::iri(iri))),
        EvalTerm::ConstLit(term) => Some(Cow::Borrowed(term)),
        EvalTerm::Var(name) => sol.get(name).map(Cow::Borrowed),
    }
}

/// Try to match `atom` against fact `f`, extending `base`; return the merged
/// solution or `None`.  A repeated variable must agree; a constant must equal the
/// native fact term exactly, including scoped blanks and recursive triple terms.
pub(crate) fn match_atom(atom: &EvalAtom, f: &Fact, base: &Solution) -> Option<Solution> {
    if atom.predicate != f.predicate {
        return None;
    }
    let mut new_bindings: Vec<(String, TermValue)> = Vec::new();
    for (pat, value) in [(&atom.subject, &f.subject), (&atom.object, &f.object)] {
        match pat {
            EvalTerm::ConstNamed(iri) => {
                if value.as_iri() != Some(iri.as_str()) {
                    return None;
                }
            }
            EvalTerm::ConstLit(expected) => {
                if expected != value {
                    return None;
                }
            }
            EvalTerm::Var(name) => {
                let existing = base
                    .get(name)
                    .or_else(|| new_bindings.iter().find(|(k, _)| k == name).map(|(_, v)| v));
                match existing {
                    Some(existing) => {
                        if existing != value {
                            return None;
                        }
                    }
                    None => new_bindings.push((name.clone(), value.clone())),
                }
            }
        }
    }
    let mut sol = base.clone();
    sol.bindings.extend(new_bindings);
    Some(sol)
}

/// Whether a negated atom is satisfied (blocks the rule) — i.e. its grounded form
/// is PRESENT in the `reference` store (the Gelfond-Lifschitz guess).
fn negated_atom_satisfied(atom: &EvalAtom, sol: &Solution, reference: &FactStore) -> bool {
    let s = ground(&atom.subject, sol);
    let o = ground(&atom.object, sol);
    match (s, o) {
        (Some(s), Some(o)) => reference.row_index_parts(&s, &atom.predicate, &o).is_some(),
        // A partially-bound negated atom never arises in the DL-safe gmeow fragment
        // (every negated var is bound by a positive body atom).  Treat unbound as
        // not-satisfied (the rule is not blocked); the corpus never hits this.
        _ => false,
    }
}

/// Whether every inequality guard holds (native-term inequality). An unbound guard
/// variable is a hard error.  Mirrors `foundation.rs::distinct_pairs_satisfied`.
pub(crate) fn distinct_pairs_satisfied(
    distinct_pairs: &[(String, String)],
    sol: &Solution,
) -> gmeow_errors::Result<bool> {
    for (a, b) in distinct_pairs {
        let va = sol.get(a).ok_or_else(|| {
            ir_err(format!(
                "Inequality guard variable {a:?} is unbound after body matching"
            ))
        })?;
        let vb = sol.get(b).ok_or_else(|| {
            ir_err(format!(
                "Inequality guard variable {b:?} is unbound after body matching"
            ))
        })?;
        if va == vb {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether a positive atom's binding to fact `f` is restricted to a delta scan.
///
/// Mirrors `foundation.rs::Scan` exactly.  In a delta-position scan we walk only the
/// rows in the predicate bucket whose key is in `delta`; in a full-store scan we walk
/// the whole bucket; in an old-only scan we walk only rows NOT in `delta`.  All three
/// modes walk the insertion-ordered predicate bucket so the matched subsequence (and
/// thus `source_facts` order) is identical to a full scan filtered post-hoc.
enum Scan {
    /// Bind `a_p` to facts whose key is **in** `delta` (the "new at p" position).
    Delta,
    /// Bind to **any** fact in the full store (no delta constraint).
    Full,
    /// Bind only to facts whose key is **not** in `delta` (the "old after p"
    /// positions, j > p, that keep each delta-touching solution produced once).
    OldOnly,
}

/// Extend each partial solution by matching `atom` against the store under `scan`.
///
/// `EvalAtom::predicate` is always a constant named relation in the GMEOW fragment,
/// so this always uses the predicate bucket — gated by delta membership for the
/// [`Scan::Delta`] / [`Scan::OldOnly`] positions.  Walks the bucket in insertion order
/// so the produced solutions (and their `source_facts`) match a full insertion-ordered
/// scan.  Mirrors `foundation.rs::extend_solutions`.
fn extend_solutions(
    atom: &EvalAtom,
    store: &FactStore,
    delta: &DenseBitset,
    scan: &Scan,
    solutions: &[Solution],
) -> Vec<Solution> {
    // The row index `i` is already in hand from the predicate bucket, so delta
    // membership is one dense `u64`-word test on the row's `RowId` — NO `Fact::key()`
    // rendering (2 allocs + 1 clone) and NO hashing per (solution × row).
    let keep = |i: usize| match scan {
        Scan::Delta => delta.contains(RowId::from_index(i)),
        Scan::Full => true,
        Scan::OldOnly => !delta.contains(RowId::from_index(i)),
    };
    let mut next: Vec<Solution> = Vec::new();
    let bucket = store.facts_for_predicate(atom.predicate.as_str());
    for sol in solutions {
        for &i in bucket {
            if !keep(i) {
                continue;
            }
            let f = &store.facts()[i];
            if let Some(mut merged) = match_atom(atom, f, sol) {
                merged.source_facts.push(f.clone());
                next.push(merged);
            }
        }
    }
    next
}

/// Join all body atoms against `store`, evaluating NAF against `reference`.
///
/// Uses true semi-naive delta×full position-decomposition (mirroring
/// `foundation.rs::join_body`): for each positive body atom position `p`, the union
/// over `p` of { a_p ∈ delta, a_{<p} ∈ full, a_{>p} ∈ store \ delta } produces every
/// delta-touching solution exactly once — at its first (lowest-index) delta position.
/// NAF literals are filtered after the positive join, evaluated against `reference`.
///
/// By-construction tiebreak is applied per-round in `least_model_of_reduct`
/// (cross-reference `foundation.rs::chase_world`).
fn join_body(
    rule: &EvalRule,
    store: &FactStore,
    reference: &FactStore,
    delta: &DenseBitset,
) -> Vec<Solution> {
    let positive: Vec<&EvalAtom> = rule.body.iter().filter(|a| !a.negated).collect();
    let negated: Vec<&EvalAtom> = rule.body.iter().filter(|a| a.negated).collect();

    let empty = Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    };

    let mut solutions: Vec<Solution> = if positive.is_empty() {
        // The empty conjunction is relational identity: one empty substitution. This
        // lets unconditional, NAF-only, and constraint-only rules fire once; duplicate
        // heads are suppressed by the store on the following round.
        vec![empty.clone()]
    } else {
        // True semi-naive: union over delta position p of
        //   { a_p ∈ delta, a_{<p} ∈ full, a_{>p} ∈ store \ delta }.
        let k = positive.len();
        let mut all: Vec<Solution> = Vec::new();
        for p in 0..k {
            let mut partial: Vec<Solution> = vec![empty.clone()];
            for (j, atom) in positive.iter().enumerate() {
                let scan = if j < p {
                    Scan::Full
                } else if j == p {
                    Scan::Delta
                } else {
                    Scan::OldOnly
                };
                partial = extend_solutions(atom, store, delta, &scan, &partial);
                if partial.is_empty() {
                    break;
                }
            }
            all.extend(partial);
        }
        all
    };

    if !negated.is_empty() {
        solutions.retain(|sol| {
            !negated
                .iter()
                .any(|neg| negated_atom_satisfied(neg, sol, reference))
        });
    }

    solutions
}

/// A candidate derivation within a single chase round for the reduct evaluator.
///
/// `sorted_sources` is a sorted copy of `sources` used ONLY for the deterministic
/// tiebreak comparison.  The emitted [`DerivedRow`] always uses body-order `sources`
/// for `source_quad_ids`; the sorted copy never appears in output.
///
/// Winner selection uses a **quality-ordered total-order** over same-head candidates:
/// `(proof_height, sum_src_depth, sorted_sources, rule_iri, sources)` — smaller wins.
/// This prefers the most-direct (shallowest) derivation, tiebreaks toward
/// asserted-rooted proofs (lower depth sum), uses lex-min sorted reifiers as a
/// content-addressed tiebreaker, uses `rule_iri` as a backstop (since rule IRIs vary
/// per rule, unlike `foundation.rs` where a single anonymous IRI is used), and finally
/// the body-order `sources` as the **total-order closer** so selection is independent
/// of candidate enumeration order (the columnar store enumerates rows in value order).
/// The per-derived-tuple provenance of a candidate: the rule that fired and the
/// premise (source) reifiers, plus the derivation-depth scalars used for winner
/// selection.  Recording this is the memory the closure-only lane must NOT pay, so it
/// is an `Option` on [`RuleRoundCandidate`]: the forward (Record) leg carries `Some`,
/// the backward (Skip) leg carries `None` — the absence is type-enforced, not a
/// sentinel-filled struct that "is never read."
#[derive(Clone)]
pub(crate) struct Provenance {
    /// Native modal support may reference admitted facts in other worlds.
    pub(crate) cross_world: Option<crate::modal::native::NativeCrossWorldEvidence>,
    /// Reifiers of matched positive body facts, in body (scan) order — goes into
    /// `DerivedRow.source_quad_ids`.
    pub(crate) sources: Vec<String>,
    /// Sorted copy of `sources`, used only for deterministic winner comparison.
    pub(crate) sorted_sources: Vec<String>,
    /// Content-addressed derivation IRI.
    pub(crate) deriv: String,
    /// The firing rule IRI (carried for comparison and output).
    pub(crate) rule_iri: String,
    /// Minimal proof height of this candidate firing.
    pub(crate) proof_height: ProofHeight,
    /// Sum of derivation depths across matched source facts.
    pub(crate) sum_src_depth: u64,
    /// The matched positive body facts, in body order — the same facts whose
    /// reifiers are [`sources`](Self::sources).  Carried into
    /// [`DerivedRow::antecedents`] so the oracle seam can re-expose the decoded
    /// antecedent triples (a reifier hash cannot be inverted).
    pub(crate) source_facts: Vec<Fact>,
}

#[derive(Clone)]
pub(crate) struct RuleRoundCandidate {
    pub(crate) head: Fact,
    // `key: FactKey` REMOVED — it was always identical to `head.key()`, computed once at
    // construction only to be re-derived by callers.  The per-round winner map now caches
    // each candidate's key alongside it (like a retained native model key), and the commit loop
    // reads that cached key, so the redundant per-candidate field is gone.
    /// The recorded provenance, or `None` on the facts-only (Skip) lane.
    pub(crate) prov: Option<Provenance>,
}

/// The quality-ordered **total-order** tiebreak key produced by
/// [`RuleRoundCandidate::tiebreak_key`]:
/// `(proof_height, sum_src_depth, sorted_sources, rule_iri, sources)`, smaller wins.
/// The trailing body-order `sources` is the total-order closer (see `tiebreak_key`).
type TiebreakKey<'a> = (
    ProofHeight,
    u64,
    &'a [String],
    &'a str,
    &'a [String],
    &'a str,
);

impl RuleRoundCandidate {
    /// Whether this recorded candidate is the semiring-selected winner over
    /// `current`, including the deterministic total-order tie-break after equal
    /// minimal heights.
    pub(crate) fn preferred_over(&self, current: &Self) -> gmeow_errors::Result<bool> {
        let (Some(candidate), Some(existing)) = (&self.prov, &current.prov) else {
            return Ok(false);
        };
        let selected =
            MinProofHeightSemiring.choose(candidate.proof_height, existing.proof_height)?;
        if candidate.proof_height != existing.proof_height {
            return Ok(selected == candidate.proof_height);
        }
        Ok(self.tiebreak_key() < current.tiebreak_key())
    }

    /// The quality-ordered **total-order** tiebreak key —
    /// `(proof_height, sum_src_depth, sorted_sources, rule_iri, sources)`, smaller wins.
    /// `None` (the facts-only lane, which never tiebreaks — it only `or_insert`s a
    /// first-seen winner) sorts below any `Some` and never participates in a compare.
    ///
    /// The final `sources` (body-order reifiers) component makes the key **total over
    /// observable provenance**: two same-head candidates whose earlier components tie
    /// (a symmetric body such as `co(?z) :- rel(?x,?z), rel(?y,?z)` yields the same
    /// `sorted_sources` from different body orders) still differ in body-order `sources`
    /// — which drives `source_quad_ids` and the minted derivation id. Comparing it makes
    /// winner selection independent of candidate *enumeration order*, so the columnar
    /// store may enumerate rows in value order (not insertion order) without perturbing
    /// which provenance wins. The final derivation identity additionally distinguishes
    /// modal applications with equal triple reifiers but different context-qualified
    /// occurrences or completed frontiers.
    pub(crate) fn tiebreak_key(&self) -> Option<TiebreakKey<'_>> {
        self.prov.as_ref().map(|p| {
            (
                p.proof_height,
                p.sum_src_depth,
                p.sorted_sources.as_slice(),
                p.rule_iri.as_str(),
                p.sources.as_slice(),
                p.deriv.as_str(),
            )
        })
    }
}

/// The least model of the Gelfond-Lifschitz reduct of `rules` w.r.t. `reference`,
/// seeded from `edb`.
///
/// The positive semi-naive join grows a fresh store seeded from `edb`; a negated
/// body atom blocks its rule iff its grounded form is PRESENT in `reference`.  The
/// returned [`ReductResult`] carries the final store AND the first-wins provenance
/// of every DERIVED (non-EDB) fact, selected by a quality-ordered total-order
/// tiebreak (mirroring `foundation.rs::chase_world`):
///
/// 1. **Minimal proof height** (`proof_height`) — prefer the candidate whose
///    `1 + max(source heights)` annotation is lowest.
/// 2. **Asserted-rooted preference** (`sum_src_depth`) — tiebreak on sum of source depths.
/// 3. **Lex-min sorted source reifiers** (`sorted_sources`) — content-addressed tiebreaker.
/// 4. **Rule IRI** (`rule_iri`) — total-order backstop (rule IRIs vary per rule here,
///    unlike the single anonymous IRI in `foundation.rs`).
/// 5. **Body-order sources** (`sources`) — total-order closer over observable
///    provenance: a symmetric body (`co(?z) :- rel(?x,?z), rel(?y,?z)`) yields equal
///    `sorted_sources` from different body orders, so the body-order reifiers (which
///    drive `source_quad_ids` and the derivation id) are the decisive final key.
///
/// The comparison is **independent of firing-enumeration order** by construction — the
/// key is a total order over every candidate that differs in any output byte, so the
/// columnar store may enumerate rows in value order without changing which winner is
/// selected.
///
/// # Errors
///
/// Returns `Err` for an unbound head variable, an unbound inequality guard, or a
/// provenance-recipe failure.
pub(crate) fn least_model_of_reduct(
    edb: &FactStore,
    rules: &[EvalRule],
    reference: &FactStore,
) -> gmeow_errors::Result<ReductResult> {
    admit_reduct_rules(rules)?;
    let mut store = FactStore::new();

    // Per-fact derivation-depth column, indexed by the store's insertion-order row:
    // `depth[i]` is the depth of `store`'s row `i`.  Depth 0 for every EDB (asserted)
    // fact; derived facts get depth = 1 + max(source depths) when committed at round
    // end.  It is pushed in lockstep with `store.insert`, so it never needs an owned-key
    // map — the row index the store returns is the column index.
    let mut depth: Vec<ProofHeight> = Vec::new();

    for f in edb.facts() {
        // `edb` is itself a `FactStore` (no duplicate keys), so every seed inserts into
        // the fresh `store`; guard on `Some` regardless so `depth` tracks `store`'s rows
        // exactly.
        if store.insert(f.clone()).is_some() {
            depth.push(ProofHeight::ASSERTED); // EDB facts have height 0
        }
    }

    // The EDB occupies rows `0..edb_row_count` (seeded first, in dense order); a later
    // winner is a genuine derivation iff its assigned row index is at or beyond this,
    // replacing the old `edb_keys` membership set.
    let edb_row_count = store.row_count();

    let mut derivations: Vec<DerivedRow> = Vec::new();

    // Seed delta with every EDB row so rules fire against the seed in round 1.  The store
    // holds exactly the EDB facts in dense order `0..edb_row_count`, so the whole set is
    // the low `edb_row_count` bits — one `all_set`, no per-key materialization.
    let mut delta = DenseBitset::all_set(store.row_count());
    loop {
        // Per-round canonical-winner map: a borrowed-key `HashTable<usize>` into a side
        // `Vec<(FactKey, RuleRoundCandidate)>` (uses a retained native key
        // probe), holding the candidate chosen by a quality-ordered total-order tiebreak
        // (see struct doc above).  This makes provenance selection independent of
        // firing-enumeration order.  The cached `FactKey` is reused at commit for the
        // sort, so no candidate's surface is re-rendered on a probe.
        let mut round_entries: Vec<(FactKey, RuleRoundCandidate)> = Vec::new();
        let mut round_index: HashTable<usize> = HashTable::new();

        for rule in rules {
            for sol in join_body(rule, &store, reference, &delta) {
                if !distinct_pairs_satisfied(&rule.distinct_pairs, &sol)? {
                    continue;
                }
                let head = ground_head(&rule.head, &sol)?;
                let key = head.key();
                if store.contains_key(&key) {
                    continue; // a prior round already derived it; earlier round wins
                }

                // Provenance: reifiers of matched POSITIVE body facts in body order.
                let mut sources: Vec<String> = Vec::with_capacity(sol.source_facts.len());
                let mut max_sd = ProofHeight::ASSERTED;
                let mut sum_sd: u64 = 0;
                for sf in &sol.source_facts {
                    sources.push(sf.reifier()?);
                    let source_key = sf.key();
                    let row = store.row_index(&source_key).ok_or_else(|| {
                        ir_err(format!(
                            "provenance source {source_key:?} is absent from the reduct fact store"
                        ))
                    })?;
                    drop(source_key);
                    let d = depth.get(row).copied().ok_or_else(|| {
                        ir_err(format!(
                            "provenance source row {row} has no proof-height annotation"
                        ))
                    })?;
                    max_sd = max_sd.max(d);
                    sum_sd = sum_sd.saturating_add(u64::from(d.get()));
                }
                let proof_height = MinProofHeightSemiring.derive([max_sd])?;
                let src_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                let deriv = mint_derivation_id(&rule.rule_iri, &src_refs);
                let mut sorted_sources = sources.clone();
                sorted_sources.sort();

                // Quality-ordered total-order tiebreak:
                //   (proof_height, sum_src_depth, sorted_sources, rule_iri) — smaller wins.
                // Level 1: fewest derivation steps (most direct).
                // Level 2: asserted-rooted preference (lower depth sum).
                // Level 3: lex-min sorted reifiers (content-addressed tiebreaker).
                // Level 4: rule_iri — total-order backstop (IRIs vary per rule).
                let candidate = RuleRoundCandidate {
                    head,
                    prov: Some(Provenance {
                        cross_world: None,
                        sources,
                        sorted_sources,
                        deriv,
                        rule_iri: rule.rule_iri.clone(),
                        proof_height,
                        sum_src_depth: sum_sd,
                        source_facts: sol.source_facts.clone(),
                    }),
                };
                let hash = fact_key_hash(&key);
                match round_index.find(hash, |&i| round_entries[i].0 == key) {
                    Some(&i) => {
                        if candidate.preferred_over(&round_entries[i].1)? {
                            round_entries[i].1 = candidate;
                        }
                    }
                    None => {
                        let idx = round_entries.len();
                        round_entries.push((key, candidate));
                        let entries = &round_entries;
                        round_index.insert_unique(hash, idx, |&i| fact_key_hash(&entries[i].0));
                    }
                }
            }
        }

        if round_entries.is_empty() {
            break; // fixpoint
        }

        // Commit all winners from this round in native FactKey order, not raw
        // table order, so store/index insertion order is deterministic.
        let round_len = round_entries.len();
        let mut winners: Vec<(FactKey, RuleRoundCandidate)> = round_entries;
        winners.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut new_delta = DenseBitset::with_capacity(store.row_count() + round_len);
        for (_key, winner) in winners {
            let RuleRoundCandidate { head, prov } = winner;
            // The reduct evaluator always records provenance; a `None` here is an engine
            // bug (a candidate committed without its derivation), not a data condition.
            let prov = prov.expect("least_model_of_reduct always records provenance");
            let winner_depth = prov.proof_height;
            // A winner is always a genuinely-new fact (heads already present are skipped
            // above via `store.contains_key`), so the insert returns `Some(idx)`; that
            // index drives the lockstep depth push AND the EDB-vs-derived membership test.
            let idx = store
                .insert(head.clone())
                .expect("a round winner is a genuinely-new fact (head not already present)");
            assert_eq!(
                idx,
                depth.len(),
                "depth/store index desync: `depth` and the `FactStore` rows must stay in \
                 lockstep (each committed row pushes exactly one depth slot)"
            );
            depth.push(winner_depth);
            new_delta.set(RowId::from_index(idx));

            // Record provenance only for genuinely-derived facts (a rule whose head
            // re-states an EDB fact — row index below the EDB range — is not a
            // derivation row).
            if idx >= edb_row_count {
                derivations.push(DerivedRow {
                    cross_world: None,
                    graph: String::new(),
                    subject: head.subject,
                    predicate: head.predicate,
                    object: head.object,
                    rule_iri: prov.rule_iri,
                    source_quad_ids: prov.sources, // body-order, NEVER sorted copy
                    derivation_id: prov.deriv,
                    proof_height: prov.proof_height,
                    antecedents: prov.source_facts,
                });
            }
        }

        delta = new_delta;
    }

    Ok(ReductResult { store, derivations })
}

/// Ground a rule head into an RDF-shaped [`Fact`], failing hard on an unbound head variable
/// or a literal first argument.
pub(crate) fn ground_head(head: &EvalAtom, sol: &Solution) -> gmeow_errors::Result<Fact> {
    let fact = ground_relational_head(head, sol)?;
    // Forward materialization emits RDF-shaped rows, whose subject must be an IRI/blank
    // node. Facts-only backward query evaluation uses `ground_relational_head` directly
    // because its binary IDB tuples are positional relations, not asserted RDF triples.
    if fact.subject.is_literal() {
        return Err(ir_err(
            "rule_ir: head subject grounded to a literal (no-optionality)".to_owned(),
        ));
    }
    Ok(fact)
}

/// Ground a rule head into an internal binary-relation tuple.
///
/// Unlike [`ground_head`], this admits a literal in the first position. The facts-only
/// backward engine projects such tuples straight to query bindings and never emits them as
/// RDF or mints triple provenance for them.
pub(crate) fn ground_relational_head(
    head: &EvalAtom,
    sol: &Solution,
) -> gmeow_errors::Result<Fact> {
    let subject = ground_term_to_value(&head.subject, sol, "head subject")?;
    let object = ground_term_to_value(&head.object, sol, "head object")?;
    Ok(Fact {
        subject,
        predicate: head.predicate.clone(),
        object,
    })
}

/// Ground an [`EvalTerm`] into a concrete native [`TermValue`].
pub(crate) fn ground_term_to_value(
    term: &EvalTerm,
    sol: &Solution,
    slot: &str,
) -> gmeow_errors::Result<TermValue> {
    match term {
        EvalTerm::ConstNamed(iri) => Ok(TermValue::iri(iri.clone())),
        EvalTerm::ConstLit(t) => Ok(t.clone()),
        EvalTerm::Var(name) => {
            let value = sol.get(name).ok_or_else(|| {
                ir_err(format!(
                    "{slot} variable {name:?} unbound after body matching"
                ))
            })?;
            Ok(value.clone())
        }
    }
}

// ── Asserted-EDB echo (mirror of foundation.rs chase_world's assert block) ───────

/// Produce the asserted-EDB rows for one world.
///
/// Each row: `rule_iri = logic:assert`, `source_quad_ids = [self_reifier]`,
/// `derivation_id = mint_derivation_id(logic:assert, &[self_reifier])`.  The object
/// surface is the term's N3 form, matching `py.rs` and `foundation.rs`.
pub(crate) fn echo_asserted(world: &str, edb: &[Fact]) -> gmeow_errors::Result<Vec<DerivedRow>> {
    let mut out: Vec<DerivedRow> = Vec::with_capacity(edb.len());
    for f in edb {
        let reifier = f.reifier()?;
        let deriv = mint_derivation_id(ASSERT_RULE_IRI, &[reifier.as_str()]);
        out.push(DerivedRow {
            cross_world: None,
            graph: world.to_owned(),
            subject: f.subject.clone(),
            predicate: f.predicate.clone(),
            object: f.object.clone(),
            rule_iri: ASSERT_RULE_IRI.to_owned(),
            source_quad_ids: vec![reifier],
            derivation_id: deriv,
            proof_height: ProofHeight::ASSERTED,
            // An asserted EDB fact has no antecedents (it is echoed, not derived).
            antecedents: Vec::new(),
        });
    }
    Ok(out)
}

// ── EDB extraction from a WorldStore ─────────────────────────────────────────────

/// Collect the EDB facts of one world from a [`crate::store::WorldStore`],
/// sorted by key (deterministic seed order, mirroring `foundation.rs`).
///
/// # Errors
///
/// Returns `Err` if an admitted store row has a non-IRI predicate.
pub(crate) fn world_edb_facts(
    store: &crate::store::WorldStore,
    world: &str,
) -> gmeow_errors::Result<Vec<Fact>> {
    let raw = store.quads_for_pattern_in_world(world, None, None, None);
    let mut facts: Vec<Fact> = Vec::with_capacity(raw.len());
    for r in raw {
        let predicate =
            r.p.as_iri()
                .ok_or_else(|| ir_err(format!("world EDB predicate is not an IRI: {:?}", r.p)))?
                .to_owned();
        facts.push(Fact {
            subject: r.s,
            predicate,
            object: r.o,
        });
    }
    facts.sort_by_key(Fact::key);
    Ok(facts)
}

/// Admit the operators represented by the ordinary Gelfond-Lifschitz reduct.
/// Check before iterating worlds, so an empty source cannot hide unsupported IR.
pub(crate) fn admit_reduct_rules(rules: &[EvalRule]) -> gmeow_errors::Result<()> {
    if rules.iter().any(|rule| !rule.numeric.is_empty()) {
        return Err(ir_err("finite RDF numeric relations require the certified native producer plan; reduct evaluation cannot erase their instructions".to_owned()));
    }
    if rules.iter().any(|rule| rule.reduction.is_some()) {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
            detail: "Gelfond-Lifschitz reduct requires aggregate-aware completion".into(),
        }));
    }
    Ok(())
}

#[path = "rule_ir.tests.rs"]
#[cfg(test)]
mod tests;
