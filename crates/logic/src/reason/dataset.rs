// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Borrowed native RDF access for structural reasoning over base and delta views.
//! Only the row being inspected is owned; the input dictionaries and indexes stay
//! with PurRDF. Statement-layer rows retain their own graph and term identity.

use purrdf::dataset_view::TermGuard;
use purrdf::{DatasetView, RdfLiteral, RdfQuad, RdfTerm, RdfTriple, TermRef, TermValue};

/// Recover native term identity from the owned RDF model without a text round trip.
pub(super) fn value(term: &RdfTerm) -> TermValue {
    match term {
        RdfTerm::Iri(iri) => TermValue::iri(iri),
        RdfTerm::BlankNode(label) => {
            let (label, scope) = purrdf::BlankScope::unqualify_label(label);
            TermValue::Blank {
                label: label.into_owned(),
                scope,
            }
        }
        RdfTerm::Literal(literal) => crate::rule_ir::literal_value(literal),
        RdfTerm::Triple(triple) => TermValue::Triple {
            s: value(&triple.subject).into(),
            p: TermValue::iri(&triple.predicate).into(),
            o: value(&triple.object).into(),
        },
    }
}

/// Own one native term from any admitted view, retaining its exact blank scope,
/// literal attributes and nested RDF 1.2 structure without an RDF text round trip.
pub(crate) fn native<V: DatasetView<ReadError = std::convert::Infallible> + ?Sized>(
    view: &V,
    id: V::Id,
) -> TermValue {
    match crate::seam::resident(view.resolve(id)).term() {
        TermRef::Iri(value) => TermValue::iri(value),
        TermRef::Blank { label, scope } => TermValue::Blank {
            label: label.to_owned(),
            scope,
        },
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => TermValue::Literal {
            lexical_form: lexical.to_owned(),
            datatype: iri(view, datatype),
            language: language.map(str::to_owned),
            direction,
        },
        TermRef::Triple { s, p, o } => TermValue::Triple {
            s: native(view, s).into(),
            p: native(view, p).into(),
            o: native(view, o).into(),
        },
    }
}

fn iri<V: DatasetView<ReadError = std::convert::Infallible> + ?Sized>(
    view: &V,
    id: V::Id,
) -> String {
    match crate::seam::resident(view.resolve(id)).term() {
        TermRef::Iri(value) => value.to_owned(),
        other => unreachable!("admitted RDF predicate/datatype must be an IRI, got {other:?}"),
    }
}

fn term<V: DatasetView<ReadError = std::convert::Infallible>>(view: &V, id: V::Id) -> RdfTerm {
    match crate::seam::resident(view.resolve(id)).term() {
        TermRef::Iri(value) => RdfTerm::iri(value),
        TermRef::Blank { label, scope } => RdfTerm::blank_node(scope.qualify_label(label)),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => RdfTerm::literal(RdfLiteral {
            lexical_form: lexical.to_owned(),
            datatype: Some(iri(view, datatype)),
            language: language.map(str::to_owned),
            direction,
        }),
        TermRef::Triple { s, p, o } => {
            RdfTerm::triple(RdfTriple::new(term(view, s), iri(view, p), term(view, o)))
        }
    }
}

/// Visit every admitted RDF statement without materializing the input view.
pub(super) fn owned_quads<V: DatasetView<ReadError = std::convert::Infallible>>(
    view: &V,
) -> impl Iterator<Item = RdfQuad> + '_ {
    view.quads()
        .chain(view.reifier_quads())
        .chain(view.annotation_quads())
        .map(|quad| {
            let mut row = RdfQuad::new(term(view, quad.s), iri(view, quad.p), term(view, quad.o));
            row.graph_name = quad.g.map(|graph| term(view, graph));
            row
        })
}
