// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Target-independent GMN validation verdicts over an explicitly supplied dictionary.

use purrdf_lex::json::record::{DecodeError, FromJson, Record, ToJson};
use purrdf_lex::json::{Object, Value};

use crate::{Gmn1Document, GmnDictionary, gmn1_read};

/// Public GMN validator result, retaining the codec's exact failure class and detail.
/// Field order preserves the canonical JSON emitted by the browser wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmnValidationVerdict {
    /// Whether the document resolved completely against the selected dictionary.
    pub conformant: bool,
    /// Codec diagnostic, present only when validation failed.
    pub detail: Option<String>,
    /// Full typed language failure IRI, present only when validation failed.
    pub failure_class: Option<String>,
}

impl ToJson for GmnValidationVerdict {
    /// Emit declaration-order members, omitting absent diagnostic strings.
    fn to_json(&self) -> Value {
        let mut object = Object::new().with("conformant", self.conformant);
        if let Some(detail) = &self.detail {
            object.insert("detail", detail.to_json());
        }
        if let Some(failure_class) = &self.failure_class {
            object.insert("failureClass", failure_class.to_json());
        }
        Value::from(object)
    }
}

impl FromJson for GmnValidationVerdict {
    /// Admit an object with a required boolean, nullable strings and ignored unknown fields.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut record = Record::new(value, "a GMN validation verdict object")?;
        Ok(Self {
            conformant: record.required("conformant")?,
            detail: record.optional("detail")?,
            failure_class: record.optional("failureClass")?,
        })
    }
}

/// Validate a document using the caller's already prepared dictionary.
///
/// No source loading, dictionary compilation or weaker syntax-only fallback occurs.
/// The browser and producer serialize this same result at their own JSON boundary.
#[must_use]
pub fn validate_gmn_document(
    document: &Gmn1Document,
    dictionary: &GmnDictionary,
) -> GmnValidationVerdict {
    match gmn1_read(document, dictionary) {
        Ok(_) => GmnValidationVerdict {
            conformant: true,
            detail: None,
            failure_class: None,
        },
        Err(error) => GmnValidationVerdict {
            conformant: false,
            detail: Some(error.to_string()),
            failure_class: Some(error.failure_class().to_owned()),
        },
    }
}

#[cfg(test)]
#[path = "gmn_validation.tests.rs"]
mod tests;
