// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn synthetic_catalogs_record_reviewed_keys_and_reject_malformed_input() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let slice = root.join("slices/control/example/i18n/fr.po");
    std::fs::create_dir_all(slice.parent().unwrap()).unwrap();
    std::fs::create_dir_all(root.join("tests/fixtures/i18n")).unwrap();
    let po = "msgctxt \"urn:term|rdfs:label\"\nmsgid \"Example\"\nmsgstr \"Exemple\"\n";
    std::fs::write(&slice, po).unwrap();
    for language in ["fr", "zh"] {
        std::fs::write(root.join(format!("tests/fixtures/i18n/{language}.po")), po).unwrap();
    }
    let observation = observe(root).unwrap();
    assert_eq!(observation.parsed_catalogs.len(), 3);
    assert_eq!(
        observation.reviewed_coverage["slices/control/example/i18n/fr.po"],
        vec!["urn:term|http://www.w3.org/2000/01/rdf-schema#label"]
    );
    assert_eq!(
        observation.reviewed_coverage,
        reviewed_coverage_map(root).unwrap()
    );
    std::fs::write(&slice, "msgid \"unterminated\n").unwrap();
    assert!(
        observe(root).is_err(),
        "a bad selected catalog cannot disappear"
    );
}
