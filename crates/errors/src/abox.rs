// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single source of truth for the "self-describing A-Box individual"
//! annotation contract.
//!
//! The assertional-tier validation contract requires every generated A-Box
//! individual to carry four annotations: `rdfs:label`, `skos:definition`,
//! `rdfs:isDefinedBy` (pointing at the individual's containing named graph),
//! and `gmeow:graphBoxRole` (pointing at the assertional `gmeow:boxABox` role
//! individual). Before this module, two producers — `crate::render`'s
//! `to_gmeow_rdf_in_graph` and `gmeow_docs::rdf::to_gmeow_rdf` — each hand-rolled
//! their own copy of this four-triple block, and had already drifted (the
//! `render` copy was missing `skos:definition` entirely). This module is the ONE
//! place the contract is expressed; both producers route through it.
//!
//! [`abox_annotation_pairs`] is the box-role-parameterized core: it NEVER
//! hardcodes [`BOX_ABOX`], so a future T-Box/R-Box emitter can reuse the exact
//! same core with its own role IRI. [`abox_annotations`] is the A-Box
//! convenience wrapper every current caller uses. [`annotate_nquads`] and
//! [`annotate_builder`] are the two substrate-specific thin adapters over the
//! core — one emitting raw N-Quads text lines (matching the serialization style
//! already used by `render.rs`/`docs/rdf.rs`), the other pushing quads into a
//! [`purrdf_core::RdfDatasetBuilder`] for a native RDF-IR consumer (e.g.
//! `logic-compile`). The `cross_substrate_parity` test below locks the two
//! adapters to emit the identical logical quad set for the same inputs.
//!
//! Label/definition literals carry the [`X_GMEOW_ENGLISH`] private-use carrier
//! language tag, never bare `en` — see `docs/GROUNDING.md` for why the carrier
//! tag exists.

use crate::render::nq_escape;

/// The GMEOW namespace IRI prefix.
pub const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// `rdf:type`.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdfs:label`.
pub const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// `rdfs:isDefinedBy`.
pub const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
/// `skos:definition`.
pub const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
/// `gmeow:graphBoxRole` — the predicate a generated individual's box role rides on.
pub const GRAPH_BOX_ROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";
/// `gmeow:boxABox` — the assertional-tier box-role individual every generated
/// A-Box individual carries by default (via [`abox_annotations`]).
pub const BOX_ABOX: &str = "https://blackcatinformatics.ca/gmeow/boxABox";
/// `gmeow:boxTBox` — the terminological-tier box-role individual a generated
/// `owl:Ontology` header node (a T-Box document, never an assertional
/// individual) carries via [`abox_annotation_pairs`] instead of [`BOX_ABOX`] —
/// the future T-Box reuse [`abox_annotation_pairs`]'s doc comment anticipates.
pub const BOX_TBOX: &str = "https://blackcatinformatics.ca/gmeow/boxTBox";
/// The private-use carrier language tag every generated label/definition literal
/// MUST use instead of bare `en` — the one spelling every emitter's English prose
/// rides under.
pub const X_GMEOW_ENGLISH: &str = "x-gmeow-english";

/// One A-Box annotation predicate/object pair: a substrate-neutral Rust value,
/// not yet serialized to N-Quads text or interned into an RDF builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AboxObject {
    /// An IRI object.
    Iri(String),
    /// A literal object carrying the [`X_GMEOW_ENGLISH`] carrier language tag.
    CarrierLiteral(String),
}

/// The box-role-parameterized core: the four `(predicate, object)` annotation
/// pairs every generated A-Box individual carries, in the fixed emission order
/// (label, definition, isDefinedBy, graphBoxRole) both adapters below preserve.
///
/// `box_role_iri` is NEVER hardcoded here — a future T-Box/R-Box emitter reuses
/// this exact core with its own role IRI instead of [`BOX_ABOX`]; see
/// [`abox_annotations`] for the A-Box convenience wrapper every current caller
/// uses.
#[must_use]
pub fn abox_annotation_pairs(
    subject_iri: &str,
    label: &str,
    definition: &str,
    graph_iri: &str,
    box_role_iri: &str,
) -> [(&'static str, AboxObject); 4] {
    // The subject plays no role in *which* pairs are produced (every subject
    // gets the same predicate shape), but a caller passing an empty subject IRI
    // is always a bug at the call site — catch it here, once, rather than in
    // every adapter.
    debug_assert!(
        !subject_iri.trim().is_empty(),
        "abox annotation subject IRI must not be empty"
    );
    [
        (RDFS_LABEL, AboxObject::CarrierLiteral(label.to_owned())),
        (
            SKOS_DEFINITION,
            AboxObject::CarrierLiteral(definition.to_owned()),
        ),
        (RDFS_IS_DEFINED_BY, AboxObject::Iri(graph_iri.to_owned())),
        (GRAPH_BOX_ROLE, AboxObject::Iri(box_role_iri.to_owned())),
    ]
}

/// The A-Box convenience wrapper over [`abox_annotation_pairs`]: defaults the box
/// role to [`BOX_ABOX`], the role every generated assertional individual carries.
#[must_use]
pub fn abox_annotations(
    subject_iri: &str,
    label: &str,
    definition: &str,
    graph_iri: &str,
) -> [(&'static str, AboxObject); 4] {
    abox_annotation_pairs(subject_iri, label, definition, graph_iri, BOX_ABOX)
}

/// String-flavor adapter: push the four A-Box annotation N-Quads lines for
/// `subject_iri` onto `out`, in the fixed (label, definition, isDefinedBy,
/// graphBoxRole) order. Literals are [`nq_escape`]d and carry the
/// [`X_GMEOW_ENGLISH`] carrier language tag; IRIs are angle-bracketed. Matches
/// the exact N-Quads serialization style the two producers this module
/// replaces already used (`<s> <p> o <g> .`).
pub fn annotate_nquads(
    subject_iri: &str,
    label: &str,
    definition: &str,
    graph_iri: &str,
    out: &mut Vec<String>,
) {
    let subject = format!("<{subject_iri}>");
    let graph = format!("<{graph_iri}>");
    for (predicate, object) in abox_annotations(subject_iri, label, definition, graph_iri) {
        let object_text = match object {
            AboxObject::Iri(iri) => format!("<{iri}>"),
            AboxObject::CarrierLiteral(value) => {
                format!("\"{}\"@{X_GMEOW_ENGLISH}", nq_escape(&value))
            }
        };
        out.push(format!("{subject} <{predicate}> {object_text} {graph} ."));
    }
}

/// Builder-flavor adapter: push the four A-Box annotation quads for
/// `subject_iri` into `builder`'s named graph `graph_iri`, in the same fixed
/// order [`annotate_nquads`] emits. Literals carry the [`X_GMEOW_ENGLISH`]
/// carrier language tag via [`purrdf_core::RdfLiteral::language_tagged`] — the
/// `cross_substrate_parity` test below locks this adapter's output to the same
/// logical quad set [`annotate_nquads`] produces for identical inputs, so a
/// native RDF-IR consumer (e.g. `logic-compile`) gets the identical contract.
pub fn annotate_builder(
    builder: &mut purrdf_core::RdfDatasetBuilder,
    subject_iri: &str,
    label: &str,
    definition: &str,
    graph_iri: &str,
) {
    let graph_term = purrdf_core::RdfTerm::iri(graph_iri.to_owned());
    for (predicate, object) in abox_annotations(subject_iri, label, definition, graph_iri) {
        let object_term = match object {
            AboxObject::Iri(iri) => purrdf_core::RdfTerm::iri(iri),
            AboxObject::CarrierLiteral(value) => purrdf_core::RdfTerm::literal(
                purrdf_core::RdfLiteral::language_tagged(value, X_GMEOW_ENGLISH),
            ),
        };
        let quad = purrdf_core::RdfQuad::new(
            purrdf_core::RdfTerm::iri(subject_iri.to_owned()),
            predicate,
            object_term,
        )
        .in_graph(graph_term.clone());
        builder.push_owned_quad(&quad);
    }
}

/// `rdfs:` namespace IRI — for rendering the contract's fixed predicate IRIs as
/// prefixed Turtle terms.
const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
/// `skos:` namespace IRI.
const SKOS_NS: &str = "http://www.w3.org/2004/02/skos/core#";

/// Render one of the contract's fixed predicate/object IRIs as a Turtle term: a
/// `prefix:local` name for the `rdfs:` / `skos:` / `gmeow:` namespaces when the local
/// part is a legal single-segment name (non-empty, no `/`), else an angle-bracketed
/// full IRI. This reproduces exactly how the shape emitters already spell the four
/// clauses: the box-role IRI (`gmeow:boxABox`/`gmeow:boxTBox`) prefixes, while the
/// containing-graph IRI (a `gmeow:graph/…` path) has a `/` in its local part and so
/// stays angle-bracketed.
#[must_use]
fn turtle_iri(iri: &str) -> String {
    for (namespace, prefix) in [(RDFS_NS, "rdfs"), (SKOS_NS, "skos"), (GMEOW, "gmeow")] {
        if let Some(local) = iri.strip_prefix(namespace)
            && !local.is_empty()
            && !local.contains('/')
        {
            return format!("{prefix}:{local}");
        }
    }
    format!("<{iri}>")
}

/// Render one [`AboxObject`] as a Turtle object term.
#[must_use]
fn turtle_object(object: &AboxObject) -> String {
    match object {
        AboxObject::Iri(iri) => turtle_iri(iri),
        AboxObject::CarrierLiteral(value) => format!("\"{}\"@{X_GMEOW_ENGLISH}", nq_escape(value)),
    }
}

/// Turtle-flavor adapter: the four A-Box annotation clauses as prefixed-Turtle
/// predicate-object lines (`{indent}{predicate} {object} ;`), in the fixed (label,
/// definition, isDefinedBy, graphBoxRole) order, derived from [`abox_annotation_pairs`]
/// so the predicate set and box-role default live ONLY in this module — a Turtle
/// emitter splices these lines into its statement instead of hand-rolling the
/// four-triple skeleton (which is how `frame_shapes`/`result_shapes`/`shacl_af`/
/// `profiles` had each re-drifted the contract). Literals are [`nq_escape`]d and carry
/// the [`X_GMEOW_ENGLISH`] carrier tag; the box-role IRI renders as `gmeow:boxABox`
/// (or `gmeow:boxTBox` for an `owl:Ontology` header), the containing-graph IRI as an
/// angle-bracketed full IRI. Each returned line ends in ` ;`; the caller owns the
/// subject line, the type line, and the statement terminator, and MUST declare the
/// `rdfs:`, `skos:`, and `gmeow:` prefixes in its Turtle document.
#[must_use]
pub fn abox_annotation_turtle_lines(
    subject_iri: &str,
    label: &str,
    definition: &str,
    graph_iri: &str,
    box_role_iri: &str,
    indent: &str,
) -> [String; 4] {
    abox_annotation_pairs(subject_iri, label, definition, graph_iri, box_role_iri).map(
        |(predicate, object)| {
            format!(
                "{indent}{} {} ;",
                turtle_iri(predicate),
                turtle_object(&object)
            )
        },
    )
}

#[path = "abox.tests.rs"]
#[cfg(test)]
mod tests;
