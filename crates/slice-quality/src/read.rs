// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read the RECORDED quality-assessment corpus back into the grade vectors that
//! produced it — the inverse of [`crate::report::SliceReport::to_gmeow_rdf`].
//!
//! The pipeline scores every slice once at the DAG root and projects the result to
//! `graph/quality-assessment` (on disk, `generated/quality/gmeow.quality-assessment.nt`).
//! Until now nothing read it back, so every consumer that needed a grade re-ran the
//! whole sweep. This module closes that loop: a consumer LOADS the recorded grades
//! instead of recomputing them.
//!
//! Three properties make that substitution safe, and all three are enforced here
//! rather than assumed:
//!
//! * **Losslessness.** Every field of an [`AxisGrade`] is recoverable EXACTLY. The
//!   axis is a first-class `gmeow:assessmentAxis` (never inferred from the many-to-one
//!   `gmeow:qualityDimension`, and never from the lowercased subject slug); the score
//!   is the shortest round-tripping `f64` lexical, so it reparses bit-identically; the
//!   tier is an IRI resolved against the SAME rubric ladder the scorer graded with.
//!   `crate::report::tests::recorded_grades_round_trip_exactly` pins this.
//! * **Completeness.** A slice missing from the record, or a slice missing an axis the
//!   rubric declares, is a HARD FAIL. There is no partial reading: a truncated record
//!   can never be mistaken for a passing one.
//! * **Freshness.** The corpus carries a `gmeow:versionFingerprint` over the
//!   canonicalized authored source set it scored. [`verify_fresh`] recomputes it and
//!   hard-fails on absence or mismatch, so a stale record is an error rather than a
//!   silently-accepted pass.

use std::collections::BTreeMap;
use std::path::Path;

use purrdf::RdfDataset;

use crate::error;
use crate::graph;
use crate::model::{AxisGrade, MeasurementStandard, SliceAssessment};
use crate::report::{SLICE_QUALITY_GRAPH, VERSION_FINGERPRINT};

/// The committed on-disk projection of the quality-assessment corpus, relative to the
/// repository root. This is the SAME path `crates/pipeline`'s fanout writes from the
/// `graph/fanout/…` reconstruction graph, so reading it and reading the bundle graph
/// are the same fact.
pub const RECORDED_CORPUS_PATH: &str = "generated/quality/gmeow.quality-assessment.nt";

const GMEOW: &str = crate::model::GMEOW;
const MATH: &str = crate::model::MATH;

/// The recorded corpus: every scored slice's assessment, plus the freshness witness.
#[derive(Debug, Clone)]
pub struct RecordedCorpus {
    /// The `gmeow:versionFingerprint` the sweep stamped on the corpus — the
    /// canonicalized digest of every authored file it scored.
    pub fingerprint: String,
    /// Each scored slice's assessment, keyed by slice IRI.
    pub by_slice: BTreeMap<String, SliceAssessment>,
}

impl RecordedCorpus {
    /// The recorded assessment of `slice_iri`, or a hard error naming the gap.
    ///
    /// # Errors
    /// If the corpus carries no assessment for `slice_iri` — a slice that exists on
    /// disk but is absent from the record means the record does not describe this
    /// tree, which is never a reason to skip the slice.
    pub fn assessment(&self, slice_iri: &str) -> gmeow_errors::Result<&SliceAssessment> {
        self.by_slice.get(slice_iri).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(error::Record {
                detail: format!(
                    "the recorded quality-assessment corpus carries no assessment for {slice_iri} \
                     — the record does not describe this working tree; regenerate it"
                ),
            })
        })
    }

    /// Hard-fail unless the corpus was produced from the authored sources currently
    /// under `repo_root`.
    ///
    /// This is the whole warrant for reading a record instead of re-scoring: the
    /// recomputation is skipped only once the record is PROVEN current. A mismatch is
    /// an error naming both digests, never a fall-back to scoring (which would hide
    /// the fact that a consumer was about to trust a stale artifact) and never a pass.
    ///
    /// # Errors
    /// If the recorded fingerprint differs from the digest of the current sources, or
    /// if any scored source file cannot be read.
    pub fn verify_fresh(&self, repo_root: &Path) -> gmeow_errors::Result<()> {
        let live = crate::scored_input_fingerprint(repo_root)?;
        if live == self.fingerprint {
            return Ok(());
        }
        Err(gmeow_errors::Diag::of_kind(error::Record {
            detail: format!(
                "the recorded quality-assessment corpus at {RECORDED_CORPUS_PATH} is STALE: it \
                 was produced from sources fingerprinting {recorded}, but the authored sources \
                 under {root} now fingerprint {live}. Regenerate it (`make regen`) — a stale \
                 record is never read as current.",
                recorded = self.fingerprint,
                root = repo_root.display(),
            ),
        }))
    }
}

/// Read the recorded corpus from `repo_root`'s committed projection and resolve every
/// tier IRI against `standard`'s ladder.
///
/// # Errors
/// If the projection is absent or unparseable, if any assessment is structurally
/// incomplete (no assessed slice, no score, no tier, or a tier outside the ladder), or
/// if any slice's grade vector does not cover exactly the axes `standard` declares.
pub fn read_recorded_corpus(
    repo_root: &Path,
    standard: &MeasurementStandard,
) -> gmeow_errors::Result<RecordedCorpus> {
    let path = repo_root.join(RECORDED_CORPUS_PATH);
    let bytes = std::fs::read(&path).map_err(|e| {
        gmeow_errors::Diag::of_kind(error::Record {
            detail: format!(
                "cannot read the recorded quality-assessment corpus at {}: {e}. It is a \
                 projection of gmeow.gts — regenerate it (`make regen`); its absence is never \
                 a reason to skip the check",
                path.display()
            ),
        })
    })?;
    read_recorded_corpus_bytes(&bytes, standard)
}

/// [`read_recorded_corpus`] over in-memory projection bytes — the entry point the
/// round-trip proof drives, so the test exercises the identical parse the gate does.
///
/// # Errors
/// As [`read_recorded_corpus`].
pub fn read_recorded_corpus_bytes(
    bytes: &[u8],
    standard: &MeasurementStandard,
) -> gmeow_errors::Result<RecordedCorpus> {
    // N-Quads, whose grammar makes the graph label optional — so ONE parser reads both
    // the raw N-Quads the emitter produces in memory (graph-labelled) and the flat
    // `.nt` projection the fanout writes (unlabelled). The reader keys entirely off
    // subject/predicate/object, so which of the two it was handed never matters.
    let ds = purrdf::parse_dataset(bytes, "application/n-quads", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(error::Record {
            detail: format!("the recorded quality-assessment corpus does not parse: {e}"),
        })
    })?;
    let fingerprint = read_fingerprint(&ds)?;
    let by_slice = read_assessments(&ds, standard)?;
    Ok(RecordedCorpus {
        fingerprint,
        by_slice,
    })
}

/// The corpus-level `gmeow:versionFingerprint`, or a hard error.
fn read_fingerprint(ds: &RdfDataset) -> gmeow_errors::Result<String> {
    let missing = || {
        gmeow_errors::Diag::of_kind(error::Record {
            detail: format!(
                "the recorded quality-assessment corpus carries no gmeow:versionFingerprint on \
                 <{SLICE_QUALITY_GRAPH}>, so its freshness cannot be proven — it must not be \
                 read as current. Regenerate it (`make regen`)"
            ),
        })
    };
    let (Some(subject), Some(pred)) = (
        graph::id(ds, SLICE_QUALITY_GRAPH),
        graph::id(ds, VERSION_FINGERPRINT),
    ) else {
        return Err(missing());
    };
    graph::one_lit(ds, subject, pred).ok_or_else(missing)
}

/// Every recorded per-slice assessment, keyed by slice IRI.
fn read_assessments(
    ds: &RdfDataset,
    standard: &MeasurementStandard,
) -> gmeow_errors::Result<BTreeMap<String, SliceAssessment>> {
    let assessment_class = format!("{GMEOW}QualityAssessment");
    let assessed_entity = graph::id(ds, &format!("{GMEOW}assessedEntity"));
    let assessment_axis = graph::id(ds, &format!("{GMEOW}assessmentAxis"));
    let observation_result = graph::id(ds, &format!("{GMEOW}observationResult"));
    let quantity_value = graph::id(ds, &format!("{MATH}quantityValue"));

    // Per slice: the per-axis grades collected so far, and the roll-up tier IRI.
    let mut grades: BTreeMap<String, Vec<AxisGrade>> = BTreeMap::new();
    let mut rollups: BTreeMap<String, String> = BTreeMap::new();

    for subject_iri in graph::instances_of(ds, &assessment_class) {
        let Some(subject) = graph::id(ds, &subject_iri) else {
            continue;
        };
        let slice = assessed_entity
            .and_then(|p| graph::one_iri(ds, subject, p))
            .ok_or_else(|| {
                record_err(format!(
                    "the recorded assessment <{subject_iri}> names no gmeow:assessedEntity"
                ))
            })?;
        let results = observation_result
            .map(|p| graph::all_iris(ds, subject, p))
            .unwrap_or_default();

        // The roll-up assessment is exactly the one carrying NO gmeow:assessmentAxis
        // (it spans every axis and names none); its sole result is the meet tier.
        let Some(axis_iri) = assessment_axis.and_then(|p| graph::one_iri(ds, subject, p)) else {
            let tier_iri = match results.as_slice() {
                [only] => only.clone(),
                _ => {
                    return Err(record_err(format!(
                        "the recorded roll-up assessment <{subject_iri}> must carry exactly one \
                         gmeow:observationResult (the meet tier), found {}",
                        results.len()
                    )));
                }
            };
            if rollups.insert(slice.clone(), tier_iri).is_some() {
                return Err(record_err(format!(
                    "the recorded corpus carries two roll-up assessments for {slice}"
                )));
            }
            continue;
        };

        // A per-axis grade carries two coexisting results: the math:Quantity holding
        // the score, and the categorical tier the score earned. They are told apart
        // structurally — the quantity is the one bearing math:quantityValue — never by
        // IRI shape, so the reader does not depend on the minting convention.
        let mut score: Option<f64> = None;
        let mut tier_iri: Option<String> = None;
        for result in &results {
            let Some(result_id) = graph::id(ds, result) else {
                continue;
            };
            match quantity_value.and_then(|p| graph::one_lit(ds, result_id, p)) {
                Some(lexical) => {
                    let value = lexical.parse::<f64>().map_err(|e| {
                        record_err(format!(
                            "the recorded score <{result}> of axis {axis_iri} on {slice} is not a \
                             number ({lexical:?}): {e}"
                        ))
                    })?;
                    if score.replace(value).is_some() {
                        return Err(record_err(format!(
                            "the recorded grade <{subject_iri}> carries two scores"
                        )));
                    }
                }
                None => {
                    if tier_iri.replace(result.clone()).is_some() {
                        return Err(record_err(format!(
                            "the recorded grade <{subject_iri}> carries two tiers"
                        )));
                    }
                }
            }
        }
        let score = score.ok_or_else(|| {
            record_err(format!(
                "the recorded grade for axis {axis_iri} on {slice} carries no \
                 math:quantityValue score"
            ))
        })?;
        let tier_iri = tier_iri.ok_or_else(|| {
            record_err(format!(
                "the recorded grade for axis {axis_iri} on {slice} carries no tier"
            ))
        })?;
        let tier = standard.tier(&tier_iri).cloned().ok_or_else(|| {
            record_err(format!(
                "the recorded grade for axis {axis_iri} on {slice} names tier <{tier_iri}>, which \
                 is not a rung of the rubric's tier ladder"
            ))
        })?;
        grades.entry(slice).or_default().push(AxisGrade {
            axis_iri,
            score,
            tier,
        });
    }

    // Every axis the rubric declares must be graded for every slice in the record: a
    // record missing an axis would silently un-floor that axis for that slice.
    let declared: Vec<&str> = standard.axes.iter().map(|a| a.iri.as_str()).collect();
    let mut out = BTreeMap::new();
    for (slice, mut slice_grades) in grades {
        // The projection sorts by axis IRI and the scorer grades in the standard's
        // (IRI-sorted) axis order, so this restores the scorer's exact vector order.
        slice_grades.sort_by(|a, b| a.axis_iri.cmp(&b.axis_iri));
        let recorded: Vec<&str> = slice_grades.iter().map(|g| g.axis_iri.as_str()).collect();
        let mut expected = declared.clone();
        expected.sort_unstable();
        if recorded != expected {
            return Err(record_err(format!(
                "the recorded assessment of {slice} grades {} axes but the rubric declares {} — \
                 the record does not describe this rubric; regenerate it",
                recorded.len(),
                expected.len()
            )));
        }
        let rollup_iri = rollups.remove(&slice).ok_or_else(|| {
            record_err(format!(
                "the recorded assessment of {slice} carries no roll-up tier"
            ))
        })?;
        let rollup = standard.tier(&rollup_iri).cloned().ok_or_else(|| {
            record_err(format!(
                "the recorded roll-up of {slice} names tier <{rollup_iri}>, which is not a rung \
                 of the rubric's tier ladder"
            ))
        })?;
        out.insert(
            slice.clone(),
            SliceAssessment {
                slice,
                grades: slice_grades,
                rollup,
            },
        );
    }
    if let Some((slice, _)) = rollups.into_iter().next() {
        return Err(record_err(format!(
            "the recorded corpus carries a roll-up for {slice} but no per-axis grades"
        )));
    }
    Ok(out)
}

fn record_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(error::Record { detail })
}
