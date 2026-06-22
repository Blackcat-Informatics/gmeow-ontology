// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared evaluable rule IR + Gelfond-Lifschitz reduct engine (issue #651).
//!
//! This module is the native (non-Nemo) substrate for the two *non-stratifiable*
//! semantics the foundation chase cannot express: the well-founded model
//! (alternating fixpoint, [`crate::wellfounded`]) and the stable-model / answer-set
//! semantics ([`crate::stablemodel`]).  Nemo rejects non-stratifiable programs up
//! front, so those two evaluators hand-roll the fixpoint here — but they reuse
//! Nemo's **parser** to lower `.rls` text into the [`EvalRule`] IR, exactly as
//! [`crate::certify`] does, so the predicate / variable surface is byte-identical
//! to the engine.
//!
//! # Why oxigraph terms, not strings
//!
//! Unlike [`crate::foundation`] (whose facts are all-IRI and stored as bare
//! strings), the IR here works over oxigraph [`Term`] / [`NamedNode`] so literal
//! object constants and the golden-pinned provenance recipe
//! ([`crate::provenance::mint_reifier`]) are handled for free.  The dedup key is
//! the `(subject.to_string(), predicate.as_str(), object.to_string())` triple of
//! N3 surfaces, mirroring `foundation.rs`'s first-wins `fact_index`.
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
//! # Phase-A dead code
//!
//! This is Phase A of #651: the evaluators and their unit tests are landed, but the
//! `py.rs` materialize routing that consumes [`parse_eval_rules`] and reads the
//! [`DerivedRow`] provenance fields is Phase B.  Until that lands, a few
//! constructors / fields are exercised only by tests, so this module allows
//! `dead_code` crate-internally rather than scattering per-item attributes that
//! would have to be unwound next phase.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use oxigraph::model::{NamedNode, Term};

use crate::provenance::{mint_derivation_id, mint_reifier, ASSERT_RULE_IRI, LOGIC_NAMESPACE};

// ── Evaluable term / atom / rule ────────────────────────────────────────────────

/// A head/body term: a `?var` reference, a constant IRI, or a constant literal.
///
/// Subject and predicate are never literals (an `.rls` predicate is always an IRI
/// and a subject is an IRI/blank in the gmeow fragment); only an *object* may be a
/// [`ConstLit`](EvalTerm::ConstLit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EvalTerm {
    /// A variable, e.g. `?X` (the string includes the leading `?`, matching Nemo's
    /// `Display` for a universal variable).
    Var(String),
    /// A constant IRI.
    ConstNamed(NamedNode),
    /// A constant literal (object position only).
    ConstLit(Term),
}

/// A single arity-3-derived atom, with the world slot dropped (subject, object).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvalAtom {
    /// The subject term (slot 0).
    pub(crate) subject: EvalTerm,
    /// The predicate IRI (constant in the gmeow fragment).
    pub(crate) predicate: NamedNode,
    /// The object term (slot 1).
    pub(crate) object: EvalTerm,
    /// `true` iff this is a negation-as-failure body literal.
    pub(crate) negated: bool,
}

/// A lowered rule: one head atom, an ordered body (positive atoms then negated
/// atoms), the firing rule IRI (from `#[name("...")]`), and inequality guards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvalRule {
    /// The single head atom.
    pub(crate) head: EvalAtom,
    /// The body atoms, positive first then negated.
    pub(crate) body: Vec<EvalAtom>,
    /// The firing rule IRI (the `#[name(...)]` value, or a synthesized anonymous IRI).
    pub(crate) rule_iri: String,
    /// Inequality guards `(?A, ?B)`.  The WFS/stable corpus has none, so this is
    /// empty for every corpus case (see the NOTE in [`parse_eval_rules`]).
    pub(crate) distinct_pairs: Vec<(String, String)>,
}

// ── Ground fact + store (oxigraph-term based, insertion-ordered, first-wins) ─────

/// A fully-ground fact `(subject, predicate, object)` over oxigraph terms.
#[derive(Debug, Clone)]
pub(crate) struct Fact {
    /// The subject term (an IRI/blank node in practice).
    pub(crate) subject: Term,
    /// The predicate IRI.
    pub(crate) predicate: NamedNode,
    /// The object term (IRI or literal).
    pub(crate) object: Term,
}

/// The dedup key of a fact: the N3 surfaces of `(subject, predicate, object)`.
type FactKey = (String, String, String);

impl Fact {
    /// The dedup / membership key `(s.to_string(), p.as_str(), o.to_string())`.
    pub(crate) fn key(&self) -> FactKey {
        (
            self.subject.to_string(),
            self.predicate.as_str().to_owned(),
            self.object.to_string(),
        )
    }

    /// The reifier IRI for this fact, via the golden-pinned recipe.
    pub(crate) fn reifier(&self) -> Result<String, String> {
        mint_reifier(&self.subject, &self.predicate, &self.object)
    }
}

/// Insertion-ordered fact store with O(1) dedup — mirrors `foundation.rs::FactStore`.
#[derive(Debug, Clone, Default)]
pub(crate) struct FactStore {
    facts: Vec<Fact>,
    keys: HashSet<FactKey>,
    /// Predicate surface (`predicate.as_str()`, the same component `Fact::key`
    /// uses) → row indices into `facts`, in insertion order.  Maintained in
    /// lockstep with `facts` so each bucket's order equals insertion order; this
    /// lets the join scan only the rows for a constant-predicate atom while
    /// returning exactly the subsequence (same relative order) a full scan would.
    predicate_index: HashMap<String, Vec<usize>>,
}

impl FactStore {
    /// A fresh, empty store.
    pub(crate) fn new() -> Self {
        Self {
            facts: Vec::new(),
            keys: HashSet::new(),
            predicate_index: HashMap::new(),
        }
    }

    /// Insert `fact` if its key is new; return `true` if it was inserted.
    pub(crate) fn insert(&mut self, fact: Fact) -> bool {
        let key = fact.key();
        if self.keys.contains(&key) {
            return false;
        }
        self.keys.insert(key);
        let predicate = fact.predicate.as_str().to_owned();
        self.facts.push(fact);
        // Push the new row index in lockstep with `facts`, preserving insertion
        // order within the predicate bucket (only on a successful insert).
        self.predicate_index
            .entry(predicate)
            .or_default()
            .push(self.facts.len() - 1);
        true
    }

    /// Whether a fact with this key exists.
    pub(crate) fn contains_key(&self, key: &FactKey) -> bool {
        self.keys.contains(key)
    }

    /// The set of all fact keys (for fixpoint comparison).
    pub(crate) fn key_set(&self) -> HashSet<FactKey> {
        self.keys.clone()
    }

    /// The facts in insertion order.
    pub(crate) fn facts(&self) -> &[Fact] {
        &self.facts
    }

    /// Row indices (into [`facts`](Self::facts), insertion-ordered) of facts whose
    /// predicate surface (`predicate.as_str()`) equals `pred`; empty slice if none.
    pub(crate) fn facts_for_predicate(&self, pred: &str) -> &[usize] {
        self.predicate_index
            .get(pred)
            .map_or(&[][..], Vec::as_slice)
    }
}

// ── Output row (the seam-contract provenance for one derived/asserted fact) ──────

/// A materialized quad with full content-addressed provenance.
///
/// `graph` is filled by the caller (per world).  `object` is an oxigraph [`Term`];
/// its N3 surface (`Term::to_string()`) is what the seam stamps, matching
/// `foundation.rs` and `py.rs`.
#[derive(Debug, Clone)]
pub(crate) struct DerivedRow {
    /// The world IRI (named-graph component).
    pub(crate) graph: String,
    /// The subject term.
    pub(crate) subject: Term,
    /// The predicate IRI.
    pub(crate) predicate: NamedNode,
    /// The object term.
    pub(crate) object: Term,
    /// The firing rule IRI (`logic:assert` for EDB, else the rule's `#[name(...)]`).
    pub(crate) rule_iri: String,
    /// The reifier IRIs of the antecedent quads consumed by the firing.
    pub(crate) source_quad_ids: Vec<String>,
    /// The content-addressed derivation IRI.
    pub(crate) derivation_id: String,
}

/// Sort rows canonically by `(graph, subject, predicate, object)` N3 surfaces —
/// the same deterministic order the Nemo path and `foundation.rs` emit. Shared by
/// the well-founded and stable-model materializers.
///
/// Uses `sort_by_cached_key` so each row's string key is materialized once (O(n)
/// allocations) rather than on every comparison.
pub(crate) fn sort_rows(rows: &mut [DerivedRow]) {
    rows.sort_by_cached_key(|r| {
        (
            r.graph.clone(),
            r.subject.to_string(),
            r.predicate.as_str().to_owned(),
            r.object.to_string(),
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

// ── Parsing `.rls` into the IR (Nemo parser reuse, mirroring certify.rs) ─────────

/// Lower a Nemo [`Term`](nemo::rule_model::components::term::Term) into an
/// [`EvalTerm`].
///
/// A variable renders as `?Name`; a ground IRI as `<iri>`; a literal as its N3.
/// The `is_subject` flag enforces no-optionality: a literal in subject/predicate
/// position is a hard error (the gmeow fragment never emits one).
fn lower_nemo_term(
    term: &nemo::rule_model::components::term::Term,
    slot: &str,
) -> Result<EvalTerm, String> {
    if term.is_variable() {
        return Ok(EvalTerm::Var(term.to_string()));
    }
    let rendered = term.to_string();
    if let Some(iri) = rendered.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        let nn = NamedNode::new(iri).map_err(|e| format!("invalid {slot} IRI {iri:?}: {e}"))?;
        return Ok(EvalTerm::ConstNamed(nn));
    }
    // A literal (or any non-IRI ground term).  Only an object may be a literal.
    if slot != "object" {
        return Err(format!(
            "rule_ir: non-IRI constant {rendered:?} in {slot} position — \
             only an object may be a literal (no-optionality)"
        ));
    }
    // Parse the literal's N3 surface into an oxigraph term via the same N-Triples
    // grammar the rest of the stack uses.  Wrap into a one-triple document so
    // oxigraph's reader yields the object term.
    let lit = parse_n3_object_literal(&rendered)?;
    Ok(EvalTerm::ConstLit(lit))
}

/// Parse a literal object's Nemo N3 surface (`"lex"`, `"lex"@lang`,
/// `"lex"^^<dt>`) into an oxigraph [`Term`].
fn parse_n3_object_literal(n3: &str) -> Result<Term, String> {
    use oxigraph::io::RdfFormat;
    use oxigraph::model::{NamedNode, Quad};
    let doc = format!("<urn:s> <urn:p> {n3} .\n");
    let store =
        oxigraph::store::Store::new().map_err(|e| format!("rule_ir: in-memory store: {e}"))?;
    store
        .load_from_reader(RdfFormat::NTriples, doc.as_bytes())
        .map_err(|e| format!("rule_ir: cannot parse literal object {n3:?}: {e}"))?;
    let want_p = NamedNode::new("urn:p").expect("constant IRI");
    let quads: Vec<Quad> = store
        .quads_for_pattern(None, Some(want_p.as_ref()), None, None)
        .filter_map(Result::ok)
        .collect();
    let q = quads
        .into_iter()
        .next()
        .ok_or_else(|| format!("rule_ir: literal object {n3:?} produced no triple"))?;
    Ok(q.object)
}

/// Lower a Nemo atom into an [`EvalAtom`], dropping the arity-3 world slot.
///
/// `terms()[0]` = subject, `terms()[1]` = object; `terms()[2]` (world) is ignored,
/// exactly like `certify.rs::logical_terms`.
fn lower_nemo_atom(
    atom: &nemo::rule_model::components::atom::Atom,
    negated: bool,
) -> Result<EvalAtom, String> {
    let predicate_iri = atom.predicate().to_string();
    let predicate = NamedNode::new(&predicate_iri)
        .map_err(|e| format!("invalid predicate IRI {predicate_iri:?}: {e}"))?;
    let mut it = atom.terms();
    let subj = it
        .next()
        .ok_or("rule_ir: atom has no subject term (arity < 1)")?;
    let obj = it
        .next()
        .ok_or("rule_ir: atom has no object term (arity < 2)")?;
    let subject = lower_nemo_term(subj, "subject")?;
    let object = lower_nemo_term(obj, "object")?;
    Ok(EvalAtom {
        subject,
        predicate,
        object,
        negated,
    })
}

/// Parse `.rls` text into the evaluable IR via Nemo's parser.
///
/// Reuses [`crate::nemo_engine::NemoParsedRules::parse_unvalidated`] (the same
/// translation-only path `certify.rs` uses), so the predicate / variable surface
/// is identical to the engine.  The body is `body_positive()` atoms (negated
/// `false`) followed by `body_negative()` atoms (negated `true`).  The rule IRI is
/// the `#[name(...)]` value, or a synthesized `{LOGIC_NAMESPACE}rule/anonymous`.
///
/// # Errors
///
/// Returns the Nemo parse-error string, or a lowering error (e.g. a literal in a
/// subject slot).
pub(crate) fn parse_eval_rules(rules: &str) -> Result<Vec<EvalRule>, String> {
    use crate::nemo_engine::NemoParsedRules;
    use nemo::rule_model::programs::ProgramRead;

    let program = NemoParsedRules::parse_unvalidated(rules)?.into_program();

    let mut out: Vec<EvalRule> = Vec::new();
    for rule in program.rules() {
        let head_atom = rule
            .head()
            .first()
            .ok_or("rule_ir: rule has no head atom")?;
        let head = lower_nemo_atom(head_atom, false)?;

        let mut body: Vec<EvalAtom> = Vec::new();
        for atom in rule.body_positive() {
            body.push(lower_nemo_atom(atom, false)?);
        }
        for atom in rule.body_negative() {
            body.push(lower_nemo_atom(atom, true)?);
        }

        let rule_iri = rule
            .name()
            .unwrap_or_else(|| format!("{LOGIC_NAMESPACE}rule/anonymous"));

        // NOTE: the WFS / stable-model conformance corpus carries NO body
        // inequality guards, so `distinct_pairs` is always empty here.  Extracting
        // Nemo's `Operation`-encoded inequalities is deferred until a corpus case
        // needs it; an empty vec is correct for every case Phase A targets.
        out.push(EvalRule {
            head,
            body,
            rule_iri,
            distinct_pairs: Vec::new(),
        });
    }
    Ok(out)
}

// ── Join engine (semi-naive, NAF against a SEPARATE reference store) ─────────────

/// A candidate solution: variable→N3-surface bindings plus the matched positive
/// body facts (their full [`Fact`]s, for provenance recovery).
#[derive(Clone)]
struct Solution {
    bindings: Vec<(String, String)>,
    source_facts: Vec<Fact>,
}

impl Solution {
    fn get(&self, var_name: &str) -> Option<&str> {
        self.bindings
            .iter()
            .find(|(k, _)| k == var_name)
            .map(|(_, v)| v.as_str())
    }
}

/// The N3 surface of an [`EvalTerm`] under bindings, or `None` if an unbound var.
fn ground(term: &EvalTerm, sol: &Solution) -> Option<String> {
    match term {
        EvalTerm::ConstNamed(nn) => Some(format!("<{}>", nn.as_str())),
        EvalTerm::ConstLit(t) => Some(t.to_string()),
        EvalTerm::Var(name) => sol.get(name).map(str::to_owned),
    }
}

/// The N3 surface a term pattern must equal against a fact term, for a constant.
fn const_surface(term: &EvalTerm) -> Option<String> {
    match term {
        EvalTerm::ConstNamed(nn) => Some(format!("<{}>", nn.as_str())),
        EvalTerm::ConstLit(t) => Some(t.to_string()),
        EvalTerm::Var(_) => None,
    }
}

/// Try to match `atom` against fact `f`, extending `base`; return the merged
/// solution or `None`.  A repeated variable must agree; a constant must equal the
/// fact term's N3 surface exactly.  Mirrors `foundation.rs::match_atom`.
fn match_atom(atom: &EvalAtom, f: &Fact, base: &Solution) -> Option<Solution> {
    let fact_surfaces = [
        f.subject.to_string(),
        format!("<{}>", f.predicate.as_str()),
        f.object.to_string(),
    ];
    let pats = [
        &atom.subject,
        &EvalTerm::ConstNamed(atom.predicate.clone()),
        &atom.object,
    ];

    let mut new_bindings: Vec<(String, String)> = Vec::new();
    for (pat, fact_surface) in pats.into_iter().zip(fact_surfaces.iter()) {
        match pat {
            EvalTerm::ConstNamed(_) | EvalTerm::ConstLit(_) => {
                let want = const_surface(pat).expect("constant has a surface");
                if &want != fact_surface {
                    return None;
                }
            }
            EvalTerm::Var(name) => {
                let existing = base.get(name).or_else(|| {
                    new_bindings
                        .iter()
                        .find(|(k, _)| k == name)
                        .map(|(_, v)| v.as_str())
                });
                match existing {
                    Some(existing) => {
                        if existing != fact_surface {
                            return None;
                        }
                    }
                    None => new_bindings.push((name.clone(), fact_surface.clone())),
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
    // The predicate component of a `Fact::key` is the BARE IRI (no angle brackets);
    // build the lookup key to match it exactly.
    let p = atom.predicate.as_str().to_owned();
    let o = ground(&atom.object, sol);
    match (s, o) {
        (Some(s), Some(o)) => reference.contains_key(&(s, p, o)),
        // A partially-bound negated atom never arises in the DL-safe gmeow fragment
        // (every negated var is bound by a positive body atom).  Treat unbound as
        // not-satisfied (the rule is not blocked); the corpus never hits this.
        _ => false,
    }
}

/// Whether every inequality guard holds (N3-surface inequality).  An unbound guard
/// variable is a hard error.  Mirrors `foundation.rs::distinct_pairs_satisfied`.
fn distinct_pairs_satisfied(
    distinct_pairs: &[(String, String)],
    sol: &Solution,
) -> Result<bool, String> {
    for (a, b) in distinct_pairs {
        let va = sol.get(a).ok_or_else(|| {
            format!("Inequality guard variable {a:?} is unbound after body matching")
        })?;
        let vb = sol.get(b).ok_or_else(|| {
            format!("Inequality guard variable {b:?} is unbound after body matching")
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
/// `EvalAtom::predicate` is always a constant `NamedNode` in the gmeow `.rls` fragment,
/// so this always uses the predicate bucket — gated by delta membership for the
/// [`Scan::Delta`] / [`Scan::OldOnly`] positions.  Walks the bucket in insertion order
/// so the produced solutions (and their `source_facts`) match a full insertion-ordered
/// scan.  Mirrors `foundation.rs::extend_solutions`.
fn extend_solutions(
    atom: &EvalAtom,
    store: &FactStore,
    delta: &HashSet<FactKey>,
    scan: &Scan,
    solutions: &[Solution],
) -> Vec<Solution> {
    let keep = |f: &Fact| match scan {
        Scan::Delta => delta.contains(&f.key()),
        Scan::Full => true,
        Scan::OldOnly => !delta.contains(&f.key()),
    };
    let mut next: Vec<Solution> = Vec::new();
    let bucket = store.facts_for_predicate(atom.predicate.as_str());
    for sol in solutions {
        for &i in bucket {
            let f = &store.facts()[i];
            if !keep(f) {
                continue;
            }
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
    delta: &HashSet<FactKey>,
) -> Vec<Solution> {
    let positive: Vec<&EvalAtom> = rule.body.iter().filter(|a| !a.negated).collect();
    let negated: Vec<&EvalAtom> = rule.body.iter().filter(|a| a.negated).collect();

    let empty = Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    };

    let mut solutions: Vec<Solution> = if positive.is_empty() {
        // Zero positive atoms: the empty solution never touches delta, so it never
        // fires in a semi-naive round.  Emit nothing (matches the prior end-filter
        // behaviour where an empty source_facts list never passed the delta check).
        Vec::new()
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
/// `(max_src_depth, sum_src_depth, sorted_sources, rule_iri)` — smaller wins.  This
/// prefers the most-direct (shallowest) derivation, tiebreaks toward asserted-rooted
/// proofs (lower depth sum), uses lex-min sorted reifiers as a content-addressed
/// tiebreaker, and finally uses `rule_iri` as a total-order backstop (since rule IRIs
/// vary per rule, unlike `foundation.rs` where a single anonymous IRI is used).
#[derive(Clone)]
struct RuleRoundCandidate {
    head: Fact,
    key: FactKey,
    /// Reifiers of matched positive body facts, in body (scan) order — goes into
    /// `DerivedRow.source_quad_ids`.
    sources: Vec<String>,
    /// Sorted copy of `sources`, used only for deterministic winner comparison.
    sorted_sources: Vec<String>,
    /// Content-addressed derivation IRI.
    deriv: String,
    /// The firing rule IRI (carried for comparison and output).
    rule_iri: String,
    /// Maximum derivation depth across matched source facts (depth 0 = asserted).
    max_src_depth: u32,
    /// Sum of derivation depths across matched source facts.
    sum_src_depth: u64,
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
/// 1. **Fewest derivation steps** (`max_src_depth`) — prefer the candidate whose
///    deepest source has the lowest depth.
/// 2. **Asserted-rooted preference** (`sum_src_depth`) — tiebreak on sum of source depths.
/// 3. **Lex-min sorted source reifiers** (`sorted_sources`) — content-addressed tiebreaker.
/// 4. **Rule IRI** (`rule_iri`) — total-order backstop (rule IRIs vary per rule here,
///    unlike the single anonymous IRI in `foundation.rs`).
///
/// The comparison is **independent of firing-enumeration order** by construction.
///
/// # Errors
///
/// Returns `Err` for an unbound head variable, an unbound inequality guard, or a
/// provenance-recipe failure.
pub(crate) fn least_model_of_reduct(
    edb: &FactStore,
    rules: &[EvalRule],
    reference: &FactStore,
) -> Result<ReductResult, String> {
    let mut store = FactStore::new();
    let edb_keys: HashSet<FactKey> = edb.key_set();

    // Per-fact derivation-depth map: depth 0 for every EDB (asserted) fact;
    // derived facts get depth = 1 + max(source depths) when committed at round end.
    let mut depth: HashMap<FactKey, u32> = HashMap::new();

    for f in edb.facts() {
        let key = f.key();
        depth.insert(key, 0); // EDB facts have depth 0
        store.insert(f.clone());
    }

    let mut derivations: Vec<DerivedRow> = Vec::new();

    // Seed delta with all EDB keys so rules fire against the seed in round 1.
    let mut delta: HashSet<FactKey> = store.key_set();
    loop {
        // Per-round canonical-winner map: keyed by head key, holds the candidate
        // chosen by a quality-ordered total-order tiebreak (see struct doc above).
        // This makes provenance selection independent of firing-enumeration order.
        let mut round: HashMap<FactKey, RuleRoundCandidate> = HashMap::new();

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
                let mut max_sd: u32 = 0;
                let mut sum_sd: u64 = 0;
                for sf in &sol.source_facts {
                    sources.push(sf.reifier()?);
                    let d = *depth.get(&sf.key()).unwrap_or(&0);
                    max_sd = max_sd.max(d);
                    sum_sd = sum_sd.saturating_add(u64::from(d));
                }
                let src_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                let deriv = mint_derivation_id(&rule.rule_iri, &src_refs);
                let mut sorted_sources = sources.clone();
                sorted_sources.sort();

                // Quality-ordered total-order tiebreak:
                //   (max_src_depth, sum_src_depth, sorted_sources, rule_iri) — smaller wins.
                // Level 1: fewest derivation steps (most direct).
                // Level 2: asserted-rooted preference (lower depth sum).
                // Level 3: lex-min sorted reifiers (content-addressed tiebreaker).
                // Level 4: rule_iri — total-order backstop (IRIs vary per rule).
                let candidate = RuleRoundCandidate {
                    head,
                    key: key.clone(),
                    sources,
                    sorted_sources,
                    deriv,
                    rule_iri: rule.rule_iri.clone(),
                    max_src_depth: max_sd,
                    sum_src_depth: sum_sd,
                };
                round
                    .entry(key)
                    .and_modify(|existing| {
                        let cand_key = (
                            candidate.max_src_depth,
                            candidate.sum_src_depth,
                            &candidate.sorted_sources,
                            &candidate.rule_iri,
                        );
                        let exist_key = (
                            existing.max_src_depth,
                            existing.sum_src_depth,
                            &existing.sorted_sources,
                            &existing.rule_iri,
                        );
                        if cand_key < exist_key {
                            *existing = candidate.clone();
                        }
                    })
                    .or_insert(candidate);
            }
        }

        if round.is_empty() {
            break; // fixpoint
        }

        // Commit all winners from this round.
        let mut new_delta: HashSet<FactKey> = HashSet::with_capacity(round.len());
        for (_key, winner) in round {
            let winner_depth = winner.max_src_depth.saturating_add(1);
            depth.insert(winner.key.clone(), winner_depth);
            store.insert(winner.head.clone());
            new_delta.insert(winner.key.clone());

            // Record provenance only for genuinely-derived facts (a rule whose
            // head re-states an EDB fact is not a derivation row).
            if !edb_keys.contains(&winner.key) {
                derivations.push(DerivedRow {
                    graph: String::new(),
                    subject: winner.head.subject,
                    predicate: winner.head.predicate,
                    object: winner.head.object,
                    rule_iri: winner.rule_iri,
                    source_quad_ids: winner.sources, // body-order, NEVER sorted copy
                    derivation_id: winner.deriv,
                });
            }
        }

        delta = new_delta;
    }

    Ok(ReductResult { store, derivations })
}

/// Ground a rule head into a [`Fact`], failing hard on an unbound head variable or
/// a literal subject/predicate.
fn ground_head(head: &EvalAtom, sol: &Solution) -> Result<Fact, String> {
    let subject = ground_term_to_oxi(&head.subject, sol, "head subject")?;
    let object = ground_term_to_oxi(&head.object, sol, "head object")?;
    // The subject must be an IRI/blank node, never a literal.
    if let Term::Literal(_) = subject {
        return Err("rule_ir: head subject grounded to a literal (no-optionality)".to_owned());
    }
    Ok(Fact {
        subject,
        predicate: head.predicate.clone(),
        object,
    })
}

/// Ground an [`EvalTerm`] into a concrete oxigraph [`Term`].
fn ground_term_to_oxi(term: &EvalTerm, sol: &Solution, slot: &str) -> Result<Term, String> {
    match term {
        EvalTerm::ConstNamed(nn) => Ok(Term::NamedNode(nn.clone())),
        EvalTerm::ConstLit(t) => Ok(t.clone()),
        EvalTerm::Var(name) => {
            let surface = sol
                .get(name)
                .ok_or_else(|| format!("{slot} variable {name:?} unbound after body matching"))?;
            surface_to_term(surface)
        }
    }
}

/// Re-materialize an oxigraph [`Term`] from its N3 surface (`<iri>` or a literal).
fn surface_to_term(surface: &str) -> Result<Term, String> {
    if let Some(iri) = surface.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        let nn =
            NamedNode::new(iri).map_err(|e| format!("rule_ir: invalid bound IRI {iri:?}: {e}"))?;
        return Ok(Term::NamedNode(nn));
    }
    if let Some(inner) = surface.strip_prefix("_:") {
        let bn = oxigraph::model::BlankNode::new(inner)
            .map_err(|e| format!("rule_ir: invalid bound blank node {surface:?}: {e}"))?;
        return Ok(Term::BlankNode(bn));
    }
    // Literal surface.
    parse_n3_object_literal(surface)
}

// ── Asserted-EDB echo (mirror of foundation.rs chase_world's assert block) ───────

/// Produce the asserted-EDB rows for one world.
///
/// Each row: `rule_iri = logic:assert`, `source_quad_ids = [self_reifier]`,
/// `derivation_id = mint_derivation_id(logic:assert, &[self_reifier])`.  The object
/// surface is the term's N3 form, matching `py.rs` and `foundation.rs`.
pub(crate) fn echo_asserted(world: &str, edb: &[Fact]) -> Result<Vec<DerivedRow>, String> {
    let mut out: Vec<DerivedRow> = Vec::with_capacity(edb.len());
    for f in edb {
        let reifier = f.reifier()?;
        let deriv = mint_derivation_id(ASSERT_RULE_IRI, &[reifier.as_str()]);
        out.push(DerivedRow {
            graph: world.to_owned(),
            subject: f.subject.clone(),
            predicate: f.predicate.clone(),
            object: f.object.clone(),
            rule_iri: ASSERT_RULE_IRI.to_owned(),
            source_quad_ids: vec![reifier],
            derivation_id: deriv,
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
/// Returns `Err` for an invalid IRI in the input.
pub(crate) fn world_edb_facts(
    store: &crate::store::WorldStore,
    world: &str,
) -> Result<Vec<Fact>, String> {
    let raw = store.quads_in_world(world);
    let mut facts: Vec<Fact> = Vec::with_capacity(raw.len());
    for r in &raw {
        // r[0], r[1], r[2] are N3 surfaces from oxigraph `to_string()`.
        let subject = surface_to_term(&r[0])?;
        let predicate = {
            let p = strip_angle(&r[1]);
            NamedNode::new(p).map_err(|e| format!("rule_ir: invalid predicate IRI {p:?}: {e}"))?
        };
        let object = surface_to_term(&r[2])?;
        facts.push(Fact {
            subject,
            predicate,
            object,
        });
    }
    facts.sort_by_key(Fact::key);
    Ok(facts)
}

/// Strip a leading `<` and trailing `>`; identity if absent.
fn strip_angle(s: &str) -> &str {
    s.strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A tiny stratifiable program for the join engine: r(?X,?X) :- p(?X,?Y).
    const NS: &str = "https://example.org/t/";

    fn rls_simple() -> String {
        format!("<{NS}r>(?X, ?X, ?W) :- <{NS}p>(?X, ?Y, ?W) .\n")
    }

    fn fact(s: &str, p: &str, o: &str) -> Fact {
        Fact {
            subject: Term::NamedNode(NamedNode::new(format!("{NS}{s}")).unwrap()),
            predicate: NamedNode::new(format!("{NS}{p}")).unwrap(),
            object: Term::NamedNode(NamedNode::new(format!("{NS}{o}")).unwrap()),
        }
    }

    #[test]
    fn parse_lowers_world_slot_dropped() {
        let rules = parse_eval_rules(&rls_simple()).expect("parse");
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        // head r(?X,?X): subject and object are both ?X.
        match (&r.head.subject, &r.head.object) {
            (EvalTerm::Var(a), EvalTerm::Var(b)) => {
                assert_eq!(a, "?X");
                assert_eq!(b, "?X");
            }
            other => panic!("unexpected head terms: {other:?}"),
        }
        assert_eq!(r.head.predicate.as_str(), format!("{NS}r"));
        // body has one positive atom p(?X,?Y); world slot dropped.
        assert_eq!(r.body.len(), 1);
        assert!(!r.body[0].negated);
        assert_eq!(r.body[0].predicate.as_str(), format!("{NS}p"));
    }

    #[test]
    fn reduct_derives_simple_head() {
        let rules = parse_eval_rules(&rls_simple()).expect("parse");
        let mut edb = FactStore::new();
        edb.insert(fact("a", "p", "b"));
        let reference = FactStore::new();
        let res = least_model_of_reduct(&edb, &rules, &reference).expect("lmr");
        // r(a,a) should be derived.
        let key = (format!("<{NS}a>"), format!("{NS}r"), format!("<{NS}a>"));
        assert!(
            res.store.contains_key(&key),
            "r(a,a) should be in the model"
        );
        assert_eq!(res.derivations.len(), 1, "exactly one derived row");
        let row = &res.derivations[0];
        // An unnamed rule synthesizes the logic-namespace anonymous IRI.
        let anon = format!("{LOGIC_NAMESPACE}rule/anonymous");
        assert_eq!(row.rule_iri, anon);
        // source = reifier of the matched p(a,b) fact.
        let want_src = fact("a", "p", "b").reifier().unwrap();
        assert_eq!(row.source_quad_ids, vec![want_src.clone()]);
        assert_eq!(
            row.derivation_id,
            mint_derivation_id(&anon, &[want_src.as_str()])
        );
    }

    #[test]
    fn echo_asserted_uses_assert_sentinel() {
        let f = fact("a", "p", "b");
        let rows = echo_asserted("urn:w", std::slice::from_ref(&f)).expect("echo");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].rule_iri, ASSERT_RULE_IRI);
        let self_ref = f.reifier().unwrap();
        assert_eq!(rows[0].source_quad_ids, vec![self_ref.clone()]);
        assert_eq!(
            rows[0].derivation_id,
            mint_derivation_id(ASSERT_RULE_IRI, &[self_ref.as_str()])
        );
    }

    // ── Determinism / quality-ordered tiebreak test ───────────────────────────
    //
    // Mirrors `foundation/tests.rs::first_wins_tiebreak_prefers_most_direct_derivation_order_independent`
    // but uses `RuleRoundCandidate`'s four-field tiebreak key, which adds `rule_iri`
    // as a total-order backstop (since rule IRIs vary per rule in rule_ir, unlike
    // foundation.rs where a single anonymous IRI is used for all rules).
    //
    // Proves:
    //   1. Depth dominates lex order — shallower wins over lex-smaller deeper candidate.
    //   2. Sum-depth tiebreaks at equal max-depth.
    //   3. Lex-min sorted_sources as final content-addressed tiebreaker (all depths equal).
    //   4. All three levels are enumeration-order-independent (forward, reverse, permuted).
    //   5. `rule_iri` provides a total-order backstop when all other fields are equal.
    /// Verify that the per-round winner-selection tiebreak in `least_model_of_reduct` is
    /// quality-ordered and independent of the order in which candidates are folded into
    /// the round map.
    ///
    /// The total order is `(max_src_depth, sum_src_depth, sorted_sources, rule_iri)` —
    /// smaller wins.  Self-contained; no external store or rule parsing required.
    #[test]
    fn first_wins_tiebreak_prefers_most_direct_derivation_order_independent() {
        /// A minimal stand-in for [`RuleRoundCandidate`]'s comparison key.
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct FakeCand {
            max_depth: u32,
            sum_depth: u64,
            sorted_sources: Vec<String>,
            rule_iri: String,
            label: &'static str, // for assertion messages only
        }

        /// Fold a slice of candidates using the same `and_modify` logic as
        /// `least_model_of_reduct`, returning a clone of the winning candidate.
        fn fold(cands: &[FakeCand]) -> FakeCand {
            let mut winner: Option<FakeCand> = None;
            for c in cands {
                match &winner {
                    None => winner = Some(c.clone()),
                    Some(w) => {
                        let c_key = (c.max_depth, c.sum_depth, &c.sorted_sources, &c.rule_iri);
                        let w_key = (w.max_depth, w.sum_depth, &w.sorted_sources, &w.rule_iri);
                        if c_key < w_key {
                            winner = Some(c.clone());
                        }
                    }
                }
            }
            winner.unwrap()
        }

        // ── Level 1: depth dominates lex order ──────────────────────────────
        //
        // `shallow` has max_depth=1; `deep` has max_depth=2 but a lex-smaller
        // sorted_sources.  Expected winner: `shallow` (depth beats lex).
        let shallow = FakeCand {
            max_depth: 1,
            sum_depth: 1,
            sorted_sources: vec!["urn:z".to_owned()], // lex-larger
            rule_iri: "urn:rule/b".to_owned(),
            label: "shallow",
        };
        let deep = FakeCand {
            max_depth: 2,
            sum_depth: 2,
            sorted_sources: vec!["urn:a".to_owned()], // lex-smaller — but loses on depth
            rule_iri: "urn:rule/a".to_owned(),
            label: "deep",
        };
        let pool1 = vec![shallow.clone(), deep.clone()];
        let pool1_rev = vec![deep.clone(), shallow.clone()];
        assert_eq!(fold(&pool1).label, "shallow", "fwd: depth must beat lex");
        assert_eq!(
            fold(&pool1_rev).label,
            "shallow",
            "rev: depth must beat lex"
        );

        // ── Level 2: sum-depth tiebreak at equal max-depth ───────────────────
        //
        // `asserted_rooted` has max=1, sum=1; `chain_rooted` has max=1, sum=3.
        // Same max-depth; sum-depth picks `asserted_rooted`.
        let asserted_rooted = FakeCand {
            max_depth: 1,
            sum_depth: 1,
            sorted_sources: vec!["urn:m".to_owned()], // lex-larger
            rule_iri: "urn:rule/b".to_owned(),
            label: "asserted_rooted",
        };
        let chain_rooted = FakeCand {
            max_depth: 1,
            sum_depth: 3,
            sorted_sources: vec!["urn:a".to_owned()], // lex-smaller — but loses on sum
            rule_iri: "urn:rule/a".to_owned(),
            label: "chain_rooted",
        };
        let pool2 = vec![asserted_rooted.clone(), chain_rooted.clone()];
        let pool2_rev = vec![chain_rooted.clone(), asserted_rooted.clone()];
        assert_eq!(
            fold(&pool2).label,
            "asserted_rooted",
            "fwd: sum-depth must beat lex at equal max-depth"
        );
        assert_eq!(
            fold(&pool2_rev).label,
            "asserted_rooted",
            "rev: sum-depth must beat lex at equal max-depth"
        );

        // ── Level 3: lex-min sorted_sources as content-addressed tiebreaker ─
        //
        // All candidates have same max-depth, sum-depth, and rule_iri;
        // only sorted_sources (lex order) decides.
        let cands3: Vec<FakeCand> = vec![
            FakeCand {
                max_depth: 0,
                sum_depth: 0,
                sorted_sources: vec!["urn:a".to_owned(), "urn:c".to_owned()],
                rule_iri: "urn:rule/x".to_owned(),
                label: "ac",
            },
            FakeCand {
                max_depth: 0,
                sum_depth: 0,
                sorted_sources: vec!["urn:a".to_owned(), "urn:b".to_owned()], // ← lex smallest
                rule_iri: "urn:rule/x".to_owned(),
                label: "ab",
            },
            FakeCand {
                max_depth: 0,
                sum_depth: 0,
                sorted_sources: vec!["urn:b".to_owned(), "urn:d".to_owned()],
                rule_iri: "urn:rule/x".to_owned(),
                label: "bd",
            },
        ];
        let cands3_rev: Vec<FakeCand> = cands3.iter().cloned().rev().collect();
        let cands3_perm: Vec<FakeCand> =
            vec![cands3[2].clone(), cands3[0].clone(), cands3[1].clone()];
        assert_eq!(
            fold(&cands3).label,
            "ab",
            "fwd: lex-min sorted_sources must win when depths equal"
        );
        assert_eq!(
            fold(&cands3_rev).label,
            "ab",
            "rev: lex-min sorted_sources must win when depths equal"
        );
        assert_eq!(
            fold(&cands3_perm).label,
            "ab",
            "perm: lex-min sorted_sources must win when depths equal"
        );

        // ── Level 4: rule_iri total-order backstop ───────────────────────────
        //
        // All depth/sum/sorted_sources equal; rule_iri lex order is the final tiebreak.
        let rule_a = FakeCand {
            max_depth: 0,
            sum_depth: 0,
            sorted_sources: vec!["urn:s".to_owned()],
            rule_iri: "urn:rule/a".to_owned(), // ← lex smallest
            label: "rule_a",
        };
        let rule_b = FakeCand {
            max_depth: 0,
            sum_depth: 0,
            sorted_sources: vec!["urn:s".to_owned()],
            rule_iri: "urn:rule/b".to_owned(),
            label: "rule_b",
        };
        let pool4 = vec![rule_b.clone(), rule_a.clone()];
        let pool4_rev = vec![rule_a.clone(), rule_b.clone()];
        assert_eq!(
            fold(&pool4).label,
            "rule_a",
            "fwd: lex-min rule_iri must win when all other fields equal"
        );
        assert_eq!(
            fold(&pool4_rev).label,
            "rule_a",
            "rev: lex-min rule_iri must win when all other fields equal"
        );
    }
}
