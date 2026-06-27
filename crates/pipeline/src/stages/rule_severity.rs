// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-rule severity gradient: `gmeow:ruleSeverity`.
//!
//! Every harvested rule and generated shape may declare whether it is
//! **binding** (compliance — a broken rule is an error) or **advisory** (advice
//! — a recommendation the consumer may decline). The declaration projects onto
//! two delivery lanes:
//!
//! * the **SHACL/shape lane** — `shacl_token` emits `sh:Violation` (binding) or
//!   `sh:Warning` (advisory) into generated shapes; the SHACL→diagnostics map
//!   then surfaces those as Error / Warning respectively, and
//! * the **harvested-rule diagnostic lane** — `diagnostic_severity` emits an
//!   `Error` (binding) or a `Note` (advisory, the soft advisory tier) finding
//!   directly, without a SHACL round-trip.
//!
//! Absent `gmeow:ruleSeverity` is the defined default: **binding**. A hard rule
//! stays hard unless it explicitly opts into advice — no existing axiom changes
//! behaviour. An unrecognized non-empty literal is a hard error: the value set
//! is closed, so a typo is a modeling mistake, never a silently-coerced default.

use gmeow_diagnostics::Severity;

/// The binding-vs-advisory severity tier declared by `gmeow:ruleSeverity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSeverity {
    /// Compliance: a broken rule is a violation / error.
    Binding,
    /// Advice: a recommendation, surfaced softly (warning / note).
    Advisory,
}

impl RuleSeverity {
    /// Parse a `gmeow:ruleSeverity` literal.
    ///
    /// * `None` (no declaration) → [`RuleSeverity::Binding`] — the defined default.
    /// * `"binding"` → [`RuleSeverity::Binding`].
    /// * `"advisory"` → [`RuleSeverity::Advisory`].
    ///
    /// Matching is case-insensitive and trims surrounding whitespace. Any other
    /// non-empty literal is rejected (hard fail): the value set is closed.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None => Ok(Self::Binding),
            Some(raw) => match raw.trim().to_ascii_lowercase().as_str() {
                "binding" => Ok(Self::Binding),
                "advisory" => Ok(Self::Advisory),
                other => Err(format!(
                    "unknown gmeow:ruleSeverity `{other}`; expected binding or advisory"
                )),
            },
        }
    }

    /// SHACL severity IRI token for generated shapes.
    pub fn shacl_token(self) -> &'static str {
        match self {
            Self::Binding => "sh:Violation",
            Self::Advisory => "sh:Warning",
        }
    }

    /// Diagnostic severity for findings emitted directly from a harvested rule
    /// (the soft advisory tier, no SHACL round-trip).
    pub fn diagnostic_severity(self) -> Severity {
        match self {
            Self::Binding => Severity::Error,
            Self::Advisory => Severity::Note,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_defaults_to_binding() {
        assert_eq!(RuleSeverity::parse(None).unwrap(), RuleSeverity::Binding);
    }

    #[test]
    fn explicit_binding_parses() {
        assert_eq!(
            RuleSeverity::parse(Some("binding")).unwrap(),
            RuleSeverity::Binding
        );
    }

    #[test]
    fn explicit_advisory_parses() {
        assert_eq!(
            RuleSeverity::parse(Some("advisory")).unwrap(),
            RuleSeverity::Advisory
        );
    }

    #[test]
    fn parse_trims_and_ignores_case() {
        assert_eq!(
            RuleSeverity::parse(Some("  Advisory ")).unwrap(),
            RuleSeverity::Advisory
        );
        assert_eq!(
            RuleSeverity::parse(Some("BINDING")).unwrap(),
            RuleSeverity::Binding
        );
    }

    #[test]
    fn unknown_literal_hard_fails() {
        assert!(RuleSeverity::parse(Some("violation")).is_err());
        assert!(RuleSeverity::parse(Some("warn")).is_err());
        assert!(RuleSeverity::parse(Some("")).is_err());
    }

    #[test]
    fn shacl_token_maps_both_tiers() {
        assert_eq!(RuleSeverity::Binding.shacl_token(), "sh:Violation");
        assert_eq!(RuleSeverity::Advisory.shacl_token(), "sh:Warning");
    }

    #[test]
    fn diagnostic_severity_maps_both_tiers() {
        assert_eq!(RuleSeverity::Binding.diagnostic_severity(), Severity::Error);
        assert_eq!(RuleSeverity::Advisory.diagnostic_severity(), Severity::Note);
    }
}
