// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The evaluator's typed error channel.
//!
//! Per the project `no-optionality` / hard-fail doctrine, every condition that is
//! not a valid in-scope result is a typed error — there is no lenient mode, no
//! partial solution sequence, and no silent degradation. An out-of-S6-scope
//! algebra node or an unimplemented builtin is [`EvalError::Unsupported`], not a
//! best-effort answer.

use gmeow_sparql_algebra::ParseError;

/// An error raised while evaluating a SPARQL query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    /// A query failed to parse in [`gmeow_sparql_algebra`]. Carries the rendered
    /// parse error.
    Parse(String),

    /// A well-formed but out-of-scope algebra node, query form, or builtin.
    ///
    /// This is the hard-fail boundary: `SERVICE`, `LATERAL`, `DESCRIBE`, SPARQL
    /// `UPDATE`, and not-yet-implemented builtins all surface here rather than being
    /// partially evaluated. The string names the unsupported construct. (Property
    /// paths are now evaluated in-engine — S8 #914 — so they are no longer here.)
    Unsupported(String),

    /// An internal invariant was violated — e.g. a solution row whose width does
    /// not match its schema. This indicates a bug in the evaluator, not bad input
    /// (a frozen, validated dataset and a parsed algebra cannot legitimately cause
    /// it); it is surfaced rather than panicking so callers fail cleanly.
    Internal(String),

    /// A `SERVICE` federation step failed (transport error, undecodable remote
    /// response, or no remote source configured) and the `SERVICE` was **not**
    /// `SILENT`. Per the hard-fail doctrine a non-silent federation failure aborts
    /// the query rather than silently contributing no bindings; `SERVICE SILENT`
    /// instead swallows the failure to the join identity (S6b #928).
    Remote(String),
}

impl EvalError {
    /// Construct an [`EvalError::Unsupported`] from any displayable construct name.
    pub fn unsupported(what: impl Into<String>) -> Self {
        Self::Unsupported(what.into())
    }

    /// Construct an [`EvalError::Internal`] from any displayable message.
    pub fn internal(what: impl Into<String>) -> Self {
        Self::Internal(what.into())
    }

    /// Construct an [`EvalError::Remote`] from any displayable message.
    pub fn remote(what: impl Into<String>) -> Self {
        Self::Remote(what.into())
    }
}

impl core::fmt::Display for EvalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EvalError::Parse(msg) => write!(f, "SPARQL parse error: {msg}"),
            EvalError::Unsupported(what) => {
                write!(f, "unsupported in sparql-eval (S6 scope): {what}")
            }
            EvalError::Internal(msg) => write!(f, "internal evaluator error: {msg}"),
            EvalError::Remote(msg) => write!(f, "SERVICE federation error: {msg}"),
        }
    }
}

impl std::error::Error for EvalError {}

impl From<ParseError> for EvalError {
    fn from(err: ParseError) -> Self {
        EvalError::Parse(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_converts_and_renders() {
        let pe = ParseError::Unsupported("VALUES with mixed arity".to_owned());
        let ee: EvalError = pe.into();
        assert!(matches!(ee, EvalError::Parse(_)));
        assert!(ee.to_string().contains("parse error"));
    }

    #[test]
    fn unsupported_names_the_construct() {
        let e = EvalError::unsupported("SERVICE");
        assert!(e.to_string().contains("SERVICE"));
        assert!(e.to_string().contains("scope"));
    }
}
