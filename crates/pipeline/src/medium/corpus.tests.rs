// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use super::*;
use crate::medium::registry::fixture;

fn registry_of(extra: &str) -> Result<MediumRegistry, gmeow_errors::Diag> {
    MediumRegistry::from_dataset(&fixture::dataset(extra))
}

/// The fixture archive's members: enough of them that the DECLARED split
/// (modulus 8) partitions them both ways, which
/// `the_declared_split_partitions_the_fixture_archive_both_ways` pins so a
/// degenerate fixture can never make the split tests vacuous.
fn archive_members() -> Vec<(String, Vec<u8>)> {
    (0..64u32)
        .map(|i| {
            (
                format!("slices/core/gts/cell-{i:02}.ttl"),
                format!("<https://e/s> <https://e/p> <https://e/o{i}> .\n").into_bytes(),
            )
        })
        .collect()
}

struct Harness {
    dataset: Arc<RdfDataset>,
    archives: Vec<BlobRow>,
    artifacts: BTreeMap<String, Vec<u8>>,
    upstream: BTreeMap<String, StageProduct>,
    root: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp root");
        std::fs::create_dir_all(root.path().join("slices/core/gts")).expect("authored tree");
        std::fs::write(
            root.path().join("slices/core/gts/module.ttl"),
            b"# an authored source file the path-prefix selector reads\n",
        )
        .expect("authored file");

        let archive = purrdf::ustar::write_archive(&archive_members()).expect("fixture archive");

        Self {
            dataset: fixture::dataset(""),
            archives: vec![BlobRow {
                data: archive,
                media_type: "application/x-tar".to_string(),
                rep: "cells-archive".to_string(),
            }],
            artifacts: [(
                "generated/statements/claims.ttl".to_string(),
                b"<https://e/claim> <https://e/p> \"v\" .\n".to_vec(),
            )]
            .into(),
            upstream: BTreeMap::new(),
            root,
        }
    }

    fn sources(&self) -> CorpusSources<'_> {
        CorpusSources {
            root: self.root.path(),
            dataset: &self.dataset,
            archives: &self.archives,
            artifacts: &self.artifacts,
            upstream: &self.upstream,
        }
    }
}

#[test]
fn a_blob_rep_and_path_prefix_corpus_resolves_to_real_bytes() {
    let harness = Harness::new();
    let registry = registry_of("").expect("registry");
    let resolved = assemble(&registry, &gm("corpusCore"), &harness.sources())
        .expect("the core corpus resolves");
    // The archive MEMBER (not the tar) and the authored source file.
    assert!(
        resolved
            .training
            .iter()
            .any(|s| s.starts_with(b"<https://e/s>")),
        "the archive's members are the samples, not the tar itself"
    );
    assert!(
        resolved
            .training
            .iter()
            .any(|s| s.starts_with(b"# an authored source file")),
        "an AUTHORED path prefix reads the repo tree"
    );
}

#[test]
fn a_filename_selector_excludes_similarly_prefixed_siblings() {
    let mut harness = Harness::new();
    std::fs::write(
        harness.root.path().join("slices/core/gts/module.ttl.bak"),
        b"# must not enter the exact authored-file corpus\n",
    )
    .expect("authored sibling");
    harness.artifacts.insert(
        "generated/statements/claims.ttl.sig".to_string(),
        b"signature sibling must not enter the exact generated-file corpus\n".to_vec(),
    );
    let registry = registry_of(
        "gmeow:corpusExact a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsPathPrefix \"slices/core/gts/module.ttl\" ,\n\
             \x20       \"generated/statements/claims.ttl\" .",
    )
    .expect("registry");
    let resolved = assemble(&registry, &gm("corpusExact"), &harness.sources())
        .expect("exact file selectors resolve");

    assert_eq!(resolved.training.len(), 2);
    assert!(
        resolved
            .training
            .iter()
            .any(|sample| sample.starts_with(b"# an authored source file"))
    );
    assert!(
        resolved
            .training
            .iter()
            .any(|sample| sample.starts_with(b"<https://e/claim>"))
    );
    assert!(resolved.training.iter().all(|sample| {
        !sample.starts_with(b"# must not enter") && !sample.starts_with(b"signature sibling")
    }));
}

#[test]
fn repeated_graph_selectors_share_one_canonical_sample() {
    let mut harness = Harness::new();
    harness.dataset = purrdf::parse_dataset(
        b"@prefix ex: <https://e/> . ex:graph { ex:s ex:p \"value\" . }",
        "application/trig",
        None,
    )
    .expect("named-graph fixture");
    let registry = registry_of(
        "gmeow:corpusGraphA a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsGraph <https://e/graph> .\n\
             gmeow:corpusGraphB a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsGraph <https://e/graph> .",
    )
    .expect("registry");
    let mut cache = CorpusAssemblyCache::default();
    let first = assemble_with_cache(
        &registry,
        &gm("corpusGraphA"),
        &harness.sources(),
        &mut cache,
    )
    .expect("first resolution");
    let second = assemble_with_cache(
        &registry,
        &gm("corpusGraphB"),
        &harness.sources(),
        &mut cache,
    )
    .expect("second resolution");

    assert_eq!(cache.graph_samples.len(), 1);
    let first = first.training.first().expect("first sample");
    let second = second.training.first().expect("second sample");
    assert!(
        Arc::ptr_eq(first, second),
        "the second dictionary must reuse the canonical graph bytes"
    );
}

/// The members the declared split holds out of `archive_members()`, computed the
/// way a READER would: rank by content digest, take every stride-th.
fn expected_held_out(split: &crate::medium::registry::TrainingSplitDef) -> Vec<Vec<u8>> {
    let ranked: BTreeMap<String, Vec<u8>> = archive_members()
        .into_iter()
        .map(|(_, bytes)| (crate::medium::blake3_digest(&bytes), bytes))
        .collect();
    ranked
        .into_values()
        .enumerate()
        .filter(|(rank, _)| split.holds_out_rank(*rank))
        .map(|(_, bytes)| bytes)
        .collect()
}

/// The split PARTITIONS the fixture archive — both sides non-empty — and does so
/// by a stride over content-digest rank, which is what makes properness a
/// theorem rather than a coin flip on a small corpus.
#[test]
fn the_declared_split_partitions_the_fixture_archive_both_ways() {
    let registry = registry_of("").expect("registry");
    let split = registry.training_split().expect("the fixture declares one");
    let total = archive_members().len();
    let held = expected_held_out(split).len();
    assert_eq!(
        held,
        total.div_ceil(split.stride as usize),
        "one member in every {} is held out, in content-digest rank order",
        split.stride
    );
    assert!(held > 0 && held < total, "{held} of {total} held out");
}

/// The trainer NEVER sees the held-out members, and the resolution says how many
/// it held out.
#[test]
fn the_declared_split_keeps_held_out_members_out_of_the_training_set() {
    let harness = Harness::new();
    let registry = registry_of("").expect("registry");
    let split = registry.training_split().expect("the fixture declares one");
    let resolved = assemble(&registry, &gm("corpusCore"), &harness.sources())
        .expect("the core corpus resolves");

    let expected_held = expected_held_out(split);
    assert_eq!(resolved.held_out_count, expected_held.len() as u64);
    for held in &expected_held {
        assert!(
            !resolved.training.contains(held.as_slice()),
            "a held-out member reached the trainer — the evaluation would be over bytes the \
                 dictionary memorized"
        );
    }
    // …and it is a PROPER subset: the training side is the larger one.
    assert!(resolved.training.len() > resolved.held_out_count as usize);
}

/// An archive-backed corpus the declared split does not PARTITION is a hard fail
/// — and there is no exemption. A one-member archive is that case: whichever side
/// it lands on, the other side is empty.
#[test]
fn an_archive_the_split_cannot_partition_hard_fails() {
    let mut harness = Harness::new();
    let registry = registry_of("").expect("registry");
    harness.archives = vec![BlobRow {
        data: purrdf::ustar::write_archive(&archive_members()[..1]).expect("fixture archive"),
        media_type: "application/x-tar".to_string(),
        rep: "cells-archive".to_string(),
    }];
    let diag = assemble(&registry, &gm("corpusCore"), &harness.sources())
        .expect_err("an unsplittable archive-backed corpus must hard-fail");
    assert!(
        diag.to_string().contains("does not partition them")
            && diag.to_string().contains("do NOT exempt"),
        "{diag}"
    );
}

/// A carrier with no declared `gmeow:CorpusTrainingSplit` refuses an
/// archive-backed corpus rather than quietly training on every member.
#[test]
fn an_archive_backed_corpus_without_a_declared_split_hard_fails() {
    let harness = Harness::new();
    let text = fixture::turtle("").replace("a gmeow:CorpusTrainingSplit", "a gmeow:Retired");
    let ds = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).expect("turtle");
    let registry = MediumRegistry::from_dataset(&ds).expect("registry");
    let diag = assemble(&registry, &gm("corpusCore"), &harness.sources())
        .expect_err("a corpus with no declared split must hard-fail");
    assert!(
        diag.to_string().contains("gmeow:CorpusTrainingSplit"),
        "{diag}"
    );
}

/// Two declared splits leave "which members did this dictionary never see" with
/// two answers, so the registry refuses to read at all.
#[test]
fn two_declared_splits_are_rejected() {
    let diag = registry_of(
        "gmeow:corpusTrainingSplitV2 a gmeow:CorpusTrainingSplit ;\n\
             \x20   gmeow:splitHeldOutStride 4 ;\n\
             \x20   gmeow:splitHeldOutOffset 1 .",
    )
    .expect_err("two splits must be rejected");
    assert!(diag.to_string().contains("two answers"), "{diag}");
}

/// (d) A declared dictionary whose selector matches nothing hard-fails with
/// `pipeline.medium.undeclared-dictionary` — never a silently empty dictionary.
#[test]
fn a_corpus_matching_nothing_hard_fails_as_undeclared() {
    let mut harness = Harness::new();
    harness.artifacts = BTreeMap::new();
    harness.archives = Vec::new();
    let registry = registry_of(
        "gmeow:corpusEmpty a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsPathPrefix \"generated/nothing-here/\" .",
    )
    .expect("registry");
    let diag = assemble(&registry, &gm("corpusEmpty"), &harness.sources())
        .expect_err("an empty corpus must hard-fail");
    assert_eq!(
        diag.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{diag}"
    );
    assert_eq!(
        crate::error::MediumUndeclaredDictionary::CODE,
        "pipeline.medium.undeclared-dictionary"
    );
}

/// (e) An unrecognized selector predicate hard-fails. Skipping it would train on
/// a strict subset of the declaration with nothing to surface the gap.
#[test]
fn an_unrecognized_selector_predicate_hard_fails() {
    let diag = registry_of(
        "gmeow:corpusWeird a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsBlobRep \"cells-archive\" ;\n\
             \x20   gmeow:corpusSelectsMoonPhase \"waxing\" .",
    )
    .expect_err("an unknown selector must be rejected");
    assert!(
        diag.to_string().contains("corpusSelectsMoonPhase")
            && diag.to_string().contains("never a silent skip"),
        "{diag}"
    );
}

/// (f) A selector covering `graph/medium-registry` is rejected — the cycle
/// dictionary → registry → corpus → dictionary never converges.
#[test]
fn a_selector_covering_the_medium_registry_graph_is_rejected() {
    let diag = registry_of(&format!(
        "gmeow:corpusLoop a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsGraph <{MEDIUM_REGISTRY_GRAPH}> ."
    ))
    .expect_err("a fixpoint selector must be rejected");
    assert!(diag.to_string().contains("close a cycle"), "{diag}");

    // The measurement graph is excluded for the same reason.
    let diag = registry_of(&format!(
        "gmeow:corpusLoop a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsGraph <{MEDIUM_MEASUREMENT_GRAPH}> ."
    ))
    .expect_err("the measurement graph is excluded too");
    assert!(diag.to_string().contains("close a cycle"), "{diag}");
}

/// Path-prefix coverage runs BOTH ways: a prefix INSIDE `generated/medium/` and
/// a prefix that CONTAINS it are equally a cycle.
#[test]
fn a_path_prefix_covering_the_emitted_family_is_rejected_in_both_directions() {
    for prefix in ["generated/", "generated/medium/", "generated/medium/v1/"] {
        let diag = registry_of(&format!(
            "gmeow:corpusLoop a gmeow:DictionaryCorpus ;\n\
                 \x20   gmeow:corpusSelectsPathPrefix \"{prefix}\" ."
        ))
        .unwrap_err();
        assert!(
            diag.to_string().contains("close a cycle"),
            "prefix {prefix:?} must be rejected: {diag}"
        );
    }
    // A sibling family under `generated/` is NOT covered — the exclusion is
    // narrow, not a blanket ban on generated material.
    registry_of(
        "gmeow:corpusFine a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsPathPrefix \"generated/statements/\" .",
    )
    .expect("a sibling generated/ family is legitimate corpus material");
}

/// A blob rep names a CONTAINER, so its coverage is decided by its contents.
#[test]
fn an_archive_reaching_into_the_emitted_family_is_rejected() {
    let mut harness = Harness::new();
    harness.archives = vec![BlobRow {
        data: purrdf::ustar::write_archive(&[(
            format!("{MEDIUM_GENERATED_PREFIX}gmeow-core-v1.zdict"),
            vec![1, 2, 3],
        )])
        .expect("fixture archive"),
        media_type: "application/x-tar".to_string(),
        rep: "cells-archive".to_string(),
    }];
    let registry = registry_of("").expect("registry");
    let diag = assemble(&registry, &gm("corpusCore"), &harness.sources())
        .expect_err("an archive carrying the medium output must be rejected");
    assert!(diag.to_string().contains("closes a cycle"), "{diag}");
}

/// A stage-product selector reaches the named product's artifact lane without
/// falling back to a committed file.
#[test]
fn a_stage_product_selector_reads_the_named_product() {
    let mut harness = Harness::new();
    harness.upstream.insert(
        "stage-reason".to_string(),
        StageProduct::from_artifacts(
            "stage-reason",
            [(
                "generated/reasoning/explanations.ttl".to_string(),
                b"<https://e/why> <https://e/because> \"rule\" .\n".to_vec(),
            )]
            .into(),
        ),
    );
    let registry = registry_of(
        "gmeow:corpusReason a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsStageProduct gmeow:stage-reason .",
    )
    .expect("registry");
    let resolved = assemble(&registry, &gm("corpusReason"), &harness.sources())
        .expect("the stage-product corpus resolves");
    assert!(
        resolved
            .training
            .iter()
            .any(|s| s.starts_with(b"<https://e/why>"))
    );
    assert_eq!(
        resolved.held_out_count, 0,
        "a stage-product corpus carries no archive members to split"
    );
}

/// A stage-product selector whose product carries the medium registry graph is
/// a transitive cycle even though the selector named only a stage.
#[test]
fn a_stage_product_carrying_the_registry_graph_is_rejected() {
    let mut harness = Harness::new();
    let trig = format!("<{MEDIUM_REGISTRY_GRAPH}> {{ <https://e/real> <https://e/p> \"v\" . }}\n");
    let dataset = purrdf::parse_dataset(trig.as_bytes(), "application/trig", None).expect("trig");
    harness.upstream.insert(
        "stage-reason".to_string(),
        StageProduct::from_artifacts_over(
            "stage-reason",
            dataset,
            [("generated/reasoning/x.ttl".to_string(), b"# x\n".to_vec())].into(),
        ),
    );
    let registry = registry_of(
        "gmeow:corpusReason a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsStageProduct gmeow:stage-reason .",
    )
    .expect("registry");
    let diag = assemble(&registry, &gm("corpusReason"), &harness.sources())
        .expect_err("a product carrying the registry graph must be rejected");
    assert!(diag.to_string().contains("closes a cycle"), "{diag}");
}

/// A stage-product selector naming a stage this stage does not consume is a
/// missing dataflow edge, not permission to read that stage's output from disk.
#[test]
fn a_stage_product_selector_without_an_upstream_edge_hard_fails() {
    let harness = Harness::new();
    let registry = registry_of(
        "gmeow:corpusReason a gmeow:DictionaryCorpus ;\n\
             \x20   gmeow:corpusSelectsStageProduct gmeow:stage-reason .",
    )
    .expect("registry");
    let diag = assemble(&registry, &gm("corpusReason"), &harness.sources())
        .expect_err("a missing upstream product must hard-fail");
    assert!(
        diag.to_string().contains("gmeow:dataflowConsumes"),
        "{diag}"
    );
}

/// The term-table rendering is canonical, vocabulary-only, and stable across
/// two parses of isomorphic input — the property that makes a term-table
/// dictionary explainable and RNG-free.
#[test]
fn the_term_table_rendering_is_canonical_and_vocabulary_only() {
    let a = purrdf::parse_dataset(
        b"<https://e/s> <https://e/p> \"v\" .\n<https://e/s> <https://e/q> _:x .\n",
        "application/n-triples",
        None,
    )
    .expect("parse");
    let b = purrdf::parse_dataset(
        b"<https://e/s> <https://e/q> _:renamed .\n<https://e/s> <https://e/p> \"v\" .\n",
        "application/n-triples",
        None,
    )
    .expect("parse");
    let rendered = term_table_sample(&a);
    assert_eq!(
        rendered,
        term_table_sample(&b),
        "the rendering must not depend on blank labels or parse order"
    );
    let text = String::from_utf8(rendered).expect("utf-8");
    assert!(text.contains("<https://e/p>\n"), "{text}");
    assert!(text.contains("\"v\"\n"), "{text}");
    assert!(!text.contains("_:"), "blank labels are excluded: {text}");
    // Sorted, one term per line — a canonical rendering, not an interning-order dump.
    let lines: Vec<&str> = text.lines().collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted);
}
