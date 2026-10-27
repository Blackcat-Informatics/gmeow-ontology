// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only admission of the authenticated class-source channel.

use super::Admissions;
use gmeow_conformance::native_observation::SourceAdmissionObservation;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson},
};
use std::collections::BTreeMap;

struct AdmissionOutcome(Result<SourceAdmissionObservation, gmeow_errors::RecordedDiag>);
impl FromJson for AdmissionOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "class-source admission outcome",
            "one class-source admission outcome variant",
        )
        .map(Self)
    }
}

pub(in super::super) fn from_bytes(bytes: &[u8]) -> Result<Admissions, DecodeError> {
    let observations: BTreeMap<String, AdmissionOutcome> =
        purrdf_lex::json::record::from_slice(bytes)?;
    Ok(observations
        .into_iter()
        .map(|(name, outcome)| (name, outcome.0))
        .collect())
}
