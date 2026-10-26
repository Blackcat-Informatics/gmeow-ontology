// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete declaration and facet outcomes; decoding never reopens source RDF.

use super::Observation;
use gmeow_errors::json::{read_recorded_result, recorded_result_to_json};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::{BTreeMap, BTreeSet};

struct MemberOutcome(Result<BTreeSet<String>, gmeow_errors::RecordedDiag>);

impl FromJson for MemberOutcome {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        read_recorded_result(
            value,
            "vocabulary member outcome",
            "one vocabulary member outcome variant",
        )
        .map(Self)
    }
}

impl ToJson for Observation {
    fn to_json(&self) -> Value {
        let members: Object = self
            .members
            .iter()
            .map(|(name, outcome)| (name.as_str(), recorded_result_to_json(outcome)))
            .collect();
        Object::new()
            .with("source", self.source.to_json())
            .with("members", Value::from(members))
            .with(
                "preset_facets",
                recorded_result_to_json(&self.preset_facets),
            )
            .into()
    }
}

impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        let source = fields.required("source")?;
        // Native map admission visits every supplied value before last-wins
        // replacement. A malformed shadowed outcome is never discarded unread.
        let members: BTreeMap<String, MemberOutcome> = fields.required("members")?;
        let preset_facets = fields.required_with("preset_facets", |value| {
            read_recorded_result(
                value,
                "vocabulary facet outcome",
                "one vocabulary facet outcome variant",
            )
        })?;
        Ok(Self {
            source,
            members: members
                .into_iter()
                .map(|(name, outcome)| (name, outcome.0))
                .collect(),
            preset_facets,
        })
    }
}
