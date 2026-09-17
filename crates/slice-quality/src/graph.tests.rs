// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const SUBJECT: &str = "https://example.test/tier";

/// Parse one compact multilingual label fixture.
fn dataset(labels: &str) -> std::sync::Arc<RdfDataset> {
    let turtle = format!(
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <{SUBJECT}> {labels} .\n"
    );
    crate::dataset_from_documents(&[("labels.ttl", turtle.as_bytes())])
        .expect("label fixture parses")
}

#[test]
/// Carrier English wins regardless of source quad order without deleting translations.
fn carrier_english_wins_independently_of_quad_order() {
    let forward = dataset("rdfs:label \"Lie\"@fr, \"Public\"@en, \"Carrier\"@x-gmeow-english");
    let reverse = dataset("rdfs:label \"Carrier\"@x-gmeow-english, \"Public\"@en, \"Lie\"@fr");
    for ds in [&forward, &reverse] {
        let subject = id(ds, SUBJECT).expect("subject interned");
        assert_eq!(label_of(ds, subject), "Carrier");
        let label = id(ds, RDFS_LABEL).expect("label predicate interned");
        assert_eq!(all_lits(ds, subject, label).len(), 3, "labels are retained");
    }
}

#[test]
/// Public English precedes neutral and non-English literals when carrier English is absent.
fn public_english_precedes_neutral_and_other_languages() {
    let ds = dataset("rdfs:label \"Zulu\"@zu, \"Neutral\", \"English\"@en");
    let subject = id(&ds, SUBJECT).expect("subject interned");
    assert_eq!(label_of(&ds, subject), "English");
}

#[test]
/// Other-language fallback is total-ordered independently of parser insertion order.
fn non_english_fallback_is_a_stable_total_order() {
    let forward = dataset("rdfs:label \"Zulu\"@zu, \"Francais\"@fr");
    let reverse = dataset("rdfs:label \"Francais\"@fr, \"Zulu\"@zu");
    for ds in [&forward, &reverse] {
        let subject = id(ds, SUBJECT).expect("subject interned");
        assert_eq!(label_of(ds, subject), "Francais");
    }
}
