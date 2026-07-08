// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared RDF-reading helpers over a native `purrdf::RdfDataset`.
//!
//! The rubric loader and every axis producer read the same way: resolve an IRI to
//! a term id, then walk `quads_for_pattern`. These helpers centralise that idiom.

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};

use crate::model::GMEOW;

/// The `rdf:type` IRI.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The `rdfs:label` IRI.
pub const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// Fully-qualify a `gmeow:` local name.
#[must_use]
pub fn g(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Resolve an IRI to a term id, if present in the dataset.
#[must_use]
pub fn id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// The single object IRI for `(subject, predicate)`, if any IRI object.
#[must_use]
pub fn one_iri(ds: &RdfDataset, subject: TermId, pred: TermId) -> Option<String> {
    ds.quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .find_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
}

/// The single literal lexical for `(subject, predicate)`, if any literal object.
#[must_use]
pub fn one_lit(ds: &RdfDataset, subject: TermId, pred: TermId) -> Option<String> {
    ds.quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .find_map(|q| match ds.resolve(q.o) {
            TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
            _ => None,
        })
}

/// Every literal lexical for `(subject, predicate)`.
#[must_use]
pub fn all_lits(ds: &RdfDataset, subject: TermId, pred: TermId) -> Vec<String> {
    ds.quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
            _ => None,
        })
        .collect()
}

/// All object IRIs for `(subject, predicate)`.
#[must_use]
pub fn all_iris(ds: &RdfDataset, subject: TermId, pred: TermId) -> Vec<String> {
    ds.quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect()
}

/// True if `(subject predicate object)` is present.
#[must_use]
pub fn has(ds: &RdfDataset, subject: TermId, pred: TermId, object: TermId) -> bool {
    ds.quads_for_pattern(Some(subject), Some(pred), Some(object), GraphMatch::Any)
        .next()
        .is_some()
}

/// True if `subject` has any object under `pred`.
#[must_use]
pub fn has_any(ds: &RdfDataset, subject: TermId, pred: TermId) -> bool {
    ds.quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .next()
        .is_some()
}

/// True if `pred` is used by any triple in the dataset.
#[must_use]
pub fn predicate_used(ds: &RdfDataset, pred: TermId) -> bool {
    ds.quads_for_pattern(None, Some(pred), None, GraphMatch::Any)
        .next()
        .is_some()
}

/// True if any triple has `pred` as predicate and `object` as object — including
/// blank-node subjects (unlike [`instances_of`], which yields only IRI subjects).
#[must_use]
pub fn has_any_object(ds: &RdfDataset, pred: TermId, object: TermId) -> bool {
    ds.quads_for_pattern(None, Some(pred), Some(object), GraphMatch::Any)
        .next()
        .is_some()
}

/// Every subject IRI typed `class_iri`, sorted and deduped.
#[must_use]
pub fn instances_of(ds: &RdfDataset, class_iri: &str) -> Vec<String> {
    let (Some(type_id), Some(class_id)) = (id(ds, RDF_TYPE), id(ds, class_iri)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = ds
        .quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.s) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The `rdfs:label` literal of a subject, or the empty string.
#[must_use]
pub fn label_of(ds: &RdfDataset, subject: TermId) -> String {
    id(ds, RDFS_LABEL)
        .and_then(|p| one_lit(ds, subject, p))
        .unwrap_or_default()
}

/// All `gmeow:`-namespaced subject IRIs that are typed as some `owl:Class` or
/// `owl:*Property` in the dataset — the slice's own authored terms. Sorted,
/// deduped. A term counts as authored-here when its IRI is in the `gmeow:`
/// namespace and it carries at least one `rdf:type`.
#[must_use]
pub fn gmeow_terms(ds: &RdfDataset) -> Vec<String> {
    let Some(type_id) = id(ds, RDF_TYPE) else {
        return Vec::new();
    };
    let mut out: Vec<String> = ds
        .quads_for_pattern(None, Some(type_id), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.s) {
            TermRef::Iri(iri) if iri.starts_with(GMEOW) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}
