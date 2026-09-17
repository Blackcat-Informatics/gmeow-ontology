// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::medium::registry::fixture;

fn registry() -> MediumRegistry {
    MediumRegistry::from_dataset(&fixture::dataset("")).expect("fixture registry")
}

fn effect(bytes: u64, baseline: u64, in_band: u64) -> DictionaryEffect {
    DictionaryEffect {
        dictionary_id: "gmeow-core-v1".to_string(),
        population: Population::EmittedBlobFrames,
        bytes_on_disk: bytes,
        bytes_on_disk_baseline: baseline,
        dictionary_in_band_bytes: in_band,
        corpus_sample_count: 12,
        evaluated_frame_count: 3,
    }
}

/// The gain fraction is BOUNDED in `[0, 1]` in every direction, including the
/// degenerate ones — an unbounded ratio would be unusable on a scale.
#[test]
fn the_gain_fraction_is_bounded_in_zero_one() {
    assert_eq!(effect(400, 1000, 100).gain_fraction_lexical(), "0.500000");
    // A dictionary that costs more than it saves is clamped, never negative.
    assert_eq!(effect(900, 1000, 500).gain_fraction_lexical(), "0.000000");
    // An empty baseline is zero rather than a division by zero.
    assert_eq!(effect(0, 0, 0).gain_fraction_lexical(), "0.000000");
    // A free dictionary saving everything saturates at 1.
    assert_eq!(effect(0, 1000, 0).gain_fraction_lexical(), "1.000000");
}

/// The two-part code charges the dictionary's OWN bytes — the property that keeps
/// the criterion non-vacuous under a train/test overlap.
#[test]
fn the_two_part_code_charges_the_dictionary_its_own_bytes() {
    let saves_but_costs_more = effect(500, 1000, 600);
    assert_eq!(saves_but_costs_more.two_part_code_bytes(), 1100);
    assert!(
        !saves_but_costs_more.wins(),
        "a dictionary whose bytes cost more than it saves must lose even though the frames \
             got smaller"
    );
    assert!(effect(500, 1000, 400).wins());
    // Strictly less: a tie is a loss (the dictionary raised the reader contract for
    // nothing).
    assert!(!effect(500, 1000, 500).wins());
}

/// The gate names the DECLARED snapshot exclusion in its failure message, so a
/// reader is never left to infer which frames were evaluated.
#[test]
fn the_gate_reds_and_names_the_declared_exclusion() {
    let diag = check(&[effect(900, 1000, 500)], &BTreeSet::new())
        .expect_err("a losing dictionary must hard-fail");
    assert_eq!(
        diag.code(),
        crate::error::MediumDictionaryRegression::register(),
        "{diag}"
    );
    let text = diag.to_string();
    assert!(text.contains("snapshot frame"), "{text}");
    assert!(text.contains("no threshold to relax"), "{text}");
    check(&[effect(400, 1000, 100)], &BTreeSet::new()).expect("a winning dictionary passes");
}

/// A declared dictionary with NO measured row is a failure, not an uncovered row:
/// that is exactly how a dictionary would slip in without gate coverage.
#[test]
fn a_declared_dictionary_with_no_measured_row_hard_fails() {
    let required: BTreeSet<String> = ["gmeow-core-v1".to_string(), "gmeow-terms-v1".to_string()]
        .into_iter()
        .collect();
    let diag = check(&[effect(400, 1000, 100)], &required)
        .expect_err("an unmeasured declared dictionary must hard-fail");
    assert_eq!(
        diag.code(),
        crate::error::MediumDictionaryRegression::register(),
        "{diag}"
    );
    assert!(diag.to_string().contains("gmeow-terms-v1"), "{diag}");
}

/// The encode runs the MANDATED chain: `zstd-rsyncable` primes EVERY independent
/// block with the dictionary, so a dictionary-primed encode of repetitive RDF is
/// materially smaller than the unprimed one at the same level.
#[test]
fn the_measured_chain_is_zstd_rsyncable_and_dictionary_primed() {
    let owned: Vec<Vec<u8>> = (0..400u32)
        .map(|i| {
            format!(
                "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {i} in the gmeow ontology\" .\n",
                i % 37
            )
            .into_bytes()
        })
        .collect();
    let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let dict = crate::medium::train::build(
        crate::medium::registry::DictionaryStrategy::Trained,
        &corpus,
        4096,
    )
    .expect("train");
    let payload = owned[0].clone();
    let primed = encoded_len("zstd-rsyncable", 12, Some(&dict), &payload).expect("primed");
    let bare = encoded_len("zstd-rsyncable", 12, None, &payload).expect("bare");
    assert!(
        primed < bare,
        "a primed encode of one small RDF record must beat the unprimed one: {primed} vs \
             {bare}"
    );
    // A codec the writer cannot encode with is a HARD FAIL, never a proxy.
    assert!(encoded_len("brotli", 12, None, &payload).is_err());
}

/// Population A groups by the ASSIGNED dictionary and skips baseline-assigned reps
/// — a rep already written through the no-dictionary medium has no dictionary to
/// pay for.
#[test]
fn population_a_groups_by_the_assigned_dictionary() {
    let registry = registry();
    let owned: Vec<Vec<u8>> = (0..400u32)
        .map(|i| format!("<https://e/s{}> <https://e/p> \"v{i}\" .\n", i % 29).into_bytes())
        .collect();
    let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    let dict = crate::medium::train::build(
        crate::medium::registry::DictionaryStrategy::Trained,
        &corpus,
        4096,
    )
    .expect("train");
    let rows = [BlobRow {
        data: owned.concat(),
        media_type: "application/x-tar".to_string(),
        rep: "cells-archive".to_string(),
    }];
    let borrowed: Vec<&BlobRow> = rows.iter().collect();
    let trained: BTreeMap<String, Vec<u8>> = [("gmeow-core-v1".to_string(), dict)].into();
    let sample_counts: BTreeMap<String, u64> = [("gmeow-core-v1".to_string(), 400)].into();
    let effects = population_a(
        &registry,
        &borrowed,
        &trained,
        &sample_counts,
        "zstd-rsyncable",
        12,
    )
    .expect("population A");
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].dictionary_id, "gmeow-core-v1");
    assert_eq!(effects[0].evaluated_frame_count, 1);
    assert_eq!(effects[0].corpus_sample_count, 400);
    assert!(effects[0].bytes_on_disk > 0 && effects[0].bytes_on_disk_baseline > 0);
}

/// A measurement that also rides its `graph/fanout/…` reconstruction twin is read
/// back ONCE.
///
/// The emission folds both copies (`serialize_snapshot` pushes the measurement graph
/// and the re-rooted twin the superset gate rebuilds `generated/medium/
/// dictionary-effect.ttl` from), so a reader that scanned every graph would report
/// one dictionary's single two-part code twice — which is what `gmeow medium
/// explain` did.
#[test]
fn the_fanout_twin_does_not_double_count_a_measurement() {
    let registry = registry();
    let quads = project(&registry, &[effect(400, 1000, 100)]).expect("project");
    // Derived through the SAME function the emission re-roots the twin with, so this
    // fixture cannot drift from the graph the bundle actually carries.
    let fanout_graph = RdfTerm::iri(
        crate::stages::superset::rdf_fanout_graph_iri(MEDIUM_EFFECT_PATH)
            .expect("the dictionary-effect fanout path is an RDF path"),
    );
    let twinned: Vec<RdfQuad> = quads
        .iter()
        .cloned()
        .chain(quads.iter().cloned().map(|mut quad| {
            quad.graph_name = Some(fanout_graph.clone());
            quad
        }))
        .collect();
    let dataset = purrdf::dataset_from_quads(&twinned).expect("the twinned graphs freeze");
    let read_back = effects(&registry, dataset.as_ref()).expect("the rows read back");
    assert_eq!(
        read_back.len(),
        1,
        "the fanout twin is the SAME measurement in its transport graph, not a second \
             one: {read_back:?}"
    );
    assert_eq!(read_back[0], effect(400, 1000, 100));
}

/// The projection lands entirely in `graph/medium-measurement` — NOT in
/// `graph/medium-registry` — and carries exactly one `gmeow:observationMethod`.
#[test]
fn the_projection_lands_in_the_measurement_graph_with_one_method() {
    let registry = registry();
    let quads = project(&registry, &[effect(400, 1000, 100)]).expect("project");
    assert!(!quads.is_empty());
    assert!(
        quads
            .iter()
            .all(|q| q.graph_name == Some(RdfTerm::iri(MEDIUM_MEASUREMENT_GRAPH))),
        "a measurement must never land in graph/medium-registry: the registry says what the \
             dictionaries ARE, the measurement says what they DO"
    );
    let subject = measurement_iri("gmeow-core-v1", Population::EmittedBlobFrames);
    let methods = quads
        .iter()
        .filter(|q| {
            q.subject == RdfTerm::iri(subject.as_str()) && q.predicate == gm("observationMethod")
        })
        .count();
    assert_eq!(
        methods, 1,
        "gmeow:Measurement carries a min/max-1 qualified cardinality on \
             gmeow:observationMethod"
    );
    let predicates: Vec<&str> = quads.iter().map(|q| q.predicate.as_str()).collect();
    for required in [
        "measurementBytesOnDisk",
        "measurementBytesOnDiskBaseline",
        "measurementDictionaryInBandBytes",
        "measurementTwoPartCodeBytes",
        "measurementCorpusSampleCount",
        "measurementEvaluatedFrameCount",
        "measurementPopulation",
        "measuresDictionary",
        "observedFeature",
    ] {
        assert!(
            predicates.contains(&gm(required).as_str()),
            "the projection must carry gmeow:{required}"
        );
    }
    // The gain fraction rides a math:Quantity, never a bare literal on the
    // observation (gmeow:observationResult's range is logic:Individual).
    assert!(
        quads
            .iter()
            .any(|q| q.predicate == format!("{MATH}quantityValue")),
        "the bounded gain fraction must ride a math:Quantity"
    );
}

/// The projection is a pure function of the measured SET, not of push order — the
/// property that makes the committed `.ttl` byte-identical across two runs.
#[test]
fn the_projection_is_emission_order_independent() {
    let registry = registry();
    let mut a = effect(400, 1000, 100);
    a.dictionary_id = "gmeow-core-v1".to_string();
    let mut b = effect(300, 900, 90);
    b.dictionary_id = "gmeow-terms-v1".to_string();
    let forward = project(&registry, &[a.clone(), b.clone()]).expect("project");
    let reversed = project(&registry, &[b, a]).expect("project");
    assert_eq!(forward, reversed);
}

/// A store whose header pins the dictionary ONCE PER RECORD pays for it once per
/// record — the exact shape that makes a runtime-store dictionary lose.
#[test]
fn the_in_band_cost_is_counted_per_segment_header() {
    let dict = vec![7u8; 512];
    let one = fake_store(&[("gmeow-memory-hot-v1", dict.clone())], 1);
    let many = fake_store(&[("gmeow-memory-hot-v1", dict.clone())], 8);
    assert_eq!(
        in_band_dictionary_bytes(&one, "gmeow-memory-hot-v1").expect("one header"),
        512
    );
    assert_eq!(
        in_band_dictionary_bytes(&many, "gmeow-memory-hot-v1").expect("eight headers"),
        8 * 512,
        "each segment header re-pins the whole dictionary, and the two-part code charges \
             every copy"
    );
    // A store pinning nothing cannot be measured: it would win vacuously.
    let bare = fake_store(&[], 3);
    assert_eq!(
        in_band_dictionary_bytes(&bare, "gmeow-memory-hot-v1").expect("no entry"),
        0
    );
    assert!(population_b("gmeow-memory-hot-v1", &bare, &bare, 1, 1).is_err());
}

/// A minimal CBOR sequence of `headers` GTS segment headers, each pinning `dicts`.
fn fake_store(dicts: &[(&str, Vec<u8>)], headers: usize) -> Vec<u8> {
    use ciborium::value::Value;
    let mut out: Vec<u8> = Vec::new();
    for _ in 0..headers {
        let dct = Value::Map(
            dicts
                .iter()
                .map(|(name, bytes)| {
                    (
                        Value::Text((*name).to_string()),
                        Value::Bytes(bytes.clone()),
                    )
                })
                .collect(),
        );
        let header = Value::Map(vec![
            (Value::Text("gts".into()), Value::Text("GTS1".into())),
            (Value::Text("dct".into()), dct),
        ]);
        ciborium::ser::into_writer(&header, &mut out).expect("cbor");
    }
    out
}
