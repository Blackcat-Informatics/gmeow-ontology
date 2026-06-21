// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The interning half of the immutable `RdfDataset` builder (#819 C1).
//!
//! This module owns term storage and value-interning. The C0 literal-identity
//! policy (datatype expansion, language lowercasing, direction-in-key, verbatim
//! lexical spelling — see `docs/design/819-rdf-ir-dataflow.md` *Appendix C0.1*) is
//! applied here, at intern time, so that the frozen dataset carries fully resolved
//! identity.
//!
//! Freeze, structural validation (positional constraints, orphan-id rejection,
//! triple-term cycle rejection) and the quad/reifier/annotation tables are Task 3
//! concerns and are intentionally absent here.

use std::collections::HashMap;

use crate::RdfLiteral;

use super::term::{BlankScope, InternedLiteral, InternedTerm, TermId, RDF_LANG_STRING, XSD_STRING};

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

    #[allow(dead_code)]
    fn term_count(&self) -> usize {
        self.terms.len()
    }
}

/// The fallible builder that interns terms and (in Task 3) freezes into an
/// immutable `Arc<RdfDataset>`.
pub struct RdfDatasetBuilder {
    /// Owns terms + the value-intern index + the C0 identity policy.
    interner: Interner,
    // Task 3: quads / reifiers / annotations / source-location tables / diagnostics
    // fields plus `push_*`/`freeze` will be added here.
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
    /// Task-3 freeze concern, not enforced here.
    pub fn intern_triple(&mut self, s: TermId, p: TermId, o: TermId) -> TermId {
        self.interner.intern(InternedTerm::Triple { s, p, o })
    }

    /// Crate-internal read access to an interned term. Task 3 (freeze) consumes
    /// this — kept now as the read seam the dataset materializer will use.
    #[allow(dead_code)]
    pub(crate) fn term(&self, id: TermId) -> &InternedTerm {
        self.interner.term(id)
    }

    /// The number of distinct interned terms. Exercised by the property test;
    /// Task 3 (freeze) also consumes it.
    #[allow(dead_code)]
    pub(crate) fn term_count(&self) -> usize {
        self.interner.term_count()
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
