// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native observation records; catalog discovery and reviewed-key policy stay upstream.

use super::{ReviewedCoverageObservation, ReviewedCoverageOutcome};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;

impl ToJson for ReviewedCoverageObservation {
    fn to_json(&self) -> Value {
        Object::new()
            .with("parsed_catalogs", self.parsed_catalogs.to_json())
            .with("reviewed_coverage", self.reviewed_coverage.to_json())
            .into()
    }
}

/// This wire-only count lets the native map admit every duplicate occurrence
/// before retaining its last value; unsigned syntax and host bounds are required.
struct ParsedCatalogCount(usize);

impl FromJson for ParsedCatalogCount {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_unsigned(value, "usize").map(Self)
    }
}
impl FromJson for ReviewedCoverageObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ReviewedCoverageObservation")?;
        let parsed_catalogs = fields
            .required::<BTreeMap<String, ParsedCatalogCount>>("parsed_catalogs")?
            .into_iter()
            .map(|(path, count)| (path, count.0))
            .collect();
        Ok(Self {
            parsed_catalogs,
            reviewed_coverage: fields.required("reviewed_coverage")?,
        })
    }
}
impl ToJson for ReviewedCoverageOutcome {
    fn to_json(&self) -> Value {
        gmeow_errors::json::recorded_result_to_json(&self.0)
    }
}
impl FromJson for ReviewedCoverageOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "reviewed coverage Result",
            "one reviewed coverage Result variant",
        )
        .map(Self)
    }
}

#[cfg(test)]
mod tests;
