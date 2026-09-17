// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The declared consumes set covers every stage-product selector, and the
/// sink-folded proof-trace corpus names exactly the three report artifacts owned
/// by stage-reason rather than selecting its closure-bearing whole product.
#[test]
fn the_consumes_set_covers_shipped_corpus_producers_exactly() {
    let registry =
        crate::medium::source_observation::authenticated(&gmeow_conformance::paths::repo_root())
            .expect("authenticated source medium registry")
            .registry;

    let mut selected: Vec<String> = Vec::new();
    for corpus in registry.corpora().values() {
        for selector in &corpus.selectors {
            if let crate::medium::corpus::CorpusSelector::StageProduct(iri) = selector {
                let stage = iri
                    .strip_prefix(GMEOW)
                    .expect("a stage-product selector names a gmeow: individual");
                selected.push(stage.to_string());
            }
        }
    }
    selected.sort();
    selected.dedup();
    for stage in &selected {
        assert!(
            CONSUMES.contains(&stage.as_str()),
            "corpus selector names `{stage}`, which {STAGE_ID} does not consume — \
                 corpus::assemble would hard-fail on the missing gmeow:dataflowConsumes edge"
        );
    }

    let prooftrace = registry
        .corpora()
        .get(&format!("{GMEOW}corpusGmeowProoftraceV1"))
        .expect("the proof-trace corpus is declared");
    let paths: Vec<&str> = prooftrace
        .selectors
        .iter()
        .filter_map(|selector| match selector {
            crate::medium::corpus::CorpusSelector::PathPrefix(path) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        paths,
        [
            crate::stages::reason::LEDGER_PATH,
            crate::stages::reason::PERF_LEDGER_PATH,
            crate::stages::reason::EXPLANATIONS_PATH,
        ]
    );
    assert!(!paths.contains(&crate::stages::reason::CLOSURE_PATH));
    assert!(CONSUMES.contains(&"stage-reason"));
}

/// The Rust `consumes()` is sorted and matches the declared constant, which is the
/// half of the three-way declaration (`Stage::consumes`, `run::full_spec`,
/// `module.ttl`) this crate can check without running the loader.
#[test]
fn the_consumes_set_is_sorted_and_bound() {
    let stage = MediumDictionariesStage::new();
    let mut sorted = stage.consumes().to_vec();
    sorted.sort();
    assert_eq!(stage.consumes(), sorted.as_slice());
    assert_eq!(stage.id(), STAGE_ID);
    assert_eq!(
        stage.carrier_consumes(),
        ["stage-archive-blobs", "stage-snapshot"]
    );
    assert_eq!(
        stage.resources(),
        [crate::node::SERIALIZATION_BUFFER_RESOURCE]
    );
    assert!(stage.consumes().iter().any(|id| id == "stage-reason"));
    assert!(stage.consumes().iter().any(|id| id == "stage-statements"));
    assert!(
        !stage
            .carrier_consumes()
            .iter()
            .any(|id| id == "stage-reason")
    );
    assert!(
        !stage
            .carrier_consumes()
            .iter()
            .any(|id| id == "stage-statements")
    );
}

/// The frame identity is a pure function of `(rep, digest)` and separates frames
/// that share either coordinate alone — the property that keeps one envelope per
/// frame rather than per rep.
#[test]
fn the_frame_identity_separates_rep_and_digest() {
    let a = frame_iri("cells-archive", "blake3:aa");
    let b = frame_iri("cells-archive", "blake3:bb");
    let c = frame_iri("axioms-archive", "blake3:aa");
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_eq!(a, frame_iri("cells-archive", "blake3:aa"));
    assert!(a.starts_with("https://blackcatinformatics.ca/gmeow/frame/"));
}
