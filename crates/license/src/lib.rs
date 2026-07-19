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

/// The descriptor of a vendored external corpus — the `corpus.json` fields that bear on the
/// reuse policy. The vendoring CATEGORY below is keyed off THESE descriptor fields, never off
/// a filesystem path, so it is reusable for any future vendored corpus with no policy churn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendoredCorpus<'a> {
    /// SPDX license identifier of the vendored artifacts.
    pub spdx_license: &'a str,
    /// The upstream source URL the attribution points at (the provenance of the vendored
    /// bytes). Must be non-empty for the share-alike vendoring category.
    pub source_url: &'a str,
    /// The human attribution the vendored corpus credits (authors / project). Must be
    /// non-empty for the share-alike vendoring category.
    pub attribution: &'a str,
    /// Whether the corpus is RING-FENCED: committed as a clearly-separated, non-published
    /// vendored fragment that is NEVER merged into the CC-BY GMEOW vocabulary.
    pub ring_fenced: bool,
}

/// Classify a vendored corpus into a reuse policy, admitting the ONE additional category the
/// bare-token classifier cannot express: a **ring-fenced, fully-attributed CC BY-SA
/// share-alike corpus**.
///
/// Share-alike (`SA`) is [`LicensePolicy::ReferenceOnly`] as a bare token because copying such
/// content into the CC-BY-published GMEOW vocabulary would violate the copyleft. But a corpus
/// that is (a) ring-fenced — never merged into the published vocabulary — AND (b) fully
/// attributed — a credited source URL and attribution line — honours CC BY-SA 4.0's own terms,
/// so it clears vendoring as [`LicensePolicy::ImportOk`].
///
/// The exception is gated on the CC-BY-SA share-alike family SPECIFICALLY and does NOT loosen
/// any other restrictive license: a non-commercial (`NC`) or no-derivatives (`ND`) CC license,
/// a GPL/EUPL copyleft, or a proprietary/unknown token stays [`LicensePolicy::ReferenceOnly`]
/// regardless of ring-fencing or attribution.
pub fn policy_for_vendored_corpus(corpus: &VendoredCorpus) -> LicensePolicy {
    // A token the bare classifier already clears needs no exception.
    if policy_for_license(corpus.spdx_license) == LicensePolicy::ImportOk {
        return LicensePolicy::ImportOk;
    }
    // The share-alike vendoring exception: CC-BY-SA (attribution + share-alike, and NOT
    // NC/ND), ring-fenced, with a non-empty source URL and attribution.
    if is_cc_by_sa(corpus.spdx_license)
        && corpus.ring_fenced
        && !corpus.attribution.trim().is_empty()
        && !corpus.source_url.trim().is_empty()
    {
        return LicensePolicy::ImportOk;
    }
    LicensePolicy::ReferenceOnly
}

/// Whether the token is a Creative-Commons Attribution-ShareAlike license (`CC-BY-SA-*`):
/// a CC attribution + share-alike license carrying NO non-commercial (`NC`) or no-derivatives
/// (`ND`) restriction. These are exactly the licenses the ring-fenced-vendoring category
/// admits; a CC license bearing `NC`/`ND` is deliberately excluded.
fn is_cc_by_sa(license_id: &str) -> bool {
    let token = license_id.trim().to_uppercase().replace(['_', ' '], "-");
    let segments: Vec<&str> = token.split('-').collect();
    segments.first() == Some(&"CC")
        && segments.contains(&"BY")
        && segments.contains(&"SA")
        && !segments.contains(&"NC")
        && !segments.contains(&"ND")
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

    /// The exact input→output rows the retired `test_config.py::test_policy_for_license`
    /// pinned — including the two rows no existing test covered (`CC-BY-NC-ND`, bare
    /// `Proprietary`).
    #[test]
    fn retired_test_config_policy_table_rows() {
        for id in [
            "CC-BY-4.0",
            "CC-BY-3.0",
            "CC0-1.0",
            "MIT",
            "Apache-2.0",
            "PDDL-1.0",
            "ODC-BY-1.0",
            "Public-Domain",
        ] {
            assert_eq!(policy_for_license(id), LicensePolicy::ImportOk, "{id}");
        }
        for id in [
            "CC-BY-SA-3.0",
            "CC-BY-NC-ND 4.0",
            "CC-BY-NC-SA 4.0",
            "GPL-2.0",
            "LGPL",
            "EUPL-1.2",
            "Proprietary",
            "SomethingUnknown",
        ] {
            assert_eq!(policy_for_license(id), LicensePolicy::ReferenceOnly, "{id}");
        }
    }

    /// The FULL policy table is exercised arm-by-arm: since `policy_for_license` is
    /// `&str`-keyed (no compiler-checked exhaustiveness), every `IMPORT_OK_LICENSES`
    /// entry and every `REFERENCE_ONLY_MARKERS` marker gets a pinned case, and the
    /// recognized-arm count is pinned so a NEW arm added without a case fails here.
    #[test]
    fn full_policy_table_is_exercised_arm_by_arm() {
        for id in IMPORT_OK_LICENSES {
            assert_eq!(
                policy_for_license(id),
                LicensePolicy::ImportOk,
                "import-ok arm {id}"
            );
        }
        for marker in REFERENCE_ONLY_MARKERS {
            // The marker as a delimited segment forces ReferenceOnly even under the
            // otherwise-permissive `CC-BY-…` prefix.
            let id = format!("CC-BY-{marker}-4.0");
            assert_eq!(
                policy_for_license(&id),
                LicensePolicy::ReferenceOnly,
                "reference-only marker {marker}"
            );
        }
        assert_eq!(
            policy_for_license("Totally-Unknown-XYZ"),
            LicensePolicy::ReferenceOnly,
            "unknown fails safe"
        );
        // Pin the recognized-arm count: a new arm added without a test case above
        // (which would change one of these lengths) trips this assertion.
        assert_eq!(IMPORT_OK_LICENSES.len(), 22, "import-ok arm count");
        assert_eq!(
            REFERENCE_ONLY_MARKERS.len(),
            8,
            "reference-only marker count"
        );
    }

    #[test]
    fn ring_fenced_attributed_cc_by_sa_is_import_ok() {
        // The Gate-2 vendored UD fragment: CC BY-SA 4.0 is ReferenceOnly as a bare token
        // (share-alike), but ring-fenced + fully attributed it clears vendoring.
        assert_eq!(
            policy_for_license("CC-BY-SA-4.0"),
            LicensePolicy::ReferenceOnly,
            "bare share-alike is reference-only"
        );
        let ok = VendoredCorpus {
            spdx_license: "CC-BY-SA-4.0",
            source_url: "https://raw.githubusercontent.com/UniversalDependencies/\
                         UD_English-EWT/master/en_ewt-ud-dev.conllu",
            attribution: "Universal Dependencies English EWT — the UD project and treebank authors",
            ring_fenced: true,
        };
        assert_eq!(policy_for_vendored_corpus(&ok), LicensePolicy::ImportOk);
    }

    #[test]
    fn unattributed_or_unfenced_cc_by_sa_hard_fails() {
        // Missing attribution → rejected.
        let no_attr = VendoredCorpus {
            spdx_license: "CC-BY-SA-4.0",
            source_url: "https://example.org/treebank.conllu",
            attribution: "   ",
            ring_fenced: true,
        };
        assert_eq!(
            policy_for_vendored_corpus(&no_attr),
            LicensePolicy::ReferenceOnly
        );
        // Missing source URL → rejected.
        let no_url = VendoredCorpus {
            spdx_license: "CC-BY-SA-4.0",
            source_url: "",
            attribution: "The UD project",
            ring_fenced: true,
        };
        assert_eq!(
            policy_for_vendored_corpus(&no_url),
            LicensePolicy::ReferenceOnly
        );
        // Not ring-fenced → rejected.
        let leaky = VendoredCorpus {
            spdx_license: "CC-BY-SA-4.0",
            source_url: "https://example.org/treebank.conllu",
            attribution: "The UD project",
            ring_fenced: false,
        };
        assert_eq!(
            policy_for_vendored_corpus(&leaky),
            LicensePolicy::ReferenceOnly
        );
    }

    #[test]
    fn ring_fencing_does_not_loosen_other_restrictive_licenses() {
        // Even fully ring-fenced + attributed, a non-commercial / no-derivatives CC license, a
        // GPL/EUPL copyleft, or a proprietary/unknown token stays ReferenceOnly — the category
        // admits the CC-BY-SA share-alike family SPECIFICALLY, nothing else.
        for id in [
            "CC-BY-NC-SA-4.0",
            "CC-BY-ND-4.0",
            "CC-BY-NC-4.0",
            "GPL-3.0",
            "AGPL-3.0",
            "EUPL-1.2",
            "Some-Proprietary-EULA",
        ] {
            let d = VendoredCorpus {
                spdx_license: id,
                source_url: "https://example.org/x",
                attribution: "credited authors",
                ring_fenced: true,
            };
            assert_eq!(
                policy_for_vendored_corpus(&d),
                LicensePolicy::ReferenceOnly,
                "{id}"
            );
        }
    }
}
