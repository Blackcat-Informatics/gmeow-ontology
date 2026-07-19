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
    let missing_root = std::env::temp_dir().join(format!(
        "gmeow-missing-glyph-authority-{}/slices/test-glyphs",
        std::process::id()
    ));
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
