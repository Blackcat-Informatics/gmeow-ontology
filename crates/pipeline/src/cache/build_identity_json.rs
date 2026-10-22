// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native transport of declared build metadata, without obtaining host identity.
use super::BuildIdentity;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for BuildIdentity {
    fn to_json(&self) -> Value {
        Object::new()
            .with("fingerprint", self.fingerprint.to_json())
            .with("toolchain", self.toolchain.to_json())
            .with("target", self.target.to_json())
            .with("profile", self.profile.to_json())
            .with("features", self.features.to_json())
            .into()
    }
}
impl FromJson for BuildIdentity {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct BuildIdentity")?;
        Ok(Self {
            fingerprint: fields.required("fingerprint")?,
            toolchain: fields.required("toolchain")?,
            target: fields.required("target")?,
            profile: fields.required("profile")?,
            features: fields.required("features")?,
        })
    }
}
#[cfg(test)]
mod tests;
