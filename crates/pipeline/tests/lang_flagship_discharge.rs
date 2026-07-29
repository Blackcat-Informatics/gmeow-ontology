// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `lang:` grounding layer's flagship execution-discharge harness.
//!
//! The math flagship precedent (`math:FlagshipScenario`) discharges its acceptance bar by
//! EXISTENCE — a manifest names the artifacts and three surfaces check they are present and
//! fully linked. This harness goes one rung further: it discharges the language grounding
//! layer's five flagships by EXECUTION, through the SLICE-GENERIC discharge core in
//! [`support::flagship_discharge`]. The generic runner reads the acceptance manifest
//! `slices/grounding/lang/examples/flagship-acceptance.ttl`, and for each of the five
//! `gmeow:FlagshipScenario` individuals it:
//!
//! 1. **Runs the guard.** Loads the `gmeow:guardedByCounterExample` fixture and pushes it
//!    through BOTH native validation channels — the structural lint and the native SHACL
//!    engine — and asserts the UNION of triggered `lang:` failure classes equals EXACTLY the
//!    one named by `gmeow:enforcesFailureClass`. The gate must bite, and bite for precisely
//!    the declared reason.
//! 2. **Checks the worked example.** Loads the `gmeow:demonstratedByExample` fixture, runs the
//!    SAME two channels, and asserts NO `lang:` failure class fires — the positive is
//!    well-formed.
//! 3. **Runs the producer.** This file supplies the per-slice producer callback: it dispatches
//!    the `gmeow:demonstratedByProducer` identifier to the named native entrypoint, RUNS it,
//!    and asserts its output carries the structure that flagship claims (a compositional
//!    lowering with per-stage exact preservation, a prose-lift corpus with prose-hashes and
//!    exact round-trips, a translation corpus with per-unit preservation judgments, a
//!    grammar-projection corpus with exact round-trips and per-reading routing).
//!
//! The five (counter-example, example, failure-class, producer) tuples are READ from the
//! manifest, never hard-coded — so a manifest edit that unwires a flagship is caught here.

use std::collections::{HashMap, HashSet};

use gmeow_lang_bridge::lower::{flagship_svo_sentence, lower_svo};
use gmeow_logic_compile::ir::PreservationKind;

mod support;
use support::flagship_discharge::{
    Flagship, FlagshipCtx, SliceSpec, local_name, native_failure_classes, repo_root,
    run_flagship_discharge,
};

/// The `lang:` grounding namespace (byte-identical to every `lang:` producer). Used for the
/// SCANNED failure classes (`lang:<Class>`), which stay slice-namespaced.
use gmeow_ns::LANG_NS;

/// The `lang:` slice's discharge identity: its base IRI, short prefix, on-disk root, and the
/// acceptance-manifest path relative to that root.
fn lang_spec() -> SliceSpec {
    SliceSpec {
        slice_ns: LANG_NS,
        slice_prefix: "lang",
        slice_root: repo_root().join("slices").join("grounding").join("lang"),
        manifest_rel: "examples/flagship-acceptance.ttl",
    }
}

#[test]
fn every_flagship_is_discharged_by_execution() {
    run_flagship_discharge(&lang_spec(), 5, &run_producer);
}

/// Dispatch and RUN a `lang:` flagship's `gmeow:demonstratedByProducer`, asserting the executed
/// output carries the structure the flagship claims. This is the `lang:`-specific producer
/// callback the generic runner invokes once per scenario.
fn run_producer(flagship: &Flagship, ctx: &FlagshipCtx<'_>) {
    let catalog = ctx.catalog;
    match flagship.producer.as_str() {
        // FS1: the compositional-lowering corpus PRODUCER — the production wiring that folds the
        // SVO lowering into gmeow.gts. A sentence lowers, compositionally, to a full-FOL formula;
        // every stage is declared and Exact (asserted on the SAME `lower_svo` the producer runs),
        // and the FOLDED corpus N-Triples carry the compositional formula (the `chase` relation).
        "pipeline::stages::lang_lowering::build_corpus" => {
            // The stage-structure asserts, on the same lowering the producer runs.
            let lowering =
                lower_svo(&flagship_svo_sentence()).expect("FS1: the flagship SVO sentence lowers");
            lowering
                .assert_all_stages_declared()
                .expect("FS1: every lowering stage is declared");
            for stage in &lowering.stages {
                assert_eq!(
                    stage.preservation,
                    PreservationKind::Exact,
                    "FS1: the modeled fragment lowers exactly, stage {}",
                    stage.name
                );
            }

            // The FOLDED production corpus: the bundle projection the pipeline lands in gmeow.gts.
            let corpus = gmeow_pipeline::stages::lang_lowering::build_corpus()
                .expect("FS1: the compositional-lowering corpus builds");
            let nt = String::from_utf8(corpus.ntriples)
                .expect("FS1: corpus N-Triples emission is UTF-8");
            assert!(
                !nt.trim().is_empty(),
                "FS1: the folded lowering corpus is non-empty"
            );
            for needle in [
                "CompositionalLowering", // the lowering typing
                "LoweringStage",         // the per-stage preservation records
                "chase",                 // the compositional formula's `chase` relation
            ] {
                assert!(
                    nt.contains(needle),
                    "FS1: the folded lowering corpus must carry {needle}"
                );
            }
            // Every folded stage records an exact preservation (no lossy lowering shipped).
            let exact = PreservationKind::Exact.iri();
            let stage_exact = nt
                .lines()
                .filter(|l| l.contains("preservationKind") && l.contains(&exact))
                .count();
            assert_eq!(
                stage_exact,
                lowering.stages.len(),
                "FS1: every folded lowering stage records an exact preservation"
            );
        }

        // FS2: the prose-lift stage. Every @x-gmeow-english literal is lifted to a
        // content-addressed surface carrying its prose-hash and an exact surface round-trip.
        "pipeline::stages::lang_form::build_corpus" => {
            let corpus = gmeow_pipeline::stages::lang_form::build_corpus(Some(catalog))
                .expect("FS2: the prose-lift corpus builds over the real slice catalog");
            let nt = String::from_utf8(corpus.ntriples).expect("FS2: corpus N-Triples is UTF-8");
            assert!(
                !nt.trim().is_empty(),
                "FS2: the prose-lift corpus is non-empty"
            );
            for needle in [
                "candidateSourceHash",   // the prose-hash (logic:candidateSourceHash)
                "surfaceCorrespondence", // the surface round-trip Correspondence
                "surfaceText",           // the lifted surface literal
                "ExactPreservation",     // the folded exact-preservation ledger judgment
            ] {
                assert!(
                    nt.contains(needle),
                    "FS2: the prose-lift corpus must carry {needle}"
                );
            }

            // The TOTALITY the flagship advertises, discharged on the PRODUCTION corpus by
            // the contract artifact itself: every DISTINCT @x-gmeow-english literal in the
            // extraction universe is lifted to a reachable lang:SurfaceForm (inline
            // surfaceText, or a by-reference surfaceBlob digest for document-scale surfaces),
            // so `covered == universe` — the count-equality, not mere presence of a token.
            let coverage = gmeow_pipeline::stages::lang_form::prose_lift_coverage(Some(catalog))
                .expect("FS2: prose-lift coverage computes over the real slice catalog");
            assert!(
                coverage.universe > 0,
                "FS2: the source bundle must carry @x-gmeow-english prose to lift"
            );
            assert_eq!(
                coverage.covered,
                coverage.universe,
                "FS2: {} of {} distinct @x-gmeow-english literals are not lifted — the prose \
                 lift is not total",
                coverage.universe - coverage.covered,
                coverage.universe
            );
        }

        // FS3: the translation stage. The multilingual docs are lang:TranslationUnits, each
        // carrying a per-unit preservation judgment rather than a silent Exact default.
        "pipeline::stages::lang_translation::build_corpus" => {
            let corpus = gmeow_pipeline::stages::lang_translation::build_corpus(&ctx.repo_root)
                .expect("FS3: the translation corpus builds over the real .po catalogs");
            let nt = String::from_utf8(corpus.ntriples).expect("FS3: corpus N-Triples is UTF-8");
            for needle in ["TranslationUnit", "preservationKind"] {
                assert!(
                    nt.contains(needle),
                    "FS3: the translation corpus must carry {needle}"
                );
            }
        }

        // FS4 and FS5 both name the projection stage, so the shared producer string cannot
        // tell them apart. Their DISTINCTIVE claims are discharged SEPARATELY, keyed on the
        // flagship node's local name, so each is INDEPENDENTLY falsifiable over the SAME real
        // projection corpus:
        //   FS4 (serializationsAsGrammars) — the serialization grammars are lang:Grammar
        //     objects whose emit/parse round-trip is exact (a lossless crossing).
        //   FS5 (ambiguityHeldHonestly) — a genuinely ambiguous authored form projects its
        //     readings as first-class CO-RESIDENT data: the corpus carries an emission with
        //     lang:emittedReadingCount >= 2 AND the MATCHING number of co-resident reading
        //     artifacts (one per reading), so the projection holds the ambiguity rather than
        //     silently collapsing it to a single winner. The projection stage's Invariant 3
        //     hard-fail (`lang:ProjectionSilentDisambiguation`) is the negative teeth; this is
        //     the POSITIVE discharge on the shipped surface.
        "pipeline::stages::lang_projection::build_corpus" => {
            let corpus = gmeow_pipeline::stages::lang_projection::build_corpus(Some(catalog))
                .expect("FS4/FS5: the projection corpus builds over the real grammars");
            let nt = String::from_utf8(corpus.ntriples).expect("corpus N-Triples is UTF-8");
            for needle in [
                "Grammar",            // lang:Grammar objects
                "ProjectionEmission", // one emission per grammar / per reading
                "ExactPreservation",  // the lossless crossing judgment
                "roundTripHolds",     // the measured emit/parse round-trip
            ] {
                assert!(
                    nt.contains(needle),
                    "FS4/FS5: the projection corpus must carry {needle}"
                );
            }
            assert!(
                !nt.contains("roundTripHolds \"false\""),
                "FS4/FS5: no Exact emission may record a failing round-trip"
            );

            match local_name(&flagship.subject).as_str() {
                // FS4: the grammar round-trip is exact — the authored *.ebnf grammars drive
                // lang:Grammar objects and EBNF projection artifacts whose crossing is Exact.
                "serializationsAsGrammars" => {
                    assert!(
                        corpus
                            .artifacts
                            .iter()
                            .any(|(p, _)| p.starts_with("generated/projections/lang/ebnf/")),
                        "FS4: the serialization grammars must drive EBNF projection artifacts"
                    );
                }
                // FS5: the ambiguity is HELD — a co-resident emission with >= 2 readings and
                // exactly that many co-resident reading artifacts on the shipped surface.
                "ambiguityHeldHonestly" => assert_ambiguity_held(&nt, &corpus.artifacts),
                other => panic!(
                    "flagship {}: a lang_projection producer is bound to an unexpected flagship \
                     node {other:?}; FS4/FS5 are the only projection-stage flagships",
                    flagship.subject
                ),
            }
        }

        other => panic!(
            "flagship {}: unknown gmeow:demonstratedByProducer identifier {other:?}",
            flagship.subject
        ),
    }
}

/// The FS5 positive discharge: over the REAL projection corpus, a genuinely ambiguous
/// authored form keeps every reading as first-class co-resident data. Assert that some
/// `lang:ProjectionEmission` in the corpus declares `lang:emittedReadingCount` >= 2 AND that
/// the co-resident reading artifacts for that emission's source number EXACTLY that count
/// (>= 2, one artifact per reading) — the ambiguity is held on the shipped surface, never
/// silently collapsed to a single winner.
///
/// This is independently falsifiable from FS4: it fails if no authored form projects two
/// co-resident readings (the count drops to 1), or if the per-reading artifacts do not match
/// the declared reading count — neither of which FS4's grammar-round-trip assert can mask.
fn assert_ambiguity_held(nt: &str, artifacts: &[(String, Vec<u8>)]) {
    let pred_count = format!("{LANG_NS}emittedReadingCount");
    let pred_source = format!("{LANG_NS}projectsSource");

    // Per emission subject, the declared reading count and the projected source IRI.
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut sources: HashMap<String, String> = HashMap::new();
    for line in nt.lines() {
        let mut parts = line.splitn(3, ' ');
        let (Some(subj), Some(pred), Some(obj)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let subj = subj.trim_matches(['<', '>']).to_owned();
        let pred = pred.trim_matches(['<', '>']);
        if pred == pred_count {
            // obj: "N"^^<…nonNegativeInteger> .
            if let Some(n) = obj
                .split('"')
                .nth(1)
                .and_then(|lex| lex.parse::<u64>().ok())
            {
                counts.insert(subj, n);
            }
        } else if pred == pred_source {
            // obj: <source-iri> .
            let src = obj
                .trim_end_matches(" .")
                .trim_matches(['<', '>'])
                .to_owned();
            sources.insert(subj, src);
        }
    }

    // A co-resident emission: reading count >= 2 whose source resolves.
    let (emission, count) = counts
        .iter()
        .filter(|&(_, &n)| n >= 2)
        .max_by_key(|&(_, &n)| n)
        .map(|(s, &n)| (s.clone(), n))
        .expect(
            "FS5: the real projection corpus must carry a lang:ProjectionEmission with \
             lang:emittedReadingCount >= 2 — a genuinely ambiguous authored form whose readings \
             are held as co-resident data, not collapsed to a single winner",
        );
    let source = sources
        .get(&emission)
        .unwrap_or_else(|| panic!("FS5: co-resident emission {emission} names no projectsSource"));

    // The co-resident reading artifacts for that source form: the CoNLL-U target emits one
    // `…<form>.reading-<i>.conllu` artifact per reading. Their number must equal the declared
    // count (and hence be >= 2) — the ambiguity is materialized one-artifact-per-reading.
    let form_local = source.rsplit(['/', '#']).next().unwrap_or(source);
    let marker = format!(".{form_local}.reading-");
    let coresident = artifacts
        .iter()
        .filter(|(p, _)| p.contains("/conllu/") && p.contains(&marker))
        .count() as u64;
    assert!(
        coresident >= 2,
        "FS5: the ambiguous form <{source}> must materialize >= 2 co-resident reading \
         artifacts, found {coresident}"
    );
    assert_eq!(
        coresident, count,
        "FS5: the ambiguous form <{source}> declares {count} co-resident reading(s) but \
         materialized {coresident} artifact(s) — one artifact per reading, never a silent \
         collapse or a phantom reading"
    );
}

#[test]
fn native_lang_failures_handles_non_ascii_and_isolates_camelcase_class() {
    // A `lang:` token immediately followed by a NON-ASCII multibyte char must neither panic
    // (the scan must never slice at a non-char boundary) nor match. A CamelCase class token
    // `lang:<Class>:` in the SAME message must still be collected, and only that one.
    let errors = vec![
        "guard raised lang:中文 alongside lang:denotesEntity: and lang:ExactPreservationViolated: here"
            .to_string(),
    ];
    let got = native_failure_classes(&errors, "lang");
    let want: HashSet<String> = std::iter::once("ExactPreservationViolated".to_string()).collect();
    assert_eq!(
        got, want,
        "only the CamelCase-before-colon class matches; lowercase and non-ascii tokens do not, and the multibyte char does not panic"
    );

    // A `lang:` at the very end of a string and a trailing multibyte token also stay panic-free.
    let edge = vec!["dangling lang:".to_string(), "tail lang:漢字".to_string()];
    assert!(native_failure_classes(&edge, "lang").is_empty());
}
