// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test consumers decode complete observations without executing their producer.

use super::super::Observations;
use super::*;
use std::collections::BTreeMap;

struct CaseOutcome(Result<ProjectionObservation, gmeow_errors::RecordedDiag>);
impl FromJson for CaseOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_recorded_result(
            value,
            "projection case outcome",
            "one projection case outcome variant",
        )
        .map(Self)
    }
}

pub(in super::super) fn from_bytes(bytes: &[u8]) -> Result<Observations, DecodeError> {
    let cases: BTreeMap<String, CaseOutcome> = purrdf_lex::json::record::from_slice(bytes)?;
    Ok(cases
        .into_iter()
        .map(|(name, outcome)| (name, outcome.0))
        .collect())
}
