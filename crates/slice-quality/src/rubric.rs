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
use crate::model::{Axis, ContextScope, Exemption, Rubric, Threshold, Tier};

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
        let weight = weight_p
            .and_then(|p| one_lit(ds, sid, p))
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
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

    Ok(Rubric {
        tiers,
        axes,
        exemptions,
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
        assert_eq!(rubric.exemptions.len(), 1);
        assert_eq!(rubric.exemptions[0].axis_iri, format!("{GMEOW_NS}axisFoo"));
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

    const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
}
