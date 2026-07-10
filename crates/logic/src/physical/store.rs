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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use foldhash::fast::FixedState;
use purrdf::TermValue;

use crate::facts::{PredId, PredInterner, TermId, TermInterner, skolem_iri};
use crate::physical::cursor::RowCursor;
use crate::physical::id::RowId;
use crate::provenance::term_display;
use crate::seam::ScryerForeign;

// ── Chase-invented nulls: recipe-carrying Skolem terms ──────────────────────────
//
// The existential chase value-invents a fresh witness for a head variable not
// bound by the body.  A witness is a **Skolem constant, not a blank node** (the
// same doctrine `relational_core` follows: the clausifier "mints Skolem constants,
// never blanks (no-optionality)").  Every witness IRI is minted through the single
// [`crate::facts::skolem_iri`] surface — the one value-invention interning point,
// shared with `reason/dl.rs`'s TBox witness pass — so null identity has ONE source
// of truth.
//
// Beyond the opaque interned IRI, each witness retains a **decomposable recipe**
// ([`SkolemRecipe`]).  An interned IRI is a hash and cannot be unified structurally;
// the recipe can.  Keeping it is what leaves the door open for later Skolem
// FUNCTIONS, full-FOL backward resolution, and provenance-semiring worlds — none of
// which an opaque hash could express.  It also drives recursive, order-independent
// null-blind parity against Nemo and an "explain invented individual" surface.

/// The decomposable recipe for a chase-invented null — a Skolem **function** of the
/// frontier binding, the standard restricted-chase witness.
///
/// The invented value depends on the bound frontier VALUES (never the lexical
/// variable names), so alpha-variant rules firing on the same data mint the same
/// null (`content_key` alpha-normalized identity), and — matching Nemo's restricted
/// chase — two distinct frontier bindings mint two distinct witnesses.  A frontier
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
pub(crate) struct WitnessDerivation {
    /// The invented-null IRI surface being explained.
    pub(crate) witness: String,
    /// The content-addressed firing rule IRI that invented the witness.
    pub(crate) rule_iri: String,
    /// The existential head-variable ordinal (distinct ∃-vars ⇒ distinct witnesses).
    pub(crate) ordinal: usize,
    /// The Skolem-function arguments: the bound frontier terms, in a fixed order.
    pub(crate) frontier: Vec<TermValue>,
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

/// A single binary relation: `(subject, object)` tuples for ONE predicate IRI.
///
/// Insertion-ordered, O(1)-deduped on the interned tuple key, with subject/object
/// indexes maintained in lockstep — the column-oriented sibling of `FactStore`'s
/// predicate bucket.  Term interning lives at the [`RelationStore`] level (one
/// dictionary shared by every relation), so `insert` borrows the store's interner.
///
/// `pub(crate)` (its fields stay private) so the arrangement's native lending cursor
/// [`crate::physical::cursor::RowCursor`] can borrow it and resolve rows via
/// [`row_at`](Self::row_at); the cursor is the SOLE row-materialization path.
#[derive(Debug, Clone, Default)]
pub(crate) struct Relation {
    /// `(subject_id, object_id)` tuples in insertion order.
    ///
    /// Rows hold interned [`TermId`]s ONLY — never cloned [`TermValue`]s (greenfield:
    /// the owned-term row path is deleted).  A `TermId` is a `Copy` niche integer, so a
    /// row is 8 bytes and `select` copies (never clones/allocates) the matching rows; a
    /// caller that needs a surface resolves it lazily via the store's
    /// [`TermInterner`](RelationStore::interner) at the point it actually stringifies
    /// (head grounding / provenance), which is exactly where a surface is already required.
    rows: Vec<(TermId, TermId)>,
    /// The store-global dense [`RowId`] of each row, parallel to [`rows`](Self::rows).
    ///
    /// A [`RowId`] is assigned once by [`RelationStore::insert`] in store-wide insertion
    /// order (spanning every relation), so it is a stable, cross-relation row identity —
    /// the index space the semi-naive delta bitset
    /// ([`crate::physical::bitset::DenseBitset`]) is keyed on.  Held here so
    /// [`select`](Self::select) can hand each selected row its id for a one-word delta
    /// probe, with no re-hashing of the tuple surface.
    row_ids: Vec<RowId>,
    /// Dedup keys `(subject_id, object_id)` for O(1) membership.
    ///
    /// Fixed-seed hashed (`FixedState`) — off std's SipHash; the id keys are `Copy`
    /// so a probe never clones. Determinism never comes from this set's order.
    keys: HashSet<(TermId, TermId), FixedState>,
    /// Subject term id → row indices into `rows`, in insertion order.
    by_subject: HashMap<TermId, Vec<usize>, FixedState>,
    /// Object term id → row indices into `rows`, in insertion order.
    by_object: HashMap<TermId, Vec<usize>, FixedState>,
}

impl Relation {
    /// Insert `(subject, object)` if its interned key is new, stamping it with the
    /// store-assigned `row_id`; return `Some((subject_id, object_id))` with the terms'
    /// interned ids if the row was newly inserted, or `None` if the key was already
    /// present (dedup miss).
    ///
    /// On a successful insert the new row index is appended to BOTH indexes in
    /// lockstep with `rows` (so each bucket's order equals insertion order), and the
    /// store-global [`RowId`] is recorded parallel to the row.  Returning the interned
    /// ids lets the caller thread them onward without a second interner lookup.
    fn insert(
        &mut self,
        interner: &mut TermInterner,
        subject: &TermValue,
        object: &TermValue,
        row_id: RowId,
    ) -> Option<(TermId, TermId)> {
        let s_id = interner.intern(subject);
        let o_id = interner.intern(object);
        if !self.keys.insert((s_id, o_id)) {
            return None;
        }
        let idx = self.rows.len();
        self.rows.push((s_id, o_id));
        self.row_ids.push(row_id);
        self.by_subject.entry(s_id).or_default().push(idx);
        self.by_object.entry(o_id).or_default().push(idx);
        Some((s_id, o_id))
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

    /// The number of rows in this relation — the length of the [`Bound::Any`] full
    /// scan's implicit `0..row_count` posting run.
    #[inline]
    pub(crate) fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Resolve a relation-local row index to its `(subject_id, object_id, row_id)` id
    /// row.
    ///
    /// The single row-materialization point [`RowCursor`] yields through: all `Copy`
    /// ids, so this copies — never clones a `TermValue`.
    #[inline]
    pub(crate) fn row_at(&self, i: usize) -> (TermId, TermId, RowId) {
        let (s, o) = self.rows[i];
        (s, o, self.row_ids[i])
    }

    /// A galloping lending [`RowCursor`] over the `(subject_id, object_id, row_id)` id
    /// rows selected by `bound`, in **row-id (insertion) order**.
    ///
    /// The cursor yields interned [`TermId`] rows plus each row's store-global
    /// [`RowId`] one at a time, borrowing this relation's columns — NO per-stage `Vec`
    /// is materialized (the eager-`Vec` `select_*` kernels are
    /// deleted, greenfield).  A caller resolves term surfaces lazily via the store's
    /// interner only where it stringifies, and tests delta membership on the `RowId`
    /// with a single word probe.
    ///
    /// # Adornment dispatched ONCE per call, never per row
    ///
    /// The [`Bound`] shape (the query-plan *adornment*) is resolved by a SINGLE `match`
    /// here — once per `select` invocation — into the specialized cursor constructor
    /// ([`RowCursor::any`] / [`subject`](RowCursor::subject) /
    /// [`object`](RowCursor::object) / [`select_both`](RowCursor::select_both)).  The
    /// shape IS the cursor, so no residual adornment branch survives on the per-row
    /// path.  Dispatch is a plain enum `match`, never a trait object.  `Both` drives a
    /// leapfrog intersection of the two buckets inside the cursor.
    fn select(&self, bound: Bound) -> RowCursor<'_> {
        match bound {
            Bound::Any => RowCursor::any(self),
            Bound::Subject(s) => RowCursor::subject(self, self.rows_for_subject(s)),
            Bound::Object(o) => RowCursor::object(self, self.rows_for_object(o)),
            Bound::Both(s, o) => {
                RowCursor::select_both(self, self.rows_for_subject(s), self.rows_for_object(o))
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

    /// The number of distinct tuples stored under `predicate` (0 if unknown).
    pub(crate) fn len_for(&self, predicate: &str) -> usize {
        self.relation(predicate).map_or(0, |r| r.rows.len())
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
        store.insert(&dq.predicate, &dq.subject, &dq.object);
    }
    store
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::cursor::LendingIterator;
    use crate::seam::{BudgetStatus, DerivationId, DerivedQuad};

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

    #[test]
    fn physical_select_subject_bound() {
        let s = sample_store();
        let a = id_of(&s, "<http://ex/a>");
        let got = select_rows(&s, "http://ex/knows", Bound::Subject(a));
        assert_eq!(
            resolved(&s, &got),
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
        let got = select_rows(&s, "http://ex/knows", Bound::Object(c));
        assert_eq!(
            resolved(&s, &got),
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
        let got = select_rows(&s, "http://ex/knows", Bound::Both(a, c));
        assert_eq!(
            resolved(&s, &got),
            vec![(term("http://ex/a"), term("http://ex/c"))]
        );

        // A both-bound miss (b is interned but (b,b) is not a tuple) yields nothing.
        let none = select_rows(&s, "http://ex/knows", Bound::Both(b, b));
        assert!(none.is_empty());
    }

    #[test]
    fn physical_select_any_is_insertion_order() {
        let s = sample_store();
        let got = select_rows(&s, "http://ex/knows", Bound::Any);
        assert_eq!(
            resolved(&s, &got),
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
            resolved(&s, &select_rows(&s, "http://ex/knows", Bound::Any)),
            vec![(term("http://ex/a"), term("http://ex/b"))],
        );
    }

    /// `insert` stamps each newly-inserted row with a dense [`RowId`] in store-wide
    /// insertion order — `0, 1, 2, …` ACROSS relations, not per-relation — and `select`
    /// hands each selected row that same id.  A dedup returns `None` (no id consumed), so
    /// the id space stays gap-free and `row_count` counts exactly the live rows.
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
        // `select` returns each row with its store-global RowId: knows row 0 is id 0,
        // knows row 1 is id 2 (the interleaved likes insert took id 1).
        let knows_rows = select_rows(&s, knows, Bound::Any);
        assert_eq!(
            knows_rows.iter().map(|&(_, _, r)| r).collect::<Vec<_>>(),
            vec![RowId::from_index(0), RowId::from_index(2)],
            "selected rows carry their store-global RowId, not a per-relation index",
        );
        let likes_rows = select_rows(&s, likes, Bound::Any);
        assert_eq!(
            likes_rows.iter().map(|&(_, _, r)| r).collect::<Vec<_>>(),
            vec![RowId::from_index(1)],
        );
    }

    /// THE BYTE-IDENTITY INVARIANT: within ONE relation,
    /// row-INDEX order and store-global [`RowId`] order COINCIDE.
    ///
    /// `row_ids[idx]` is assigned once per successful store-wide `insert` (a strictly
    /// increasing global counter), and a single relation only ever grows by appending,
    /// so its `row_ids` are strictly increasing in `idx`.  This is the load-bearing
    /// fact that makes the galloping cursor's "row-id-ordered value runs" claim hold:
    /// galloping over ascending row-INDEX positions (the `by_subject`/`by_object`
    /// buckets, and the full scan's `0..len`) IS galloping in RowId order, so the
    /// leading full scan iterates row-id order with NO key-sorted reordering.  We build
    /// a heavily-interleaved store (so RowIds are NOT `0,1,2,…` within a relation) and
    /// assert every relation's `row_ids` still ascend in lockstep with `idx`.
    #[test]
    fn physical_row_index_order_coincides_with_row_id_order() {
        let (p, q) = ("http://ex/p", "http://ex/q");
        let mut s = RelationStore::new();
        // Interleave p and q so neither relation's RowIds are the contiguous 0,1,2,….
        for i in 0..6 {
            let pred = if i % 2 == 0 { p } else { q };
            assert!(
                s.insert(
                    pred,
                    &term("http://ex/s"),
                    &term(&format!("http://ex/o{i}"))
                )
                .is_some()
            );
        }
        // Every relation's parallel `row_ids` is strictly increasing in the row index —
        // the direct empirical statement of the invariant.
        for rel in &s.relations {
            for w in rel.row_ids.windows(2) {
                assert!(
                    w[0] < w[1],
                    "row_ids must strictly increase in row-index order within a relation"
                );
            }
        }
        // And a full scan (`Bound::Any`, the leading-atom scan) therefore yields RowIds
        // in ascending (= row-index = insertion) order — never a key-sorted order.
        for pred in [p, q] {
            let ids: Vec<RowId> = select_rows(&s, pred, Bound::Any)
                .iter()
                .map(|&(_, _, r)| r)
                .collect();
            let mut ascending = ids.clone();
            ascending.sort();
            assert_eq!(
                ids, ascending,
                "the full scan emits rows in ascending RowId (row-index) order"
            );
        }
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
            resolved(&s, &select_rows(&s, "http://ex/likes", Bound::Subject(a))),
            vec![(term("http://ex/a"), term("http://ex/c"))],
        );
    }

    /// Emission-order guard: the `relations` table is now a
    /// FIXED-seed `FastMap`, so its raw iteration order is unspecified.  The ONLY
    /// consumer-facing enumeration — [`RelationStore::predicates`] — MUST still be
    /// lexical, sorted through the `BTreeSet` sweep, NEVER leaking the map's hash
    /// order.  Insert predicates in deliberately anti-lexical order and assert the
    /// output is lexical regardless.
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
            "predicates() must be lexical — the FixedState map order must never leak"
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
            resolved(&edb, &select_rows(&edb, "http://ex/knows", Bound::Any)),
            vec![
                (term("http://ex/a"), term("http://ex/b")),
                (term("http://ex/a"), term("http://ex/c")),
            ],
        );
        assert!(edb.contains("http://ex/likes", "<http://ex/a>", "<http://ex/c>"));
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
        // (matching Nemo) — distinct frontier values ⇒ distinct nulls.
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
