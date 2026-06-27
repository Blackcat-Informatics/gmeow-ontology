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

use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use crate::dataset_view::GraphMatch;
use crate::{
    RdfAnnotation, RdfLiteral, RdfLocation, RdfQuad, RdfReifier, RdfStoreCapabilities, RdfTerm,
    RdfTextDirection, RdfTriple,
};

use super::term::{arena_str, BlankScope, InternedTerm, TermId, TermValue};

/// The `rdf:reifies` predicate IRI — the indirection edge of the RDF 1.2 reification
/// layer (`reifier rdf:reifies <<( s p o )>>`). Used to expose the reifier side-table
/// as virtual triples in [`RdfDataset::reifier_quads`].
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

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
    /// The byte arena owning every interned string ONCE (#879 P3b); `terms` hold
    /// `StrRange`s into it, and `resolve` borrows `&str` from here.
    arena: Box<[u8]>,
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
    /// Lazy value→id reverse index for [`RdfDataset::term_id_by_value`] (#838).
    /// Keyed by a canonical **hash** of each term's dataset-independent value (with
    /// `Vec<TermId>` buckets to resolve the rare collision), NOT by full
    /// [`TermValue`] copies — so building it duplicates **no** term strings (~10×
    /// leaner than an owned-key map). Built lazily (the builder's interner index is
    /// dropped at freeze); `OnceLock` keeps the frozen dataset `Send + Sync`.
    value_index: OnceLock<HashMap<u64, Vec<TermId>>>,
    /// Lazy permutation quad indexes for indexed
    /// [`quads_for_pattern`](RdfDataset::quads_for_pattern_indexed) (#891 P4b). SPOG
    /// is free (the `quads` table is already freeze-sorted by `(s, p, o, g)`); the
    /// other five orderings are `u32` ordinal-indirection arrays (4 B/quad) built
    /// lazily on the first pattern query that selects them. `OnceLock` keeps the
    /// frozen dataset `Send + Sync`.
    indexes: QuadIndexes,
}

/// The lazy non-identity permutation indexes over the freeze-sorted `quads` table
/// (#891 P4b). Each is a `u32`-per-quad ordinal-indirection array: `arr[i]` is the
/// ordinal into [`RdfDataset`]`::quads` of the `i`-th quad in that permutation's
/// order. SPOG needs no array (the table is already SPOG-sorted); these five cover
/// the remaining bound-set shapes. All five warm ≈ 20 B/quad on top of the table.
#[derive(Debug, Default)]
struct QuadIndexes {
    pos: OnceLock<Box<[u32]>>,
    osp: OnceLock<Box<[u32]>>,
    gspo: OnceLock<Box<[u32]>>,
    gpos: OnceLock<Box<[u32]>>,
    gosp: OnceLock<Box<[u32]>>,
}

/// A quad-position axis, used to describe a permutation's sort-key order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    S,
    P,
    O,
    G,
}

/// The orderable key of one quad axis. Subject/predicate/object map to the dense
/// `TermId` index; the graph slot maps `None` (default graph) to `0` and `Some(id)`
/// to `index + 1`, so the default graph sorts before every named graph (matching
/// `Option<TermId>`'s own ordering). Each axis is only ever compared against the same
/// axis, so the differing scales never interact.
#[inline]
fn axis_key(axis: Axis, q: &QuadRow) -> u32 {
    match axis {
        Axis::S => q.s.index() as u32,
        Axis::P => q.p.index() as u32,
        Axis::O => q.o.index() as u32,
        Axis::G => match q.g {
            None => 0,
            Some(id) => id.index() as u32 + 1,
        },
    }
}

/// Compare two quads under a permutation's axis order, short-circuiting at the first
/// differing axis (so it computes only the axis keys it needs, never a full `[u32; 4]`).
#[inline]
fn compare_quads(axes: &[Axis; 4], a: &QuadRow, b: &QuadRow) -> Ordering {
    for &axis in axes {
        match axis_key(axis, a).cmp(&axis_key(axis, b)) {
            Ordering::Equal => {}
            ord => return ord,
        }
    }
    Ordering::Equal
}

/// Compare a quad's leading `prefix` axes (in a permutation's order) against a bound
/// `target`, short-circuiting at the first differing axis. Drives the `partition_point`
/// bisection without materializing a key array.
#[inline]
fn compare_prefix(axes: &[Axis; 4], q: &QuadRow, target: &[u32; 4], prefix: usize) -> Ordering {
    for i in 0..prefix {
        match axis_key(axes[i], q).cmp(&target[i]) {
            Ordering::Equal => {}
            ord => return ord,
        }
    }
    Ordering::Equal
}

/// One of the six quad orderings. SPOG is the identity (the freeze-sorted table); the
/// other five are materialized lazily as ordinal arrays.
#[derive(Clone, Copy)]
enum QuadPermutation {
    Spog,
    Pos,
    Osp,
    Gspo,
    Gpos,
    Gosp,
}

impl QuadPermutation {
    /// Every permutation, identity first so it wins prefix-length ties (it needs no
    /// array). The dispatch scans this list to pick the best ordering for a pattern.
    const ALL: [QuadPermutation; 6] = [
        QuadPermutation::Spog,
        QuadPermutation::Pos,
        QuadPermutation::Osp,
        QuadPermutation::Gspo,
        QuadPermutation::Gpos,
        QuadPermutation::Gosp,
    ];

    /// This permutation's axis order (its sort-key sequence).
    #[inline]
    fn axes(self) -> [Axis; 4] {
        use Axis::{G, O, P, S};
        match self {
            QuadPermutation::Spog => [S, P, O, G],
            QuadPermutation::Pos => [P, O, S, G],
            QuadPermutation::Osp => [O, S, P, G],
            QuadPermutation::Gspo => [G, S, P, O],
            QuadPermutation::Gpos => [G, P, O, S],
            QuadPermutation::Gosp => [G, O, S, P],
        }
    }
}

/// The candidate-quad source for an indexed pattern query. Unifies the two access
/// shapes into one `Iterator<Item = &QuadRow>` so `quads_for_pattern` returns a single
/// concrete type regardless of which permutation the dispatch chose:
/// - `Slice` — a contiguous sub-slice of the freeze-sorted `quads` table, iterated
///   SEQUENTIALLY (SPOG bisection, or the low-selectivity fallback). Bounds-check-free.
/// - `Permuted` — a sub-slice of a permutation array whose `u32` ordinals index back
///   into `quads` (the only path that pays random-access indirection; taken only when
///   the candidate run is small enough to beat a sequential scan).
enum QuadCandidates<'a> {
    Slice(std::slice::Iter<'a, QuadRow>),
    Permuted {
        ordinals: std::slice::Iter<'a, u32>,
        quads: &'a [QuadRow],
    },
}

impl<'a> Iterator for QuadCandidates<'a> {
    type Item = &'a QuadRow;
    #[inline]
    fn next(&mut self) -> Option<&'a QuadRow> {
        match self {
            QuadCandidates::Slice(iter) => iter.next(),
            QuadCandidates::Permuted { ordinals, quads } => ordinals.next().map(|&ord| {
                debug_assert!(
                    (ord as usize) < quads.len(),
                    "permutation ordinal out of range"
                );
                // SAFETY: every permutation array is built as a sort of `0..quads.len()`
                // (see `permutation`), so each ordinal is a valid index into the SAME
                // `quads` slice. The `debug_assert` pins the invariant in test builds.
                unsafe { quads.get_unchecked(ord as usize) }
            }),
        }
    }
}

impl RdfDataset {
    /// Assemble a frozen dataset from already-validated, already-ordered parts.
    /// Crate-internal: only [`RdfDatasetBuilder::freeze`] calls this, after
    /// validation.
    ///
    /// [`RdfDatasetBuilder::freeze`]: super::builder::RdfDatasetBuilder::freeze
    pub(crate) fn from_parts(
        arena: Box<[u8]>,
        terms: Box<[InternedTerm]>,
        quads: Box<[QuadRow]>,
        reifiers: Box<[(TermId, TermId)]>,
        annotations: Box<[(TermId, TermId, TermId)]>,
        locations: Box<[(QuadHandle, RdfLocation)]>,
        caps: RdfStoreCapabilities,
    ) -> Self {
        Self {
            arena,
            terms,
            quads,
            reifiers,
            annotations,
            locations,
            caps,
            value_index: OnceLock::new(),
            indexes: QuadIndexes::default(),
        }
    }

    /// Resolve a term id to the owned [`RdfTerm`] model, recursively for triple
    /// terms. This allocates owned strings at the explicit owned-model boundary
    /// used by serializers, oxigraph materialization, the C-ABI (`gmeow-rdf-capi`
    /// renders a cursor term to N-Triples through this), and tests.
    pub fn to_owned_term(&self, id: TermId) -> RdfTerm {
        match self.resolve(id) {
            TermRef::Iri(iri) => RdfTerm::iri(iri),
            TermRef::Blank { label, scope } => RdfTerm::blank_node(scope.qualify_label(label)),
            TermRef::Literal {
                lexical,
                datatype,
                language,
                direction,
            } => {
                let datatype_iri = match self.resolve(datatype) {
                    TermRef::Iri(iri) => iri.to_owned(),
                    other => {
                        unreachable!("literal datatype must resolve to an IRI, got {other:?}")
                    }
                };
                RdfTerm::literal(RdfLiteral {
                    lexical_form: lexical.to_owned(),
                    datatype: Some(datatype_iri),
                    language: language.map(str::to_owned),
                    direction,
                })
            }
            TermRef::Triple { s, p, o } => {
                let subject = self.to_owned_term(s);
                let predicate = self.iri_string(p);
                let object = self.to_owned_term(o);
                RdfTerm::triple(RdfTriple::new(subject, predicate, object))
            }
        }
    }

    /// Resolve a term id that must be an IRI (a predicate / triple-predicate
    /// position) to its owned IRI string.
    fn iri_string(&self, id: TermId) -> String {
        match self.resolve(id) {
            TermRef::Iri(iri) => iri.to_owned(),
            other => unreachable!("expected an IRI in this position, got {other:?}"),
        }
    }

    /// Resolve one ID-native quad row to an owned [`RdfQuad`], attaching the
    /// quad's source location by frozen ordinal.
    pub fn to_owned_quad(&self, frozen_index: usize, q: QuadIds) -> RdfQuad {
        let mut quad = RdfQuad::new(
            self.to_owned_term(q.s),
            self.iri_string(q.p),
            self.to_owned_term(q.o),
        );
        quad.graph_name = q.g.map(|g| self.to_owned_term(g));
        if let Some(loc) = self.location_of(QuadHandle::from_index(frozen_index as u32)) {
            quad = quad.with_location(loc.clone());
        }
        quad
    }

    /// Resolve a `(reifier, triple-term)` binding to an owned [`RdfReifier`].
    pub fn to_owned_reifier(&self, reifier: TermId, triple: TermId) -> RdfReifier {
        let statement = match self.resolve(triple) {
            TermRef::Triple { s, p, o } => RdfTriple::new(
                self.to_owned_term(s),
                self.iri_string(p),
                self.to_owned_term(o),
            ),
            other => unreachable!("a reifier must bind a triple term, got {other:?}"),
        };
        RdfReifier::new(self.to_owned_term(reifier), statement)
    }

    /// Resolve a `(reifier, predicate, object)` annotation to an owned
    /// [`RdfAnnotation`].
    pub fn to_owned_annotation(&self, reifier: TermId, p: TermId, o: TermId) -> RdfAnnotation {
        RdfAnnotation::new(
            self.to_owned_term(reifier),
            self.iri_string(p),
            self.to_owned_term(o),
        )
    }

    /// Iterate over all quads resolved to their owned [`RdfQuad`] representation.
    pub fn owned_quads(&self) -> impl Iterator<Item = RdfQuad> + '_ {
        self.quads()
            .enumerate()
            .map(|(index, quad)| self.to_owned_quad(index, quad))
    }

    /// Iterate over all reifiers resolved to their owned [`RdfReifier`] representation.
    pub fn owned_reifiers(&self) -> impl Iterator<Item = RdfReifier> + '_ {
        self.reifiers()
            .map(|(reifier, triple)| self.to_owned_reifier(reifier, triple))
    }

    /// Iterate over all annotations resolved to their owned [`RdfAnnotation`] representation.
    pub fn owned_annotations(&self) -> impl Iterator<Item = RdfAnnotation> + '_ {
        self.annotations().map(|(reifier, predicate, object)| {
            self.to_owned_annotation(reifier, predicate, object)
        })
    }

    /// Borrow (building on first access) the ordinal-indirection array for a
    /// non-identity permutation (#891 P4b): `arr[i]` is the ordinal into `self.quads`
    /// of the `i`-th quad in `perm`'s order. Sorted by [`perm_key`]; `OnceLock` makes
    /// the first-access build race-safe and keeps the dataset `Send + Sync`. Never
    /// called for [`QuadPermutation::Spog`] (the table is already that order).
    fn permutation(&self, perm: QuadPermutation) -> &[u32] {
        let cell = match perm {
            QuadPermutation::Spog => unreachable!("SPOG is the identity table, never materialized"),
            QuadPermutation::Pos => &self.indexes.pos,
            QuadPermutation::Osp => &self.indexes.osp,
            QuadPermutation::Gspo => &self.indexes.gspo,
            QuadPermutation::Gpos => &self.indexes.gpos,
            QuadPermutation::Gosp => &self.indexes.gosp,
        };
        cell.get_or_init(|| {
            let axes = perm.axes();
            // The ordinal arrays are `u32`, so a dataset with more than u32::MAX quads
            // could not be addressed; fail fast rather than silently truncate the cast.
            let len = u32::try_from(self.quads.len()).expect("dataset quad count exceeds u32::MAX");
            let mut ordinals: Vec<u32> = (0..len).collect();
            ordinals.sort_by(|&a, &b| {
                compare_quads(&axes, &self.quads[a as usize], &self.quads[b as usize])
            });
            ordinals.into_boxed_slice()
        })
    }

    /// Indexed [`DatasetView::quads_for_pattern`](crate::DatasetView::quads_for_pattern)
    /// (#891 P4b): pick the permutation whose sort prefix covers the most bound
    /// positions, binary-search the contiguous candidate run, then apply the EXACT
    /// linear-scan filter to each candidate. Correctness is identical to the scan by
    /// construction — the index only narrows the candidate set; the residual filter is
    /// the same id-equality + [`GraphMatch`] predicate the default scan uses.
    pub(crate) fn quads_for_pattern_indexed(
        &self,
        s: Option<TermId>,
        p: Option<TermId>,
        o: Option<TermId>,
        g: GraphMatch,
    ) -> impl Iterator<Item = QuadIds> + '_ {
        // The bound key for an axis, or `None` if the pattern leaves it free.
        let bound = |axis: Axis| -> Option<u32> {
            match axis {
                Axis::S => s.map(|id| id.index() as u32),
                Axis::P => p.map(|id| id.index() as u32),
                Axis::O => o.map(|id| id.index() as u32),
                Axis::G => match g {
                    GraphMatch::Any => None,
                    GraphMatch::Default => Some(0),
                    GraphMatch::Named(id) => Some(id.index() as u32 + 1),
                },
            }
        };

        // Choose the permutation with the longest leading run of bound axes. SPOG is
        // first in `ALL`, so it wins ties (it needs no array). The chosen `target`
        // holds the bound key for each of the `prefix` leading axes.
        let mut best = QuadPermutation::Spog;
        let mut prefix = 0usize;
        let mut target = [0u32; 4];
        for perm in QuadPermutation::ALL {
            let axes = perm.axes();
            let mut k = 0;
            let mut t = [0u32; 4];
            while k < 4 {
                match bound(axes[k]) {
                    Some(v) => {
                        t[k] = v;
                        k += 1;
                    }
                    None => break,
                }
            }
            if k > prefix {
                prefix = k;
                best = perm;
                target = t;
            }
        }

        let axes = best.axes();
        // Binary-search the contiguous run whose `prefix` leading keys equal `target`.
        // For SPOG the run is a sub-slice of the freeze-sorted table (sequential);
        // otherwise it is a sub-slice of the permutation array (ordinal indirection).
        let candidates = match best {
            QuadPermutation::Spog => {
                let lo = self
                    .quads
                    .partition_point(|q| compare_prefix(&axes, q, &target, prefix).is_lt());
                let hi = self
                    .quads
                    .partition_point(|q| compare_prefix(&axes, q, &target, prefix).is_le());
                QuadCandidates::Slice(self.quads[lo..hi].iter())
            }
            _ => {
                let arr = self.permutation(best);
                let lo = arr.partition_point(|&ord| {
                    compare_prefix(&axes, &self.quads[ord as usize], &target, prefix).is_lt()
                });
                let hi = arr.partition_point(|&ord| {
                    compare_prefix(&axes, &self.quads[ord as usize], &target, prefix).is_le()
                });
                // Selectivity guard (#891): a non-identity permutation visits its run
                // via `u32` ordinal indirection — random access into `quads`. For a
                // LOW-selectivity prefix (a large run, e.g. a predicate matching much of
                // the dataset), that scattered access costs more than a sequential pass,
                // so fall back to a full sequential scan + residual filter (same result).
                // Random access runs ~4× a sequential pass, so the crossover is a run
                // wider than a quarter of the table. (Per-predicate cardinality stats
                // that would let us choose upfront are deferred to the SPARQL round; this
                // is a free, post-bisection check.)
                if (hi - lo).saturating_mul(4) > self.quads.len() {
                    QuadCandidates::Slice(self.quads.iter())
                } else {
                    QuadCandidates::Permuted {
                        ordinals: arr[lo..hi].iter(),
                        quads: &self.quads,
                    }
                }
            }
        };

        candidates
            // The same predicate the linear-scan default applies (dataset_view.rs).
            .filter(move |q| {
                s.is_none_or(|id| q.s == id)
                    && p.is_none_or(|id| q.p == id)
                    && o.is_none_or(|id| q.o == id)
                    && g.matches(q.g)
            })
            .map(|q| QuadIds::from(*q))
    }

    /// Hash an interned term **with zero allocations**, byte-for-byte identically to
    /// [`TermValue`]'s manual `Hash` (explicit tags; the literal datatype is hashed
    /// as its resolved IRI string, triple components recurse). The round-trip tests
    /// fail if this drifts from `TermValue::hash`.
    fn hash_term<H: Hasher>(&self, id: TermId, state: &mut H) {
        match &self.terms[id.index()] {
            InternedTerm::Iri(iri) => {
                0u8.hash(state);
                arena_str(&self.arena, *iri).hash(state);
            }
            InternedTerm::Blank { label, scope } => {
                1u8.hash(state);
                arena_str(&self.arena, *label).hash(state);
                scope.hash(state);
            }
            InternedTerm::Literal(lit) => {
                2u8.hash(state);
                arena_str(&self.arena, lit.lexical_form).hash(state);
                self.hash_iri_string(lit.datatype, state);
                lit.language.map(|r| arena_str(&self.arena, r)).hash(state);
                lit.direction.hash(state);
            }
            InternedTerm::Triple { s, p, o } => {
                3u8.hash(state);
                self.hash_term(*s, state);
                self.hash_term(*p, state);
                self.hash_term(*o, state);
            }
        }
    }

    /// Hash the IRI string of a term known to be an interned IRI (a literal datatype).
    fn hash_iri_string<H: Hasher>(&self, id: TermId, state: &mut H) {
        match &self.terms[id.index()] {
            InternedTerm::Iri(iri) => arena_str(&self.arena, *iri).hash(state),
            // Unreachable for a validated dataset (a literal datatype is always an
            // IRI); hash the Debug form rather than panic.
            other => format!("{other:?}").hash(state),
        }
    }

    /// Whether an interned term equals a dataset-independent [`TermValue`], compared
    /// **with zero allocations** directly against the interned representation
    /// (resolving each string range through the arena, and the literal datatype id
    /// to its IRI). Resolves hash collisions in [`RdfDataset::term_id_by_value`].
    fn term_matches_value(&self, id: TermId, value: &TermValue) -> bool {
        match (&self.terms[id.index()], value) {
            (InternedTerm::Iri(iri), TermValue::Iri(v)) => arena_str(&self.arena, *iri) == v,
            (
                InternedTerm::Blank { label, scope },
                TermValue::Blank {
                    label: vl,
                    scope: vs,
                },
            ) => arena_str(&self.arena, *label) == vl && scope == vs,
            (
                InternedTerm::Literal(lit),
                TermValue::Literal {
                    lexical_form,
                    datatype,
                    language,
                    direction,
                },
            ) => {
                arena_str(&self.arena, lit.lexical_form) == lexical_form
                    && lit.direction == *direction
                    && lit.language.map(|r| arena_str(&self.arena, r)) == language.as_deref()
                    && self.iri_matches(lit.datatype, datatype)
            }
            (
                InternedTerm::Triple { s, p, o },
                TermValue::Triple {
                    s: vs,
                    p: vp,
                    o: vo,
                },
            ) => {
                self.term_matches_value(*s, vs)
                    && self.term_matches_value(*p, vp)
                    && self.term_matches_value(*o, vo)
            }
            _ => false,
        }
    }

    /// Whether a term known to be an interned IRI equals `expected` (zero-alloc).
    fn iri_matches(&self, id: TermId, expected: &str) -> bool {
        matches!(&self.terms[id.index()], InternedTerm::Iri(iri) if arena_str(&self.arena, *iri) == expected)
    }

    /// Canonical hash of a value, matching [`hash_term`](Self::hash_term).
    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// The id of an interned term given its **dataset-independent** value, or
    /// `None` if the dataset contains no such term (purrdf P4, #838).
    ///
    /// The reverse hash→id index is built **lazily on first call** (the builder's
    /// interner index is dropped at freeze) and cached; `OnceLock::get_or_init`
    /// guarantees a single build even under concurrent first access. The index is
    /// keyed by a canonical value hash with `Vec<TermId>` collision buckets and
    /// stores **no** term strings, so building it is allocation-light. Keying on
    /// [`TermValue`] (not [`TermRef`](crate::ir::TermRef)) is the correctness rule: a
    /// `TermRef`'s datatype/triple ids are local to whichever dataset minted them.
    #[must_use]
    pub fn term_id_by_value(&self, value: &TermValue) -> Option<TermId> {
        let index = self.value_index.get_or_init(|| {
            let mut map: HashMap<u64, Vec<TermId>> = HashMap::with_capacity(self.terms.len());
            for i in 0..self.terms.len() {
                let id = TermId::from_index(i as u32);
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                self.hash_term(id, &mut hasher);
                map.entry(hasher.finish()).or_default().push(id);
            }
            map
        });
        index
            .get(&Self::hash_of(value))?
            .iter()
            .copied()
            .find(|&id| self.term_matches_value(id, value))
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
            InternedTerm::Iri(iri) => TermRef::Iri(arena_str(&self.arena, *iri)),
            InternedTerm::Blank { label, scope } => TermRef::Blank {
                label: arena_str(&self.arena, *label),
                scope: *scope,
            },
            InternedTerm::Literal(lit) => TermRef::Literal {
                lexical: arena_str(&self.arena, lit.lexical_form),
                datatype: lit.datatype,
                language: lit.language.map(|r| arena_str(&self.arena, r)),
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

    /// Iterate `(reifier, triple-term)` bindings with each id resolved to its borrowed
    /// [`TermRef`]. Zero allocation (string content is borrowed from the term table),
    /// infallible. The triple-term resolves to [`TermRef::Triple`].
    ///
    /// The borrowed twin of [`reifiers`](Self::reifiers): consumers that read the
    /// RDF 1.2 statement layer off the concrete IR (the GTS writer, the oxigraph
    /// materializer) use this to read reifiers WITHOUT the owned `RdfReifier` model —
    /// the id-based read surface for the purrdf consumer migration (#886).
    #[inline]
    pub fn reifier_refs(&self) -> impl Iterator<Item = (TermRef<'_>, TermRef<'_>)> + '_ {
        self.reifiers()
            .map(move |(r, t)| (self.resolve(r), self.resolve(t)))
    }

    /// Iterate `(reifier, predicate, object)` annotations with each id resolved to its
    /// borrowed [`TermRef`]. Zero allocation, infallible. The borrowed twin of
    /// [`annotations`](Self::annotations) — see [`reifier_refs`](Self::reifier_refs).
    #[inline]
    pub fn annotation_refs(
        &self,
    ) -> impl Iterator<Item = (TermRef<'_>, TermRef<'_>, TermRef<'_>)> + '_ {
        self.annotations()
            .map(move |(r, p, o)| (self.resolve(r), self.resolve(p), self.resolve(o)))
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

    /// The interned id of the `rdf:reifies` predicate IRI, or `None` if the dataset
    /// never interned it. A dataset with at least one reifier always has it interned
    /// (a reifier binding is serialized as `reifier rdf:reifies <<( s p o )>>`), so the
    /// `None` case can only coincide with an empty reifier table — exactly the case in
    /// which [`reifier_quads`](Self::reifier_quads) yields nothing anyway.
    fn rdf_reifies_id(&self) -> Option<TermId> {
        self.term_id_by_value(&TermValue::Iri(RDF_REIFIES.to_owned()))
    }

    /// Iterate the reifier side-table AS resolved virtual triples: each
    /// `(reifier, triple-term)` binding becomes a `(reifier, rdf:reifies, triple-term)`
    /// quad in the default graph (`g == None`).
    ///
    /// The RDF 1.2 reification layer is stored in a SEPARATE side-table — it is NOT in
    /// the `quads` table — so this view is the only way a triple-pattern matcher can see
    /// it. Yields in the reifier table's frozen `(reifier, triple)` sorted order, so the
    /// output is deterministic. If the dataset has no reifiers (and so never interned
    /// `rdf:reifies`), this yields nothing.
    pub fn reifier_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        // `flat_map` over the `Option<TermId>` so the iterator type is fixed whether or
        // not `rdf:reifies` is interned; an empty option ⇒ an empty stream.
        self.rdf_reifies_id().into_iter().flat_map(move |reifies| {
            self.reifiers().map(move |(reifier, triple)| QuadIds {
                s: reifier,
                p: reifies,
                o: triple,
                g: None,
            })
        })
    }

    /// Iterate the annotation side-table AS resolved virtual triples: each
    /// `(reifier, predicate, object)` annotation becomes a `(reifier, predicate, object)`
    /// quad in the default graph (`g == None`).
    ///
    /// Like [`reifier_quads`](Self::reifier_quads), the annotation layer lives in a
    /// SEPARATE side-table outside `quads`; this is the only triple-pattern view of it.
    /// Yields in the annotation table's frozen `(reifier, predicate, object)` sorted
    /// order, so the output is deterministic.
    pub fn annotation_quads(&self) -> impl Iterator<Item = QuadIds> + '_ {
        self.annotations()
            .map(|(reifier, predicate, object)| QuadIds {
                s: reifier,
                p: predicate,
                o: object,
                g: None,
            })
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
    // RdfDataset carries lazy `OnceLock` indexes — the value index (#838) and the
    // permutation quad indexes (#891 P4b). `OnceLock` (not `RefCell`) is what keeps
    // this guard holding once those interior-mutable caches are added.
    assert_send_sync::<RdfDataset>();
    assert_send_sync::<TermId>();
    assert_send_sync::<QuadIds>();
    assert_send_sync::<TermValue>();
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
    fn term_id_by_value_round_trips_every_kind() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        let bn = b.intern_blank("b0".to_string(), BlankScope::DEFAULT);
        let plain = b.intern_literal(RdfLiteral::simple("hello"));
        let typed = b.intern_literal(RdfLiteral::typed(
            "42",
            "http://www.w3.org/2001/XMLSchema#integer",
        ));
        let lang = b.intern_literal(RdfLiteral::language_tagged("bonjour", "fr"));
        let tr = b.intern_triple(s, p, o);
        b.push_quad(s, p, o, None);
        let r = iri(&mut b, "r");
        let reifies =
            b.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies".to_string());
        b.push_quad(r, reifies, tr, None);
        let ds = b.freeze().expect("freeze");

        assert_eq!(
            ds.term_id_by_value(&TermValue::Iri("http://example.org/s".to_string())),
            Some(s)
        );
        assert_eq!(
            ds.term_id_by_value(&TermValue::Blank {
                label: "b0".to_string(),
                scope: BlankScope::DEFAULT,
            }),
            Some(bn)
        );
        assert_eq!(
            ds.term_id_by_value(&TermValue::Literal {
                lexical_form: "hello".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                language: None,
                direction: None,
            }),
            Some(plain)
        );
        assert_eq!(
            ds.term_id_by_value(&TermValue::Literal {
                lexical_form: "42".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#integer".to_string(),
                language: None,
                direction: None,
            }),
            Some(typed)
        );
        assert_eq!(
            ds.term_id_by_value(&TermValue::Literal {
                lexical_form: "bonjour".to_string(),
                datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".to_string(),
                language: Some("fr".to_string()),
                direction: None,
            }),
            Some(lang)
        );
        // A triple term resolves recursively by value.
        let triple_val = TermValue::Triple {
            s: Box::new(TermValue::Iri("http://example.org/s".to_string())),
            p: Box::new(TermValue::Iri("http://example.org/p".to_string())),
            o: Box::new(TermValue::Iri("http://example.org/o".to_string())),
        };
        assert_eq!(ds.term_id_by_value(&triple_val), Some(tr));
        // An absent value misses.
        assert_eq!(
            ds.term_id_by_value(&TermValue::Iri("http://example.org/absent".to_string())),
            None
        );
    }

    #[test]
    fn term_id_by_value_disambiguates_same_lexical_different_datatype() {
        // Two literals share the lexical form "1" but differ by datatype — the
        // hash-bucketed index must disambiguate them by value (term_matches_value
        // resolves the datatype id to its IRI), not collapse them.
        let mut b = RdfDatasetBuilder::new();
        let as_int = b.intern_literal(RdfLiteral::typed(
            "1",
            "http://www.w3.org/2001/XMLSchema#integer",
        ));
        let as_bool = b.intern_literal(RdfLiteral::typed(
            "1",
            "http://www.w3.org/2001/XMLSchema#boolean",
        ));
        let s = iri(&mut b, "s");
        b.push_quad(s, s, as_int, None);
        b.push_quad(s, s, as_bool, None);
        let ds = b.freeze().unwrap();
        assert_ne!(as_int, as_bool);
        assert_eq!(
            ds.term_id_by_value(&TermValue::Literal {
                lexical_form: "1".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#integer".to_string(),
                language: None,
                direction: None,
            }),
            Some(as_int)
        );
        assert_eq!(
            ds.term_id_by_value(&TermValue::Literal {
                lexical_form: "1".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#boolean".to_string(),
                language: None,
                direction: None,
            }),
            Some(as_bool)
        );
    }

    #[test]
    fn term_id_by_value_is_dataset_independent_not_id_based() {
        // The SAME value maps to DIFFERENT ids across datasets; a value lookup must
        // return each dataset's OWN id (proves it is value-keyed, never smuggling a
        // foreign dataset-local id — the #838 correctness rule).
        let val = TermValue::Iri("http://example.org/x".to_string());
        let mut a = RdfDatasetBuilder::new();
        let _pad = iri(&mut a, "pad"); // shift x's id in dataset `a`
        let xa = a.intern_iri("http://example.org/x".to_string());
        a.push_quad(xa, xa, xa, None);
        let da = a.freeze().unwrap();

        let mut b = RdfDatasetBuilder::new();
        let xb = b.intern_iri("http://example.org/x".to_string());
        b.push_quad(xb, xb, xb, None);
        let db = b.freeze().unwrap();

        assert_ne!(xa, xb, "the same value has different ids across datasets");
        assert_eq!(da.term_id_by_value(&val), Some(xa));
        assert_eq!(db.term_id_by_value(&val), Some(xb));
    }

    #[test]
    fn term_id_by_value_lazy_init_is_thread_safe() {
        use std::sync::Arc;
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        b.push_quad(s, s, s, None);
        let ds = b.freeze().unwrap(); // Arc<RdfDataset>
        let want = TermValue::Iri("http://example.org/s".to_string());
        // Many threads race the OnceLock first-init; all must agree.
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let ds = Arc::clone(&ds);
                let want = want.clone();
                std::thread::spawn(move || ds.term_id_by_value(&want))
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), Some(s));
        }
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
    fn reifier_and_annotation_refs_resolve_to_borrowed_terms() {
        // The borrowed read surface (#886) must resolve every reifier/annotation id to
        // its `TermRef` with full fidelity — including a triple-term reifier statement
        // and a directional literal annotation object (MAXIMAL INFORMATION FLOW).
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        let triple = b.intern_triple(s, p, o);
        let r = iri(&mut b, "r");
        let ap = iri(&mut b, "ap");
        let rtl = b.intern_literal(RdfLiteral {
            lexical_form: "مرحبا".to_string(),
            datatype: None,
            language: Some("ar".to_string()),
            direction: Some(RdfTextDirection::Rtl),
        });
        b.push_reifier(r, triple);
        b.push_annotation(r, ap, rtl);
        let ds = b.freeze().expect("valid");

        // reifier_refs: the reifier is an IRI, the statement resolves to a triple term.
        let reifier_refs: Vec<_> = ds.reifier_refs().collect();
        assert_eq!(reifier_refs.len(), 1);
        let (reifier, statement) = &reifier_refs[0];
        assert!(matches!(reifier, TermRef::Iri("http://example.org/r")));
        match statement {
            TermRef::Triple { s: ts, p: tp, .. } => {
                assert_eq!(*ts, s);
                assert_eq!(*tp, p);
            }
            other => panic!("reifier statement must be a triple term, got {other:?}"),
        }

        // annotation_refs: the directional literal object survives resolution intact.
        let annotation_refs: Vec<_> = ds.annotation_refs().collect();
        assert_eq!(annotation_refs.len(), 1);
        let (a_reifier, a_pred, a_obj) = &annotation_refs[0];
        assert!(matches!(a_reifier, TermRef::Iri("http://example.org/r")));
        assert!(matches!(a_pred, TermRef::Iri("http://example.org/ap")));
        match a_obj {
            TermRef::Literal {
                lexical,
                language,
                direction,
                ..
            } => {
                assert_eq!(*lexical, "مرحبا");
                assert_eq!(*language, Some("ar"));
                assert_eq!(*direction, Some(RdfTextDirection::Rtl));
            }
            other => panic!("annotation object must be the directional literal, got {other:?}"),
        }
    }

    #[test]
    fn reifier_quads_expose_the_side_table_as_virtual_triples() {
        // The reification layer lives outside `quads`; `reifier_quads` exposes each
        // `(reifier, triple)` binding as a `(reifier, rdf:reifies, triple)` default-graph
        // quad so a triple-pattern matcher can see it.
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        let triple = b.intern_triple(s, p, o);
        let r1 = iri(&mut b, "r1");
        let r2 = iri(&mut b, "r2");
        // The ingest path interns `rdf:reifies` as a term alongside the reifier
        // binding (it is the serialized indirection edge); `reifier_quads` uses that
        // interned id as the virtual predicate.
        let reifies = b.intern_iri(RDF_REIFIES.to_owned());
        b.push_reifier(r1, triple);
        b.push_reifier(r2, triple);
        let ds = b.freeze().expect("valid");

        assert_eq!(
            ds.term_id_by_value(&TermValue::Iri(RDF_REIFIES.to_owned())),
            Some(reifies)
        );
        let rows: Vec<QuadIds> = ds.reifier_quads().collect();
        // Frozen `(reifier, triple)` sorted order; r1 < r2 by interning order/id.
        assert_eq!(
            rows,
            vec![
                QuadIds {
                    s: r1,
                    p: reifies,
                    o: triple,
                    g: None,
                },
                QuadIds {
                    s: r2,
                    p: reifies,
                    o: triple,
                    g: None,
                },
            ]
        );
        // The reification layer is NOT in the quads table (no double counting).
        assert_eq!(ds.quad_count(), 0);
    }

    #[test]
    fn annotation_quads_expose_the_side_table_as_virtual_triples() {
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        let triple = b.intern_triple(s, p, o);
        let r = iri(&mut b, "r");
        let ap1 = iri(&mut b, "ap1");
        let ap2 = iri(&mut b, "ap2");
        let ao1 = iri(&mut b, "ao1");
        let ao2 = iri(&mut b, "ao2");
        b.push_reifier(r, triple);
        b.push_annotation(r, ap1, ao1);
        b.push_annotation(r, ap2, ao2);
        let ds = b.freeze().expect("valid");

        let rows: Vec<QuadIds> = ds.annotation_quads().collect();
        // Frozen `(reifier, predicate, object)` sorted order.
        assert_eq!(
            rows,
            vec![
                QuadIds {
                    s: r,
                    p: ap1,
                    o: ao1,
                    g: None,
                },
                QuadIds {
                    s: r,
                    p: ap2,
                    o: ao2,
                    g: None,
                },
            ]
        );
    }

    #[test]
    fn reifier_quads_empty_when_no_reifiers() {
        // No reifiers ⇒ `rdf:reifies` is never interned ⇒ an empty virtual stream
        // (the `None` branch of `rdf_reifies_id`), not a panic.
        let mut b = RdfDatasetBuilder::new();
        let s = iri(&mut b, "s");
        let p = iri(&mut b, "p");
        let o = iri(&mut b, "o");
        b.push_quad(s, p, o, None);
        let ds = b.freeze().expect("valid");
        assert_eq!(ds.reifier_quads().count(), 0);
        assert_eq!(ds.annotation_quads().count(), 0);
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

        /// The #891 P4b correctness gate: the indexed `quads_for_pattern` must return
        /// EXACTLY the same quad set as a linear scan, for every `(s?, p?, o?) ×
        /// GraphMatch` shape. The index only narrows candidates; the residual filter is
        /// the same predicate the scan applies, so any divergence is a range-math bug.
        #[test]
        fn proptest_indexed_pattern_matches_linear_scan(
            rows in prop::collection::vec(
                (0u8..5, 0u8..5, 0u8..5, prop::option::of(0u8..3)),
                0..48,
            ),
            s_sel in prop::option::of(0u8..5),
            p_sel in prop::option::of(0u8..5),
            o_sel in prop::option::of(0u8..5),
            // 0 = Any, 1 = Default, 2..5 = Named(graphs[g - 2]).
            g_sel in 0u8..5,
        ) {
            use std::collections::BTreeSet;

            let mut b = RdfDatasetBuilder::new();
            let pool: Vec<TermId> = (0..5)
                .map(|n| b.intern_iri(format!("http://example.org/n{n}")))
                .collect();
            let graphs: Vec<TermId> = (0..3)
                .map(|n| b.intern_iri(format!("http://example.org/g{n}")))
                .collect();
            for (s, p, o, g) in rows {
                b.push_quad(pool[s as usize], pool[p as usize], pool[o as usize],
                    g.map(|gi| graphs[gi as usize]));
            }
            let ds = b.freeze().expect("random valid dataset must freeze");

            let s = s_sel.map(|i| pool[i as usize]);
            let p = p_sel.map(|i| pool[i as usize]);
            let o = o_sel.map(|i| pool[i as usize]);
            let g = match g_sel {
                0 => GraphMatch::Any,
                1 => GraphMatch::Default,
                n => GraphMatch::Named(graphs[(n - 2) as usize]),
            };

            // Reference: the exact linear scan the trait default would run.
            let key = |q: QuadIds| (q.s, q.p, q.o, q.g);
            let scan: BTreeSet<_> = ds
                .quads()
                .filter(|q| {
                    s.is_none_or(|id| q.s == id)
                        && p.is_none_or(|id| q.p == id)
                        && o.is_none_or(|id| q.o == id)
                        && g.matches(q.g)
                })
                .map(key)
                .collect();
            let indexed: BTreeSet<_> =
                ds.quads_for_pattern_indexed(s, p, o, g).map(key).collect();
            prop_assert_eq!(indexed, scan);
        }
    }
}
