// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Named fragment inventory transport; observing source and deciding remain separate.

use super::NativeFragmentRegistry;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for NativeFragmentRegistry {
    fn to_json(&self) -> Value {
        Object::new()
            .with("pattern_ids", self.pattern_ids.to_json())
            .with("decided_ids", self.decided_ids.to_json())
            .with("source_admission_ids", self.source_admission_ids.to_json())
            .with(
                "source_admission_requirements",
                self.source_admission_requirements.to_json(),
            )
            .with("boundary_ids", self.boundary_ids.to_json())
            .with("deciding_patterns", self.deciding_patterns.to_json())
            .with("completeness_bounds", self.completeness_bounds.to_json())
            .with("boundary_reasons", self.boundary_reasons.to_json())
            .into()
    }
}

impl FromJson for NativeFragmentRegistry {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "native fragment registry")?;
        Ok(Self {
            pattern_ids: fields.required("pattern_ids")?,
            decided_ids: fields.required("decided_ids")?,
            source_admission_ids: fields.required("source_admission_ids")?,
            source_admission_requirements: fields.required("source_admission_requirements")?,
            boundary_ids: fields.required("boundary_ids")?,
            deciding_patterns: fields.required("deciding_patterns")?,
            completeness_bounds: fields.required("completeness_bounds")?,
            boundary_reasons: fields.required("boundary_reasons")?,
        })
    }
}
