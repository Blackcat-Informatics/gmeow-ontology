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
//! [`purrdf::RdfDatasetBuilder`] for a native RDF-IR consumer (e.g.
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
/// carrier language tag via [`purrdf::RdfLiteral::language_tagged`] — the
/// `cross_substrate_parity` test below locks this adapter's output to the same
/// logical quad set [`annotate_nquads`] produces for identical inputs, so a
/// native RDF-IR consumer (e.g. `logic-compile`) gets the identical contract.
pub fn annotate_builder(
    builder: &mut purrdf::RdfDatasetBuilder,
    subject_iri: &str,
    label: &str,
    definition: &str,
    graph_iri: &str,
) {
    let graph_term = purrdf::RdfTerm::iri(graph_iri.to_owned());
    for (predicate, object) in abox_annotations(subject_iri, label, definition, graph_iri) {
        let object_term = match object {
            AboxObject::Iri(iri) => purrdf::RdfTerm::iri(iri),
            AboxObject::CarrierLiteral(value) => purrdf::RdfTerm::literal(
                purrdf::RdfLiteral::language_tagged(value, X_GMEOW_ENGLISH),
            ),
        };
        let quad = purrdf::RdfQuad::new(
            purrdf::RdfTerm::iri(subject_iri.to_owned()),
            predicate,
            object_term,
        )
        .in_graph(graph_term.clone());
        builder.push_owned_quad(&quad);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/example/subject";
    const LABEL: &str = "Example subject";
    const DEFINITION: &str = "A subject minted for a unit test.";
    const GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/example";

    /// Exactly four triples, in the fixed (label, definition, isDefinedBy,
    /// graphBoxRole) order — the order both adapters commit to.
    #[test]
    fn annotate_nquads_emits_four_triples_in_fixed_order() {
        let mut out = Vec::new();
        annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut out);
        assert_eq!(out.len(), 4, "must emit exactly four annotation triples");
        assert!(
            out[0].contains(RDFS_LABEL),
            "line 0 must be rdfs:label: {out:?}"
        );
        assert!(
            out[1].contains(SKOS_DEFINITION),
            "line 1 must be skos:definition: {out:?}"
        );
        assert!(
            out[2].contains(RDFS_IS_DEFINED_BY),
            "line 2 must be rdfs:isDefinedBy: {out:?}"
        );
        assert!(
            out[3].contains(GRAPH_BOX_ROLE),
            "line 3 must be gmeow:graphBoxRole: {out:?}"
        );
    }

    /// `isDefinedBy`'s object is exactly the passed graph IRI, and
    /// `graphBoxRole`'s object is exactly `gmeow:boxABox`.
    #[test]
    fn annotate_nquads_isdefinedby_and_role_objects_are_correct() {
        let mut out = Vec::new();
        annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut out);
        assert!(
            out[2].contains(&format!("<{GRAPH}>")),
            "isDefinedBy must point at the containing graph IRI: {}",
            out[2]
        );
        assert!(
            out[3].contains(&format!("<{BOX_ABOX}>")),
            "graphBoxRole must point at gmeow:boxABox: {}",
            out[3]
        );
    }

    /// Both label and definition literals carry the `x-gmeow-english` carrier
    /// language tag, never bare `en`.
    #[test]
    fn annotate_nquads_literals_carry_the_carrier_language_tag() {
        let mut out = Vec::new();
        annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut out);
        for line in &out[0..2] {
            assert!(
                line.ends_with(&format!("\"@{X_GMEOW_ENGLISH} <{GRAPH}> .")),
                "literal line must carry the x-gmeow-english carrier tag: {line}"
            );
            assert!(
                !line.contains("\"@en "),
                "literal line must never carry a bare @en tag: {line}"
            );
        }
    }

    /// Determinism: identical inputs produce byte-identical output, every call.
    #[test]
    fn annotate_nquads_is_deterministic() {
        let mut a = Vec::new();
        let mut b = Vec::new();
        annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut a);
        annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut b);
        assert_eq!(a, b);
    }

    /// A subject or literal carrying characters `nq_escape` must escape (a quote,
    /// a backslash) round-trips through the string adapter without corrupting the
    /// N-Quads line shape (still four triples, still ending in the graph token).
    #[test]
    fn annotate_nquads_escapes_literal_content() {
        let mut out = Vec::new();
        annotate_nquads(SUBJECT, "has \"quotes\"", "has\\backslash", GRAPH, &mut out);
        assert!(out[0].contains("has \\\"quotes\\\""), "{}", out[0]);
        assert!(out[1].contains("has\\\\backslash"), "{}", out[1]);
    }

    /// Cross-substrate parity (the key LSP test): the quad SET the string
    /// adapter emits equals the quad set the builder adapter emits, for the
    /// same inputs — parsed back into logical `(s, p, o, g)` tuples so the
    /// comparison is substrate-independent (a builder-frozen dataset needn't
    /// preserve N-Quads' exact literal spelling, only the same RDF term
    /// identity).
    #[test]
    fn cross_substrate_parity_string_and_builder_emit_the_same_quad_set() {
        use std::collections::BTreeSet;

        let mut lines = Vec::new();
        annotate_nquads(SUBJECT, LABEL, DEFINITION, GRAPH, &mut lines);

        let mut builder = purrdf::RdfDatasetBuilder::new();
        annotate_builder(&mut builder, SUBJECT, LABEL, DEFINITION, GRAPH);
        let dataset = builder.freeze().expect("valid dataset");

        // Render the builder's quads through the SAME term Display the
        // production N-Triples/N-Quads codec uses, so both sides compare in
        // the identical textual term grammar (`<iri>` / `"lex"@tag`).
        let mut from_builder: BTreeSet<String> = BTreeSet::new();
        for quad in dataset.owned_quads() {
            let g = quad
                .graph_name
                .as_ref()
                .map(|g| g.to_string())
                .unwrap_or_default();
            from_builder.insert(format!(
                "{} <{}> {} {}",
                quad.subject, quad.predicate, quad.object, g
            ));
        }

        // Parse the string adapter's N-Quads lines into the same
        // "s p o g" shape (predicate is already bare-angle-bracketed like the
        // subject/graph, so strip the trailing " ." and re-join on a single
        // space, dropping the doubled predicate brackets vs. Display's IRI
        // rendering — both sides normalize to `<iri>`/`"lex"@tag` tokens).
        let mut from_nquads: BTreeSet<String> = BTreeSet::new();
        for line in &lines {
            let body = line
                .strip_suffix(" .")
                .expect("every emitted line ends with ' .'");
            from_nquads.insert(body.to_owned());
        }

        assert_eq!(
            from_builder, from_nquads,
            "string and builder adapters must emit the identical logical quad set"
        );
    }
}
