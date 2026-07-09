// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! openEHR OPT constraint IR and its **pure** lift to the canonical [`ValidationShapeIr`].
//!
//! This is the XML-free half of the ADL2/OPT constraints axis. The `roxmltree` reader
//! ([`crate::openehr_opt`]) parses an Operational Template into [`OptConstraintIr`] values; this
//! module lifts each to a `logic:` validation shape, from which the SHACL Core and ShEx
//! surfaces are projected ([`crate::projections::shapes`]). Keeping the lift here — with no
//! XML dependency — is what lets `crates/logic-compile` stay wasm-clean (the reusable-crate
//! ring-fence) while still owning the canonical lowering.
//!
//! **The round-trip law.** The OPT↔`logic:` leg is *structurally exact for every family*:
//! [`recover_opt_from_shape`] ∘ [`lift_opt_to_validation_shape`] is the identity (the
//! section/retraction `u∘d=id` law the conformance gate pins). Loss enters only downstream,
//! at the `logic:`→SHACL/ShEx projection — a `C_STRING` regex dialect and an external
//! terminology binding have no faithful shape form, so their fidelity is carried and flagged
//! in the loss ledger ([`crate::projections::shapes::shacl_residue`]), never dropped in
//! silence. The IR round-trip preserves them exactly; the *shape surface* is where they are
//! declared lossy.

use crate::ir::{
    ConstraintComponent, ConstraintProvenance, PropertyConstraintIr, ShapeTarget, ShapeValue,
    ValidationShapeIr,
};

/// The datatype every C_DV_QUANTITY magnitude bound carries in the projected shape.
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// The datatype a C_DV_QUANTITY units value carries in the projected shape.
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

/// A half-open-capable numeric interval parsed from an OPT `C_DV_QUANTITY` `<magnitude>`.
/// A `None` bound is an open end (the OPT omits or leaves that side unconstrained).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OptInterval {
    /// The lower bound (`None` ⇒ unbounded below).
    pub lower: Option<f64>,
    /// The upper bound (`None` ⇒ unbounded above).
    pub upper: Option<f64>,
    /// Whether `lower` is admitted (`sh:minInclusive` vs `sh:minExclusive`).
    pub lower_included: bool,
    /// Whether `upper` is admitted (`sh:maxInclusive` vs `sh:maxExclusive`).
    pub upper_included: bool,
}

/// A half-open-capable `xsd:dateTime` interval parsed from an OPT `C_DATE_TIME` range.
#[derive(Debug, Clone, PartialEq)]
pub struct OptDateTimeRange {
    /// The lower bound lexical (`None` ⇒ unbounded below).
    pub lower: Option<String>,
    /// The upper bound lexical (`None` ⇒ unbounded above).
    pub upper: Option<String>,
    /// Whether `lower` is admitted.
    pub lower_included: bool,
    /// Whether `upper` is admitted.
    pub upper_included: bool,
}

/// A parsed openEHR OPT constraint for one archetype `ELEMENT` node — the pure, XML-free
/// carrier the shape surfaces lower from.
#[derive(Debug, Clone, PartialEq)]
pub struct OptConstraintIr {
    /// The IRI to mint for the lifted validation shape.
    pub shape_iri: String,
    /// The RM/domain class the shape targets (`sh:targetClass`).
    pub target_class: String,
    /// The constraint payload (one node-kind family).
    pub kind: OptConstraintKind,
}

/// The OPT constraint node-kind families. Every variant is structurally exactly invertible
/// through the shape IR; the `StringPattern` and `TerminologyBinding` variants are lossy only
/// under the *SHACL/ShEx projection* (recorded in the loss ledger), not under the IR lift.
#[derive(Debug, Clone, PartialEq)]
pub enum OptConstraintKind {
    /// `C_DV_QUANTITY`: a magnitude interval on `magnitude_path` and a `units` value on
    /// `units_path` (the sibling DV_QUANTITY unit slot).
    Quantity {
        /// The predicate reaching the magnitude value.
        magnitude_path: String,
        /// The magnitude interval bounds and inclusivity.
        interval: OptInterval,
        /// The predicate reaching the units value.
        units_path: String,
        /// The unit string (e.g. `mm[Hg]`).
        units: String,
        /// The optional `DV_QUANTITY.precision` decimal-place-count interval, paired with the
        /// predicate reaching it. `None` when the OPT omits `<precision>`. Precision counts are
        /// integers carried as `f64` (e.g. `1.0..1.0`); it is a non-discriminating satellite of the
        /// Quantity family (recovered by [`recover_precision`], never a second magnitude range).
        precision: Option<(String, OptInterval)>,
    },
    /// `occurrences` / `existence` / cardinality: a closed-world count bound on `path`.
    Cardinality {
        /// The predicate the count bound applies to.
        path: String,
        /// Minimum occurrences (`sh:minCount`; `None` ⇒ unbounded below).
        min: Option<u32>,
        /// Maximum occurrences (`sh:maxCount`; `None` ⇒ unbounded above).
        max: Option<u32>,
    },
    /// `C_DV_ORDINAL` / `C_DV_CODED_TEXT`: an inline value set of coded terms (IRIs) on `path`.
    ValueSet {
        /// The predicate the value set applies to.
        path: String,
        /// The admitted coded-term IRIs.
        codes: Vec<String>,
    },
    /// `C_DATE_TIME`: an `xsd:dateTime` interval on `path`.
    DateTime {
        /// The predicate the datetime range applies to.
        path: String,
        /// The datetime interval.
        range: OptDateTimeRange,
    },
    /// `C_STRING`: a regular-expression pattern on `path`. Lossy under projection (the SHACL
    /// regex dialect differs from the source), but the regex string round-trips exactly here.
    StringPattern {
        /// The predicate the pattern applies to.
        path: String,
        /// The regular expression.
        regex: String,
        /// Optional SHACL `sh:flags`.
        flags: Option<String>,
    },
    /// A `term_binding` / `C_TERMINOLOGY_CODE`: an external terminology reference on `path`.
    /// Lossy under projection (no faithful closed shape form), but the id + codes round-trip
    /// exactly here.
    TerminologyBinding {
        /// The predicate the binding applies to.
        path: String,
        /// The terminology identifier (e.g. `SNOMED-CT`, `openehr`).
        terminology_id: String,
        /// The bound codes.
        codes: Vec<String>,
    },
    /// `C_DV_ORDINAL`: an ordinal value set of (ordinal integer, coded-symbol IRI) pairs on `path`.
    Ordinal {
        /// The predicate the ordinal set applies to.
        path: String,
        /// The (ordinal integer, coded-symbol IRI) pairs.
        ordinals: Vec<(i64, String)>,
    },
    /// `C_DATE_TIME` validity pattern: a required datetime precision/format pattern on `path`.
    DateTimePattern {
        /// The predicate the datetime pattern applies to.
        path: String,
        /// The openEHR validity pattern (e.g. `yyyy-mm-ddTHH:MM:SS`).
        pattern: String,
    },
}

/// Lift an [`OptConstraintIr`] to the canonical [`ValidationShapeIr`] (the `d`/down leg).
///
/// Every family lowers to one target class with one or two property shapes carrying the
/// corresponding [`ConstraintComponent`]s. OPT-native cardinality is
/// [`ConstraintProvenance::OptNative`] (closed-world by construction).
pub fn lift_opt_to_validation_shape(c: &OptConstraintIr) -> Result<ValidationShapeIr, String> {
    let target = ShapeTarget::Class(c.target_class.clone());
    let properties = match &c.kind {
        OptConstraintKind::Quantity {
            magnitude_path,
            interval,
            units_path,
            units,
            precision,
        } => {
            let magnitude = PropertyConstraintIr::new(
                magnitude_path,
                None,
                None,
                None,
                vec![
                    ConstraintComponent::NumericRange {
                        min: interval.lower,
                        max: interval.upper,
                        min_inclusive: interval.lower_included,
                        max_inclusive: interval.upper_included,
                    },
                    ConstraintComponent::Datatype(XSD_DECIMAL.to_owned()),
                ],
            )?;
            let unit = PropertyConstraintIr::new(
                units_path,
                Some(1),
                Some(1),
                Some(ConstraintProvenance::OptNative),
                vec![ConstraintComponent::In(vec![ShapeValue::Literal {
                    lexical: units.clone(),
                    datatype: Some(XSD_STRING.to_owned()),
                    lang: None,
                }])],
            )?;
            let mut properties = vec![magnitude, unit];
            // The optional precision satellite: a single PrecisionRange property, kept distinct
            // from the magnitude NumericRange so recovery never treats it as a second discriminator.
            if let Some((precision_path, precision_interval)) = precision {
                properties.push(PropertyConstraintIr::new(
                    precision_path,
                    None,
                    None,
                    None,
                    vec![ConstraintComponent::PrecisionRange {
                        min: precision_interval.lower,
                        max: precision_interval.upper,
                        min_inclusive: precision_interval.lower_included,
                        max_inclusive: precision_interval.upper_included,
                    }],
                )?);
            }
            properties
        }
        OptConstraintKind::Cardinality { path, min, max } => vec![PropertyConstraintIr::new(
            path,
            *min,
            *max,
            Some(ConstraintProvenance::OptNative),
            vec![],
        )?],
        OptConstraintKind::ValueSet { path, codes } => vec![PropertyConstraintIr::new(
            path,
            None,
            None,
            None,
            vec![ConstraintComponent::In(
                codes.iter().map(|c| ShapeValue::Iri(c.clone())).collect(),
            )],
        )?],
        OptConstraintKind::DateTime { path, range } => vec![PropertyConstraintIr::new(
            path,
            None,
            None,
            None,
            vec![ConstraintComponent::DateTimeRange {
                min: range.lower.clone(),
                max: range.upper.clone(),
                min_inclusive: range.lower_included,
                max_inclusive: range.upper_included,
            }],
        )?],
        OptConstraintKind::StringPattern { path, regex, flags } => vec![PropertyConstraintIr::new(
            path,
            None,
            None,
            None,
            vec![ConstraintComponent::Pattern {
                regex: regex.clone(),
                flags: flags.clone(),
            }],
        )?],
        OptConstraintKind::TerminologyBinding {
            path,
            terminology_id,
            codes,
        } => vec![PropertyConstraintIr::new(
            path,
            None,
            None,
            None,
            vec![ConstraintComponent::TerminologyBinding {
                terminology_id: terminology_id.clone(),
                codes: codes.clone(),
            }],
        )?],
        OptConstraintKind::Ordinal { path, ordinals } => vec![PropertyConstraintIr::new(
            path,
            None,
            None,
            None,
            vec![ConstraintComponent::OrdinalSet {
                pairs: ordinals.clone(),
            }],
        )?],
        OptConstraintKind::DateTimePattern { path, pattern } => vec![PropertyConstraintIr::new(
            path,
            None,
            None,
            None,
            vec![ConstraintComponent::DateTimePattern(pattern.clone())],
        )?],
    };
    ValidationShapeIr::new(&c.shape_iri, target, properties, None)
}

/// Recover an [`OptConstraintIr`] from a lifted [`ValidationShapeIr`] (the `u`/up leg). This
/// is the structural inverse of [`lift_opt_to_validation_shape`] across every family: it
/// detects the family from the discriminating component and reconstructs the OPT constraint.
/// Hard-fails if the shape is not a well-formed lifted OPT constraint (no silent defaulting).
pub fn recover_opt_from_shape(shape: &ValidationShapeIr) -> Result<OptConstraintIr, String> {
    let target_class = match &shape.target {
        ShapeTarget::Class(c) => c.clone(),
        ShapeTarget::ValueKeyed { .. } => {
            return Err(
                "recover_opt_from_shape: a value-keyed target is not an OPT constraint".into(),
            );
        }
        ShapeTarget::SubjectsOf(_) | ShapeTarget::ObjectsOf(_) => {
            return Err(
                "recover_opt_from_shape: a subjects-of/objects-of (domain/range) target is not an \
                 OPT constraint"
                    .into(),
            );
        }
        ShapeTarget::DirectClass(_) => {
            return Err(
                "recover_opt_from_shape: a direct-instance target is not an OPT constraint".into(),
            );
        }
        ShapeTarget::Sparql(_) => {
            return Err(
                "recover_opt_from_shape: a raw-sparql target is not an OPT constraint".into(),
            );
        }
    };
    // Collect EVERY family's discriminating component — never return on the first match. A
    // well-formed lifted OPT constraint carries exactly one discriminating family (Quantity's
    // units is an In-of-Literal, a value set is an In-of-IRI, so the two In shapes never
    // collide). More than one match means the shape is ambiguous (structure was gained or lost),
    // so we HARD-FAIL rather than silently pick one by iteration order — that silent pick would
    // let a lossy shape masquerade as a faithful inverse and break the u∘d=id law.
    let mut recovered: Vec<OptConstraintKind> = Vec::new();
    for p in &shape.properties {
        for comp in &p.components {
            match comp {
                ConstraintComponent::NumericRange {
                    min,
                    max,
                    min_inclusive,
                    max_inclusive,
                } => {
                    let (units_path, units) = recover_units(shape)?;
                    recovered.push(OptConstraintKind::Quantity {
                        magnitude_path: p.path.clone(),
                        interval: OptInterval {
                            lower: *min,
                            upper: *max,
                            lower_included: *min_inclusive,
                            upper_included: *max_inclusive,
                        },
                        units_path,
                        units,
                        // A satellite, not a discriminator: recovered by scanning for the
                        // PrecisionRange property (absent ⇒ None), so u∘d=id in both directions.
                        precision: recover_precision(shape),
                    });
                }
                ConstraintComponent::DateTimeRange {
                    min,
                    max,
                    min_inclusive,
                    max_inclusive,
                } => {
                    recovered.push(OptConstraintKind::DateTime {
                        path: p.path.clone(),
                        range: OptDateTimeRange {
                            lower: min.clone(),
                            upper: max.clone(),
                            lower_included: *min_inclusive,
                            upper_included: *max_inclusive,
                        },
                    });
                }
                ConstraintComponent::Pattern { regex, flags } => {
                    recovered.push(OptConstraintKind::StringPattern {
                        path: p.path.clone(),
                        regex: regex.clone(),
                        flags: flags.clone(),
                    });
                }
                ConstraintComponent::TerminologyBinding {
                    terminology_id,
                    codes,
                } => {
                    recovered.push(OptConstraintKind::TerminologyBinding {
                        path: p.path.clone(),
                        terminology_id: terminology_id.clone(),
                        codes: codes.clone(),
                    });
                }
                ConstraintComponent::OrdinalSet { pairs } => {
                    recovered.push(OptConstraintKind::Ordinal {
                        path: p.path.clone(),
                        ordinals: pairs.clone(),
                    });
                }
                ConstraintComponent::DateTimePattern(pattern) => {
                    recovered.push(OptConstraintKind::DateTimePattern {
                        path: p.path.clone(),
                        pattern: pattern.clone(),
                    });
                }
                ConstraintComponent::In(vs)
                    if vs.iter().all(|v| matches!(v, ShapeValue::Iri(_))) =>
                {
                    let codes = vs
                        .iter()
                        .map(|v| match v {
                            ShapeValue::Iri(i) => i.clone(),
                            _ => unreachable!("guarded to all-IRI above"),
                        })
                        .collect();
                    recovered.push(OptConstraintKind::ValueSet {
                        path: p.path.clone(),
                        codes,
                    });
                }
                // A precision satellite is NOT a discriminator (recovered by `recover_precision`
                // inside the Quantity arm); ignore it here so it never inflates the family count
                // and trips the ambiguity guard.
                ConstraintComponent::PrecisionRange { .. } => {}
                _ => {}
            }
        }
    }
    // A bare cardinality (occurrences/existence) constraint carries no value component.
    for p in &shape.properties {
        if (p.min_count.is_some() || p.max_count.is_some()) && p.components.is_empty() {
            recovered.push(OptConstraintKind::Cardinality {
                path: p.path.clone(),
                min: p.min_count,
                max: p.max_count,
            });
        }
    }
    match recovered.len() {
        0 => {
            Err("recover_opt_from_shape: shape is not a recognizable lifted OPT constraint".into())
        }
        1 => Ok(mk(
            shape,
            target_class,
            recovered.into_iter().next().expect("len checked == 1"),
        )),
        n => Err(format!(
            "recover_opt_from_shape: ambiguous shape — {n} discriminating OPT families present; \
             a lifted OPT constraint must carry exactly one (no silent first-wins recovery)"
        )),
    }
}

/// Reconstruct the shape's `iri`/`target_class` envelope around a recovered kind.
fn mk(shape: &ValidationShapeIr, target_class: String, kind: OptConstraintKind) -> OptConstraintIr {
    OptConstraintIr {
        shape_iri: shape.iri.clone(),
        target_class,
        kind,
    }
}

/// Recover a quantity shape's `units_path`/`units` (the singleton `In`-of-literal property).
fn recover_units(shape: &ValidationShapeIr) -> Result<(String, String), String> {
    for p in &shape.properties {
        for comp in &p.components {
            if let ConstraintComponent::In(vs) = comp
                && let [ShapeValue::Literal { lexical, .. }] = vs.as_slice()
            {
                return Ok((p.path.clone(), lexical.clone()));
            }
        }
    }
    Err("recover_opt_from_shape: quantity shape has no units (singleton sh:in literal)".into())
}

/// Recover a quantity shape's optional precision satellite — the property carrying the single
/// [`ConstraintComponent::PrecisionRange`] component — as `(path, interval)`. Returns `None` when
/// no property carries a precision range (a quantity without a precision constraint).
fn recover_precision(shape: &ValidationShapeIr) -> Option<(String, OptInterval)> {
    for p in &shape.properties {
        for comp in &p.components {
            if let ConstraintComponent::PrecisionRange {
                min,
                max,
                min_inclusive,
                max_inclusive,
            } = comp
            {
                return Some((
                    p.path.clone(),
                    OptInterval {
                        lower: *min,
                        upper: *max,
                        lower_included: *min_inclusive,
                        upper_included: *max_inclusive,
                    },
                ));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
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
            err.contains("ambiguous"),
            "expected an ambiguity hard-fail, got: {err}"
        );
    }
}
