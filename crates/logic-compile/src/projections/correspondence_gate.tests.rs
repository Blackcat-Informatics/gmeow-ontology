// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn equivalence_predicates_are_recognized() {
    for p in [
        "skos:exactMatch",
        "http://www.w3.org/2004/02/skos/core#exactMatch",
        "exactMatch",
        "owl:equivalentClass",
        "owl:equivalentProperty",
        "=",
        "Equiv",
    ] {
        assert!(is_equivalence_predicate(p), "{p} should be equivalence");
    }
    for p in [
        "skos:closeMatch",
        "skos:broadMatch",
        "<=",
        "rdfs:subClassOf",
    ] {
        assert!(
            !is_equivalence_predicate(p),
            "{p} should NOT be equivalence"
        );
    }
}

#[test]
fn equiv_correspondence_may_emit_exact_match() {
    // The honest case: a genuine equivalence emits exactMatch — allowed.
    assert!(
        assert_relation_no_overclaim(
            "sssom",
            CorrespondenceRelation::Equiv,
            MorphismClass::Isomorphism,
            MorphismKind::InstitutionMorphism,
            "skos:exactMatch",
        )
        .is_ok()
    );
    // A weaker relation emitting a weaker predicate is fine too.
    assert!(
        assert_relation_no_overclaim(
            "sssom",
            CorrespondenceRelation::Overlaps,
            MorphismClass::LossyLens,
            MorphismKind::InstitutionMorphism,
            "skos:closeMatch",
        )
        .is_ok()
    );
}

#[test]
fn bridge_view_emitting_equivalence_is_red() {
    // The issue's first RED witness: a bridge view emitting equivalence.
    let err = assert_relation_no_overclaim(
        "edoal",
        CorrespondenceRelation::Equiv,
        MorphismClass::BridgeView,
        MorphismKind::CommitmentShiftingBridge,
        "=",
    )
    .unwrap_err();
    assert!(err.0.contains("bridge"), "{}", err.0);
    assert!(err.0.contains("Principle 5"), "{}", err.0);
}

#[test]
fn caveated_overlap_emitting_exact_match_is_red() {
    // The issue's second RED witness: a caveated overlap emitting sssom exactMatch.
    let err = assert_relation_no_overclaim(
        "sssom",
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        "skos:exactMatch",
    )
    .unwrap_err();
    assert!(err.0.contains("Overlaps"), "{}", err.0);
    assert!(err.0.contains("equivalence"), "{}", err.0);
}

#[test]
fn affect_closematch_bridge_emitting_exact_match_is_red() {
    // The affect epic's Stage-5 witness: a classifier-registry bridge is a
    // CAVEATED closeMatch (a GoEmotions label is not guaranteed to denote the
    // same ontological thing as a canonical GMEOW emotion). If such a bridge
    // ever emitted an equivalence predicate, the overclaim gate MUST red it —
    // closeMatch-by-default, exactMatch only after review (never loosen the
    // recall floor).
    let err = assert_relation_no_overclaim(
        "sssom",
        CorrespondenceRelation::Overlaps,
        MorphismClass::LossyLens,
        MorphismKind::InstitutionMorphism,
        "skos:exactMatch",
    )
    .unwrap_err();
    assert!(err.0.contains("Overlaps"), "{}", err.0);
    assert!(err.0.contains("equivalence"), "{}", err.0);
    // The honest closeMatch a GoEmotions bridge actually emits is never flagged.
    assert!(
        assert_relation_no_overclaim(
            "sssom",
            CorrespondenceRelation::Overlaps,
            MorphismClass::LossyLens,
            MorphismKind::InstitutionMorphism,
            "skos:closeMatch",
        )
        .is_ok()
    );
}
