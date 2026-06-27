// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native (Rust) license/vendoring policy for external corpora (#753).
//!
//! A renamed Rust **subsumption** of the Python `gmeow_tools.config.LinkPolicy` /
//! `policy_for_license` classifier: the project is RUST-FIRST and adding a Python
//! dependency to the conformance crate is a `.goals` violation, so the IMPORT_OK /
//! REFERENCE_ONLY sets and the conservative matcher are re-implemented here. The
//! intent differs from the Python original (which governs *axiom copying into the
//! CC-BY ontology*): here it governs *whether a third-party test corpus may be
//! vendored* into `cases/external/`. Same algorithm, different consumer.
//!
//! The classifier is conservative: a restrictive marker (NC/ND/SA/GPL/…) anywhere
//! in the token forces [`LicensePolicy::ReferenceOnly`], even if a permissive
//! substring is present (e.g. `CC-BY-NC-SA`). An unknown license defaults to
//! `ReferenceOnly` so a mistake fails safe (vendoring refused).

/// Whether a third-party corpus may be vendored into the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicensePolicy {
    /// Compatibly licensed — the corpus may be vendored under `cases/external/`.
    ImportOk,
    /// Restrictive / unknown — vendoring is refused; the corpus may only be fetched
    /// live in the Lane-B (non-required, Docker) lane and never committed.
    ReferenceOnly,
}

/// License-id tokens (uppercased) that block vendoring into the repository.
/// Non-commercial, no-derivatives, share-alike, and copyleft software licenses.
const REFERENCE_ONLY_MARKERS: [&str; 8] = [
    "NC",          // non-commercial
    "ND",          // no-derivatives
    "SA",          // share-alike
    "GPL",         // GPL / LGPL / AGPL copyleft
    "EUPL",        // European Union Public License (copyleft)
    "PROPRIETARY", //
    "INTERNAL",    //
    "ACADEMIC",    //
];

/// License-id tokens (uppercased) explicitly cleared for vendoring.
const IMPORT_OK_LICENSES: [&str; 22] = [
    "CC0",
    "CC0-1.0",
    "CC-BY",
    "CC-BY-1.0",
    "CC-BY-3.0",
    "CC-BY-4.0",
    "MIT",
    "APACHE-2.0",
    "BSD-2-CLAUSE",
    "BSD-3-CLAUSE",
    "PDDL-1.0",
    "PDDL",
    "ODC-BY-1.0",
    "ODC-BY",
    "PUBLIC-DOMAIN",
    "PUBLIC DOMAIN",
    "W3C",
    "W3C-DOCUMENT",
    "OGC",
    "NIST-PUBLIC-DOMAIN",
    "NIST PUBLIC DOMAIN",
    "UNLICENSE",
];

/// Classify a license identifier into a vendoring policy.
///
/// Faithful port of `gmeow_tools.config.policy_for_license`: restrictive markers
/// win over any permissive substring; the bare `CC-BY-<version>` family is cleared
/// when it carries no `SA`/`NC`; everything unrecognised fails safe to
/// [`LicensePolicy::ReferenceOnly`].
pub fn policy_for_license(license_id: &str) -> LicensePolicy {
    let token = license_id.trim().to_uppercase();
    // Restrictive markers win, regardless of any permissive substring.
    for marker in REFERENCE_ONLY_MARKERS {
        if has_marker_segment(&token, marker) {
            return LicensePolicy::ReferenceOnly;
        }
    }
    if IMPORT_OK_LICENSES.contains(&token.as_str()) {
        return LicensePolicy::ImportOk;
    }
    // Bare "CC-BY" with a version suffix not already listed (substring `SA`/`NC`
    // check mirrors the Python belt-and-suspenders guard).
    if token.starts_with("CC-BY-") && !token.contains("SA") && !token.contains("NC") {
        return LicensePolicy::ImportOk;
    }
    LicensePolicy::ReferenceOnly
}

/// Whether `marker` appears as a `-`/`_`/space-delimited segment of `token`.
///
/// Segment matching means `ND` does NOT spuriously match inside `PUBLIC-DOMAIN`.
/// The GPL family also matches as a suffix segment (`LGPL`, `AGPL-3.0`).
fn has_marker_segment(token: &str, marker: &str) -> bool {
    let normalized = token.replace(['_', ' '], "-");
    let segments: Vec<&str> = normalized.split('-').collect();
    if segments.contains(&marker) {
        return true;
    }
    marker == "GPL" && segments.iter().any(|seg| seg.ends_with("GPL"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissive_licenses_are_import_ok() {
        for id in [
            "CC0-1.0",
            "CC-BY-4.0",
            "MIT",
            "Apache-2.0",
            "BSD-3-Clause",
            "PDDL-1.0",
            "ODC-BY-1.0",
            "W3C",
            "W3C-Document",
            "Unlicense",
        ] {
            assert_eq!(policy_for_license(id), LicensePolicy::ImportOk, "{id}");
        }
    }

    #[test]
    fn public_domain_is_import_ok_despite_nd_substring() {
        // The `ND` marker must NOT match inside "PUBLIC-DOMAIN" (segment, not substring).
        assert_eq!(policy_for_license("PUBLIC-DOMAIN"), LicensePolicy::ImportOk);
        assert_eq!(policy_for_license("public domain"), LicensePolicy::ImportOk);
    }

    #[test]
    fn restrictive_markers_win_over_permissive_substring() {
        // CC-BY-NC-SA contains the permissive "CC-BY" substring but is reference-only.
        assert_eq!(
            policy_for_license("CC-BY-NC-SA-4.0"),
            LicensePolicy::ReferenceOnly
        );
        assert_eq!(
            policy_for_license("CC-BY-SA-4.0"),
            LicensePolicy::ReferenceOnly
        );
        assert_eq!(
            policy_for_license("CC-BY-ND-4.0"),
            LicensePolicy::ReferenceOnly
        );
    }

    #[test]
    fn gpl_family_suffix_rule() {
        for id in ["GPL-3.0", "LGPL-2.1", "AGPL-3.0", "EUPL-1.2"] {
            assert_eq!(policy_for_license(id), LicensePolicy::ReferenceOnly, "{id}");
        }
    }

    #[test]
    fn cc_by_version_fallthrough() {
        // An unlisted CC-BY version with no SA/NC is import-ok.
        assert_eq!(policy_for_license("CC-BY-2.5"), LicensePolicy::ImportOk);
        assert_eq!(policy_for_license("CC-BY-2.0"), LicensePolicy::ImportOk);
    }

    #[test]
    fn unknown_license_fails_safe_to_reference_only() {
        assert_eq!(policy_for_license("WTFPL"), LicensePolicy::ReferenceOnly);
        assert_eq!(policy_for_license(""), LicensePolicy::ReferenceOnly);
        assert_eq!(
            policy_for_license("Some-Proprietary-EULA"),
            LicensePolicy::ReferenceOnly
        );
    }
}
