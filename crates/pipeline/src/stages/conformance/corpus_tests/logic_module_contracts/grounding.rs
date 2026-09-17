// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native calculus and namespace anchors remain justified by actual authored laws.

use std::collections::BTreeSet;

fn shipped_grounding_pairs() -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for (law, endpoints) in &super::observations().grounding_laws {
        assert_eq!(
            endpoints.sources.len(),
            1,
            "grounding correspondence {law} must declare exactly one logic:sourceEndpoint"
        );
        assert_eq!(
            endpoints.targets.len(),
            1,
            "grounding correspondence {law} must declare exactly one logic:targetEndpoint"
        );
        pairs.insert((endpoints.sources[0].clone(), endpoints.targets[0].clone()));
    }
    pairs
}

/// The reasoner's `CALCULUS_VOCABULARY`, consumed DIRECTLY from the engine via
/// [`gmeow_logic::reason::calculus_vocabulary`] rather than re-typed here — each
/// `(canonical logic: IRI, projected W3C OWL/RDFS IRI)` pair as owned `String`s. A
/// hand-copied mirror could drift from the engine table silently; reading the exposed
/// table makes any drift a compile/link fact, not a stale duplicate.
fn expected_calculus_pairs() -> BTreeSet<(String, String)> {
    gmeow_logic::reason::calculus_vocabulary()
        .iter()
        .map(|(canonical, projected)| ((*canonical).to_owned(), (*projected).to_owned()))
        .collect()
}

/// Every reasoner-lowered construct is backed by a shipped grounding law.
pub(super) fn calculus_vocabulary_is_backed_by_shipped_grounding_laws() {
    let shipped = shipped_grounding_pairs();
    assert!(
        !shipped.is_empty(),
        "no logic:GroundingCorrespondence laws parsed from the logic slice — the query is vacuous"
    );

    let expected = expected_calculus_pairs();
    // Non-vacuity: the mirrored table must be the full 52-row calculus vocabulary.
    assert_eq!(
        expected.len(),
        52,
        "expected calculus vocabulary must have 52 rows, matching CALCULUS_VOCABULARY"
    );

    let missing: Vec<&(String, String)> = expected.difference(&shipped).collect();
    assert!(
        missing.is_empty(),
        "reasoner CALCULUS_VOCABULARY rows with no shipped logic:GroundingCorrespondence \
         (source, target) law:\n{missing:#?}"
    );
}

/// The `gmeow-ns` typing-marker constants agree with the shipped laws, pinning
/// the ns anchors, the reasoner table, and the correspondence corpus together.
pub(super) fn ns_typing_marker_constants_match_shipped_laws() {
    let shipped = shipped_grounding_pairs();
    for (logic_iri, owl_iri) in [
        (gmeow_ns::LOGIC_CLASS, gmeow_ns::OWL_CLASS),
        (
            gmeow_ns::LOGIC_OBJECT_PROPERTY,
            gmeow_ns::OWL_OBJECT_PROPERTY,
        ),
        (
            gmeow_ns::LOGIC_DATATYPE_PROPERTY,
            gmeow_ns::OWL_DATATYPE_PROPERTY,
        ),
        (
            gmeow_ns::LOGIC_ANNOTATION_PROPERTY,
            gmeow_ns::OWL_ANNOTATION_PROPERTY,
        ),
        (
            gmeow_ns::LOGIC_NAMED_INDIVIDUAL,
            gmeow_ns::OWL_NAMED_INDIVIDUAL,
        ),
        (gmeow_ns::LOGIC_ONTOLOGY, gmeow_ns::OWL_ONTOLOGY),
        (gmeow_ns::LOGIC_THING, gmeow_ns::OWL_THING),
        (gmeow_ns::LOGIC_NOTHING, gmeow_ns::OWL_NOTHING),
        (gmeow_ns::LOGIC_RESTRICTION, gmeow_ns::OWL_RESTRICTION),
    ] {
        let pair = (logic_iri.to_owned(), owl_iri.to_owned());
        assert!(
            shipped.contains(&pair),
            "ns constant pair {pair:?} has no backing logic:GroundingCorrespondence law"
        );
    }
}
