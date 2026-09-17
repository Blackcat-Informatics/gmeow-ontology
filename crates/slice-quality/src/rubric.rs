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

use crate::graph::{all_iris, all_lits, g, id, instances_of, label_of, one_iri, one_lit};
use crate::model::{
    Axis, AxisFloorCommitment, CeilingRelocation, ContextScope, CountKind, Exemption,
    GovernanceFloors, MeasurementStandard, ProjectionCeilingCommitment, ProjectionVocabulary,
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

    // --- Projection vocabularies (the guarded set for the ratchet) ----------
    // The ontology-resident guarded-vocabulary registry the projection-ceiling
    // ratchet reads instead of a hardcoded Rust list. Each required binding is a
    // hard fail when missing — a vocabulary with no prefix, namespace, subsumer,
    // count-kind, default ceiling, or preservation cannot drive the counter, so we
    // never silently default one (.goals no-optionality).
    let vocab_prefix_p = id(ds, &g("vocabularyPrefix"));
    let vocab_ns_p = id(ds, &g("vocabularyNamespace"));
    let vocab_subsumed_p = id(ds, &g("vocabularySubsumedBy"));
    let vocab_owner_p = id(ds, &g("vocabularyOwner"));
    let vocab_countkind_p = id(ds, &g("vocabularyCountKind"));
    let vocab_default_p = id(ds, &g("vocabularyDefaultCeiling"));
    let vocab_preservation_p = id(ds, &g("vocabularyPreservation"));
    let vocab_align_p = id(ds, &g("vocabularyAlignmentPredicate"));
    let vocab_countpred_p = id(ds, &g("vocabularyCountPredicate"));
    let mut vocabularies: Vec<(String, ProjectionVocabulary)> = Vec::new();
    // The vocab IRI → prefix map the ceiling loop validates gmeow:ceilingVocabulary
    // against; an unknown vocab reference is a hard fail there, never a silent skip.
    let mut vocab_iri_to_prefix: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    // Two ProjectionVocabulary individuals sharing a prefix collapse in the
    // prefix-keyed downstream maps (the ceiling key is (slice, prefix)) — hard fail.
    let mut seen_vocab_prefixes: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for iri in instances_of(ds, &g("ProjectionVocabulary")) {
        let sid = id(ds, &iri)
            .ok_or_else(|| rubric_err(format!("projection vocabulary {iri} not resolvable")))?;
        let prefix = vocab_prefix_p
            .and_then(|p| one_lit(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "projection vocabulary {iri} has no gmeow:vocabularyPrefix"
                ))
            })?;
        if !seen_vocab_prefixes.insert(prefix.clone()) {
            return Err(rubric_err(format!(
                "duplicate gmeow:ProjectionVocabulary prefix {prefix} ({iri}) — two vocabs \
                 with the same prefix collapse silently in the (slice, prefix) ceiling key"
            )));
        }
        // A vocabulary MUST carry at least one namespace, or the counter can never
        // recognise one of its constructs — hard fail, never a zero-namespace vocab.
        let mut namespaces = vocab_ns_p.map(|p| all_lits(ds, sid, p)).unwrap_or_default();
        namespaces.sort();
        namespaces.dedup();
        if namespaces.is_empty() {
            return Err(rubric_err(format!(
                "projection vocabulary {iri} ({prefix}) has no gmeow:vocabularyNamespace"
            )));
        }
        let subsumed_by = vocab_subsumed_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "projection vocabulary {iri} ({prefix}) has no gmeow:vocabularySubsumedBy"
                ))
            })?;
        // Every guarded vocabulary is owned by exactly one grounding slice (logic:,
        // math:, or lang:) — the only boundary at which its external terms may be
        // authored. A missing owner cannot drive the owner-boundary enforcement, so it
        // is a hard fail, never a silent default (.goals no-optionality).
        let owner = vocab_owner_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "projection vocabulary {iri} ({prefix}) has no gmeow:vocabularyOwner"
                ))
            })?;
        let count_kind_local = vocab_countkind_p
            .and_then(|p| one_lit(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "projection vocabulary {iri} ({prefix}) has no gmeow:vocabularyCountKind"
                ))
            })?;
        // An unknown count-kind (a typo) would otherwise load cleanly and then never
        // count anything — hard fail, mirroring the unknown-axis check on floors.
        let count_kind = CountKind::from_local(&count_kind_local).ok_or_else(|| {
            rubric_err(format!(
                "projection vocabulary {iri} ({prefix}) names unknown gmeow:vocabularyCountKind \
                 {count_kind_local} (expected countKindShape / countKindTypedAxiom / \
                 countKindNonRdfSurface)"
            ))
        })?;
        let default_ceiling = vocab_default_p
            .and_then(|p| one_lit(ds, sid, p))
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| {
                rubric_err(format!(
                    "projection vocabulary {iri} ({prefix}) has no non-negative-integer \
                     gmeow:vocabularyDefaultCeiling"
                ))
            })?;
        let preservation = vocab_preservation_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "projection vocabulary {iri} ({prefix}) has no gmeow:vocabularyPreservation"
                ))
            })?;
        let mut alignment_predicates = vocab_align_p
            .map(|p| all_lits(ds, sid, p))
            .unwrap_or_default();
        alignment_predicates.sort();
        alignment_predicates.dedup();
        let mut counted_predicates = vocab_countpred_p
            .map(|p| all_lits(ds, sid, p))
            .unwrap_or_default();
        counted_predicates.sort();
        counted_predicates.dedup();
        // countKindStructuralAxiom counts only triples whose predicate is in this
        // allowlist; an empty allowlist would count nothing and silently disable the
        // guard — hard fail. Other count kinds ignore the field, and carrying one is a
        // hard fail (a typo that would never take effect).
        if count_kind == CountKind::StructuralAxiom {
            if counted_predicates.is_empty() {
                return Err(rubric_err(format!(
                    "projection vocabulary {iri} ({prefix}) is countKindStructuralAxiom but has \
                     no gmeow:vocabularyCountPredicate allowlist — it would count nothing"
                )));
            }
        } else if !counted_predicates.is_empty() {
            return Err(rubric_err(format!(
                "projection vocabulary {iri} ({prefix}) declares gmeow:vocabularyCountPredicate \
                 but is not countKindStructuralAxiom — the allowlist would never take effect"
            )));
        }
        vocab_iri_to_prefix.insert(iri.clone(), prefix.clone());
        vocabularies.push((
            iri,
            ProjectionVocabulary {
                prefix,
                namespaces,
                subsumed_by,
                owner,
                count_kind,
                default_ceiling,
                preservation,
                alignment_predicates,
                counted_predicates,
            },
        ));
    }
    vocabularies.sort_by(|a, b| a.0.cmp(&b.0));
    let vocabularies: Vec<ProjectionVocabulary> =
        vocabularies.into_iter().map(|(_, v)| v).collect();

    // --- Projection ceiling commitments ------------------------------------
    // A per-(slice, vocabulary) non-increasing residue ceiling — the inverse-polarity
    // twin of gmeow:AxisFloorCommitment (lower-only, not raise-only). Each of the three
    // bindings (ceilingSlice, ceilingVocabulary, ceilingCount) is a hard fail when
    // missing; the vocabulary reference must resolve to a loaded ProjectionVocabulary.
    let ceiling_slice_p = id(ds, &g("ceilingSlice"));
    let ceiling_vocab_p = id(ds, &g("ceilingVocabulary"));
    let ceiling_count_p = id(ds, &g("ceilingCount"));
    let mut ceilings: Vec<(String, ProjectionCeilingCommitment)> = Vec::new();
    // Two ceilings for the same (slice, vocab) collapse in the downstream BTreeMap
    // keyed on that pair (last-writer-wins) — a hard fail here, never a silent skip.
    let mut seen_ceiling_keys: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for iri in instances_of(ds, &g("ProjectionCeilingCommitment")) {
        let sid = id(ds, &iri)
            .ok_or_else(|| rubric_err(format!("ceiling commitment {iri} not resolvable")))?;
        let slice = ceiling_slice_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "ceiling commitment {iri} has no gmeow:ceilingSlice"
                ))
            })?;
        let vocab_iri = ceiling_vocab_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "ceiling commitment {iri} has no gmeow:ceilingVocabulary"
                ))
            })?;
        // Every ceiling must name a REAL loaded ProjectionVocabulary — an unknown
        // reference would otherwise load cleanly and never gate anything (dead
        // ratchet), so hard-fail, mirroring the unknown-axis floor check.
        let vocab_prefix = vocab_iri_to_prefix
            .get(&vocab_iri)
            .cloned()
            .ok_or_else(|| {
                rubric_err(format!(
                    "ceiling commitment {iri} names unknown gmeow:ceilingVocabulary {vocab_iri} \
                 (no such gmeow:ProjectionVocabulary in the registry)"
                ))
            })?;
        if !seen_ceiling_keys.insert((slice.clone(), vocab_prefix.clone())) {
            return Err(rubric_err(format!(
                "duplicate gmeow:ProjectionCeilingCommitment for slice {slice} vocab \
                 {vocab_prefix} ({iri}) — two ceilings for the same (slice, vocab) pair \
                 collapse silently downstream"
            )));
        }
        let count = ceiling_count_p
            .and_then(|p| one_lit(ds, sid, p))
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| {
                rubric_err(format!(
                    "ceiling commitment {iri} has no non-negative-integer gmeow:ceilingCount"
                ))
            })?;
        ceilings.push((
            iri,
            ProjectionCeilingCommitment {
                slice,
                vocab_prefix,
                count,
            },
        ));
    }
    ceilings.sort_by(|a, b| a.0.cmp(&b.0));
    let ceilings: Vec<ProjectionCeilingCommitment> = ceilings.into_iter().map(|(_, c)| c).collect();

    // --- Ceiling relocation declarations -----------------------------------
    // The AUTHORED half of relocation-aware ceiling accounting: a maintainer states
    // that named terms MOVED from one slice to another, and the gate re-projects the
    // base ceiling through that relocation before the lower-only comparison. Every
    // binding is a hard fail when missing — a declaration with no term, no source, no
    // destination, or no date cannot be corroborated against the derived witness, and a
    // silently-defaulted one would be an unbounded permit (.goals no-optionality).
    //
    // The AUTHORITY for the four required-binding checks below (relocationTerm,
    // relocationFromSlice, relocationToSlice, relocationDate) is the EL-safe
    // required-path PAIR authored on `gmeow:CeilingRelocation` in
    // slices/core/slice-quality-rubric/module.ttl: a `logic:subClassOf
    // [ a logic:Restriction ; logic:onProperty P ; logic:allValuesFrom F ]` value
    // restriction PLUS a class-scoped `[ a logic:ClosureEntry ; logic:onClass
    // gmeow:CeilingRelocation ; logic:closureKey P ; logic:closureValue
    // logic:ClosedWorldClosure ]`, which together derive the `sh:minCount 1` this
    // loader's hard fail mirrors — never an un-qualified `logic:minCardinality`,
    // which sits outside the EL fragment. This loader's hard fail is that axiom
    // pair's DERIVED enforcement, not a second, Rust-only source of truth. The
    // cross-node `from_slice == to_slice` rejection and the unknown-vocabulary-
    // reference rejections below are genuinely procedural checks with no declarative
    // cardinality/class/datatype form, so they remain enforced here only.
    let reloc_term_p = id(ds, &g("relocationTerm"));
    let reloc_from_p = id(ds, &g("relocationFromSlice"));
    let reloc_to_p = id(ds, &g("relocationToSlice"));
    let reloc_vocab_p = id(ds, &g("relocationVocabulary"));
    let reloc_date_p = id(ds, &g("relocationDate"));
    let mut relocations: Vec<CeilingRelocation> = Vec::new();
    for iri in instances_of(ds, &g("CeilingRelocation")) {
        let sid = id(ds, &iri)
            .ok_or_else(|| rubric_err(format!("ceiling relocation {iri} not resolvable")))?;
        let mut terms = reloc_term_p
            .map(|p| all_iris(ds, sid, p))
            .unwrap_or_default();
        terms.sort();
        terms.dedup();
        if terms.is_empty() {
            return Err(rubric_err(format!(
                "ceiling relocation {iri} names no gmeow:relocationTerm — a declaration with no \
                 term can never be corroborated by the derived relocation witness"
            )));
        }
        let from_slice = reloc_from_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "ceiling relocation {iri} has no gmeow:relocationFromSlice"
                ))
            })?;
        let to_slice = reloc_to_p
            .and_then(|p| one_iri(ds, sid, p))
            .ok_or_else(|| {
                rubric_err(format!(
                    "ceiling relocation {iri} has no gmeow:relocationToSlice"
                ))
            })?;
        if from_slice == to_slice {
            return Err(rubric_err(format!(
                "ceiling relocation {iri} names the same slice {from_slice} as both source and \
                 destination — a relocation that does not cross a slice boundary moves no residue"
            )));
        }
        // The vocabulary scope is OPTIONAL, but a PRESENT reference must resolve to a
        // real loaded gmeow:ProjectionVocabulary — an unknown IRI would otherwise load
        // cleanly and scope the declaration to nothing (a dead declaration), so it is a
        // hard fail exactly as an unknown gmeow:ceilingVocabulary is.
        let vocabulary = match reloc_vocab_p.and_then(|p| one_iri(ds, sid, p)) {
            None => None,
            Some(vocab_iri) => Some(vocab_iri_to_prefix.get(&vocab_iri).cloned().ok_or_else(
                || {
                    rubric_err(format!(
                        "ceiling relocation {iri} names unknown gmeow:relocationVocabulary \
                         {vocab_iri} (no such gmeow:ProjectionVocabulary in the registry)"
                    ))
                },
            )?),
        };
        let date = reloc_date_p
            .and_then(|p| one_lit(ds, sid, p))
            .unwrap_or_default();
        if date.trim().is_empty() {
            return Err(rubric_err(format!(
                "ceiling relocation {iri} is undated — every relocation declaration carries a \
                 gmeow:relocationDate, exactly as a gmeow:AxisExemption does"
            )));
        }
        relocations.push(CeilingRelocation {
            iri,
            terms,
            from_slice,
            to_slice,
            vocabulary,
            date,
        });
    }
    relocations.sort_by(|a, b| a.iri.cmp(&b.iri));

    Ok(Rubric {
        standard: MeasurementStandard { tiers, axes },
        floors: GovernanceFloors {
            exemptions,
            commitments,
            tier_floors,
            vocabularies,
            ceilings,
            relocations,
        },
    })
}

#[path = "rubric.tests.rs"]
#[cfg(test)]
mod tests;
