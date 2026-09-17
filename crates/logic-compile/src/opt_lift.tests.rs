// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::projections::shapes::{project_validation_shape_shacl, shacl_residue};

/// The vendored GECCO blood-pressure systolic constraint: half-open [0, 1000) mm[Hg].
fn systolic() -> OptConstraintIr {
    OptConstraintIr {
        shape_iri: "https://gmeow.example/openehr/bp/SystolicShape".into(),
        target_class: "https://gmeow.example/openehr/bp/Systolic".into(),
        kind: OptConstraintKind::Quantity {
            magnitude_path: "https://gmeow.example/openehr/bp/magnitude".into(),
            interval: OptInterval {
                lower: Some(0.0),
                upper: Some(1000.0),
                lower_included: true,
                upper_included: false,
            },
            units_path: "https://gmeow.example/openehr/bp/units".into(),
            units: "mm[Hg]".into(),
            precision: None,
        },
    }
}

/// Round-trip an OPT constraint through the shape IR and assert the identity.
fn assert_u_after_d_is_identity(c: &OptConstraintIr) {
    let shape = lift_opt_to_validation_shape(c).unwrap();
    let recovered = recover_opt_from_shape(&shape).unwrap();
    assert_eq!(&recovered, c, "u∘d must be the identity");
}

#[test]
fn quantity_round_trips_and_projects_half_open() {
    let c = systolic();
    assert_u_after_d_is_identity(&c);
    let ttl = project_validation_shape_shacl(&lift_opt_to_validation_shape(&c).unwrap());
    assert!(ttl.contains("sh:minInclusive 0"), "{ttl}");
    assert!(ttl.contains("sh:maxExclusive 1000"), "{ttl}");
    assert!(ttl.contains("mm[Hg]"), "{ttl}");
}

#[test]
fn quantity_with_precision_round_trips_and_projects() {
    // A Quantity carrying an optional precision satellite [1, 1] decimal places: the satellite
    // must round-trip (u∘d=id) WITHOUT tripping the recovery ambiguity guard (it is not a
    // second discriminator), and it must project to SHACL as an ordinary numeric facet.
    let mut c = systolic();
    match &mut c.kind {
        OptConstraintKind::Quantity { precision, .. } => {
            *precision = Some((
                "https://gmeow.example/openehr/bp/precision".into(),
                OptInterval {
                    lower: Some(1.0),
                    upper: Some(1.0),
                    lower_included: true,
                    upper_included: true,
                },
            ));
        }
        _ => unreachable!(),
    }
    assert_u_after_d_is_identity(&c);
    let shape = lift_opt_to_validation_shape(&c).unwrap();
    let ttl = project_validation_shape_shacl(&shape);
    assert!(ttl.contains("sh:minInclusive 1"), "{ttl}");
    assert!(ttl.contains("sh:maxInclusive 1"), "{ttl}");
    // The precision satellite is faithfully projected — no loss-ledger residue.
    assert!(
        shacl_residue(&shape).is_empty(),
        "{:?}",
        shacl_residue(&shape)
    );
}

#[test]
fn quantity_round_trips_across_inclusivity() {
    for (li, ui) in [(true, false), (false, true), (true, true), (false, false)] {
        let mut c = systolic();
        match &mut c.kind {
            OptConstraintKind::Quantity { interval, .. } => {
                interval.lower_included = li;
                interval.upper_included = ui;
            }
            _ => unreachable!(),
        }
        assert_u_after_d_is_identity(&c);
    }
}

#[test]
fn cardinality_round_trips() {
    let c = OptConstraintIr {
        shape_iri: "https://ex/S".into(),
        target_class: "https://ex/C".into(),
        kind: OptConstraintKind::Cardinality {
            path: "https://ex/items".into(),
            min: Some(1),
            max: Some(3),
        },
    };
    assert_u_after_d_is_identity(&c);
    let ttl = project_validation_shape_shacl(&lift_opt_to_validation_shape(&c).unwrap());
    assert!(
        ttl.contains("sh:minCount 1") && ttl.contains("sh:maxCount 3"),
        "{ttl}"
    );
}

#[test]
fn value_set_round_trips() {
    let c = OptConstraintIr {
        shape_iri: "https://ex/S".into(),
        target_class: "https://ex/C".into(),
        kind: OptConstraintKind::ValueSet {
            path: "https://ex/code".into(),
            codes: vec!["https://ex/at0004".into(), "https://ex/at0005".into()],
        },
    };
    assert_u_after_d_is_identity(&c);
    let ttl = project_validation_shape_shacl(&lift_opt_to_validation_shape(&c).unwrap());
    assert!(
        ttl.contains("sh:in ( <https://ex/at0004> <https://ex/at0005> )"),
        "{ttl}"
    );
}

#[test]
fn datetime_round_trips() {
    let c = OptConstraintIr {
        shape_iri: "https://ex/S".into(),
        target_class: "https://ex/C".into(),
        kind: OptConstraintKind::DateTime {
            path: "https://ex/when".into(),
            range: OptDateTimeRange {
                lower: Some("2020-01-01T00:00:00Z".into()),
                upper: Some("2030-01-01T00:00:00Z".into()),
                lower_included: true,
                upper_included: false,
            },
        },
    };
    assert_u_after_d_is_identity(&c);
    let ttl = project_validation_shape_shacl(&lift_opt_to_validation_shape(&c).unwrap());
    assert!(
        ttl.contains("\"2020-01-01T00:00:00Z\"^^xsd:dateTime"),
        "{ttl}"
    );
}

#[test]
fn string_pattern_round_trips_exactly_but_is_ledgered_lossy() {
    let c = OptConstraintIr {
        shape_iri: "https://ex/S".into(),
        target_class: "https://ex/C".into(),
        kind: OptConstraintKind::StringPattern {
            path: "https://ex/name".into(),
            regex: "^[A-Z][a-z]+$".into(),
            flags: None,
        },
    };
    // Exact at the IR level ...
    assert_u_after_d_is_identity(&c);
    // ... but the SHACL projection declares the regex-dialect loss.
    let shape = lift_opt_to_validation_shape(&c).unwrap();
    assert_eq!(shacl_residue(&shape).len(), 1, "pattern must be ledgered");
}

#[test]
fn terminology_binding_round_trips_exactly_but_is_ledgered_lossy() {
    let c = OptConstraintIr {
        shape_iri: "https://ex/S".into(),
        target_class: "https://ex/C".into(),
        kind: OptConstraintKind::TerminologyBinding {
            path: "https://ex/code".into(),
            terminology_id: "SNOMED-CT".into(),
            codes: vec!["271649006".into()],
        },
    };
    assert_u_after_d_is_identity(&c);
    let shape = lift_opt_to_validation_shape(&c).unwrap();
    // The external terminology is not emitted into SHACL, but is ledgered.
    let ttl = project_validation_shape_shacl(&shape);
    assert!(!ttl.contains("SNOMED"), "{ttl}");
    assert_eq!(
        shacl_residue(&shape).len(),
        1,
        "terminology must be ledgered"
    );
}

#[test]
fn ordinal_round_trips() {
    let c = OptConstraintIr {
        shape_iri: "https://ex/S".into(),
        target_class: "https://ex/C".into(),
        kind: OptConstraintKind::Ordinal {
            path: "https://ex/value".into(),
            ordinals: vec![
                (1, "https://ex/terminology/local/at0014".into()),
                (2, "https://ex/terminology/local/at0015".into()),
            ],
        },
    };
    assert_u_after_d_is_identity(&c);
}

#[test]
fn datetime_pattern_round_trips() {
    let c = OptConstraintIr {
        shape_iri: "https://ex/S".into(),
        target_class: "https://ex/C".into(),
        kind: OptConstraintKind::DateTimePattern {
            path: "https://ex/value".into(),
            pattern: "yyyy-mm-ddTHH:MM:SS".into(),
        },
    };
    assert_u_after_d_is_identity(&c);
}

#[test]
fn ordinal_and_value_set_do_not_alias() {
    // The discriminator-distinctness guarantee: an Ordinal must recover to Ordinal (NOT
    // ValueSet), and a DateTimePattern must recover to DateTimePattern (NOT StringPattern) —
    // the OPT lift must not collapse these into the plain `In`/`Pattern` components that
    // would recover to the wrong family.
    let ordinal = OptConstraintIr {
        shape_iri: "https://ex/S1".into(),
        target_class: "https://ex/C1".into(),
        kind: OptConstraintKind::Ordinal {
            path: "https://ex/value".into(),
            ordinals: vec![(1, "https://ex/terminology/local/at0014".into())],
        },
    };
    let shape = lift_opt_to_validation_shape(&ordinal).unwrap();
    let recovered = recover_opt_from_shape(&shape).unwrap();
    assert!(
        matches!(recovered.kind, OptConstraintKind::Ordinal { .. }),
        "expected Ordinal, got {:?}",
        recovered.kind
    );

    let datetime_pattern = OptConstraintIr {
        shape_iri: "https://ex/S2".into(),
        target_class: "https://ex/C2".into(),
        kind: OptConstraintKind::DateTimePattern {
            path: "https://ex/value".into(),
            pattern: "yyyy-mm-ddTHH:MM:SS".into(),
        },
    };
    let shape = lift_opt_to_validation_shape(&datetime_pattern).unwrap();
    let recovered = recover_opt_from_shape(&shape).unwrap();
    assert!(
        matches!(recovered.kind, OptConstraintKind::DateTimePattern { .. }),
        "expected DateTimePattern, got {:?}",
        recovered.kind
    );
}

#[test]
fn recover_hard_fails_on_a_non_opt_shape() {
    let empty = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![],
        None,
    )
    .unwrap();
    assert!(recover_opt_from_shape(&empty).is_err());
}

#[test]
fn recover_hard_fails_on_an_ambiguous_multi_family_shape() {
    // Two discriminating families in one shape: a datetime range AND a string pattern. A
    // well-formed lifted OPT constraint carries exactly one family, so recovery must
    // HARD-FAIL rather than silently pick the first by iteration order (defends u∘d=id).
    let dt = PropertyConstraintIr::new(
        "https://ex/when",
        None,
        None,
        None,
        vec![ConstraintComponent::DateTimeRange {
            min: Some("2020-01-01T00:00:00Z".into()),
            max: None,
            min_inclusive: true,
            max_inclusive: false,
        }],
    )
    .unwrap();
    let pat = PropertyConstraintIr::new(
        "https://ex/name",
        None,
        None,
        None,
        vec![ConstraintComponent::Pattern {
            regex: "^x".into(),
            flags: None,
        }],
    )
    .unwrap();
    let shape = ValidationShapeIr::new(
        "https://ex/S",
        ShapeTarget::Class("https://ex/C".into()),
        vec![dt, pat],
        None,
    )
    .unwrap();
    let err = recover_opt_from_shape(&shape).unwrap_err();
    assert!(
        err.message().contains("ambiguous"),
        "expected an ambiguity hard-fail, got: {err}"
    );
}
