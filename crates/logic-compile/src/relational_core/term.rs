// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Typed native term payloads shared by identity and RDF projection.

use purrdf::{BlankScope, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfTerm, TermId, TermRef};

use super::{LOGIC_NAMESPACE, RcTerm};

pub(super) fn error(detail: impl std::fmt::Display) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::RelationalCore {
        detail: format!("relational-core: {detail}"),
    })
}

pub(super) fn iri(builder: &mut RdfDatasetBuilder, value: &str) -> gmeow_errors::Result<TermId> {
    purrdf::iri::BaseIri::parse(value).map_err(error)?;
    Ok(builder.intern_iri(value))
}

fn payload(
    builder: &mut RdfDatasetBuilder,
    term: &RcTerm,
) -> gmeow_errors::Result<(&'static str, TermId)> {
    match term {
        RcTerm::Blank(label) => Ok(("blank", builder.intern_blank(label, BlankScope::DEFAULT))),
        RcTerm::Iri(value) => Ok(("rcIri", iri(builder, value)?)),
        RcTerm::Var(name) => Ok((
            "rcVariable",
            builder.intern_literal(RdfLiteral::simple(name)),
        )),
        RcTerm::Literal(value) => {
            let mut value = value.clone();
            crate::ir::literal_serde::normalize(&mut value).map_err(error)?;
            purrdf::iri::BaseIri::parse(value.datatype_iri()).map_err(error)?;
            Ok(("rcLiteral", builder.intern_literal(value)))
        }
    }
}

/// Temporary canonical identity graphs can use a compact quoted envelope. It
/// never enters the production GTS carrier, whose plain quads forbid triple terms.
pub(super) fn encode(
    builder: &mut RdfDatasetBuilder,
    term: &RcTerm,
) -> gmeow_errors::Result<TermId> {
    let (kind, value) = payload(builder, term)?;
    if kind == "blank" {
        return Ok(value);
    }
    let carrier = builder.intern_iri(&format!("{LOGIC_NAMESPACE}RelationalCoreTerm"));
    let predicate = builder.intern_iri(&format!("{LOGIC_NAMESPACE}{kind}"));
    Ok(builder.intern_triple(carrier, predicate, value))
}

/// Public projection uses typed term records compatible with GTS native tables.
/// Only this metadata record is asserted, never the represented object-level atom.
pub(super) fn project(
    builder: &mut RdfDatasetBuilder,
    term: &RcTerm,
) -> gmeow_errors::Result<TermId> {
    let (kind, value) = payload(builder, term)?;
    if kind == "blank" {
        return Ok(value);
    }
    let record = builder.intern_iri(&format!(
        "{LOGIC_NAMESPACE}relational-core/term/{}",
        super::sha256_hex(&term.key())
    ));
    let class = builder.intern_iri(&format!("{LOGIC_NAMESPACE}RelationalCoreTerm"));
    let rdf_type = builder.intern_iri(super::RDF_TYPE);
    let predicate = builder.intern_iri(&format!("{LOGIC_NAMESPACE}{kind}"));
    builder.push_quad(record, rdf_type, class, None);
    builder.push_quad(record, predicate, value, None);
    Ok(record)
}

pub(super) fn decode(dataset: &RdfDataset, value: TermId) -> gmeow_errors::Result<RcTerm> {
    if let TermRef::Blank { label, scope } = dataset.resolve(value) {
        return Ok(RcTerm::Blank(scope.qualify_label(label).into_owned()));
    }
    if !matches!(dataset.resolve(value), TermRef::Iri(_)) {
        return Err(error(
            "term requires a typed RelationalCoreTerm record or blank constant",
        ));
    }
    let typed = dataset
        .term_id_by_iri(super::RDF_TYPE)
        .zip(dataset.term_id_by_iri(&format!("{LOGIC_NAMESPACE}RelationalCoreTerm")))
        .is_some_and(|(predicate, class)| {
            crate::graphutil::default_graph_pattern(
                dataset,
                Some(value),
                Some(predicate),
                Some(class),
            )
            .next()
            .is_some()
        });
    if !typed {
        return Err(error("term record is missing type RelationalCoreTerm"));
    }
    let mut fields = ["rcIri", "rcLiteral", "rcVariable"]
        .into_iter()
        .flat_map(|kind| {
            dataset
                .term_id_by_iri(&format!("{LOGIC_NAMESPACE}{kind}"))
                .into_iter()
                .flat_map(move |predicate| {
                    crate::graphutil::default_graph_pattern(
                        dataset,
                        Some(value),
                        Some(predicate),
                        None,
                    )
                    .map(move |quad| (kind, quad.o))
                })
        });
    let (kind, object) = fields
        .next()
        .ok_or_else(|| error("term record has no value"))?;
    if fields.next().is_some() {
        return Err(error(
            "ambiguous term record: exactly one typed value is required",
        ));
    }
    match (kind, dataset.resolve(object)) {
        ("rcIri", TermRef::Iri(iri)) => Ok(RcTerm::Iri(iri.to_owned())),
        ("rcLiteral", TermRef::Literal { .. }) => {
            let RdfTerm::Literal(mut value) = dataset.to_owned_term(object) else {
                return Err(error("rcLiteral requires a native literal"));
            };
            crate::ir::literal_serde::normalize(&mut value).map_err(error)?;
            Ok(RcTerm::Literal(value))
        }
        ("rcVariable", _) => Ok(RcTerm::Var(text(dataset, object)?.to_owned())),
        _ => Err(error("invalid typed term record payload")),
    }
}

pub(super) fn text(dataset: &RdfDataset, value: TermId) -> gmeow_errors::Result<&str> {
    match dataset.resolve(value) {
        TermRef::Literal {
            lexical,
            datatype,
            language: None,
            direction: None,
        } if dataset.term_id_by_iri("http://www.w3.org/2001/XMLSchema#string")
            == Some(datatype) =>
        {
            Ok(lexical)
        }
        _ => Err(error("text field requires an untagged xsd:string literal")),
    }
}
