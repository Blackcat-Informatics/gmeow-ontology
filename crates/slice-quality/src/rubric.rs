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
    // relocationFromSlice, relocationToSlice, relocationDate all minCardinality 1)
    // is the `gmeow:CeilingRelocation logic:subClassOf [ a logic:Restriction ; ... ]`
    // EL-safe axiom authored on `gmeow:CeilingRelocation` in
    // slices/core/slice-quality-rubric/module.ttl — this loader's hard fail is that
    // axiom's DERIVED enforcement, not a second, Rust-only source of truth. The
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

    // --- Projection vocabulary + ceiling loaders ---------------------------

    /// A well-formed guarded vocabulary the ceiling tests reference.
    const VOCAB_SH: &str = r#"gmeow:projVocab-sh a gmeow:ProjectionVocabulary ;
    gmeow:vocabularyPrefix "sh" ;
    gmeow:vocabularyNamespace "http://www.w3.org/ns/shacl#" ;
    gmeow:vocabularySubsumedBy gmeow:sliceLogic ;
    gmeow:vocabularyOwner gmeow:sliceLogic ;
    gmeow:vocabularyCountKind "countKindShape" ;
    gmeow:vocabularyDefaultCeiling 0 ;
    gmeow:vocabularyPreservation gmeow:presSoundUnder ."#;

    #[test]
    fn projection_vocabulary_and_ceiling_load() {
        // Happy path: a guarded vocab plus a ceiling that references it resolve to
        // the expected (prefix, namespaces, kind) and (slice, vocab-prefix, count).
        let body = format!(
            "{VOCAB_SH}\n\
gmeow:pcc-foo-sh a gmeow:ProjectionCeilingCommitment ;\n\
    gmeow:ceilingSlice gmeow:sliceFoo ;\n\
    gmeow:ceilingVocabulary gmeow:projVocab-sh ;\n\
    gmeow:ceilingCount 7 ."
        );
        let rubric = load(&rubric_with(&body)).expect("valid vocab + ceiling load");
        assert_eq!(rubric.floors.vocabularies.len(), 1);
        let v = &rubric.floors.vocabularies[0];
        assert_eq!(v.prefix, "sh");
        assert_eq!(v.namespaces, vec!["http://www.w3.org/ns/shacl#".to_owned()]);
        assert_eq!(v.count_kind, crate::model::CountKind::Shape);
        assert_eq!(v.default_ceiling, 0);
        assert_eq!(rubric.floors.ceilings.len(), 1);
        let c = &rubric.floors.ceilings[0];
        assert_eq!(c.slice, format!("{GMEOW_NS}sliceFoo"));
        assert_eq!(c.vocab_prefix, "sh");
        assert_eq!(c.count, 7);
    }

    #[test]
    fn ceiling_with_unknown_vocabulary_hard_fails() {
        // A ceiling naming a vocab the registry never loaded is a dead ratchet cell —
        // hard fail, never a silent skip.
        let body = "gmeow:pcc-foo-nope a gmeow:ProjectionCeilingCommitment ;\n\
    gmeow:ceilingSlice gmeow:sliceFoo ;\n\
    gmeow:ceilingVocabulary gmeow:projVocab-nope ;\n\
    gmeow:ceilingCount 1 .";
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(
            err.message().contains("unknown gmeow:ceilingVocabulary"),
            "{err}"
        );
        assert!(err.message().contains("projVocab-nope"), "names it: {err}");
    }

    #[test]
    fn vocabulary_with_unknown_count_kind_hard_fails() {
        let body = r#"gmeow:projVocab-sh a gmeow:ProjectionVocabulary ;
    gmeow:vocabularyPrefix "sh" ;
    gmeow:vocabularyNamespace "http://www.w3.org/ns/shacl#" ;
    gmeow:vocabularySubsumedBy gmeow:sliceLogic ;
    gmeow:vocabularyOwner gmeow:sliceLogic ;
    gmeow:vocabularyCountKind "countKindBogus" ;
    gmeow:vocabularyDefaultCeiling 0 ;
    gmeow:vocabularyPreservation gmeow:presSoundUnder ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(
            err.message().contains("unknown gmeow:vocabularyCountKind"),
            "{err}"
        );
        assert!(err.message().contains("countKindBogus"), "names it: {err}");
    }

    #[test]
    fn vocabulary_with_no_namespace_hard_fails() {
        let body = r#"gmeow:projVocab-sh a gmeow:ProjectionVocabulary ;
    gmeow:vocabularyPrefix "sh" ;
    gmeow:vocabularySubsumedBy gmeow:sliceLogic ;
    gmeow:vocabularyOwner gmeow:sliceLogic ;
    gmeow:vocabularyCountKind "countKindShape" ;
    gmeow:vocabularyDefaultCeiling 0 ;
    gmeow:vocabularyPreservation gmeow:presSoundUnder ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(
            err.message().contains("no gmeow:vocabularyNamespace"),
            "{err}"
        );
    }

    #[test]
    fn duplicate_vocabulary_prefix_hard_fails() {
        let body = format!(
            "{VOCAB_SH}\n\
gmeow:projVocab-sh2 a gmeow:ProjectionVocabulary ;\n\
    gmeow:vocabularyPrefix \"sh\" ;\n\
    gmeow:vocabularyNamespace \"http://example.org/other#\" ;\n\
    gmeow:vocabularySubsumedBy gmeow:sliceLogic ;\n\
    gmeow:vocabularyOwner gmeow:sliceLogic ;\n\
    gmeow:vocabularyCountKind \"countKindShape\" ;\n\
    gmeow:vocabularyDefaultCeiling 0 ;\n\
    gmeow:vocabularyPreservation gmeow:presSoundUnder ."
        );
        let err = load(&rubric_with(&body)).unwrap_err();
        assert!(err.message().contains("duplicate"), "{err}");
        assert!(
            err.message().contains("prefix sh"),
            "names the prefix: {err}"
        );
    }

    #[test]
    fn duplicate_ceiling_for_same_slice_vocab_hard_fails() {
        let body = format!(
            "{VOCAB_SH}\n\
gmeow:pcc-a a gmeow:ProjectionCeilingCommitment ;\n\
    gmeow:ceilingSlice gmeow:sliceFoo ;\n\
    gmeow:ceilingVocabulary gmeow:projVocab-sh ;\n\
    gmeow:ceilingCount 2 .\n\
gmeow:pcc-b a gmeow:ProjectionCeilingCommitment ;\n\
    gmeow:ceilingSlice gmeow:sliceFoo ;\n\
    gmeow:ceilingVocabulary gmeow:projVocab-sh ;\n\
    gmeow:ceilingCount 3 ."
        );
        let err = load(&rubric_with(&body)).unwrap_err();
        assert!(err.message().contains("duplicate"), "{err}");
        assert!(err.message().contains("sliceFoo"), "names the slice: {err}");
    }

    // --- Ceiling relocation loaders -----------------------------------------
    //
    // `gmeow:CeilingRelocation logic:subClassOf [ a logic:Restriction ; ... ]` in
    // `slices/core/slice-quality-rubric/module.ttl` authors the four required-binding
    // axioms (relocationTerm/relocationFromSlice/relocationToSlice/relocationDate all
    // minCardinality 1) as EL-safe declarative axioms; this loader is the DERIVED
    // enforcement of those axioms, not a second, Rust-only source of truth. The
    // `from_slice == to_slice` rejection and the unknown-vocabulary-reference
    // rejection are genuinely procedural checks with no declarative cardinality/
    // class/datatype form and remain enforced here only. These tests exercise the
    // LOADER'S behavior, not that axiom authoring.

    #[test]
    fn relocation_with_no_term_hard_fails() {
        let body = r#"gmeow:reloc-noterm a gmeow:CeilingRelocation ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ;
    gmeow:relocationDate "2026-07-08" ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(
            err.message().contains("names no gmeow:relocationTerm"),
            "{err}"
        );
        assert!(
            err.message().contains("reloc-noterm"),
            "names the offending declaration: {err}"
        );
    }

    #[test]
    fn relocation_with_no_from_slice_hard_fails() {
        let body = r#"gmeow:reloc-nofrom a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ;
    gmeow:relocationDate "2026-07-08" ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(
            err.message().contains("has no gmeow:relocationFromSlice"),
            "{err}"
        );
        assert!(
            err.message().contains("reloc-nofrom"),
            "names the offending declaration: {err}"
        );
    }

    #[test]
    fn relocation_with_no_to_slice_hard_fails() {
        let body = r#"gmeow:reloc-noto a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationDate "2026-07-08" ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(
            err.message().contains("has no gmeow:relocationToSlice"),
            "{err}"
        );
        assert!(
            err.message().contains("reloc-noto"),
            "names the offending declaration: {err}"
        );
    }

    #[test]
    fn relocation_naming_the_same_slice_twice_hard_fails() {
        let body = r#"gmeow:reloc-same a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceFoo ;
    gmeow:relocationDate "2026-07-08" ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(
            err.message().contains("as both source and destination"),
            "{err}"
        );
        assert!(
            err.message().contains("sliceFoo"),
            "names the offending slice: {err}"
        );
    }

    #[test]
    fn relocation_with_unknown_vocabulary_hard_fails() {
        let body = r#"gmeow:reloc-badvocab a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ;
    gmeow:relocationVocabulary gmeow:projVocab-nope ;
    gmeow:relocationDate "2026-07-08" ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(
            err.message().contains("unknown gmeow:relocationVocabulary"),
            "{err}"
        );
        assert!(err.message().contains("projVocab-nope"), "names it: {err}");
    }

    #[test]
    fn relocation_with_no_date_hard_fails() {
        let body = r#"gmeow:reloc-nodate a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(err.message().contains("undated"), "{err}");
        assert!(
            err.message().contains("reloc-nodate"),
            "names the offending declaration: {err}"
        );
    }

    #[test]
    fn relocation_with_blank_date_hard_fails() {
        // A PRESENT but whitespace-only gmeow:relocationDate must fail exactly like a
        // missing one (the loader's `date.trim().is_empty()` check) — a blank date
        // dates nothing, and a regression that dropped the `.trim()` would let this
        // one slip through as "present" while `relocation_with_no_date_hard_fails`
        // above stays green.
        let body = r#"gmeow:reloc-blankdate a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ;
    gmeow:relocationDate "   " ."#;
        let err = load(&rubric_with(body)).unwrap_err();
        assert!(err.message().contains("undated"), "{err}");
        assert!(
            err.message().contains("reloc-blankdate"),
            "names the offending declaration: {err}"
        );
    }

    #[test]
    fn well_formed_relocation_loads_with_sorted_deduped_terms_and_resolved_vocabulary() {
        // (a) A well-formed gmeow:CeilingRelocation: repeated and out-of-order
        // gmeow:relocationTerm values collapse to a SORTED, DEDUPED `terms` vec, and
        // the optional gmeow:relocationVocabulary resolves to the registered prefix.
        let body = format!(
            "{VOCAB_SH}\n\
gmeow:reloc-good a gmeow:CeilingRelocation ;\n\
    gmeow:relocationTerm gmeow:termB, gmeow:termA, gmeow:termA ;\n\
    gmeow:relocationFromSlice gmeow:sliceFoo ;\n\
    gmeow:relocationToSlice gmeow:sliceBar ;\n\
    gmeow:relocationVocabulary gmeow:projVocab-sh ;\n\
    gmeow:relocationDate \"2026-07-08\" ."
        );
        let rubric = load(&rubric_with(&body)).expect("valid relocation loads");
        assert_eq!(rubric.floors.relocations.len(), 1);
        let r = &rubric.floors.relocations[0];
        assert_eq!(
            r.terms,
            vec![format!("{GMEOW_NS}termA"), format!("{GMEOW_NS}termB")],
            "terms are sorted and deduped: {r:?}"
        );
        assert_eq!(r.from_slice, format!("{GMEOW_NS}sliceFoo"));
        assert_eq!(r.to_slice, format!("{GMEOW_NS}sliceBar"));
        assert_eq!(r.vocabulary, Some("sh".to_owned()));
        assert_eq!(r.date, "2026-07-08");
    }

    use gmeow_ns::GMEOW_NS;
}
