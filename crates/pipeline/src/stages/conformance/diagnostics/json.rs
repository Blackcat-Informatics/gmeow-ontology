// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native diagnostic observations preserve every success and recorded refusal.

use super::{
    GateObservation, MetaDerivation, MetaReportObservation, Observations, ProjectionObservation,
};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;

struct Recorded<T>(Result<T, gmeow_errors::RecordedDiag>);

impl<T: FromJson> FromJson for Recorded<T> {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "diagnostic outcome",
            "one diagnostic outcome variant",
        )
        .map(Self)
    }
}

fn outcomes<T: ToJson>(values: &BTreeMap<String, Result<T, gmeow_errors::RecordedDiag>>) -> Value {
    Value::Object(
        values
            .iter()
            .map(|(name, outcome)| {
                (
                    name.as_str(),
                    gmeow_errors::json::recorded_result_to_json(outcome),
                )
            })
            .collect(),
    )
}

struct Coordinate((String, String, String));

impl FromJson for Coordinate {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let [severity, category, standpoint] = gmeow_errors::json::read_string_row(value)?;
        Ok(Self((severity, category, standpoint)))
    }
}

::purrdf_lex::json_record!(impl ToJson for ProjectionObservation {
    "nquads" => nquads,
    "fatal" => fatal,
});
impl FromJson for ProjectionObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProjectionObservation")?;
        Ok(Self {
            nquads: fields.required("nquads")?,
            fatal: fields.required("fatal")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for MetaReportObservation {
    "nquads" => nquads,
    "derivation" => derivation,
});
impl FromJson for MetaReportObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct MetaReportObservation")?;
        Ok(Self {
            nquads: fields.required("nquads")?,
            derivation: fields.required("derivation")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for GateObservation {
    "coordinates" => coordinates,
    "category_blocking" => category_blocking,
    "fatal" => fatal,
    "projections" => projections,
});
impl FromJson for GateObservation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GateObservation")?;
        let coordinates: BTreeMap<String, Coordinate> = fields.required("coordinates")?;
        Ok(Self {
            coordinates: coordinates
                .into_iter()
                .map(|(name, coordinate)| (name, coordinate.0))
                .collect(),
            category_blocking: fields.required("category_blocking")?,
            fatal: fields.required("fatal")?,
            projections: fields.required("projections")?,
        })
    }
}

impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("meta_sources", self.meta_sources.to_json())
            .with("meta", outcomes(&self.meta))
            .with(
                "gate",
                gmeow_errors::json::recorded_result_to_json(&self.gate),
            )
            .with("reports", outcomes(&self.reports))
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        let meta_sources = fields.required("meta_sources")?;
        let meta: BTreeMap<String, Recorded<MetaDerivation>> = fields.required("meta")?;
        let gate: Recorded<GateObservation> = fields.required("gate")?;
        let reports: BTreeMap<String, Recorded<MetaReportObservation>> =
            fields.required("reports")?;
        Ok(Self {
            meta_sources,
            meta: meta
                .into_iter()
                .map(|(name, outcome)| (name, outcome.0))
                .collect(),
            gate: gate.0,
            reports: reports
                .into_iter()
                .map(|(name, outcome)| (name, outcome.0))
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
