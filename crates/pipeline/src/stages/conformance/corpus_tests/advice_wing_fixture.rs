// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read the real advice constraints' producer-observed diagnostics and claims.
//! Full shapes, ontology hydration and validation belong to the explicit producer.

use std::collections::BTreeSet;
use std::path::Path;

use gmeow_errors::Severity;
use gmeow_validate::advisory::DEONTIC_RECOMMENDATION_IRI;

use super::super::advice_wing::{ARTIFACT, Observation};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const BARE_THING: &str = "https://ex.test/bareThing";
const GOOD_THING: &str = "https://ex.test/goodThing";
const BAD_EVENT: &str = "https://ex.test/badEvent";

/// The original verbatim Entity avoidance prose, independently graded against
/// both the owning projected shape and the advisory the control actually fired.
const EXPECTED_MESSAGE: &str = "Avoid typing an instance as a bare gmeow:Entity when a more \
     specific sortal applies; reserve the unqualified type for genuinely category-neutral \
     resources, and never use it for occurrents (those are gufo:Event, not endurants).";
/// The original Event avoidance prose; the Entity+Event control fires this advice.
const EXPECTED_EAE_MESSAGE: &str = "Avoid typing an endurant as an Event (a person/document/place \
     is a gmeow:Entity, not an occurrence), avoid minting an event-kind SUBCLASS (the kind is a \
     gmeow:eventType value, Principle 9), and reach for gmeow:Activity when the occurrence is an \
     agent-driven provenance act with inputs and outputs.";

/// Pin all source-provenance fields on the same real constraint's projected shape,
/// preventing an unrelated Info shape or message from satisfying the old contract.
fn assert_shape_source(observed: &Observation, constraint: &str, term: &str, message: &str) {
    let candidates: Vec<_> = observed
        .shape_sources
        .iter()
        .filter(|(iri, _)| iri.contains(constraint))
        .collect();
    assert!(
        !candidates.is_empty(),
        "real projected {constraint} source is required"
    );
    assert!(
        candidates.iter().any(|(_, properties)| {
            properties.get("https://blackcatinformatics.ca/logic/formalizes")
                == Some(&BTreeSet::from([format!("<{GMEOW}{term}>")]))
                && properties.get("http://www.w3.org/ns/shacl#severity")
                    == Some(&BTreeSet::from([
                        "<http://www.w3.org/ns/shacl#Info>".to_owned()
                    ]))
                && properties.get("http://www.w3.org/ns/shacl#message")
                    == Some(&BTreeSet::from([format!(
                        "\"{message}\"^^<http://www.w3.org/2001/XMLSchema#string>"
                    )]))
        }),
        "the same {constraint} shape must retain formalization, Info severity and verbatim message: {candidates:?}"
    );
}

/// Preserve the exact three advice matches, both original messages and Note
/// severities, the proper-sort control, and the emitted deontic claim contracts.
#[test]
fn bare_entity_fixture_fires_the_real_advisory_constraint_end_to_end() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(&root, ARTIFACT)
        .expect("explicit producer supplies exact advice observations");
    let observed: Observation = serde_json::from_slice(&bytes).expect("typed advice observations");
    assert_eq!(observed.profile, super::super::production_shapes::PROFILE);
    assert_shape_source(
        &observed,
        "BareEntitySortalAdviceConstraint",
        "Entity",
        EXPECTED_MESSAGE,
    );
    assert_shape_source(
        &observed,
        "EndurantAsEventAdviceConstraint",
        "Event",
        EXPECTED_EAE_MESSAGE,
    );
    assert!(
        observed.retained_conforms,
        "non-advisory report must conform: {:?}",
        observed.retained_findings
    );
    let advisories = &observed.advisories;
    assert_eq!(
        advisories.len(),
        3,
        "one bareThing and two badEvent advisories: {advisories:?}"
    );
    let bare = advisories
        .iter()
        .find(|advice| {
            advice.code.contains("BareEntitySortalAdviceConstraint")
                && advice.subject_iri.as_deref() == Some(BARE_THING)
        })
        .expect("bare Entity advice must fire on bareThing");
    assert!(bare.code.starts_with("advice."));
    assert_eq!(bare.severity, Severity::Note);
    assert_eq!(bare.message, EXPECTED_MESSAGE);
    let eae = advisories
        .iter()
        .find(|advice| advice.code.contains("EndurantAsEventAdviceConstraint"))
        .expect("endurant-as-Event advice must fire");
    assert_eq!(eae.subject_iri.as_deref(), Some(BAD_EVENT));
    assert_eq!(eae.severity, Severity::Note);
    assert_eq!(eae.message, EXPECTED_EAE_MESSAGE);
    let eae_subjects: Vec<_> = advisories
        .iter()
        .filter(|advice| advice.code.contains("EndurantAsEventAdviceConstraint"))
        .filter_map(|advice| advice.subject_iri.as_deref())
        .collect();
    assert_eq!(eae_subjects, vec![BAD_EVENT]);
    let bare_subjects: BTreeSet<_> = advisories
        .iter()
        .filter(|advice| advice.code.contains("BareEntitySortalAdviceConstraint"))
        .filter_map(|advice| advice.subject_iri.as_deref())
        .collect();
    assert_eq!(bare_subjects, BTreeSet::from([BARE_THING, BAD_EVENT]));
    for advice in advisories {
        assert_eq!(advice.modality_iri, DEONTIC_RECOMMENDATION_IRI);
        assert_eq!(advice.claim_code, advice.code);
        assert_eq!(advice.claim_subject_iri, advice.subject_iri);
        assert_eq!(advice.advised_proposition, advice.message);
        assert!(
            !advice.standpoint_iri.is_empty(),
            "the claim retains its issuing standpoint"
        );
    }
    assert_eq!(eae.claim_subject_iri.as_deref(), Some(BAD_EVENT));
    let nquads = &observed.claims_nquads;
    assert!(
        nquads.contains(&format!("{}/assessment", eae.claim_code)),
        "claim identity includes the advice code: {nquads}"
    );
    assert!(
        nquads.contains(&format!(
            "<{GMEOW}deonticModality> <{DEONTIC_RECOMMENDATION_IRI}>"
        )),
        "emitted recommendation modality: {nquads}"
    );
    assert!(
        nquads.contains(&format!("<{GMEOW}ComplianceAssessment>")),
        "emitted assessment type: {nquads}"
    );
    assert!(
        !advisories
            .iter()
            .any(|advice| advice.subject_iri.as_deref() == Some(GOOD_THING)),
        "proper Entity+Agent must not trigger advice: {advisories:?}"
    );
}
