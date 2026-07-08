// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The rubric loader: read the ontology-resident rubric out of an RDF dataset.
//!
//! The rubric is authored as `gmeow:Profile`/`gmeow:QualityAxis`/… individuals in
//! `slices/core/slice-quality-rubric/module.ttl`. This module resolves that data
//! into the [`Rubric`] the scorer consumes — so tuning a threshold or minting an
//! axis is a slice edit, never a code change. A malformed rubric (an axis with no
//! producer, a threshold with no tier) is a hard error, never a silent skip.

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

use crate::model::{Axis, ContextScope, Exemption, GMEOW, Rubric, Threshold, Tier};

/// Fully-qualify a `gmeow:` local name.
fn g(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Resolve an IRI to a term id, if present in the dataset.
fn id(ds: &RdfDataset, iri: &str) -> Option<purrdf::TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// The single object IRI for `(subject, predicate)`, if exactly one IRI object.
fn one_iri(ds: &RdfDataset, subject: purrdf::TermId, pred: purrdf::TermId) -> Option<String> {
    ds.quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .find_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
}

/// The single literal lexical for `(subject, predicate)`, if any literal object.
fn one_lit(ds: &RdfDataset, subject: purrdf::TermId, pred: purrdf::TermId) -> Option<String> {
    ds.quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .find_map(|q| match ds.resolve(q.o) {
            TermRef::Literal { lexical, .. } => Some(lexical.to_owned()),
            _ => None,
        })
}

/// All object IRIs for `(subject, predicate)`.
fn all_iris(ds: &RdfDataset, subject: purrdf::TermId, pred: purrdf::TermId) -> Vec<String> {
    ds.quads_for_pattern(Some(subject), Some(pred), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect()
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn read_instances(ds: &RdfDataset, class_iri: &str) -> Vec<String> {
    let (Some(type_id), Some(class_id)) = (id(ds, RDF_TYPE), id(ds, class_iri)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = ds
        .quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.s) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn label_of(ds: &RdfDataset, subject: purrdf::TermId) -> String {
    id(ds, "http://www.w3.org/2000/01/rdf-schema#label")
        .and_then(|p| one_lit(ds, subject, p))
        .unwrap_or_default()
}

/// Load the whole rubric from a dataset that contains the rubric module graph.
///
/// # Errors
/// Returns a message if the rubric is structurally incomplete — no tier ladder,
/// an axis missing its producer/dimension/scope, or a threshold naming an
/// unknown tier. A missing required binding is a hard fail, never papered over.
pub fn load_rubric(ds: &RdfDataset) -> Result<Rubric, String> {
    // --- Tiers -------------------------------------------------------------
    let rank_p = id(ds, &g("tierRank"));
    let mut tiers: Vec<Tier> = Vec::new();
    for iri in read_instances(ds, &g("QualityTier")) {
        let sid = id(ds, &iri).ok_or_else(|| format!("tier {iri} not resolvable"))?;
        let rank = rank_p
            .and_then(|p| one_lit(ds, sid, p))
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| format!("tier {iri} has no integer gmeow:tierRank"))?;
        tiers.push(Tier {
            iri,
            label: label_of(ds, sid),
            rank,
        });
    }
    if tiers.is_empty() {
        return Err("rubric has no gmeow:QualityTier ladder".to_owned());
    }
    tiers.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

    // --- Thresholds (resolved inline per axis) -----------------------------
    let thr_tier_p = id(ds, &g("thresholdTier"));
    let thr_floor_p = id(ds, &g("thresholdFloor"));
    let load_threshold = |thr_iri: &str| -> Result<Threshold, String> {
        let tid = id(ds, thr_iri).ok_or_else(|| format!("threshold {thr_iri} not resolvable"))?;
        let tier_iri = thr_tier_p
            .and_then(|p| one_iri(ds, tid, p))
            .ok_or_else(|| format!("threshold {thr_iri} has no gmeow:thresholdTier"))?;
        let floor = thr_floor_p
            .and_then(|p| one_lit(ds, tid, p))
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or_else(|| format!("threshold {thr_iri} has no decimal gmeow:thresholdFloor"))?;
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
    for iri in read_instances(ds, &g("QualityAxis")) {
        let sid = id(ds, &iri).ok_or_else(|| format!("axis {iri} not resolvable"))?;
        let producer = producer_p
            .and_then(|p| one_lit(ds, sid, p))
            .ok_or_else(|| format!("axis {iri} has no gmeow:axisProducer"))?;
        let dimension_iri = dimension_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| format!("axis {iri} has no gmeow:axisDimension"))?;
        let scope_iri = scope_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| format!("axis {iri} has no gmeow:axisContextScope"))?;
        let scope = ContextScope::from_local(scope_iri.rsplit(['/', '#']).next().unwrap_or(""))
            .ok_or_else(|| format!("axis {iri} names unknown scope {scope_iri}"))?;
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
            return Err(format!("axis {iri} has no gmeow:axisThreshold"));
        }
        // Validate every threshold names a real tier.
        for t in &thresholds {
            if !tiers.iter().any(|tier| tier.iri == t.tier_iri) {
                return Err(format!(
                    "axis {iri} threshold names unknown tier {}",
                    t.tier_iri
                ));
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
        return Err("rubric has no gmeow:QualityAxis individuals".to_owned());
    }
    axes.sort_by(|a, b| a.iri.cmp(&b.iri));

    // --- Exemptions --------------------------------------------------------
    let exempts_p = id(ds, &g("exemptsAxis"));
    let reason_p = id(ds, &g("exemptionReason"));
    let date_p = id(ds, &g("exemptionDate"));
    let exproducer_p = id(ds, &g("exemptionProducer"));
    let mut exemptions: Vec<Exemption> = Vec::new();
    for iri in read_instances(ds, &g("AxisExemption")) {
        let sid = id(ds, &iri).ok_or_else(|| format!("exemption {iri} not resolvable"))?;
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
            return Err(format!(
                "exemption {iri} must carry a dated producer symbol"
            ));
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
