// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit, ontology-authored selection of a quality operation's measurements.
//! Exclusions are decisions in the selected model, never inferred from absent inputs.

use std::collections::BTreeMap;

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef};
use serde::Serialize;

use crate::graph::{g, instances_of};
use crate::model::MeasurementStandard;
use crate::rubric::rubric_err as invalid;

const EXTERNAL_INPUT_DOMAIN: purrdf_hash::Domain =
    purrdf_hash::Domain::new(b"gmeow-quality-external-input-v1");

mod imports;

/// An authored disposition for one measurement axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum Applicability {
    /// The selected operation requires this measurement and all of its inputs.
    Measured,
    /// The corpus shape deliberately falls outside this axis's declared domain.
    NotApplicable { reason: String },
    /// A known requested measurement has no supported implementation for this profile.
    Unsupported { reason: String },
}

/// A profile resolved against the exact measurement standard before evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct QualityProfile {
    /// Canonical identity of the selected authored profile.
    pub iri: String,
    /// Unique consumer selector, independent of display labels.
    pub key: String,
    /// Known tier each measured axis must reach to pass.
    pub target_tier: String,
    /// Every rubric axis, including explicit exclusions and unsupported measurements.
    pub axes: BTreeMap<String, Applicability>,
}

/// One reported outcome; an excluded axis never carries an invented numeric score.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum AxisOutcome {
    Passed { score: f64, tier: String },
    Failed { score: f64, tier: String },
    NotApplicable { reason: String },
    Unsupported { reason: String },
}

impl AxisOutcome {
    /// Consumer-facing disposition including the measurement or explicit reason.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::Passed { score, tier } => format!("passed: {score:.4}, {tier}"),
            Self::Failed { score, tier } => format!("failed: {score:.4}, {tier}"),
            Self::NotApplicable { reason } => format!("not applicable: {reason}"),
            Self::Unsupported { reason } => format!("unsupported: {reason}"),
        }
    }
}

/// The identities and complete denominator of one selected quality operation.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileAssessment {
    /// Fully resolved selection contract used for this operation.
    pub profile: QualityProfile,
    /// Digest of the normalized profile contract.
    pub profile_identity: String,
    /// Digest of the exact embedded rubric artifact.
    pub rubric_identity: String,
    /// Digest of every supplied path and its complete bytes.
    pub corpus_identity: String,
    /// Deterministically ordered, admitted transitive import owners.
    pub declared_imports: Vec<String>,
    /// Entire rubric roster, before applicability selection.
    pub total_axes: usize,
    /// Axes actually measured; the numeric roll-up denominator.
    pub measured_axes: usize,
    /// Axes deliberately outside the selected corpus contract.
    pub not_applicable_axes: usize,
    /// Requested axes outside the supported implementation fragment.
    pub unsupported_axes: usize,
    /// False whenever any selected measurement is unsupported.
    pub complete: bool,
    /// Exactly one honest disposition for every rubric axis.
    pub outcomes: BTreeMap<String, AxisOutcome>,
}

impl ProfileAssessment {
    /// Bind every axis disposition to the actual grades and exact input file map.
    ///
    /// # Errors
    /// Refuses missing/non-finite measurements or an unknown target tier. Unsupported
    /// axes make the operation explicitly incomplete, never silently improve its result.
    pub fn new(
        profile: &QualityProfile,
        rubric_identity: &str,
        files: &BTreeMap<String, Vec<u8>>,
        declared_imports: Vec<String>,
        report: &crate::report::SliceReport,
    ) -> gmeow_errors::Result<Self> {
        let target = report
            .standard
            .tier(&profile.target_tier)
            .ok_or_else(|| invalid("selected quality target tier disappeared"))?;
        let mut outcomes = BTreeMap::new();
        let mut measured_axes = 0;
        let mut not_applicable_axes = 0;
        let mut unsupported_axes = 0;
        for (axis, applicability) in &profile.axes {
            let outcome = match applicability {
                Applicability::Measured => {
                    let grade = report
                        .assessment
                        .grades
                        .iter()
                        .find(|grade| &grade.axis_iri == axis)
                        .ok_or_else(|| {
                            invalid(format!("selected axis {axis} has no measurement"))
                        })?;
                    if !grade.score.is_finite() {
                        return Err(invalid(format!(
                            "selected axis {axis} has a non-finite score"
                        )));
                    }
                    measured_axes += 1;
                    if grade.tier.rank >= target.rank {
                        AxisOutcome::Passed {
                            score: grade.score,
                            tier: grade.tier.iri.clone(),
                        }
                    } else {
                        AxisOutcome::Failed {
                            score: grade.score,
                            tier: grade.tier.iri.clone(),
                        }
                    }
                }
                Applicability::NotApplicable { reason } => {
                    not_applicable_axes += 1;
                    AxisOutcome::NotApplicable {
                        reason: reason.clone(),
                    }
                }
                Applicability::Unsupported { reason } => {
                    unsupported_axes += 1;
                    AxisOutcome::Unsupported {
                        reason: reason.clone(),
                    }
                }
            };
            outcomes.insert(axis.clone(), outcome);
        }
        let mut input = purrdf_hash::blake3::Hasher::new();
        input.update(EXTERNAL_INPUT_DOMAIN.as_bytes());
        for (path, bytes) in files {
            for field in [path.as_bytes(), bytes.as_slice()] {
                purrdf_hash::frame::frame_le_with(field, &mut |part| {
                    input.update(part);
                });
            }
        }
        let profile_bytes = serde_json::to_vec(profile)
            .map_err(|error| invalid(format!("encode selected profile identity: {error}")))?;
        Ok(Self {
            profile: profile.clone(),
            profile_identity: format!(
                "blake3:{}",
                purrdf_hash::hex::encode(purrdf_hash::blake3::hash(&profile_bytes).as_bytes())
            ),
            rubric_identity: rubric_identity.to_owned(),
            corpus_identity: format!(
                "blake3:{}",
                purrdf_hash::hex::encode(input.finalize().as_bytes())
            ),
            declared_imports,
            total_axes: outcomes.len(),
            measured_axes,
            not_applicable_axes,
            unsupported_axes,
            complete: unsupported_axes == 0,
            outcomes,
        })
    }
}

/// Prepare only supplied corpus files and the transitive declared import owners.
pub(crate) fn prepare_inputs(
    files: &BTreeMap<String, Vec<u8>>,
    bundle: &RdfDataset,
) -> gmeow_errors::Result<(std::sync::Arc<RdfDataset>, Vec<String>)> {
    let manifest = files
        .get("manifest.ttl")
        .ok_or_else(|| invalid("external quality operation requires manifest.ttl"))?;
    let manifest = crate::dataset_from_documents(&[("manifest.ttl", manifest)])?;
    let data = crate::dataset_from_documents(&crate::report::slice_ttl_documents(files))?;
    let import_documents: Vec<_> = files
        .iter()
        .filter(|(path, _)| path.starts_with("imports/") && path.ends_with(".ttl"))
        .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
        .collect();
    let supplied_imports = crate::dataset_from_documents(&import_documents)?;
    imports::assemble(&data, &manifest, &[bundle, &supplied_imports])
}

fn values(ds: &RdfDataset, subject: TermId, property: &str) -> Vec<TermId> {
    let Some(predicate) = ds.term_id_by_iri(&g(property)) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Any)
        .map(|quad| quad.o)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn scalar<'a>(
    ds: &'a RdfDataset,
    subject: TermId,
    property: &str,
    iri: bool,
) -> gmeow_errors::Result<&'a str> {
    let objects = values(ds, subject, property);
    let [object] = objects.as_slice() else {
        return Err(invalid(format!(
            "quality profile field gmeow:{property} requires exactly one value; found {}",
            objects.len()
        )));
    };
    match (iri, ds.resolve(*object)) {
        (true, TermRef::Iri(value)) => Ok(value),
        (
            false,
            TermRef::Literal {
                lexical,
                datatype,
                language: None,
                direction: None,
            },
        ) if !lexical.trim().is_empty()
            && matches!(
                ds.resolve(datatype),
                TermRef::Iri("http://www.w3.org/2001/XMLSchema#string")
            ) =>
        {
            Ok(lexical)
        }
        _ => Err(invalid(format!(
            "quality profile field gmeow:{property} requires {}",
            if iri { "an IRI" } else { "a non-empty literal" }
        ))),
    }
}

fn validate_fields(
    ds: &RdfDataset,
    subject: TermId,
    prefix: &str,
    allowed: &[&str],
) -> gmeow_errors::Result<()> {
    for quad in ds.quads_for_pattern(Some(subject), None, None, GraphMatch::Any) {
        if let TermRef::Iri(predicate) = ds.resolve(quad.p)
            && let Some(local) = predicate.strip_prefix(crate::model::GMEOW)
            && local.starts_with(prefix)
            && !allowed.contains(&local)
        {
            return Err(invalid(format!(
                "unsupported quality profile field <{predicate}>"
            )));
        }
    }
    Ok(())
}

impl QualityProfile {
    /// Admit a uniquely named profile and its complete axis roster from canonical data.
    ///
    /// # Errors
    /// Rejects absent/ambiguous profiles, scalar multiplicity, unknown axes or tiers,
    /// conflicting dispositions, unsupported status values and missing reasons.
    pub fn load(
        ds: &RdfDataset,
        standard: &MeasurementStandard,
        key: &str,
    ) -> gmeow_errors::Result<Self> {
        let mut selected = None;
        for iri in instances_of(ds, &g("QualityMeasurementProfile")) {
            let subject = ds
                .term_id_by_iri(&iri)
                .ok_or_else(|| invalid("unresolvable quality profile"))?;
            if scalar(ds, subject, "qualityProfileKey", false)? != key {
                continue;
            }
            if selected.is_some() {
                return Err(invalid(format!("ambiguous quality profile {key:?}")));
            }
            selected = Some((iri, subject));
        }
        let (iri, subject) =
            selected.ok_or_else(|| invalid(format!("unknown quality profile {key:?}")))?;
        validate_fields(
            ds,
            subject,
            "qualityProfile",
            &[
                "qualityProfileKey",
                "qualityProfileTargetTier",
                "qualityProfileDefaultApplicability",
                "qualityProfileApplicability",
            ],
        )?;
        let target_tier = scalar(ds, subject, "qualityProfileTargetTier", true)?.to_owned();
        if standard.tiers.iter().all(|tier| tier.iri != target_tier) {
            return Err(invalid(format!(
                "quality profile {iri} names unknown target tier {target_tier}"
            )));
        }
        if scalar(ds, subject, "qualityProfileDefaultApplicability", true)? != g("qualityMeasured")
        {
            return Err(invalid(format!(
                "quality profile {iri} must explicitly require measurement by default"
            )));
        }
        let mut axes: BTreeMap<_, _> = standard
            .axes
            .iter()
            .map(|axis| (axis.iri.clone(), Applicability::Measured))
            .collect();
        let mut overridden = std::collections::BTreeSet::new();
        for binding in values(ds, subject, "qualityProfileApplicability") {
            validate_fields(
                ds,
                binding,
                "qualityApplicability",
                &[
                    "qualityApplicabilityAxis",
                    "qualityApplicabilityStatus",
                    "qualityApplicabilityReason",
                ],
            )?;
            let typing = ds
                .term_id_by_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
                .ok_or_else(|| invalid("applicability binding has no type"))?;
            if !ds.quads_for_pattern(Some(binding), Some(typing), None, GraphMatch::Any)
                .any(|quad| matches!(ds.resolve(quad.o), TermRef::Iri(iri) if iri == g("QualityAxisApplicability")))
            {
                return Err(invalid("quality applicability binding must be typed gmeow:QualityAxisApplicability"));
            }
            let axis = scalar(ds, binding, "qualityApplicabilityAxis", true)?.to_owned();
            if !axes.contains_key(&axis) {
                return Err(invalid(format!(
                    "quality profile {iri} names unknown axis {axis}"
                )));
            }
            if !overridden.insert(axis.clone()) {
                return Err(invalid(format!(
                    "quality profile {iri} repeats applicability for {axis}"
                )));
            }
            let status = scalar(ds, binding, "qualityApplicabilityStatus", true)?;
            let reason = scalar(ds, binding, "qualityApplicabilityReason", false)?.to_owned();
            let applicability = if status == g("qualityNotApplicable") {
                Applicability::NotApplicable { reason }
            } else if status == g("qualityUnsupported") {
                Applicability::Unsupported { reason }
            } else {
                return Err(invalid(format!(
                    "quality profile {iri} has invalid exclusion status {status}"
                )));
            };
            axes.insert(axis, applicability);
        }
        Ok(Self {
            iri,
            key: key.to_owned(),
            target_tier,
            axes,
        })
    }

    /// Select mandatory measurements without changing thresholds or the tier lattice.
    ///
    /// # Errors
    /// A profile excluding every measurement cannot manufacture a vacuous roll-up.
    pub fn measurement_standard(
        &self,
        standard: &MeasurementStandard,
    ) -> gmeow_errors::Result<MeasurementStandard> {
        let axes: Vec<_> = standard
            .axes
            .iter()
            .filter(|axis| self.axes.get(&axis.iri) == Some(&Applicability::Measured))
            .cloned()
            .collect();
        if axes.is_empty() {
            return Err(invalid(format!(
                "quality profile {} selects no measurable axes",
                self.iri
            )));
        }
        Ok(MeasurementStandard {
            tiers: standard.tiers.clone(),
            axes,
        })
    }
}

#[cfg(test)]
#[path = "profile.tests.rs"]
mod tests;
