// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! openEHR OPT constraint IR and its **pure** lift to the canonical [`ValidationShapeIr`].
//!
//! This is the XML-free half of the ADL2/OPT constraints axis. The `roxmltree` reader
//! (`crates/shacl`) parses an Operational Template into [`OptConstraintIr`] values; this
//! module lifts each to a `logic:` validation shape, from which the SHACL Core and ShEx
//! surfaces are projected ([`crate::projections::shapes`]). Keeping the lift here — with no
//! XML dependency — is what lets `crates/logic-compile` stay wasm-clean (the reusable-crate
//! ring-fence) while still owning the canonical lowering.
//!
//! The quantity family is the **exactly-invertible** keystone: a magnitude interval
//! (`lower`/`upper`/`lower_included`/`upper_included`) plus a `units` value round-trips
//! through the shape with zero lossy fields, so [`recover_opt_from_shape`] ∘
//! [`lift_opt_to_validation_shape`] is the identity (the `u∘d=id` section/retraction law the
//! conformance gate pins). Later families (patterns, terminology bindings) round-trip only
//! up to the loss ledger; those are added as further [`OptConstraintKind`] variants.

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

/// The OPT constraint node-kind families. The quantity family is exactly invertible; further
/// variants (ordinal/coded-text value sets, string patterns, datetime, terminology bindings)
/// extend this sum as they are added.
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
    },
}

/// Lift an [`OptConstraintIr`] to the canonical [`ValidationShapeIr`] (the `d`/down leg).
///
/// Quantity: the magnitude path carries the interval as a
/// [`ConstraintComponent::NumericRange`] plus an `xsd:decimal` datatype; the units path
/// carries the unit as a singleton [`ConstraintComponent::In`] with an exactly-one
/// cardinality (a natively closed-world OPT constraint, so [`ConstraintProvenance::OptNative`]).
pub fn lift_opt_to_validation_shape(c: &OptConstraintIr) -> Result<ValidationShapeIr, String> {
    match &c.kind {
        OptConstraintKind::Quantity {
            magnitude_path,
            interval,
            units_path,
            units,
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
            ValidationShapeIr::new(
                &c.shape_iri,
                ShapeTarget::Class(c.target_class.clone()),
                vec![magnitude, unit],
                None,
                None,
                false,
            )
        }
    }
}

/// Recover an [`OptConstraintIr`] from a lifted [`ValidationShapeIr`] (the `u`/up leg) for
/// the exactly-invertible quantity family. This is the structural inverse of
/// [`lift_opt_to_validation_shape`]: it reads the `NumericRange` back to an interval and the
/// singleton units value-set back to a unit string. Hard-fails if the shape is not a
/// well-formed lifted quantity constraint (no silent defaulting).
pub fn recover_opt_from_shape(shape: &ValidationShapeIr) -> Result<OptConstraintIr, String> {
    let target_class = match &shape.target {
        ShapeTarget::Class(c) => c.clone(),
        ShapeTarget::ValueKeyed { .. } => {
            return Err(
                "recover_opt_from_shape: a value-keyed target is not a quantity shape".into(),
            )
        }
    };
    let mut interval: Option<OptInterval> = None;
    let mut magnitude_path: Option<String> = None;
    let mut units: Option<String> = None;
    let mut units_path: Option<String> = None;
    for p in &shape.properties {
        for comp in &p.components {
            match comp {
                ConstraintComponent::NumericRange {
                    min,
                    max,
                    min_inclusive,
                    max_inclusive,
                } => {
                    interval = Some(OptInterval {
                        lower: *min,
                        upper: *max,
                        lower_included: *min_inclusive,
                        upper_included: *max_inclusive,
                    });
                    magnitude_path = Some(p.path.clone());
                }
                ConstraintComponent::In(vs) => {
                    if let [ShapeValue::Literal { lexical, .. }] = vs.as_slice() {
                        units = Some(lexical.clone());
                        units_path = Some(p.path.clone());
                    }
                }
                _ => {}
            }
        }
    }
    Ok(OptConstraintIr {
        shape_iri: shape.iri.clone(),
        target_class,
        kind: OptConstraintKind::Quantity {
            magnitude_path: magnitude_path
                .ok_or("recover_opt_from_shape: no magnitude (NumericRange) property")?,
            interval: interval.ok_or("recover_opt_from_shape: no interval")?,
            units_path: units_path
                .ok_or("recover_opt_from_shape: no units (singleton sh:in) property")?,
            units: units.ok_or("recover_opt_from_shape: no units value")?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projections::shapes::project_validation_shape_shacl;

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
            },
        }
    }

    #[test]
    fn lift_quantity_produces_a_targeted_shape_with_interval_and_units() {
        let shape = lift_opt_to_validation_shape(&systolic()).unwrap();
        assert_eq!(shape.node_kind, crate::ir::NodeKind::ValidationShape);
        assert_eq!(shape.properties.len(), 2);
        // The lifted shape, projected to SHACL, is the half-open interval.
        let ttl = project_validation_shape_shacl(&shape);
        assert!(ttl.contains("sh:minInclusive 0"), "{ttl}");
        assert!(ttl.contains("sh:maxExclusive 1000"), "{ttl}");
        assert!(
            !ttl.contains("sh:maxInclusive"),
            "half-open upper must be exclusive: {ttl}"
        );
        assert!(ttl.contains("mm[Hg]"), "units must survive the lift: {ttl}");
    }

    #[test]
    fn u_after_d_is_identity_on_the_quantity_family() {
        // The section/retraction law: recover ∘ lift = id on the exactly-invertible family.
        let original = systolic();
        let shape = lift_opt_to_validation_shape(&original).unwrap();
        let recovered = recover_opt_from_shape(&shape).unwrap();
        assert_eq!(
            recovered, original,
            "u∘d must be the identity on a quantity constraint"
        );
    }

    #[test]
    fn u_after_d_is_identity_across_inclusivity_combinations() {
        for (li, ui) in [(true, false), (false, true), (true, true), (false, false)] {
            let mut c = systolic();
            // Irrefutable while Quantity is the only variant; a `match` keeps this honest
            // (it must gain arms when new OptConstraintKind families land).
            match &mut c.kind {
                OptConstraintKind::Quantity { interval, .. } => {
                    interval.lower_included = li;
                    interval.upper_included = ui;
                }
            }
            let shape = lift_opt_to_validation_shape(&c).unwrap();
            let recovered = recover_opt_from_shape(&shape).unwrap();
            assert_eq!(recovered, c, "u∘d must hold for inclusivity ({li},{ui})");
        }
    }

    #[test]
    fn recover_hard_fails_on_a_non_quantity_shape() {
        let empty = ValidationShapeIr::new(
            "https://ex/S",
            ShapeTarget::Class("https://ex/C".into()),
            vec![],
            None,
            None,
            false,
        )
        .unwrap();
        assert!(recover_opt_from_shape(&empty).is_err());
    }
}
