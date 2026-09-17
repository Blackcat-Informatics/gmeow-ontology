// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::parse_dataset;

const CORE: &str = "https://blackcatinformatics.ca/gmeow/gmnRingCore";
const TRUSTED: &str = "https://blackcatinformatics.ca/gmeow/gmnRingTrusted";

// The authored lattice and demonstrator are exercised by producer observations.
// These independent failure controls use only explicit, tiny inputs.
fn lattice() -> RingLattice {
    let dataset = parse_dataset(
        br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.test/> .
gmeow:gmnRingCore gmeow:gmnRingLevel ex:core .
gmeow:gmnRingTrusted gmeow:gmnRingLevel ex:trusted .
gmeow:gmnRingRestricted gmeow:gmnRingLevel ex:restricted .
ex:core gmeow:gmnRingLevelDominates ex:core, ex:trusted, ex:restricted .
ex:trusted gmeow:gmnRingLevelDominates ex:trusted, ex:restricted .
ex:restricted gmeow:gmnRingLevelDominates ex:restricted .
"#,
        "text/turtle",
        None,
    )
    .expect("tiny ring coordinates");
    RingLattice::from_dataset(&dataset)
}

fn model(ttl: &str) -> Gmn0Model {
    let dataset =
        parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("tiny consume-path input");
    Gmn0Model::from_dataset(&dataset)
}

#[test]
fn unclassified_content_raises_the_named_leak_class() {
    let input = model(
        "@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/lang/> .\n\
             ex:ringDemoOrphan ex:ringDemoField ex:ringDemoOrphanDatum .\n",
    );
    let error = consume_project(&input, &lattice(), TRUSTED, None, &GmnDictionary::default())
        .expect_err("unclassified content must fail closed");
    assert!(matches!(error, GmnConsumeError::Unclassified { .. }));
    assert_eq!(error.failure_class(), CLASS_RING_LEAK);
}

#[test]
fn admitted_reference_to_excluded_content_raises_the_named_leak_class() {
    let input = model(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://blackcatinformatics.ca/gmeow/examples/lang/> .\n\
             ex:ringDemoCore gmeow:gmnContentRing gmeow:gmnRingCore ;\n\
                 ex:ringDemoRefers ex:ringDemoRestricted .\n\
             ex:ringDemoRestricted gmeow:gmnContentRing gmeow:gmnRingRestricted ;\n\
                 ex:ringDemoField ex:ringDemoRestrictedDatum .\n",
    );
    let selected_lattice = lattice();
    assert_eq!(selected_lattice.within(CORE, TRUSTED), Some(true));
    let error = consume_project(
        &input,
        &selected_lattice,
        TRUSTED,
        None,
        &GmnDictionary::default(),
    )
    .expect_err("an admitted reference to excluded content must fail closed");
    assert!(matches!(error, GmnConsumeError::ReferenceLeak { .. }));
    assert_eq!(error.failure_class(), CLASS_RING_LEAK);
}

#[test]
fn unknown_target_ring_hard_fails() {
    let error = consume_project(
        &Gmn0Model::default(),
        &lattice(),
        "https://example.org/notARing",
        None,
        &GmnDictionary::default(),
    )
    .expect_err("unknown target ring must fail closed even for an empty input");
    assert!(matches!(error, GmnConsumeError::UnknownTargetRing { .. }));
    assert_eq!(error.failure_class(), CLASS_RING_LATTICE_MALFORMED);
}
