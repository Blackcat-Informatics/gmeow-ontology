// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_trust.py.
//!
//! Migrated tests load the `trust-contested.ttl` coverage fixture, validate
//! it against the whole shapes corpus, and assert triple membership using the
//! parsed N-Triples store — mirroring the Python fixture-only `run_shacl(g)`
//! pattern.
//!
//! `three_axes_are_orthogonal_in_trust` and `no_preferred_or_primary_trust_term`
//! are TBox sweeps over the merged ontology (`GraphStore::ontology()`): their
//! subjects (gmeow:accordingTo / wasAttributedTo / confidence) live in the
//! standpoint module (cross-slice), and the absence guard must hold whole-graph.

use crate::conformance_support::*;

use purrdf::{RdfTerm, flat_rdf_quads_from_dataset, parse_dataset};

// ── Helpers ───────────────────────────────────────────────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_TRUST: &str = "https://blackcatinformatics.ca/gmeow/examples/trust/";
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Parse `data_nt` (N-Triples) into the native IR and check that the all-IRI triple
/// `(subject, predicate, object)` is present.
fn has_triple_nnn(data_nt: &str, subject: &str, predicate: &str, object: &str) -> bool {
    let dataset = parse_dataset(data_nt.as_bytes(), "application/n-triples", None)
        .expect("N-Triples parse must succeed");
    flat_rdf_quads_from_dataset(&dataset).iter().any(|quad| {
        matches!(&quad.subject, RdfTerm::Iri(s) if s == subject)
            && quad.predicate == predicate
            && matches!(&quad.object, RdfTerm::Iri(o) if o == object)
    })
}

// ── Migrated tests ────────────────────────────────────────────────────────────

/// `test_contested_certification_coexists` — A contested key↔identity binding:
/// one standpoint affirms, another refutes.  Both claims load, SHACL-pass, and
/// are retained — the refutation is first-class.
///
/// Data source: `tests/fixtures/coverage/trust-contested.ttl` (fixture-only
/// `Graph().parse(...)` → `run_shacl(g)` → `validate` mode).
#[gmeow_test_batch_macros::batch_test]
fn contested_certification_coexists() {
    let nt = fixture_as_nt("coverage", "trust-contested");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "contested trust fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );

    // The certification itself exists.
    assert!(
        has_triple_nnn(
            &nt,
            &format!("{EX_TRUST}contestedCert"),
            &format!(
                "{}{}",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "type"
            ),
            &format!("{GMEOW}Certification"),
        ),
        "ex:contestedCert must be typed as gmeow:Certification"
    );

    // Both standpoint axioms coexist: affirmation and refutation.
    assert!(
        has_triple_nnn(
            &nt,
            &format!("{EX_TRUST}claimCertAffirmed"),
            &format!(
                "{}{}",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "type"
            ),
            &format!("{OWL_NS}Axiom"),
        ),
        "ex:claimCertAffirmed must be typed as owl:Axiom"
    );
    assert!(
        has_triple_nnn(
            &nt,
            &format!("{EX_TRUST}claimCertRefuted"),
            &format!(
                "{}{}",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#", "type"
            ),
            &format!("{OWL_NS}Axiom"),
        ),
        "ex:claimCertRefuted must be typed as owl:Axiom"
    );
}

/// `test_three_axes_are_orthogonal_in_trust`: gmeow:accordingTo,
/// gmeow:wasAttributedTo, and gmeow:confidence stay orthogonal — no
/// subPropertyOf / equivalentProperty bridge among them (either direction) in the
/// merged ontology.
#[gmeow_test_batch_macros::batch_test]
fn three_axes_are_orthogonal_in_trust() {
    let g = GraphStore::ontology();
    let axes = [gm("accordingTo"), gm("wasAttributedTo"), gm("confidence")];
    for i in 0..axes.len() {
        for j in (i + 1)..axes.len() {
            let (a, b) = (&axes[i], &axes[j]);
            assert!(!g.has(Some(a), Some(RDFS_SUBPROPERTY_OF), Some(b)));
            assert!(!g.has(Some(b), Some(RDFS_SUBPROPERTY_OF), Some(a)));
            assert!(!g.has(Some(a), Some(OWL_EQUIVALENT_PROPERTY), Some(b)));
            assert!(!g.has(Some(b), Some(OWL_EQUIVALENT_PROPERTY), Some(a)));
        }
    }
}

/// `test_no_preferred_or_primary_trust_term` (Principle 9): trust mints no
/// preferred/primary selector for a contested certification or trust level —
/// a whole-graph absence guard over the merged ontology.
#[gmeow_test_batch_macros::batch_test]
fn no_preferred_or_primary_trust_term() {
    let g = GraphStore::ontology();
    let prop_types = [
        OWL_OBJECT_PROPERTY,
        OWL_DATATYPE_PROPERTY,
        OWL_ANNOTATION_PROPERTY,
    ];
    for banned in [
        "primaryCertification",
        "preferredCertification",
        "primaryTrust",
        "preferredTrust",
        "preferredRank",
    ] {
        let node = gm(banned);
        for pt in prop_types {
            assert!(
                !g.has(Some(&node), Some(RDF_TYPE), Some(pt)),
                "gmeow:{banned} must not exist as a property"
            );
        }
        assert!(
            !g.has(Some(&node), Some(RDF_TYPE), Some(OWL_CLASS)),
            "gmeow:{banned} must not exist as a class"
        );
    }
}
