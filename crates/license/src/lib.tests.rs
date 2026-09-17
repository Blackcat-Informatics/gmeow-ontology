// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
