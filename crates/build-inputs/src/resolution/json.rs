// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native JSON owners; hydration preserves the admitted fields without normalization.

use super::{CargoResolutionEvidence, CargoResolutionInputs};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for CargoResolutionInputs {
    "schema" => schema,
    "manifests" => manifests,
    "policy_files" => policy_files,
    "files" => files,
    "workspace_patterns" => workspace_patterns,
    "configuration" => configuration,
    "environment" => environment,
}}

impl FromJson for CargoResolutionInputs {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CargoResolutionInputs")?;
        let result = Self {
            schema: fields.required_with("schema", |value| {
                gmeow_errors::json::read_unsigned::<u32>(value, "u32")
            })?,
            manifests: fields.required("manifests")?,
            policy_files: fields.required("policy_files")?,
            files: fields.required("files")?,
            workspace_patterns: fields.required("workspace_patterns")?,
            configuration: fields.required("configuration")?,
            environment: fields.required("environment")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for CargoResolutionEvidence {
    "schema" => schema,
    "selection_sha256" => selection_sha256,
    "inputs" => inputs,
}}

impl FromJson for CargoResolutionEvidence {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CargoResolutionEvidence")?;
        let result = Self {
            schema: fields.required_with("schema", |value| {
                gmeow_errors::json::read_unsigned::<u32>(value, "u32")
            })?,
            selection_sha256: fields.required("selection_sha256")?,
            inputs: fields.required("inputs")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
