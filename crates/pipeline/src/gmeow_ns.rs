// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! gmeow's ontology namespace, injected into purrdf's namespace-neutral emitters.
//!
//! purrdf is a namespace-neutral toolkit — its slice emitters, the
//! SHACL→JSON-Schema keying, and the JSON-LD-star statement-metadata downcast all
//! take a namespace/vocab from the CONSUMER rather than baking in any ontology.
//! gmeow builds one [`purrdf::OntologyProfile`] here and derives the per-emitter
//! vocab views from it, so every generated artifact carries gmeow's `gmeow:` terms.

use purrdf::{Namespaces, OntologyProfile, SliceVocab};

/// gmeow's canonical ontology namespace (trailing `/` for term concatenation).
pub(crate) const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
/// gmeow's logic-core namespace (the second authored gmeow-ecosystem namespace).
pub(crate) const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// gmeow's single ontology profile: the `gmeow:` primary namespace plus the
/// authored `logic:` prefix. purrdf's builtins (xsd/rdf/rdfs/owl/sh) are always
/// available on top of these, so the profile only carries gmeow's own vocab.
pub(crate) fn gmeow_profile() -> OntologyProfile {
    OntologyProfile::for_namespace(GMEOW_NS)
        .with_prefix("gmeow")
        .with_prefixes(vec![
            ("gmeow".to_owned(), GMEOW_NS.to_owned()),
            ("logic".to_owned(), LOGIC_NS.to_owned()),
        ])
}

/// The slice-emitter vocabulary (prefix `gmeow`) purrdf's emitters key on.
pub(crate) fn gmeow_slice_vocab() -> SliceVocab {
    gmeow_profile().slice_vocab()
}

/// The SHACL→JSON-Schema keying namespaces (gmeow primary + authored prefixes).
/// Construction cannot fail: the `gmeow` primary prefix is always declared.
pub(crate) fn gmeow_json_schema_namespaces() -> Namespaces {
    gmeow_profile()
        .namespaces()
        .expect("gmeow primary prefix is declared in gmeow_profile")
}
