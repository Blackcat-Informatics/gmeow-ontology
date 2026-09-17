// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated consume-path outputs; corpus construction belongs to the producer.

use gmeow_lang_bridge::{CLASS_RING_LATTICE_MALFORMED, CLASS_RING_LEAK, GmnConsumeError};

use super::super::gmn_consume::{CORE, NATO, Observation, RESTRICTED, SOURCE, TRUSTED};

fn observation() -> &'static Observation {
    super::gmn_grounding::observations()
        .sources
        .get(SOURCE)
        .expect("required ring-consumption demonstrator")
        .consume
        .as_ref()
        .expect("producer-recorded ring-consumption outputs")
}

#[test]
fn within_closure_matches_the_authored_lattice() {
    let observed = observation();
    for ring in [CORE, TRUSTED, RESTRICTED, NATO] {
        assert!(observed.rings.contains(ring), "required ring {ring}");
    }
    for (content, target, expected) in [
        (CORE, TRUSTED, true),
        (CORE, RESTRICTED, true),
        (TRUSTED, CORE, false),
        (RESTRICTED, TRUSTED, false),
        (NATO, TRUSTED, true),
        (TRUSTED, NATO, false),
        (CORE, NATO, false),
    ] {
        assert_eq!(
            observed.within[content][target],
            Some(expected),
            "authored within relation {content} -> {target}",
        );
    }
    assert_eq!(observed.within["https://example.org/notARing"][CORE], None);
}

#[test]
fn consume_path_excludes_out_of_ring_content() {
    let observed = observation();
    let trusted = observed.trusted.as_ref().expect("trusted projection");
    assert_eq!(trusted.target, TRUSTED);
    assert!(trusted.text.contains("ringDemoCoreDatum"));
    assert!(trusted.text.contains("ringDemoTrustedDatum"));
    assert!(trusted.text.contains("ringDemoNatoDatum"));
    assert!(!trusted.text.contains("ringDemoRestrictedDatum"));
    assert_eq!(trusted.excluded_claims, 1);
    assert_eq!(trusted.admitted_claims, 3);

    let nato = observed.nato.as_ref().expect("NATO projection");
    assert_eq!(nato.target, NATO);
    assert!(nato.text.contains("ringDemoNatoDatum"));
    assert!(!nato.text.contains("ringDemoTrustedDatum"));
    assert!(!nato.text.contains("ringDemoCoreDatum"));
    assert!(!nato.text.contains("ringDemoRestrictedDatum"));
    assert_eq!(nato.admitted_claims, 1);
    assert_eq!(nato.excluded_claims, 3);

    let core = observed.core.as_ref().expect("core projection");
    assert_eq!(core.target, CORE);
    assert!(core.text.contains("ringDemoCoreDatum"));
    assert!(!core.text.contains("ringDemoTrustedDatum"));
    assert!(!core.text.contains("ringDemoNatoDatum"));
    assert!(!core.text.contains("ringDemoRestrictedDatum"));
    assert_eq!(core.admitted_claims, 1);
}

#[test]
fn consume_path_fit_discloses_elision() {
    let observed = observation();
    let full = observed.trusted.as_ref().expect("full trusted projection");
    assert_eq!(full.elided_claims, 0);
    assert!(full.disclosure.is_none());
    assert!(
        full.tokens > 1,
        "the full projection has a measurable token size"
    );
    let fitted = observed
        .budgeted
        .as_ref()
        .expect("a successful nonempty baseline requires the budget probe")
        .as_ref()
        .expect("budgeted trusted projection");
    assert_eq!(fitted.target, TRUSTED);
    assert_eq!(fitted.budget, Some(full.tokens - 1));
    assert_eq!(fitted.admitted_claims, full.admitted_claims);
    assert!(fitted.emitted_claims < fitted.admitted_claims);
    assert!(fitted.emitted_claims >= 1);
    assert_eq!(
        fitted.elided_claims,
        fitted.admitted_claims - fitted.emitted_claims,
    );
    let disclosure = fitted
        .disclosure
        .as_ref()
        .expect("elision must be disclosed");
    assert!(disclosure.contains(&format!(
        "{} of {} admitted claims elided",
        fitted.elided_claims, fitted.admitted_claims,
    )));
    assert!(disclosure.contains("never silently dropped"));
}

#[test]
fn unclassified_content_raises_the_named_leak_class() {
    let error = observation()
        .unclassified
        .as_ref()
        .expect_err("unclassified content must fail closed");
    assert!(matches!(error, GmnConsumeError::Unclassified { .. }));
    assert_eq!(error.failure_class(), CLASS_RING_LEAK);
}

#[test]
fn admitted_reference_to_excluded_content_raises_the_named_leak_class() {
    let error = observation()
        .reference_leak
        .as_ref()
        .expect_err("an admitted reference to excluded content must fail closed");
    assert!(matches!(error, GmnConsumeError::ReferenceLeak { .. }));
    assert_eq!(error.failure_class(), CLASS_RING_LEAK);
}

#[test]
fn unknown_target_ring_hard_fails() {
    let error = observation()
        .unknown_target
        .as_ref()
        .expect_err("unknown target ring must fail closed");
    assert!(matches!(error, GmnConsumeError::UnknownTargetRing { .. }));
    assert_eq!(error.failure_class(), CLASS_RING_LATTICE_MALFORMED);
}
