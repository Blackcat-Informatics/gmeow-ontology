// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dictionary-EFFECT gate, proved on real bytes and against the COMMITTED
//! evidence.
//!
//! `crates/pipeline/tests/medium_bundle.rs` proves the measurement on the shipped
//! artifact but costs a whole DAG run. These clauses are the ones that must be able to
//! RED — a gate nobody has watched fail is a gate nobody knows works — plus the cheap
//! agreements between the committed winner table (`bench/medium-baseline.json`), the
//! authored medium axis (`slices/core/gts/module.ttl`), and the code that consumes
//! both. None of them needs the pipeline to run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gmeow_pipeline::medium::measure::{self, DictionaryEffect, Population};
use gmeow_pipeline::medium::registry::{DictionaryStrategy, MediumRegistry};
use gmeow_pipeline::medium::{sweep, train};
use purrdf::gts::examples::agent_memory::{Memory, MemoryOptions, StoreOptions};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// The medium axis as `slices/core/gts/module.ttl` authors it.
///
/// Parsed from the slice because a unit-scale test has no carrier; PRODUCTION always
/// reads the in-memory dataset, and the whole-bundle gate proves the two agree.
fn live_registry() -> MediumRegistry {
    let module = repo_root().join("slices/core/gts/module.ttl");
    let text = std::fs::read_to_string(&module).expect("the gts slice is readable");
    let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", Some(GMEOW))
        .expect("the gts slice parses as Turtle");
    MediumRegistry::from_dataset(&dataset).expect("the live medium axis reads")
}

fn committed_baseline() -> sweep::MediumBaseline {
    sweep::load(&repo_root()).expect("the committed winner table is readable")
}

/// A corpus shaped like the RDF the real dictionaries see: many small records over a
/// small shared vocabulary, which is the byte profile a trained dictionary pays for.
fn rdf_corpus(records: usize) -> Vec<Vec<u8>> {
    (0..records)
        .map(|i| {
            format!(
                "<{GMEOW}term{}> <{GMEOW}definition> \"a definition of term {i} in the gmeow \
                 ontology\" .\n",
                i % 37
            )
            .into_bytes()
        })
        .collect()
}

// ── (f) the committed table ↔ the live registry ──────────────────────────────

/// (f) The committed winner table and the MEASURABLE registry are a bijection, on the
/// LIVE declaration rather than on a fixture.
#[test]
fn the_committed_winner_table_matches_the_shipped_registry() {
    let registry = live_registry();
    let baseline = committed_baseline();
    sweep::check_bijection(&registry, &baseline)
        .expect("bench/medium-baseline.json ↔ the measurable registry must be a bijection");

    // The exception is EXACTLY one dictionary, and it is genuinely declared: a
    // silently-widened exception list is how a dictionary escapes measurement.
    let declared: BTreeSet<&str> = registry
        .dictionaries()
        .values()
        .map(|def| def.id.as_str())
        .collect();
    let measurable = sweep::measurable_ids(&registry);
    assert_eq!(
        declared.len() - measurable.len(),
        sweep::UNMEASURABLE_DICTIONARIES.len(),
        "the unmeasurable exception must cover exactly {:?}",
        sweep::UNMEASURABLE_DICTIONARIES
    );
    for id in sweep::UNMEASURABLE_DICTIONARIES {
        assert!(
            declared.contains(id),
            "the exception names {id:?}, which the registry does not declare — a stale exception \
             is an exception that covers nothing and hides the next one"
        );
        assert!(!measurable.contains(id));
    }
}

/// (f) It hard-fails in BOTH directions against the LIVE registry too — a dropped row
/// and an invented one are each a way for a dictionary to escape coverage.
#[test]
fn the_bijection_reds_in_both_directions_against_the_live_registry() {
    let registry = live_registry();

    let mut short = committed_baseline();
    let dropped = short.dictionaries.pop().expect("the table is non-empty");
    let diag = sweep::check_bijection(&registry, &short)
        .expect_err("a declared-but-unmeasured dictionary must hard-fail");
    assert!(diag.to_string().contains(&dropped.id), "{diag}");

    let mut wide = committed_baseline();
    let mut invented = dropped;
    invented.id = "gmeow-never-declared-v1".to_string();
    wide.dictionaries.push(invented);
    let diag = sweep::check_bijection(&registry, &wide)
        .expect_err("a committed-but-undeclared dictionary must hard-fail");
    assert!(
        diag.to_string().contains("gmeow-never-declared-v1"),
        "{diag}"
    );
}

/// The AUTHORED training point of every measurable dictionary IS the committed argmin.
///
/// This is the check that lets the build train from the DECLARATION while still only
/// ever training at a MEASURED point. A sweep that found a different argmin is a
/// reportable finding a human reconciles — never a silent overwrite of the slice — and
/// this stays red until they do.
#[test]
fn the_declared_training_points_are_the_committed_winners() {
    let registry = live_registry();
    let baseline = committed_baseline();
    sweep::check_declared_matches_winners(&registry, &baseline).expect(
        "slices/core/gts/module.ttl must declare the measured argmin for every measurable \
         dictionary",
    );

    // …and the check can RED: perturbing one committed winner must be caught.
    let mut drifted = committed_baseline();
    let row = drifted
        .dictionaries
        .first_mut()
        .expect("the table is non-empty");
    row.winning_target_length = row.winning_target_length.saturating_mul(2) + 1;
    let diag = sweep::check_declared_matches_winners(&registry, &drifted)
        .expect_err("a declaration that is not the committed argmin must hard-fail");
    assert_eq!(
        gmeow_errors::code::code_str(diag.code()),
        "pipeline.medium.dictionary-regression",
        "{diag}"
    );
}

/// The committed table carries REAL measurements — the guard against a bootstrap seed
/// (`medium-sweep --seed`, every measured field zero) ever being committed as evidence.
#[test]
fn the_committed_winner_table_carries_real_measurements() {
    let baseline = committed_baseline();
    assert!(
        !baseline.codec_sweep.rows.is_empty(),
        "the codec grid is the evidence behind the mandated chain; an empty grid is a seed"
    );
    assert!(!baseline.dictionaries.is_empty());
    for row in &baseline.dictionaries {
        assert!(
            row.bytes_on_disk_baseline > 0,
            "{}: a zero baseline cannot have been measured — this is a bootstrap seed, not \
             evidence",
            row.id
        );
        assert!(
            !row.grid.is_empty(),
            "{}: a winner with no grid is an assertion, not a measurement",
            row.id
        );
        // (c) each measurable dictionary WINS on the population it primes, net of its
        // own in-band bytes.
        assert!(
            row.two_part_code_bytes < row.bytes_on_disk_baseline,
            "{}: two-part code {} B is not strictly less than the baseline {} B — the dictionary \
             does not pay for itself",
            row.id,
            row.two_part_code_bytes,
            row.bytes_on_disk_baseline
        );
        assert_eq!(
            row.two_part_code_bytes,
            row.bytes_on_disk + row.dictionary_in_band_bytes,
            "{}: the two-part code must be the sum of its two recorded components",
            row.id
        );
        // The committed winner IS the grid's argmin.
        let argmin = row
            .grid
            .iter()
            .map(|cell| cell.two_part_code_bytes)
            .min()
            .expect("non-empty grid");
        assert_eq!(
            row.two_part_code_bytes, argmin,
            "{}: the committed winner is not the grid's two-part argmin",
            row.id
        );
    }
}

/// The codec grid PRICES the mandated cell, and the committed `mandated_is_argmin` flag
/// is a true statement about the grid it is committed beside.
///
/// The flag is `false`, and that is the RECORDED, human-settled answer rather than an
/// open finding: the mandated `zstd-rsyncable` @ 12 chain costs materially more than
/// plain `zstd` at the same level on this corpus, and it is KEPT — the grid prices SIZE
/// ONLY, while GTS §8.4 rsyncable framing buys delta-transfer locality no size grid can
/// see, and the mandated profile is normative Rule 6 doctrine. `bench/README.md` records
/// the reasoning, including the two facts that keep the tradeoff live.
///
/// So what is asserted here is not the ANSWER (a human owns that) but that the artifact
/// cannot LIE about its own evidence:
///
/// * the mandated cell is on the grid, so the flag is about a priced cell;
/// * the flag equals the comparison recomputed from the committed rows, so it can never
///   drift from them — a refresh that quietly flipped it while the numbers said
///   otherwise reds here;
/// * when it is `false`, the strictly cheaper cell is VISIBLE in the grid, so the cost
///   of the decision stays legible instead of collapsing to a boolean.
#[test]
fn the_codec_grid_prices_the_mandated_cell_and_the_flag_matches_it() {
    let baseline = committed_baseline();
    let sweep = &baseline.codec_sweep;
    let mandated = sweep
        .rows
        .iter()
        .find(|row| row.codec == sweep.mandated_codec && row.level == sweep.mandated_level)
        .unwrap_or_else(|| {
            panic!(
                "the committed codec grid does not price the MANDATED cell ({} level {}) — a \
                 grid that cannot price the chain the bundle actually writes is evidence about \
                 nothing",
                sweep.mandated_codec, sweep.mandated_level
            )
        });
    let argmin = sweep
        .rows
        .iter()
        .map(|row| row.bytes)
        .min()
        .expect("the codec grid is non-empty");
    assert_eq!(
        sweep.mandated_is_argmin,
        mandated.bytes == argmin,
        "the committed mandated_is_argmin flag ({}) disagrees with the grid committed beside \
         it: the mandated cell codes {} B and the grid's argmin is {} B. The flag is a \
         statement ABOUT these rows and may never drift from them",
        sweep.mandated_is_argmin,
        mandated.bytes,
        argmin
    );
    if !sweep.mandated_is_argmin {
        // The decision to keep the mandated chain is a human's; the PRICE of it must
        // stay visible rather than collapsing into a boolean.
        let cheaper: Vec<&sweep::CodecRow> = sweep
            .rows
            .iter()
            .filter(|row| row.bytes < mandated.bytes)
            .collect();
        assert!(
            !cheaper.is_empty(),
            "mandated_is_argmin is false but no committed row is cheaper than the mandated cell"
        );
        println!(
            "the mandated {} @ {} codes {} B; the size-only argmin is {} B. The chain is KEPT \
             (bench/README.md): the grid prices size only, and §8.4 rsyncable framing buys \
             delta-transfer locality it cannot see.",
            sweep.mandated_codec, sweep.mandated_level, mandated.bytes, argmin
        );
    }
}

// ── (d) the chain the measurement runs on ────────────────────────────────────

/// (d) `bytes_on_disk` is computed on `zstd-rsyncable`, asserted by INSPECTING the
/// chain the measurement derives from the shipped media — not by trusting a constant.
#[test]
fn the_measured_chain_is_the_mandated_zstd_rsyncable_chain() {
    let registry = live_registry();
    // One blob row per registered rep that is not the snapshot wire schema: exactly
    // the shape `mandated_chain` sees at the terminal.
    let rows: Vec<purrdf::gts_compose::BlobRow> = registry
        .schemas()
        .values()
        .filter(|schema| schema.rep != gmeow_pipeline::medium::SNAPSHOT_WIRE_REP)
        .map(|schema| purrdf::gts_compose::BlobRow {
            data: b"<https://e/s> <https://e/p> \"v\" .\n".to_vec(),
            media_type: "application/x-tar".to_string(),
            rep: schema.rep.clone(),
        })
        .collect();
    let borrowed: Vec<&purrdf::gts_compose::BlobRow> = rows.iter().collect();
    let (codec, level) = measure::mandated_chain(&registry, &borrowed)
        .expect("every registered rep resolves to one declared chain");
    assert_eq!(
        (codec.as_str(), level),
        ("zstd-rsyncable", 12),
        "the effect measurement must run the MANDATED chain, never a cheaper plain-zstd proxy — \
         a proxy would report bytes this build never writes"
    );
    assert_eq!(
        baseline_committed_chain(),
        ("zstd-rsyncable".to_string(), 12),
        "the committed evidence must have been taken on the same chain"
    );

    // …and the encoder actually reached is the dictionary-primed rsyncable one: a
    // primed encode of a small RDF record beats the unprimed one at the same level.
    let owned = rdf_corpus(400);
    let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let dict = train::build(DictionaryStrategy::Trained, &corpus, 4096).expect("train");
    let primed = measure::encoded_len(&codec, level, Some(&dict), &owned[0]).expect("primed");
    let bare = measure::encoded_len(&codec, level, None, &owned[0]).expect("bare");
    assert!(primed < bare, "{primed} vs {bare}");
}

fn baseline_committed_chain() -> (String, i32) {
    let baseline = committed_baseline();
    (
        baseline.codec_sweep.mandated_codec,
        baseline.codec_sweep.mandated_level,
    )
}

// ── (b) the gate CAN red ─────────────────────────────────────────────────────

/// (b) A deliberately-bad dictionary — oversized and low-yield — hard-fails with
/// `pipeline.medium.dictionary-regression`, on the real chain over real bytes.
///
/// "Oversized and low-yield" is the shape that must lose and is easiest to ship by
/// accident: a dictionary trained at a target far larger than its corpus can justify,
/// priming a population small enough that the in-band bytes dwarf the saving. Nothing
/// about it is malformed — every other medium-axis check passes — which is precisely
/// why the two-part code has to be the thing that catches it.
#[test]
fn an_oversized_low_yield_dictionary_hard_fails_the_gate() {
    let owned = rdf_corpus(2000);
    let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let oversized = train::build(DictionaryStrategy::Trained, &corpus, 262_144)
        .expect("an oversized dictionary still builds");
    assert!(
        oversized.len() > 64 * 1024,
        "the fixture must genuinely be oversized: {} B",
        oversized.len()
    );

    // A population of ONE small frame: the dictionary shrinks it, and still loses.
    let frame = owned[0].clone();
    let with_dict =
        measure::encoded_len("zstd-rsyncable", 12, Some(&oversized), &frame).expect("primed");
    let baseline = measure::encoded_len("zstd-rsyncable", 12, None, &frame).expect("bare");
    assert!(
        with_dict < baseline,
        "the frames really do get smaller ({with_dict} < {baseline}) — so only the two-part code \
         can catch this, which is the whole point"
    );

    let effect = DictionaryEffect {
        dictionary_id: "gmeow-core-v1".to_string(),
        population: Population::EmittedBlobFrames,
        bytes_on_disk: with_dict,
        bytes_on_disk_baseline: baseline,
        dictionary_in_band_bytes: oversized.len() as u64,
        corpus_sample_count: owned.len() as u64,
        evaluated_frame_count: 1,
    };
    assert!(!effect.wins());
    let diag = measure::check(&[effect], &BTreeSet::new())
        .expect_err("an oversized, low-yield dictionary must hard-fail");
    assert_eq!(
        gmeow_errors::code::code_str(diag.code()),
        "pipeline.medium.dictionary-regression",
        "{diag}"
    );
    assert!(
        diag.to_string().contains("no threshold to relax"),
        "the failure must refuse the escape hatch by name: {diag}"
    );
}

// ── (e) population B: one header per claim ───────────────────────────────────

/// The population-B replay corpus: small, deterministic, and shaped like the claim
/// text a runtime store actually holds.
fn replay_corpus(records: usize) -> Vec<String> {
    (0..records)
        .map(|i| {
            format!(
                "<{GMEOW}claim/{i}> <{GMEOW}statementAbout> <{GMEOW}term{}> ; \
                 <{GMEOW}accordingTo> <{GMEOW}standpoint/replay> .",
                i % 23
            )
        })
        .collect()
}

/// Write `corpus` into ONE append-only store, primed with `dict` — the production
/// shape, one segment header for the whole file.
fn one_header_store(dir: &Path, dict: &[u8], corpus: &[String]) -> Vec<u8> {
    let path = dir.join("one-header.gts");
    let memory = Memory::with_options(
        &path,
        MemoryOptions {
            dicts: vec![("gmeow-memory-hot-v1".to_string(), dict.to_vec())],
            dict: Some("gmeow-memory-hot-v1".to_string()),
            ..MemoryOptions::default()
        },
    );
    for text in corpus {
        memory
            .store(text, StoreOptions::default())
            .expect("the store accepts a record");
    }
    std::fs::read(&path).expect("read the store")
}

/// Write `corpus` with ONE SEGMENT HEADER PER CLAIM — each record in its own pack,
/// concatenated into one file, so every header re-pins the whole dictionary.
///
/// This is not a synthetic corruption: it is the shape a store had before purrdf's
/// append mode existed ("an N-claim file was N headers"), and the shape any future
/// change that stopped continuing the tail segment would silently reintroduce.
fn one_header_per_claim_store(dir: &Path, dict: &[u8], corpus: &[String]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for (i, text) in corpus.iter().enumerate() {
        let path = dir.join(format!("per-claim-{i}.gts"));
        let memory = Memory::with_options(
            &path,
            MemoryOptions {
                dicts: vec![("gmeow-memory-hot-v1".to_string(), dict.to_vec())],
                dict: Some("gmeow-memory-hot-v1".to_string()),
                ..MemoryOptions::default()
            },
        );
        memory
            .store(text, StoreOptions::default())
            .expect("the store accepts a record");
        out.extend(std::fs::read(&path).expect("read the per-claim pack"));
    }
    out
}

/// Write `corpus` unprimed — the declared no-dictionary counterfactual.
fn baseline_store(dir: &Path, corpus: &[String]) -> Vec<u8> {
    let path = dir.join("baseline.gts");
    let memory = Memory::with_options(&path, MemoryOptions::default());
    for text in corpus {
        memory
            .store(text, StoreOptions::default())
            .expect("the store accepts a record");
    }
    std::fs::read(&path).expect("read the store")
}

/// (e) A store that opens ONE SEGMENT HEADER PER CLAIM makes population B's gate RED,
/// while the same corpus in one append-only segment passes.
///
/// Both arms hold the SAME records and the SAME dictionary. The only difference is how
/// many times the file re-pins the dictionary in band — which is exactly the term the
/// runtime-store criterion turns on, and exactly the term a synthetic "sum of frame
/// payloads" measurement would have dropped.
#[test]
fn a_one_header_per_claim_store_reds_population_b() {
    let dir = tempfile::tempdir().expect("tempdir");
    // The DECLARED replay extent, and a dictionary trained over that same corpus at the
    // committed runtime-store winner's target. Both are load-bearing: a store dictionary
    // wins only once the records it primes outweigh its own bytes, so a smaller corpus
    // or a larger dictionary would make the append-only arm lose too and the RED below
    // would prove nothing about headers.
    let corpus = replay_corpus(sweep::REPLAY_RECORD_COUNT);
    let owned: Vec<Vec<u8>> = corpus
        .iter()
        .map(|text| text.clone().into_bytes())
        .collect();
    let samples: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let dict = train::build(DictionaryStrategy::Trained, &samples, 4096).expect("train");

    let baseline = baseline_store(dir.path(), &corpus);
    let healthy = one_header_store(dir.path(), &dict, &corpus);
    let pathological = one_header_per_claim_store(dir.path(), &dict, &corpus);

    let healthy_effect = measure::population_b(
        "gmeow-memory-hot-v1",
        &healthy,
        &baseline,
        owned.len() as u64,
        corpus.len() as u64,
    )
    .expect("the healthy store measures");
    assert_eq!(
        healthy_effect.dictionary_in_band_bytes,
        dict.len() as u64,
        "an append-only store pins the dictionary exactly once"
    );
    measure::check(std::slice::from_ref(&healthy_effect), &BTreeSet::new()).unwrap_or_else(|err| {
        panic!(
            "the append-only arm must pass, or the RED below proves nothing: two-part {} B vs \
             baseline {} B ({err})",
            healthy_effect.two_part_code_bytes(),
            healthy_effect.bytes_on_disk_baseline
        )
    });

    let bad = measure::population_b(
        "gmeow-memory-hot-v1",
        &pathological,
        &baseline,
        owned.len() as u64,
        corpus.len() as u64,
    )
    .expect("the pathological store measures");
    assert_eq!(
        bad.dictionary_in_band_bytes,
        dict.len() as u64 * corpus.len() as u64,
        "one header per claim re-pins the WHOLE dictionary each time — that is the cost the \
         two-part code must charge"
    );
    let diag = measure::check(&[bad], &BTreeSet::new())
        .expect_err("a one-header-per-claim store must hard-fail population B");
    assert_eq!(
        gmeow_errors::code::code_str(diag.code()),
        "pipeline.medium.dictionary-regression",
        "{diag}"
    );
    assert!(
        diag.to_string().contains("runtime-store-segments"),
        "{diag}"
    );
}

/// The declared replay extent is a CONSTANT, not a fixture author's taste: whether a
/// store dictionary wins is a pure function of the record count, so the count is part
/// of the claim.
#[test]
fn the_replay_corpus_extent_is_declared_and_bounded() {
    assert_eq!(sweep::REPLAY_RECORD_COUNT, 512);
    let baseline = committed_baseline();
    for row in &baseline.dictionaries {
        if row.population != Population::RuntimeStoreSegments.wire() {
            continue;
        }
        assert!(
            row.evaluated_frame_count > 0 && row.evaluated_frame_count <= 512,
            "{}: the committed runtime-store reading must record its replay cardinality \
             ({} records)",
            row.id,
            row.evaluated_frame_count
        );
    }
}
