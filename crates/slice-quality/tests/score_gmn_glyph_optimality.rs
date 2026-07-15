// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Regression tests for the GMN glyph-optimality denominator.
//!
//! An executable glyph is derived from the same Denotation -> Grapheme join the GMN writer
//! consumes. Such a binding must not be able to bypass the quality audit merely because its
//! author forgot to add a `gmeow:GmnSymbolCandidate` row.

use std::path::PathBuf;

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::{ScoreContext, ScoringEnv};
use purrdf::parse_dataset;

const SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/test-glyphs";

fn score(extra_candidate: &str) -> gmeow_slice_quality::score::AxisScore {
    let turtle = format!(
        r#"
@prefix ex: <https://example.test/glyph/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;
    gmeow:references gmeow:gmnScript , gmeow:gmnDictV2 ;
    gmeow:gmnDictionaryVersion "2" ;
    gmeow:gmnGlyphTableVersion "2" .

gmeow:gmnDictV2 a gmeow:GmnDictionary ;
    gmeow:gmnDictionaryVersion "2" .

gmeow:gmnScript a lang:Script ;
    lang:hasGrapheme ex:glyph .

ex:target a owl:ObjectProperty ;
    rdfs:isDefinedBy <{SLICE}> .

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
    let context = ScoreContext::new(SLICE.to_owned(), PathBuf::new(), &dataset, ScoringEnv::Repo);
    axes::resolve("gmn_glyph_optimality_axis").expect("axis is registered")(&context)
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
