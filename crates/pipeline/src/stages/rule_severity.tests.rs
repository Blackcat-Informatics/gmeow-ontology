// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
