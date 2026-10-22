// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native inventory transport; source selection and profile admission are unchanged.

use super::{ProjectionInventory, ProjectionProfile};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
use std::collections::BTreeMap;

impl ToJson for ProjectionProfile {
    fn to_json(&self) -> Value {
        Object::new()
            .with("iri", self.iri.to_json())
            .with("source", self.source.to_json())
            .with("cell_minimum", self.cell_minimum)
            .into()
    }
}

impl ToJson for ProjectionInventory {
    fn to_json(&self) -> Value {
        Object::new()
            .with("declared", self.declared.to_json())
            .with("measured", self.measured.to_json())
            .into()
    }
}

/// Profile floors and measured cell counts share strict unsigned-token admission.
/// The private wire value lets the native map decoder retain its last-wins policy.
struct ProfileCellCount(usize);

impl FromJson for ProfileCellCount {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        value
            .as_u64()
            .and_then(|number| usize::try_from(number).ok())
            .map(Self)
            .ok_or_else(|| DecodeError::invalid_value(value, "usize"))
    }
}

impl FromJson for ProjectionProfile {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct ProjectionProfile")?;
        Ok(Self {
            iri: record.required("iri")?,
            source: record.required("source")?,
            cell_minimum: record.required::<ProfileCellCount>("cell_minimum")?.0,
        })
    }
}

impl FromJson for ProjectionInventory {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "struct ProjectionInventory")?;
        let declared = record.required("declared")?;
        let measured = record
            .required::<BTreeMap<String, ProfileCellCount>>("measured")?
            .into_iter()
            .map(|(source, count)| (source, count.0))
            .collect();
        Ok(Self { declared, measured })
    }
}

#[cfg(test)]
mod tests;
