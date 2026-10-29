// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native JSON owners; hydration preserves the admitted fields without normalization.

use super::{ProductionSelection, UnitSelection};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record},
};

::purrdf_lex::json_record! { impl ToJson for UnitSelection {
    "package" => package,
    "manifest" => manifest,
    "source" => source,
    "target" => target,
    "edition" => edition,
    "kinds" => kinds,
    "cfg" => cfg,
    "dependencies" => dependencies,
    "dependency_names" => dependency_names,
    "controller" => controller,
}}

impl FromJson for UnitSelection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct UnitSelection")?;
        let result = Self {
            package: fields.required("package")?,
            manifest: fields.optional("manifest")?,
            source: fields.optional("source")?,
            target: fields.required("target")?,
            edition: fields.required("edition")?,
            kinds: fields.required("kinds")?,
            cfg: fields.required("cfg")?,
            dependencies: fields.required_with("dependencies", crate::json::indices)?,
            dependency_names: fields.required("dependency_names")?,
            controller: fields.required("controller")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for ProductionSelection {
    "schema" => schema,
    "units" => units,
    "roots" => roots,
    "policy_files" => policy_files,
}}

impl FromJson for ProductionSelection {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ProductionSelection")?;
        let result = Self {
            schema: fields.required_with("schema", |value| {
                gmeow_errors::json::read_unsigned::<u32>(value, "u32")
            })?,
            units: fields.required("units")?,
            roots: fields.required_with("roots", crate::json::indices)?,
            policy_files: fields.required("policy_files")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
