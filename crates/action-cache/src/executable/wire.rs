// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native executable evidence; typed records retain declaration order.

use super::{CompilationUnit, ExecutableReceipt, ExecutableRecipe};
use gmeow_errors::json::read_unsigned;
use purrdf_lex::json::{
    Object, Value,
    record::{self, DecodeError, FromJson, Record, ToJson},
};

fn schema(fields: &mut Record<'_>) -> Result<u32, DecodeError> {
    fields.required_with("schema", |value| read_unsigned::<u32>(value, "u32"))
}

fn indices(value: &Value) -> Result<Vec<usize>, DecodeError> {
    record::items_with(value, |value| read_unsigned::<usize>(value, "usize"))
}

/// Classify known headers without accepting unknown or obsolete receipt bodies.
pub(super) fn schemas(value: &Value) -> Result<(u32, u32), DecodeError> {
    let mut header = Record::new(value, "executable receipt header")?;
    let receipt = schema(&mut header)?;
    let recipe = header.required_with("recipe", |value| {
        schema(&mut Record::new(value, "executable recipe header")?)
    })?;
    Ok((receipt, recipe))
}

impl ToJson for CompilationUnit {
    fn to_json(&self) -> Value {
        // Cargo's free-form profile was a sorted map in the original wire format.
        // Normalize this field alone; the typed outer record keeps declaration order.
        let mut profile = self.profile.clone();
        record::sorted_last_wins(&mut profile);
        Object::new()
            .with("package", self.package.to_json())
            .with("target", self.target.to_json())
            .with("target_kinds", self.target_kinds.to_json())
            .with("mode", self.mode.to_json())
            .with("platform", self.platform.to_json())
            .with("features", self.features.to_json())
            .with("profile", profile)
            .with("dependencies", self.dependencies.to_json())
            .into()
    }
}

impl FromJson for CompilationUnit {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct CompilationUnit")?;
        let result = Self {
            package: fields.required("package")?,
            target: fields.required("target")?,
            target_kinds: fields.required("target_kinds")?,
            mode: fields.required("mode")?,
            platform: fields.optional("platform")?,
            features: fields.required("features")?,
            profile: fields.required("profile")?,
            dependencies: fields.required_with("dependencies", indices)?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for ExecutableRecipe {
    "schema" => schema,
    "profile" => profile,
    "source_digest" => source_digest,
    "source_inventory" => source_inventory,
    "rustc" => rustc,
    "cargo" => cargo,
    "compiler_environment" => compiler_environment,
    "units" => units,
    "roots" => roots,
}}

impl FromJson for ExecutableRecipe {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ExecutableRecipe")?;
        let result = Self {
            schema: schema(&mut fields)?,
            profile: fields.required("profile")?,
            source_digest: fields.required("source_digest")?,
            source_inventory: fields.required("source_inventory")?,
            rustc: fields.required("rustc")?,
            cargo: fields.required("cargo")?,
            compiler_environment: fields.required("compiler_environment")?,
            units: fields.required("units")?,
            roots: fields.required_with("roots", indices)?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

::purrdf_lex::json_record! { impl ToJson for ExecutableReceipt {
    "schema" => schema,
    "recipe" => recipe,
    "executable_sha256" => executable_sha256,
    "resolution" => resolution,
}}

impl FromJson for ExecutableReceipt {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ExecutableReceipt")?;
        let result = Self {
            schema: schema(&mut fields)?,
            recipe: fields.required("recipe")?,
            executable_sha256: fields.required("executable_sha256")?,
            resolution: fields.required("resolution")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
