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

const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";

/// The canonical `logic:` typing / header / class-declaration LOCAL names, each of
/// which shares its local name with the `owl:` spelling (a pure namespace swap). These
/// are the bare declaration markers — NOT the structural predicates (`subClassOf`,
/// `disjointWith`, …, handled by `owl_for_pred`) and NOT the property characteristics
/// (`transitiveProperty`, …, handled by `owl_for_char`).
pub(crate) const TYPING_LOCALS: [&str; 8] = [
    "Class",
    "ObjectProperty",
    "DatatypeProperty",
    "AnnotationProperty",
    "NamedIndividual",
    "Ontology",
    "Thing",
    "Nothing",
];

/// The `owl:` spelling of a canonical `logic:` typing / header marker, or `None` when
/// `iri` is not one — the single lookup behind all three readers. Mirrors the
/// reasoner's `calculus_projection`.
pub(crate) fn owl_typing_projection(iri: &str) -> Option<String> {
    let local = iri.strip_prefix(LOGIC_NS)?;
    TYPING_LOCALS
        .contains(&local)
        .then(|| format!("{OWL_NS}{local}"))
}

/// Whether `iri` is a canonical `logic:` bare typing / header marker (`logic:Class`,
/// `logic:ObjectProperty`, `logic:Ontology`, …). The OWL projections use this to drop
/// the marker in lockstep with its (already-omitted) `owl:` spelling.
pub(crate) fn is_logic_typing_marker(iri: &str) -> bool {
    owl_typing_projection(iri).is_some()
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
    fn projects_canonical_typing_markers_to_owl() {
        assert_eq!(
            owl_typing_projection("https://blackcatinformatics.ca/logic/Class").as_deref(),
            Some("http://www.w3.org/2002/07/owl#Class")
        );
        assert_eq!(
            owl_typing_projection("https://blackcatinformatics.ca/logic/Ontology").as_deref(),
            Some("http://www.w3.org/2002/07/owl#Ontology")
        );
    }

    #[test]
    fn passes_through_non_typing_and_owl_iris() {
        // A structural predicate is NOT a typing marker (owned by owl_for_pred).
        assert!(owl_typing_projection("https://blackcatinformatics.ca/logic/subClassOf").is_none());
        // A characteristic is NOT a typing marker (owned by owl_for_char).
        assert!(
            owl_typing_projection("https://blackcatinformatics.ca/logic/transitiveProperty")
                .is_none()
        );
        // A domain type in the logic: namespace (e.g. the holon surface) is not a marker.
        assert!(owl_typing_projection("https://blackcatinformatics.ca/logic/Holon").is_none());
        // An already-`owl:` IRI is not a canonical marker (the reverse direction is never taken).
        assert!(owl_typing_projection("http://www.w3.org/2002/07/owl#Class").is_none());
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
