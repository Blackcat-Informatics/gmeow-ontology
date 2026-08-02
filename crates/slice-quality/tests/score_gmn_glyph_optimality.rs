// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Regression tests for the GMN glyph-optimality denominator.
//!
//! An executable glyph is derived from the same Denotation -> Grapheme join the GMN writer
//! consumes. Such a binding must not be able to bypass the quality audit merely because its
//! author forgot to add a `gmeow:GmnSymbolCandidate` row.

use std::path::PathBuf;
use std::sync::Arc;

use gmeow_lang_bridge::GmnDictionary;
use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::{ScoreContext, ScoringEnv};
use purrdf::parse_dataset;

const SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/test-glyphs";
const LANG_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/lang";

fn score(extra_candidate: &str) -> gmeow_slice_quality::score::AxisScore {
    score_for(SLICE, extra_candidate)
}

fn score_for(slice_iri: &str, extra_candidate: &str) -> gmeow_slice_quality::score::AxisScore {
    let turtle = format!(
        r#"
@prefix ex: <https://example.test/glyph/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;
    gmeow:references gmeow:gmnScript , gmeow:gmnDictV3 ;
    gmeow:gmnDictionaryVersion "3" ;
    gmeow:gmnGlyphTableVersion "2" .

gmeow:gmnDictV3 a gmeow:GmnDictionary ;
    gmeow:gmnDictionaryVersion "3" .

gmeow:gmnScript a lang:Script ;
    lang:hasGrapheme ex:glyph .

ex:target a owl:ObjectProperty ;
    rdfs:isDefinedBy <{slice_iri}> .

ex:glyph a lang:Grapheme ;
    gmeow:gmnCodepoints "U+002B" .

ex:denotation a lang:Denotation ;
    lang:denotedForm ex:form ;
    lang:denotationTarget ex:target ;
    gmeow:gmnDenotationGrapheme ex:glyph .

ex:form a lang:WordForm .

{extra_candidate}
"#
    );
    let dataset = parse_dataset(turtle.as_bytes(), "text/turtle", None).expect("fixture parses");
    // Fixture-scale scoring supplies the complete audit graph directly, matching the
    // embedded-bundle path. Repo mode is exercised separately below because it must
    // assemble the canonical lang authority from a real checkout.
    let context = ScoreContext::new(
        slice_iri.to_owned(),
        PathBuf::new(),
        &dataset,
        ScoringEnv::Bundle(Arc::new(GmnDictionary::default())),
    );
    axes::resolve("gmn_glyph_optimality_axis").expect("axis is registered")(&context)
}

#[test]
fn targetless_candidate_is_not_filtered_out_of_the_lang_authority_audit() {
    let result = score_for(LANG_SLICE, "ex:targetless a gmeow:GmnSymbolCandidate .");

    assert_eq!(result.score, 0.0);
    assert!(result.findings.iter().any(|finding| {
        finding.code == "slice-quality.gmn-glyph-optimality.incomplete"
            && finding
                .message
                .contains("expected exactly one target, found 0")
    }));
}

#[test]
fn repo_scoring_fails_closed_when_symbol_audit_authority_is_unavailable() {
    let dataset = parse_dataset(
        format!(
            r#"
@prefix ex: <https://example.test/glyph/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:target a owl:ObjectProperty ; rdfs:isDefinedBy <{SLICE}> .
"#
        )
        .as_bytes(),
        "text/turtle",
        None,
    )
    .expect("fixture parses");
    // Deliberately NEVER created: the axis must fail closed on a slice root that
    // does not exist. Only its parent is a real temp tree, so nothing is left
    // behind once the guard drops.
    let tmp = tempfile::tempdir().expect("create temp dir");
    let missing_root = tmp
        .path()
        .join("gmeow-missing-glyph-authority/slices/test-glyphs");
    let context = ScoreContext::new(SLICE.to_owned(), missing_root, &dataset, ScoringEnv::Repo);

    let result = axes::resolve("gmn_glyph_optimality_axis").expect("axis is registered")(&context);

    assert_eq!(result.score, 0.0);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(
        result.findings[0].code,
        "slice-quality.gmn-glyph-optimality.audit-graph-unavailable"
    );
}

#[test]
fn executable_glyph_without_candidate_enters_denominator_and_is_named() {
    let result = score("");
    assert_eq!(result.score, 0.0);
    assert!(result.findings.iter().any(|finding| {
        finding.code == "slice-quality.gmn-glyph-optimality.unaudited-executable-target"
            && finding
                .message
                .contains("https://example.test/glyph/target")
    }));
}

#[test]
fn real_math_slice_glyph_optimality_is_perfect() {
    // AC1, demonstrated on the production surface: score the REAL math slice's
    // gmn_glyph_optimality axis in repo mode (the math-plane candidates live in the lang
    // authority, which the axis joins for a math-slice score). A score of 1.0 with no
    // findings proves every audited math target carries a complete, evidence-backed
    // disposition AND no executable math glyph binding lacks a candidate — the "no silent
    // gaps" contract, over the authored graph rather than a synthetic fixture. A missed or
    // half-specified math candidate would drop this below 1.0.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root");
    let math_dir = repo_root.join("slices/grounding/math");
    let paths = gmeow_slice_quality::report::slice_ttl_paths(&math_dir);
    let path_refs: Vec<&std::path::Path> = paths.iter().map(PathBuf::as_path).collect();
    let ds = gmeow_slice_quality::dataset_from_paths(&path_refs).expect("math slice loads");
    let slice_iri = gmeow_slice_quality::slice_iri_of_dir(&math_dir).expect("math slice IRI");
    let ctx = ScoreContext::new(slice_iri, math_dir, &ds, ScoringEnv::Repo);

    let result = axes::resolve("gmn_glyph_optimality_axis").expect("axis is registered")(&ctx);
    assert_eq!(
        result.score, 1.0,
        "the real math-plane glyph audit must be perfect (no silent gaps); findings: {:?}",
        result.findings
    );
    assert!(
        result.findings.is_empty(),
        "a perfect math-plane audit surfaces no advisories: {:?}",
        result.findings
    );
}

#[test]
fn complete_candidate_closes_the_executable_target_gap() {
    let result = score(
        r#"
ex:candidate a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateTarget ex:target ;
    gmeow:gmnCandidateGlyph "+" ;
    gmeow:gmnAsciiFallback "add" ;
    gmeow:gmnSpokenLabel "plus" ;
    gmeow:gmnDispositionRationale "The conventional sign is no dearer than its fallback." ;
    gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph ;
    gmeow:gmnDispositionBasis gmeow:gmnBasisTokenCost ;
    gmeow:gmnCandidateDenotation ex:denotation ;
    gmeow:cites ex:source .
"#,
    );
    assert_eq!(result.score, 1.0, "findings: {:?}", result.findings);
    assert!(result.findings.is_empty());
}
