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
//! insertion order, with O(1) dedup on the N3 tuple key and TWO secondary indexes
//! (`by_subject`, `by_object`) maintained in lockstep, exactly mirroring
//! `FactStore`'s `predicate_index` discipline.
//!
//! # Determinism (non-negotiable)
//!
//! - Tuples are stored in insertion order; both indexes append row indices in
//!   lockstep so every bucket's order equals insertion order.
//! - The N3 surfaces `(subject.to_string(), object.to_string())` are the canonical
//!   keys, matching `Fact::key`'s subject/object components.
//! - Any "all predicates" / "all tuples" iteration is sorted (BTreeSet/BTreeMap),
//!   never raw `HashMap` iteration order, so the engine's output is byte-stable.
//!
//! # The single oxigraph → columnar bridge
//!
//! [`extract_edb`] is the SOLE place the forward and backward engine paths cross from
//! the oxigraph blackboard ([`crate::seam::ScryerForeign`]) into the columnar form.

use std::collections::{BTreeSet, HashMap, HashSet};

use oxigraph::model::{NamedNode, Term};

use crate::seam::ScryerForeign;

/// A position-pattern over a binary relation's `(subject, object)` columns.
///
/// The `&str` payloads are N3 surfaces (`Term::to_string()`), matching the keys of
/// the secondary indexes; this lets a join probe the relation without re-stringifying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bound<'a> {
    /// No position bound — every tuple, in insertion order.
    Any,
    /// Subject bound to this N3 surface.
    Subject(&'a str),
    /// Object bound to this N3 surface.
    Object(&'a str),
    /// Both positions bound (subject, object) to these N3 surfaces.
    Both(&'a str, &'a str),
}

/// A single binary relation: `(subject, object)` tuples for ONE predicate IRI.
///
/// Insertion-ordered, O(1)-deduped on the N3 tuple key, with subject/object indexes
/// maintained in lockstep — the column-oriented sibling of `FactStore`'s
/// predicate bucket.
#[derive(Debug, Clone, Default)]
struct Relation {
    /// `(subject, object)` tuples in insertion order.
    rows: Vec<(Term, Term)>,
    /// Dedup keys `(subject.to_string(), object.to_string())` for O(1) membership.
    keys: HashSet<(String, String)>,
    /// Subject N3 surface → row indices into `rows`, in insertion order.
    by_subject: HashMap<String, Vec<usize>>,
    /// Object N3 surface → row indices into `rows`, in insertion order.
    by_object: HashMap<String, Vec<usize>>,
}

impl Relation {
    /// Insert `(subject, object)` if its N3 key is new; return `true` if inserted.
    ///
    /// On a successful insert the new row index is appended to BOTH indexes in
    /// lockstep with `rows`, so each bucket's order equals insertion order.
    fn insert(&mut self, subject: Term, object: Term) -> bool {
        let s_key = subject.to_string();
        let o_key = object.to_string();
        let key = (s_key.clone(), o_key.clone());
        if self.keys.contains(&key) {
            return false;
        }
        self.keys.insert(key);
        let idx = self.rows.len();
        self.rows.push((subject, object));
        self.by_subject.entry(s_key).or_default().push(idx);
        self.by_object.entry(o_key).or_default().push(idx);
        true
    }

    /// Whether a tuple with these N3 surfaces is present.
    fn contains(&self, subject: &str, object: &str) -> bool {
        // `HashSet::contains` needs an owned key here because the set is keyed on
        // `(String, String)`; the borrow surface is a `(&str, &str)` lookup which the
        // stdlib does not expose for tuple keys, so build the pair.
        self.keys.contains(&(subject.to_owned(), object.to_owned()))
    }

    /// Rows whose subject N3 surface equals `s`, in insertion order.
    fn rows_for_subject(&self, s: &str) -> &[usize] {
        self.by_subject.get(s).map_or(&[][..], Vec::as_slice)
    }

    /// Rows whose object N3 surface equals `o`, in insertion order.
    fn rows_for_object(&self, o: &str) -> &[usize] {
        self.by_object.get(o).map_or(&[][..], Vec::as_slice)
    }

    /// Materialize the tuples selected by `bound`, cloned, in insertion order.
    ///
    /// `Both` picks the SMALLER of the two index buckets to scan, filtering against
    /// the other position — the cheapest probe for the bound positions.
    fn select(&self, bound: Bound) -> Vec<(Term, Term)> {
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
                // Scan the smaller bucket and filter on the other position's surface.
                // Both buckets are insertion-ordered, so the surviving subsequence is
                // exactly what a full scan would yield in the same relative order.
                if by_s.len() <= by_o.len() {
                    by_s.iter()
                        .map(|&i| &self.rows[i])
                        .filter(|(_, obj)| obj.to_string() == o)
                        .cloned()
                        .collect()
                } else {
                    by_o.iter()
                        .map(|&i| &self.rows[i])
                        .filter(|(subj, _)| subj.to_string() == s)
                        .cloned()
                        .collect()
                }
            }
        }
    }
}

/// A columnar set of binary relations keyed by predicate IRI (`NamedNode::as_str()`).
///
/// One [`Relation`] per predicate; this is the native engine's working EDB/IDB form.
#[derive(Debug, Clone, Default)]
pub(crate) struct RelationStore {
    /// Predicate IRI surface → its binary relation.
    relations: HashMap<String, Relation>,
}

impl RelationStore {
    /// A fresh, empty store.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Insert `(subject, object)` under `predicate`; return `true` if newly inserted.
    ///
    /// Deduped on the N3 tuple key per predicate; both secondary indexes are
    /// maintained in lockstep.
    pub(crate) fn insert(&mut self, predicate: &NamedNode, subject: Term, object: Term) -> bool {
        self.relations
            .entry(predicate.as_str().to_owned())
            .or_default()
            .insert(subject, object)
    }

    /// Whether `(subject, predicate, object)` is present (N3 surfaces).
    ///
    /// Membership on N3 surfaces, for NAF and downstream dedup.
    pub(crate) fn contains(&self, predicate: &str, subject: &str, object: &str) -> bool {
        self.relations
            .get(predicate)
            .is_some_and(|r| r.contains(subject, object))
    }

    /// The tuples under `predicate` selected by `bound`, cloned, in insertion order.
    ///
    /// Picks the cheapest index for the bound positions; an unknown predicate yields
    /// the empty vector.
    pub(crate) fn select(&self, predicate: &str, bound: Bound) -> Vec<(Term, Term)> {
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
    /// Sorted (BTreeSet) so any "all relations" sweep is deterministic.
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
pub(crate) fn extract_edb(foreign: &dyn ScryerForeign, world: &NamedNode) -> RelationStore {
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

    fn nn(iri: &str) -> NamedNode {
        NamedNode::new(iri).expect("valid IRI")
    }

    fn term(iri: &str) -> Term {
        Term::NamedNode(nn(iri))
    }

    /// Build a store with a small `knows`/`likes` corpus.
    ///
    /// `knows`: (a,b), (a,c), (b,c)  — `likes`: (a,c)
    fn sample_store() -> RelationStore {
        let knows = nn("http://ex/knows");
        let likes = nn("http://ex/likes");
        let mut s = RelationStore::new();
        assert!(s.insert(&knows, term("http://ex/a"), term("http://ex/b")));
        assert!(s.insert(&knows, term("http://ex/a"), term("http://ex/c")));
        assert!(s.insert(&knows, term("http://ex/b"), term("http://ex/c")));
        assert!(s.insert(&likes, term("http://ex/a"), term("http://ex/c")));
        s
    }

    #[test]
    fn physical_select_subject_bound() {
        let s = sample_store();
        let got = s.select("http://ex/knows", Bound::Subject("<http://ex/a>"));
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
        let got = s.select("http://ex/knows", Bound::Object("<http://ex/c>"));
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
        let got = s.select(
            "http://ex/knows",
            Bound::Both("<http://ex/a>", "<http://ex/c>"),
        );
        assert_eq!(got, vec![(term("http://ex/a"), term("http://ex/c"))]);

        // A both-bound miss yields nothing.
        let none = s.select(
            "http://ex/knows",
            Bound::Both("<http://ex/b>", "<http://ex/b>"),
        );
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
        let knows = nn("http://ex/knows");
        let mut s = RelationStore::new();
        assert!(s.insert(&knows, term("http://ex/a"), term("http://ex/b")));
        // Re-inserting the same (s,p,o) is a no-op that reports false.
        assert!(!s.insert(&knows, term("http://ex/a"), term("http://ex/b")));
        assert_eq!(s.len_for("http://ex/knows"), 1);
        assert_eq!(
            s.select("http://ex/knows", Bound::Any),
            vec![(term("http://ex/a"), term("http://ex/b"))],
        );
    }

    #[test]
    fn physical_contains_on_n3_surfaces() {
        let s = sample_store();
        assert!(s.contains("http://ex/knows", "<http://ex/a>", "<http://ex/b>"));
        assert!(!s.contains("http://ex/knows", "<http://ex/a>", "<http://ex/z>"));
        // Unknown predicate is a clean miss, not a panic.
        assert!(!s.contains("http://ex/nope", "<http://ex/a>", "<http://ex/b>"));
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
        world: NamedNode,
        quads: Vec<DerivedQuad>,
    }

    impl FakeForeign {
        fn new(world: &str, tuples: &[(&str, &str, &str)]) -> Self {
            let world_nn = nn(world);
            let quads = tuples
                .iter()
                .map(|(s, p, o)| DerivedQuad {
                    graph: world_nn.clone(),
                    subject: term(s),
                    predicate: nn(p),
                    object: term(o),
                    graph_component: world_nn.clone(),
                    derivation_id: DerivationId("http://ex/d".to_owned()),
                    rule_iri: "http://ex/r".to_owned(),
                    source_quad_ids: vec![],
                    profile: "http://ex/profile".to_owned(),
                    budget_status: BudgetStatus::Ok,
                })
                .collect();
            Self {
                world: world_nn,
                quads,
            }
        }
    }

    impl ScryerForeign for FakeForeign {
        fn in_world<'a>(
            &'a self,
            world: &NamedNode,
            subject: Option<&Term>,
            predicate: Option<&NamedNode>,
            object: Option<&Term>,
        ) -> Box<dyn Iterator<Item = &'a DerivedQuad> + 'a> {
            let world = world.clone();
            let subject = subject.cloned();
            let predicate = predicate.cloned();
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
            _world: &NamedNode,
        ) -> Box<dyn Iterator<Item = NamedNode> + 'a> {
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
