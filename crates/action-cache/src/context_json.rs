// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete action-key framing. Normalization belongs to the constructors;
//! this writer preserves every stored field, vector entry and explicit null.

use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

use super::{ActionContext, ActionInput, FileKind, ProducerIdentity};

impl ToJson for ProducerIdentity {
    fn to_json(&self) -> Value {
        Object::new()
            .with("digest", self.digest.to_json())
            .with("toolchain", self.toolchain.to_json())
            .with("target", self.target.to_json())
            .with("profile", self.profile.to_json())
            .with("features", self.features.to_json())
            .into()
    }
}

impl ToJson for FileKind {
    fn to_json(&self) -> Value {
        match self {
            Self::File => "file",
            Self::Symlink => "symlink",
            Self::Aggregate => "aggregate",
        }
        .to_json()
    }
}

impl ToJson for ActionInput {
    fn to_json(&self) -> Value {
        match self {
            Self::Upstream {
                producer,
                entity,
                receipt_digest,
                product_digest,
            } => Object::new()
                .with("kind", "upstream".to_json())
                .with("producer", producer.to_json())
                .with("entity", entity.to_json())
                .with("receipt_digest", receipt_digest.to_json())
                .with("product_digest", product_digest.to_json())
                .into(),
            Self::Raw {
                logical_path,
                file_kind,
                executable,
                digest,
            } => Object::new()
                .with("kind", "raw".to_json())
                .with("logical_path", logical_path.to_json())
                .with("file_kind", file_kind.to_json())
                .with("executable", executable.to_json())
                .with("digest", digest.to_json())
                .into(),
        }
    }
}

impl ToJson for ActionContext {
    fn to_json(&self) -> Value {
        Object::new()
            .with("schema_version", self.schema_version.to_json())
            .with("domain", self.domain.to_json())
            .with("action", self.action.to_json())
            .with("implementation", self.implementation.to_json())
            .with("codec", self.codec.to_json())
            .with("inputs", self.inputs.to_json())
            .with("dimensions", self.dimensions.to_json())
            .into()
    }
}

::purrdf_lex::json_record! { impl FromJson for ProducerIdentity as "struct ProducerIdentity" {
    "digest" => digest: required,
    "toolchain" => toolchain: required,
    "target" => target: required,
    "profile" => profile: required,
    "features" => features: required
}}

impl FromJson for FileKind {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match value.as_str() {
            Some("file") => Ok(Self::File),
            Some("symlink") => Ok(Self::Symlink),
            Some("aggregate") => Ok(Self::Aggregate),
            Some(other) => Err(DecodeError::unknown_variant(
                other,
                &["file", "symlink", "aggregate"],
            )),
            None => Err(DecodeError::invalid_type(value, "a file kind")),
        }
    }
}

impl FromJson for ActionInput {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "enum ActionInput")?;
        let kind: String = fields.required("kind")?;
        let result = match kind.as_str() {
            "upstream" => Self::Upstream {
                producer: fields.required("producer")?,
                entity: fields.required("entity")?,
                receipt_digest: fields.required("receipt_digest")?,
                product_digest: fields.required("product_digest")?,
            },
            "raw" => Self::Raw {
                logical_path: fields.required("logical_path")?,
                file_kind: fields.required("file_kind")?,
                executable: fields.required("executable")?,
                digest: fields.required("digest")?,
            },
            other => return Err(DecodeError::unknown_variant(other, &["upstream", "raw"])),
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}

impl FromJson for ActionContext {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ActionContext")?;
        let result = Self {
            schema_version: super::receipt_json::schema_version(&mut fields)?,
            domain: fields.required("domain")?,
            action: fields.required("action")?,
            implementation: fields.required("implementation")?,
            codec: fields.required("codec")?,
            inputs: fields.required("inputs")?,
            dimensions: fields.required("dimensions")?,
        };
        fields.deny_unknown()?;
        Ok(result)
    }
}
