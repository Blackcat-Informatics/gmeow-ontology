// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_license::{LicensePolicy, policy_for_license};

/// The retired `test_config.py::test_alignment_targets_policies` spot-checks: the
/// curated targets' licenses classify to the expected reuse policy THROUGH the
/// shared `gmeow_license` classifier (never a hard-coded policy column).
#[test]
fn alignment_target_policies_match_the_retired_spot_checks() {
    for (key, expected) in [
        ("gufo", LicensePolicy::ImportOk),
        ("umbel", LicensePolicy::ImportOk),
        ("foaf", LicensePolicy::ImportOk),
        ("dolce", LicensePolicy::ReferenceOnly),
        ("schema", LicensePolicy::ReferenceOnly),
    ] {
        let (_, license) = target(key).unwrap_or_else(|| panic!("target {key} missing"));
        assert_eq!(policy_for_license(license), expected, "{key} ({license})");
    }
}

/// Non-vacuity: the table is genuinely populated and every curated license
/// classifies (no key is empty, no license panics the classifier).
#[test]
fn every_alignment_target_license_classifies() {
    assert!(
        ALIGNMENT_TARGETS.len() > 10,
        "alignment-target table is implausibly small"
    );
    for (key, _name, license) in ALIGNMENT_TARGETS {
        assert!(!key.is_empty(), "target key must be non-empty");
        let _ = policy_for_license(license);
    }
}
