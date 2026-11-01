// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native invocation measurements; every declared optional field is retained as null.

use super::{
    ExampleExecution, ExampleValidationMeasurements, ExampleValidationSample, ExampleViews,
};
use purrdf_lex::json::{Object, Value, record::ToJson};

impl ToJson for ExampleValidationMeasurements {
    /// Preserve the complete declared measurement fields and explicit null options.
    fn to_json(&self) -> Value {
        Object::new()
            .with("base_projection_us", self.base_projection_us)
            .with("shape_preparation_us", self.shape_preparation_us)
            .with("examples", self.examples.to_json())
            .into()
    }
}

impl ToJson for ExampleValidationSample {
    /// Preserve the complete declared measurement fields and explicit null options.
    fn to_json(&self) -> Value {
        Object::new()
            .with("example", self.example.as_str())
            .with("execution", self.execution.as_ref().to_json())
            .into()
    }
}

impl ToJson for ExampleExecution {
    /// Preserve the complete declared measurement fields and explicit null options.
    fn to_json(&self) -> Value {
        Object::new()
            .with("parse_us", self.parse_us)
            .with("projection_us", self.projection_us)
            .with("binding_us", self.binding_us)
            .with("validation_us", self.validation_us)
            .with("views", self.views.as_ref().to_json())
            .into()
    }
}

impl ToJson for ExampleViews {
    /// Preserve the complete declared measurement fields and explicit null options.
    fn to_json(&self) -> Value {
        Object::new()
            .with("retained_payload_bytes", self.retained_payload_bytes)
            .with("composite_auxiliary_bytes", self.composite_auxiliary_bytes)
            .with("copied_terms", self.copied_terms)
            .with("copied_rows", self.copied_rows)
            .with("copied_text_bytes", self.copied_text_bytes)
            .with("copied_index_bytes", self.copied_index_bytes)
            .with("freezes", self.freezes)
            .with(
                "composite_materializations",
                self.composite_materializations,
            )
            .with("core_auxiliary_bytes", self.core_auxiliary_bytes)
            .with("query_auxiliary_bytes", self.query_auxiliary_bytes)
            .with("core_materializations", self.core_materializations)
            .with("query_materializations", self.query_materializations)
            .into()
    }
}
