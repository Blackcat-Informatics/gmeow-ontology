// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_mereology.py
//!
//! The universal-mereology no-winner / no-cardinality guard: no gmeow: term is a
//! primary/preferred part-or-whole selector, and none of the part-like relations
//! is declared functional. A whole-merged-graph sweep (`GraphStore::ontology()`).

mod conformance_support;
use conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_no_winner_or_cardinality_terms_for_parts`: the forbidden
/// primary/preferred part-and-whole locals appear on no gmeow: subject, and none of
/// partOf / hasPart / subOrganizationOf / subEventOf is an owl:FunctionalProperty.
#[test]
fn no_winner_or_cardinality_terms_for_parts() {
    let g = GraphStore::ontology();
    let forbidden = [
        "primaryPart",
        "preferredPart",
        "primaryWhole",
        "preferredWhole",
    ];

    let (_vars, rows) = g.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
    for row in &rows {
        let Some(Some(term)) = row.first() else {
            continue;
        };
        let Some(iri) = term.as_iri() else {
            continue;
        };
        if let Some(local) = iri.strip_prefix(GMEOW) {
            assert!(
                !forbidden.contains(&local),
                "forbidden mereology winner/cardinality term declared: {iri}"
            );
        }
    }

    for prop in ["partOf", "hasPart", "subOrganizationOf", "subEventOf"] {
        assert!(
            !g.has(
                Some(&gm(prop)),
                Some(RDF_TYPE),
                Some(OWL_FUNCTIONAL_PROPERTY)
            ),
            "gmeow:{prop} must not be a functional property"
        );
    }
}
