// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The rubric loader: read the ontology-resident rubric out of an RDF dataset.
//!
//! The rubric is authored as `gmeow:Profile`/`gmeow:QualityAxis`/… individuals in
//! `slices/core/slice-quality-rubric/module.ttl`. This module resolves that data
//! into the [`Rubric`] the scorer consumes — so tuning a threshold or minting an
//! axis is a slice edit, never a code change. A malformed rubric (an axis with no
//! producer, a threshold with no tier) is a hard error, never a silent skip.

use purrdf::RdfDataset;

use crate::graph::{all_iris, g, id, instances_of, label_of, one_iri, one_lit};
use crate::model::{
    Axis, AxisFloorCommitment, ContextScope, Exemption, GovernanceFloors, MeasurementStandard,
    Rubric, SliceTierFloorCommitment, Threshold, Tier,
};

/// Wrap a structural-rubric-defect message as a typed diagnostic on the substrate,
/// preserving the authored text verbatim.
fn rubric_err(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Rubric {
        detail: detail.into(),
    })
}

/// Load the whole rubric from a dataset that contains the rubric module graph.
///
/// # Errors
/// Returns a message if the rubric is structurally incomplete — no tier ladder,
/// an axis missing its producer/dimension/scope, or a threshold naming an
/// unknown tier. A missing required binding is a hard fail, never papered over.
pub fn load_rubric(ds: &RdfDataset) -> gmeow_errors::Result<Rubric> {
    // --- Tiers -------------------------------------------------------------
    let rank_p = id(ds, &g("tierRank"));
    let mut tiers: Vec<Tier> = Vec::new();
    for iri in instances_of(ds, &g("QualityTier")) {
        let sid = id(ds, &iri).ok_or_else(|| rubric_err(format!("tier {iri} not resolvable")))?;
        let rank = rank_p
            .and_then(|p| one_lit(ds, sid, p))
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| rubric_err(format!("tier {iri} has no integer gmeow:tierRank")))?;
        tiers.push(Tier {
            iri,
            label: label_of(ds, sid),
            rank,
        });
    }
    if tiers.is_empty() {
        return Err(rubric_err("rubric has no gmeow:QualityTier ladder"));
    }
    tiers.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    // --- Thresholds (resolved inline per axis) -----------------------------
    let thr_tier_p = id(ds, &g("thresholdTier"));
    let thr_floor_p = id(ds, &g("thresholdFloor"));
    let load_threshold = |thr_iri: &str| -> gmeow_errors::Result<Threshold> {
        let tid = id(ds, thr_iri)
            .ok_or_else(|| rubric_err(format!("threshold {thr_iri} not resolvable")))?;
        let tier_iri = thr_tier_p
            .and_then(|p| one_iri(ds, tid, p))
            .ok_or_else(|| rubric_err(format!("threshold {thr_iri} has no gmeow:thresholdTier")))?;
        let floor = thr_floor_p
            .and_then(|p| one_lit(ds, tid, p))
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| {
                rubric_err(format!(
                    "threshold {thr_iri} has no decimal gmeow:thresholdFloor"
                ))
            })?;
        // A NaN/±inf floor is silently poisonous downstream: it collapses the
        // `score + EPSILON >= floor` gate checks and the ascending floor sort
        // below into non-deterministic or vacuous comparisons. Hard-fail here
        // rather than let a malformed literal degrade the ladder silently.
        if !floor.is_finite() {
            return Err(rubric_err(format!(
                "threshold {thr_iri} has a non-finite gmeow:thresholdFloor {floor}"
            )));
        }
        Ok(Threshold { tier_iri, floor })
    };

    // --- Axes --------------------------------------------------------------
    let producer_p = id(ds, &g("axisProducer"));
    let dimension_p = id(ds, &g("axisDimension"));
    let threshold_p = id(ds, &g("axisThreshold"));
    let weight_p = id(ds, &g("axisWeight"));
    let scope_p = id(ds, &g("axisContextScope"));
    let advice_p = id(ds, &g("axisAdviceTemplate"));

    let mut axes: Vec<Axis> = Vec::new();
    for iri in instances_of(ds, &g("QualityAxis")) {
        let sid = id(ds, &iri).ok_or_else(|| rubric_err(format!("axis {iri} not resolvable")))?;
        let producer = producer_p
            .and_then(|p| one_lit(ds, sid, p))
            .ok_or_else(|| rubric_err(format!("axis {iri} has no gmeow:axisProducer")))?;
        let dimension_iri = dimension_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| rubric_err(format!("axis {iri} has no gmeow:axisDimension")))?;
        let scope_iri = scope_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| rubric_err(format!("axis {iri} has no gmeow:axisContextScope")))?;
        let scope = ContextScope::from_local(scope_iri.rsplit(['/', '#']).next().unwrap_or(""))
            .ok_or_else(|| rubric_err(format!("axis {iri} names unknown scope {scope_iri}")))?;
        // Only a MISSING gmeow:axisWeight defaults to 1.0 (unweighted). A
        // PRESENT value must be a finite number: a non-finite (NaN/±inf) weight
        // parses fine as an f64 and then silently collapses the advisory
        // weight-rank comparator (`partial_cmp(..).unwrap_or(Equal)` in
        // report.rs) into a no-op order, and a non-numeric weight would silently
        // degrade back to the default. Both are hard fails, never papered over.
        let weight = match weight_p.and_then(|p| one_lit(ds, sid, p)) {
            None => 1.0,
            Some(s) => match s.parse::<f64>() {
                Ok(w) if w.is_finite() => w,
                Ok(w) => {
                    return Err(rubric_err(format!(
                        "axis {iri} has a non-finite gmeow:axisWeight {w}"
                    )));
                }
                Err(_) => {
                    return Err(rubric_err(format!(
                        "axis {iri} has a non-numeric gmeow:axisWeight {s:?}"
                    )));
                }
            },
        };
        let advice = advice_p
            .and_then(|p| one_lit(ds, sid, p))
            .unwrap_or_default();

        let mut thresholds: Vec<Threshold> = Vec::new();
        if let Some(p) = threshold_p {
            for thr_iri in all_iris(ds, sid, p) {
                thresholds.push(load_threshold(&thr_iri)?);
            }
        }
        if thresholds.is_empty() {
            return Err(rubric_err(format!("axis {iri} has no gmeow:axisThreshold")));
        }
        // Validate every threshold names a real tier.
        for t in &thresholds {
            if !tiers.iter().any(|tier| tier.iri == t.tier_iri) {
                return Err(rubric_err(format!(
                    "axis {iri} threshold names unknown tier {}",
                    t.tier_iri
                )));
            }
        }
        thresholds.sort_by(|a, b| {
            a.floor
                .partial_cmp(&b.floor)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        axes.push(Axis {
            iri,
            label: label_of(ds, sid),
            producer,
            dimension_iri,
            thresholds,
            weight,
            scope,
            advice,
        });
    }
    if axes.is_empty() {
        return Err(rubric_err("rubric has no gmeow:QualityAxis individuals"));
    }
    axes.sort_by(|a, b| a.iri.cmp(&b.iri));

    // --- Exemptions --------------------------------------------------------
    let exempts_p = id(ds, &g("exemptsAxis"));
    let reason_p = id(ds, &g("exemptionReason"));
    let date_p = id(ds, &g("exemptionDate"));
    let exproducer_p = id(ds, &g("exemptionProducer"));
    let mut exemptions: Vec<Exemption> = Vec::new();
    for iri in instances_of(ds, &g("AxisExemption")) {
        let sid =
            id(ds, &iri).ok_or_else(|| rubric_err(format!("exemption {iri} not resolvable")))?;
        let axis_iri = exempts_p
            .and_then(|p| one_iri(ds, sid, p))
            .unwrap_or_default();
        let reason = reason_p
            .and_then(|p| one_lit(ds, sid, p))
            .unwrap_or_default();
        let date = date_p.and_then(|p| one_lit(ds, sid, p)).unwrap_or_default();
        let producer = exproducer_p
            .and_then(|p| one_lit(ds, sid, p))
            .unwrap_or_default();
        if producer.is_empty() || date.is_empty() {
            return Err(rubric_err(format!(
                "exemption {iri} must carry a dated producer symbol"
            )));
        }
        // Every exemption must name a REAL loaded axis. A missing or unknown
        // gmeow:exemptsAxis is a hard fail (.goals no-optionality): otherwise the
        // axis_iri silently defaults to an unresolvable value and the staleness /
        // completeness gates can never bind the exemption to the surface it exempts.
        if axis_iri.is_empty() {
            return Err(rubric_err(format!(
                "exemption {iri} names no gmeow:exemptsAxis"
            )));
        }
        if !axes.iter().any(|a| a.iri == axis_iri) {
            return Err(rubric_err(format!(
                "exemption {iri} exempts unknown axis {axis_iri} (no such gmeow:QualityAxis in the rubric)"
            )));
        }
        exemptions.push(Exemption {
            iri,
            axis_iri,
            reason,
            date,
            producer,
        });
    }
    exemptions.sort_by(|a, b| a.iri.cmp(&b.iri));

    // --- Axis floor commitments --------------------------------------------
    // A per-slice, per-axis raise-only measured-score floor. Each of the three
    // required bindings (floorSlice, floorAxis, floorValue) is a hard fail when
    // missing — a floor with no slice, no axis, or no value cannot pin a
    // regression bar, so we never silently default it (.goals no-optionality).
    let floor_slice_p = id(ds, &g("floorSlice"));
    let floor_axis_p = id(ds, &g("floorAxis"));
    let floor_value_p = id(ds, &g("floorValue"));
    let floor_tier_p = id(ds, &g("floorTier"));
    let mut commitments: Vec<(String, AxisFloorCommitment)> = Vec::new();
    // Two AxisFloorCommitment individuals for the same (slice, axis) pair
    // collapse silently in the downstream BTreeMap keyed on that pair
    // (last-writer-wins) — a hard fail here, never a silent skip.
    let mut seen_floor_keys: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for iri in instances_of(ds, &g("AxisFloorCommitment")) {
        let sid = id(ds, &iri)
            .ok_or_else(|| rubric_err(format!("floor commitment {iri} not resolvable")))?;
        let slice = floor_slice_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| rubric_err(format!("floor commitment {iri} has no gmeow:floorSlice")))?;
        let axis = floor_axis_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| rubric_err(format!("floor commitment {iri} has no gmeow:floorAxis")))?;
        // Every floor commitment must name a REAL loaded axis — an unknown
        // gmeow:floorAxis (e.g. a typo) would otherwise load cleanly and then
        // silently never gate anything, leaving the ratchet dead
        // (.goals no-optionality; mirrors the exemptsAxis check above).
        if !axes.iter().any(|a| a.iri == axis) {
            return Err(rubric_err(format!(
                "floor commitment {iri} floors unknown axis {axis} (no such gmeow:QualityAxis in the rubric)"
            )));
        }
        if !seen_floor_keys.insert((slice.clone(), axis.clone())) {
            return Err(rubric_err(format!(
                "duplicate gmeow:AxisFloorCommitment for slice {slice} axis {axis} ({iri}) — \
                 two commitments for the same (slice, axis) pair collapse silently downstream"
            )));
        }
        let floor = floor_value_p
            .and_then(|p| one_lit(ds, sid, p))
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| {
                rubric_err(format!(
                    "floor commitment {iri} has no decimal gmeow:floorValue"
                ))
            })?;
        // A NaN/±inf floor silently defeats the raise-only ratchet comparison
        // (every `>=` against it is vacuous), so hard-fail rather than admit a
        // malformed literal — mirroring the gmeow:thresholdFloor discipline above.
        if !floor.is_finite() {
            return Err(rubric_err(format!(
                "floor commitment {iri} has a non-finite gmeow:floorValue {floor}"
            )));
        }
        commitments.push((iri, AxisFloorCommitment { slice, axis, floor }));
    }
    commitments.sort_by(|a, b| a.0.cmp(&b.0));
    let commitments: Vec<AxisFloorCommitment> = commitments.into_iter().map(|(_, c)| c).collect();

    // --- Slice tier floors -------------------------------------------------
    // A per-slice raise-only roll-up tier floor. Both required bindings
    // (floorSlice, floorTier) hard-fail when missing, same no-optionality rule.
    let mut tier_floors: Vec<(String, SliceTierFloorCommitment)> = Vec::new();
    // Two SliceTierFloor individuals for the same slice collapse silently in
    // the downstream BTreeMap keyed on slice (last-writer-wins) — a hard fail
    // here, never a silent skip.
    let mut seen_tier_floor_slices: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for iri in instances_of(ds, &g("SliceTierFloor")) {
        let sid =
            id(ds, &iri).ok_or_else(|| rubric_err(format!("tier floor {iri} not resolvable")))?;
        let slice = floor_slice_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| rubric_err(format!("tier floor {iri} has no gmeow:floorSlice")))?;
        if !seen_tier_floor_slices.insert(slice.clone()) {
            return Err(rubric_err(format!(
                "duplicate gmeow:SliceTierFloor for slice {slice} ({iri}) — two tier floors for \
                 the same slice collapse silently downstream"
            )));
        }
        let tier = floor_tier_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| rubric_err(format!("tier floor {iri} has no gmeow:floorTier")))?;
        // Every tier floor must name a REAL loaded tier — an unknown
        // gmeow:floorTier would otherwise load cleanly and then silently never
        // gate anything, leaving the ratchet dead (.goals no-optionality;
        // mirrors the axis-threshold tier check above).
        if !tiers.iter().any(|t| t.iri == tier) {
            return Err(rubric_err(format!(
                "tier floor {iri} names unknown tier {tier} (no such gmeow:QualityTier in the rubric ladder)"
            )));
        }
        tier_floors.push((iri, SliceTierFloorCommitment { slice, tier }));
    }
    tier_floors.sort_by(|a, b| a.0.cmp(&b.0));
    let tier_floors: Vec<SliceTierFloorCommitment> =
        tier_floors.into_iter().map(|(_, c)| c).collect();

    Ok(Rubric {
        standard: MeasurementStandard { tiers, axes },
        floors: GovernanceFloors {
            exemptions,
            commitments,
            tier_floors,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal but structurally complete rubric: one tier, one axis with a
    /// threshold, and one exemption whose `gmeow:exemptsAxis` is `exempts_axis`.
    fn rubric_ttl(exempts_axis: &str) -> String {
        format!(
            r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
gmeow:exFoo a gmeow:AxisExemption ;
    gmeow:exemptsAxis {exempts_axis} ;
    gmeow:exemptionReason "unlanded" ;
    gmeow:exemptionDate "2026-07-08" ;
    gmeow:exemptionProducer "FooProducer" .
"#
        )
    }

    fn load(ttl: &str) -> gmeow_errors::Result<Rubric> {
        let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
            .map_err(|e| super::rubric_err(e.to_string()))?;
        let mut b = purrdf::RdfDatasetBuilder::new();
        b.push_dataset(&ds);
        let frozen = b.freeze().map_err(|e| super::rubric_err(e.to_string()))?;
        load_rubric(&frozen)
    }

    #[test]
    fn exemption_naming_a_real_axis_loads() {
        // Control: the same fixture with a valid axis_iri loads cleanly, proving the
        // negative test isolates the axis check (not a malformed fixture).
        let rubric = load(&rubric_ttl("gmeow:axisFoo")).expect("valid rubric loads");
        assert_eq!(rubric.floors.exemptions.len(), 1);
        assert_eq!(
            rubric.floors.exemptions[0].axis_iri,
            format!("{GMEOW_NS}axisFoo")
        );
    }

    #[test]
    fn exemption_with_unknown_axis_hard_fails() {
        // (e) An exemption naming an axis the rubric never loaded is a hard fail.
        let err = load(&rubric_ttl("gmeow:axisNope")).unwrap_err();
        assert!(err.message().contains("exempts unknown axis"), "{err}");
        assert!(
            err.message().contains("axisNope"),
            "names the offending axis: {err}"
        );
    }

    #[test]
    fn non_finite_axis_weight_hard_fails() {
        // A NaN gmeow:axisWeight parses fine as an f64 and would otherwise
        // silently collapse the advisory weight-rank comparator — G4 mandates a
        // hard fail at load time instead.
        let ttl = format!(
            r#"@prefix gmeow: <{GMEOW_NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisWeight "NaN" ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
"#
        );
        let err = load(&ttl).unwrap_err();
        assert!(
            err.message().contains("non-finite gmeow:axisWeight"),
            "{err}"
        );
        assert!(
            err.message().contains("axisFoo"),
            "names the offending axis: {err}"
        );
    }

    #[test]
    fn non_numeric_axis_weight_hard_fails() {
        // A PRESENT but non-numeric gmeow:axisWeight must hard-fail, never
        // silently degrade to the missing-value default of 1.0 (.goals
        // no-optionality) — only an ABSENT predicate earns that default.
        let ttl = format!(
            r#"@prefix gmeow: <{GMEOW_NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisWeight "abc" ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
"#
        );
        let err = load(&ttl).unwrap_err();
        assert!(
            err.message().contains("non-numeric gmeow:axisWeight"),
            "{err}"
        );
        assert!(
            err.message().contains("axisFoo"),
            "names the offending axis: {err}"
        );
    }

    #[test]
    fn non_finite_threshold_floor_hard_fails() {
        // Same defect class for gmeow:thresholdFloor: it feeds the ascending
        // floor sort and the `score + EPSILON >= floor` gate comparisons, so a
        // NaN/inf literal must hard-fail at load rather than silently break
        // tier ordering.
        let ttl = format!(
            r#"@prefix gmeow: <{GMEOW_NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor "inf" .
"#
        );
        let err = load(&ttl).unwrap_err();
        assert!(
            err.message().contains("non-finite gmeow:thresholdFloor"),
            "{err}"
        );
        assert!(
            err.message().contains("thrFoo"),
            "names the offending threshold: {err}"
        );
    }

    /// A structurally complete rubric (one tier, one axis, one threshold) with an
    /// extra `body` block appended — used to exercise the floor-commitment loaders
    /// without duplicating the required ladder/axis scaffolding.
    fn rubric_with(body: &str) -> String {
        format!(
            r#"@prefix gmeow: <{GMEOW_NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
{body}
"#
        )
    }

    #[test]
    fn axis_floor_commitment_loads_with_full_precision() {
        // (a) A well-formed gmeow:AxisFloorCommitment resolves to (slice, axis,
        // floor) carrying the full f64 precision the measured score commits.
        let rubric = load(&rubric_with(
            r#"gmeow:floorFooGrounding a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ;
    gmeow:floorValue 0.9954337899543378 ."#,
        ))
        .expect("valid floor commitment loads");
        assert_eq!(rubric.floors.commitments.len(), 1);
        let c = &rubric.floors.commitments[0];
        assert_eq!(c.slice, format!("{GMEOW_NS}sliceFoo"));
        assert_eq!(c.axis, format!("{GMEOW_NS}axisFoo"));
        assert!((c.floor - 0.995_433_789_954_337_8).abs() < f64::EPSILON);
    }

    #[test]
    fn slice_tier_floor_loads() {
        // (b) A well-formed gmeow:SliceTierFloor resolves to (slice, tier).
        let rubric = load(&rubric_with(
            r#"gmeow:tierFloorFoo a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorTier gmeow:tierRegistered ."#,
        ))
        .expect("valid tier floor loads");
        assert_eq!(rubric.floors.tier_floors.len(), 1);
        let f = &rubric.floors.tier_floors[0];
        assert_eq!(f.slice, format!("{GMEOW_NS}sliceFoo"));
        assert_eq!(f.tier, format!("{GMEOW_NS}tierRegistered"));
    }

    #[test]
    fn axis_floor_commitment_missing_value_hard_fails() {
        // (c) A commitment missing gmeow:floorValue is a hard fail — a floor with
        // no value cannot pin a regression bar, so we never silently skip it.
        let err = load(&rubric_with(
            r#"gmeow:floorFooGrounding a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ."#,
        ))
        .unwrap_err();
        assert!(
            err.message().contains("no decimal gmeow:floorValue"),
            "{err}"
        );
        assert!(
            err.message().contains("floorFooGrounding"),
            "names the offending commitment: {err}"
        );
    }

    #[test]
    fn axis_floor_commitment_missing_axis_hard_fails() {
        // (c) A commitment missing gmeow:floorAxis is likewise a hard fail.
        let err = load(&rubric_with(
            r#"gmeow:floorFooGrounding a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorValue 0.5 ."#,
        ))
        .unwrap_err();
        assert!(err.message().contains("no gmeow:floorAxis"), "{err}");
    }

    #[test]
    fn axis_floor_commitment_unknown_axis_hard_fails() {
        // A floor commitment naming an axis the rubric never loaded (a typo'd
        // gmeow:floorAxis) must hard-fail — otherwise it loads cleanly and then
        // silently never gates anything, leaving the ratchet dead.
        let err = load(&rubric_with(
            r#"gmeow:floorFooGrounding a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisNope ;
    gmeow:floorValue 0.5 ."#,
        ))
        .unwrap_err();
        assert!(err.message().contains("unknown axis"), "{err}");
        assert!(
            err.message().contains("axisNope"),
            "names the offending axis: {err}"
        );
    }

    #[test]
    fn slice_tier_floor_unknown_tier_hard_fails() {
        // A tier floor naming a tier the rubric ladder never loaded (a typo'd
        // gmeow:floorTier) must hard-fail — otherwise it loads cleanly and then
        // silently never gates anything, leaving the ratchet dead.
        let err = load(&rubric_with(
            r#"gmeow:tierFloorFoo a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorTier gmeow:tierNope ."#,
        ))
        .unwrap_err();
        assert!(err.message().contains("unknown tier"), "{err}");
        assert!(
            err.message().contains("tierNope"),
            "names the offending tier: {err}"
        );
    }

    #[test]
    fn slice_tier_floor_missing_tier_hard_fails() {
        // A tier floor missing gmeow:floorTier is a hard fail: it names no rung.
        let err = load(&rubric_with(
            r#"gmeow:tierFloorFoo a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ."#,
        ))
        .unwrap_err();
        assert!(err.message().contains("no gmeow:floorTier"), "{err}");
    }

    #[test]
    fn duplicate_axis_floor_commitment_hard_fails() {
        // Two AxisFloorCommitment individuals naming the SAME (slice, axis) pair
        // collapse silently in the downstream BTreeMap (last-writer-wins) — the
        // loader must hard-fail rather than let one commitment shadow the other.
        let err = load(&rubric_with(
            r#"gmeow:floorFooA a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ;
    gmeow:floorValue 0.5 .
gmeow:floorFooB a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ;
    gmeow:floorValue 0.9 ."#,
        ))
        .unwrap_err();
        assert!(err.message().contains("duplicate"), "{err}");
        assert!(
            err.message().contains("axisFoo"),
            "names the offending axis: {err}"
        );
        assert!(
            err.message().contains("sliceFoo"),
            "names the offending slice: {err}"
        );
    }

    #[test]
    fn distinct_axis_floor_commitments_for_same_slice_load_cleanly() {
        // Positive control: two commitments for the SAME slice but DIFFERENT axes
        // are not duplicates — proves the guard keys on the (slice, axis) pair,
        // not the slice alone.
        let rubric = load(&rubric_with(
            r#"gmeow:axisBar a gmeow:QualityAxis ;
    gmeow:axisProducer "bar" ;
    gmeow:axisDimension gmeow:dimBar ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrBar .
gmeow:thrBar a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
gmeow:floorFooA a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ;
    gmeow:floorValue 0.5 .
gmeow:floorFooB a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisBar ;
    gmeow:floorValue 0.9 ."#,
        ))
        .expect("distinct (slice, axis) commitments load cleanly");
        assert_eq!(rubric.floors.commitments.len(), 2);
    }

    #[test]
    fn duplicate_slice_tier_floor_hard_fails() {
        // Two SliceTierFloor individuals naming the SAME slice collapse silently
        // in the downstream BTreeMap (last-writer-wins) — the loader must
        // hard-fail rather than let one tier floor shadow the other.
        let err = load(&rubric_with(
            r#"gmeow:tierFloorFooA a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorTier gmeow:tierRegistered .
gmeow:tierFloorFooB a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorTier gmeow:tierRegistered ."#,
        ))
        .unwrap_err();
        assert!(err.message().contains("duplicate"), "{err}");
        assert!(
            err.message().contains("sliceFoo"),
            "names the offending slice: {err}"
        );
    }

    const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
}
