// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
