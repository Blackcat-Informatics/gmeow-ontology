// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `RdfDataset`-direct structural comparator (#819 C1/C2): the equality oracle
//! for importer equivalence and downstream tests.
//!
//! [`datasets_isomorphic`] decides whether two frozen datasets are
//! **RDF-structurally isomorphic**: the same quads (under a blank-node bijection),
//! the same reifier bindings, and the same annotations. It operates **directly on
//! [`RdfDataset`]** and **NEVER consults oxigraph**. That is deliberate and is the
//! acceptance gate of #819 (design doc *Appendix C0*, point 4): oxigraph
//! canonicalizes typed-literal lexical forms (`0.70` → `0.7`, `+00:00` → `Z`) and
//! drops the reifier/annotation overlay entirely — so two datasets that differ only
//! in lexical spelling or in reifier COUNT would compare *equal* through oxigraph,
//! exactly the differences this comparator must catch.
//!
//! ## Identity contract
//!
//! - **Ground terms** (IRIs; literals incl. datatype / language / direction; and
//!   triple terms whose components are all ground) compare by their exact resolved
//!   value. The interned literal-identity policy (C0.1) already lives in the dataset,
//!   so e.g. `"x"` and `"x"^^xsd:string` resolve identically and compare equal, while
//!   two distinct directions or lexical spellings compare unequal.
//! - **Blank nodes** compare by **bijection**, never by `(label, scope)`: the two
//!   importer paths assign different [`BlankScope`](super::term::BlankScope) numbers
//!   and labels, yet a structurally identical graph must compare equal.
//! - **Reifiers and annotations are part of the structure**: two datasets that differ
//!   only in how many reifiers bind one triple term, or in an annotation triple,
//!   compare UNEQUAL.
//!
//! ## Blank-node canonicalization & its correctness caveat
//!
//! Blanks are canonicalized by **iterative signature hashing** (a simplified
//! RDFC-1.0): each blank starts from a constant seed, then on each round absorbs a
//! commutative (order-independent) digest of every incident quad/reifier/annotation —
//! each ground neighbour by value, each blank neighbour by its *current* signature,
//! plus the blank's role (position) in that statement. Iterating to a fixed point
//! propagates structure outward so non-isomorphic wirings diverge.
//!
//! After the fixed point, each dataset is rendered into a **canonical multiset** of
//! quads/reifiers/annotations with every blank replaced by its signature, and the two
//! multisets are compared. The hash is **commutative across statements and across the
//! refinement frontier**, so it is invariant under blank relabeling.
//!
//! Caveat: a pure hash refinement is NOT a full RDFC-1.0 isomorphism decision for
//! *pathologically symmetric* blank graphs (automorphism classes a hash cannot split
//! without hash-tie-break + backtracking). This implementation therefore guards
//! against a **false positive** with a final structural check: after canonical
//! labeling, if any blank's signature is shared by more than one blank *within a
//! single dataset* (an unresolved symmetry), and the two datasets are not already
//! proven equal by the multiset comparison, the comparator returns **false** rather
//! than risk reporting two genuinely-different datasets as equal. The contract is:
//! never a false positive; a false negative on a pathological symmetry is acceptable
//! (and does not arise for the importer-equivalence fixtures, whose blanks are
//! distinguished by their ground neighbours).

use std::collections::BTreeMap;

use super::dataset::RdfDataset;
use super::term::TermId;

/// A 64-bit signature used both as a blank-node canonical label and as a term key.
type Sig = u64;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[inline]
fn fnv_mix(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[inline]
fn fnv_u64(hash: u64, value: u64) -> u64 {
    fnv_mix(hash, &value.to_le_bytes())
}

/// A fully-resolved, blank-agnostic VALUE key for a ground term, used as the stable
/// component of statement signatures. Blanks are excluded here (they carry no ground
/// value); their contribution to a statement signature is their canonical signature.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum GroundKey {
    Iri(String),
    /// lexical, datatype-iri, language, direction-discriminant.
    Literal(String, String, Option<String>, Option<u8>),
    /// A triple term keyed by its three components' canonical keys.
    Triple(Box<(TermKey, TermKey, TermKey)>),
}

/// A canonical key for ANY term: a ground value, or a blank identified by its
/// canonical signature (never by label/scope).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum TermKey {
    Ground(GroundKey),
    Blank(Sig),
}

/// Per-dataset canonicalization state.
struct Canon<'a> {
    ds: &'a RdfDataset,
    /// The blank `TermId`s present in the dataset, in a stable order.
    blanks: Vec<TermId>,
    /// Current signature for each blank `TermId`.
    sig: BTreeMap<TermId, Sig>,
}

impl<'a> Canon<'a> {
    fn new(ds: &'a RdfDataset) -> Self {
        // Collect every blank TermId actually referenced by a quad / reifier /
        // annotation (resolving recursively into triple terms).
        let mut blank_set: std::collections::BTreeSet<TermId> = std::collections::BTreeSet::new();
        for q in ds.quads() {
            collect_blanks(ds, q.s, &mut blank_set);
            collect_blanks(ds, q.p, &mut blank_set);
            collect_blanks(ds, q.o, &mut blank_set);
            if let Some(g) = q.g {
                collect_blanks(ds, g, &mut blank_set);
            }
        }
        for (r, t) in ds.reifiers() {
            collect_blanks(ds, r, &mut blank_set);
            collect_blanks(ds, t, &mut blank_set);
        }
        for (r, p, o) in ds.annotations() {
            collect_blanks(ds, r, &mut blank_set);
            collect_blanks(ds, p, &mut blank_set);
            collect_blanks(ds, o, &mut blank_set);
        }
        let blanks: Vec<TermId> = blank_set.into_iter().collect();
        // All blanks start from the SAME seed: their identity must come purely from
        // structure, never from label/scope/id order.
        let sig = blanks.iter().map(|&b| (b, FNV_OFFSET)).collect();
        Self { ds, blanks, sig }
    }

    /// The current key for a term: ground value, or a blank's current signature.
    fn term_key(&self, id: TermId) -> TermKey {
        match self.ds.resolve(id) {
            super::dataset::TermRef::Iri(iri) => TermKey::Ground(GroundKey::Iri(iri.to_owned())),
            super::dataset::TermRef::Blank { .. } => {
                TermKey::Blank(*self.sig.get(&id).expect("blank must be tracked"))
            }
            super::dataset::TermRef::Literal {
                lexical,
                datatype,
                language,
                direction,
            } => {
                let datatype_iri = match self.ds.resolve(datatype) {
                    super::dataset::TermRef::Iri(iri) => iri.to_owned(),
                    other => unreachable!("literal datatype must be an IRI, got {other:?}"),
                };
                TermKey::Ground(GroundKey::Literal(
                    lexical.to_owned(),
                    datatype_iri,
                    language.map(str::to_owned),
                    direction.map(|d| d as u8),
                ))
            }
            super::dataset::TermRef::Triple { s, p, o } => TermKey::Ground(GroundKey::Triple(
                Box::new((self.term_key(s), self.term_key(p), self.term_key(o))),
            )),
        }
    }

    /// Hash a term key into an accumulator. Stable across relabeling because a blank
    /// contributes only its current signature, never its id.
    fn hash_term_key(&self, hash: u64, key: &TermKey) -> u64 {
        match key {
            TermKey::Ground(g) => self.hash_ground(fnv_u64(hash, 1), g),
            TermKey::Blank(sig) => fnv_u64(fnv_u64(hash, 2), *sig),
        }
    }

    fn hash_ground(&self, hash: u64, g: &GroundKey) -> u64 {
        match g {
            GroundKey::Iri(iri) => fnv_mix(fnv_u64(hash, 10), iri.as_bytes()),
            GroundKey::Literal(lex, dt, lang, dir) => {
                let mut h = fnv_mix(fnv_u64(hash, 11), lex.as_bytes());
                h = fnv_mix(h, dt.as_bytes());
                if let Some(l) = lang {
                    h = fnv_mix(fnv_u64(h, 1), l.as_bytes());
                }
                if let Some(d) = dir {
                    h = fnv_u64(fnv_u64(h, 2), u64::from(*d));
                }
                h
            }
            GroundKey::Triple(parts) => {
                let h = fnv_u64(hash, 12);
                let h = self.hash_term_key(h, &parts.0);
                let h = self.hash_term_key(h, &parts.1);
                self.hash_term_key(h, &parts.2)
            }
        }
    }

    /// One refinement round: recompute each blank's signature from a commutative
    /// digest of its incident statements (each statement contributes a hash that
    /// folds in the blank's role/position and every other position's current key).
    /// Returns the new signature map.
    fn refine(&self) -> BTreeMap<TermId, Sig> {
        // For each blank, accumulate a COMMUTATIVE (XOR-folded) digest over all the
        // statements it participates in, so the result is independent of statement
        // order and of which other blank is which.
        let mut acc: BTreeMap<TermId, u64> = self.blanks.iter().map(|&b| (b, 0u64)).collect();

        let mut contribute = |id: TermId, stmt_hash: u64, role: u64| {
            if let Some(slot) = acc.get_mut(&id) {
                // Fold the per-statement hash (already role-tagged) commutatively.
                *slot ^= fnv_u64(stmt_hash, role).rotate_left((role % 63) as u32 + 1);
            }
        };

        // Quads: role tags 0=s, 1=p, 2=o, 3=g.
        for q in self.ds.quads() {
            let sk = self.term_key(q.s);
            let pk = self.term_key(q.p);
            let ok = self.term_key(q.o);
            let gk = q.g.map(|g| self.term_key(g));
            // Per-position statement hash: the WHOLE statement minus the focused
            // blank's signature (its signature is replaced by a constant focus token),
            // tagged with the focus role. This makes a blank's update depend on its
            // neighbours, not on itself.
            let base = self.hash_quad(&sk, &pk, &ok, gk.as_ref());
            self.contribute_blanks_of(q.s, base, 0, &mut contribute);
            self.contribute_blanks_of(q.p, base, 1, &mut contribute);
            self.contribute_blanks_of(q.o, base, 2, &mut contribute);
            if let Some(g) = q.g {
                self.contribute_blanks_of(g, base, 3, &mut contribute);
            }
        }
        // Reifiers: role 4=reifier, the bound triple's blanks via the triple key.
        for (r, t) in self.ds.reifiers() {
            let rk = self.term_key(r);
            let tk = self.term_key(t);
            let base = self.hash_reifier(&rk, &tk);
            self.contribute_blanks_of(r, base, 4, &mut contribute);
            self.contribute_blanks_of(t, base, 5, &mut contribute);
        }
        // Annotations: roles 6=reifier, 7=predicate, 8=object.
        for (r, p, o) in self.ds.annotations() {
            let rk = self.term_key(r);
            let pk = self.term_key(p);
            let ok = self.term_key(o);
            let base = self.hash_annotation(&rk, &pk, &ok);
            self.contribute_blanks_of(r, base, 6, &mut contribute);
            self.contribute_blanks_of(p, base, 7, &mut contribute);
            self.contribute_blanks_of(o, base, 8, &mut contribute);
        }

        // The next signature mixes the previous signature with the accumulated digest.
        self.blanks
            .iter()
            .map(|&b| {
                let prev = *self.sig.get(&b).expect("tracked");
                let digest = *acc.get(&b).expect("tracked");
                (b, fnv_u64(prev, digest))
            })
            .collect()
    }

    /// Contribute `base` (a statement hash) to EVERY blank reachable at `id`
    /// (including blanks nested inside a triple term), tagged with `role`.
    fn contribute_blanks_of(
        &self,
        id: TermId,
        base: u64,
        role: u64,
        contribute: &mut impl FnMut(TermId, u64, u64),
    ) {
        match self.ds.resolve(id) {
            super::dataset::TermRef::Blank { .. } => contribute(id, base, role),
            super::dataset::TermRef::Triple { s, p, o } => {
                // Nested-triple blanks get a role offset so position inside the triple
                // matters.
                self.contribute_blanks_of(
                    s,
                    base,
                    role.wrapping_mul(31).wrapping_add(20),
                    contribute,
                );
                self.contribute_blanks_of(
                    p,
                    base,
                    role.wrapping_mul(31).wrapping_add(21),
                    contribute,
                );
                self.contribute_blanks_of(
                    o,
                    base,
                    role.wrapping_mul(31).wrapping_add(22),
                    contribute,
                );
            }
            _ => {}
        }
    }

    fn hash_quad(&self, s: &TermKey, p: &TermKey, o: &TermKey, g: Option<&TermKey>) -> u64 {
        let mut h = fnv_u64(FNV_OFFSET, 100);
        h = self.hash_term_key(fnv_u64(h, 0), s);
        h = self.hash_term_key(fnv_u64(h, 1), p);
        h = self.hash_term_key(fnv_u64(h, 2), o);
        match g {
            Some(g) => self.hash_term_key(fnv_u64(h, 3), g),
            None => fnv_u64(h, 4),
        }
    }

    fn hash_reifier(&self, r: &TermKey, t: &TermKey) -> u64 {
        let h = fnv_u64(FNV_OFFSET, 200);
        let h = self.hash_term_key(fnv_u64(h, 0), r);
        self.hash_term_key(fnv_u64(h, 1), t)
    }

    fn hash_annotation(&self, r: &TermKey, p: &TermKey, o: &TermKey) -> u64 {
        let h = fnv_u64(FNV_OFFSET, 300);
        let h = self.hash_term_key(fnv_u64(h, 0), r);
        let h = self.hash_term_key(fnv_u64(h, 1), p);
        self.hash_term_key(fnv_u64(h, 2), o)
    }

    /// Iterate refinement to a fixed point (signatures stop changing) or a bounded
    /// number of rounds (`blanks + 2`, enough for structure to propagate across the
    /// blank graph's diameter for non-pathological inputs).
    fn run_to_fixpoint(&mut self) {
        let rounds = self.blanks.len() + 2;
        for _ in 0..rounds {
            let next = self.refine();
            if next == self.sig {
                break;
            }
            self.sig = next;
        }
    }

    /// `true` iff two distinct blanks share the same final signature — an unresolved
    /// symmetry that a hash refinement cannot split. Used to guard against false
    /// positives.
    fn has_signature_collision(&self) -> bool {
        let mut seen: std::collections::BTreeSet<Sig> = std::collections::BTreeSet::new();
        for &b in &self.blanks {
            let sig = *self.sig.get(&b).expect("tracked");
            if !seen.insert(sig) {
                return true;
            }
        }
        false
    }

    /// The canonical multisets (quads, reifiers, annotations) with blanks rendered by
    /// signature, for cross-dataset comparison.
    fn canonical_form(&self) -> CanonicalForm {
        let mut quads: BTreeMap<(TermKey, TermKey, TermKey, Option<TermKey>), usize> =
            BTreeMap::new();
        for q in self.ds.quads() {
            let key = (
                self.term_key(q.s),
                self.term_key(q.p),
                self.term_key(q.o),
                q.g.map(|g| self.term_key(g)),
            );
            *quads.entry(key).or_insert(0) += 1;
        }
        let mut reifiers: BTreeMap<(TermKey, TermKey), usize> = BTreeMap::new();
        for (r, t) in self.ds.reifiers() {
            *reifiers
                .entry((self.term_key(r), self.term_key(t)))
                .or_insert(0) += 1;
        }
        let mut annotations: BTreeMap<(TermKey, TermKey, TermKey), usize> = BTreeMap::new();
        for (r, p, o) in self.ds.annotations() {
            *annotations
                .entry((self.term_key(r), self.term_key(p), self.term_key(o)))
                .or_insert(0) += 1;
        }
        CanonicalForm {
            quads,
            reifiers,
            annotations,
        }
    }
}

/// Collect every blank `TermId` reachable at `id` (recursing into triple terms).
fn collect_blanks(ds: &RdfDataset, id: TermId, out: &mut std::collections::BTreeSet<TermId>) {
    match ds.resolve(id) {
        super::dataset::TermRef::Blank { .. } => {
            out.insert(id);
        }
        super::dataset::TermRef::Triple { s, p, o } => {
            collect_blanks(ds, s, out);
            collect_blanks(ds, p, out);
            collect_blanks(ds, o, out);
        }
        _ => {}
    }
}

/// The signature-rendered canonical form of one dataset.
#[derive(PartialEq, Eq)]
struct CanonicalForm {
    quads: BTreeMap<(TermKey, TermKey, TermKey, Option<TermKey>), usize>,
    reifiers: BTreeMap<(TermKey, TermKey), usize>,
    annotations: BTreeMap<(TermKey, TermKey, TermKey), usize>,
}

/// IR-direct structural comparison. Returns `true` iff the two datasets are
/// RDF-structurally isomorphic: the same quads (under a blank-node bijection), the
/// same reifier bindings, and the same annotations. **Oxigraph is NEVER consulted.**
///
/// Prefers a false negative to a false positive: on an unresolved blank symmetry it
/// returns `false` rather than risk equating two genuinely-different datasets (see the
/// module-level caveat).
pub fn datasets_isomorphic(a: &RdfDataset, b: &RdfDataset) -> bool {
    // Fast structural rejections that do not depend on blank labeling.
    if a.quad_count() != b.quad_count() {
        return false;
    }
    if a.reifiers().count() != b.reifiers().count() {
        return false;
    }
    if a.annotations().count() != b.annotations().count() {
        return false;
    }

    let mut ca = Canon::new(a);
    let mut cb = Canon::new(b);
    if ca.blanks.len() != cb.blanks.len() {
        return false;
    }
    ca.run_to_fixpoint();
    cb.run_to_fixpoint();

    let equal = ca.canonical_form() == cb.canonical_form();
    if !equal {
        return false;
    }

    // Equal canonical forms, BUT if either side has an internal signature collision
    // (two blanks sharing a signature), the hash refinement did not resolve the
    // symmetry; we cannot prove a true bijection, so refuse to claim equality.
    if ca.has_signature_collision() || cb.has_signature_collision() {
        return false;
    }
    true
}

/// A structural diff between two datasets, for test diagnostics. Counts only; the
/// blank-aware verdict is [`datasets_isomorphic`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetDiff {
    /// `(a, b)` quad counts.
    pub quad_counts: (usize, usize),
    /// `(a, b)` reifier-binding counts.
    pub reifier_counts: (usize, usize),
    /// `(a, b)` annotation counts.
    pub annotation_counts: (usize, usize),
    /// `(a, b)` blank-node counts.
    pub blank_counts: (usize, usize),
    /// The blank-aware structural verdict.
    pub isomorphic: bool,
}

/// A richer diff for test diagnostics: structural counts plus the isomorphism verdict.
pub fn dataset_diff(a: &RdfDataset, b: &RdfDataset) -> DatasetDiff {
    let ca = Canon::new(a);
    let cb = Canon::new(b);
    DatasetDiff {
        quad_counts: (a.quad_count(), b.quad_count()),
        reifier_counts: (a.reifiers().count(), b.reifiers().count()),
        annotation_counts: (a.annotations().count(), b.annotations().count()),
        blank_counts: (ca.blanks.len(), cb.blanks.len()),
        isomorphic: datasets_isomorphic(a, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RdfDatasetBuilder;
    use crate::{RdfLiteral, RdfTextDirection};
    use std::sync::Arc;

    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(format!("http://example.org/{n}"))
    }

    fn ground_triple() -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        b.push_quad(s, p, o, None);
        b.freeze().expect("valid")
    }

    #[test]
    fn identical_ground_datasets_are_isomorphic() {
        let a = ground_triple();
        let b = ground_triple();
        assert!(datasets_isomorphic(&a, &b));
    }

    #[test]
    fn differing_ground_iri_is_not_isomorphic() {
        let a = ground_triple();
        let mut bb = RdfDatasetBuilder::new();
        let (s, p, o) = (
            iri(&mut bb, "s"),
            iri(&mut bb, "p"),
            iri(&mut bb, "DIFFERENT"),
        );
        bb.push_quad(s, p, o, None);
        let b = bb.freeze().expect("valid");
        assert!(!datasets_isomorphic(&a, &b));
    }

    /// HEADLINE GATE: differing only in reifier COUNT for the same triple →
    /// NOT isomorphic. Oxigraph canonicalization would hide this.
    #[test]
    fn reifier_count_difference_is_not_isomorphic() {
        let build = |reifiers: &[&str]| -> Arc<RdfDataset> {
            let mut b = RdfDatasetBuilder::new();
            let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
            let triple = b.intern_triple(s, p, o);
            b.push_quad(s, p, o, None);
            for r in reifiers {
                let rid = iri(&mut b, r);
                b.push_reifier(rid, triple);
            }
            b.freeze().expect("valid")
        };
        let one = build(&["r1"]);
        let two = build(&["r1", "r2"]);
        assert!(
            !datasets_isomorphic(&one, &two),
            "TWO reifiers vs ONE must compare unequal"
        );
        // And the same reifier set IS isomorphic.
        let one_again = build(&["r1"]);
        assert!(datasets_isomorphic(&one, &one_again));
    }

    #[test]
    fn annotation_difference_is_not_isomorphic() {
        let build = |with_annotation: bool| -> Arc<RdfDataset> {
            let mut b = RdfDatasetBuilder::new();
            let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
            let triple = b.intern_triple(s, p, o);
            let r = iri(&mut b, "r");
            b.push_quad(s, p, o, None);
            b.push_reifier(r, triple);
            if with_annotation {
                let ap = iri(&mut b, "ap");
                let ao = iri(&mut b, "ao");
                b.push_annotation(r, ap, ao);
            }
            b.freeze().expect("valid")
        };
        assert!(!datasets_isomorphic(&build(true), &build(false)));
        assert!(datasets_isomorphic(&build(true), &build(true)));
    }

    /// Directional / datatype literal differences → NOT isomorphic (ground equality).
    #[test]
    fn directional_literal_difference_is_not_isomorphic() {
        let build = |dir: Option<RdfTextDirection>| -> Arc<RdfDataset> {
            let mut b = RdfDatasetBuilder::new();
            let (s, p) = (iri(&mut b, "s"), iri(&mut b, "p"));
            let lit = b.intern_literal(RdfLiteral {
                lexical_form: "x".to_owned(),
                datatype: None,
                language: Some("en".to_owned()),
                direction: dir,
            });
            b.push_quad(s, p, lit, None);
            b.freeze().expect("valid")
        };
        assert!(!datasets_isomorphic(
            &build(Some(RdfTextDirection::Ltr)),
            &build(Some(RdfTextDirection::Rtl))
        ));
        assert!(datasets_isomorphic(
            &build(Some(RdfTextDirection::Ltr)),
            &build(Some(RdfTextDirection::Ltr))
        ));
    }

    /// BLANK BIJECTION: same structure, different blank labels AND scopes → TRUE.
    #[test]
    fn blank_bijection_same_structure_different_labels_is_isomorphic() {
        use super::super::term::BlankScope;
        let build = |label: &str, scope: u32| -> Arc<RdfDataset> {
            let mut b = RdfDatasetBuilder::new();
            let (s, p) = (iri(&mut b, "s"), iri(&mut b, "p"));
            let blank = b.intern_blank(label.to_owned(), BlankScope(scope));
            b.push_quad(s, p, blank, None);
            b.freeze().expect("valid")
        };
        let a = build("b1", 0);
        let b = build("xyz", 7);
        assert!(
            datasets_isomorphic(&a, &b),
            "blanks differ only by label/scope; structure is identical"
        );
    }

    /// BLANK BIJECTION negative: genuinely different blank wiring → FALSE.
    /// `a`: one blank linked to ex:o1. `b`: one blank linked to ex:o2.
    #[test]
    fn blank_different_wiring_is_not_isomorphic() {
        use super::super::term::BlankScope;
        let build = |neighbour: &str| -> Arc<RdfDataset> {
            let mut b = RdfDatasetBuilder::new();
            let (s, p, link) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "link"));
            let blank = b.intern_blank("b".to_owned(), BlankScope::DEFAULT);
            let nb = iri(&mut b, neighbour);
            b.push_quad(s, p, blank, None);
            b.push_quad(blank, link, nb, None);
            b.freeze().expect("valid")
        };
        assert!(!datasets_isomorphic(&build("o1"), &build("o2")));
    }

    /// Two blanks with swapped-but-equivalent wiring stay isomorphic under bijection.
    #[test]
    fn two_blanks_relabeled_is_isomorphic() {
        use super::super::term::BlankScope;
        // a: _:x ex:p ex:A ; _:y ex:p ex:B
        // b: _:m ex:p ex:A ; _:n ex:p ex:B  (different labels/scopes)
        let build = |l1: &str, s1: u32, l2: &str, s2: u32| -> Arc<RdfDataset> {
            let mut b = RdfDatasetBuilder::new();
            let p = iri(&mut b, "p");
            let a_node = iri(&mut b, "A");
            let b_node = iri(&mut b, "B");
            let x = b.intern_blank(l1.to_owned(), BlankScope(s1));
            let y = b.intern_blank(l2.to_owned(), BlankScope(s2));
            b.push_quad(x, p, a_node, None);
            b.push_quad(y, p, b_node, None);
            b.freeze().expect("valid")
        };
        let a = build("x", 0, "y", 0);
        let b = build("m", 3, "n", 9);
        assert!(datasets_isomorphic(&a, &b));
    }

    #[test]
    fn dataset_diff_reports_counts_and_verdict() {
        let a = ground_triple();
        let b = ground_triple();
        let diff = dataset_diff(&a, &b);
        assert_eq!(diff.quad_counts, (1, 1));
        assert!(diff.isomorphic);
    }
}
