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

/// The evidence digest of a table measured over exactly the fixture resolution.
fn current_evidence() -> String {
    blake3_digest(b"the fixture corpus resolution")
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
    let realization = realize(
        def,
        &bytes,
        measured(DictionaryStrategy::Trained),
        current_evidence(),
    )
    .expect("realize");
    assert_eq!(realization.byte_length, bytes.len());
    assert!(is_canonical_digest(&realization.content_digest));
    assert!(
        is_canonical_digest(&realization.corpus_digest),
        "the resolved-corpus identity rides on the shipped realization"
    );
    assert_ne!(realization.zstd_dictionary_id, 0);
    // The MEASURED strategy is its own field: a trainer that fell back must be
    // able to say so without touching the authored definition.
    let fell_back = realize(
        def,
        &bytes,
        measured(DictionaryStrategy::RawContent),
        current_evidence(),
    )
    .expect("realize");
    assert_eq!(fell_back.strategy, DictionaryStrategy::RawContent);
    assert_eq!(def.strategy, DictionaryStrategy::Trained);
}

#[test]
fn the_projection_lands_entirely_in_the_medium_registry_graph() {
    let registry = registry();
    let def = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("declared");
    let realization = realize(
        def,
        &trained_bytes(),
        measured(DictionaryStrategy::Trained),
        current_evidence(),
    )
    .expect("realize");
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
    let a = realize(
        core,
        &bytes,
        measured(DictionaryStrategy::Trained),
        current_evidence(),
    )
    .expect("realize");
    let b = realize(
        terms,
        &bytes,
        measured(DictionaryStrategy::TermTable),
        current_evidence(),
    )
    .expect("realize");

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
    let mut realization = realize(
        def,
        &trained_bytes(),
        measured(DictionaryStrategy::Trained),
        current_evidence(),
    )
    .expect("realize");
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
    let mut realization = realize(
        def,
        &trained_bytes(),
        measured(DictionaryStrategy::Trained),
        current_evidence(),
    )
    .expect("realize");
    realization.content_digest = "not-a-digest".to_string();
    let diag = project(&registry, &[realization], &[])
        .expect_err("a malformed digest must not be projected");
    assert_eq!(
        diag.code(),
        crate::error::MediumDigestMismatch::register(),
        "{diag}"
    );
}

/// The fixture registry's declarations plus the given realizations, frozen as the
/// dataset a bundle would carry. No producer runs.
fn bundle_dataset(realizations: &[DictionaryRealization]) -> std::sync::Arc<purrdf::RdfDataset> {
    let registry = registry();
    let mut quads: Vec<RdfQuad> =
        purrdf::flat_rdf_quads_from_dataset(crate::medium::registry::fixture::dataset("").as_ref())
            .into_iter()
            .collect();
    quads.extend(project(&registry, realizations, &[]).expect("project"));
    purrdf::dataset_from_quads(&quads).expect("freeze the bundle dataset")
}

fn realized(id: &str, strategy: DictionaryStrategy, evidence: String) -> DictionaryRealization {
    let registry = registry();
    let def = registry.dictionary_by_id(id).expect("declared");
    realize(def, &trained_bytes(), measured(strategy), evidence).expect("realize")
}

#[test]
fn the_strict_form_refuses_stale_and_absent_dictionary_evidence_from_the_bundles_own_records() {
    let current = || {
        vec![
            realized(
                "gmeow-core-v1",
                DictionaryStrategy::Trained,
                current_evidence(),
            ),
            realized(
                "gmeow-terms-v1",
                DictionaryStrategy::TermTable,
                current_evidence(),
            ),
        ]
    };
    let strict = EvidenceRequirement::RequireRecords;
    require_current_evidence(bundle_dataset(&current()).as_ref(), strict)
        .expect("every declared dictionary is realized with current evidence");

    let recorded = blake3_digest(b"the corpus the table measured");
    let mut stale = current();
    stale[0] = realized(
        "gmeow-core-v1",
        DictionaryStrategy::Trained,
        recorded.clone(),
    );
    let diag = require_current_evidence(bundle_dataset(&stale).as_ref(), strict)
        .expect_err("stale evidence must be refused");
    let text = diag.to_string();
    let hex = |digest: &str| digest.trim_start_matches("blake3:").to_string();
    assert!(
        text.contains("gmeow-core-v1")
            && text.contains(&hex(&recorded))
            && text.contains(&hex(&current_evidence()))
            && !text.contains("gmeow-terms-v1"),
        "the message names the stale dictionary and both digests: {text}"
    );
    assert_eq!(diag.code(), crate::error::MediumCorpusDrift::register());

    let partial = current().into_iter().take(1).collect::<Vec<_>>();
    let missing = require_current_evidence(bundle_dataset(&partial).as_ref(), strict)
        .expect_err("a declared dictionary with no realization must be refused");
    assert!(missing.to_string().contains("gmeow-terms-v1"), "{missing}");

    let none = require_current_evidence(bundle_dataset(&[]).as_ref(), strict)
        .expect_err("a bundle with no realization records must be refused");
    assert!(
        none.to_string()
            .contains("no gmeow:CompressionDictionaryRealization"),
        "{none}"
    );
}

/// An authentic release that predates the evidence records carries none; the default
/// form must not mistake that for stale, while a positively stale record is refused in
/// both forms.
#[test]
fn the_default_form_passes_a_bundle_with_no_records_and_still_refuses_a_stale_one() {
    let default = EvidenceRequirement::RefuseStale;
    require_current_evidence(bundle_dataset(&[]).as_ref(), default)
        .expect("a bundle with no dictionary evidence records is not stale");

    let recorded = blake3_digest(b"the corpus the table measured");
    let stale = vec![realized(
        "gmeow-core-v1",
        DictionaryStrategy::Trained,
        recorded.clone(),
    )];
    for requirement in [default, EvidenceRequirement::RequireRecords] {
        let diag = require_current_evidence(bundle_dataset(&stale).as_ref(), requirement)
            .expect_err("a positively stale record is refused in both forms");
        assert!(diag.to_string().contains("gmeow-core-v1"), "{diag}");
    }
}
