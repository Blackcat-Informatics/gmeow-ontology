// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Columnar `RelationStore` + index selection + the shared EDB extractor.
//!
//! # Why a second store next to `FactStore`
//!
//! [`crate::rule_ir::FactStore`] is a *ternary* `(subject, predicate, object)` store
//! bucketed by predicate only.  The native execution core joins over
//! *binary* relations — one relation per predicate IRI — and needs to select rows by
//! a bound **subject** OR a bound **object**, not just by predicate.  This module is
//! the column-oriented analogue: per predicate it keeps `(subject, object)` tuples in
//! insertion order, with O(1) dedup on the interned tuple key and TWO secondary
//! indexes (`by_subject`, `by_object`) maintained in lockstep, exactly mirroring
//! `FactStore`'s `predicate_index` discipline.
//!
//! # Determinism (non-negotiable)
//!
//! - Tuples are stored in insertion order; both indexes append row indices in
//!   lockstep so every bucket's order equals insertion order.
//! - Keys and index buckets are [`TermId`]s minted by the store's single
//!   [`TermInterner`], which is keyed on the [`crate::provenance::term_display`]
//!   surface — so two
//!   terms share an id exactly when their display surfaces are byte-equal,
//!   preserving the string-keyed dedup semantics byte-exactly.  `TermId`s are
//!   per-store handles: they are assigned in insertion order, NEVER sorted by
//!   (their derived order is mint order, not lexical order), and never
//!   serialized or hashed — canonical sorts stay on the string surfaces.
//! - A join probe translates a ground surface to an id via
//!   [`RelationStore::term_id`] (non-inserting): a miss means the term has never
//!   entered the store, so the selection is empty — the single place that
//!   semantics lives.
//! - Any "all predicates" / "all tuples" iteration is sorted (BTreeSet/BTreeMap),
//!   never raw `HashMap` iteration order, so the engine's output is byte-stable.
//!
//! # The single oxigraph → columnar bridge
//!
//! [`extract_edb`] is the SOLE place the forward and backward engine paths cross from
//! the oxigraph blackboard ([`crate::seam::ScryerForeign`]) into the columnar form.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

use purrdf::TermValue;

use crate::facts::{TermId, TermInterner};
use crate::seam::ScryerForeign;

/// A position-pattern over a binary relation's `(subject, object)` columns.
///
/// The [`TermId`] payloads are handles minted by the interner of the SAME
/// [`RelationStore`] the bound is probed against (obtain them via
/// [`RelationStore::term_id`]); this lets a join probe the relation without
/// re-stringifying or re-hashing term surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bound {
    /// No position bound — every tuple, in insertion order.
    Any,
    /// Subject bound to this interned term.
    Subject(TermId),
    /// Object bound to this interned term.
    Object(TermId),
    /// Both positions bound (subject, object) to these interned terms.
    Both(TermId, TermId),
}

/// A single binary relation: `(subject, object)` tuples for ONE predicate IRI.
///
/// Insertion-ordered, O(1)-deduped on the interned tuple key, with subject/object
/// indexes maintained in lockstep — the column-oriented sibling of `FactStore`'s
/// predicate bucket.  Term interning lives at the [`RelationStore`] level (one
/// dictionary shared by every relation), so `insert` borrows the store's interner.
#[derive(Debug, Clone, Default)]
struct Relation {
    /// `(subject, object)` tuples in insertion order.
    rows: Vec<(TermValue, TermValue)>,
    /// Dedup keys `(subject_id, object_id)` for O(1) membership.
    keys: HashSet<(TermId, TermId)>,
    /// Subject term id → row indices into `rows`, in insertion order.
    by_subject: HashMap<TermId, Vec<usize>>,
    /// Object term id → row indices into `rows`, in insertion order.
    by_object: HashMap<TermId, Vec<usize>>,
}

impl Relation {
    /// Insert `(subject, object)` if its interned key is new; return `true` if inserted.
    ///
    /// On a successful insert the new row index is appended to BOTH indexes in
    /// lockstep with `rows`, so each bucket's order equals insertion order.
    fn insert(
        &mut self,
        interner: &mut TermInterner,
        subject: TermValue,
        object: TermValue,
    ) -> bool {
        let s_id = interner.intern(&subject);
        let o_id = interner.intern(&object);
        if !self.keys.insert((s_id, o_id)) {
            return false;
        }
        let idx = self.rows.len();
        self.rows.push((subject, object));
        self.by_subject.entry(s_id).or_default().push(idx);
        self.by_object.entry(o_id).or_default().push(idx);
        true
    }

    /// Whether a tuple with these interned terms is present.
    fn contains(&self, subject: TermId, object: TermId) -> bool {
        self.keys.contains(&(subject, object))
    }

    /// Rows whose subject is the interned term `s`, in insertion order.
    fn rows_for_subject(&self, s: TermId) -> &[usize] {
        self.by_subject.get(&s).map_or(&[][..], Vec::as_slice)
    }

    /// Rows whose object is the interned term `o`, in insertion order.
    fn rows_for_object(&self, o: TermId) -> &[usize] {
        self.by_object.get(&o).map_or(&[][..], Vec::as_slice)
    }

    /// Materialize the tuples selected by `bound`, cloned, in insertion order.
    ///
    /// `Both` picks the SMALLER of the two index buckets to scan, filtering against
    /// the other position — the cheapest probe for the bound positions.
    fn select(&self, bound: Bound) -> Vec<(TermValue, TermValue)> {
        match bound {
            Bound::Any => self.rows.clone(),
            Bound::Subject(s) => self
                .rows_for_subject(s)
                .iter()
                .map(|&i| self.rows[i].clone())
                .collect(),
            Bound::Object(o) => self
                .rows_for_object(o)
                .iter()
                .map(|&i| self.rows[i].clone())
                .collect(),
            Bound::Both(s, o) => {
                let by_s = self.rows_for_subject(s);
                let by_o = self.rows_for_object(o);
                // Both buckets hold row indices in ascending (insertion) order, so the
                // rows satisfying BOTH bounds are exactly their sorted intersection: a
                // two-pointer merge, with no per-row term re-hashing. The result keeps
                // insertion order, matching a full scan's relative order.
                let mut out = Vec::new();
                let (mut i, mut j) = (0usize, 0usize);
                while i < by_s.len() && j < by_o.len() {
                    match by_s[i].cmp(&by_o[j]) {
                        Ordering::Less => i += 1,
                        Ordering::Greater => j += 1,
                        Ordering::Equal => {
                            out.push(self.rows[by_s[i]].clone());
                            i += 1;
                            j += 1;
                        }
                    }
                }
                out
            }
        }
    }
}

/// A columnar set of binary relations keyed by predicate IRI (`NamedNode::as_str()`).
///
/// One [`Relation`] per predicate, all sharing ONE [`TermInterner`]; this is the
/// native engine's working EDB/IDB form.  The ids the interner mints are meaningless
/// outside this store — probes obtain them via [`Self::term_id`].
#[derive(Debug, Clone, Default)]
pub(crate) struct RelationStore {
    /// The store's term dictionary, shared by every relation.
    interner: TermInterner,
    /// Predicate IRI surface → its binary relation.
    relations: HashMap<String, Relation>,
}

impl RelationStore {
    /// A fresh, empty store.
    pub(crate) fn new() -> Self {
        Self {
            interner: TermInterner::new(),
            relations: HashMap::new(),
        }
    }

    /// Insert `(subject, object)` under `predicate`; return `true` if newly inserted.
    ///
    /// Deduped on the interned tuple key per predicate; both secondary indexes are
    /// maintained in lockstep.
    pub(crate) fn insert(
        &mut self,
        predicate: &str,
        subject: TermValue,
        object: TermValue,
    ) -> bool {
        self.relations
            .entry(predicate.to_owned())
            .or_default()
            .insert(&mut self.interner, subject, object)
    }

    /// The interned id of the term with this [`crate::provenance::term_display`]
    /// surface, if the term
    /// has ever been inserted into ANY relation of this store.  Never inserts.
    ///
    /// This is the SINGLE place probe-miss semantics lives: `None` means the term
    /// has never been seen, so any selection or membership bound on it is empty /
    /// false — callers short-circuit to the empty result exactly where a
    /// surface-keyed index would have produced zero matches.
    pub(crate) fn term_id(&self, display: &str) -> Option<TermId> {
        self.interner.lookup(display)
    }

    /// Whether `(subject, predicate, object)` is present (display surfaces).
    ///
    /// Membership for NAF and downstream dedup: both surfaces must resolve to
    /// interned ids ([`Self::term_id`]) or the tuple cannot be present.
    pub(crate) fn contains(&self, predicate: &str, subject: &str, object: &str) -> bool {
        let (Some(s), Some(o)) = (self.term_id(subject), self.term_id(object)) else {
            return false;
        };
        self.relations
            .get(predicate)
            .is_some_and(|r| r.contains(s, o))
    }

    /// The tuples under `predicate` selected by `bound`, cloned, in insertion order.
    ///
    /// Picks the cheapest index for the bound positions; an unknown predicate yields
    /// the empty vector.
    pub(crate) fn select(&self, predicate: &str, bound: Bound) -> Vec<(TermValue, TermValue)> {
        self.relations
            .get(predicate)
            .map_or_else(Vec::new, |r| r.select(bound))
    }

    /// The number of distinct tuples stored under `predicate` (0 if unknown).
    pub(crate) fn len_for(&self, predicate: &str) -> usize {
        self.relations.get(predicate).map_or(0, |r| r.rows.len())
    }

    /// Every predicate IRI surface that has at least one tuple, in sorted order.
    ///
    /// Sorted (BTreeSet) so any "all relations" sweep is deterministic.  Predicate
    /// names stay `String`-keyed and lexically sorted — NEVER `TermId`-ordered
    /// (id order is mint order, not lexical order).
    pub(crate) fn predicates(&self) -> impl Iterator<Item = &str> {
        self.relations
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            .into_iter()
    }
}

/// Extract the EDB of `world` from the blackboard into columnar form.
///
/// This is the SINGLE oxigraph → columnar bridge used by both the forward and
/// backward native engine paths.  It scans every quad in `world` via
/// [`ScryerForeign::in_world`] and inserts each `(subject, predicate, object)` as a
/// binary tuple.  Insertion order follows `in_world`'s iteration order; dedup and
/// index maintenance are handled by [`RelationStore::insert`].
pub(crate) fn extract_edb(foreign: &dyn ScryerForeign, world: &str) -> RelationStore {
    let mut store = RelationStore::new();
    for dq in foreign.in_world(world, None, None, None) {
        store.insert(&dq.predicate, dq.subject.clone(), dq.object.clone());
    }
    store
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::{BudgetStatus, DerivationId, DerivedQuad};

    fn term(iri: &str) -> TermValue {
        TermValue::iri(iri)
    }

    /// The interned id for a display surface, asserting it is present.
    fn id_of(s: &RelationStore, display: &str) -> TermId {
        s.term_id(display)
            .unwrap_or_else(|| panic!("term {display:?} must be interned"))
    }

    /// Build a store with a small `knows`/`likes` corpus.
    ///
    /// `knows`: (a,b), (a,c), (b,c)  — `likes`: (a,c)
    fn sample_store() -> RelationStore {
        let knows = "http://ex/knows";
        let likes = "http://ex/likes";
        let mut s = RelationStore::new();
        assert!(s.insert(knows, term("http://ex/a"), term("http://ex/b")));
        assert!(s.insert(knows, term("http://ex/a"), term("http://ex/c")));
        assert!(s.insert(knows, term("http://ex/b"), term("http://ex/c")));
        assert!(s.insert(likes, term("http://ex/a"), term("http://ex/c")));
        s
    }

    #[test]
    fn physical_select_subject_bound() {
        let s = sample_store();
        let a = id_of(&s, "<http://ex/a>");
        let got = s.select("http://ex/knows", Bound::Subject(a));
        assert_eq!(
            got,
            vec![
                (term("http://ex/a"), term("http://ex/b")),
                (term("http://ex/a"), term("http://ex/c")),
            ],
        );
    }

    #[test]
    fn physical_select_object_bound() {
        let s = sample_store();
        let c = id_of(&s, "<http://ex/c>");
        let got = s.select("http://ex/knows", Bound::Object(c));
        assert_eq!(
            got,
            vec![
                (term("http://ex/a"), term("http://ex/c")),
                (term("http://ex/b"), term("http://ex/c")),
            ],
        );
    }

    #[test]
    fn physical_select_both_bound() {
        let s = sample_store();
        let a = id_of(&s, "<http://ex/a>");
        let b = id_of(&s, "<http://ex/b>");
        let c = id_of(&s, "<http://ex/c>");
        let got = s.select("http://ex/knows", Bound::Both(a, c));
        assert_eq!(got, vec![(term("http://ex/a"), term("http://ex/c"))]);

        // A both-bound miss (b is interned but (b,b) is not a tuple) yields nothing.
        let none = s.select("http://ex/knows", Bound::Both(b, b));
        assert!(none.is_empty());
    }

    #[test]
    fn physical_select_any_is_insertion_order() {
        let s = sample_store();
        let got = s.select("http://ex/knows", Bound::Any);
        assert_eq!(
            got,
            vec![
                (term("http://ex/a"), term("http://ex/b")),
                (term("http://ex/a"), term("http://ex/c")),
                (term("http://ex/b"), term("http://ex/c")),
            ],
        );
    }

    #[test]
    fn physical_dedup_returns_false_and_stores_one_row() {
        let knows = "http://ex/knows";
        let mut s = RelationStore::new();
        assert!(s.insert(knows, term("http://ex/a"), term("http://ex/b")));
        // Re-inserting the same (s,p,o) is a no-op that reports false.
        assert!(!s.insert(knows, term("http://ex/a"), term("http://ex/b")));
        assert_eq!(s.len_for("http://ex/knows"), 1);
        assert_eq!(
            s.select("http://ex/knows", Bound::Any),
            vec![(term("http://ex/a"), term("http://ex/b"))],
        );
    }

    #[test]
    fn physical_contains_on_display_surfaces() {
        let s = sample_store();
        assert!(s.contains("http://ex/knows", "<http://ex/a>", "<http://ex/b>"));
        // A never-seen term surface fails the lookup, so containment is false.
        assert!(!s.contains("http://ex/knows", "<http://ex/a>", "<http://ex/z>"));
        // Unknown predicate is a clean miss, not a panic.
        assert!(!s.contains("http://ex/nope", "<http://ex/a>", "<http://ex/b>"));
    }

    #[test]
    fn physical_term_id_lookup_never_inserts() {
        let s = sample_store();
        // Interned terms resolve; a never-seen surface is None (⇒ empty selection).
        assert!(s.term_id("<http://ex/a>").is_some());
        assert_eq!(s.term_id("<http://ex/never-seen>"), None);
        // The miss did not insert: a second lookup still misses.
        assert_eq!(s.term_id("<http://ex/never-seen>"), None);
    }

    #[test]
    fn physical_interner_is_shared_across_relations() {
        // The same term inserted under two predicates mints ONE id (store-level
        // interner), and a Bound built from that id probes either relation.
        let s = sample_store();
        let a = id_of(&s, "<http://ex/a>");
        assert_eq!(
            s.select("http://ex/likes", Bound::Subject(a)),
            vec![(term("http://ex/a"), term("http://ex/c"))],
        );
    }

    #[test]
    fn physical_predicates_are_sorted_and_deterministic() {
        let s = sample_store();
        let preds: Vec<&str> = s.predicates().collect();
        assert_eq!(preds, vec!["http://ex/knows", "http://ex/likes"]);

        // Repeated builds give identical select output (determinism).
        let s2 = sample_store();
        assert_eq!(
            s.select("http://ex/knows", Bound::Any),
            s2.select("http://ex/knows", Bound::Any),
        );
        let p2: Vec<&str> = s2.predicates().collect();
        assert_eq!(preds, p2);
    }

    // ── extract_edb round-trip via a minimal ScryerForeign test double ───────────

    /// A hand-rolled `ScryerForeign` yielding a fixed list of `DerivedQuad`s in
    /// `world`. Only `in_world` is exercised by `extract_edb`; the other legs are
    /// vacuous (and unused) for this test.
    struct FakeForeign {
        world: String,
        quads: Vec<DerivedQuad>,
    }

    impl FakeForeign {
        fn new(world: &str, tuples: &[(&str, &str, &str)]) -> Self {
            let world_iri = world.to_owned();
            let quads = tuples
                .iter()
                .map(|(s, p, o)| DerivedQuad {
                    graph: world_iri.clone(),
                    subject: term(s),
                    predicate: (*p).to_owned(),
                    object: term(o),
                    graph_component: world_iri.clone(),
                    derivation_id: DerivationId("http://ex/d".to_owned()),
                    rule_iri: "http://ex/r".to_owned(),
                    source_quad_ids: vec![],
                    profile: "http://ex/profile".to_owned(),
                    budget_status: BudgetStatus::Ok,
                })
                .collect();
            Self {
                world: world_iri,
                quads,
            }
        }
    }

    impl ScryerForeign for FakeForeign {
        fn in_world<'a>(
            &'a self,
            world: &str,
            subject: Option<&TermValue>,
            predicate: Option<&str>,
            object: Option<&TermValue>,
        ) -> Box<dyn Iterator<Item = &'a DerivedQuad> + 'a> {
            let world = world.to_owned();
            let subject = subject.cloned();
            let predicate = predicate.map(str::to_owned);
            let object = object.cloned();
            Box::new(self.quads.iter().filter(move |dq| {
                dq.graph == world
                    && subject.as_ref().is_none_or(|s| &dq.subject == s)
                    && predicate.as_ref().is_none_or(|p| &dq.predicate == p)
                    && object.as_ref().is_none_or(|o| &dq.object == o)
            }))
        }

        fn derived_by<'a>(
            &'a self,
            _quad_id: Option<&DerivationId>,
            _rule: Option<&str>,
            _sources: Option<&[String]>,
        ) -> Box<dyn Iterator<Item = (&'a DerivationId, &'a str, &'a [String])> + 'a> {
            Box::new(std::iter::empty())
        }

        fn contradiction_witness<'a>(
            &'a self,
            _world: &str,
        ) -> Box<dyn Iterator<Item = String> + 'a> {
            Box::new(std::iter::empty())
        }
    }

    #[test]
    fn physical_extract_edb_round_trips() {
        let foreign = FakeForeign::new(
            "http://ex/world",
            &[
                ("http://ex/a", "http://ex/knows", "http://ex/b"),
                ("http://ex/a", "http://ex/knows", "http://ex/c"),
                ("http://ex/a", "http://ex/likes", "http://ex/c"),
                // A duplicate quad must collapse to one row.
                ("http://ex/a", "http://ex/knows", "http://ex/b"),
            ],
        );
        let edb = extract_edb(&foreign, &foreign.world);

        let preds: Vec<&str> = edb.predicates().collect();
        assert_eq!(preds, vec!["http://ex/knows", "http://ex/likes"]);
        assert_eq!(edb.len_for("http://ex/knows"), 2);
        assert_eq!(edb.len_for("http://ex/likes"), 1);
        assert_eq!(
            edb.select("http://ex/knows", Bound::Any),
            vec![
                (term("http://ex/a"), term("http://ex/b")),
                (term("http://ex/a"), term("http://ex/c")),
            ],
        );
        assert!(edb.contains("http://ex/likes", "<http://ex/a>", "<http://ex/c>"));
    }
}
