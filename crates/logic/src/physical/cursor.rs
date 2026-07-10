// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The arrangement's native lending cursor: a galloping [`LendingIterator`] over
//! **row-id-ordered** runs (issue 1418, item 6).
//!
//! # What this replaces
//!
//! The per-atom scan of the semi-naive join formerly materialized a
//! `Vec<(TermId, TermId, RowId)>` for every partial solution
//! ([`super::store::RelationStore::select`]) and iterated it.  A [`RowCursor`]
//! yields the same `(subject_id, object_id, row_id)` id rows one at a time,
//! borrowing the relation's columns — **no per-stage `Vec` allocation** on the hot
//! join path (greenfield: the eager-`Vec` `select_*` kernels are deleted).
//!
//! # `seek` is the primitive, and it is over row INDICES
//!
//! The cursor's core primitive is [`LendingIterator::seek`]: a **galloping** search
//! (exponential probing to bracket the target, then binary search within the
//! bracket — never a linear scan) that advances the cursor to the first posting
//! whose **relation-local row index** is `>= target`.  `seek` operates on `usize`
//! row-index POSITIONS — the values already held (ascending) in a relation's
//! `by_subject` / `by_object` buckets, and the implicit `0..rows.len()` of a full
//! scan.  It is emphatically NOT a seek to "the row whose term equals X": the
//! cursor never compares `TermId` *values*, only row-index positions, so it can
//! never introduce a key-sorted (lexical) ordering.  This is the exact primitive
//! issue-1306's leapfrog-triejoin / WCOJ lever reuses (multiway leapfrog needs only
//! `seek` + `next`), so it is built here even though nothing in THIS PR calls
//! [`LendingIterator::seek`] on the scan path directly — [`RowCursor::select_both`]
//! drives it internally for the two-cursor intersection.
//!
//! # Byte-identity boundary (critical — issue 1418 Enhancement Audit #11)
//!
//! Value runs stay **row-id-ordered**.  A relation's row indices are appended in
//! store-global insertion order, and (within one relation) `row_ids[idx]` is
//! strictly increasing in `idx`, so "row-index order" and "store-global `RowId`
//! order" COINCIDE — see [`super::store`]'s invariant test.  The leading unbound
//! full scan therefore iterates **row-id order** (`0, 1, 2, …`), byte-identically
//! to the former `(0..rows.len()).map(..)` scan.  This module introduces NO global
//! key-sorted arrangement (that is a separate, explicitly declined future scope);
//! galloping is over ascending row-index positions only.

use super::id::{RowId, TermId};
use super::store::Relation;

/// The sealing supertrait, mirroring `purrdf`'s `DatasetView` discipline
/// (`mod sealed { pub trait Sealed {} }` + a private-supertrait bound): only types
/// in THIS module can implement [`LendingIterator`], so the cursor contract cannot
/// be re-implemented (or its `seek`/`next` invariants weakened) from outside.
mod sealed {
    /// The private supertrait; unreachable outside `physical::cursor`.
    pub trait Sealed {}

    impl Sealed for super::RowCursor<'_> {}
}

/// A GAT lending iterator over a relation's id rows: it yields borrowed views tied
/// to the `&mut self` borrow (`Item<'a>`), so a driver never collects a `Vec`.
///
/// Sealed via [`sealed::Sealed`] (only [`RowCursor`] implements it).  The two
/// methods are the whole contract issue-1306's WCOJ lever composes:
/// [`next`](Self::next) (advance-and-yield) and [`seek`](Self::seek) (gallop to a
/// row-index frontier).
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

    /// **Galloping** seek: advance the cursor to the first posting whose
    /// relation-local **row index** is `>= target`, returning whether the cursor is
    /// then positioned on a live posting (i.e. NOT exhausted).
    ///
    /// The search is exponential-probe-then-binary-search (never linear): from the
    /// current position it doubles the stride (1, 2, 4, 8, …) until it brackets a
    /// posting whose row index is `>= target` (or runs off the end), then binary
    /// searches within that bracket.  `target` is a row-INDEX position, never a
    /// `TermId` value — the cursor never orders by term surface.
    fn seek(&mut self, target: usize) -> bool;
}

/// One ascending run of relation-local row indices the cursor gallops over.
///
/// Both variants present the SAME abstract sequence — strictly ascending `usize`
/// row indices — so [`gallop`](Self::gallop) is uniform.  The `Range` form is the
/// full scan's implicit `0, 1, …, len-1` (value at position `p` IS `p`); the
/// `Bucket` form is a `by_subject` / `by_object` posting slice (value at position
/// `p` is `slice[p]`), which is already ascending because rows are appended in
/// insertion order.
#[derive(Clone, Copy)]
enum Postings<'a> {
    /// The full scan's implicit `0..len`; the row index at position `p` is `p`.
    Range { len: usize },
    /// A secondary-index bucket: ascending relation-local row indices.
    Bucket(&'a [usize]),
}

impl Postings<'_> {
    /// The number of postings in the run.
    #[inline]
    fn len(self) -> usize {
        match self {
            Self::Range { len } => len,
            Self::Bucket(b) => b.len(),
        }
    }

    /// The relation-local row index at posting position `p` (`p < len`).
    #[inline]
    fn value_at(self, p: usize) -> usize {
        match self {
            Self::Range { .. } => p,
            Self::Bucket(b) => b[p],
        }
    }

    /// The smallest position `>= from` whose row-index value is `>= target`, found
    /// by galloping (exponential probe then binary search); `len` if none.
    ///
    /// The run is strictly ascending, so this is a well-defined lower-bound.  It
    /// never scans linearly: it doubles the stride to bracket the target, then
    /// binary-searches the bracket — the primitive a future multiway-leapfrog
    /// triejoin would reuse.
    fn gallop(self, from: usize, target: usize) -> usize {
        let len = self.len();
        if from >= len {
            return len;
        }
        // Already at-or-past the target ⇒ the current position is the answer.
        if self.value_at(from) >= target {
            return from;
        }
        // Exponential probe: keep the invariant `value_at(lo) < target`, growing the
        // stride until `hi` brackets a posting `>= target` (or runs off the end).
        let mut lo = from;
        let mut step = 1usize;
        let hi = loop {
            let probe = lo.saturating_add(step);
            if probe >= len {
                break len;
            }
            if self.value_at(probe) >= target {
                break probe;
            }
            lo = probe;
            step = step.saturating_mul(2);
        };
        // The first position `>= target` lies in `(lo, hi]` — `value_at(lo) < target`
        // and either `hi == len` or `value_at(hi) >= target`.  Binary-search it.
        let mut left = lo + 1;
        let mut right = hi;
        while left < right {
            let mid = left + (right - left) / 2;
            if self.value_at(mid) >= target {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        left
    }
}

/// The cursor kind: a single-run scan (`Any` / `Subject` / `Object`) or a
/// leapfrog intersection of two runs (`Both`).
enum Kind<'a> {
    /// A single ascending run; `pos` is the next posting to yield.
    Scan { run: Postings<'a>, pos: usize },
    /// The intersection of a subject run and an object run, driven by alternating
    /// [`Postings::gallop`] calls (the leapfrog-join); `si`/`oi` are the two runs'
    /// next positions.
    Leapfrog {
        subj: Postings<'a>,
        obj: Postings<'a>,
        si: usize,
        oi: usize,
    },
}

/// A galloping lending cursor over one relation's `(subject_id, object_id, row_id)`
/// id rows, in **row-id (insertion) order**.
///
/// Borrows the relation for the cursor's lifetime; every yielded row is resolved
/// through [`Relation::row_at`], so nothing is cloned or allocated per row.  Built
/// by the [`RowCursor::any`] / [`subject`](Self::subject) / [`object`](Self::object)
/// / [`select_both`](Self::select_both) / [`empty`](Self::empty) constructors — one
/// per [`super::store::Bound`] shape.
pub(crate) struct RowCursor<'a> {
    rel: &'a Relation,
    kind: Kind<'a>,
}

impl<'a> RowCursor<'a> {
    /// The `Bound::Any` cursor: every row in insertion (= row-id) order.
    pub(crate) fn any(rel: &'a Relation) -> Self {
        Self {
            rel,
            kind: Kind::Scan {
                run: Postings::Range {
                    len: rel.row_count(),
                },
                pos: 0,
            },
        }
    }

    /// The `Bound::Subject` cursor: rows whose subject is `bucket`'s term, ascending.
    pub(crate) fn subject(rel: &'a Relation, bucket: &'a [usize]) -> Self {
        Self {
            rel,
            kind: Kind::Scan {
                run: Postings::Bucket(bucket),
                pos: 0,
            },
        }
    }

    /// The `Bound::Object` cursor: rows whose object is `bucket`'s term, ascending.
    pub(crate) fn object(rel: &'a Relation, bucket: &'a [usize]) -> Self {
        Self {
            rel,
            kind: Kind::Scan {
                run: Postings::Bucket(bucket),
                pos: 0,
            },
        }
    }

    /// The `Bound::Both` cursor: the leapfrog intersection of the subject and object
    /// buckets.
    ///
    /// Both buckets hold ascending row indices, so the rows satisfying BOTH bounds
    /// are exactly their sorted intersection.  This drives two runs with alternating
    /// [`Postings::gallop`] seeks (the leapfrog-join), yielding the intersection in
    /// ascending row-index order — **byte-identical** to the former linear
    /// two-pointer merge (same match set, same ascending order), but the mismatched
    /// side gallops past runs of non-matches instead of stepping one at a time.
    pub(crate) fn select_both(rel: &'a Relation, by_s: &'a [usize], by_o: &'a [usize]) -> Self {
        Self {
            rel,
            kind: Kind::Leapfrog {
                subj: Postings::Bucket(by_s),
                obj: Postings::Bucket(by_o),
                si: 0,
                oi: 0,
            },
        }
    }

    /// The empty cursor: an unknown predicate yields no rows (over an empty run).
    pub(crate) fn empty(rel: &'a Relation) -> Self {
        Self {
            rel,
            kind: Kind::Scan {
                run: Postings::Range { len: 0 },
                pos: 0,
            },
        }
    }

    /// Whether the cursor has at least one remaining row — the allocation-free
    /// membership probe used by existential NAF (no `Vec` materialized just to ask
    /// `!is_empty()`).
    pub(crate) fn any_remaining(mut self) -> bool {
        self.next().is_some()
    }

    /// Advance the leapfrog cursor to the next matching row index, leaving `si`/`oi`
    /// pointing PAST it; `None` when either run is exhausted.
    ///
    /// A faithful galloping translation of the two-pointer intersection: on a
    /// mismatch the smaller side gallops to the first position `>= the other side's
    /// current value` (skipping only strictly-smaller values, which can never be
    /// matches), and on a tie the shared row index is emitted and both advance.  The
    /// emitted sequence is therefore exactly the sorted intersection — identical to
    /// the linear merge.
    fn leapfrog_next(
        subj: Postings<'a>,
        obj: Postings<'a>,
        si: &mut usize,
        oi: &mut usize,
    ) -> Option<usize> {
        while *si < subj.len() && *oi < obj.len() {
            let a = subj.value_at(*si);
            let b = obj.value_at(*oi);
            match a.cmp(&b) {
                std::cmp::Ordering::Less => *si = subj.gallop(*si + 1, b),
                std::cmp::Ordering::Greater => *oi = obj.gallop(*oi + 1, a),
                std::cmp::Ordering::Equal => {
                    *si += 1;
                    *oi += 1;
                    return Some(a);
                }
            }
        }
        None
    }
}

impl LendingIterator for RowCursor<'_> {
    type Item<'a>
        = (TermId, TermId, RowId)
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>> {
        match &mut self.kind {
            Kind::Scan { run, pos } => {
                if *pos >= run.len() {
                    return None;
                }
                let ri = run.value_at(*pos);
                *pos += 1;
                Some(self.rel.row_at(ri))
            }
            Kind::Leapfrog { subj, obj, si, oi } => {
                let ri = RowCursor::leapfrog_next(*subj, *obj, si, oi)?;
                Some(self.rel.row_at(ri))
            }
        }
    }

    fn seek(&mut self, target: usize) -> bool {
        match &mut self.kind {
            Kind::Scan { run, pos } => {
                *pos = run.gallop(*pos, target);
                *pos < run.len()
            }
            Kind::Leapfrog { subj, obj, si, oi } => {
                // Gallop BOTH runs to the `>= target` frontier — the multiway-leapfrog
                // seek primitive a future triejoin would compose one such seek per
                // level from.  A subsequent `next()` converges from the new frontier.
                *si = subj.gallop(*si, target);
                *oi = obj.gallop(*oi, target);
                *si < subj.len() && *oi < obj.len()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::store::Bound;

    /// Drain a cursor into a `Vec` — a `#[cfg(test)]`-only convenience for asserting a
    /// cursor's full row sequence (the production hot path never collects).
    fn drain(mut c: RowCursor<'_>) -> Vec<(TermId, TermId, RowId)> {
        let mut out = Vec::new();
        while let Some(row) = c.next() {
            out.push(row);
        }
        out
    }

    /// A naive linear two-pointer intersection of two ascending buckets — the
    /// pre-leapfrog reference `select_both` used, kept `#[cfg(test)]`-only as the
    /// differential oracle for the galloping leapfrog.
    fn naive_intersect(by_s: &[usize], by_o: &[usize]) -> Vec<usize> {
        let mut out = Vec::new();
        let (mut i, mut j) = (0usize, 0usize);
        while i < by_s.len() && j < by_o.len() {
            match by_s[i].cmp(&by_o[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    out.push(by_s[i]);
                    i += 1;
                    j += 1;
                }
            }
        }
        out
    }

    /// `Postings::gallop` is a correct lower-bound: for every `target` it returns the
    /// first position whose value is `>= target`, matching a linear reference — over
    /// both the `Range` (full-scan) and `Bucket` (posting) forms.
    #[test]
    fn cursor_gallop_matches_linear_lower_bound() {
        // A Range 0..10: value_at(p) == p, so lower_bound(target) == min(target, 10).
        let range = Postings::Range { len: 10 };
        for target in 0..=12 {
            let want = (0..10).position(|v| v >= target).unwrap_or(10);
            assert_eq!(
                range.gallop(0, target),
                want,
                "range gallop lower-bound for target {target}"
            );
        }
        // A sparse ascending bucket with gaps (exercises the exponential probe).
        let bucket_vec = vec![0usize, 3, 4, 9, 15, 16, 100];
        let bucket = Postings::Bucket(&bucket_vec);
        for target in 0..=101 {
            let want = bucket_vec
                .iter()
                .position(|&v| v >= target)
                .unwrap_or(bucket_vec.len());
            assert_eq!(
                bucket.gallop(0, target),
                want,
                "bucket gallop lower-bound for target {target}"
            );
        }
        // Galloping FROM a non-zero start never returns a position before `from`.
        assert_eq!(bucket.gallop(3, 0), 3, "gallop respects the `from` floor");
        assert_eq!(
            bucket.gallop(3, 10),
            4,
            "gallop(from=3, target=10) ⇒ pos of 15"
        );
    }

    /// LEAPFROG-MERGE DIFFERENTIAL TEST (required by Task 6).
    ///
    /// The galloping leapfrog intersection MUST yield exactly the same row-index
    /// sequence, in the same order, as the naive linear two-pointer merge — across a
    /// spread of overlap shapes (full overlap, partial, disjoint, single-sided,
    /// empty, long-run-vs-sparse which is where galloping actually skips).  Any
    /// divergence here is a byte-drift on the `Bound::Both` join.
    #[test]
    fn cursor_leapfrog_matches_naive_two_pointer_merge() {
        let cases: &[(&[usize], &[usize])] = &[
            (&[], &[]),
            (&[0, 1, 2], &[]),
            (&[], &[0, 1, 2]),
            (&[0, 1, 2, 3], &[0, 1, 2, 3]),           // full overlap
            (&[0, 2, 4, 6, 8], &[1, 3, 5, 7]),        // disjoint (interleaved)
            (&[0, 1, 2, 3, 4], &[2, 3, 8, 9]),        // partial overlap
            (&[5], &[0, 1, 2, 3, 4, 5, 6]),           // single element deep in a run
            (&[0, 1, 2, 3, 4, 5, 6, 7], &[7]),        // single element at the tail
            (&[0, 50, 100], &[1, 2, 3, 50, 51, 100]), // sparse-vs-dense (gallop skips)
            (
                &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
                &[3, 12],
            ), // long run intersected by two points — the galloping win case
        ];
        for &(by_s, by_o) in cases {
            let want = naive_intersect(by_s, by_o);
            // Drive the leapfrog directly over the two runs (relation-free: the merge
            // is over row-INDEX positions, independent of term resolution).
            let (subj, obj) = (Postings::Bucket(by_s), Postings::Bucket(by_o));
            let (mut si, mut oi) = (0usize, 0usize);
            let mut got = Vec::new();
            while let Some(ri) = RowCursor::leapfrog_next(subj, obj, &mut si, &mut oi) {
                got.push(ri);
            }
            assert_eq!(
                got, want,
                "leapfrog intersection of {by_s:?} ∩ {by_o:?} must equal the naive merge"
            );
        }
    }

    /// The leapfrog `seek` gallops BOTH runs to the `>= target` frontier, so the next
    /// converged match is at row index `>= target` — the composable seek a future
    /// multiway triejoin would reuse.  Exercised at the `Postings` level (the exact two
    /// gallop calls [`RowCursor::seek`]'s `Leapfrog` arm performs, then a converge),
    /// since a real relation's `(s,o)` unique key makes a live `Both` cursor's buckets
    /// intersect in at most one row — too coarse to probe the multi-match frontier.
    #[test]
    fn cursor_leapfrog_seek_advances_frontier() {
        let by_s = vec![0usize, 2, 4, 6, 8, 10];
        let by_o = vec![0usize, 4, 8, 12];
        let (subj, obj) = (Postings::Bucket(&by_s), Postings::Bucket(&by_o));
        // Full intersection is {0, 4, 8}. Seeking to 5 gallops both runs past 0 and 4.
        let mut si = subj.gallop(0, 5);
        let mut oi = obj.gallop(0, 5);
        assert!(
            si < subj.len() && oi < obj.len(),
            "seek(5) leaves a live frontier (8 remains)"
        );
        assert_eq!(
            RowCursor::leapfrog_next(subj, obj, &mut si, &mut oi),
            Some(8),
            "after seek(5) the next converged match is 8 (0 and 4 skipped)"
        );
        // And the whole post-seek intersection is exactly {8} (10 is not in `by_o`).
        assert_eq!(RowCursor::leapfrog_next(subj, obj, &mut si, &mut oi), None);
    }

    /// [`RowCursor::seek`]'s `Scan` arm, driven over a REAL relation: galloping an
    /// `Any` full scan to a row-index frontier leaves exactly the rows at index
    /// `>= target`, and seeking past the end exhausts the cursor.  This exercises the
    /// production `seek` path (row-INDEX positions, never term values).
    #[test]
    fn cursor_scan_seek_over_real_relation() {
        use crate::physical::store::RelationStore;
        use purrdf::TermValue;

        let mut s = RelationStore::new();
        for o in ["b", "c", "d", "e", "f"] {
            assert!(
                s.insert(
                    "http://ex/p",
                    &TermValue::iri("http://ex/a"),
                    &TermValue::iri(format!("http://ex/{o}")),
                )
                .is_some()
            );
        }
        // A full scan over 5 rows (indices 0..5). seek(3) leaves rows 3 and 4.
        let mut c = s.select("http://ex/p", Bound::Any);
        assert!(c.seek(3), "seek(3) leaves a live frontier");
        let mut remaining = 0;
        while c.next().is_some() {
            remaining += 1;
        }
        assert_eq!(remaining, 2, "rows at index 3 and 4 remain after seek(3)");

        // Seeking past the end exhausts the cursor (no live posting).
        let mut c2 = s.select("http://ex/p", Bound::Any);
        assert!(!c2.seek(10), "seek past the end reports exhaustion");
        assert!(c2.next().is_none(), "an exhausted scan yields nothing");

        // A `Both` cursor over the unique (a,d) key yields exactly that one row, and a
        // real `RowCursor::seek` on its `Leapfrog` arm is well-formed.
        let a = s.term_id("<http://ex/a>").expect("a interned");
        let d = s.term_id("<http://ex/d>").expect("d interned");
        assert_eq!(drain(s.select("http://ex/p", Bound::Both(a, d))).len(), 1);
        let mut cb = s.select("http://ex/p", Bound::Both(a, d));
        assert!(cb.seek(0), "seek(0) keeps the single (a,d) match live");
        assert!(cb.next().is_some(), "the (a,d) row is yielded");
        assert!(cb.next().is_none(), "unique key ⇒ exactly one Both row");
    }
}
