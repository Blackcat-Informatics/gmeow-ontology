// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The fallible `RdfDataset` builder: value-interning plus the quad / reifier /
//! annotation / source-location tables, and the validate-then-freeze path (#819 C1).
//!
//! This module owns term storage and value-interning. The C0 literal-identity
//! policy (datatype expansion, language lowercasing, direction-in-key, verbatim
//! lexical spelling — see `docs/design/819-rdf-ir-dataflow.md` *Appendix C0.1*) is
//! applied here, at intern time, so that the frozen dataset carries fully resolved
//! identity.
//!
//! Pushing structure (`push_quad` / `push_reifier` / `push_annotation` /
//! `attach_location`) accumulates raw rows; [`RdfDatasetBuilder::freeze`] then runs
//! structural validation ([`super::validate`]) and, on success, materializes an
//! immutable, deterministically-ordered, deduplicated [`RdfDataset`]. Per the
//! no-optionality doctrine, malformed structure is a HARD failure (`Err`), never a
//! silent default.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::RdfLiteral;

use super::dataset::{QuadHandle, QuadRow, RdfDataset};
use super::term::{BlankScope, InternedLiteral, InternedTerm, TermId, RDF_LANG_STRING, XSD_STRING};
use crate::RdfLocation;

/// Term storage + value-interning dedup + the C0 identity policy, in one cohesive
/// unit (SRP). Private: the builder is the only public surface.
struct Interner {
    /// Dense table of interned terms, addressed by [`TermId::index`].
    terms: Vec<InternedTerm>,
    /// Value → id index enforcing one id per distinct interned value.
    index: HashMap<InternedTerm, TermId>,
}

impl Interner {
    fn new() -> Self {
        Self {
            terms: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Intern a fully-formed [`InternedTerm`], returning its (possibly existing)
    /// id. Idempotent: equal values map to the same id.
    fn intern(&mut self, term: InternedTerm) -> TermId {
        if let Some(&id) = self.index.get(&term) {
            return id;
        }
        let id = TermId::from_index(
            u32::try_from(self.terms.len()).expect("term table exceeds u32::MAX entries"),
        );
        self.terms.push(term.clone());
        self.index.insert(term, id);
        id
    }

    fn term(&self, id: TermId) -> &InternedTerm {
        &self.terms[id.index()]
    }

    fn term_count(&self) -> usize {
        self.terms.len()
    }
}

/// The fallible builder that interns terms, accumulates structure, and freezes
/// into an immutable `Arc<RdfDataset>`.
///
/// Pushed structure is accumulated in deterministic insertion order; quads and
/// annotations are deduplicated *during* push (the dataset is a set, C0.5), while
/// [`freeze`](RdfDatasetBuilder::freeze) re-sorts everything into a stable,
/// reproducible order.
pub struct RdfDatasetBuilder {
    /// Owns terms + the value-intern index + the C0 identity policy.
    interner: Interner,
    /// Deduplicated quad rows in first-seen order; `g == None` is the default graph.
    quads: Vec<QuadRow>,
    /// Membership set collapsing duplicate quads to one row (C0.5).
    quad_set: HashSet<QuadRow>,
    /// `(reifier, triple-term)` bindings. Several reifiers MAY bind one triple term
    /// and the same binding MAY be pushed more than once; duplicates collapse (C0.4).
    reifiers: Vec<(TermId, TermId)>,
    reifier_set: HashSet<(TermId, TermId)>,
    /// `(reifier, predicate, object)` annotations; duplicates collapse (C0.5).
    annotations: Vec<(TermId, TermId, TermId)>,
    annotation_set: HashSet<(TermId, TermId, TermId)>,
    /// Sparse source locations keyed by the pushed-quad ordinal. Only quads with a
    /// recorded location appear here.
    locations: Vec<(QuadHandle, RdfLocation)>,
}

impl Default for RdfDatasetBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RdfDatasetBuilder {
    /// A fresh, empty builder.
    pub fn new() -> Self {
        Self {
            interner: Interner::new(),
            quads: Vec::new(),
            quad_set: HashSet::new(),
            reifiers: Vec::new(),
            reifier_set: HashSet::new(),
            annotations: Vec::new(),
            annotation_set: HashSet::new(),
            locations: Vec::new(),
        }
    }

    /// Intern an IRI term. Idempotent: the same IRI string yields the same id.
    pub fn intern_iri(&mut self, iri: String) -> TermId {
        self.interner
            .intern(InternedTerm::Iri(iri.into_boxed_str()))
    }

    /// Intern a blank node. Identity is `(label, scope)` (C0.2): same label + same
    /// scope → same id; same label + different scope → different id.
    pub fn intern_blank(&mut self, label: String, scope: BlankScope) -> TermId {
        self.interner.intern(InternedTerm::Blank {
            label: label.into_boxed_str(),
            scope,
        })
    }

    /// Intern a literal, applying the C0.1 identity policy:
    ///
    /// - A language tag → datatype `rdf:langString`; the language is lowercased
    ///   for the key.
    /// - Otherwise an explicit datatype → that datatype.
    /// - Otherwise → `xsd:string`.
    ///
    /// The datatype is always stored as an interned IRI [`TermId`]. The lexical
    /// form is preserved byte-for-byte; base direction participates in identity.
    pub fn intern_literal(&mut self, lit: RdfLiteral) -> TermId {
        let RdfLiteral {
            lexical_form,
            datatype,
            language,
            direction,
        } = lit;

        // C0.1: a language tag forces rdf:langString and a lowercased language key,
        // regardless of any (illegal) explicit datatype on the input literal.
        let (datatype_iri, language_key) = match language {
            Some(lang) => (RDF_LANG_STRING.to_string(), Some(lang.to_lowercase())),
            None => match datatype {
                Some(dt) => (dt, None),
                None => (XSD_STRING.to_string(), None),
            },
        };

        let datatype_id = self.intern_iri(datatype_iri);

        self.interner.intern(InternedTerm::Literal(InternedLiteral {
            lexical_form: lexical_form.into_boxed_str(),
            datatype: datatype_id,
            language: language_key.map(String::into_boxed_str),
            direction,
        }))
    }

    /// Intern a triple term (RDF 1.2 quoted triple). Identified structurally by the
    /// resolved `(s, p, o)` ids (C0.3); dedup is by that triple. Acyclicity is a
    /// freeze-time concern ([`super::validate`]), not enforced here.
    pub fn intern_triple(&mut self, s: TermId, p: TermId, o: TermId) -> TermId {
        self.interner.intern(InternedTerm::Triple { s, p, o })
    }

    /// Crate-internal read access to an interned term. [`freeze`](Self::freeze) and
    /// [`super::validate`] consume this to materialize and check the dataset.
    pub(crate) fn term(&self, id: TermId) -> &InternedTerm {
        self.interner.term(id)
    }

    /// The number of distinct interned terms. Used by validation (the ID-reference
    /// bound) and as the frozen dataset's term count.
    pub(crate) fn term_count(&self) -> usize {
        self.interner.term_count()
    }

    /// Push a quad. Duplicate quads collapse to a single row (C0.5); `g == None`
    /// names the default graph. Returns nothing — the quad's ordinal is reflected
    /// by [`attach_location`](Self::attach_location), which keys off the pushed
    /// (deduped) order via [`QuadHandle`].
    pub fn push_quad(&mut self, s: TermId, p: TermId, o: TermId, g: Option<TermId>) {
        let row = QuadRow { s, p, o, g };
        if self.quad_set.insert(row) {
            self.quads.push(row);
        }
    }

    /// Bind a reifier resource to a triple term (C0.4). Several reifiers MAY bind
    /// one triple term; an identical `(reifier, triple)` binding pushed twice
    /// collapses to one.
    pub fn push_reifier(&mut self, reifier: TermId, triple: TermId) {
        let binding = (reifier, triple);
        if self.reifier_set.insert(binding) {
            self.reifiers.push(binding);
        }
    }

    /// Push a statement annotation `(reifier, predicate, object)`. Duplicate
    /// annotations collapse to one (C0.5).
    pub fn push_annotation(&mut self, reifier: TermId, p: TermId, o: TermId) {
        let annotation = (reifier, p, o);
        if self.annotation_set.insert(annotation) {
            self.annotations.push(annotation);
        }
    }

    /// Attach a source location to a previously pushed quad, identified by its
    /// [`QuadHandle`] (the dense ordinal of the deduplicated quad). Sparse: only
    /// quads with a recorded location are stored. An empty location is ignored.
    pub fn attach_location(&mut self, handle: QuadHandle, loc: RdfLocation) {
        if !loc.is_empty() {
            self.locations.push((handle, loc));
        }
    }

    /// The [`QuadHandle`] that the next [`push_quad`](Self::push_quad) call will
    /// assign to a *newly seen* quad — i.e. the current deduplicated-quad count.
    /// Callers that need to attach a location pair this with the immediately
    /// following push.
    pub fn next_quad_handle(&self) -> QuadHandle {
        QuadHandle::from_index(self.quads.len() as u32)
    }

    /// Validate structure (positional constraints, ID-reference validity,
    /// triple-term acyclicity) and FREEZE into an immutable, deterministically
    /// ordered, deduplicated `Arc<RdfDataset>`.
    ///
    /// Per the no-optionality doctrine this HARD-fails (`Err`) on malformed
    /// structure — there is no degraded fallback and no silent default.
    pub fn freeze(self) -> Result<Arc<RdfDataset>, RdfDiagnostic> {
        super::validate::validate(&self)?;
        Ok(Arc::new(self.materialize()))
    }

    /// Borrow the accumulated quad rows (validation reads these).
    pub(crate) fn quad_rows(&self) -> &[QuadRow] {
        &self.quads
    }

    /// Borrow the accumulated reifier bindings (validation reads these).
    pub(crate) fn reifier_rows(&self) -> &[(TermId, TermId)] {
        &self.reifiers
    }

    /// Borrow the accumulated annotation rows (validation reads these).
    pub(crate) fn annotation_rows(&self) -> &[(TermId, TermId, TermId)] {
        &self.annotations
    }

    /// Consume the builder and materialize the frozen dataset. Called only by
    /// [`freeze`](Self::freeze) AFTER validation has passed.
    fn materialize(self) -> RdfDataset {
        let RdfDatasetBuilder {
            interner,
            mut quads,
            mut reifiers,
            mut annotations,
            mut locations,
            ..
        } = self;

        // Deterministic, reproducible frozen order: sort by id tuples. Terms keep
        // their interning (allocation) order, which is itself deterministic for a
        // fixed push sequence.
        quads.sort_unstable();
        reifiers.sort_unstable();
        annotations.sort_unstable();
        locations.sort_unstable_by_key(|(handle, _)| *handle);

        let caps =
            compute_capabilities(&interner.terms, &quads, &reifiers, &annotations, &locations);

        RdfDataset::from_parts(
            interner.terms.into_boxed_slice(),
            quads.into_boxed_slice(),
            reifiers.into_boxed_slice(),
            annotations.into_boxed_slice(),
            locations.into_boxed_slice(),
            caps,
        )
    }
}

use crate::{RdfDiagnostic, RdfStoreCapabilities};

/// Compute the dataset's capability flags ONCE at freeze, from the frozen tables.
fn compute_capabilities(
    terms: &[InternedTerm],
    quads: &[QuadRow],
    reifiers: &[(TermId, TermId)],
    annotations: &[(TermId, TermId, TermId)],
    locations: &[(QuadHandle, RdfLocation)],
) -> RdfStoreCapabilities {
    RdfStoreCapabilities {
        named_graphs: quads.iter().any(|q| q.g.is_some()),
        quoted_triples: terms
            .iter()
            .any(|t| matches!(t, InternedTerm::Triple { .. })),
        reifiers: !reifiers.is_empty(),
        annotations: !annotations.is_empty(),
        source_locations: !locations.is_empty(),
        // The frozen dataset is the hot graph only; envelope concerns (loss
        // records, lookaside) live elsewhere (C0.6).
        loss_records: false,
        lookaside: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RdfTextDirection;
    use proptest::prelude::*;

    fn lit_simple(s: &str) -> RdfLiteral {
        RdfLiteral::simple(s)
    }

    #[test]
    fn intern_iri_is_idempotent() {
        let mut b = RdfDatasetBuilder::new();
        let a = b.intern_iri("http://example.org/x".to_string());
        let c = b.intern_iri("http://example.org/x".to_string());
        let d = b.intern_iri("http://example.org/y".to_string());
        assert_eq!(a, c);
        assert_ne!(a, d);
    }

    #[test]
    fn intern_blank_is_idempotent() {
        let mut b = RdfDatasetBuilder::new();
        let a = b.intern_blank("b0".to_string(), BlankScope::DEFAULT);
        let c = b.intern_blank("b0".to_string(), BlankScope::DEFAULT);
        assert_eq!(a, c);
    }

    #[test]
    fn intern_literal_is_idempotent() {
        let mut b = RdfDatasetBuilder::new();
        let a = b.intern_literal(lit_simple("x"));
        let c = b.intern_literal(lit_simple("x"));
        assert_eq!(a, c);
    }

    /// C0.1: a plain literal expands to `xsd:string`, so `"x"` and an explicit
    /// `"x"^^xsd:string` intern to the same id.
    #[test]
    fn datatype_expansion_equality() {
        let mut b = RdfDatasetBuilder::new();
        let plain = b.intern_literal(RdfLiteral::simple("x"));
        let explicit = b.intern_literal(RdfLiteral::typed("x", XSD_STRING));
        assert_eq!(plain, explicit);
    }

    /// C0.1: base direction participates in identity — same lexical + language but
    /// different direction are distinct.
    #[test]
    fn directional_literal_distinctness() {
        let mut b = RdfDatasetBuilder::new();
        let base = RdfLiteral {
            lexical_form: "x".to_string(),
            datatype: None,
            language: Some("en".to_string()),
            direction: Some(RdfTextDirection::Ltr),
        };
        let mut other = base.clone();
        other.direction = Some(RdfTextDirection::Rtl);
        let none = RdfLiteral {
            direction: None,
            ..base.clone()
        };
        let ltr = b.intern_literal(base);
        let rtl = b.intern_literal(other);
        let no_dir = b.intern_literal(none);
        assert_ne!(ltr, rtl);
        assert_ne!(ltr, no_dir);
        assert_ne!(rtl, no_dir);
    }

    /// C0.1: language tags are lowercased for the key, so `@EN` and `@en` are equal.
    #[test]
    fn language_lowercasing() {
        let mut b = RdfDatasetBuilder::new();
        let upper = b.intern_literal(RdfLiteral::language_tagged("x", "EN"));
        let lower = b.intern_literal(RdfLiteral::language_tagged("x", "en"));
        assert_eq!(upper, lower);
    }

    /// A language-tagged literal expands to `rdf:langString`, distinct from a plain
    /// `xsd:string` literal of the same lexical form.
    #[test]
    fn lang_tagged_distinct_from_plain() {
        let mut b = RdfDatasetBuilder::new();
        let plain = b.intern_literal(RdfLiteral::simple("x"));
        let tagged = b.intern_literal(RdfLiteral::language_tagged("x", "en"));
        assert_ne!(plain, tagged);
    }

    /// C0.2: blank-node scope participates in the key.
    #[test]
    fn blank_scope_distinctness() {
        let mut b = RdfDatasetBuilder::new();
        let s1 = b.intern_blank("b".to_string(), BlankScope(1));
        let s2 = b.intern_blank("b".to_string(), BlankScope(2));
        let s1_again = b.intern_blank("b".to_string(), BlankScope(1));
        assert_ne!(s1, s2);
        assert_eq!(s1, s1_again);
    }

    /// C0.3: triple terms are identified structurally by resolved `(s, p, o)`, and
    /// a triple term nests as the object of another triple and stays reusable.
    #[test]
    fn nested_triple_term_structural_identity() {
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri("http://example.org/s".to_string());
        let p = b.intern_iri("http://example.org/p".to_string());
        let o = b.intern_iri("http://example.org/o".to_string());

        let t1 = b.intern_triple(s, p, o);
        let t2 = b.intern_triple(s, p, o);
        assert_eq!(t1, t2, "same (s,p,o) → same triple-term id");

        // Nest the triple term as the object of an outer triple, twice.
        let outer_p = b.intern_iri("http://example.org/asserts".to_string());
        let outer1 = b.intern_triple(s, outer_p, t1);
        let outer2 = b.intern_triple(s, outer_p, t2);
        assert_eq!(outer1, outer2, "nested triple term is reusable by id");

        // The inner triple term remains a distinct, single interned term.
        let t3 = b.intern_triple(s, p, o);
        assert_eq!(t1, t3);
    }

    /// A small helper interning a fresh IRI by suffix.
    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(format!("http://example.org/{n}"))
    }

    #[test]
    fn freeze_dedupes_quads() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        b.push_quad(s, p, o, None);
        b.push_quad(s, p, o, None);
        let ds = b.freeze().expect("valid");
        assert_eq!(ds.quads().count(), 1, "duplicate quads collapse to one row");
    }

    #[test]
    fn freeze_preserves_named_graphs() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        let g = iri(&mut b, "g");
        b.push_quad(s, p, o, None);
        b.push_quad(s, p, o, Some(g));
        let ds = b.freeze().expect("valid");
        assert_eq!(
            ds.quads().count(),
            2,
            "default and named graph are distinct"
        );
        assert!(ds.capabilities().named_graphs);
        let graphs: Vec<_> = ds.quads().map(|q| q.g).collect();
        assert!(graphs.contains(&None));
        assert!(graphs.contains(&Some(g)));
    }

    #[test]
    fn freeze_keeps_multiple_reifiers_for_one_triple() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        let triple = b.intern_triple(s, p, o);
        let r1 = iri(&mut b, "r1");
        let r2 = iri(&mut b, "r2");
        b.push_reifier(r1, triple);
        b.push_reifier(r2, triple);
        b.push_reifier(r1, triple); // duplicate binding collapses
        let ds = b.freeze().expect("valid");
        let reifiers: Vec<_> = ds.reifiers().collect();
        assert_eq!(
            reifiers.len(),
            2,
            "two distinct reifiers survive, dup collapses"
        );
        assert!(reifiers.contains(&(r1, triple)));
        assert!(reifiers.contains(&(r2, triple)));
    }

    #[test]
    fn freeze_dedupes_annotations() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        let triple = b.intern_triple(s, p, o);
        let r = iri(&mut b, "r");
        let ap = iri(&mut b, "ap");
        let ao = iri(&mut b, "ao");
        b.push_reifier(r, triple);
        b.push_annotation(r, ap, ao);
        b.push_annotation(r, ap, ao);
        let ds = b.freeze().expect("valid");
        assert_eq!(
            ds.annotations().count(),
            1,
            "duplicate annotation collapses"
        );
    }

    /// A `Strategy` producing one of a small fixed pool of distinct intern calls,
    /// so we can count the distinct *values* requested and compare to `term_count`.
    #[derive(Clone, Debug)]
    enum Op {
        Iri(u8),
        Blank(u8, u32),
        Literal(u8),
    }

    fn op_strategy() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u8..4).prop_map(Op::Iri),
            (0u8..4, 0u32..3).prop_map(|(l, s)| Op::Blank(l, s)),
            (0u8..4).prop_map(Op::Literal),
        ]
    }

    proptest! {
        /// Idempotence holds across arbitrary call sequences, and `term_count`
        /// never exceeds the number of distinct values interned.
        ///
        /// The distinct-value count is computed independently of the interner: a
        /// literal also interns its datatype IRI, so each literal value contributes
        /// itself plus its (shared) datatype term to the upper bound.
        #[test]
        fn proptest_idempotence_and_bounded_count(ops in prop::collection::vec(op_strategy(), 0..64)) {
            use std::collections::HashSet;

            let mut b = RdfDatasetBuilder::new();
            // Map a value-key → the id it first produced, to assert idempotence.
            let mut seen: HashMap<String, TermId> = HashMap::new();
            // The set of distinct *terms* (value keys, incl. datatype IRIs) that
            // SHOULD exist after the run — the exact upper bound for term_count.
            let mut distinct_terms: HashSet<String> = HashSet::new();

            for op in ops {
                let (call_key, id) = match op {
                    Op::Iri(n) => {
                        let iri = format!("http://example.org/i{n}");
                        distinct_terms.insert(format!("iri:{iri}"));
                        (format!("iri:{iri}"), b.intern_iri(iri))
                    }
                    Op::Blank(l, s) => {
                        let key = format!("blank:{l}:{s}");
                        distinct_terms.insert(key.clone());
                        (key, b.intern_blank(format!("b{l}"), BlankScope(s)))
                    }
                    Op::Literal(n) => {
                        let lex = format!("v{n}");
                        // Plain literal → xsd:string; both the literal and its
                        // datatype IRI become distinct interned terms.
                        distinct_terms.insert(format!("lit:{lex}"));
                        distinct_terms.insert(format!("iri:{XSD_STRING}"));
                        (format!("lit:{lex}"), b.intern_literal(RdfLiteral::simple(lex)))
                    }
                };

                match seen.get(&call_key) {
                    Some(&prev) => prop_assert_eq!(prev, id, "intern not idempotent"),
                    None => {
                        seen.insert(call_key, id);
                    }
                }
            }

            prop_assert_eq!(b.term_count(), distinct_terms.len());
        }
    }
}
