// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared RDF-reading helpers over a native `purrdf::RdfDataset`.
//!
//! The rubric loader and every axis producer read the same way: resolve an IRI to
//! a term id, then walk `quads_for_pattern`. These helpers centralise that idiom.

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};

use crate::model::GMEOW;

const PUBLIC_ENGLISH: &str = "en";

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

/// The deterministic consumer-facing literal for `(subject, predicate)`.
///
/// Internal carrier English is authoritative, followed by public RDF English,
/// an untagged literal, and finally the remaining languages in stable language /
/// lexical order. The dataset is only read: every multilingual literal remains
/// available to consumers that need the complete annotation coat.
#[must_use]
pub fn display_lit(ds: &RdfDataset, subject: TermId, pred: TermId) -> Option<String> {
    fn language_rank(language: Option<&str>) -> u8 {
        match language {
            Some(tag) if tag.eq_ignore_ascii_case(gmeow_errors::abox::X_GMEOW_ENGLISH) => 0,
            Some(tag) if tag.eq_ignore_ascii_case(PUBLIC_ENGLISH) => 1,
            None => 2,
            Some(_) => 3,
        }
    }

    let mut literals: Vec<(Option<String>, String)> = ds
        .quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Literal {
                lexical, language, ..
            } => Some((language.map(str::to_owned), lexical.to_owned())),
            _ => None,
        })
        .collect();
    literals.sort_by(|a, b| {
        language_rank(a.0.as_deref())
            .cmp(&language_rank(b.0.as_deref()))
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    literals.into_iter().next().map(|(_, lexical)| lexical)
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
        .and_then(|p| display_lit(ds, subject, p))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUBJECT: &str = "https://example.test/tier";

    fn dataset(labels: &str) -> std::sync::Arc<RdfDataset> {
        let turtle = format!(
            "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <{SUBJECT}> {labels} .\n"
        );
        crate::dataset_from_documents(&[("labels.ttl", turtle.as_bytes())])
            .expect("label fixture parses")
    }

    #[test]
    fn carrier_english_wins_independently_of_quad_order() {
        let forward = dataset("rdfs:label \"Lie\"@fr, \"Public\"@en, \"Carrier\"@x-gmeow-english");
        let reverse = dataset("rdfs:label \"Carrier\"@x-gmeow-english, \"Public\"@en, \"Lie\"@fr");
        for ds in [&forward, &reverse] {
            let subject = id(ds, SUBJECT).expect("subject interned");
            assert_eq!(label_of(ds, subject), "Carrier");
            let label = id(ds, RDFS_LABEL).expect("label predicate interned");
            assert_eq!(all_lits(ds, subject, label).len(), 3, "labels are retained");
        }
    }

    #[test]
    fn public_english_precedes_neutral_and_other_languages() {
        let ds = dataset("rdfs:label \"Zulu\"@zu, \"Neutral\", \"English\"@en");
        let subject = id(&ds, SUBJECT).expect("subject interned");
        assert_eq!(label_of(&ds, subject), "English");
    }

    #[test]
    fn non_english_fallback_is_a_stable_total_order() {
        let forward = dataset("rdfs:label \"Zulu\"@zu, \"Francais\"@fr");
        let reverse = dataset("rdfs:label \"Francais\"@fr, \"Zulu\"@zu");
        for ds in [&forward, &reverse] {
            let subject = id(ds, SUBJECT).expect("subject interned");
            assert_eq!(label_of(ds, subject), "Francais");
        }
    }
}
