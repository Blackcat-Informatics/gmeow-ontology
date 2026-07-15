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
//!   [`crate::provenance::term_display`] surface, so two terms share an id exactly when
//!   their display surfaces are byte-equal.  A batch's internal `(subject_id,
//!   object_id)` sort is by mint order — an INTERNAL storage order, never an emission
//!   order: the semi-naive winner selection is a total order over provenance (see
//!   [`crate::rule_ir::RuleRoundCandidate::tiebreak_key`]), so the order in which a
//!   cursor enumerates rows never reaches output.
//! - A join probe translates a ground surface to an id via
//!   [`RelationStore::term_id`] (non-inserting): a miss means the term has never
//!   entered the store, so the selection is empty — the single place that
//!   semantics lives.
//! - Any "all predicates" iteration is sorted (BTreeSet), never raw map order.
//!
//! # The single oxigraph → columnar bridge
//!
//! [`extract_edb`] is the SOLE place the forward and backward engine paths cross from
//! the oxigraph blackboard ([`crate::seam::WorldFactSource`]) into the columnar form.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::sync::OnceLock;

use purrdf::TermValue;

use crate::facts::{PredId, PredInterner, TermId, TermInterner, skolem_iri};
use crate::physical::cursor::{
    LendingIterator, RowCursor, VALUE_OBJECT, VALUE_SUBJECT, ValueCursor,
};
use crate::physical::id::RowId;
use crate::provenance::{ProvenanceSemiring, ZWeightSemiring, term_display};
use crate::rule_ir::Fact;
use crate::seam::{DerivedQuad, WorldFactPattern, WorldFactSource};

// ── Chase-invented nulls: recipe-carrying Skolem terms ──────────────────────────
//
// The existential chase value-invents a fresh witness for a head variable not
// bound by the body.  A witness is a **Skolem constant, not a blank node** (the
// same doctrine `relational_core` follows: the clausifier "mints Skolem constants,
// never blanks (no-optionality)").  Every witness IRI is minted through the single
// [`crate::facts::skolem_iri`] surface — the one restricted-chase value-invention
// interning point, so null identity has ONE source of truth. Other users of
// `skolem_iri` normalize pre-existing blank-node identifiers; they do not invent a
// second DL witness.
//
// Beyond the opaque interned IRI, each witness retains a **decomposable recipe**
// ([`SkolemRecipe`]).  An interned IRI is a hash and cannot be unified structurally;
// the recipe can.  Keeping it is what leaves the door open for later Skolem
// FUNCTIONS, full-FOL backward resolution, and provenance-semiring worlds — none of
// which an opaque hash could express.  It also drives recursive, order-independent
// null-blind comparison and an "explain invented individual" surface.

/// The decomposable recipe for a chase-invented null — a Skolem **function** of the
/// frontier binding, the standard restricted-chase witness.
///
/// The invented value depends on the bound frontier VALUES (never the lexical
/// variable names), so alpha-variant rules firing on the same data mint the same
/// null (`content_key` alpha-normalized identity), and two distinct frontier bindings
/// mint two distinct witnesses. A frontier
/// slot may itself be a prior invented null (a nested Skolem term), which stays
/// decomposable via the registry.  Termination is exactly weak acyclicity of the
/// rule set; the [`ChaseAdmission`](crate) certificate gates admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkolemTerm {
    /// The content-addressed firing rule IRI (already alpha-normalized).
    pub(crate) rule_iri: String,
    /// The existential head-variable ordinal (distinct ∃-vars ⇒ distinct witnesses).
    pub(crate) ordinal: usize,
    /// The bound frontier terms — the Skolem function's arguments, in a fixed order.
    pub(crate) frontier: Vec<TermValue>,
}

impl SkolemTerm {
    /// The content key hashed into the Skolem IRI.
    ///
    /// Length-prefixed (netstring-style) framing: every field is emitted as its
    /// byte length in decimal, a `\u{1f}` separator, then the field's raw bytes.
    /// Because a decoder reads the decimal length up to the first `\u{1f}` and
    /// then consumes EXACTLY that many bytes, no field value can forge a field
    /// boundary — whatever bytes it holds, `\u{1f}` included.  The frontier is
    /// preceded by its element COUNT, framed the same way, so a single term whose
    /// surface contains the separator can never masquerade as several terms.  The
    /// encoding is therefore injective: distinct `(rule_iri, ordinal, frontier)`
    /// tuples always yield distinct keys.  Frontier terms are rendered via
    /// [`term_display`] — their VALUE surface, never a source-variable name —
    /// preserving `content_key` alpha-normalized identity.
    fn content_key(&self) -> String {
        /// Append `field` as `<byte-len>\u{1f}<field-bytes>`.
        fn frame(out: &mut String, field: &str) {
            out.push_str(&field.len().to_string());
            out.push('\u{1f}');
            out.push_str(field);
        }
        let mut key = String::from("wa-skolem");
        frame(&mut key, &self.rule_iri);
        frame(&mut key, &self.ordinal.to_string());
        frame(&mut key, &self.frontier.len().to_string());
        for arg in &self.frontier {
            frame(&mut key, &term_display(arg));
        }
        key
    }

    /// The invented-null IRI surface for this recipe (deterministic, content-addressed).
    fn witness_iri(&self) -> String {
        skolem_iri(&self.content_key())
    }
}

/// A decomposable explanation of one chase-invented witness null — the Skolem
/// **function** application that minted it: the firing rule, the existential ordinal, and
/// the bound frontier VALUES it is addressed on.
///
/// A frontier value may itself be a prior invented null, so an explanation is recursively
/// decomposable through the same registry ([`SkolemRegistry::explain`]).  This is the
/// "explain invented individual" surface the recipe is retained per witness precisely to
/// support: an opaque interned IRI is a hash and cannot be decomposed; this can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessDerivation {
    /// The invented-null IRI surface being explained.
    pub witness: String,
    /// The content-addressed firing rule IRI that invented the witness.
    pub rule_iri: String,
    /// The existential head-variable ordinal (distinct ∃-vars ⇒ distinct witnesses).
    pub ordinal: usize,
    /// The Skolem-function arguments: the bound frontier terms, in a fixed order.
    pub frontier: Vec<TermValue>,
}

/// The witnesses a chase has invented, keyed by their IRI surface → recipe.
///
/// A `BTreeMap` so any full sweep is sorted/deterministic.  Minting the same recipe
/// twice is idempotent (re-firing an obligation on the same frontier recovers the
/// same witness — the restricted-chase blocking): the second mint returns the same
/// IRI and asserts the retained recipe is unchanged.
#[derive(Debug, Clone, Default)]
pub(crate) struct SkolemRegistry {
    recipes: BTreeMap<String, SkolemTerm>,
}

impl SkolemRegistry {
    /// A fresh, empty registry.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Mint (or recover) the witness for `recipe`, returning its `TermValue` IRI.
    ///
    /// Deterministic and idempotent: identical recipes collapse to the same witness,
    /// so re-firing an obligation never invents a fresh anonymous individual.
    pub(crate) fn mint(&mut self, recipe: SkolemTerm) -> TermValue {
        let iri = recipe.witness_iri();
        match self.recipes.get(&iri) {
            Some(existing) => debug_assert_eq!(
                existing, &recipe,
                "skolem IRI collision: two distinct recipes hashed to {iri}"
            ),
            None => {
                self.recipes.insert(iri.clone(), recipe);
            }
        }
        TermValue::iri(&iri)
    }

    /// Mint a DL-tableau witness, blocking a recursive existential obligation on
    /// the nearest ancestor produced by the same rule and existential ordinal.
    ///
    /// Production DL restrictions have one frontier subject. Keeping the first
    /// witness per root subject preserves distinct root obligations, while folding a
    /// later same-rule cycle onto its own ancestor yields a finite model for patterns
    /// such as `C subClassOf exists p.C`. General TGD chase callers continue to use
    /// [`Self::mint`] and retain ordinary frontier-Skolem semantics.
    pub(crate) fn mint_dl_blocked(&mut self, recipe: SkolemTerm) -> TermValue {
        if let Some(blocker) = self.dl_recursive_blocker(&recipe) {
            return blocker;
        }
        self.mint(recipe)
    }

    fn dl_recursive_blocker(&self, recipe: &SkolemTerm) -> Option<TermValue> {
        let [TermValue::Iri(frontier_iri)] = recipe.frontier.as_slice() else {
            return None;
        };
        let mut cursor = frontier_iri.as_str();
        let mut seen = BTreeSet::new();
        while seen.insert(cursor.to_owned()) {
            let ancestor = self.recipes.get(cursor)?;
            if ancestor.rule_iri == recipe.rule_iri && ancestor.ordinal == recipe.ordinal {
                return Some(TermValue::iri(cursor));
            }
            let [TermValue::Iri(parent)] = ancestor.frontier.as_slice() else {
                return None;
            };
            cursor = parent;
        }
        None
    }

    /// The recipe behind an invented-null IRI surface, if this registry minted it.
    pub(crate) fn recipe(&self, iri: &str) -> Option<&SkolemTerm> {
        self.recipes.get(iri)
    }

    /// Explain a chase-invented null: recover its decomposable derivation — the firing
    /// rule, the existential ordinal, and the bound frontier values — from the retained
    /// recipe, or `None` if this registry never minted `iri`.
    ///
    /// The single "explain invented individual" surface: it is non-vacuous because every
    /// witness retains its Skolem-function recipe, so an invented individual can always be
    /// decomposed back to the rule firing and frontier binding that produced it.
    pub(crate) fn explain(&self, iri: &str) -> Option<WitnessDerivation> {
        self.recipe(iri).map(|recipe| WitnessDerivation {
            witness: iri.to_owned(),
            rule_iri: recipe.rule_iri.clone(),
            ordinal: recipe.ordinal,
            frontier: recipe.frontier.clone(),
        })
    }

    /// Whether `term` is a null this registry invented.
    pub(crate) fn is_invented(&self, term: &TermValue) -> bool {
        matches!(term, TermValue::Iri(iri) if self.recipes.contains_key(iri))
    }

    /// The number of distinct witnesses invented so far.
    pub(crate) fn len(&self) -> usize {
        self.recipes.len()
    }

    /// Whether no witness has been invented yet.
    pub(crate) fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Every invented-null IRI surface, in sorted order.
    pub(crate) fn witnesses(&self) -> impl Iterator<Item = &str> {
        self.recipes.keys().map(String::as_str)
    }
}

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
            interner: TermInterner::new(),
            predicates: PredInterner::new(),
            relations: Vec::new(),
            row_count: 0,
            empty: Relation::default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::cursor::LendingIterator;
    use crate::seam::{
        BudgetStatus, DerivationId, DerivedQuad, WorldFactPattern, WorldSourceIdentity,
    };

    fn term(iri: &str) -> TermValue {
        TermValue::iri(iri)
    }

    /// Drain a [`RowCursor`] into a `Vec` of id rows — a `#[cfg(test)]`-only helper for
    /// asserting a selection's full sequence.  `select` now returns a lending cursor
    /// (no eager `Vec` on the production hot path), so tests collect it here.
    fn select_rows(
        s: &RelationStore,
        predicate: &str,
        bound: Bound,
    ) -> Vec<(TermId, TermId, RowId)> {
        let mut cursor = s.select(predicate, bound);
        let mut rows = Vec::new();
        while let Some(row) = cursor.next() {
            rows.push(row);
        }
        rows
    }

    /// The interned id for a display surface, asserting it is present.
    fn id_of(s: &RelationStore, display: &str) -> TermId {
        s.term_id(display)
            .unwrap_or_else(|| panic!("term {display:?} must be interned"))
    }

    /// Resolve selected `(subject_id, object_id, row_id)` id rows back to their
    /// `TermValue` surfaces via the store's interner — `select` returns ids only, never
    /// cloned terms, so a caller resolves lazily exactly here (the `RowId` is the delta
    /// probe key, not a surface, so it is dropped for the surface assertions).
    fn resolved(
        s: &RelationStore,
        rows: &[(TermId, TermId, RowId)],
    ) -> Vec<(TermValue, TermValue)> {
        rows.iter()
            .map(|&(si, oi, _row)| {
                (
                    s.interner().resolve(si).clone(),
                    s.interner().resolve(oi).clone(),
                )
            })
            .collect()
    }

    /// Build a store with a small `knows`/`likes` corpus.
    ///
    /// `knows`: (a,b), (a,c), (b,c)  — `likes`: (a,c)
    fn sample_store() -> RelationStore {
        let knows = "http://ex/knows";
        let likes = "http://ex/likes";
        let mut s = RelationStore::new();
        assert!(
            s.insert(knows, &term("http://ex/a"), &term("http://ex/b"))
                .is_some()
        );
        assert!(
            s.insert(knows, &term("http://ex/a"), &term("http://ex/c"))
                .is_some()
        );
        assert!(
            s.insert(knows, &term("http://ex/b"), &term("http://ex/c"))
                .is_some()
        );
        assert!(
            s.insert(likes, &term("http://ex/a"), &term("http://ex/c"))
                .is_some()
        );
        s
    }

    /// Resolve selected id rows to an ORDER-INDEPENDENT `(subject, object)` surface set.
    ///
    /// The arrangement enumerates batch-then-tail (an internal storage order), NOT a
    /// stable emission order — winner selection, a total order over provenance, fixes
    /// output — so a store test asserts the row SET, never a sequence.
    fn resolved_set(
        s: &RelationStore,
        rows: &[(TermId, TermId, RowId)],
    ) -> BTreeSet<(String, String)> {
        resolved(s, rows)
            .into_iter()
            .map(|(a, b)| (format!("{a:?}"), format!("{b:?}")))
            .collect()
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (format!("{:?}", term(a)), format!("{:?}", term(b)))
    }

    #[test]
    fn physical_select_subject_bound() {
        let s = sample_store();
        let a = id_of(&s, "<http://ex/a>");
        let got = select_rows(&s, "http://ex/knows", Bound::Subject(a));
        assert_eq!(
            resolved_set(&s, &got),
            [
                pair("http://ex/a", "http://ex/b"),
                pair("http://ex/a", "http://ex/c"),
            ]
            .into()
        );
    }

    #[test]
    fn physical_select_object_bound() {
        let s = sample_store();
        let c = id_of(&s, "<http://ex/c>");
        let got = select_rows(&s, "http://ex/knows", Bound::Object(c));
        assert_eq!(
            resolved_set(&s, &got),
            [
                pair("http://ex/a", "http://ex/c"),
                pair("http://ex/b", "http://ex/c"),
            ]
            .into()
        );
    }

    #[test]
    fn physical_select_both_bound() {
        let s = sample_store();
        let a = id_of(&s, "<http://ex/a>");
        let b = id_of(&s, "<http://ex/b>");
        let c = id_of(&s, "<http://ex/c>");
        let got = select_rows(&s, "http://ex/knows", Bound::Both(a, c));
        assert_eq!(
            resolved_set(&s, &got),
            [pair("http://ex/a", "http://ex/c")].into()
        );

        // A both-bound miss (b is interned but (b,b) is not a tuple) yields nothing.
        let none = select_rows(&s, "http://ex/knows", Bound::Both(b, b));
        assert!(none.is_empty());
    }

    #[test]
    fn physical_select_any_yields_every_row() {
        let s = sample_store();
        let got = select_rows(&s, "http://ex/knows", Bound::Any);
        assert_eq!(
            resolved_set(&s, &got),
            [
                pair("http://ex/a", "http://ex/b"),
                pair("http://ex/a", "http://ex/c"),
                pair("http://ex/b", "http://ex/c"),
            ]
            .into()
        );
    }

    #[test]
    fn physical_dedup_returns_false_and_stores_one_row() {
        let knows = "http://ex/knows";
        let mut s = RelationStore::new();
        assert!(
            s.insert(knows, &term("http://ex/a"), &term("http://ex/b"))
                .is_some()
        );
        // Re-inserting the same (s,p,o) is a no-op that reports None (no new row id).
        assert!(
            s.insert(knows, &term("http://ex/a"), &term("http://ex/b"))
                .is_none()
        );
        assert_eq!(s.len_for("http://ex/knows"), 1);
        assert_eq!(
            resolved_set(&s, &select_rows(&s, "http://ex/knows", Bound::Any)),
            [pair("http://ex/a", "http://ex/b")].into(),
        );
    }

    /// `insert` stamps each newly-inserted row with a dense [`RowId`] in store-wide
    /// insertion order — `0, 1, 2, …` ACROSS relations, not per-relation — and `select`
    /// hands each selected row that same id.  A dedup returns `None` (no id consumed), so
    /// the id space stays gap-free and `row_count` counts exactly the live rows.  The row
    /// ids are asserted as SETS (the arrangement stores rows value-sorted, so a selection
    /// enumerates them in storage order, not insertion order).
    #[test]
    fn physical_insert_assigns_dense_cross_relation_row_ids() {
        let knows = "http://ex/knows";
        let likes = "http://ex/likes";
        let mut s = RelationStore::new();
        // Interleave predicates so a per-relation index would NOT match the global RowId.
        let r0 = s
            .insert(knows, &term("http://ex/a"), &term("http://ex/b"))
            .map(|(_, _, r)| r);
        let r1 = s
            .insert(likes, &term("http://ex/a"), &term("http://ex/c"))
            .map(|(_, _, r)| r);
        let r2 = s
            .insert(knows, &term("http://ex/a"), &term("http://ex/c"))
            .map(|(_, _, r)| r);
        assert_eq!(r0, Some(RowId::from_index(0)));
        assert_eq!(r1, Some(RowId::from_index(1)));
        assert_eq!(
            r2,
            Some(RowId::from_index(2)),
            "RowIds span relations in insertion order"
        );
        // A dedup consumes no RowId — the space stays dense and `row_count` is exact.
        assert_eq!(
            s.insert(knows, &term("http://ex/a"), &term("http://ex/b")),
            None
        );
        assert_eq!(s.row_count(), 3, "three distinct rows ⇒ RowIds 0..3");
        // `select` hands each row its store-global RowId (never a per-relation index):
        // knows carries ids {0, 2}, likes carries {1} — the interleaved likes took id 1.
        let knows_ids: BTreeSet<RowId> = select_rows(&s, knows, Bound::Any)
            .iter()
            .map(|&(_, _, r)| r)
            .collect();
        assert_eq!(
            knows_ids,
            [RowId::from_index(0), RowId::from_index(2)].into(),
            "selected rows carry their store-global RowId, not a per-relation index",
        );
        let likes_ids: BTreeSet<RowId> = select_rows(&s, likes, Bound::Any)
            .iter()
            .map(|&(_, _, r)| r)
            .collect();
        assert_eq!(likes_ids, [RowId::from_index(1)].into());
    }

    /// The arrangement seals its tail into sorted batches past the threshold and still
    /// returns the exact row SET (with the exact store-global RowIds) — the galloping
    /// batch path, not just the tail leg.  A heavily-interleaved build (RowIds NOT
    /// contiguous within a relation) confirms every selected row carries its dense global
    /// id and `row_count` stays exact across relations.
    #[test]
    fn physical_sealed_batches_preserve_row_set_and_dense_ids() {
        let (p, q) = ("http://ex/p", "http://ex/q");
        let mut s = RelationStore::new();
        // Interleave p and q for > 2*threshold rows so BOTH relations seal batches and
        // neither relation's RowIds are the contiguous 0,1,2,….
        let n = super::TAIL_SEAL_THRESHOLD * 3;
        for i in 0..n {
            let pred = if i % 2 == 0 { p } else { q };
            assert!(
                s.insert(
                    pred,
                    &term("http://ex/s"),
                    &term(&format!("http://ex/o{i:04}"))
                )
                .is_some()
            );
        }
        assert_eq!(s.row_count(), n, "every distinct row is counted, gap-free");
        // Each relation returns exactly its half of the rows, each with the global RowId
        // it was stamped with at insert (the even indices went to p, odd to q).
        let p_ids: BTreeSet<RowId> = select_rows(&s, p, Bound::Any)
            .iter()
            .map(|&(_, _, r)| r)
            .collect();
        let expect_p: BTreeSet<RowId> = (0..n).step_by(2).map(RowId::from_index).collect();
        assert_eq!(p_ids, expect_p, "p carries exactly the even-index RowIds");
        // A subject-bound gallop over the sealed batches finds every one of s's edges.
        let subj = id_of(&s, "<http://ex/s>");
        assert_eq!(
            select_rows(&s, p, Bound::Subject(subj)).len(),
            n / 2,
            "subject gallop over sealed batches finds all rows"
        );
        // Dedup still holds across sealed batches: re-inserting a sealed row is a no-op.
        assert!(
            s.insert(p, &term("http://ex/s"), &term("http://ex/o0000"))
                .is_none(),
            "a row already sealed into a batch is deduped by the galloping probe"
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
            resolved_set(&s, &select_rows(&s, "http://ex/likes", Bound::Subject(a))),
            [pair("http://ex/a", "http://ex/c")].into(),
        );
    }

    /// Emission-order guard: the `relations` table is a `PredId`-indexed `Vec`, so its
    /// slot order is mint order.  The ONLY consumer-facing enumeration —
    /// [`RelationStore::predicates`] — MUST still be lexical, sorted through the
    /// `BTreeSet` sweep, NEVER leaking mint order.  Insert predicates in deliberately
    /// anti-lexical order and assert the output is lexical regardless.
    #[test]
    fn physical_predicates_never_leak_hasher_order() {
        let mut s = RelationStore::new();
        // Insert in reverse-lexical order; a raw hash-map sweep would not be sorted.
        for pred in ["http://ex/zeta", "http://ex/mu", "http://ex/alpha"] {
            assert!(
                s.insert(pred, &term("http://ex/x"), &term("http://ex/y"))
                    .is_some()
            );
        }
        let preds: Vec<&str> = s.predicates().collect();
        assert_eq!(
            preds,
            vec!["http://ex/alpha", "http://ex/mu", "http://ex/zeta"],
            "predicates() must be lexical — the PredId mint order must never leak"
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
            select_rows(&s, "http://ex/knows", Bound::Any),
            select_rows(&s2, "http://ex/knows", Bound::Any),
        );
        let p2: Vec<&str> = s2.predicates().collect();
        assert_eq!(preds, p2);
    }

    // ── extract_edb round-trip via a minimal WorldFactSource test double ───────────

    /// A hand-rolled `WorldFactSource` yielding a fixed list of `DerivedQuad`s in
    /// `world`. Only `in_world` is exercised by `extract_edb`; the other legs are
    /// vacuous (and unused) for this test.
    struct FakeForeign {
        world: String,
        quads: Vec<DerivedQuad>,
        identity: WorldSourceIdentity,
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
                identity: WorldSourceIdentity::new("test-generation", "test-contract"),
            }
        }
    }

    impl WorldFactSource for FakeForeign {
        fn identity(&self) -> &WorldSourceIdentity {
            &self.identity
        }

        fn visit_world(
            &self,
            world: &str,
            pattern: &WorldFactPattern,
            visitor: &mut dyn FnMut(&DerivedQuad) -> gmeow_errors::Result<()>,
        ) -> gmeow_errors::Result<()> {
            for quad in &self.quads {
                if quad.graph == world
                    && pattern
                        .subject
                        .as_ref()
                        .is_none_or(|subject| &quad.subject == subject)
                    && pattern
                        .predicate
                        .as_ref()
                        .is_none_or(|predicate| &quad.predicate == predicate)
                    && pattern
                        .object
                        .as_ref()
                        .is_none_or(|object| &quad.object == object)
                {
                    visitor(quad)?;
                }
            }
            Ok(())
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
        let edb = extract_edb(&foreign, &foreign.world).expect("extract test EDB");

        let preds: Vec<&str> = edb.predicates().collect();
        assert_eq!(preds, vec!["http://ex/knows", "http://ex/likes"]);
        assert_eq!(edb.len_for("http://ex/knows"), 2);
        assert_eq!(edb.len_for("http://ex/likes"), 1);
        assert_eq!(
            resolved_set(&edb, &select_rows(&edb, "http://ex/knows", Bound::Any)),
            [
                pair("http://ex/a", "http://ex/b"),
                pair("http://ex/a", "http://ex/c"),
            ]
            .into()
        );
        assert!(edb.contains("http://ex/likes", "<http://ex/a>", "<http://ex/c>"));
    }

    // ── The Z-set seam: signed-weight consolidation (compiled + exercised) ───────

    /// Build a single-row `Batch<i64>` with an explicit signed weight — the seam a
    /// signed delta rides.  (Set-semantics `insert` never mints a non-unit weight, so
    /// the seam is exercised here by constructing weighted batches directly.)
    fn weighted_row(s: TermId, o: TermId, r: RowId, w: i64) -> Batch<i64> {
        Batch {
            subj: vec![s],
            obj: vec![o],
            row_id: vec![r],
            weight: vec![w],
            object_index: OnceLock::new(),
        }
    }

    /// The consolidation merge is generic over the [`Weight`] monoid and compiles for
    /// `W = i64` (a Z-set): a `+1` and a `-1` on the SAME key combine to `0`, which
    /// annihilates and DROPS the row — retraction falls out of the same merge, no
    /// special deletion pass.  This proves "the representation admits signed weights" is
    /// a compiled, exercised fact, not a promise; production stays at the ZST `W = ()`.
    #[test]
    fn batch_merge_is_a_z_set_over_signed_weights() {
        let s = TermId::from_index(0);
        let o = TermId::from_index(1);

        // (+1) + (-1) = 0 ⇒ the row annihilates and is dropped (retraction).
        let plus = weighted_row(s, o, RowId::from_index(5), 1);
        let minus = weighted_row(s, o, RowId::from_index(2), -1);
        let retracted = merge_batches(&plus, &minus).expect("signed retraction combines");
        assert_eq!(
            retracted.len(),
            0,
            "(+1)+(-1)=0 annihilates the shared-key row"
        );

        // (+1) + (+2) = 3 ⇒ one surviving row, weights summed, LOWER RowId kept (R4).
        let two = weighted_row(s, o, RowId::from_index(2), 2);
        let summed = merge_batches(&plus, &two).expect("signed addition combines");
        assert_eq!(summed.len(), 1, "a non-annihilating combine keeps one row");
        assert_eq!(summed.weight[0], 3, "weights sum: 1 + 2 = 3");
        assert_eq!(
            summed.row_id[0],
            RowId::from_index(2),
            "the lower RowId deterministically survives a key collision"
        );

        // Disjoint keys interleave with NO combine — the set-semantics `W = ()` shape.
        let o2 = TermId::from_index(2);
        let a = weighted_row(s, o, RowId::from_index(0), 1);
        let b = weighted_row(s, o2, RowId::from_index(1), 1);
        let disjoint = merge_batches(&a, &b).expect("disjoint signed batches interleave");
        assert_eq!(
            disjoint.len(),
            2,
            "disjoint keys interleave, no combine fires"
        );
    }

    /// Saturation is not a ring operation (and is not associative across mixed-sign
    /// updates), so overflow must hard-fail instead of silently changing the Z-set.
    #[test]
    fn signed_weight_overflow_never_saturates() {
        let err = i64::MAX
            .combine(1)
            .expect_err("signed overflow must be a structured failure");
        assert!(err.message().contains("overflow"), "{err}");
        assert!(err.message().contains("addition"), "{err}");
    }

    // ── Chase-invented Skolem-term nulls ─────────────────────────────────────────

    fn witness(ordinal: usize, frontier: Vec<TermValue>) -> SkolemTerm {
        SkolemTerm {
            rule_iri: "http://ex/rule".to_owned(),
            ordinal,
            frontier,
        }
    }

    #[test]
    fn skolem_mint_is_deterministic_and_idempotent() {
        let mut reg = SkolemRegistry::new();
        let a = reg.mint(witness(0, vec![term("http://ex/a")]));
        // Re-firing on the SAME frontier recovers the SAME witness (restricted-chase
        // blocking) and does not grow the registry — the fixpoint's teeth.
        let b = reg.mint(witness(0, vec![term("http://ex/a")]));
        assert_eq!(a, b);
        assert_eq!(reg.len(), 1);
        assert!(reg.is_invented(&a));
    }

    #[test]
    fn skolem_distinct_frontiers_give_distinct_witnesses() {
        // The standard restricted chase mints one fresh witness per frontier binding
        // Distinct frontier values imply distinct nulls.
        let mut reg = SkolemRegistry::new();
        let wa = reg.mint(witness(0, vec![term("http://ex/a")]));
        let wb = reg.mint(witness(0, vec![term("http://ex/b")]));
        assert_ne!(wa, wb);
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn skolem_distinct_ordinals_give_distinct_witnesses() {
        // The n distinct existential vars of `≥n p.D` (same frontier) are distinct.
        let mut reg = SkolemRegistry::new();
        let w0 = reg.mint(witness(0, vec![term("http://ex/a")]));
        let w1 = reg.mint(witness(1, vec![term("http://ex/a")]));
        assert_ne!(w0, w1);
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn skolem_addresses_on_values_not_variable_names() {
        // The Skolem function keys on the bound frontier VALUES.  Two firings of
        // alpha-variant rules (?x vs ?y) that bind the SAME data mint the byte-identical
        // null — `content_key` alpha-normalized identity (no lexical name in the key).
        let mut reg = SkolemRegistry::new();
        let frontier = vec![term("http://ex/a"), term("http://ex/b")];
        let a = reg.mint(witness(0, frontier.clone()));
        let b = reg.mint(witness(0, frontier));
        assert_eq!(a, b);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn skolem_recipe_round_trips_and_nests() {
        // The IRI → recipe lookup recovers the structured recipe (decomposability),
        // and a frontier slot may itself be a prior invented null (nested Skolem term)
        // that is still decomposable through the registry.
        let mut reg = SkolemRegistry::new();
        let inner = reg.mint(witness(0, vec![term("http://ex/a")]));
        let inner_iri = match &inner {
            TermValue::Iri(s) => s.clone(),
            _ => unreachable!("mint returns an IRI"),
        };
        let outer = reg.mint(witness(0, vec![inner.clone()]));
        let outer_iri = match &outer {
            TermValue::Iri(s) => s.clone(),
            _ => unreachable!(),
        };

        // The outer recipe decomposes to reveal the inner null in its frontier…
        let outer_recipe = reg.recipe(&outer_iri).expect("outer recipe retained");
        assert_eq!(outer_recipe.frontier, vec![inner]);
        // …and the inner null is itself decomposable (its own frontier is `a`).
        let inner_recipe = reg.recipe(&inner_iri).expect("inner recipe retained");
        assert_eq!(inner_recipe.frontier, vec![term("http://ex/a")]);
        // A term this registry never minted is not recognized as invented.
        assert!(!reg.is_invented(&term("http://ex/a")));
    }

    #[test]
    fn skolem_content_key_is_injective_across_frontier_shapes() {
        // A frontier term whose `term_display` surface itself contains the field
        // separator MUST NOT be able to forge a boundary.  `term("a>\u{1f}<b")`
        // renders as `<a>\u{1f}<b>` — byte-identical to the two-term frontier
        // `[term("a"), term("b")]` rendered as `<a>` `\u{1f}` `<b>` joined.  Under a
        // naive separator-joined key these two DISTINCT recipes collide to one
        // witness; the length-prefixed encoding keeps them distinct.
        let one = witness(0, vec![term("a>\u{1f}<b")]);
        let two = witness(0, vec![term("a"), term("b")]);
        assert_ne!(
            one.witness_iri(),
            two.witness_iri(),
            "distinct frontier recipes must mint distinct witnesses"
        );

        // The same collision, driven through the registry: two mints, two witnesses.
        let mut reg = SkolemRegistry::new();
        let w_one = reg.mint(one);
        let w_two = reg.mint(two);
        assert_ne!(w_one, w_two);
        assert_eq!(reg.len(), 2);
    }
}
