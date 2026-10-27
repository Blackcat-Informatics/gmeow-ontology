// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only consumers retain the complete producer-selected source outcomes.

use super::super::Observations;
use super::*;
use std::collections::BTreeMap;

struct SourceOutcome(Result<FormulaObservation, gmeow_errors::RecordedDiag>);
impl FromJson for SourceOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_recorded_result(
            value,
            "formula source outcome",
            "one formula source outcome variant",
        )
        .map(Self)
    }
}

pub(in super::super) fn from_bytes(bytes: &[u8]) -> Result<Observations, DecodeError> {
    let sources: BTreeMap<String, SourceOutcome> = purrdf_lex::json::record::from_slice(bytes)?;
    Ok(sources
        .into_iter()
        .map(|(name, outcome)| (name, outcome.0))
        .collect())
}
