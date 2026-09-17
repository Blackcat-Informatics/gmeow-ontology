// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::medium::envelope::{DigestStratum, FrameFacts, seal};
use crate::medium::registry::{fixture, gm as reg_gm};
use crate::medium::train;

fn registry() -> MediumRegistry {
    MediumRegistry::from_dataset(&fixture::dataset("")).expect("fixture registry")
}

/// The MEASURED point a fixture realization records: the strategy under test at
/// the fixture's declared target, over a corpus of known size.
fn measured(strategy: DictionaryStrategy) -> Measured {
    Measured {
        strategy,
        target_length: 4096,
        corpus_sample_count: 400,
        corpus_digest: blake3_digest(b"the fixture corpus resolution"),
    }
}

fn trained_bytes() -> Vec<u8> {
    let owned: Vec<Vec<u8>> = (0..400u32)
        .map(|i| {
            format!(
                "<https://blackcatinformatics.ca/gmeow/t{}> <https://e/p> \"v{i}\" .\n",
                i % 31
            )
            .into_bytes()
        })
        .collect();
    let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    train::build(DictionaryStrategy::Trained, &corpus, 4096).expect("train")
}

#[test]
fn a_realization_carries_every_measured_field() {
    let registry = registry();
    let def = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("declared");
    let bytes = trained_bytes();
    let realization = realize(def, &bytes, measured(DictionaryStrategy::Trained)).expect("realize");
    assert_eq!(realization.byte_length, bytes.len());
    assert!(is_canonical_digest(&realization.content_digest));
    assert!(
        is_canonical_digest(&realization.corpus_digest),
        "the resolved-corpus identity rides on the shipped realization"
    );
    assert_ne!(realization.zstd_dictionary_id, 0);
    // The MEASURED strategy is its own field: a trainer that fell back must be
    // able to say so without touching the authored definition.
    let fell_back =
        realize(def, &bytes, measured(DictionaryStrategy::RawContent)).expect("realize");
    assert_eq!(fell_back.strategy, DictionaryStrategy::RawContent);
    assert_eq!(def.strategy, DictionaryStrategy::Trained);
}

#[test]
fn the_projection_lands_entirely_in_the_medium_registry_graph() {
    let registry = registry();
    let def = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("declared");
    let realization =
        realize(def, &trained_bytes(), measured(DictionaryStrategy::Trained)).expect("realize");
    let envelope = seal(
        &registry,
        &crate::medium::registry::MediumSelection::Authored,
        &FrameFacts {
            frame: "https://e/frame7",
            rep: "cells-archive",
            payload: b"payload",
            stratum_bytes: b"stratum",
            stratum: DigestStratum::PayloadExcludingMediumEnvelope,
            dictionary_id: Some("gmeow-core-v1"),
        },
    )
    .expect("seal");

    let quads = project(&registry, &[realization], &[envelope]).expect("project");
    assert!(!quads.is_empty());
    assert!(
        quads
            .iter()
            .all(|q| q.graph_name == Some(RdfTerm::iri(MEDIUM_REGISTRY_GRAPH))),
        "every projected quad lives in the build-time registry graph"
    );
    let predicates: Vec<&str> = quads.iter().map(|q| q.predicate.as_str()).collect();
    for required in [
        "realizesDictionary",
        "dictionaryContentDigest",
        "dictionaryByteLength",
        "zstdDictionaryId",
        "envelopeSchema",
        "envelopeMedium",
        "envelopeDictionary",
        "envelopeDigestStratum",
        "strataDigest",
    ] {
        assert!(
            predicates.contains(&reg_gm(required).as_str()),
            "the projection must carry gmeow:{required}"
        );
    }
}

/// The projection is a pure function of the registry, not of the caller's
/// emission order — two shuffles of the same records produce identical quads.
#[test]
fn the_projection_is_emission_order_independent() {
    let registry = registry();
    let core = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("declared");
    let terms = registry
        .dictionary_by_id("gmeow-terms-v1")
        .expect("declared");
    let bytes = trained_bytes();
    let a = realize(core, &bytes, measured(DictionaryStrategy::Trained)).expect("realize");
    let b = realize(terms, &bytes, measured(DictionaryStrategy::TermTable)).expect("realize");

    let forward = project(&registry, &[a.clone(), b.clone()], &[]).expect("project");
    let reversed = project(&registry, &[b, a], &[]).expect("project");
    assert_eq!(forward, reversed);
}

/// Retiring a shipped version orphans every artifact primed with it.
#[test]
fn a_retired_dictionary_version_is_a_regression() {
    let registry = registry();
    let def = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("declared");
    let mut realization =
        realize(def, &trained_bytes(), measured(DictionaryStrategy::Trained)).expect("realize");
    check_dictionary_retention(&registry, &[realization.clone()])
        .expect("the emitted version is still declared");

    realization.version = "0".to_string();
    let diag = check_dictionary_retention(&registry, &[realization])
        .expect_err("a dropped version must be a regression");
    assert_eq!(
        diag.code(),
        crate::error::MediumDictionaryRegression::register(),
        "{diag}"
    );
}

/// A malformed digest is refused at EMISSION: it makes the decoder's
/// pre-priming comparison meaningless, so shipping it would be shipping an
/// uncheckable claim.
#[test]
fn a_malformed_realization_digest_is_refused_at_emission() {
    let registry = registry();
    let def = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("declared");
    let mut realization =
        realize(def, &trained_bytes(), measured(DictionaryStrategy::Trained)).expect("realize");
    realization.content_digest = "not-a-digest".to_string();
    let diag = project(&registry, &[realization], &[])
        .expect_err("a malformed digest must not be projected");
    assert_eq!(
        diag.code(),
        crate::error::MediumDigestMismatch::register(),
        "{diag}"
    );
}
