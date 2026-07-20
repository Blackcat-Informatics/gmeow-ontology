// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_archaeological_evidence.py (whole
//! file; the Python file is deleted).
//!
//! Both twins run over the merged ontology (`GraphStore::ontology()`):
//!   - `attested_on_carrier_exists`: gmeow:attestedOnCarrier is defined in
//!     slices/extensions/lexicon/module.ttl (cross-slice), so a scopeModule cell
//!     over the archaeological-evidence module would miss it.
//!   - `no_primary_or_preferred_archaeological_terms`: a dynamic whole-graph
//!     subject sweep for any gmeow term whose local name begins with "primary"
//!     or "preferred"; narrowing to one module would gut the regression guard.

mod conformance_support;
use conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// gmeow:attestedOnCarrier is a non-functional object property from a
/// UsageAttestation to a PhysicalObject (the lexicon carrier hook).
#[test]
fn attested_on_carrier_exists() {
    let g = GraphStore::ontology();
    let prop = gm("attestedOnCarrier");
    assert!(
        g.has(Some(&prop), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)),
        "gmeow:attestedOnCarrier must be an owl:ObjectProperty"
    );
    assert!(
        !g.is_functional_carrier(&prop),
        "gmeow:attestedOnCarrier must NOT carry a logic: functionalProperty characteristic (one usage may cite several carriers)"
    );
    assert!(
        g.has(
            Some(&prop),
            Some(RDFS_DOMAIN),
            Some(&gm("UsageAttestation"))
        ),
        "gmeow:attestedOnCarrier domain must be gmeow:UsageAttestation"
    );
    assert!(
        g.has(Some(&prop), Some(RDFS_RANGE), Some(&gm("PhysicalObject"))),
        "gmeow:attestedOnCarrier range must be gmeow:PhysicalObject"
    );
}

/// Principle 9: no gmeow term whose local name begins with "primary" or
/// "preferred" is declared anywhere in the merged ontology — a whole-graph
/// subject sweep, catching any accidental re-introduction of a "one slot to win"
/// selector.
#[test]
fn no_primary_or_preferred_archaeological_terms() {
    let g = GraphStore::ontology();
    let offenders = g.primary_or_preferred_terms();
    assert!(
        offenders.is_empty(),
        "preferred/primary terms must not exist: {offenders:?}"
    );
}
