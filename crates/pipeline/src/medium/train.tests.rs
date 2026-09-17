// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A corpus with enough repeated structure for FastCOVER to actually train on.
/// Shaped like the RDF the real dictionaries see, so the test exercises the
/// production path rather than a degenerate one.
fn sample_corpus() -> Vec<Vec<u8>> {
    (0..400u32)
        .map(|i| {
            format!(
                "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {} in the gmeow ontology\" .\n",
                i % 37,
                i
            )
            .into_bytes()
        })
        .collect()
}

fn slices(owned: &[Vec<u8>]) -> Vec<&[u8]> {
    owned.iter().map(Vec::as_slice).collect()
}

/// GMEOW's two raw-content strategies select the same producer, and every
/// declared strategy must return a dictionary our typed ID adapter admits.
#[test]
fn declared_strategies_return_finalized_dictionaries_and_share_raw_term_training() {
    let owned = sample_corpus();
    let corpus = slices(&owned);
    let dictionaries = [
        DictionaryStrategy::Trained,
        DictionaryStrategy::RawContent,
        DictionaryStrategy::TermTable,
    ]
    .map(|strategy| {
        let dictionary = build(strategy, &corpus, 4096).expect("declared strategy builds");
        assert_ne!(
            zstd_dictionary_id(&dictionary).expect("GMEOW admits the finalized dictionary"),
            0
        );
        dictionary
    });
    assert_eq!(
        dictionaries[1], dictionaries[2],
        "term-table and raw-content strategy dispatch agree"
    );
}

/// An empty corpus is a HARD FAIL, never "no dictionary": a frame primed with
/// the id this dictionary was supposed to carry would be undecodable.
#[test]
fn an_empty_corpus_hard_fails() {
    let error = build(DictionaryStrategy::Trained, &[], 4096)
        .expect_err("an empty corpus must be rejected");
    assert_eq!(
        error.code(),
        crate::error::MediumUndeclaredDictionary::register()
    );
}
