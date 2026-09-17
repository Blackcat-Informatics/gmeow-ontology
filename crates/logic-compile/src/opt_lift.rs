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
use gmeow_errors::Diag;

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
pub fn lift_opt_to_validation_shape(
    c: &OptConstraintIr,
) -> gmeow_errors::Result<ValidationShapeIr> {
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
                vec![ConstraintComponent::In(vec![ShapeValue::Literal(
                    purrdf::RdfLiteral {
                        lexical_form: units.clone(),
                        datatype: Some(XSD_STRING.to_owned()),
                        language: None,
                        direction: None,
                    },
                )])],
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
pub fn recover_opt_from_shape(shape: &ValidationShapeIr) -> gmeow_errors::Result<OptConstraintIr> {
    let target_class = match &shape.target {
        ShapeTarget::Class(c) => c.clone(),
        ShapeTarget::ValueKeyed { .. } => {
            return Err(Diag::of_kind(crate::error::OptLift {
                detail: "recover_opt_from_shape: a value-keyed target is not an OPT constraint"
                    .to_owned(),
            }));
        }
        ShapeTarget::SubjectsOf(_) | ShapeTarget::ObjectsOf(_) => {
            return Err(Diag::of_kind(crate::error::OptLift {
                detail: "recover_opt_from_shape: a subjects-of/objects-of (domain/range) target is not an \
                 OPT constraint"
                    .to_owned(),
            }));
        }
        ShapeTarget::DirectClass(_) => {
            return Err(Diag::of_kind(crate::error::OptLift {
                detail: "recover_opt_from_shape: a direct-instance target is not an OPT constraint"
                    .to_owned(),
            }));
        }
        ShapeTarget::Sparql(_) => {
            return Err(Diag::of_kind(crate::error::OptLift {
                detail: "recover_opt_from_shape: a raw-sparql target is not an OPT constraint"
                    .to_owned(),
            }));
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
        0 => Err(Diag::of_kind(crate::error::OptLift {
            detail: "recover_opt_from_shape: shape is not a recognizable lifted OPT constraint"
                .to_owned(),
        })),
        1 => Ok(mk(
            shape,
            target_class,
            recovered.into_iter().next().expect("len checked == 1"),
        )),
        n => Err(Diag::of_kind(crate::error::OptLift {
            detail: format!(
                "recover_opt_from_shape: ambiguous shape — {n} discriminating OPT families present; \
             a lifted OPT constraint must carry exactly one (no silent first-wins recovery)"
            ),
        })),
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
fn recover_units(shape: &ValidationShapeIr) -> gmeow_errors::Result<(String, String)> {
    for p in &shape.properties {
        for comp in &p.components {
            if let ConstraintComponent::In(vs) = comp
                && let [
                    ShapeValue::Literal(purrdf::RdfLiteral {
                        lexical_form: lexical,
                        ..
                    }),
                ] = vs.as_slice()
            {
                return Ok((p.path.clone(), lexical.clone()));
            }
        }
    }
    Err(Diag::of_kind(crate::error::OptLift {
        detail: "recover_opt_from_shape: quantity shape has no units (singleton sh:in literal)"
            .to_owned(),
    }))
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

#[path = "opt_lift.tests.rs"]
#[cfg(test)]
mod tests;
