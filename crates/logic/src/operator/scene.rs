// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-bound native operator observations. These records preserve actual rows,
//! applications and explicit refusals; they are not whole-theory certificates.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Derivation, refinement::RefinementRecord};

/// Compact producer artifact, consumed without parsing or executing a source.
pub const CHANNEL: &str = "pipeline/operator-scene-observations.json";

#[derive(Serialize, Deserialize)]
pub struct Observations {
    /// Every selected example, including sources with no applicable label claim.
    pub examples: BTreeMap<String, ExampleSelection>,
    pub scenes: BTreeMap<String, SourceObservation>,
}

#[derive(Serialize, Deserialize)]
pub struct ExampleSelection {
    pub source_digest: String,
    /// The existing sweep's exact source marker, never an inferred success flag.
    pub contains_entry_label_marker: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SourceObservation {
    pub source_path: String,
    pub source_digest: String,
    pub has_attempt_of_intent: bool,
    pub original: Result<Derivation, gmeow_errors::RecordedDiag>,
    pub ocr: Option<OcrObservation>,
    pub refinement: Option<RefinementRecord>,
}

#[derive(Serialize, Deserialize)]
pub struct OcrObservation {
    pub entry: String,
    pub action: String,
    pub asserted_labels: Vec<String>,
    pub entry_actions: Vec<String>,
    pub mutation: LabelMutation,
}

#[derive(Serialize, Deserialize)]
pub struct LabelMutation {
    pub predicate: String,
    pub removed: String,
    pub inserted: String,
    pub removed_rows: usize,
    pub inserted_rows: usize,
    pub result: Result<Derivation, gmeow_errors::RecordedDiag>,
}
