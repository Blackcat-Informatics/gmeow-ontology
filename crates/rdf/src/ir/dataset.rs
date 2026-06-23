// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The frozen, immutable `RdfDataset` and its infallible, zero-allocation
//! iteration surface (#819 C1).
//!
//! A `RdfDataset` is produced only by [`RdfDatasetBuilder::freeze`] after structural
//! validation has passed, so every consumer observes a dataset with valid ID
//! references, positionally well-formed quads, no triple-term cycles, deduplicated
//! quads/annotations, and capability flags computed once. Iteration does **not**
//! return `Result` and performs no heap allocations or term-string clones:
//! diagnostics belong to ingestion (the builder), not to reads of an already-frozen
//! dataset (see `docs/design/819-rdf-ir-dataflow.md`, *Iteration surface*).
//!
//! Two iteration views are offered:
//! - [`RdfDataset::quads`] yields [`QuadIds`] — a `Copy`, ID-native row for
//!   consumers that work in term ids.
//! - [`RdfDataset::quad_refs`] yields [`QuadRef`] — a borrowed, resolved view
//!   (`&str` lexical content, no allocation) for consumers that need values.
//!
//! [`super::builder::RdfDatasetBuilder`]: super::builder::RdfDatasetBuilder

use crate::{RdfLocation, RdfStoreCapabilities, RdfTextDirection};

use super::term::{BlankScope, InternedTerm, TermId};

/// A handle identifying a pushed quad by its dense (deduplicated) ordinal, used to
/// attach a source location sparsely. Like [`TermId`], it is local to one frozen
/// dataset and is **not** persistent or merge-stable.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct QuadHandle(u32);

impl QuadHandle {
    /// Construct a handle from a quad ordinal.
    ///
    /// Public so that provenance sidecars (e.g. `DatasetProvenance` in the
    /// `gmeow-validate` crate) can mint handles that correspond to a parallel
    /// quad sequence before or without a frozen `RdfDataset` being available.
    /// Within `gmeow-rdf` itself only the builder mints handles in deduplicated
    /// push order.
    pub fn from_index(index: u32) -> Self {
        Self(index)
    }

    /// The dense quad ordinal this handle addresses.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One frozen quad row, stored in deterministic order. `g == None` names the
/// default graph (the graph-default sentinel, C0.9).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct QuadRow {
    pub s: TermId,
    pub p: TermId,
    pub o: TermId,
    pub g: Option<TermId>,
}

// #837 P3a: with the `NonZeroU32` `TermId` niche, the `g: Option<TermId>` slot
// costs no discriminant word, so a quad row is 16 bytes (3×4 ids + 4 for the
// niche-packed optional graph) rather than 20. This is the ~20%-off-the-quad-table
// win; the assertion fails the build if the niche or field layout regresses.
const _: () = assert!(std::mem::size_of::<QuadRow>() == 16);

/// A small `Copy` quad row in term ids, for ID-native consumers. `g == None` is the
/// default graph.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct QuadIds {
    pub s: TermId,
    pub p: TermId,
    pub o: TermId,
    pub g: Option<TermId>,
}

impl From<QuadRow> for QuadIds {
    #[inline]
    fn from(row: QuadRow) -> Self {
        Self {
            s: row.s,
            p: row.p,
            o: row.o,
            g: row.g,
        }
    }
}

/// A borrowed, resolved view of a term — mirrors [`InternedTerm`] but exposes
/// `&str` slices borrowed from the dataset, so resolving a term performs **no
/// allocation and no clone**. Triple components are returned as ids; resolve them
/// recursively with [`RdfDataset::resolve`] if their values are needed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TermRef<'a> {
    /// An IRI, by its borrowed full string.
    Iri(&'a str),
    /// A blank node, identified by `(label, scope)` (C0.2).
    Blank { label: &'a str, scope: BlankScope },
    /// A literal: borrowed lexical form, the (interned) datatype id, an optional
    /// borrowed language tag, and an optional base direction (C0.1).
    Literal {
        lexical: &'a str,
        datatype: TermId,
        language: Option<&'a str>,
        direction: Option<RdfTextDirection>,
    },
    /// A triple term (RDF 1.2 quoted triple), by its resolved component ids (C0.3).
    Triple { s: TermId, p: TermId, o: TermId },
}

/// A borrowed, resolved quad view: each position is a [`TermRef`] borrowing into the
/// dataset's term table. No allocation, no clone per quad.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct QuadRef<'a> {
    pub s: TermRef<'a>,
    pub p: TermRef<'a>,
    pub o: TermRef<'a>,
    pub g: Option<TermRef<'a>>,
}

/// The immutable, frozen RDF 1.2 dataset. Constructed only via
/// [`RdfDatasetBuilder::freeze`](super::builder::RdfDatasetBuilder::freeze).
///
/// All tables are boxed slices in deterministic, reproducible order; capability
/// flags are computed once at freeze.
#[derive(Debug)]
pub struct RdfDataset {
    /// The interned term table; addressed by [`TermId::index`].
    terms: Box<[InternedTerm]>,
    /// Deduplicated quad rows in deterministic order (C0.5).
    quads: Box<[QuadRow]>,
    /// `(reifier, triple-term)` bindings; many reifiers MAY bind one triple (C0.4).
    reifiers: Box<[(TermId, TermId)]>,
    /// `(reifier, predicate, object)` annotations, deduplicated (C0.5).
    annotations: Box<[(TermId, TermId, TermId)]>,
    /// Sparse source locations, sorted by handle for binary-search lookup.
    locations: Box<[(QuadHandle, RdfLocation)]>,
    /// Capability flags, computed ONCE at freeze.
    caps: RdfStoreCapabilities,
}

impl RdfDataset {
    /// Assemble a frozen dataset from already-validated, already-ordered parts.
    /// Crate-internal: only [`RdfDatasetBuilder::freeze`] calls this, after
    /// validation.
    ///
    /// [`RdfDatasetBuilder::freeze`]: super::builder::RdfDatasetBuilder::freeze
    pub(crate) fn from_parts(
        terms: Box<[InternedTerm]>,
        quads: Box<[QuadRow]>,
        reifiers: Box<[(TermId, TermId)]>,
        annotations: Box<[(TermId, TermId, TermId)]>,
        locations: Box<[(QuadHandle, RdfLocation)]>,
        caps: RdfStoreCapabilities,
    ) -> Self {
        Self {
            terms,
            quads,
            reifiers,
            annotations,
            locations,
            caps,
        }
    }

    /// Iterate quads as ID-native [`QuadIds`]. **Zero allocations, infallible, no
    /// clone**: each frozen [`QuadRow`] is mapped to a `Copy` [`QuadIds`] in place;
    /// the iterator is not boxed and yields no `Result`.
    #[inline]
    pub fn quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.quads.iter().copied().map(QuadIds::from)
    }

    /// Iterate quads as borrowed, resolved [`QuadRef`] views. Each term is resolved
    /// by borrowing into the term table — no allocation, no clone per quad.
    #[inline]
    pub fn quad_refs(&self) -> RdfDatasetIter<'_> {
        RdfDatasetIter {
            dataset: self,
            inner: self.quads.iter(),
        }
    }

    /// Resolve one frozen [`QuadRow`] to a borrowed [`QuadRef`] (no allocation).
    #[inline]
    fn quad_ref_of(&self, row: &QuadRow) -> QuadRef<'_> {
        QuadRef {
            s: self.resolve(row.s),
            p: self.resolve(row.p),
            o: self.resolve(row.o),
            g: row.g.map(|g| self.resolve(g)),
        }
    }

    /// Resolve a term id to a borrowed [`TermRef`]. No allocation: string content is
    /// borrowed directly from the term table.
    #[inline]
    pub fn resolve(&self, id: TermId) -> TermRef<'_> {
        match &self.terms[id.index()] {
            InternedTerm::Iri(iri) => TermRef::Iri(iri),
            InternedTerm::Blank { label, scope } => TermRef::Blank {
                label,
                scope: *scope,
            },
            InternedTerm::Literal(lit) => TermRef::Literal {
                lexical: &lit.lexical_form,
                datatype: lit.datatype,
                language: lit.language.as_deref(),
                direction: lit.direction,
            },
            InternedTerm::Triple { s, p, o } => TermRef::Triple {
                s: *s,
                p: *p,
                o: *o,
            },
        }
    }

    /// Iterate `(reifier, triple-term)` bindings. Zero allocation, infallible.
    #[inline]
    pub fn reifiers(&self) -> impl Iterator<Item = (TermId, TermId)> + '_ {
        self.reifiers.iter().copied()
    }

    /// Iterate `(reifier, predicate, object)` annotations. Zero allocation,
    /// infallible.
    #[inline]
    pub fn annotations(&self) -> impl Iterator<Item = (TermId, TermId, TermId)> + '_ {
        self.annotations.iter().copied()
    }

    /// The reifier resources bound to a triple term (C0.4). Several reifiers MAY
    /// bind one triple, so this yields zero or more — the single source for "who
    /// reifies this statement", used by the SARIF/annotation threading and validate
    /// lints instead of re-deriving it.
    ///
    /// A **linear** scan: the reifier table is sorted by `(reifier, triple)`, so the
    /// `triple` argument is the *secondary* key — entries for one triple are not
    /// contiguous and a binary search does not apply. The table is small (a few
    /// bindings per statement), so this is not a hot path.
    pub fn reifiers_of(&self, triple: TermId) -> impl Iterator<Item = TermId> + '_ {
        self.reifiers
            .iter()
            .filter(move |(_, t)| *t == triple)
            .map(|(r, _)| *r)
    }

    /// The `(predicate, object)` statement annotations attached to a reifier
    /// resource (RDF 1.2 annotation syntax) — the single source for a reified
    /// statement's annotation triples (e.g. confidence, provenance, x-gmeow tags).
    ///
    /// `O(log n)` to locate the run: annotations are frozen sorted by
    /// `(reifier, predicate, object)`, so all entries for one reifier are
    /// contiguous — `partition_point` finds the start, then a `take_while` walks the
    /// run.
    pub fn annotations_of(&self, reifier: TermId) -> impl Iterator<Item = (TermId, TermId)> + '_ {
        let start = self.annotations.partition_point(|(r, _, _)| *r < reifier);
        self.annotations[start..]
            .iter()
            .take_while(move |(r, _, _)| *r == reifier)
            .map(|(_, p, o)| (*p, *o))
    }

    /// The source location attached to a quad, if any. `O(log n)` binary search over
    /// the handle-sorted sparse table. The handle addresses the quad's FROZEN
    /// ordinal (the position it occupies in [`quads`](Self::quads)).
    pub fn location_of(&self, handle: QuadHandle) -> Option<&RdfLocation> {
        self.locations
            .binary_search_by_key(&handle, |(h, _)| *h)
            .ok()
            .map(|i| &self.locations[i].1)
    }

    /// The capability flags, computed once at freeze.
    #[inline]
    pub fn capabilities(&self) -> RdfStoreCapabilities {
        self.caps
    }

    /// The number of distinct interned terms.
    #[inline]
    pub fn term_count(&self) -> usize {
        self.terms.len()
    }

    /// The number of deduplicated quads.
    #[inline]
    pub fn quad_count(&self) -> usize {
        self.quads.len()
    }
}

/// A zero-allocation, zero-dynamic-dispatch iterator over an [`RdfDataset`]'s quads
/// as resolved [`QuadRef`]s. Yielded by [`RdfDataset::quad_refs`] and by
/// `for quad in &dataset`. Backed by a `core::slice::Iter` (no_std-ready), it is
/// `Double-ended`, `ExactSize`, and `Fused` — a drop-in for the standard iterator
/// adapters with no per-item heap cost.
pub struct RdfDatasetIter<'a> {
    dataset: &'a RdfDataset,
    inner: core::slice::Iter<'a, QuadRow>,
}

impl<'a> Iterator for RdfDatasetIter<'a> {
    type Item = QuadRef<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let dataset = self.dataset;
        self.inner.next().map(|row| dataset.quad_ref_of(row))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for RdfDatasetIter<'_> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        let dataset = self.dataset;
        self.inner.next_back().map(|row| dataset.quad_ref_of(row))
    }
}

impl ExactSizeIterator for RdfDatasetIter<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl core::iter::FusedIterator for RdfDatasetIter<'_> {}

/// `for quad in &dataset` yields each [`QuadRef`] (resolved, borrowed terms — no
/// per-quad allocation, no dynamic dispatch; see [`RdfDatasetIter`]).
impl<'a> IntoIterator for &'a RdfDataset {
    type Item = QuadRef<'a>;
    type IntoIter = RdfDatasetIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.quad_refs()
    }
}

// A frozen `RdfDataset` is an immutable, `Arc`-shared snapshot; it (and the `Copy`
// `TermId` that indexes it) are `Send + Sync` so consumers can fan reasoning/
// serialization across threads. These guards fail the build if that ever regresses.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RdfDataset>();
    assert_send_sync::<TermId>();
    assert_send_sync::<QuadIds>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RdfDatasetBuilder;
    use crate::RdfLiteral;

    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(format!("http://example.org/{n}"))
    }

    #[test]
    fn extend_with_interned_ids_and_into_iterator() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p) = (iri(&mut b, "s"), iri(&mut b, "p"));
        let (o1, o2) = (iri(&mut b, "o1"), iri(&mut b, "o2"));
        // Extend<QuadIds>: bulk-push ids interned in THIS builder (#841).
        b.extend([
            QuadIds {
                s,
                p,
                o: o1,
                g: None,
            },
            QuadIds {
                s,
                p,
                o: o2,
                g: None,
            },
        ]);
        let ds = b.freeze().expect("freeze");
        assert_eq!(ds.quad_count(), 2);
        // IntoIterator for &RdfDataset yields one QuadRef per quad.
        assert_eq!((&*ds).into_iter().count(), 2);
        // The named iterator is ExactSize, DoubleEnded, and Fused (#841).
        let mut it = ds.quad_refs();
        assert_eq!(it.len(), 2);
        assert!(it.next_back().is_some());
        assert_eq!(it.len(), 1);
        assert!(it.next().is_some());
        assert!(it.next().is_none());
        assert!(it.next().is_none(), "fused: stays exhausted");
    }

    #[test]
    fn extend_empty_and_dedup() {
        // Empty extend yields an empty dataset.
        let mut b = RdfDatasetBuilder::new();
        b.extend(core::iter::empty::<QuadIds>());
        assert_eq!(b.freeze().expect("freeze").quad_count(), 0);
        // Duplicate quads collapse — Extend routes through push_quad's dedup.
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        let q = QuadIds { s, p, o, g: None };
        b.extend([q, q]);
        assert_eq!(b.freeze().expect("freeze").quad_count(), 1);
    }

    #[test]
    fn resolve_round_trips_iri() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        b.push_quad(s, p, o, None);
        let ds = b.freeze().expect("valid");
        match ds.resolve(s) {
            TermRef::Iri(v) => assert_eq!(v, "http://example.org/s"),
            other => panic!("expected iri, got {other:?}"),
        }
    }

    #[test]
    fn resolve_round_trips_literal_content() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let lit = b.intern_literal(RdfLiteral::language_tagged("Bonjour", "FR"));
        b.push_quad(s, p, lit, None);
        let ds = b.freeze().expect("valid");
        match ds.resolve(lit) {
            TermRef::Literal {
                lexical, language, ..
            } => {
                assert_eq!(lexical, "Bonjour", "lexical preserved verbatim");
                assert_eq!(language, Some("fr"), "language lowercased per C0.1");
            }
            other => panic!("expected literal, got {other:?}"),
        }
    }

    #[test]
    fn location_lookup_is_sparse_and_binary_searchable() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o0 = iri(&mut b, "o0");
        let o1 = iri(&mut b, "o1");
        let o2 = iri(&mut b, "o2");

        let h0 = b.next_quad_handle();
        b.push_quad(s, p, o0, None);
        // No location for the middle quad.
        b.push_quad(s, p, o1, None);
        let h2 = b.next_quad_handle();
        b.push_quad(s, p, o2, None);

        b.attach_location(h0, RdfLocation::logical("first"));
        b.attach_location(h2, RdfLocation::logical("third"));

        let ds = b.freeze().expect("valid");
        assert_eq!(
            ds.location_of(h0).map(|l| l.logical.as_deref().unwrap()),
            Some("first")
        );
        assert_eq!(
            ds.location_of(h2).map(|l| l.logical.as_deref().unwrap()),
            Some("third")
        );
        // The middle quad has no location.
        assert!(ds.location_of(QuadHandle::from_index(1)).is_none());
    }

    #[test]
    fn location_follows_quad_through_freeze_sort() {
        // Push quads in an order that does NOT match the frozen sort order, attach a
        // location to one of them, and assert the location follows that quad to its
        // post-sort position. This is the handle/sort remap — an LSP correctness
        // guard: before the remap, `location_of` returned a *different* quad's
        // location once the sort reordered the rows.
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o0 = iri(&mut b, "o0");
        let o1 = iri(&mut b, "o1");
        let o2 = iri(&mut b, "o2");

        // Push in DESCENDING object order; the frozen order is ascending, so push
        // order and frozen order genuinely differ.
        let h_o2 = b.next_quad_handle();
        b.push_quad(s, p, o2, None);
        b.push_quad(s, p, o1, None);
        b.push_quad(s, p, o0, None);
        b.attach_location(h_o2, RdfLocation::logical("loc-o2"));

        let ds = b.freeze().expect("valid");
        let frozen_o2 = ds.quads().position(|q| q.o == o2).expect("o2 present");
        assert_eq!(
            ds.location_of(QuadHandle::from_index(frozen_o2 as u32))
                .and_then(|l| l.logical.as_deref()),
            Some("loc-o2"),
            "location must follow the o2 quad to its frozen position"
        );
        // The o0 quad (which sorts first) carries no location.
        let frozen_o0 = ds.quads().position(|q| q.o == o0).unwrap();
        assert!(ds
            .location_of(QuadHandle::from_index(frozen_o0 as u32))
            .is_none());
    }

    #[test]
    fn reifiers_of_and_annotations_of() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        let triple = b.intern_triple(s, p, o);
        let r1 = iri(&mut b, "r1");
        let r2 = iri(&mut b, "r2");
        let ap = iri(&mut b, "ap");
        let ao = iri(&mut b, "ao");
        b.push_reifier(r1, triple);
        b.push_reifier(r2, triple);
        b.push_annotation(r1, ap, ao);
        let ds = b.freeze().expect("valid");

        let reifiers: std::collections::BTreeSet<_> = ds.reifiers_of(triple).collect();
        assert_eq!(reifiers, [r1, r2].into_iter().collect());
        let anns: Vec<_> = ds.annotations_of(r1).collect();
        assert_eq!(anns, vec![(ap, ao)]);
        assert_eq!(ds.annotations_of(r2).count(), 0);
    }

    #[test]
    fn quad_ids_match_pushed_quads() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        let g = iri(&mut b, "g");
        b.push_quad(s, p, o, Some(g));
        let ds = b.freeze().expect("valid");
        let q = ds.quads().next().expect("one quad");
        assert_eq!(
            q,
            QuadIds {
                s,
                p,
                o,
                g: Some(g)
            }
        );
    }

    use proptest::prelude::*;

    proptest! {
        /// Build → freeze a random *valid* dataset (IRI subjects/predicates/objects
        /// over a small pool, with optional named graphs), then assert:
        /// - `quads().count()` equals the number of DISTINCT quads pushed (C0.5);
        /// - every yielded `TermId` is in range (`< term_count()`).
        #[test]
        fn proptest_freeze_quads_count_and_in_range(
            rows in prop::collection::vec(
                (0u8..5, 0u8..5, 0u8..5, prop::option::of(0u8..3)),
                0..48,
            )
        ) {
            use std::collections::HashSet;

            let mut b = RdfDatasetBuilder::new();
            // Intern a fixed pool of IRIs once so positional constraints always hold.
            let pool: Vec<TermId> = (0..5)
                .map(|n| b.intern_iri(format!("http://example.org/n{n}")))
                .collect();
            let graphs: Vec<TermId> = (0..3)
                .map(|n| b.intern_iri(format!("http://example.org/g{n}")))
                .collect();

            let mut distinct: HashSet<(TermId, TermId, TermId, Option<TermId>)> = HashSet::new();
            for (s, p, o, g) in rows {
                let s = pool[s as usize];
                let p = pool[p as usize];
                let o = pool[o as usize];
                let g = g.map(|gi| graphs[gi as usize]);
                b.push_quad(s, p, o, g);
                distinct.insert((s, p, o, g));
            }

            let term_count = b.term_count();
            let ds = b.freeze().expect("random valid dataset must freeze");
            prop_assert_eq!(ds.quads().count(), distinct.len());

            for q in ds.quads() {
                prop_assert!(q.s.index() < term_count);
                prop_assert!(q.p.index() < term_count);
                prop_assert!(q.o.index() < term_count);
                if let Some(g) = q.g {
                    prop_assert!(g.index() < term_count);
                }
            }
        }
    }
}
