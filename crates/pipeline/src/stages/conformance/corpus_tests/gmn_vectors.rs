// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Frozen GMN corpus assertions over authenticated native observations.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use super::super::gmn_vectors::{CHANNEL, Observation, ROOT};

fn observation() -> &'static Observation {
    static OBSERVATION: OnceLock<
        Result<super::source_artifact::Selected<Observation>, gmeow_errors::Diag>,
    > = OnceLock::new();
    super::source_artifact::get(&OBSERVATION, CHANNEL)
}

#[test]
fn complete_frozen_vectors_preserve_bytes_witnesses_and_negative_classes() {
    let observed = observation();
    assert_eq!(
        observed.manifest_stems.len(),
        19,
        "complete committed vector inventory"
    );
    assert_eq!(
        observed.manifest_stems,
        observed.positives.keys().cloned().collect()
    );
    assert_eq!(
        observed.codebook_digest,
        "blake3:26a68453ecfe47867551038b8b247e9f4b07b3815bd87979c401afb0f7edf5ce"
    );
    assert_eq!(
        observed.declared_digests,
        BTreeSet::from([observed.codebook_digest.clone()])
    );
    let root = gmeow_conformance::paths::repo_root();
    for (stem, result) in &observed.positives {
        let result = result
            .as_ref()
            .unwrap_or_else(|error| panic!("positive {stem}: {error}"));
        let frozen = std::fs::read(root.join(ROOT).join(format!("{stem}.gmn")))
            .expect("frozen expected bytes");
        assert!(
            result.failures(&frozen).is_empty(),
            "{stem}: {:?}",
            result.failures(&frozen)
        );
    }
    assert_eq!(
        observed.expected_negatives.len(),
        8,
        "complete codec negative inventory"
    );
    assert_eq!(
        observed.expected_negatives.keys().collect::<Vec<_>>(),
        observed.negatives.keys().collect::<Vec<_>>()
    );
    for (filename, class) in &observed.expected_negatives {
        assert_eq!(
            observed.negatives[filename]
                .as_ref()
                .expect("executed codec negative")
                .as_ref(),
            Some(class),
            "{filename}"
        );
    }
    let basic = observed.positives["claim-basic"].as_ref().unwrap();
    assert!(basic.content_digest.starts_with("blake3:"));
    assert!(basic.reconstructed.as_ref().unwrap().contains(
        "<https://blackcatinformatics.ca/gmeow/gate1> <https://blackcatinformatics.ca/gmeow/hasState> <https://blackcatinformatics.ca/gmeow/doorGate1> ."
    ));
    // Grading altered expected bytes never executes a vector or copies a corpus.
    let mut corrupted = basic.text.as_bytes().to_vec();
    corrupted.extend_from_slice(b"CORRUPT");
    assert!(
        basic
            .failures(&corrupted)
            .iter()
            .any(|failure| failure.contains("byte mismatch"))
    );
    assert_eq!(observed.positives.len() - 1, 18);
}
