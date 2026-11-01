// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete native verification observations, including fallible report metadata
//! projections. Cache readers use the shared diagnostic envelope profile.

use super::{Observations, RepairObservation, SourceObservation};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl Observations {
    pub(super) fn json_value(&self) -> Result<Value, DecodeError> {
        let mut sources = Object::new();
        for (name, source) in &self.sources {
            sources.insert(name, source.json_value()?);
        }
        Ok(Object::new()
            .with("producer", self.producer.to_json())
            .with("query_digests", self.query_digests.to_json())
            .with("query_set_digest", self.query_set_digest.as_str())
            .with("law_source_digests", self.law_source_digests.to_json())
            .with("sources", sources)
            .into())
    }
}

impl SourceObservation {
    pub(super) fn json_value(&self) -> Result<Value, DecodeError> {
        let mut repairs = Object::new();
        for (name, repair) in &self.repairs {
            repairs.insert(name, repair.json_value()?);
        }
        Ok(Object::new()
            .with("source_path", self.source_path.as_str())
            .with("source_digest", self.source_digest.as_str())
            .with("original", self.original.json_value()?)
            .with("repairs", repairs)
            .into())
    }
}

impl RepairObservation {
    fn json_value(&self) -> Result<Value, DecodeError> {
        Ok(Object::new()
            .with("mutation_digest", self.mutation_digest.as_str())
            .with("removed_rows", self.removed_rows.to_json())
            .with("inserted_rows", self.inserted_rows.to_json())
            .with("report", self.report.json_value()?)
            .into())
    }
}

impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "verification observations")?;
        Ok(Self {
            producer: fields.required("producer")?,
            query_digests: fields.required("query_digests")?,
            query_set_digest: fields.required("query_set_digest")?,
            law_source_digests: fields.required("law_source_digests")?,
            sources: fields.required("sources")?,
        })
    }
}

impl FromJson for SourceObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "verification source observation")?;
        Ok(Self {
            source_path: fields.required("source_path")?,
            source_digest: fields.required("source_digest")?,
            original: fields.required("original")?,
            repairs: fields.required("repairs")?,
        })
    }
}

impl FromJson for RepairObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "verification repair observation")?;
        Ok(Self {
            mutation_digest: fields.required("mutation_digest")?,
            removed_rows: fields.required_with("removed_rows", |value| {
                gmeow_errors::json::read_unsigned(value, "usize removed row count")
            })?,
            inserted_rows: fields.required_with("inserted_rows", |value| {
                gmeow_errors::json::read_unsigned(value, "usize inserted row count")
            })?,
            report: fields.required("report")?,
        })
    }
}

#[cfg(test)]
mod tests;
