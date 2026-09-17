// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfQuad, RdfTerm};

#[test]
fn independent_byte_and_semantic_failures_survive_observation_transport() {
    let model = Gmn0Model {
        quads: vec![RdfQuad::new(
            RdfTerm::iri("https://blackcatinformatics.ca/gmeow/cliSubject"),
            "https://blackcatinformatics.ca/gmeow/cliPredicate",
            RdfTerm::iri("https://blackcatinformatics.ca/gmeow/cliObject"),
        )],
    };
    let observed = observe_positive(&model, &GmnDictionary::default()).unwrap();
    let expected = b"@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s: gmeow__cliSubject, p: gmeow__cliPredicate, o: gmeow__cliObject}\n";
    assert!(observed.failures(expected).is_empty(), "{observed:?}");
    assert_eq!(observed.content_digest, crate::content_digest(&model));
    let bytes = serde_json::to_vec(&observed).unwrap();
    let mut restored: PositiveObservation = serde_json::from_slice(&bytes).unwrap();
    assert!(restored.failures(expected).is_empty());
    assert_eq!(restored.failures(b"corrupted bytes").len(), 1);
    restored.reconstructed = Ok(String::new());
    restored.per_claim = Err(record_failure(std::io::Error::other(
        "missing claim identity",
    )));
    restored.idempotence = Err(record_failure(std::io::Error::other(
        "different reference payload",
    )));
    assert_eq!(restored.failures(b"corrupted bytes").len(), 4);
}
