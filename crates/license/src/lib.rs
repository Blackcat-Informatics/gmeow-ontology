// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW license-token policy classifier — the RUST-FIRST single source of truth.
//!
//! A pure, dependency-free classifier over SPDX-ish license identifiers. One algorithm,
//! two named consumers:
//!   * `gmeow-conformance` — whether a third-party test corpus may be *vendored* into
//!     `cases/external/`.
//!   * the Python `gmeow_tools.config.LinkPolicy` surface — whether an external
//!     vocabulary's *axioms may be copied* into the CC-BY-published GMEOW ontology.
//!
//! Same algorithm, two named consumers; the Python side is a thin marshalling shim over
//! the PyO3 `license_policy_for` entrypoint (in `gmeow-validate`), which delegates here.
//!
//! The classifier is conservative: a restrictive marker (NC/ND/SA/GPL/…) anywhere in the
//! token forces [`LicensePolicy::ReferenceOnly`], even if a permissive substring is
//! present (e.g. `CC-BY-NC-SA`). An unknown license defaults to `ReferenceOnly` so a
//! mistake fails safe (vendoring refused / axiom-copying refused, linking still allowed).

/// Whether a license clears content reuse (vendoring / axiom copying).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicensePolicy {
    /// Compatibly licensed — the content may be reused (vendored / axioms copied).
    ImportOk,
    /// Restrictive / unknown — content reuse is refused; the source may still be
    /// referenced by IRI (which copies nothing).
    ReferenceOnly,
}

/// License-id tokens (uppercased) that block content reuse.
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

/// License-id tokens (uppercased) explicitly cleared for content reuse.
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

/// Classify a license identifier into a reuse policy.
///
/// Restrictive markers win over any permissive substring; the bare `CC-BY-<version>`
/// family is cleared when it carries no `SA`/`NC`; everything unrecognised fails safe
/// to [`LicensePolicy::ReferenceOnly`].
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
    // check mirrors the belt-and-suspenders guard).
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
