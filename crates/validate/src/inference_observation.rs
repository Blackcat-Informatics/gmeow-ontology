// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared inference-validation transport, independent of data construction and authentication.
mod json;
use std::collections::BTreeMap;

/// Complete declared control results under one validation profile.
#[derive(Debug)]
pub struct Observations {
    /// Exact selected validation profile.
    pub profile: String,
    /// Original and generated source identities used by the selected operation.
    pub sources: BTreeMap<String, String>,
    /// Each selected control's native results.
    pub controls: BTreeMap<String, Control>,
}
/// One control's exact input identity and ordered results.
#[derive(Debug)]
pub struct Control {
    /// Original authored control-byte digest.
    pub input_digest: String,
    /// Complete ordered severity and optional message rows.
    pub results: Vec<ResultMessage>,
}
/// A native validation report row with its original severity spelling.
#[derive(Debug)]
pub struct ResultMessage {
    /// Exact severity IRI; transport does not reinterpret it.
    pub severity: String,
    /// Original message, with explicit absence retained.
    pub message: Option<String>,
}
impl Control {
    /// Violation messages in report order; a missing message retains the empty display.
    #[must_use]
    pub fn violations(&self) -> Vec<&str> {
        self.results
            .iter()
            .filter(|result| result.severity == "http://www.w3.org/ns/shacl#Violation")
            .map(|result| result.message.as_deref().unwrap_or_default())
            .collect()
    }
}
