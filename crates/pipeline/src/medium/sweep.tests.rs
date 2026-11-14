// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
        engine_identity: Vec::new(),
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
    let legacy = json.replace(MEDIUM_BASELINE_SCHEMA, "gmeow.medium-baseline.v2");
    assert!(MediumBaseline::from_json(&legacy).is_err());
}

/// Unchanged corpus evidence cannot authenticate a different trainer or codec.
#[test]
fn loading_the_table_requires_the_exact_unique_sorted_engine_identity() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir(root.join("bench")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.dependencies]\npurrdf = '3'\npurrdf-core = '3'\n",
    )
    .unwrap();
    let checksum = "a".repeat(64);
    let package = |name: &str| {
        format!(
            "[[package]]\nname = '{name}'\nversion = '3.0.1'\nsource = \
             'registry+https://github.com/rust-lang/crates.io-index'\nchecksum = '{checksum}'\n"
        )
    };
    std::fs::write(
        root.join("Cargo.lock"),
        format!(
            "version = 4\n{}{}",
            package("purrdf"),
            package("purrdf-core")
        ),
    )
    .unwrap();
    let mut original = baseline(&["gmeow-core-v1"]);
    original.engine_identity = resolve_purrdf(&root.join("Cargo.toml"), &root.join("Cargo.lock"))
        .unwrap()
        .packages;
    let save = |table: &MediumBaseline| {
        std::fs::write(root.join(MEDIUM_BASELINE_PATH), table.to_json().unwrap()).unwrap();
    };
    save(&original);
    assert_eq!(load(root).unwrap(), original);

    let mut stale = original.clone();
    stale.engine_identity[1].checksum = "b".repeat(64);
    let mut duplicate = original.clone();
    duplicate
        .engine_identity
        .push(duplicate.engine_identity[0].clone());
    let mut reordered = original.clone();
    reordered.engine_identity.reverse();
    let mut missing = original.clone();
    missing.engine_identity.clear();
    for table in [stale, duplicate, reordered, missing] {
        save(&table);
        let error = load(root).unwrap_err().to_string();
        assert!(error.contains("trainer/codec identity"), "{error}");
        assert!(error.contains("maint-medium-sweep"), "{error}");
    }
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

/// A resolution of the dictionary's corpus carrying `digest`.
fn resolution(digest: &str) -> crate::medium::corpus::CorpusResolution {
    crate::medium::corpus::CorpusResolution {
        training: [std::sync::Arc::from(b"a sample".as_slice())]
            .into_iter()
            .collect(),
        held_out_count: 2,
        digest: digest.to_string(),
    }
}

/// A committed row whose grid prices the winner at `best` bytes and the declared cell
/// (trained/4096) at `declared` bytes; the winner is a different cell when they differ.
fn graded_row(id: &str, best: u64, declared: u64) -> DictionaryBaseline {
    let mut row = row(id);
    row.bytes_on_disk_baseline = best.max(declared) * 2;
    let cell = |strategy: &str, target_length: u64, two_part_code_bytes: u64| GridRow {
        strategy: strategy.to_string(),
        target_length,
        dictionary_bytes: 100,
        two_part_code_bytes,
    };
    if best == declared {
        row.grid = vec![cell("trained", 4096, declared)];
    } else {
        row.winning_strategy = "trained".to_string();
        row.winning_target_length = 2048;
        row.declared_is_argmin = false;
        row.grid = vec![cell("trained", 2048, best), cell("trained", 4096, declared)];
    }
    row
}

fn graded(
    row: DictionaryBaseline,
    live_digest: &str,
    profile: crate::run::BuildProfile,
) -> Result<DictionaryEvidence, gmeow_errors::Diag> {
    let registry = registry();
    let def = registry
        .dictionary_by_id("gmeow-core-v1")
        .expect("declared");
    let mut baseline = baseline(&[]);
    baseline.dictionaries = vec![row];
    grade_dictionary(def, &baseline, &resolution(live_digest), profile)
}

#[test]
fn a_matching_corpus_digest_grades_the_declared_winner_as_current_evidence() {
    let row = graded_row("gmeow-core-v1", 500, 500);
    let digest = row.corpus_digest.clone();
    let evidence = graded(row, &digest, crate::run::BuildProfile::Release).expect("current");
    assert_eq!(evidence.declared_point, DeclaredPoint::Winner);
    assert_eq!(evidence.corpus, EvidenceCorpus::Current);
    assert_eq!(evidence.evidence_corpus_digest, digest);
}

#[test]
fn an_ordinary_build_accepts_a_moved_corpus_as_stale_evidence() {
    let row = graded_row("gmeow-core-v1", 500, 500);
    let recorded = row.corpus_digest.clone();
    let moved = super::super::blake3_digest(b"the corpus moved");
    let evidence = graded(row, &moved, crate::run::BuildProfile::Ordinary).expect("stale");
    assert_eq!(
        evidence.corpus,
        EvidenceCorpus::Stale {
            recorded: recorded.clone(),
            resolved: moved,
        }
    );
    assert_eq!(
        evidence.evidence_corpus_digest, recorded,
        "the realization must carry the digest the table measured, not the resolved one"
    );
}

#[test]
fn a_release_build_refuses_a_moved_corpus() {
    let row = graded_row("gmeow-core-v1", 500, 500);
    let moved = super::super::blake3_digest(b"the corpus moved");
    let diag = graded(row, &moved, crate::run::BuildProfile::Release)
        .expect_err("a release must refuse stale evidence");
    assert_eq!(
        diag.code(),
        crate::error::MediumCorpusDrift::register(),
        "{diag}"
    );
    assert!(diag.to_string().contains("maint-medium-sweep"), "{diag}");
}

#[test]
fn a_declared_cell_within_the_margin_of_the_winner_is_accepted() {
    let row = graded_row("gmeow-core-v1", 10_000, 10_050);
    let digest = row.corpus_digest.clone();
    let evidence = graded(row, &digest, crate::run::BuildProfile::Release).expect("within");
    let DeclaredPoint::WithinMargin { declared, best } = evidence.declared_point else {
        panic!("a near-tie must be reported as such");
    };
    assert_eq!(declared.two_part_code_bytes, 10_050);
    assert_eq!(best.two_part_code_bytes, 10_000);
}

#[test]
fn a_declared_cell_outside_the_margin_of_the_winner_hard_fails_on_every_profile() {
    for profile in [
        crate::run::BuildProfile::Ordinary,
        crate::run::BuildProfile::Release,
    ] {
        let row = graded_row("gmeow-core-v1", 10_000, 10_200);
        let digest = row.corpus_digest.clone();
        let diag = graded(row, &digest, profile).expect_err("above the margin");
        assert_eq!(
            gmeow_errors::code::code_str(diag.code()),
            "pipeline.medium.dictionary-regression",
            "{diag}"
        );
    }
}

#[test]
fn a_declared_cell_missing_from_the_grid_hard_fails() {
    let mut row = graded_row("gmeow-core-v1", 500, 500);
    row.grid.clear();
    row.grid.push(GridRow {
        strategy: "trained".to_string(),
        target_length: 2048,
        dictionary_bytes: 100,
        two_part_code_bytes: 500,
    });
    row.winning_target_length = 2048;
    let digest = row.corpus_digest.clone();
    let diag = graded(row, &digest, crate::run::BuildProfile::Ordinary)
        .expect_err("nothing measured the declared cell");
    assert_eq!(
        gmeow_errors::code::code_str(diag.code()),
        "pipeline.medium.dictionary-regression",
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

/// The sweep measures the frames the committed-scope bundle carries, so it schedules
/// exactly the committed output scope: the runtime-only exporters (whose whole-carrier
/// JSON-LD serialization refuses on the current corpus) are not run, while the snapshot
/// and the archive producer the frames come from are.
#[test]
fn the_sweep_schedules_exactly_the_committed_output_scope() {
    let selected: BTreeSet<String> = sweep_spec()
        .expect("select the sweep's stages")
        .stages
        .into_iter()
        .map(|stage| stage.id)
        .collect();
    let committed: BTreeSet<String> =
        crate::run::spec_for_scope(crate::full_spec(), crate::run::RunOutputScope::Committed)
            .expect("committed scope")
            .stages
            .into_iter()
            .map(|stage| stage.id)
            .collect();
    assert_eq!(selected, committed);
    for runtime_only in [
        "stage-export-yaml-ld",
        "stage-export-export",
        "stage-export-okf",
    ] {
        assert!(
            !selected.contains(runtime_only),
            "{runtime_only} is scheduled"
        );
    }
    for needed in ["stage-snapshot", "stage-archive-blobs"] {
        assert!(selected.contains(needed), "{needed} is not scheduled");
    }
}

#[test]
fn a_declared_cell_that_does_not_pay_for_itself_is_refused_though_the_winner_does() {
    // The winner (990 B) beats the 1000 B no-dictionary baseline; the declared cell
    // (1005 B, within the 1% margin of the winner) costs more than no dictionary.
    let mut row = graded_row("gmeow-core-v1", 990, 1005);
    row.bytes_on_disk_baseline = 1000;
    row.two_part_code_bytes = 990;
    let digest = row.corpus_digest.clone();
    let diag = graded(row, &digest, crate::run::BuildProfile::Ordinary)
        .expect_err("the shipped cell costs more than no dictionary");
    assert_eq!(
        gmeow_errors::code::code_str(diag.code()),
        "pipeline.medium.dictionary-regression",
        "{diag}"
    );
    assert!(
        diag.to_string().contains("does not pay for itself"),
        "{diag}"
    );
}

#[test]
fn the_margin_accepts_exactly_one_percent_and_refuses_the_next_byte_over() {
    let accepted = graded_row("gmeow-core-v1", 10_000, 10_100);
    let digest = accepted.corpus_digest.clone();
    graded(accepted, &digest, crate::run::BuildProfile::Release)
        .expect("exactly one percent above the winner is inside the margin");

    let refused = graded_row("gmeow-core-v1", 10_000, 10_101);
    let digest = refused.corpus_digest.clone();
    graded(refused, &digest, crate::run::BuildProfile::Release)
        .expect_err("a byte past one percent is outside the margin");
}

#[test]
fn a_table_whose_winner_is_not_its_cheapest_cell_is_refused() {
    let mut row = graded_row("gmeow-core-v1", 10_000, 10_050);
    row.grid.push(GridRow {
        strategy: "trained".to_string(),
        target_length: 8192,
        dictionary_bytes: 100,
        two_part_code_bytes: 9_000,
    });
    let digest = row.corpus_digest.clone();
    let diag = graded(row, &digest, crate::run::BuildProfile::Ordinary)
        .expect_err("the table contradicts itself");
    assert!(diag.to_string().contains("cheapest cell"), "{diag}");
}
