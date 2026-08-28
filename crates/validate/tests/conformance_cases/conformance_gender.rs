// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_gender.py
//!
//! Covers the gender building-block value vocabulary (a clock-independent
//! competency SELECT with an `OPTIONAL rdfs:label` cell) and the cross-slice
//! `gmeow:displayable` TBox guard (a non-SPARQL membership check that
//! `displayable` is a domain-free `owl:DatatypeProperty`, so it covers both
//! `gmeow:Appellation` and `gmeow:IdentityFacet`).
//!
//! The structural TBox MUST/MUST-NOT invariants those originals shared with
//! module-scoped slicetest cells stayed in `slices/core/gender/tests/structural.ttl`;
//! only the two dynamic tests the dossier retains are ported here.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_competency_gender_values_query`: the `gender-values.rq`
/// competency SELECT (with an `OPTIONAL rdfs:label` cell, `ORDER BY ?gender`)
/// over the merged ontology enumerates the recognised `gmeow:Gender` value
/// individuals. The Python "specific individuals present + `len(values) >= 11`"
/// becomes a `column_superset` over `?gender` plus `select_count_at_least(11)`.
#[gmeow_test_batch_macros::batch_test]
fn competency_gender_values_query() {
    QueryCase::new("gender/gender-values", &[Feature::Optional])
        .over_ontology()
        .query_file("gender-values.rq")
        .column_superset(
            "gender",
            vec![
                iri(&gm("genderWoman")),
                iri(&gm("genderNonBinary")),
                iri(&gm("genderAgender")),
                iri(&gm("genderTwoSpirit")),
            ],
        )
        .select_count_at_least(11)
        .run();
}

/// Twin of `test_displayable_generalised_to_cover_identity`: NON-SPARQL TBox
/// guard. `gmeow:displayable` is an `owl:DatatypeProperty` (the single display
/// control) and is deliberately domain-free — it must NOT be pinned to
/// `gmeow:Appellation` via `rdfs:domain`, so it covers `gmeow:IdentityFacet`
/// (gender/orientation) too. Expressed with the IRI-only `has` membership helper,
/// mirroring the rdflib `(s, p, o) in graph` / `not in graph` pair exactly.
#[gmeow_test_batch_macros::batch_test]
fn displayable_generalised_to_cover_identity() {
    let g = GraphStore::ontology();
    let displayable = gm("displayable");
    assert!(
        g.has(
            Some(&displayable),
            Some(RDF_TYPE),
            Some(OWL_DATATYPE_PROPERTY)
        ),
        "gmeow:displayable must be an owl:DatatypeProperty"
    );
    assert!(
        !g.has(
            Some(&displayable),
            Some(RDFS_DOMAIN),
            Some(&gm("Appellation"))
        ),
        "gmeow:displayable must be domain-free (no rdfs:domain gmeow:Appellation)"
    );
}
