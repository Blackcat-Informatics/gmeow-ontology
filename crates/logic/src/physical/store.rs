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
//! the column-oriented analogue: per predicate a [`Relation`] holds `(subject, object)`
//! tuples as a **shared arrangement** — a log of sorted immutable batches plus a small
//! mutable tail (the McSherry-et-al. columnar discipline).
//!
//! # The arrangement shape
//!
//! - A [`Batch`] is flat dense-ID columns (`subj`, `obj`, `row_id`) in canonical
//!   `(subject_id, object_id)` order, so a subject-bound probe GALLOPS the sorted
//!   `subj` column to the term's contiguous run — no eager `by_subject` map, subject
//!   grouping falls out of the sort.  The `(object, subject)` access path is a
//!   lazily-built permutation ([`ObjectIndex`]), materialized only on the first
//!   object-bound probe (never eagerly, never for a subject-only relation).
//! - The mutable **tail** absorbs the current epoch's inserts unsorted; it is sealed
//!   into a sorted batch geometrically (LSM size-tiered), and adjacent batches
//!   consolidate by a streaming merge.  A tiny relation never seals — it stays a single
//!   small tail `Vec`, allocation-light.
//! - Dedup on insert uses GALLOPING search over the sorted batches plus a linear scan
//!   of the small tail — **no per-row hashing, no postings-list maintenance** (the two
//!   eager `HashMap` indexes and the dedup `HashSet` are deleted, greenfield).
//! - The single sorted representation is generic over an abelian [`Weight`] monoid
//!   instantiated `W = ()` in production; the same consolidation merge compiles for
//!   `W = i64` (Z-set signed multiplicities), so signed-weight consolidation falls out
//!   of one representation as a compiled fact.
//!
//! # Determinism (non-negotiable)
//!
//! - Term ids are minted by the store's single [`TermInterner`], keyed on the
//!   exact native `TermValue` identity, including every literal and blank-scope field.  A batch's internal `(subject_id,
//!   object_id)` sort is by mint order — an INTERNAL storage order, never an emission
//!   order: the semi-naive winner selection is a total order over provenance (see
//!   [`crate::rule_ir::RuleRoundCandidate::tiebreak_key`]), so the order in which a
//!   cursor enumerates rows never reaches output.
//! - A join probe translates a borrowed native value to an id via
//!   [`RelationStore::term_id`] (non-inserting): a miss means the term has never
//!   entered the store, so the selection is empty — the single place that
//!   semantics lives.
//! - Any "all predicates" iteration is sorted (BTreeSet), never raw map order.
//!
//! # The single oxigraph → columnar bridge
//!
//! [`extract_edb`] is the SOLE place the forward and backward engine paths cross from
//! the oxigraph blackboard ([`crate::seam::WorldFactSource`]) into the columnar form.

mod semantic;

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::sync::OnceLock;

use purrdf::TermValue;

use crate::facts::{PredId, PredInterner, TermId, TermInterner};
use crate::physical::cursor::{
    LendingIterator, RowCursor, VALUE_OBJECT, VALUE_SUBJECT, ValueCursor,
};
use crate::physical::id::RowId;
use crate::provenance::{ProvenanceSemiring, ZWeightSemiring};
use crate::rule_ir::Fact;
use crate::seam::{DerivedQuad, WorldFactPattern, WorldFactSource};

mod witness;
pub(crate) use witness::{SkolemRegistry, SkolemTerm, WitnessContract, metadata_identity};
pub use witness::{
    WitnessDerivation, WitnessHead, WitnessOrigin, WitnessPosition, WitnessScope, WitnessStatement,
};

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

// ── The weight monoid: the Z-set seam ───────────────────────────────────────────
//
// A [`Batch`] is generic over an abelian weight `W`.  Production set semantics
// instantiate `W = ()` — the unit monoid, a zero-sized type, so `Vec<()>` allocates
// nothing and the weight column costs zero live bytes.  The SAME consolidation merge
// compiles for `W = i64` (a Z-set with signed multiplicities): `combine` sums weights
// and an annihilated (zero) row drops.  So "the representation admits signed weights"
// is a COMPILED fact — the merge already monomorphizes for both — not a promise; the
// incremental/retraction lever changes one type parameter, never the representation.

/// An abelian weight monoid over relation rows (the Z-set seam).
pub(crate) trait Weight: Copy {
    /// Structured failure type for consolidation. Set weights are infallible;
    /// signed weights report checked-ring overflow.
    type Error;
    /// The multiplicity of a freshly inserted row.
    const UNIT: Self;
    /// The abelian combine applied when two runs carry the SAME `(subject, object)` key
    /// during consolidation (associative + commutative).
    fn combine(self, rhs: Self) -> Result<Self, Self::Error>;
    /// Whether a combined weight annihilates the row, so consolidation drops it.
    fn is_annihilated(self) -> bool;
}

impl Weight for () {
    type Error = Infallible;
    const UNIT: Self = ();
    #[inline]
    fn combine(self, _rhs: Self) -> Result<Self, Self::Error> {
        Ok(())
    }
    #[inline]
    fn is_annihilated(self) -> bool {
        // Set semantics: every live row has unit weight and never consolidates away.
        false
    }
}

impl Weight for i64 {
    type Error = gmeow_errors::Diag;
    const UNIT: Self = 1;
    #[inline]
    fn combine(self, rhs: Self) -> Result<Self, Self::Error> {
        ZWeightSemiring.add(self, rhs)
    }
    #[inline]
    fn is_annihilated(self) -> bool {
        self == 0
    }
}

/// The first position `>= from` in the strictly-ascending run `xs` whose value is
/// `>= key`, found by GALLOPING (exponential probe to bracket, then binary search) —
/// never a linear scan and never a hash probe.  This is the sorted-run lower-bound the
/// whole arrangement leans on (subject-run location, object-run location, dedup), and
/// the exact primitive a future multiway-leapfrog triejoin composes.
fn gallop_lower_bound(xs: &[TermId], from: usize, key: TermId) -> usize {
    let len = xs.len();
    if from >= len {
        return len;
    }
    if xs[from] >= key {
        return from;
    }
    // Exponential probe: keep `xs[lo] < key`, doubling the stride until `hi` brackets a
    // value `>= key` (or runs off the end).
    let mut lo = from;
    let mut step = 1usize;
    let hi = loop {
        let probe = lo.saturating_add(step);
        if probe >= len {
            break len;
        }
        if xs[probe] >= key {
            break probe;
        }
        lo = probe;
        step = step.saturating_mul(2);
    };
    // The first position `>= key` lies in `(lo, hi]`; binary-search it.
    let (mut left, mut right) = (lo + 1, hi);
    while left < right {
        let mid = left + (right - left) / 2;
        if xs[mid] >= key {
            right = mid;
        } else {
            left = mid + 1;
        }
    }
    left
}

/// The lazily-built secondary access path for one [`Batch`]: the batch's row positions
/// in `(object_id, subject_id)` order, so an object-bound probe gallops to its run.
///
/// Built ON FIRST object-bound demand (never eagerly, never for a subject-only
/// relation) and memoized in a [`OnceLock`] — write-once and `Sync`, so a future
/// parallel delta-partition firing that shares `&Batch` across threads initializes it
/// cleanly.  A permutation of `u32` positions — never a hash map, never a per-key
/// `Vec`: 4 bytes per row, materialized only when an object bound is actually probed.
#[derive(Debug, Clone, Default)]
struct ObjectIndex {
    /// Row positions of the batch, sorted by `(object_id, subject_id)`.
    perm: Box<[u32]>,
}

/// One immutable sorted batch: a relation's `(subject, object)` rows in canonical
/// `(subject_id, object_id)` order, stored as flat dense-ID columns.
///
/// The primary sort is subject-major, so a subject-bound probe gallops the `subj`
/// column to the term's contiguous run with NO secondary structure (the eager
/// `by_subject` map is deleted — subject grouping falls out of the sort).  The
/// `(object, subject)` access path is the lazily-built [`ObjectIndex`].  Generic over
/// the weight monoid `W` (the Z-set seam); the production instantiation is `W = ()`.
///
/// `pub(crate)` (fields stay private) so the lending [`RowCursor`] can borrow a slice of
/// batches and drive their galloping runs; it is the SOLE row-materialization path.
#[derive(Debug, Clone)]
pub(crate) struct Batch<W: Weight = ()> {
    /// Subject column, ascending (subject-major within the `(subject, object)` sort).
    subj: Vec<TermId>,
    /// Object column, ascending within each subject run.
    obj: Vec<TermId>,
    /// Store-global dense [`RowId`] per row, parallel to the columns.
    row_id: Vec<RowId>,
    /// Multiplicity per row; `Vec<()>` is zero-sized under set semantics.
    weight: Vec<W>,
    /// The lazily-built `(object, subject)` access path (built on first object probe).
    object_index: OnceLock<ObjectIndex>,
}

impl<W: Weight> Batch<W> {
    /// Build a batch from rows ALREADY sorted ascending by `(subject_id, object_id)`
    /// and free of duplicate keys.  Weights default to [`Weight::UNIT`].
    fn from_sorted(rows: &[(TermId, TermId, RowId)]) -> Self {
        let mut subj = Vec::with_capacity(rows.len());
        let mut obj = Vec::with_capacity(rows.len());
        let mut row_id = Vec::with_capacity(rows.len());
        let mut weight = Vec::with_capacity(rows.len());
        for &(s, o, r) in rows {
            subj.push(s);
            obj.push(o);
            row_id.push(r);
            weight.push(W::UNIT);
        }
        Self {
            subj,
            obj,
            row_id,
            weight,
            object_index: OnceLock::new(),
        }
    }

    /// The number of rows in the batch.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.row_id.len()
    }

    /// The `(subject_id, object_id, row_id)` id row at column position `p`.
    #[inline]
    pub(crate) fn row_at(&self, p: usize) -> (TermId, TermId, RowId) {
        (self.subj[p], self.obj[p], self.row_id[p])
    }

    /// The `[lo, hi)` column-position run whose subject is `s`, located by galloping the
    /// sorted `subj` column (subject grouping is contiguous in the primary sort).
    pub(crate) fn subject_run(&self, s: TermId) -> (usize, usize) {
        let lo = gallop_lower_bound(&self.subj, 0, s);
        // `hi` is the first position past `s`'s contiguous run — a binary search of the
        // sorted suffix, so a `Both` probe stays O(log) rather than O(run length).
        let hi = lo + self.subj[lo..].partition_point(|&x| x <= s);
        (lo, hi)
    }

    /// The single column position of the unique `(s, o)` row, if present: gallop the
    /// subject run, then binary-search its ascending `obj` sub-column for `o`.
    pub(crate) fn both_pos(&self, s: TermId, o: TermId) -> Option<usize> {
        let (lo, hi) = self.subject_run(s);
        let run = &self.obj[lo..hi];
        run.binary_search(&o).ok().map(|off| lo + off)
    }

    /// Whether the unique `(s, o)` key is present in this batch.
    fn contains(&self, s: TermId, o: TermId) -> bool {
        self.both_pos(s, o).is_some()
    }

    /// The batch positions whose object is `o`, via the lazily-built [`ObjectIndex`]
    /// (built on first demand).  A subslice of the `(object, subject)`-sorted permutation.
    pub(crate) fn object_positions(&self, o: TermId) -> &[u32] {
        let perm = self.object_order();
        let lo = perm.partition_point(|&p| self.obj[p as usize] < o);
        let hi = perm.partition_point(|&p| self.obj[p as usize] <= o);
        &perm[lo..hi]
    }

    /// Every batch position in `(object_id, subject_id)` order. The same lazy,
    /// memoized permutation backs object-bound binary probes and object-major LFTJ
    /// trie levels; it is built once and shared by both operators.
    pub(crate) fn object_order(&self) -> &[u32] {
        &self
            .object_index
            .get_or_init(|| {
                let mut perm: Vec<u32> = (0..self.len() as u32).collect();
                // Sort positions by (object_id, subject_id) — the secondary access order.
                // `(object, subject)` keys are unique within a batch (the primary sort is
                // key-disjoint), so no equal elements exist to preserve order for: the
                // unstable sort is a pure win (no scratch allocation, lower constants),
                // matching `seal()`'s `sort_unstable_by_key`.
                perm.sort_unstable_by(|&a, &b| {
                    let (a, b) = (a as usize, b as usize);
                    (self.obj[a], self.subj[a]).cmp(&(self.obj[b], self.subj[b]))
                });
                ObjectIndex {
                    perm: perm.into_boxed_slice(),
                }
            })
            .perm
    }
}

/// The size a mutable tail may reach before it is sealed into a sorted batch.  A tiny
/// relation never reaches it — it stays a single small tail `Vec`, allocation-light
/// (the `foundation`/small-relation guarantee).  Chosen small so a tail scan (dedup on
/// insert, and the cursor's tail leg) stays cheap between seals.
const TAIL_SEAL_THRESHOLD: usize = 64;

/// A single binary relation: the `(subject, object)` rows of ONE predicate IRI, held as
/// a **shared arrangement** — a log of sorted immutable [`Batch`]es plus a mutable tail.
///
/// Term interning lives at the [`RelationStore`] level (one dictionary shared by every
/// relation), so `insert` borrows the store's interner.  Production set semantics fix
/// the weight monoid at `W = ()`.
///
/// `pub(crate)` (its fields stay private) so the arrangement's native lending cursor
/// [`crate::physical::cursor::RowCursor`] can borrow it; the cursor is the SOLE
/// row-materialization path.
#[derive(Debug, Clone, Default)]
pub(crate) struct Relation {
    /// Immutable sorted batches (each `(subject_id, object_id)`-ordered, key-disjoint),
    /// newest last.  Empty for a tail-only (never-sealed) relation.
    batches: Vec<Batch>,
    /// The mutable tail: `(subject_id, object_id, row_id)` rows of the current epoch, in
    /// insertion order, sealed into a batch once it reaches [`TAIL_SEAL_THRESHOLD`].
    tail: Vec<(TermId, TermId, RowId)>,
    /// The number of rows across batches + tail (the dense per-relation row count).
    len: usize,
}

impl Relation {
    /// Insert `(subject, object)` if its `(subject_id, object_id)` key is not already
    /// present, stamping it with the store-assigned `row_id`; return `Some((subject_id,
    /// object_id))` if newly inserted, or `None` on a duplicate.
    ///
    /// Dedup is a GALLOPING probe of every sorted batch plus a linear scan of the small
    /// tail — no per-row hashing, no postings maintenance.  A new row is appended to the
    /// unsorted tail; when the tail reaches [`TAIL_SEAL_THRESHOLD`] it is sealed into a
    /// sorted batch and the batch log consolidates.
    fn insert(
        &mut self,
        interner: &mut TermInterner,
        subject: &TermValue,
        object: &TermValue,
        row_id: RowId,
    ) -> Option<(TermId, TermId)> {
        let s_id = interner.intern(subject);
        let o_id = interner.intern(object);
        if self.contains(s_id, o_id) {
            return None;
        }
        self.tail.push((s_id, o_id, row_id));
        self.len += 1;
        if self.tail.len() >= TAIL_SEAL_THRESHOLD {
            self.seal();
        }
        Some((s_id, o_id))
    }

    /// Whether a tuple with these interned terms is present — a galloping probe of each
    /// sorted batch plus a linear scan of the tail (no hashing).
    fn contains(&self, subject: TermId, object: TermId) -> bool {
        self.batches.iter().any(|b| b.contains(subject, object))
            || self
                .tail
                .iter()
                .any(|&(s, o, _)| s == subject && o == object)
    }

    /// Seal the mutable tail into a new sorted immutable batch, then consolidate.
    ///
    /// Sorting the tail by `(subject_id, object_id)` establishes the canonical batch
    /// order; the tail is dedup-free by construction (insert rejects duplicate keys), so
    /// the sort is a plain columnar build with no combine.  Consolidation then merges
    /// the batch log geometrically.  RowIds are already stamped, so sealing is a pure
    /// storage reorganization — it never changes the row set, the row ids, or the count.
    fn seal(&mut self) {
        if self.tail.is_empty() {
            return;
        }
        let mut rows = std::mem::take(&mut self.tail);
        rows.sort_unstable_by_key(|&(s, o, _)| (s, o));
        self.batches.push(Batch::from_sorted(&rows));
        self.consolidate();
    }

    /// Geometric (size-tiered) consolidation: while the newest two batches are within a
    /// factor of two in size, merge them into one sorted batch.  This bounds the live
    /// batch count logarithmically so a probe gallops O(log n) runs.
    fn consolidate(&mut self) {
        while self.batches.len() >= 2 {
            let n = self.batches.len();
            let (a, b) = (self.batches[n - 2].len(), self.batches[n - 1].len());
            if b * 2 < a {
                break;
            }
            let right = self.batches.pop().expect("len >= 2");
            let left = self.batches.pop().expect("len >= 2");
            let merged = match merge_batches(&left, &right) {
                Ok(batch) => batch,
                Err(never) => match never {},
            };
            self.batches.push(merged);
        }
    }

    /// The number of rows in this relation (batches + tail).
    #[inline]
    pub(crate) fn row_count(&self) -> usize {
        self.len
    }

    /// A lending [`RowCursor`] over the `(subject_id, object_id, row_id)` id rows
    /// selected by `bound`, borrowing this relation's columns — no per-stage `Vec` is
    /// materialized.
    ///
    /// The cursor concatenates each batch's bound-run (galloped over the sorted columns)
    /// with a linear scan of the tail.  Enumeration order is batch-then-tail, NOT a
    /// global merge sort — sound because winner selection is a total order over
    /// provenance ([`crate::rule_ir::RuleRoundCandidate::tiebreak_key`]), so cursor
    /// order never reaches output.  The `(s, o)` key is unique, so a `Both` bound yields
    /// at most one row across the whole relation.
    fn select(&self, bound: Bound) -> RowCursor<'_> {
        RowCursor::new(self, bound)
    }

    /// The batches of this relation, newest last — the cursor's per-batch sub-runs.
    #[inline]
    pub(crate) fn batches(&self) -> &[Batch] {
        &self.batches
    }

    /// The unsorted tail rows — the cursor's final (linear-scanned) leg.
    #[inline]
    pub(crate) fn tail(&self) -> &[(TermId, TermId, RowId)] {
        &self.tail
    }
}

/// Merge two sorted, key-disjoint-or-weighted batches into one sorted batch.
///
/// A streaming two-way merge over the `(subject_id, object_id)` key: O(1) scratch beyond
/// the output, never a whole-relation re-sort, so no transient allocation spike.  On a
/// key COLLISION (only reachable for a signed weight monoid — set-semantics inserts keep
/// batches key-disjoint) the weights [`combine`](Weight::combine) and the surviving row
/// keeps the LOWER [`RowId`] (deterministic, run-order independent); an annihilated
/// weight drops the row.  For `W = ()` the collision arm is dead and this is a plain
/// interleave.
fn merge_batches<W: Weight>(left: &Batch<W>, right: &Batch<W>) -> Result<Batch<W>, W::Error> {
    let cap = left.len() + right.len();
    let mut subj = Vec::with_capacity(cap);
    let mut obj = Vec::with_capacity(cap);
    let mut row_id = Vec::with_capacity(cap);
    let mut weight = Vec::with_capacity(cap);
    let (mut i, mut j) = (0usize, 0usize);
    let push = |subj: &mut Vec<TermId>,
                obj: &mut Vec<TermId>,
                row_id: &mut Vec<RowId>,
                weight: &mut Vec<W>,
                b: &Batch<W>,
                p: usize| {
        subj.push(b.subj[p]);
        obj.push(b.obj[p]);
        row_id.push(b.row_id[p]);
        weight.push(b.weight[p]);
    };
    while i < left.len() && j < right.len() {
        let lk = (left.subj[i], left.obj[i]);
        let rk = (right.subj[j], right.obj[j]);
        match lk.cmp(&rk) {
            std::cmp::Ordering::Less => {
                push(&mut subj, &mut obj, &mut row_id, &mut weight, left, i);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                push(&mut subj, &mut obj, &mut row_id, &mut weight, right, j);
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                // Key collision (signed-weight only): combine, keep the lower RowId, drop
                // if annihilated.  Never reached under set-semantics `W = ()`.
                let w = left.weight[i].combine(right.weight[j])?;
                if !w.is_annihilated() {
                    subj.push(left.subj[i]);
                    obj.push(left.obj[i]);
                    row_id.push(left.row_id[i].min(right.row_id[j]));
                    weight.push(w);
                }
                i += 1;
                j += 1;
            }
        }
    }
    while i < left.len() {
        push(&mut subj, &mut obj, &mut row_id, &mut weight, left, i);
        i += 1;
    }
    while j < right.len() {
        push(&mut subj, &mut obj, &mut row_id, &mut weight, right, j);
        j += 1;
    }
    Ok(Batch {
        subj,
        obj,
        row_id,
        weight,
        object_index: OnceLock::new(),
    })
}

/// A columnar set of binary relations keyed by predicate IRI (`NamedNode::as_str()`).
///
/// One [`Relation`] per predicate, all sharing ONE [`TermInterner`]; this is the
/// native engine's working EDB/IDB form.  The ids the interner mints are meaningless
/// outside this store — probes obtain them via [`Self::term_id`].
#[derive(Debug, Clone, Default)]
pub(crate) struct RelationStore {
    pub(crate) semantics: crate::native_semantics::SemanticVocabulary,
    /// The store's term dictionary, shared by every relation (the persistent term
    /// arena — never reset within the store's lifetime; the future structured-term DAG seam).
    interner: TermInterner,
    /// The store's predicate dictionary: predicate IRI surface → dense [`PredId`],
    /// interned once at first insert.  Keeps [`relations`](Self::relations) keyed by a
    /// `Copy` niche integer instead of an owned `String`.
    predicates: PredInterner,
    /// Binary relations indexed by [`PredId`] slot (`relations[pid.index()]`).
    ///
    /// `PredId`s are minted densely (0, 1, 2, …) so a new predicate's slot is always
    /// the vector's current length; there are never gap / empty relations.
    relations: Vec<Relation>,
    /// The number of rows inserted so far across ALL relations — equivalently, the next
    /// dense [`RowId`] slot to assign.  RowIds are minted `0, 1, 2, …` in store-wide
    /// insertion order, so at any point the live rows are exactly RowIds `0..row_count`.
    /// This is the single row-id source; the id never enters a derivation/provenance hash.
    row_count: usize,
    /// A permanently-empty relation handed to [`select`](Self::select) on a predicate
    /// miss, so an unknown predicate yields an empty [`RowCursor`] with NO `Option`
    /// branch on the per-row scan — the cursor is over a zero-length run, its `rel`
    /// borrow never dereferenced.  Never inserted into.
    empty: Relation,
}

impl RelationStore {
    /// A fresh, empty store.
    pub(crate) fn new() -> Self {
        Self {
            semantics: crate::native_semantics::SemanticVocabulary::Exact,
            interner: TermInterner::new(),
            predicates: PredInterner::new(),
            relations: Vec::new(),
            row_count: 0,
            empty: Relation::default(),
        }
    }

    pub(crate) fn with_semantics(semantics: crate::native_semantics::SemanticVocabulary) -> Self {
        Self {
            semantics,
            ..Self::new()
        }
    }

    /// Insert `(subject, object)` under `predicate`; return
    /// `Some((subject_id, object_id, row_id))` with the terms' interned ids and the
    /// newly-assigned store-global [`RowId`] if the tuple was newly inserted, or `None`
    /// if it was already present (dedup).
    ///
    /// The predicate IRI is interned to a [`PredId`] once (borrowed-key probe — no
    /// owned-key clone per call); the tuple is deduped on its interned id key per
    /// relation, and both secondary indexes are maintained in lockstep.  A successful
    /// insert stamps the row with the next dense RowId (insertion order across the whole
    /// store) — the identity the semi-naive delta bitset is keyed on.  The interned
    /// subject/object ids are returned alongside the row id so the commit-path caller
    /// threads them onward without a redundant second interner lookup.
    pub(crate) fn insert(
        &mut self,
        predicate: &str,
        subject: &TermValue,
        object: &TermValue,
    ) -> Option<(TermId, TermId, RowId)> {
        let idx = self.predicates.intern(predicate).index();
        if idx >= self.relations.len() {
            // A newly-minted PredId's slot is always the current length (dense mint),
            // so this resize adds exactly one default relation — never an empty gap.
            self.relations.resize_with(idx + 1, Relation::default);
        }
        let row_id = RowId::from_index(self.row_count);
        self.relations[idx]
            .insert(&mut self.interner, subject, object, row_id)
            .map(|(s_id, o_id)| {
                self.row_count += 1;
                (s_id, o_id, row_id)
            })
    }

    /// The number of rows currently in the store across all relations — equivalently,
    /// the exclusive upper bound of the live dense [`RowId`]s (`0..row_count`).
    ///
    /// The semi-naive fixpoint sizes its round-1 delta bitset from this — every
    /// accumulated row is "new" in round 1, so the seed is `all_set(row_count)` with no
    /// per-key materialization.
    pub(crate) fn row_count(&self) -> usize {
        self.row_count
    }

    /// The store's term dictionary — for resolving a selected id row's `(subject,
    /// object)` back to their [`TermValue`] surfaces at the point a caller stringifies.
    pub(crate) fn interner(&self) -> &TermInterner {
        &self.interner
    }

    /// The interned [`PredId`] for `predicate`, if any relation of this store carries
    /// it; never inserts.  `None` ⇒ no relation ⇒ any selection on it is empty.
    pub(crate) fn pred_id(&self, predicate: &str) -> Option<PredId> {
        self.predicates.lookup(predicate)
    }

    /// Probe exact native identity without insertion or display allocation.
    pub(crate) fn term_id(&self, value: &TermValue) -> Option<TermId> {
        self.interner.lookup(value)
    }

    /// Probe a borrowed IRI without allocating an owned term.
    pub(crate) fn iri_id(&self, iri: &str) -> Option<TermId> {
        self.interner.lookup_iri(iri)
    }

    /// Whether the exact native tuple is present.
    pub(crate) fn contains(
        &self,
        predicate: &str,
        subject: &TermValue,
        object: &TermValue,
    ) -> bool {
        let (Some(s), Some(o)) = (self.term_id(subject), self.term_id(object)) else {
            return false;
        };
        self.relation(predicate).is_some_and(|r| r.contains(s, o))
    }

    /// A galloping lending [`RowCursor`] over the id rows under `predicate` selected by
    /// `bound`, in **row-id (insertion) order**.
    ///
    /// Yields interned `(subject_id, object_id, row_id)` rows (`Copy` — no `TermValue`
    /// clone) one at a time: the term ids for lazy surface resolution via
    /// [`interner`](Self::interner) where you stringify, and the store-global [`RowId`]
    /// for a one-word delta-bitset probe.  Picks the cheapest index for the bound
    /// positions; an unknown predicate yields an empty cursor (over the shared
    /// [`empty`](Self::empty) relation) — NO `Vec` is materialized.
    pub(crate) fn select(&self, predicate: &str, bound: Bound) -> RowCursor<'_> {
        self.relation(predicate)
            .unwrap_or(&self.empty)
            .select(bound)
    }

    /// A globally subject-value-ordered trie-level cursor over one predicate relation.
    ///
    /// `other` optionally fixes the object position. Unknown predicates use the
    /// permanent empty relation, matching [`Self::select`]'s probe-miss semantics.
    pub(crate) fn values_subject(
        &self,
        predicate: &str,
        other: Option<TermId>,
    ) -> ValueCursor<'_, VALUE_SUBJECT> {
        ValueCursor::new(self.relation(predicate).unwrap_or(&self.empty), other)
    }

    /// Object-value-ordered sibling of [`Self::values_subject`]; `other` optionally
    /// fixes the subject position.
    pub(crate) fn values_object(
        &self,
        predicate: &str,
        other: Option<TermId>,
    ) -> ValueCursor<'_, VALUE_OBJECT> {
        ValueCursor::new(self.relation(predicate).unwrap_or(&self.empty), other)
    }

    /// The number of distinct tuples stored under `predicate` (0 if unknown).
    pub(crate) fn len_for(&self, predicate: &str) -> usize {
        self.relation(predicate).map_or(0, Relation::row_count)
    }

    /// The relation for `predicate`, if interned (resolves `PredId` → slot).
    fn relation(&self, predicate: &str) -> Option<&Relation> {
        self.predicates
            .lookup(predicate)
            .and_then(|pid| self.relations.get(pid.index()))
    }

    /// Every predicate IRI surface that has at least one tuple, in sorted order.
    ///
    /// Resolves every interned [`PredId`] back to its string surface and sorts them
    /// LEXICALLY (through the `BTreeSet`) — NEVER by `PredId` mint order (id order is
    /// insertion order, not lexical order), so any "all relations" sweep is
    /// byte-deterministic.  Every interned predicate has ≥ 1 tuple (a `PredId` is
    /// minted only by [`insert`](Self::insert), which then adds the row).
    pub(crate) fn predicates(&self) -> impl Iterator<Item = &str> {
        self.predicates
            .names()
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            .into_iter()
    }

    /// Project every live row back to the shared ternary [`Fact`] IR in lexical
    /// [`Fact::key`] order.
    ///
    /// This is the single columnar-to-logical bridge used by the scratch backward
    /// evaluator and by the stateful incremental session bootstrap.  Keeping it here
    /// prevents those two consumers from growing subtly different seed ordering or
    /// term-resolution rules.
    pub(crate) fn facts_sorted(&self) -> Vec<Fact> {
        let mut facts = Vec::with_capacity(self.row_count);
        for pred in self.predicates() {
            let predicate = pred.to_owned();
            let mut cursor = self.select(pred, Bound::Any);
            while let Some((s_id, o_id, _row)) = cursor.next() {
                facts.push(Fact {
                    subject: self.interner.resolve(s_id).clone(),
                    predicate: predicate.clone(),
                    object: self.interner.resolve(o_id).clone(),
                });
            }
        }
        facts.sort_by_key(Fact::key);
        facts
    }
}

/// Extract the EDB of `world` from the blackboard into columnar form.
///
/// This is the SINGLE oxigraph → columnar bridge used by both the forward and
/// backward native engine paths.  It scans every quad in `world` via
/// [`WorldFactSource::in_world`] and inserts each `(subject, predicate, object)` as a
/// binary tuple.  Insertion order follows `in_world`'s iteration order; dedup and
/// index maintenance are handled by [`RelationStore::insert`].
pub(crate) fn extract_edb(
    foreign: &dyn WorldFactSource,
    world: &str,
) -> gmeow_errors::Result<RelationStore> {
    extract_edb_patterns(foreign, world, std::slice::from_ref(&WorldFactPattern::ANY))
}

/// Extract only the source patterns the compiled query can actually consume.
///
/// Patterns are assumed to have been deterministically minimized by the caller.
/// Their full `(S,P,O,G)` cardinality estimates are pushed into the source and used
/// to visit the smallest independent probe first; the lexical pattern is the stable
/// tie-break. Estimates never decide absence. Overlap is nevertheless harmless:
/// [`RelationStore::insert`] deduplicates the same RDF fact.
pub(crate) fn extract_edb_patterns(
    foreign: &dyn WorldFactSource,
    world: &str,
    patterns: &[WorldFactPattern],
) -> gmeow_errors::Result<RelationStore> {
    let mut store = RelationStore::new();
    visit_edb_patterns(foreign, world, patterns, &mut |quad| {
        store.insert(&quad.predicate, &quad.subject, &quad.object);
        Ok(())
    })?;
    Ok(store)
}

/// Visit a cardinality-ordered set of source patterns without an intermediate EDB.
///
/// This is the common direct-view ingestion loop for backward extraction and
/// selected forward materialization. The consumer owns deduplication because its
/// destination store already has the authoritative tuple identity.
pub(crate) fn visit_edb_patterns(
    foreign: &dyn WorldFactSource,
    world: &str,
    patterns: &[WorldFactPattern],
    visitor: &mut dyn FnMut(&DerivedQuad) -> gmeow_errors::Result<()>,
) -> gmeow_errors::Result<()> {
    let mut planned = patterns
        .iter()
        .map(|pattern| {
            Ok((
                foreign
                    .estimate_world(world, pattern)?
                    .unwrap_or(usize::MAX),
                pattern,
            ))
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    planned.sort_by(|(left_estimate, left), (right_estimate, right)| {
        left_estimate
            .cmp(right_estimate)
            .then_with(|| left.cmp(right))
    });
    for (_, pattern) in planned {
        foreign.visit_world(world, pattern, visitor)?;
    }
    Ok(())
}

#[path = "store.tests.rs"]
#[cfg(test)]
mod tests;
