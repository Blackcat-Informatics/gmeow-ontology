// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Target-independent GMN validation verdicts over an explicitly supplied dictionary.

use serde::{Deserialize, Serialize};

use crate::{Gmn1Document, GmnDictionary, gmn1_read};

/// Public GMN validator result, retaining the codec's exact failure class and detail.
/// Field order preserves the canonical JSON emitted by the browser wrapper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GmnValidationVerdict {
    /// Whether the document resolved completely against the selected dictionary.
    pub conformant: bool,
    /// Codec diagnostic, present only when validation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Full typed language failure IRI, present only when validation failed.
    #[serde(rename = "failureClass", skip_serializing_if = "Option::is_none")]
    pub failure_class: Option<String>,
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
