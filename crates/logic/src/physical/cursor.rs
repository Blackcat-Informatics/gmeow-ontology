// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The arrangement's native lending cursor: a zero-allocation [`LendingIterator`] over
//! a relation's shared arrangement (a log of sorted immutable batches plus a mutable
//! tail).
//!
//! # What this yields, and in what order
//!
//! The per-atom scan of the semi-naive join never materializes a
//! `Vec<(TermId, TermId, RowId)>`.  A [`RowCursor`] yields the `(subject_id, object_id,
//! row_id)` id rows selected by a [`super::store::Bound`] one at a time, borrowing the
//! relation's columns.  It CONCATENATES each batch's bound-run — located by GALLOPING
//! the sorted `(subject_id, object_id)` columns (subject-bound: gallop the `subj`
//! column to the term's contiguous run; object-bound: the lazily-built
//! `(object, subject)` permutation) — followed by a linear scan of the small tail.
//!
//! # Order-freedom (why batch-then-tail is byte-identical)
//!
//! Enumeration is batch-then-tail, NOT a global merge sort across runs.  This is sound
//! because per-round winner selection is a TOTAL order over provenance
//! ([`crate::rule_ir::RuleRoundCandidate::tiebreak_key`]): two candidates that would
//! produce different output bytes differ in the tiebreak key, so the winner is chosen
//! independently of the order in which the cursor enumerates its rows.  The cursor
//! therefore never introduces an observable ordering — the storage sort is an internal
//! concern only.
//!
//! # Galloping is the primitive
//!
//! Run location is a galloping lower-bound over the sorted columns (exponential probe
//! then binary search — never a linear scan, never a hash probe), the exact primitive a
//! future multiway-leapfrog / WCOJ lever composes.  The `(subject_id, object_id)` key is
//! unique per relation, so a `Both` bound yields at most one row across the whole
//! arrangement.

use super::id::{RowId, TermId};
use super::store::{Batch, Bound, Relation};

/// The sealing supertrait, mirroring `purrdf`'s `DatasetView` discipline
/// (`mod sealed { pub trait Sealed {} }` + a private-supertrait bound): only types
/// in THIS module can implement [`LendingIterator`], so the cursor contract cannot
/// be re-implemented (or its `next` invariant weakened) from outside.
mod sealed {
    /// The private supertrait; unreachable outside `physical::cursor`.
    pub trait Sealed {}

    impl Sealed for super::RowCursor<'_> {}
}

/// A GAT lending iterator over a relation's id rows: it yields borrowed views tied
/// to the `&mut self` borrow (`Item<'a>`), so a driver never collects a `Vec`.
///
/// Sealed via [`sealed::Sealed`] (only [`RowCursor`] implements it).  The single method
/// is the whole contract a future WCOJ lever composes: [`next`](Self::next)
/// (advance-and-yield over the galloped runs).
pub(crate) trait LendingIterator: sealed::Sealed {
    /// The lent item — an `(subject_id, object_id, row_id)` id row.  All three are
    /// `Copy` niche integers, so the item borrows nothing beyond the cursor's own
    /// `&mut` reborrow (the GAT lifetime), and yielding it copies rather than clones
    /// a `TermValue`.
    type Item<'a>
    where
        Self: 'a;

    /// Yield the id row at the cursor and advance past it, or `None` at the end.
    fn next(&mut self) -> Option<Self::Item<'_>>;
}

/// The within-source iteration state for the current arrangement leg.
enum Inner<'a> {
    /// A contiguous column-position range `[pos, end)` of the current batch — an `Any`
    /// full scan (`0..len`) or a subject-bound run.
    Range { pos: usize, end: usize },
    /// The current batch's object permutation subslice; `pos` indexes into it and each
    /// entry is a column position.
    Perm { perm: &'a [u32], pos: usize },
    /// At most one column position (a `Both` bound over the unique `(s, o)` key).
    One(Option<usize>),
    /// A linear scan of the relation's tail from index `pos`, filtered by the bound.
    Tail { pos: usize },
    /// No more rows.
    Done,
}

/// A lending cursor over one relation's shared arrangement, in batch-then-tail order.
///
/// Borrows the relation for the cursor's lifetime; every yielded row is resolved through
/// [`Batch::row_at`] or the tail, so nothing is cloned or allocated per row.  Built by
/// [`RowCursor::new`] from a [`Bound`]; the bound shape drives the per-source run.
pub(crate) struct RowCursor<'a> {
    rel: &'a Relation,
    bound: Bound,
    /// The current arrangement leg: `0..batches.len()` selects that batch, an index
    /// equal to `batches.len()` selects the tail, and anything greater is exhausted.
    src: usize,
    inner: Inner<'a>,
}

impl<'a> RowCursor<'a> {
    /// A cursor over the rows of `rel` selected by `bound`, positioned at the first
    /// source (batch 0, or the tail, or exhausted).
    pub(crate) fn new(rel: &'a Relation, bound: Bound) -> Self {
        let mut cursor = Self {
            rel,
            bound,
            src: 0,
            inner: Inner::Done,
        };
        cursor.enter(0);
        cursor
    }

    /// Position the cursor at source `src`, computing its bound-run.  A batch source
    /// gallops the sorted columns for the run; the tail source is a linear scan; past
    /// the tail is [`Inner::Done`].
    fn enter(&mut self, src: usize) {
        self.src = src;
        let batches = self.rel.batches();
        self.inner = if src < batches.len() {
            let b: &Batch = &batches[src];
            match self.bound {
                Bound::Any => Inner::Range {
                    pos: 0,
                    end: b.len(),
                },
                Bound::Subject(s) => {
                    let (lo, hi) = b.subject_run(s);
                    Inner::Range { pos: lo, end: hi }
                }
                Bound::Object(o) => Inner::Perm {
                    perm: b.object_positions(o),
                    pos: 0,
                },
                Bound::Both(s, o) => Inner::One(b.both_pos(s, o)),
            }
        } else if src == batches.len() {
            Inner::Tail { pos: 0 }
        } else {
            Inner::Done
        };
    }

    /// Whether the tail row `(s, o)` satisfies `bound`.
    #[inline]
    fn tail_matches(bound: Bound, s: TermId, o: TermId) -> bool {
        match bound {
            Bound::Any => true,
            Bound::Subject(bs) => s == bs,
            Bound::Object(bo) => o == bo,
            Bound::Both(bs, bo) => s == bs && o == bo,
        }
    }

    /// Whether the cursor has at least one remaining row — the allocation-free
    /// membership probe used by existential NAF (no `Vec` materialized just to ask
    /// `!is_empty()`).
    pub(crate) fn any_remaining(mut self) -> bool {
        self.next().is_some()
    }
}

impl LendingIterator for RowCursor<'_> {
    type Item<'a>
        = (TermId, TermId, RowId)
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        let rel = self.rel;
        let bound = self.bound;
        loop {
            match &mut self.inner {
                Inner::Range { pos, end } => {
                    if *pos < *end {
                        let p = *pos;
                        *pos += 1;
                        return Some(rel.batches()[self.src].row_at(p));
                    }
                }
                Inner::Perm { perm, pos } => {
                    if *pos < perm.len() {
                        let p = perm[*pos] as usize;
                        *pos += 1;
                        return Some(rel.batches()[self.src].row_at(p));
                    }
                }
                Inner::One(slot) => {
                    if let Some(p) = slot.take() {
                        return Some(rel.batches()[self.src].row_at(p));
                    }
                }
                Inner::Tail { pos } => {
                    let tail = rel.tail();
                    while *pos < tail.len() {
                        let (s, o, r) = tail[*pos];
                        *pos += 1;
                        if RowCursor::tail_matches(bound, s, o) {
                            return Some((s, o, r));
                        }
                    }
                }
                Inner::Done => return None,
            }
            // The current source is exhausted; advance to the next.
            let next_src = self.src + 1;
            self.enter(next_src);
        }
    }
}

// ── Value-ordered trie cursor (WCOJ substrate) ─────────────────────────────────

/// Const-generic trie adornments. The column choice is resolved at monomorphization,
/// never re-branched per tuple.
pub(crate) const VALUE_SUBJECT: u8 = 0;
pub(crate) const VALUE_OBJECT: u8 = 1;

/// One sorted arrangement run viewed in the requested trie orientation.
enum OrderedSource<'a, const COLUMN: u8> {
    /// A contiguous primary-column range: whole subject-major batch, or one
    /// subject-bound object run.
    Range {
        batch: &'a Batch,
        pos: usize,
        end: usize,
    },
    /// A secondary-order permutation: whole object-major batch, or one object-bound
    /// subject run.
    Perm {
        batch: &'a Batch,
        positions: &'a [u32],
        pos: usize,
    },
    /// The mutable tail is bounded below the seal threshold. Its matching positions
    /// are sorted once per cursor, never copied as rows.
    Tail {
        tail: &'a [(TermId, TermId, RowId)],
        positions: Box<[u32]>,
        pos: usize,
    },
}

impl<const COLUMN: u8> OrderedSource<'_, COLUMN> {
    #[inline(always)]
    fn project(subject: TermId, object: TermId) -> TermId {
        match COLUMN {
            VALUE_SUBJECT => subject,
            VALUE_OBJECT => object,
            _ => unreachable!("COLUMN is VALUE_SUBJECT or VALUE_OBJECT"),
        }
    }

    #[inline(always)]
    fn other_matches(other: TermId, subject: TermId, object: TermId) -> bool {
        match COLUMN {
            VALUE_SUBJECT => object == other,
            VALUE_OBJECT => subject == other,
            _ => unreachable!("COLUMN is VALUE_SUBJECT or VALUE_OBJECT"),
        }
    }

    #[inline]
    fn batch_row(batch: &Batch, position: usize) -> (TermId, RowId) {
        let (subject, object, row) = batch.row_at(position);
        (Self::project(subject, object), row)
    }

    #[inline]
    fn tail_row(tail: &[(TermId, TermId, RowId)], position: usize) -> (TermId, RowId) {
        let (subject, object, row) = tail[position];
        (Self::project(subject, object), row)
    }

    fn peek(&self) -> Option<(TermId, RowId)> {
        match self {
            Self::Range { batch, pos, end } => (*pos < *end).then(|| Self::batch_row(batch, *pos)),
            Self::Perm {
                batch,
                positions,
                pos,
            } => positions
                .get(*pos)
                .map(|&position| Self::batch_row(batch, position as usize)),
            Self::Tail {
                tail,
                positions,
                pos,
            } => positions
                .get(*pos)
                .map(|&position| Self::tail_row(tail, position as usize)),
        }
    }

    fn pop(&mut self) -> Option<(TermId, RowId)> {
        let row = self.peek()?;
        match self {
            Self::Range { pos, .. } | Self::Perm { pos, .. } | Self::Tail { pos, .. } => {
                *pos += 1;
            }
        }
        Some(row)
    }

    /// Seek this sorted run to its first projected value `>= target`.
    fn seek(&mut self, target: TermId) {
        match self {
            Self::Range { batch, pos, end } => {
                let (mut lo, mut hi) = (*pos, *end);
                while lo < hi {
                    let mid = lo + (hi - lo) / 2;
                    if Self::batch_row(batch, mid).0 < target {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                *pos = lo;
            }
            Self::Perm {
                batch,
                positions,
                pos,
            } => {
                let offset = positions[*pos..].partition_point(|&position| {
                    Self::batch_row(batch, position as usize).0 < target
                });
                *pos += offset;
            }
            Self::Tail {
                tail,
                positions,
                pos,
            } => {
                let offset = positions[*pos..].partition_point(|&position| {
                    Self::tail_row(tail, position as usize).0 < target
                });
                *pos += offset;
            }
        }
    }
}

/// A globally value-ordered cursor over one relation trie level.
///
/// Immutable batches already carry the two required orders: `(subject, object)` in
/// their primary columns and `(object, subject)` in the lazy permutation. This cursor
/// k-way merges those sorted runs plus the at-most-63-row tail. It yields projected
/// `(TermId, RowId)` pairs without materializing or sorting relation rows, and supports
/// the seek operation used by leapfrog intersection.
pub(crate) struct ValueCursor<'a, const COLUMN: u8> {
    sources: Vec<OrderedSource<'a, COLUMN>>,
}

impl<'a, const COLUMN: u8> ValueCursor<'a, COLUMN> {
    pub(crate) fn new(rel: &'a Relation, other: Option<TermId>) -> Self {
        let mut sources = Vec::with_capacity(rel.batches().len() + 1);
        for batch in rel.batches() {
            let source = match (COLUMN, other) {
                (VALUE_SUBJECT, None) => OrderedSource::Range {
                    batch,
                    pos: 0,
                    end: batch.len(),
                },
                (VALUE_SUBJECT, Some(object)) => OrderedSource::Perm {
                    batch,
                    positions: batch.object_positions(object),
                    pos: 0,
                },
                (VALUE_OBJECT, None) => OrderedSource::Perm {
                    batch,
                    positions: batch.object_order(),
                    pos: 0,
                },
                (VALUE_OBJECT, Some(subject)) => {
                    let (lo, hi) = batch.subject_run(subject);
                    OrderedSource::Range {
                        batch,
                        pos: lo,
                        end: hi,
                    }
                }
                _ => unreachable!("COLUMN is VALUE_SUBJECT or VALUE_OBJECT"),
            };
            if source.peek().is_some() {
                sources.push(source);
            }
        }

        if !rel.tail().is_empty() {
            let mut positions: Vec<u32> = rel
                .tail()
                .iter()
                .enumerate()
                .filter_map(|(position, &(subject, object, _))| {
                    other
                        .is_none_or(|bound| Self::other_matches(bound, subject, object))
                        .then_some(position as u32)
                })
                .collect();
            positions.sort_unstable_by_key(|&position| {
                let (subject, object, row) = rel.tail()[position as usize];
                (Self::project(subject, object), row)
            });
            if !positions.is_empty() {
                sources.push(OrderedSource::Tail {
                    tail: rel.tail(),
                    positions: positions.into_boxed_slice(),
                    pos: 0,
                });
            }
        }

        Self { sources }
    }

    #[inline(always)]
    fn project(subject: TermId, object: TermId) -> TermId {
        OrderedSource::<COLUMN>::project(subject, object)
    }

    #[inline(always)]
    fn other_matches(other: TermId, subject: TermId, object: TermId) -> bool {
        OrderedSource::<COLUMN>::other_matches(other, subject, object)
    }

    /// Seek every sorted run, positioning the merged cursor at its first value
    /// `>= target`.
    pub(crate) fn seek(&mut self, target: TermId) {
        for source in &mut self.sources {
            source.seek(target);
        }
    }

    /// Yield the globally-smallest remaining `(value, row)` pair and advance its run.
    pub(crate) fn next(&mut self) -> Option<(TermId, RowId)> {
        let mut choice: Option<(usize, (TermId, RowId))> = None;
        for (index, source) in self.sources.iter().enumerate() {
            let Some(row) = source.peek() else {
                continue;
            };
            if choice.is_none_or(|(_, current)| row < current) {
                choice = Some((index, row));
            }
        }
        let (index, _) = choice?;
        self.sources[index].pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::store::{Bound, RelationStore};
    use purrdf::TermValue;

    fn term(iri: &str) -> TermValue {
        TermValue::iri(iri)
    }

    /// Drain a cursor into a `Vec` — a `#[cfg(test)]`-only convenience for asserting a
    /// cursor's full row set (the production hot path never collects).
    fn drain(mut c: RowCursor<'_>) -> Vec<(TermId, TermId, RowId)> {
        let mut out = Vec::new();
        while let Some(row) = c.next() {
            out.push(row);
        }
        out
    }

    fn drain_values<const COLUMN: u8>(mut c: ValueCursor<'_, COLUMN>) -> Vec<(TermId, RowId)> {
        let mut out = Vec::new();
        while let Some(row) = c.next() {
            out.push(row);
        }
        out
    }

    /// Resolve selected id rows to `(subject, object)` display-surface pairs, as an
    /// ORDER-INDEPENDENT set (the cursor enumerates batch-then-tail, not sorted, so tests
    /// compare sets — winner selection, not cursor order, fixes output).
    fn resolved_set(
        s: &RelationStore,
        rows: &[(TermId, TermId, RowId)],
    ) -> std::collections::BTreeSet<(String, String)> {
        rows.iter()
            .map(|&(si, oi, _)| {
                (
                    format!("{:?}", s.interner().resolve(si)),
                    format!("{:?}", s.interner().resolve(oi)),
                )
            })
            .collect()
    }

    fn pair(sub: &str, obj: &str) -> (String, String) {
        (format!("{:?}", term(sub)), format!("{:?}", term(obj)))
    }

    /// A store large enough to force several batch seals (past `TAIL_SEAL_THRESHOLD`),
    /// so the galloping batch runs — not just the tail leg — are exercised.  `p` holds
    /// `(a, o_i)` for many objects plus a second subject `z` with one edge.
    fn big_store() -> RelationStore {
        let mut s = RelationStore::new();
        for i in 0..200 {
            assert!(
                s.insert(
                    "http://ex/p",
                    &term("http://ex/a"),
                    &term(&format!("http://ex/o{i:03}")),
                )
                .is_some()
            );
        }
        assert!(
            s.insert("http://ex/p", &term("http://ex/z"), &term("http://ex/o000"))
                .is_some()
        );
        s
    }

    #[test]
    fn cursor_any_yields_every_row_as_a_set() {
        let s = big_store();
        let got = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Any)));
        assert_eq!(got.len(), 201, "200 a-edges + 1 z-edge, deduped");
        assert!(got.contains(&pair("http://ex/a", "http://ex/o000")));
        assert!(got.contains(&pair("http://ex/z", "http://ex/o000")));
        assert!(got.contains(&pair("http://ex/a", "http://ex/o199")));
    }

    #[test]
    fn cursor_subject_bound_gallops_batches() {
        let s = big_store();
        let a = s.term_id("<http://ex/a>").expect("a interned");
        let got = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Subject(a))));
        assert_eq!(got.len(), 200, "exactly a's 200 edges");
        assert!(
            got.iter()
                .all(|(sub, _)| *sub == pair("http://ex/a", "x").0)
        );

        let z = s.term_id("<http://ex/z>").expect("z interned");
        let zrows = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Subject(z))));
        assert_eq!(zrows, [pair("http://ex/z", "http://ex/o000")].into());
    }

    #[test]
    fn cursor_object_bound_uses_lazy_permutation() {
        let s = big_store();
        let o0 = s.term_id("<http://ex/o000>").expect("o000 interned");
        // o000 is the object of BOTH a and z.
        let got = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Object(o0))));
        assert_eq!(
            got,
            [
                pair("http://ex/a", "http://ex/o000"),
                pair("http://ex/z", "http://ex/o000"),
            ]
            .into()
        );
        // A distinct object appears once.
        let o5 = s.term_id("<http://ex/o005>").expect("o005 interned");
        let g5 = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Object(o5))));
        assert_eq!(g5, [pair("http://ex/a", "http://ex/o005")].into());
    }

    #[test]
    fn cursor_both_bound_is_unique() {
        let s = big_store();
        let a = s.term_id("<http://ex/a>").expect("a interned");
        let o7 = s.term_id("<http://ex/o007>").expect("o007 interned");
        assert_eq!(drain(s.select("http://ex/p", Bound::Both(a, o7))).len(), 1);
        // A subject/object that never co-occur ⇒ empty.
        let z = s.term_id("<http://ex/z>").expect("z interned");
        let o7b = s.term_id("<http://ex/o007>").expect("o007 interned");
        assert!(
            drain(s.select("http://ex/p", Bound::Both(z, o7b))).is_empty(),
            "z only links o000, never o007"
        );
    }

    #[test]
    fn cursor_tail_only_small_relation() {
        // A relation below the seal threshold is a pure tail (no batches) — the
        // allocation-light regime — and still selects correctly on every bound.
        let mut s = RelationStore::new();
        for (sub, obj) in [("a", "b"), ("a", "c"), ("b", "c")] {
            assert!(
                s.insert(
                    "http://ex/k",
                    &term(&format!("http://ex/{sub}")),
                    &term(&format!("http://ex/{obj}")),
                )
                .is_some()
            );
        }
        let a = s.term_id("<http://ex/a>").expect("a interned");
        let got = resolved_set(&s, &drain(s.select("http://ex/k", Bound::Subject(a))));
        assert_eq!(
            got,
            [
                pair("http://ex/a", "http://ex/b"),
                pair("http://ex/a", "http://ex/c"),
            ]
            .into()
        );
        assert!(s.contains("http://ex/k", "<http://ex/b>", "<http://ex/c>"));
        assert!(!s.contains("http://ex/k", "<http://ex/a>", "<http://ex/z>"));
    }

    #[test]
    fn cursor_any_remaining_probes_without_collecting() {
        let s = big_store();
        let a = s.term_id("<http://ex/a>").expect("a interned");
        assert!(s.select("http://ex/p", Bound::Subject(a)).any_remaining());
        let missing = s.term_id("<http://ex/a>").expect("a interned");
        let none_obj = s.term_id("<http://ex/o000>").expect("o000 interned");
        // (a, o000) exists.
        assert!(
            s.select("http://ex/p", Bound::Both(missing, none_obj))
                .any_remaining()
        );
    }

    /// The trie cursor globally merges several immutable batches plus the tail in
    /// either orientation, and fixing the opposite column narrows the sorted stream.
    #[test]
    fn value_cursor_is_globally_sorted_in_both_orientations() {
        let s = big_store();

        let subject_rows = drain_values(s.values_subject("http://ex/p", None));
        assert_eq!(subject_rows.len(), 201);
        assert!(subject_rows.windows(2).all(|rows| rows[0].0 <= rows[1].0));

        let object_rows = drain_values(s.values_object("http://ex/p", None));
        assert_eq!(object_rows.len(), 201);
        assert!(object_rows.windows(2).all(|rows| rows[0].0 <= rows[1].0));

        let o0 = s.term_id("<http://ex/o000>").expect("o000 interned");
        let subjects_at_o0 = drain_values(s.values_subject("http://ex/p", Some(o0)));
        assert_eq!(subjects_at_o0.len(), 2, "a and z point at o000");
        assert!(subjects_at_o0.windows(2).all(|rows| rows[0].0 <= rows[1].0));

        let a = s.term_id("<http://ex/a>").expect("a interned");
        let objects_at_a = drain_values(s.values_object("http://ex/p", Some(a)));
        assert_eq!(objects_at_a.len(), 200);
        assert!(objects_at_a.windows(2).all(|rows| rows[0].0 <= rows[1].0));
    }

    /// Seek applies to every sorted run before the k-way merge, so no value below the
    /// requested trie frontier can reappear from an older batch or the mutable tail.
    #[test]
    fn value_cursor_seek_advances_all_runs() {
        let s = big_store();
        let target = s.term_id("<http://ex/o150>").expect("o150 interned");
        let mut cursor = s.values_object("http://ex/p", None);
        cursor.seek(target);
        let rows = drain_values(cursor);
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|&(value, _)| value >= target));
        assert_eq!(rows[0].0, target);
    }
}
