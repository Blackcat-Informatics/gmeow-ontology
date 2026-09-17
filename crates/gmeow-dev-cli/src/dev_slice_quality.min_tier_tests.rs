// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A five-rung ladder mirroring the shipped rubric (Registered..Maximal).
fn ladder() -> MeasurementStandard {
    let rung = |local: &str, label: &str, rank: i64| Tier {
        iri: format!("{}{local}", gmeow_slice_quality::model::GMEOW),
        label: label.to_owned(),
        rank,
    };
    MeasurementStandard {
        tiers: vec![
            rung("tierRegistered", "Registered", 0),
            rung("tierGrounded", "Grounded", 1),
            rung("tierLinked", "Linked", 2),
            rung("tierExemplified", "Exemplified", 3),
            rung("tierMaximal", "Maximal", 4),
        ],
        axes: vec![],
    }
}

fn tier(r: &MeasurementStandard, label: &str) -> Tier {
    resolve_min_tier(r, label).unwrap().clone()
}

#[test]
fn gate_below_required_fails() {
    // Measured Grounded(1) vs required Maximal(4) → below the bar, gate fails.
    let r = ladder();
    let measured = tier(&r, "Grounded");
    let required = tier(&r, "Maximal");
    assert!(
        !tier_gate_passes(&measured, Some(&required)),
        "measured below required must not pass"
    );
}

#[test]
fn gate_at_or_above_required_passes() {
    let r = ladder();
    let required = tier(&r, "Linked");
    // Exactly at the bar passes.
    assert!(tier_gate_passes(&tier(&r, "Linked"), Some(&required)));
    // Above the bar passes.
    assert!(tier_gate_passes(&tier(&r, "Maximal"), Some(&required)));
}

#[test]
fn gate_unset_is_advisory_pass() {
    // --min-tier unset → advisory, always passes even at the floor tier.
    let r = ladder();
    assert!(tier_gate_passes(&tier(&r, "Registered"), None));
}

#[test]
fn resolve_accepts_label_and_local_case_insensitively() {
    let r = ladder();
    assert_eq!(resolve_min_tier(&r, "Grounded").unwrap().rank, 1);
    assert_eq!(resolve_min_tier(&r, "grounded").unwrap().rank, 1);
    // The IRI local name is also accepted.
    assert_eq!(resolve_min_tier(&r, "tierMaximal").unwrap().rank, 4);
}

#[test]
fn resolve_unknown_tier_errors_naming_the_rungs() {
    let r = ladder();
    let err = resolve_min_tier(&r, "Platinum").unwrap_err();
    assert!(err.message().contains("unknown --min-tier"), "{err}");
    assert!(
        err.message().contains("Platinum"),
        "names the bad input: {err}"
    );
    // Lists the available rungs, ladder-ordered.
    assert!(
        err.message()
            .contains("Registered, Grounded, Linked, Exemplified, Maximal"),
        "lists rungs: {err}"
    );
}
