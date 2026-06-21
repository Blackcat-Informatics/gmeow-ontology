// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The IR→`RdfStore` coexistence bridge (#819 C1/C2 → C3).
//!
//! The frozen [`RdfDataset`] (C1) is the new authoritative graph representation,
//! but the existing consumers (SHACL, validate, LOGIC, the oxigraph materializer)
//! are written against the [`RdfStore`] trait and its **owned** model types
//! ([`RdfQuad`] / [`RdfReifier`] / [`RdfAnnotation`]). This module makes a borrowed
//! `&RdfDataset` directly usable wherever an `RdfStore` is expected, so those
//! consumers keep working WITHOUT being ported (porting them is C4/C5; deleting the
//! old shim is C8). This is purely a coexistence bridge.
//!
//! The adapter resolves each ID-native [`QuadIds`] / reifier / annotation row to the
//! owned model types at the boundary. That **does allocate** — one owned `RdfQuad`
//! per quad, with owned strings. That is expected and acceptable here: the
//! zero-allocation guarantee lives on the native [`RdfDataset::quads`] surface, not
//! on this compatibility wrapper, which exists precisely to feed allocation-based
//! legacy consumers.
//!
//! ## Direction at the owned boundary
//!
//! The owned [`RdfQuad`] model (`model.rs`) carries no base-direction field on a
//! quad's positions itself, but base direction lives on the literal
//! ([`RdfLiteral::direction`]), and quads/triples reference terms — so a directional
//! literal resolved through this adapter **preserves its direction** via
//! [`RdfTerm::Literal`]. No direction is lost crossing the owned boundary.

use crate::{
    RdfAnnotation, RdfDiagnostic, RdfLiteral, RdfQuad, RdfReifier, RdfStore, RdfStoreCapabilities,
    RdfTerm, RdfTriple,
};

use super::dataset::{QuadHandle, QuadIds, RdfDataset, TermRef};
use super::term::TermId;

impl RdfDataset {
    /// View this frozen dataset as an [`RdfStore`], so it can feed any existing
    /// consumer (SHACL / validate / LOGIC / the oxigraph materializer) without
    /// porting that consumer to the IR. The returned value borrows `self`.
    ///
    /// A plain `&dataset` is *already* an `RdfStore` (the trait is implemented for
    /// `&RdfDataset`); this is the explicit, self-documenting accessor.
    pub fn as_rdf_store(&self) -> impl RdfStore + '_ {
        self
    }

    /// Resolve a term id to the owned [`RdfTerm`] model, recursively for triple
    /// terms. This allocates owned strings — the boundary cost of feeding owned-model
    /// consumers (see the module docs).
    fn to_owned_term(&self, id: TermId) -> RdfTerm {
        match self.resolve(id) {
            TermRef::Iri(iri) => RdfTerm::iri(iri),
            TermRef::Blank { label, scope } => RdfTerm::blank_node(scope.qualify_label(label)),
            TermRef::Literal {
                lexical,
                datatype,
                language,
                direction,
            } => {
                // The datatype is interned as an IRI term (C0.1); resolve it back to
                // its IRI string for the owned model.
                let datatype_iri = match self.resolve(datatype) {
                    TermRef::Iri(iri) => iri.to_owned(),
                    other => {
                        // A literal's datatype is always an interned IRI by
                        // construction (builder C0.1); anything else is a frozen-IR
                        // invariant violation, not a recoverable input error.
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

    /// Resolve a term id that must be an IRI (a predicate / graph-name / triple
    /// predicate position) to its owned IRI string.
    fn iri_string(&self, id: TermId) -> String {
        match self.resolve(id) {
            TermRef::Iri(iri) => iri.to_owned(),
            other => unreachable!("expected an IRI in this position, got {other:?}"),
        }
    }

    /// Resolve one ID-native quad row to an owned [`RdfQuad`], attaching the quad's
    /// source location (by its FROZEN ordinal) so consumers reading through the
    /// bridge — diagnostics/SARIF, validate lints — see the same positions the IR
    /// holds. Without this the bridge silently dropped every location.
    fn to_owned_quad(&self, frozen_index: usize, q: QuadIds) -> RdfQuad {
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
    fn to_owned_reifier(&self, reifier: TermId, triple: TermId) -> RdfReifier {
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
    fn to_owned_annotation(&self, reifier: TermId, p: TermId, o: TermId) -> RdfAnnotation {
        RdfAnnotation::new(
            self.to_owned_term(reifier),
            self.iri_string(p),
            self.to_owned_term(o),
        )
    }
}

/// The coexistence bridge: a borrowed frozen dataset IS an [`RdfStore`].
///
/// Implemented for `&RdfDataset` (not `RdfDataset` by value) because every consumer
/// holds the dataset behind an `Arc`/borrow and the trait's iterator methods only
/// need shared access. The orphan rule is satisfied: both `RdfStore` and
/// `RdfDataset` are local to this crate.
impl RdfStore for &RdfDataset {
    fn quads(&self) -> Box<dyn Iterator<Item = Result<RdfQuad, RdfDiagnostic>> + '_> {
        Box::new(
            RdfDataset::quads(self)
                .enumerate()
                .map(move |(i, q)| Ok(self.to_owned_quad(i, q))),
        )
    }

    fn reifiers(&self) -> Box<dyn Iterator<Item = Result<RdfReifier, RdfDiagnostic>> + '_> {
        Box::new(RdfDataset::reifiers(self).map(move |(r, t)| Ok(self.to_owned_reifier(r, t))))
    }

    fn annotations(&self) -> Box<dyn Iterator<Item = Result<RdfAnnotation, RdfDiagnostic>> + '_> {
        Box::new(
            RdfDataset::annotations(self)
                .map(move |(r, p, o)| Ok(self.to_owned_annotation(r, p, o))),
        )
    }

    fn capabilities(&self) -> RdfStoreCapabilities {
        RdfDataset::capabilities(self)
    }

    // `lookaside()` intentionally inherits the trait default (an empty
    // `RdfLookaside`): the dataset alone is the hot graph; out-of-band material
    // lives in the `GtsBundle`'s envelope, not in a bare dataset (C0.6).

    fn len_hint(&self) -> Option<usize> {
        Some(self.quad_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RdfDatasetBuilder;
    use crate::{RdfLiteral, RdfTermKind, RdfTextDirection};

    fn iri(b: &mut RdfDatasetBuilder, n: &str) -> TermId {
        b.intern_iri(format!("http://example.org/{n}"))
    }

    #[test]
    fn adapter_resolves_quads_to_owned_model() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        b.push_quad(s, p, o, None);
        let ds = b.freeze().expect("valid");

        let store = ds.as_ref();
        let quads: Vec<_> = RdfStore::quads(&store)
            .collect::<Result<_, _>>()
            .expect("ok");
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        assert_eq!(q.predicate, "http://example.org/p");
        assert_eq!(q.subject, RdfTerm::iri("http://example.org/s"));
        assert!(q.graph_name.is_none());
        assert_eq!(RdfStore::len_hint(&store), Some(1));
    }

    #[test]
    fn adapter_preserves_directional_literal() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p) = (iri(&mut b, "s"), iri(&mut b, "p"));
        let lit = b.intern_literal(RdfLiteral {
            lexical_form: "مرحبا".to_owned(),
            datatype: None,
            language: Some("ar".to_owned()),
            direction: Some(RdfTextDirection::Rtl),
        });
        b.push_quad(s, p, lit, None);
        let ds = b.freeze().expect("valid");

        let store = ds.as_ref();
        let quads: Vec<_> = RdfStore::quads(&store)
            .collect::<Result<_, _>>()
            .expect("ok");
        match &quads[0].object {
            RdfTerm::Literal(l) => {
                assert_eq!(l.lexical_form, "مرحبا");
                assert_eq!(l.language.as_deref(), Some("ar"));
                assert_eq!(l.direction, Some(RdfTextDirection::Rtl));
            }
            other => panic!("expected literal, got {other:?}"),
        }
    }

    #[test]
    fn adapter_resolves_reifier_and_annotation() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        let triple = b.intern_triple(s, p, o);
        let r = iri(&mut b, "r");
        let ap = iri(&mut b, "ap");
        let ao = iri(&mut b, "ao");
        b.push_reifier(r, triple);
        b.push_annotation(r, ap, ao);
        let ds = b.freeze().expect("valid");

        let store = ds.as_ref();
        let reifiers: Vec<_> = RdfStore::reifiers(&store)
            .collect::<Result<_, _>>()
            .expect("ok");
        assert_eq!(reifiers.len(), 1);
        assert_eq!(reifiers[0].reifier, RdfTerm::iri("http://example.org/r"));
        assert_eq!(reifiers[0].statement.subject.kind(), RdfTermKind::Iri);

        let annotations: Vec<_> = RdfStore::annotations(&store)
            .collect::<Result<_, _>>()
            .expect("ok");
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].predicate, "http://example.org/ap");

        let caps = RdfStore::capabilities(&store);
        assert!(caps.reifiers);
        assert!(caps.annotations);
    }

    #[test]
    fn adapter_threads_quad_location() {
        // A location attached at build time must survive across the compat bridge:
        // the owned `RdfQuad` carries it. Previously the bridge dropped it.
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        let h = b.next_quad_handle();
        b.push_quad(s, p, o, None);
        b.attach_location(h, crate::RdfLocation::logical("slice:example"));
        let ds = b.freeze().expect("valid");

        let store = ds.as_ref();
        let quads: Vec<_> = RdfStore::quads(&store)
            .collect::<Result<_, _>>()
            .expect("ok");
        assert_eq!(
            quads[0]
                .location
                .as_ref()
                .and_then(|l| l.logical.as_deref()),
            Some("slice:example"),
            "the bridge must thread the quad's IR location into the owned model"
        );
    }

    #[test]
    fn adapter_lookaside_is_empty() {
        let mut b = RdfDatasetBuilder::new();
        let (s, p, o) = (iri(&mut b, "s"), iri(&mut b, "p"), iri(&mut b, "o"));
        b.push_quad(s, p, o, None);
        let ds = b.freeze().expect("valid");
        let store = ds.as_ref();
        assert!(RdfStore::lookaside(&store).is_empty());
    }
}
