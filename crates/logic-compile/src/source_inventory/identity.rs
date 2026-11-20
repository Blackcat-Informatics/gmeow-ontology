// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Existing portable source identities shared without changing their digest domains.

use crate::frontend::{CompiledTheory, SourceAssertion, SourceNode};
use crate::ir::AtomicTerm;
use purrdf::{DatasetView, GraphMatch, QuadIds, RdfDataset, TermId, TermRef};

pub(crate) use purrdf::xsd::datatype::XSD_STRING;

pub(crate) fn assertion_from_quad(
    dataset: &RdfDataset,
    quad: QuadIds,
) -> gmeow_errors::Result<SourceAssertion> {
    let subject = atomic_term(dataset, quad.s, false)?;
    let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Frontend::new(
            "RDF predicate is not an IRI".into(),
        )));
    };
    if predicate == crate::ir::CONTEXTUAL_APPLICATION_IRI {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Frontend::new("a contextual application requires a formula carrying context, relation and original arguments, not a binary RDF assertion".into())));
    }
    let object = atomic_term(dataset, quad.o, true)?;
    Ok(SourceAssertion {
        subject,
        predicate: predicate.to_owned(),
        object,
    })
}

pub(crate) fn atomic_term(
    dataset: &RdfDataset,
    id: TermId,
    literal_allowed: bool,
) -> gmeow_errors::Result<AtomicTerm> {
    match dataset.resolve(id) {
        TermRef::Iri(iri) => Ok(AtomicTerm::Iri(iri.to_owned())),
        TermRef::Blank { label, scope } => Ok(AtomicTerm::Blank(
            scope.qualify_label(label).trim_start_matches("_:").to_owned(),
        )),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } if literal_allowed => {
            let TermRef::Iri(datatype) = dataset.resolve(datatype) else {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Frontend::new("literal datatype is not an IRI".into())));
            };
            let language = language.map(str::to_owned);
            Ok(AtomicTerm::Literal(purrdf::RdfLiteral {
                lexical_form: lexical.to_owned(),
                datatype: if language.is_some() || datatype == XSD_STRING {
                    None
                } else {
                    Some(datatype.to_owned())
                },
                language,
                direction,
            }))
        }
        TermRef::Literal { .. } => Err(gmeow_errors::Diag::of_kind(crate::error::Frontend::new("literal cannot occur in assertion subject position".into()))),
        TermRef::Triple { .. } => Err(gmeow_errors::Diag::of_kind(crate::error::Frontend::new("quoted triple term requires an explicit proposition-term translation and remains unasserted"
                .into()))),
    }
}

pub(crate) fn source_digest(theory: &CompiledTheory) -> String {
    let dataset = theory.source().dataset();
    let mut hasher = purrdf_hash::blake3::Hasher::new();
    hasher.update(b"gmeow-prepared-source-v3\0");
    let mut graphs: Vec<_> = dataset
        .named_graphs()
        .map(|graph| term_anchor(dataset, dataset.resolve(graph)))
        .collect();
    graphs.sort_unstable();
    for graph in graphs {
        hasher.update(b"G|");
        hasher.update(&(graph.len() as u64).to_be_bytes());
        hasher.update(graph.as_bytes());
    }
    for quad in dataset.quads() {
        hasher.update(quad_anchor(dataset, quad).as_bytes());
        hasher.update(b"\n");
    }
    for (reifier, triple, graph) in dataset.reifiers_with_graph() {
        hasher.update(b"R|");
        hasher.update(term_anchor(dataset, dataset.resolve(reifier)).as_bytes());
        hasher.update(b"|");
        hasher.update(term_anchor(dataset, dataset.resolve(triple)).as_bytes());
        if let Some(graph) = graph {
            hasher.update(b"|");
            hasher.update(term_anchor(dataset, dataset.resolve(graph)).as_bytes());
        }
        hasher.update(b"\n");
    }
    for (reifier, predicate, object, graph) in dataset.annotations_with_graph() {
        hasher.update(b"A|");
        for term in [reifier, predicate, object] {
            hasher.update(term_anchor(dataset, dataset.resolve(term)).as_bytes());
            hasher.update(b"|");
        }
        if let Some(graph) = graph {
            hasher.update(term_anchor(dataset, dataset.resolve(graph)).as_bytes());
        }
        hasher.update(b"\n");
    }
    let mut receipts: Vec<_> = theory
        .source()
        .origins()
        .iter()
        .map(|origin| purrdf_lex::json::record::to_vec(&origin.document))
        .collect();
    receipts.sort_unstable();
    for receipt in receipts {
        hasher.update(&(receipt.len() as u64).to_be_bytes());
        hasher.update(&receipt);
    }
    purrdf_hash::hex::encode(hasher.finalize().as_bytes())
}

pub(crate) fn quad_anchor(dataset: &RdfDataset, quad: QuadIds) -> String {
    frame(
        "quad",
        [
            term_anchor(dataset, dataset.resolve(quad.s)),
            term_anchor(dataset, dataset.resolve(quad.p)),
            term_anchor(dataset, dataset.resolve(quad.o)),
            quad.g
                .map(|graph| term_anchor(dataset, dataset.resolve(graph)))
                .unwrap_or_default(),
        ],
    )
}

pub(crate) fn node_anchor(dataset: &RdfDataset, node: SourceNode) -> String {
    frame(
        "node",
        [
            term_anchor(dataset, dataset.resolve(node.term)),
            node.graph
                .map(|graph| term_anchor(dataset, dataset.resolve(graph)))
                .unwrap_or_default(),
        ],
    )
}

pub(crate) fn term_anchor(dataset: &RdfDataset, term: TermRef<'_>) -> String {
    match term {
        TermRef::Iri(iri) => frame("I", [iri.to_owned()]),
        TermRef::Blank { label, scope } => frame("B", [scope.qualify_label(label).into_owned()]),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => frame(
            "L",
            [
                lexical.to_owned(),
                match dataset.resolve(datatype) {
                    TermRef::Iri(iri) => iri.to_owned(),
                    other => format!("{other:?}"),
                },
                language.unwrap_or_default().to_owned(),
                direction
                    .map(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned(),
            ],
        ),
        TermRef::Triple { s, p, o } => frame(
            "T",
            [
                term_anchor(dataset, dataset.resolve(s)),
                term_anchor(dataset, dataset.resolve(p)),
                term_anchor(dataset, dataset.resolve(o)),
            ],
        ),
    }
}

pub(crate) fn frame(tag: &str, fields: impl IntoIterator<Item = String>) -> String {
    let mut out = String::from(tag);
    for field in fields {
        out.push('|');
        out.push_str(&field.len().to_string());
        out.push(':');
        out.push_str(&field);
    }
    out
}

pub(crate) fn digest_bytes(domain: &[u8], value: &[u8]) -> String {
    let mut hasher = purrdf_hash::blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(value);
    purrdf_hash::hex::encode(hasher.finalize().as_bytes())
}

pub(crate) fn class_field(local: &str) -> bool {
    local == "onProperty"
        || crate::restriction::CONSTRAINT_LOCALS.contains(&local)
        || matches!(
            local,
            "hasSelf"
                | "oneOf"
                | "unionOf"
                | "intersectionOf"
                | "complementOf"
                | "disjointUnionOf"
                | "onDatatype"
                | "withRestrictions"
        )
}

pub(crate) fn single_iri_object(
    dataset: &RdfDataset,
    node: SourceNode,
    predicate: &str,
) -> gmeow_errors::Result<String> {
    let invalid = || {
        gmeow_errors::Diag::of_kind(crate::error::Frontend::new(format!(
            "characteristic record requires exactly one IRI value for {predicate}"
        )))
    };
    let Some(predicate) = dataset.term_id_by_iri(predicate) else {
        return Err(invalid());
    };
    let graph = node.graph.map_or(GraphMatch::Default, GraphMatch::Named);
    let mut values = dataset.quads_for_pattern(Some(node.term), Some(predicate), None, graph);
    let Some(first) = values.next() else {
        return Err(invalid());
    };
    if values.next().is_some() {
        return Err(invalid());
    }
    match dataset.resolve(first.o) {
        TermRef::Iri(iri) => Ok(iri.to_owned()),
        _ => Err(invalid()),
    }
}
