// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Namespace constants, the packet turtle prefix set, and CURIE / local-name
//! helpers. The prefix set is replicated here (rather than depending on
//! `crates/pipeline`) so the crate stays a leaf library; it is the minimal set the
//! packet graph uses, and every binding the canonical serializer does not need is
//! dropped from the rendered header by `canonical_turtle` itself.

/// The `gmeow:` namespace.
pub const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The `logic:` namespace.
pub const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
/// The `rdf:` namespace.
pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
/// The `rdfs:` namespace.
pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
/// The `owl:` namespace.
pub const OWL: &str = "http://www.w3.org/2002/07/owl#";
/// The `skos:` namespace.
pub const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
/// The `xsd:` namespace.
pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
/// The `dcterms:` namespace.
pub const DCTERMS: &str = "http://purl.org/dc/terms/";

/// `rdf:type`.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:label`.
pub const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// `rdfs:isDefinedBy`.
pub const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
/// `skos:definition`.
pub const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
/// `xsd:string` — the literal default datatype, elided in turtle.
pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// `rdf:langString` — the datatype of a language-tagged literal.
pub const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
/// `gmeow:definitionDigest` — the per-term content digest emitted by term-manifest.
pub const GMEOW_DEFINITION_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/definitionDigest";

/// The prefix bindings the packet graph is rendered with. `canonical_turtle` keeps
/// only the ones actually used, so this is a superset the serializer prunes.
#[must_use]
pub fn prefixes() -> Vec<(String, String)> {
    [
        ("gmeow", GMEOW),
        ("logic", LOGIC),
        ("rdf", RDF),
        ("rdfs", RDFS),
        ("owl", OWL),
        ("skos", SKOS),
        ("xsd", XSD),
        ("dcterms", DCTERMS),
    ]
    .into_iter()
    .map(|(p, ns)| (p.to_string(), ns.to_string()))
    .collect()
}

/// The local name of an IRI (the part after the last `#` or `/`).
#[must_use]
pub fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}

/// True if `iri` is one of GMEOW's own (internal) namespaces — the test the
/// external-grounding filter applies to a mapping's object.
#[must_use]
pub fn is_internal(iri: &str) -> bool {
    iri.starts_with(GMEOW) || iri.starts_with(LOGIC)
}

/// Shorten a full predicate IRI to its CURIE form for the known GMEOW prefixes,
/// leaving an unknown namespace as the full IRI. Used for `gmeow:groundingPredicate`.
#[must_use]
pub fn curie(iri: &str) -> String {
    for (prefix, ns) in [
        ("rdfs", RDFS),
        ("skos", SKOS),
        ("gmeow", GMEOW),
        ("logic", LOGIC),
        ("dcterms", DCTERMS),
        ("owl", OWL),
        ("rdf", RDF),
    ] {
        if let Some(local) = iri.strip_prefix(ns) {
            return format!("{prefix}:{local}");
        }
    }
    iri.to_string()
}

/// Sanitize a term local name into a stable, filesystem/IRI-safe cell-path segment.
#[must_use]
pub fn safe_segment(local: &str) -> String {
    local
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}
