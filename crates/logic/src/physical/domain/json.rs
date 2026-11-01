// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Observational JSON for the complete selected-world receipt.
//! Hydration retains supplied identity fields; execution authority and graph
//! admission remain with the existing selected-domain and witness validators.

use super::{DomainProfile, LogicalGraph, SelectedLogicalWorld};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, Within},
};

impl FromJson for LogicalGraph {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if value
            .as_object()
            .and_then(|object| object.get("Named"))
            .is_some()
        {
            let (name, payload) = gmeow_errors::json::read_variant(
                value,
                "LogicalGraph",
                "one LogicalGraph variant",
            )?;
            return crate::term_serde::from_json(payload)
                .map(Self::Named)
                .map_err(|error| error.within(name));
        }
        match gmeow_errors::json::read_unit_name(value, "LogicalGraph")? {
            "Default" => Ok(Self::Default),
            name => Err(DecodeError::unknown_variant(name, &["Default", "Named"])),
        }
    }
}

impl FromJson for DomainProfile {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "DomainProfile")? {
            "NonemptyObjectDomainV1" => Ok(Self::NonemptyObjectDomainV1),
            name => Err(DecodeError::unknown_variant(
                name,
                &["NonemptyObjectDomainV1"],
            )),
        }
    }
}

::purrdf_lex::json_record!(impl ToJson for SelectedLogicalWorld {
    "graph" => graph,
    "profile" => profile,
    "authority" => authority,
    "selection" => selection,
});

impl FromJson for SelectedLogicalWorld {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct SelectedLogicalWorld")?;
        let selected = Self {
            graph: fields.required("graph")?,
            profile: fields.required("profile")?,
            authority: fields.required("authority")?,
            selection: fields
                .required_with("selection", gmeow_errors::json::read_byte_array::<32>)?,
        };
        fields.deny_unknown()?;
        Ok(selected)
    }
}
