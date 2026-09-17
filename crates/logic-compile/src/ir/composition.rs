// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored sequential composition declarations. These are obligations, not certificates.

use std::fmt::Write as _;

/// A named claim that `composite` executes `first` followed by `second`.
/// The referenced correspondences retain their own endpoints, context, legs,
/// laws, complements and loss. Reading this record discharges none of them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct CorrespondenceComposition {
    /// Original named source identity, retained in diagnostic and gate evidence.
    pub iri: String,
    /// First correspondence in acquisition order.
    pub first: String,
    /// Second correspondence in acquisition order; updates run in reverse order.
    pub second: String,
    /// Independently declared result correspondence to be checked.
    pub composite: String,
    /// Named canonical Formula roots selected for quantitative execution.
    pub axis_rules: Vec<String>,
    /// Source evidence declaring confidence independence for these exact operands.
    pub confidence_independence: Option<String>,
    /// Source evidence declaring probability independence for these exact operands.
    pub probability_independence: Option<String>,
}

impl CorrespondenceComposition {
    /// Require four named identities. Reference resolution belongs to the complete
    /// program, so a declaration can refer to another selected source document.
    pub fn new(
        iri: String,
        first: String,
        second: String,
        composite: String,
    ) -> gmeow_errors::Result<Self> {
        for (name, value) in [
            ("iri", &iri),
            ("first", &first),
            ("second", &second),
            ("composite", &composite),
        ] {
            purrdf::sparql::NamedNode::new(value.as_str()).map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::Ir {
                    detail: format!("correspondence composition {name}: {error}"),
                })
            })?;
        }
        Ok(Self {
            iri,
            first,
            second,
            composite,
            axis_rules: Vec::new(),
            confidence_independence: None,
            probability_independence: None,
        })
    }

    /// Attach explicit rule selections and operand-owned independence assumptions.
    /// These references preserve declarations; they do not discharge any assumption.
    pub fn with_axis_rules(
        mut self,
        mut rules: Vec<String>,
        confidence_independence: Option<String>,
        probability_independence: Option<String>,
    ) -> gmeow_errors::Result<Self> {
        for value in rules
            .iter()
            .chain(confidence_independence.iter())
            .chain(probability_independence.iter())
        {
            purrdf::sparql::NamedNode::new(value.as_str()).map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::Ir {
                    detail: format!("composition axis reference: {error}"),
                })
            })?;
        }
        rules.sort();
        rules.dedup();
        self.axis_rules = rules;
        self.confidence_independence = confidence_independence;
        self.probability_independence = probability_independence;
        Ok(self)
    }

    /// Length-framed identity preserves operand order and every authored reference.
    pub fn content_key(&self) -> String {
        let mut key = String::from("correspondence-composition-v2;");
        for value in [&self.iri, &self.first, &self.second, &self.composite] {
            write!(key, "{}:{value}", value.len()).expect("writing to a String is infallible");
        }
        for (label, values) in [
            ("rules", self.axis_rules.iter().collect::<Vec<_>>()),
            (
                "confidence-independence",
                self.confidence_independence.iter().collect(),
            ),
            (
                "probability-independence",
                self.probability_independence.iter().collect(),
            ),
        ] {
            write!(key, "{label}:{};", values.len()).expect("writing to a String is infallible");
            for value in values {
                write!(key, "{}:{value}", value.len()).expect("writing to a String is infallible");
            }
        }
        key
    }
}
