// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native statement envelopes; declaration roles never alias ordinary RDF terms.

use purrdf::TermValue;
use serde::{Deserialize, Serialize};

use super::NativeReadKind;
use crate::native_semantics::SemanticVocabulary;

/// Exact statement positions or an explicitly interpreted declaration marker.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum NativeReadPattern {
    /// Missing positions are wildcards; present RDF terms retain exact identity.
    Statement {
        /// Exact native subject, when selected.
        #[serde(with = "crate::term_serde::optional")]
        subject: Option<TermValue>,
        /// Admitted predicate role, when selected.
        predicate: Option<String>,
        /// Exact ordinary object, when selected.
        #[serde(with = "crate::term_serde::optional")]
        object: Option<TermValue>,
    },
    /// A marker can use its declared alternate spelling only in this predicate role.
    Declaration {
        /// Exact native subject, when selected.
        #[serde(with = "crate::term_serde::optional")]
        subject: Option<TermValue>,
        /// Mandatory predicate governing the marker interpretation.
        predicate: String,
        /// Exact declaration marker, with its admitted role-sensitive alternate.
        marker: String,
    },
}

/// One native read with an explicit positive-support or completed-extension contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRead {
    /// Native statement envelope, independent of physical scheduling internals.
    pub pattern: NativeReadPattern,
    /// Whether present support or final extension is required.
    pub kind: NativeReadKind,
}

/// Non-writing model finalization requires the completed shared world extension.
#[must_use]
pub(crate) fn world_completion_reads() -> Vec<NativeRead> {
    vec![NativeRead::world(NativeReadKind::Completed)]
}

impl NativeRead {
    /// Observe every statement in the selected world.
    #[must_use]
    pub fn world(kind: NativeReadKind) -> Self {
        Self::statement(None, None, None, kind)
    }

    /// Observe one predicate, independently of subject and object.
    #[must_use]
    pub fn predicate(predicate: impl Into<String>, kind: NativeReadKind) -> Self {
        Self::statement(None, Some(predicate.into()), None, kind)
    }

    /// Observe exact ordinary statement positions, with explicit wildcards.
    #[must_use]
    pub fn statement(
        subject: Option<TermValue>,
        predicate: Option<String>,
        object: Option<TermValue>,
        kind: NativeReadKind,
    ) -> Self {
        Self {
            pattern: NativeReadPattern::Statement {
                subject,
                predicate,
                object,
            },
            kind,
        }
    }

    /// Completion of one declaration role, independent of unrelated memberships.
    #[must_use]
    pub fn declaration(predicate: &str, marker: &str) -> Self {
        Self {
            pattern: NativeReadPattern::Declaration {
                subject: None,
                predicate: predicate.to_owned(),
                marker: marker.to_owned(),
            },
            kind: NativeReadKind::Completed,
        }
    }
}

impl NativeReadPattern {
    pub(crate) fn subject(&self) -> Option<&TermValue> {
        match self {
            Self::Statement { subject, .. } | Self::Declaration { subject, .. } => subject.as_ref(),
        }
    }

    pub(crate) fn predicate(&self) -> Option<&str> {
        match self {
            Self::Statement { predicate, .. } => predicate.as_deref(),
            Self::Declaration { predicate, .. } => Some(predicate),
        }
    }

    pub(crate) fn is_world(&self) -> bool {
        matches!(
            self,
            Self::Statement {
                subject: None,
                predicate: None,
                object: None
            }
        )
    }

    /// Completed envelopes must contain every statement selected by the request.
    pub(crate) fn covers(&self, needed: &Self, semantics: SemanticVocabulary) -> bool {
        if self
            .subject()
            .is_some_and(|subject| needed.subject() != Some(subject))
        {
            return false;
        }
        if let Some(predicate) = self.predicate()
            && needed
                .predicate()
                .is_none_or(|other| semantics.predicate(predicate) != semantics.predicate(other))
        {
            return false;
        }
        match (self, needed) {
            (Self::Statement { object: None, .. }, _) => true,
            (
                Self::Statement {
                    object: Some(left), ..
                },
                Self::Statement {
                    object: Some(right),
                    ..
                },
            ) => left == right,
            (
                Self::Declaration {
                    predicate,
                    marker: left,
                    ..
                },
                Self::Declaration { marker: right, .. },
            ) => {
                left == right || semantics.alternate_marker(predicate, left) == Some(right.as_str())
            }
            (
                Self::Declaration {
                    predicate, marker, ..
                },
                Self::Statement {
                    object: Some(TermValue::Iri(iri)),
                    ..
                },
            ) => {
                marker == iri || semantics.alternate_marker(predicate, marker) == Some(iri.as_str())
            }
            _ => false,
        }
    }
}
