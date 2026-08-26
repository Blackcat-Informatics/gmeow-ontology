// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single canonical-first lowering table for the `logic:` **typing / header /
//! class-declaration** vocabulary — the compiler-side twin of the reasoner's
//! `calculus_term` ([`gmeow_logic::reason`]).
//!
//! GMEOW authors its ontology in the canonical `logic:` vocabulary (Principle 17:
//! `owl:`/`rdfs:` are lossy *views*). A bare typing marker (`a logic:Class`,
//! `a logic:ObjectProperty`, …) and a header marker (`a logic:Ontology`) therefore
//! carry the SAME meaning their legacy `owl:` spelling did. Three readers must agree
//! on that mapping and none may hand-roll its own `logic:`/`owl:` pair:
//!
//! 1. the OWL-DL / OWL-EL projections ([`crate::projections::rdf`]) — a bare typing /
//!    header marker is projected exactly as its `owl:` spelling is: **omitted** from
//!    the grounding view (a bare `owl:Class` / `owl:ObjectProperty` / `owl:Ontology`
//!    declaration never reached the OWL projection in the first place — the frontend
//!    only lifts the *structural* edges and the gUFO sort, and each class already
//!    earns its `owl:Class` from that sort), so the canonical `logic:` marker must be
//!    dropped in lockstep rather than leak through as a `logic:`-namespaced type;
//! 2. the validation-shape derivation ([`crate::frontend::derive_validation_shapes`])
//!    — a shape target class / property is found by its typing marker, so the walk
//!    must recognise the `logic:` spelling exactly as it recognises `owl:`;
//! 3. the property-declaration orphan check ([`crate::frontend`]'s
//!    `PROPERTY_DECLARATION_TYPES`) — a carrier target "exists as a declared property"
//!    under either spelling.
//!
//! The canonical IR carrier (and the exact `canonical-rdf12` / `gts` projections) keep
//! the `logic:` spelling untouched; ONLY the lossy `owl:`/SHACL views consult this
//! table. The direction is always canonical → view, never the reverse.

use gmeow_ns::{LOGIC_NS, OWL_NS};

/// The canonical `logic:` typing / header / class-declaration LOCAL names, each of
/// which shares its local name with the `owl:` spelling (a pure namespace swap). These
/// are the bare declaration markers — NOT the structural predicates (`subClassOf`,
/// `disjointWith`, …, handled by `owl_for_pred`) and NOT the property characteristics
/// (`transitiveProperty`, …, handled by `owl_for_char`).
const TYPING_LOCALS: [&str; 8] = [
    "Class",
    "ObjectProperty",
    "DatatypeProperty",
    "AnnotationProperty",
    "NamedIndividual",
    "Ontology",
    "Thing",
    "Nothing",
];

/// Whether `iri` is a canonical `logic:` bare typing / header marker (`logic:Class`,
/// `logic:ObjectProperty`, `logic:Ontology`, …). The OWL projections use this to drop
/// the marker in lockstep with its (already-omitted) `owl:` spelling, and it backs the
/// shape-target walk and the property-declaration orphan check. It is a pure membership
/// test: it never allocates the projected `owl:` IRI — the projection itself, when a
/// reader needs it, comes from [`to_owl_view`] / [`gmeow_ns`]'s `OWL_*` constants.
pub(crate) fn is_logic_typing_marker(iri: &str) -> bool {
    iri.strip_prefix(LOGIC_NS)
        .is_some_and(|local| TYPING_LOCALS.contains(&local))
}

/// Normalize one authored `rdf:type` object onto the `owl:` view spelling the correspondence
/// lowerings (EDOAL entity-kind, correspondence-soundness direction check) classify against: a
/// canonical `logic:` typing marker or property-characteristic type becomes its `owl:` view; every
/// other IRI (an `owl:`/`rdfs:` term already, a domain class, a gUFO sort) passes through unchanged.
/// The reader consults the AUTHORED type, which is `logic:` after the surface flip, so without this
/// a term's EDOAL entity kind reads as indeterminate and the mapping lowering hard-fails.
///
/// The one lowering map lives in [`gmeow_ns::to_owl_view`] so every crate (pipeline mapping
/// transform, slice peerage, …) shares it; this is the `logic-compile` `String`-returning shim.
pub(crate) fn to_owl_view(iri: &str) -> String {
    gmeow_ns::to_owl_view(iri).to_owned()
}

/// Both spellings of a typing marker named by its shared LOCAL name: the canonical
/// `logic:` IRI first, then the legacy `owl:` IRI. The frontend readers scan for BOTH
/// so an `owl:`-authored and a `logic:`-authored corpus derive identical shapes /
/// declarations during (and after) the surface flip.
pub(crate) fn both_spellings(local: &str) -> [String; 2] {
    debug_assert!(
        TYPING_LOCALS.contains(&local),
        "both_spellings called with a non-typing local: {local}"
    );
    [format!("{LOGIC_NS}{local}"), format!("{OWL_NS}{local}")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_every_canonical_typing_marker() {
        // The membership test recognises every bare `logic:` typing / header marker,
        // and (per thread r3818278190) does so without allocating the `owl:` view.
        for local in TYPING_LOCALS {
            let iri = format!("{LOGIC_NS}{local}");
            assert!(is_logic_typing_marker(&iri), "marker not recognised: {iri}");
        }
    }

    #[test]
    fn rejects_non_typing_and_owl_iris() {
        // A structural predicate is NOT a typing marker (owned by owl_for_pred).
        assert!(!is_logic_typing_marker(
            "https://blackcatinformatics.ca/logic/subClassOf"
        ));
        // A characteristic is NOT a typing marker (owned by owl_for_char).
        assert!(!is_logic_typing_marker(
            "https://blackcatinformatics.ca/logic/transitiveProperty"
        ));
        // A domain type in the logic: namespace (e.g. the holon surface) is not a marker.
        assert!(!is_logic_typing_marker(
            "https://blackcatinformatics.ca/logic/Holon"
        ));
        // An already-`owl:` IRI is not a canonical marker (the reverse direction is never taken).
        assert!(!is_logic_typing_marker(
            "http://www.w3.org/2002/07/owl#Class"
        ));
    }

    #[test]
    fn to_owl_view_projects_a_typing_marker_to_its_owl_spelling() {
        // The projection itself (when a reader needs the `owl:` IRI) comes from the
        // shared `gmeow_ns` lowering, not a locally re-spelled literal.
        assert_eq!(
            to_owl_view("https://blackcatinformatics.ca/logic/Class"),
            "http://www.w3.org/2002/07/owl#Class"
        );
    }

    #[test]
    fn both_spellings_lists_logic_first() {
        assert_eq!(
            both_spellings("ObjectProperty"),
            [
                "https://blackcatinformatics.ca/logic/ObjectProperty".to_owned(),
                "http://www.w3.org/2002/07/owl#ObjectProperty".to_owned(),
            ]
        );
    }
}
