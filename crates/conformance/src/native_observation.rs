// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit native case operations; source admission never implies consistency.

use gmeow_logic::reason::refute::{ClassAdmissionObservation, PreparedClassAnalysis};
use purrdf::{NativeRdfFormat, RdfDataset, dataset_from_bytes};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The case profile selects exactly one native operation before any evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeCaseOperation {
    /// Semantic consistency of the selected native theory.
    Consistency,
    /// Source ownership and grammar of the class-expression/list contract only.
    ClassSourceAdmission,
}

/// Source identity and the exact retained native admission observation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAdmissionObservation {
    /// BLAKE3 of the exact input bytes, independent of the action store's SHA-256 key.
    pub input_blake3: [u8; 32],
    /// Original native graph ownership, selected definitions and every refusal.
    pub admission: ClassAdmissionObservation,
}

/// One typed selected result; consumers reject a result for another operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeCaseObservation {
    /// Complete consistency evidence, including any capability boundaries.
    Consistency(crate::consistency::Observation),
    /// Source admission evidence without a semantic consistency claim.
    ClassSourceAdmission(SourceAdmissionObservation),
}

impl NativeCaseObservation {
    /// The operation whose result this payload actually contains.
    #[must_use]
    pub fn operation(&self) -> NativeCaseOperation {
        match self {
            Self::Consistency(_) => NativeCaseOperation::Consistency,
            Self::ClassSourceAdmission(_) => NativeCaseOperation::ClassSourceAdmission,
        }
    }
}

/// Observe one already admitted RDF input using only its selected native operation.
/// The explicit producer supplies the original byte digest; no serialization occurs.
pub fn observe(
    dataset: &RdfDataset,
    input_blake3: [u8; 32],
    operation: NativeCaseOperation,
) -> gmeow_errors::Result<NativeCaseObservation> {
    match operation {
        NativeCaseOperation::Consistency => {
            crate::consistency::observe(dataset).map(NativeCaseObservation::Consistency)
        }
        NativeCaseOperation::ClassSourceAdmission => Ok(
            NativeCaseObservation::ClassSourceAdmission(SourceAdmissionObservation {
                input_blake3,
                admission: PreparedClassAnalysis::new(dataset)?.admission().clone(),
            }),
        ),
    }
}

/// Standalone producer ingress; tests may call it only for tiny explicit synthetic inputs.
pub fn read(
    path: &Path,
    operation: NativeCaseOperation,
) -> gmeow_errors::Result<NativeCaseObservation> {
    let bytes =
        std::fs::read(path).map_err(|error| fail(format!("read {}: {error}", path.display())))?;
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
        .map_err(|error| fail(format!("parse {}: {error}", path.display())))?;
    observe(&dataset, *blake3::hash(&bytes).as_bytes(), operation)
}

fn fail(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::RunFailed { detail })
}
