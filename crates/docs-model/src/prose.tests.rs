// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn boundary_cues_match_only_at_word_boundaries() {
    assert!(states_boundary("A relator, never a mere pair."));
    assert!(!states_boundary("Applies whenever a bearer exists."));
    assert!(states_boundary("A role, rather than a kind."));
}

#[test]
fn the_boilerplate_coat_is_not_a_boundary() {
    // The STRICT semantics: the mechanically-appended coat contains "not" but
    // distinguishes nothing, so it must not credit the ratchet-gated axis.
    assert!(!states_boundary(
        "A thing. It is not an interchangeable alias for a broader, narrower, or merely related construct."
    ));
    // The coat alone, uppercased, is still no boundary once excised.
    assert!(!states_boundary(
        "It is NOT AN INTERCHANGEABLE ALIAS FOR A BROADER, NARROWER, OR MERELY RELATED CONSTRUCT."
    ));
    // …and the excision is not a blanket veto: a REAL boundary cue phrased
    // OUTSIDE the coat still counts even though the coat is also present.
    assert!(states_boundary(
        "A relator, never a mere pair. It is not an interchangeable alias for a broader, narrower, or merely related construct."
    ));
}

#[test]
fn worked_triples_need_a_curie_and_turtle_structure() {
    assert!(is_worked_triple("ex:x a gmeow:Foo ."));
    assert!(!is_worked_triple("See section 3: important."));
    assert!(!is_worked_triple("just prose with no term reference."));
}

#[test]
fn ownership_metadata_is_not_a_worked_example() {
    // The STRICT semantics: a generated provenance inventory is not evidence of
    // the term being USED.
    assert!(!is_worked_triple(
        "gmeow:Foo rdfs:isDefinedBy gmeow:sliceCoreKernel ."
    ));
}

#[test]
fn curie_detection_rejects_prose_colons_and_iri_schemes() {
    assert!(has_curie("gmeow:Foo"));
    assert!(!has_curie("section 3: important"));
    assert!(!has_curie("<http://example.org/x>"));
}
