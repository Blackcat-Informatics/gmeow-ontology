// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared `lang:` RDF-scan helpers for the meaning/surface/document projection targets
//! ([`crate::tei`], [`crate::nif`], [`crate::semaf`]).
//!
//! Each of those targets lowers FROM a `lang:` RDF surface (Turtle) already present in the
//! composed model — a `lang:ComposedForm`, a `lang:SurfaceAnchor`, a `lang:Denotation` —
//! exactly as [`crate::ontolex`] lowers FROM an OntoLex-Lemon lexicon. This module extracts
//! the small deterministic purrdf-query surface those targets share (parse, typed-subject
//! enumeration, object lookup, inverse lookup, IRI/literal/label resolution) so each target
//! reads the model the same way and none re-implements the scan. It carries NO transform and
//! NO correspondence policy of its own — only the read primitives and the single shared
//! lossy-lens `logic:Correspondence` constructor every projection FROM the model reuses.

use std::sync::Arc;

use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeVerdict,
    LawClaimIr, MorphismClass, MorphismKind,
};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue, parse_dataset};

use crate::bridge::{IngestDiagnostic, LangFailure};

/// The `lang:` namespace base, byte-identical to every other `lang:` producer.
pub(crate) use gmeow_ns::LANG_NS;
/// The `logic:` namespace base — a `lang:Denotation`'s `lang:denotationTarget` points here.
pub(crate) use gmeow_ns::LOGIC_NS;
/// `rdf:type`.
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:label` — the human-readable surface token a projection renders where present.
pub(crate) const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// The example-instance base every minted projection individual lives under, matching the
/// base every other `lang:` producer content-addresses its individuals under.
pub(crate) const EXAMPLE_BASE: &str = "http://example.org/lang/";

/// Hard-fail helper: a construct the target cannot account for is named exactly, never
/// silently dropped (the `lang:SilentIngestDrop` floor a bridge never crosses).
pub(crate) fn unrepresentable(construct: impl Into<String>) -> IngestDiagnostic {
    IngestDiagnostic {
        failure_class: LangFailure::SilentIngestDrop,
        construct: construct.into(),
    }
}

/// Parse a `lang:` Turtle surface into a dataset, or HARD FAIL. Non-UTF-8 bytes are a
/// [`LangFailure::NonUtf8Surface`]; a Turtle syntax error is a [`LangFailure::SilentIngestDrop`]
/// naming the source.
pub(crate) fn parse_lang_turtle(
    bytes: &[u8],
    source: &str,
) -> Result<Arc<RdfDataset>, IngestDiagnostic> {
    if let Err(e) = std::str::from_utf8(bytes) {
        return Err(IngestDiagnostic {
            failure_class: LangFailure::NonUtf8Surface,
            construct: format!(
                "non-UTF-8 lang: surface '{source}': {} byte(s), first invalid byte at index {}",
                bytes.len(),
                e.valid_up_to()
            ),
        });
    }
    parse_dataset(bytes, "text/turtle", None).map_err(|e| {
        unrepresentable(format!(
            "lang: surface '{source}' does not parse as Turtle: {e}"
        ))
    })
}

/// A short display string for a term (an IRI in angle brackets, else a blank/literal marker) —
/// used only to name an offending construct in a hard-fail diagnostic and to order results
/// deterministically.
pub(crate) fn term_label(ds: &RdfDataset, id: TermId) -> String {
    match ds.resolve(id) {
        TermRef::Iri(iri) => format!("<{iri}>"),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        TermRef::Literal { lexical, .. } => format!("\"{lexical}\""),
        TermRef::Triple { .. } => "<<triple term>>".to_owned(),
    }
}

/// Resolve an IRI to its interned [`TermId`] in `ds`, if the IRI is present at all.
pub(crate) fn iri_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// The IRI string of a term, or `None` if it is not an IRI.
pub(crate) fn iri_of(ds: &RdfDataset, id: TermId) -> Option<String> {
    match ds.resolve(id) {
        TermRef::Iri(iri) => Some(iri.to_owned()),
        _ => None,
    }
}

/// The lexical text of a literal term, or `None` if it is not a literal.
pub(crate) fn literal_of(ds: &RdfDataset, id: TermId) -> Option<String> {
    match ds.resolve(id) {
        TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
        _ => None,
    }
}

/// The local name of an IRI (the segment after the last `#` or `/`).
pub(crate) fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

/// Every subject typed `class_iri`, in a deterministic order (sorted by display string, so
/// authoring/interning order is immaterial).
pub(crate) fn subjects_of_type(ds: &RdfDataset, class_iri: &str) -> Vec<TermId> {
    let (Some(type_id), Some(class_id)) = (iri_id(ds, RDF_TYPE), iri_id(ds, class_iri)) else {
        return Vec::new();
    };
    let mut out: Vec<TermId> = ds
        .quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any)
        .map(|q| q.s)
        .collect();
    out.sort_by_cached_key(|&s| term_label(ds, s));
    out.dedup();
    out
}

/// Whether `subject` is declared `rdf:type class_iri` — the type test the meaning targets use to
/// decide, e.g., whether a `lang:denotationTarget` is a `logic:Formula` (by TYPE, not by the IRI's
/// namespace, so a properly-modelled example individual typed `logic:Formula` is recognized).
pub(crate) fn has_type(ds: &RdfDataset, subject: TermId, class_iri: &str) -> bool {
    objects(ds, subject, RDF_TYPE)
        .into_iter()
        .any(|o| iri_of(ds, o).as_deref() == Some(class_iri))
}

/// The object [`TermId`]s of every `(subject, predicate)` quad, deterministically ordered.
pub(crate) fn objects(ds: &RdfDataset, subject: TermId, predicate_iri: &str) -> Vec<TermId> {
    let Some(pid) = iri_id(ds, predicate_iri) else {
        return Vec::new();
    };
    let mut out: Vec<TermId> = ds
        .quads_for_pattern(Some(subject), Some(pid), None, GraphMatch::Any)
        .map(|q| q.o)
        .collect();
    out.sort_by_cached_key(|&o| term_label(ds, o));
    out.dedup();
    out
}

/// The subject [`TermId`]s of every `(?, predicate, object)` quad — the inverse lookup that
/// finds, e.g., the `lang:SurfaceForm` owning a `lang:SurfaceAnchor` through `lang:surfaceAnchor`.
pub(crate) fn subjects_with_object(
    ds: &RdfDataset,
    predicate_iri: &str,
    object: TermId,
) -> Vec<TermId> {
    let Some(pid) = iri_id(ds, predicate_iri) else {
        return Vec::new();
    };
    let mut out: Vec<TermId> = ds
        .quads_for_pattern(None, Some(pid), Some(object), GraphMatch::Any)
        .map(|q| q.s)
        .collect();
    out.sort_by_cached_key(|&s| term_label(ds, s));
    out.dedup();
    out
}

/// The first `rdfs:label` literal on `subject`, if any — the human-readable surface token a
/// projection renders (a `<w>` in TEI, an `::snt` in AMR).
pub(crate) fn label_of(ds: &RdfDataset, subject: TermId) -> Option<String> {
    objects(ds, subject, RDFS_LABEL)
        .into_iter()
        .find_map(|o| literal_of(ds, o))
}

/// The single object IRI of `(subject, predicate)`, or `None` where the edge is absent; the
/// FIRST in deterministic order where several are present (the caller decides whether that is
/// a hard fail).
pub(crate) fn object_iri(ds: &RdfDataset, subject: TermId, predicate_iri: &str) -> Option<String> {
    objects(ds, subject, predicate_iri)
        .into_iter()
        .find_map(|o| iri_of(ds, o))
}

/// The single object literal of `(subject, predicate)`, or `None` where absent.
pub(crate) fn object_literal(
    ds: &RdfDataset,
    subject: TermId,
    predicate_iri: &str,
) -> Option<String> {
    objects(ds, subject, predicate_iri)
        .into_iter()
        .find_map(|o| literal_of(ds, o))
}

/// The single shared LOSSY-LENS `logic:Correspondence` every projection FROM the `lang:` model
/// carries: a [`MorphismClass::LossyLens`] (the model→target `get` is non-injective — the
/// richer `lang:` strata collapse), NOT `mnemomorphic` (the whole source is not retained), whose
/// `GetPut` law is carried forward as [`DischargeVerdict::ObligationUnknown`] — the honest
/// verdict for a law the projection does not discharge. It is therefore NEVER an exact
/// correspondence, so the driver derives a lossy kind (never `Exact`) from it — exactly the
/// shape [`crate::ontolex::ontolex_correspondence`] and the lossy grammar correspondence carry.
///
/// The IRI is content-addressed under `corr_base` on `source_key`, so the same source always
/// carries the same correspondence. `put_leg` is `None` for a projection with no exact inverse
/// leg (TEI, SemAF); the NIF selector leg carries a `put` for provenance only, never discharged.
pub(crate) fn lossy_lens_correspondence(
    corr_base: &str,
    source_key: &str,
    get_leg: &str,
    put_leg: Option<&str>,
) -> Correspondence {
    let iri = format!(
        "{corr_base}{}",
        crate::emit::digest16("lang-projection-lossy-corr", source_key)
    );
    Correspondence::new(
        iri,
        CorrespondenceRelation::RelatedMatch,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        false,
        Some(Determinacy::Crisp),
        Some(get_leg.to_owned()),
        put_leg.map(str::to_owned),
        vec![LawClaimIr {
            law: CorrespondenceLaw::GetPut,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .expect("lossy-lens projection correspondence is well-formed by construction")
}
