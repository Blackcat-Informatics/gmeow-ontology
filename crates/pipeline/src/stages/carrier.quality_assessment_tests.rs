// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn quality_assessment_fanout_path_is_registered_and_folds_as_ntriples() {
    // The attach ↔ committed-path bijection the superset gate enforces: the carrier
    // attaches `graph/fanout/quality/gmeow.quality-assessment.nt` and the gate claims
    // the same committed path as an N-Triples fanout fold. A committed path with no
    // attaching stage (or vice-versa) is a wiring contradiction — this pins both legs.
    let rdf_fanout_classes = crate::stages::superset::source_contracts::authored_fanout_classes();
    assert!(
        rdf_fanout_classes.contains(QUALITY_ASSESSMENT_PATH),
        "the quality-assessment committed path must be a registered RDF-fanout class"
    );
    let iri = crate::stages::superset::rdf_fanout_graph_iri(QUALITY_ASSESSMENT_PATH)
        .expect("committed path yields a fanout graph IRI");
    assert_eq!(
        crate::stages::superset::rdf_fanout_path_for_graph_iri(&iri).as_deref(),
        Some(QUALITY_ASSESSMENT_PATH),
        "the fanout IRI must invert back to the committed path (bijection)"
    );
}
