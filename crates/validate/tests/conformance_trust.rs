// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_trust.py (#867).
//!
//! Migrated tests load the `trust-contested.ttl` coverage fixture, validate
//! it against the whole shapes corpus, and assert triple membership using the
//! parsed N-Triples store — mirroring the Python fixture-only `run_shacl(g)`
//! pattern.
//!
//! Retained in Python (not migrated):
//!   - `test_three_axes_are_orthogonal_in_trust`: uses `_graph()` /
//!     `load_merged_graph` — a pure TBox structural sweep over the merged
//!     ontology.
//!   - `test_no_preferred_or_primary_trust_term`: uses `_graph()` — a dynamic
//!     whole-graph sweep; cannot be expressed without the merged graph.

mod conformance_support;
use conformance_support::*;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::GraphNameRef;
use oxigraph::store::Store;

// ── Helpers ───────────────────────────────────────────────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_TRUST: &str = "https://blackcatinformatics.ca/gmeow/examples/trust/";
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";

/// Parse `data_nt` (N-Triples) into an in-memory oxigraph store and check
/// that the triple `(subject, predicate, object)` is present (all named nodes).
fn has_triple_nnn(data_nt: &str, subject: &str, predicate: &str, object: &str) -> bool {
    let store = Store::new().expect("in-memory store");
    store
        .load_from_reader(
            RdfParser::from_format(RdfFormat::NTriples),
            data_nt.as_bytes(),
        )
        .expect("N-Triples parse must succeed");
    let s = oxigraph::model::NamedNode::new(subject).expect("valid IRI");
    let p = oxigraph::model::NamedNode::new(predicate).expect("valid IRI");
    let o = oxigraph::model::NamedNode::new(object).expect("valid IRI");
    store
        .contains(&oxigraph::model::Quad::new(
            s,
            p,
            oxigraph::model::Term::NamedNode(o),
            GraphNameRef::DefaultGraph,
        ))
        .expect("store.contains must not error")
}

// ── Migrated tests ────────────────────────────────────────────────────────────

/// `test_contested_certification_coexists` — A contested key↔identity binding:
/// one standpoint affirms, another refutes.  Both claims load, SHACL-pass, and
/// are retained — the refutation is first-class.
///
/// Data source: `tests/fixtures/coverage/trust-contested.ttl` (fixture-only
/// `Graph().parse(...)` → `run_shacl(g)` → `validate` mode).
#[test]
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
