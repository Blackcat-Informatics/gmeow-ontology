// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Qualitative evidence and numerical interpretation remain distinct source claims.

/// Source references that give a correspondence's quantitative axes their meaning.
/// A justification never implies a numeric warrant without an authored scale.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AxisEvidence {
    /// Full identities of the qualitative evidence, never local-name rankings.
    pub sources: Vec<String>,
    /// Shared warrant scale required for numerical weakest-link composition.
    pub scale: Option<String>,
    /// Cross-chain probability model; its name alone never implies multiplication.
    pub probability_model: Option<String>,
}

impl AxisEvidence {
    /// Validate named references and canonicalize the unordered evidence set.
    pub fn new(
        mut sources: Vec<String>,
        scale: Option<String>,
        probability_model: Option<String>,
    ) -> gmeow_errors::Result<Self> {
        for value in sources
            .iter()
            .chain(scale.iter())
            .chain(probability_model.iter())
        {
            purrdf::sparql::NamedNode::new(value.as_str()).map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::Ir {
                    detail: format!("correspondence axis evidence: {error}"),
                })
            })?;
        }
        sources.sort();
        sources.dedup();
        Ok(Self {
            sources,
            scale,
            probability_model,
        })
    }

    /// Complete reference identity; this contains no inferred numerical value.
    pub fn content_key(&self) -> String {
        serde_json::to_string(self).expect("typed axis evidence serialization is infallible")
    }
}
