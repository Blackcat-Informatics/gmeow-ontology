// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use super::*;
use crate::medium::registry::fixture;

fn registry() -> MediumRegistry {
    MediumRegistry::from_dataset(&fixture::dataset("")).expect("fixture registry")
}

fn row(id: &str) -> DictionaryBaseline {
    DictionaryBaseline {
        id: id.to_string(),
        population: Population::EmittedBlobFrames.wire().to_string(),
        declared_strategy: "trained".to_string(),
        declared_target_length: 4096,
        winning_strategy: "trained".to_string(),
        winning_target_length: 4096,
        declared_is_argmin: true,
        bytes_on_disk: 400,
        bytes_on_disk_baseline: 1000,
        dictionary_in_band_bytes: 100,
        two_part_code_bytes: 500,
        dictionary_gain_fraction: "0.500000".to_string(),
        corpus_sample_count: 12,
        held_out_sample_count: 2,
        corpus_digest: super::super::blake3_digest(id.as_bytes()),
        evaluated_frame_count: 3,
        grid: Vec::new(),
    }
}

fn baseline(ids: &[&str]) -> MediumBaseline {
    MediumBaseline {
        schema: MEDIUM_BASELINE_SCHEMA.to_string(),
        codec_sweep: CodecSweep {
            mandated_codec: "zstd-rsyncable".to_string(),
            mandated_level: 12,
            corpus_frame_count: 3,
            corpus_bytes: 4096,
            excluded_reps: Vec::new(),
            rows: Vec::new(),
            mandated_is_argmin: true,
        },
        dictionaries: ids.iter().map(|id| row(id)).collect(),
    }
}

/// (f) The bijection hard-fails in BOTH directions — a missing row and a stale
/// one are each a way for a dictionary to escape gate coverage.
#[test]
fn the_winner_table_registry_bijection_hard_fails_in_both_directions() {
    let registry = registry();
    check_bijection(&registry, &baseline(&["gmeow-core-v1", "gmeow-terms-v1"]))
        .expect("the exact measurable set is a bijection");

    let missing = check_bijection(&registry, &baseline(&["gmeow-core-v1"]))
        .expect_err("a declared-but-unmeasured dictionary must hard-fail");
    assert_eq!(
        missing.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{missing}"
    );
    assert!(missing.to_string().contains("gmeow-terms-v1"), "{missing}");

    let stale = check_bijection(
        &registry,
        &baseline(&["gmeow-core-v1", "gmeow-terms-v1", "gmeow-retired-v1"]),
    )
    .expect_err("a committed-but-undeclared dictionary must hard-fail");
    assert!(stale.to_string().contains("gmeow-retired-v1"), "{stale}");
}

/// The measurable set is EVERY declared dictionary — the sweep exempts none.
#[test]
fn every_declared_dictionary_is_measurable() {
    let registry = registry();
    let declared: BTreeSet<String> = registry
        .dictionaries()
        .values()
        .map(|def| def.id.clone())
        .collect();
    assert_eq!(
        measurable_ids(&registry),
        declared,
        "an id that is declared but not measurable would be trained at a guess"
    );
    assert_eq!(
        declared,
        ["gmeow-core-v1".to_string(), "gmeow-terms-v1".to_string()]
            .into_iter()
            .collect::<BTreeSet<String>>(),
        "the fixture registry's declaration set drifted, so the equality above is not the \
             statement it reads as"
    );
}

/// The artifact round-trips byte-identically and refuses an unknown schema token.
#[test]
fn the_artifact_round_trips_and_refuses_an_unknown_schema() {
    let original = baseline(&["gmeow-core-v1"]);
    let json = original.to_json().expect("serialize");
    assert!(json.ends_with('\n'), "the artifact ends with a newline");
    assert_eq!(
        MediumBaseline::from_json(&json).expect("parse"),
        original,
        "the committed artifact must round-trip"
    );
    // Byte-stability: rendering the parsed value reproduces the same bytes.
    assert_eq!(
        MediumBaseline::from_json(&json)
            .expect("parse")
            .to_json()
            .expect("serialize"),
        json
    );
    let skewed = json.replace(MEDIUM_BASELINE_SCHEMA, "gmeow.medium-baseline.v99");
    let diag = MediumBaseline::from_json(&skewed).expect_err("an unknown schema must hard-fail");
    assert!(diag.to_string().contains("maint-medium-sweep"), "{diag}");
}

/// The strategy wire tokens round-trip against `DictionaryStrategy`'s own
/// `Display`, so the committed artifact and the diagnostics cannot drift.
#[test]
fn the_strategy_wire_tokens_round_trip() {
    for strategy in SWEEP_STRATEGIES {
        let token = strategy_wire(strategy);
        assert_eq!(strategy_from_wire(&token), Some(strategy), "{token}");
    }
    assert_eq!(strategy_from_wire("invented"), None);
}

/// The sweep selects the argmin of the TWO-PART code, and the grid it selected
/// from is committed beside the winner — a winner with no visible grid is an
/// assertion, not evidence.
#[test]
fn the_sweep_selects_the_two_part_argmin_and_commits_its_grid() {
    let owned: Vec<Vec<u8>> = (0..600u32)
        .map(|i| {
            format!(
                "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {i} in the gmeow ontology\" .\n",
                i % 41
            )
            .into_bytes()
        })
        .collect();
    let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let frame = owned.concat();
    let frames: Vec<&[u8]> = vec![frame.as_slice()];
    let term_table = b"<https://blackcatinformatics.ca/gmeow/definition>\n".to_vec();
    let swept = sweep_dictionary(
        &DictionarySweepInputs {
            id: "gmeow-core-v1",
            declared_strategy: DictionaryStrategy::Trained,
            declared_target_length: 4096,
            corpus: &corpus,
            term_table: &term_table,
            corpus_sample_count: corpus.len() as u64,
            held_out_sample_count: 7,
            corpus_digest: "blake3:00",
        },
        &frames,
        "zstd-rsyncable",
        12,
    )
    .expect("sweep");
    assert_eq!(swept.held_out_sample_count, 7);
    assert_eq!(swept.corpus_digest, "blake3:00");
    assert!(!swept.grid.is_empty(), "the grid is the evidence");
    let argmin = swept
        .grid
        .iter()
        .map(|cell| cell.two_part_code_bytes)
        .min()
        .expect("non-empty grid");
    assert_eq!(
        swept.two_part_code_bytes, argmin,
        "the committed winner must be the grid's two-part argmin"
    );
    assert_eq!(
        swept.declared_is_argmin,
        swept.winning_strategy == "trained" && swept.winning_target_length == 4096
    );
    assert_eq!(swept.evaluated_frame_count, 1);
}

/// The corpus-identity gate passes on agreement and REDS the moment the resolved
/// corpus is not the one the table was measured over — which is what a committed
/// table quietly rotting looks like from the build's side.
#[test]
fn the_corpus_identity_gate_reds_when_the_resolved_corpus_moves() {
    use crate::medium::corpus::CorpusResolution;

    let registry = registry();
    let baseline = baseline(&["gmeow-core-v1", "gmeow-terms-v1"]);
    let resolution = |id: &str| CorpusResolution {
        training: [std::sync::Arc::from(b"a sample".as_slice())]
            .into_iter()
            .collect(),
        held_out_count: 2,
        digest: super::super::blake3_digest(id.as_bytes()),
    };
    let mut resolved: BTreeMap<String, CorpusResolution> = ["gmeow-core-v1", "gmeow-terms-v1"]
        .into_iter()
        .map(|id| (id.to_string(), resolution(id)))
        .collect();
    check_corpus_digests(&registry, &baseline, &resolved)
        .expect("the recorded identity IS the resolved one");

    // One archive member changes: the corpus moves, the table does not.
    resolved.insert(
        "gmeow-core-v1".to_string(),
        CorpusResolution {
            training: [
                std::sync::Arc::from(b"a sample".as_slice()),
                std::sync::Arc::from(b"a new archive member".as_slice()),
            ]
            .into_iter()
            .collect(),
            held_out_count: 2,
            digest: super::super::blake3_digest(b"gmeow-core-v1 plus one member"),
        },
    );
    let diag = check_corpus_digests(&registry, &baseline, &resolved)
        .expect_err("a moved corpus must hard-fail rather than keep grading");
    assert_eq!(
        diag.code(),
        crate::error::MediumCorpusDrift::register(),
        "{diag}"
    );
    assert!(
        diag.to_string().contains("gmeow-core-v1")
            && diag.to_string().contains("maint-medium-sweep"),
        "{diag}"
    );
    assert!(
        !diag.to_string().contains("gmeow-terms-v1"),
        "only the dictionary whose corpus moved is named: {diag}"
    );
}

/// A dictionary the build resolves no corpus for has nothing to anchor its
/// committed row to, which is the same defect wearing a different hat.
#[test]
fn a_committed_row_with_no_resolved_corpus_reds() {
    let registry = registry();
    let baseline = baseline(&["gmeow-core-v1", "gmeow-terms-v1"]);
    let diag = check_corpus_digests(&registry, &baseline, &BTreeMap::new())
        .expect_err("an unanchored row must hard-fail");
    assert_eq!(
        diag.code(),
        crate::error::MediumCorpusDrift::register(),
        "{diag}"
    );
}

/// The codec sweep prices the MANDATED cell and says, as data, whether it is the
/// argmin — the STOP-and-ask signal.
#[test]
fn the_codec_sweep_prices_the_mandated_cell() {
    let payload = b"<https://e/s> <https://e/p> \"v\" .\n".repeat(400);
    let frames: Vec<&[u8]> = vec![payload.as_slice()];
    let sweep = sweep_codecs(
        &frames,
        &["ontology-docs".to_string()],
        "zstd-rsyncable",
        12,
    )
    .expect("codec sweep");
    assert_eq!(
        sweep.rows.len(),
        SWEEP_CODECS.len() * SWEEP_LEVELS.len(),
        "every declared cell is priced"
    );
    assert!(
        sweep
            .rows
            .iter()
            .any(|row| row.codec == "zstd-rsyncable" && row.level == 12),
        "the mandated cell is on the grid"
    );
    assert_eq!(sweep.excluded_reps, ["ontology-docs".to_string()]);
    assert_eq!(sweep.corpus_frame_count, 1);
    // A cell the sweep cannot price is a hard fail, never an omitted row.
    assert!(sweep_codecs(&frames, &[], "brotli", 12).is_err());
}
