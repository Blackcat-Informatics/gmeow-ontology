// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compiler diagnostic and portable source-description transport.
//! Wire severity differs from display severity tokens.
use super::{Diagnostic, Severity, SourceBase, SourceBaseOrigin, SourceDocument};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

::purrdf_lex::json_record!(impl ToJson for SourceDocument {
    "path" => path,
    "content_digest" => content_digest,
    "role" => role,
    "base" => base,
});

::purrdf_lex::json_record!(impl ToJson for SourceBase {
    "iri" => iri,
    "origin" => origin,
});

impl ToJson for SourceBaseOrigin {
    fn to_json(&self) -> Value {
        match self {
            Self::Caller => "Caller".to_json(),
            Self::Directive { line, column } => Object::new()
                .with(
                    "Directive",
                    Object::new()
                        .with("line", line.to_json())
                        .with("column", column.to_json()),
                )
                .into(),
            Self::Enclosing => "Enclosing".to_json(),
        }
    }
}

impl ToJson for Severity {
    fn to_json(&self) -> Value {
        match self {
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Info => "Info",
        }
        .to_json()
    }
}
impl FromJson for Severity {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "Severity")? {
            "Error" => Ok(Self::Error),
            "Warning" => Ok(Self::Warning),
            "Info" => Ok(Self::Info),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Error", "Warning", "Info"],
            )),
        }
    }
}
impl ToJson for Diagnostic {
    fn to_json(&self) -> Value {
        Object::new()
            .with("severity", self.severity.to_json())
            .with("code", self.code.to_json())
            .with("message", self.message.to_json())
            .with("subject", self.subject.to_json())
            .into()
    }
}
impl FromJson for Diagnostic {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Diagnostic")?;
        Ok(Self {
            severity: fields.required("severity")?,
            code: fields.required("code")?,
            message: fields.required("message")?,
            subject: fields.optional("subject")?,
        })
    }
}
#[cfg(test)]
mod tests;
