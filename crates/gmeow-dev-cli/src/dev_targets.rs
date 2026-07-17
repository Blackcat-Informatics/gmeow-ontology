// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The curated alignment-target license table — the `(key, name, license)` data
//! the `extract` license guard keys on.
//!
//! This mirrors `gmeow_tools.config.ALIGNMENT_TARGETS`; the reuse *policy* itself
//! is derived from the license by the shared `gmeow_license::policy_for_license`
//! classifier (never hard-coded here), so a license change flips the policy.

/// `(key, display name, license id)` for every curated alignment target.
pub const ALIGNMENT_TARGETS: &[(&str, &str, &str)] = &[
    ("gufo", "gUFO", "MIT"),
    ("ontouml", "OntoUML", "CC-BY-SA-4.0"),
    ("umbel", "UMBEL", "CC-BY-3.0"),
    ("dolce", "DOLCE/DUL", "LGPL"),
    ("bfo", "BFO", "CC-BY-4.0"),
    ("foaf", "FOAF", "CC-BY-1.0"),
    ("rel", "REL (Relationship)", "CC-BY-1.0"),
    ("doap", "DOAP", "Public-Domain"),
    ("prov", "PROV-O", "W3C-Document"),
    ("dqv", "W3C DQV", "W3C-Document"),
    ("org", "ORG", "PDDL-1.0"),
    ("time", "OWL-Time", "CC-BY-4.0"),
    ("schema", "Schema.org", "CC-BY-SA-3.0"),
    ("dcterms", "DCMI Metadata Terms", "CC0-1.0"),
    ("mo", "Music Ontology", "Unknown"),
    ("mbz", "MusicBrainz", "CC0-1.0"),
    ("discogs", "Discogs", "REFERENCE_ONLY"),
    ("afo", "Audio Feature Ontology", "Unknown"),
    ("afv", "Audio Feature Vocabulary", "Unknown"),
    ("jams", "JAMS Annotation Vocabulary", "Unknown"),
    ("pon", "Polifonia Ontology Network", "Unknown"),
    ("chord", "OMRAS2 Chord Ontology", "Unknown"),
    ("gedcom", "W3C GEDCOM", "W3C-Document"),
    ("vcard", "vCard", "W3C-Document"),
    ("geo", "GeoSPARQL", "OGC"),
    ("wgs84", "WGS84 Geo Positioning", "W3C-Document"),
    ("tgn", "Getty TGN", "ODC-BY-1.0"),
    ("gvp", "Getty Vocabulary Program", "ODC-BY-1.0"),
    ("frbr", "FRBRcore", "CC-BY-3.0"),
    ("fabio", "FaBiO", "CC-BY-3.0"),
    ("lrmoo", "LRMoo", "CC-BY-3.0"),
    ("bibo", "BIBO", "CC-BY-3.0"),
    ("bibframe", "BIBFRAME", "CC0-1.0"),
    ("sioc", "SIOC", "W3C-Document"),
    ("skos", "SKOS", "W3C-Document"),
    ("nmo", "Nepomuk Message Ontology", "Unknown"),
    ("wot", "WOT Schema", "Unknown"),
    ("odrl", "ODRL 2.2", "W3C-Document"),
    ("cc", "CC REL", "CC-BY-4.0"),
    ("premis", "PREMIS 3", "CC-BY-4.0"),
    ("rstmt", "RightsStatements.org", "CC0-1.0"),
    ("spdx", "SPDX", "CC-BY-3.0"),
    ("spdxlic", "SPDX License List", "CC0-1.0"),
    ("codemeta", "CodeMeta", "Apache-2.0"),
    ("forgefed", "ForgeFed", "CC0-1.0"),
    ("ma", "Ontology for Media Resources", "W3C-Document"),
    (
        "gsso",
        "Gender, Sex, and Sexual Orientation Ontology",
        "CC-BY-NC-ND 4.0",
    ),
    ("homosaurus", "Homosaurus", "CC-BY-4.0"),
    ("fhir", "HL7 FHIR", "CC0-1.0"),
    ("bio", "BIO vocabulary", "CC-BY-3.0"),
    ("gedcomx", "GEDCOM X", "Apache-2.0"),
    ("geonames", "GeoNames", "CC-BY-4.0"),
    ("wikidata", "Wikidata", "CC0-1.0"),
    ("lexvo", "Lexvo", "CC-BY-SA-3.0"),
    ("glottolog", "Glottolog", "CC-BY-4.0"),
    ("ontolex", "OntoLex-Lemon", "W3C-Document"),
    ("lime", "LIME", "W3C-Document"),
    ("qudt", "QUDT", "CC-BY-4.0"),
    ("gtfs", "GTFS", "CC-BY-3.0"),
    ("fibo-fnd-acc-cur", "FIBO CurrencyAmount", "MIT"),
    ("fibo-iso4217", "FIBO ISO4217 Currency Codes", "MIT"),
    ("fibo-fnd-acc-ae", "FIBO AccountingEquity", "MIT"),
    ("fibo-fbc-fi-fi", "FIBO FinancialInstruments", "MIT"),
];

/// The `(name, license)` for a target key, if curated.
pub fn target(key: &str) -> Option<(&'static str, &'static str)> {
    ALIGNMENT_TARGETS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, name, license)| (*name, *license))
}

#[cfg(test)]
mod tests {
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
}
