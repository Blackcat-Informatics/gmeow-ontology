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
    let stage = MediumDictionariesStage::new(crate::run::BuildProfile::Ordinary);
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

/// An ordinary build over a moved corpus trains at the DECLARED point and the
/// projected realization carries both digests with the stale flag set, so a consumer
/// can see the bytes were not trained from the corpus the evidence measured.
#[test]
fn a_stale_evidence_build_trains_at_the_declaration_and_records_both_digests() {
    use crate::medium::registry::{fixture, gm};
    use crate::medium::sweep::{DeclaredPoint, DictionaryEvidence, EvidenceCorpus};

    let registry = MediumRegistry::from_dataset(&fixture::dataset("")).expect("fixture registry");
    let def = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("declared");
    let training = (0..400u32)
        .map(|i| {
            std::sync::Arc::<[u8]>::from(
                format!(
                    "<https://blackcatinformatics.ca/gmeow/t{}> <https://e/p> \"v{i}\" .\n",
                    i % 31
                )
                .into_bytes(),
            )
        })
        .collect();
    let resolved_digest = crate::medium::blake3_digest(b"the corpus this build resolved");
    let recorded_digest = crate::medium::blake3_digest(b"the corpus the table measured");
    let resolved = corpus::CorpusResolution {
        training,
        held_out_count: 0,
        digest: resolved_digest.clone(),
    };
    let evidence = DictionaryEvidence {
        id: def.id.clone(),
        declared_point: DeclaredPoint::Winner,
        corpus: EvidenceCorpus::Stale {
            recorded: recorded_digest.clone(),
            resolved: resolved_digest.clone(),
        },
        evidence_corpus_digest: recorded_digest.clone(),
    };

    let trained =
        train_declared_dictionary(def, &fixture::dataset(""), &resolved, &evidence, &mut None)
            .expect("train at the declared point");
    assert_eq!(trained.measured.strategy, def.strategy);
    assert_eq!(trained.measured.target_length, def.target_length);

    let realization = crate::medium::rdf::realize(
        def,
        &trained.bytes,
        trained.measured,
        trained.evidence_corpus_digest,
    )
    .expect("realize");
    let quads = crate::medium::rdf::project(&registry, &[realization], &[]).expect("project");
    let lexical = |predicate: &str| -> String {
        quads
            .iter()
            .find_map(|q| match (&q.object, q.predicate == gm(predicate)) {
                (purrdf::RdfTerm::Literal(literal), true) => Some(literal.lexical_form.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the projection carries gmeow:{predicate}"))
    };
    assert_eq!(lexical("evidenceCorpusStale"), "true");
    assert_eq!(
        lexical("measuredDictionaryTargetLength"),
        def.target_length.to_string()
    );
    assert!(
        lexical("evidenceCorpusDigest").contains(recorded_digest.trim_start_matches("blake3:"))
            && lexical("measuredCorpusDigest")
                .contains(resolved_digest.trim_start_matches("blake3:")),
        "the realization must keep the evidence digest and the trained-from digest apart"
    );
}
