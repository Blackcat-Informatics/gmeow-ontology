// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native JSON owners; hydration preserves the admitted fields without normalization.

use super::{FileInput, InputInventory, InputRole};
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

/// Preserve index order and multiplicity while retaining each refusal's index.
pub(crate) fn indices(value: &Value) -> Result<Vec<usize>, DecodeError> {
    purrdf_lex::json::record::items_with(value, |value| read_unsigned::<usize>(value, "usize"))
}

impl purrdf_lex::json::record::ToJson for InputRole {
    fn to_json(&self) -> Value {
        let name = match self {
            Self::Rust => "Rust",
            Self::Embedded => "Embedded",
            Self::BuildController => "BuildController",
            Self::Policy => "Policy",
            Self::Dynamic => "Dynamic",
        };
        purrdf_lex::json::record::ToJson::to_json(name)
    }
}

impl FromJson for InputRole {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        const VARIANTS: &[&str] = &["Rust", "Embedded", "BuildController", "Policy", "Dynamic"];
        // Serde's externally tagged unit enum admits a string or one null-valued tag.
        let name = match value {
            Value::String(name) => name.as_str(),
            Value::Object(object) => match object.members() {
                [(name, Value::Null)] => name.as_str(),
                _ => {
                    return Err(DecodeError::invalid_value(
                        value,
                        "one null-valued input role tag",
                    ));
                }
            },
            _ => return Err(DecodeError::invalid_type(value, "an input role")),
        };
        match name {
            "Rust" => Ok(Self::Rust),
            "Embedded" => Ok(Self::Embedded),
            "BuildController" => Ok(Self::BuildController),
            "Policy" => Ok(Self::Policy),
            "Dynamic" => Ok(Self::Dynamic),
            other => Err(DecodeError::unknown_variant(other, VARIANTS)),
        }
    }
}

::purrdf_lex::json_record! { impl ToJson for FileInput {
    "sha256" => sha256,
    "owners" => owners,
    "roles" => roles,
}}

impl FromJson for FileInput {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct FileInput")?;
        let result = Self {
            sha256: fields.required("sha256")?,
            owners: fields.required("owners")?,
            roles: fields.required("roles")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for InputInventory {
    "schema" => schema,
    "selection" => selection,
    "files" => files,
    "manifests" => manifests,
    "packages" => packages,
    "memberships" => memberships,
    "generated" => generated,
    "environment_owners" => environment_owners,
}}

impl FromJson for InputInventory {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct InputInventory")?;
        let result = Self {
            schema: fields.required_with("schema", |value| read_unsigned::<u32>(value, "u32"))?,
            selection: fields.required("selection")?,
            files: fields.required("files")?,
            manifests: fields.required("manifests")?,
            packages: fields.required("packages")?,
            memberships: fields.required("memberships")?,
            generated: fields.required("generated")?,
            environment_owners: fields.required("environment_owners")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
