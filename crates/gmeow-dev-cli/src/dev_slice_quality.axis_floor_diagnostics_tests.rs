// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// A fixture repo whose tree lives in an owned temp directory: dropping the fixture
/// drops the [`tempfile::TempDir`], which removes the tree — on success, on early
/// return, and while unwinding from a failed assertion alike.
struct TempFixture {
    _tmp: tempfile::TempDir,
    root: std::path::PathBuf,
}

/// A fixture repo whose demo slice measures BELOW a 1.0 floor on both
/// `axisProseQuality` and `axisTranslationCoverage`.
fn fixture_below_two_floors() -> (TempFixture, std::path::PathBuf) {
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-axis-diag-")
        .tempdir()
        .expect("create temp dir");
    let root = tmp.path().to_path_buf();
    let rubric_dir = root.join("slices/core/slice-quality-rubric");
    let demo_dir = root.join("slices/demo/demo");
    std::fs::create_dir_all(&rubric_dir).unwrap();
    std::fs::create_dir_all(&demo_dir).unwrap();
    std::fs::write(
        rubric_dir.join("module.ttl"),
        format!(
            r#"@prefix gmeow: <{NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:tierGrounded a gmeow:QualityTier ; gmeow:tierRank 1 .
gmeow:axisProseQuality a gmeow:QualityAxis ;
    gmeow:axisProducer "prose_axis" ;
    gmeow:axisDimension gmeow:dimProse ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrProse .
gmeow:thrProse a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
gmeow:axisTranslationCoverage a gmeow:QualityAxis ;
    gmeow:axisProducer "translation_axis" ;
    gmeow:axisDimension gmeow:dimTranslation ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrTranslation .
gmeow:thrTranslation a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
gmeow:afc-demo-prose a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceDemo ;
    gmeow:floorAxis gmeow:axisProseQuality ;
    gmeow:floorValue 1.0 .
gmeow:afc-demo-translation a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceDemo ;
    gmeow:floorAxis gmeow:axisTranslationCoverage ;
    gmeow:floorValue 1.0 .
"#
        ),
    )
    .unwrap();
    std::fs::write(
        rubric_dir.join("manifest.ttl"),
        format!("@prefix gmeow: <{NS}> .\ngmeow:sliceRubric a gmeow:Slice .\n"),
    )
    .unwrap();
    // The demo term: a TBox class whose definition draws NO boundary and whose
    // example is prose rather than a worked triple, with three localizable prose
    // literals and no `i18n/` catalogs at all.
    std::fs::write(
        demo_dir.join("module.ttl"),
        format!(
            r#"@prefix gmeow: <{NS}> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
gmeow:DemoThing a owl:Class ;
    rdfs:isDefinedBy gmeow:sliceDemo ;
    rdfs:label "Demo Thing"@x-gmeow-english ;
    skos:definition "A demo thing that exists in the demo slice."@x-gmeow-english ;
    skos:example "see the demo documentation for usage"@x-gmeow-english .
"#
        ),
    )
    .unwrap();
    std::fs::write(
        demo_dir.join("manifest.ttl"),
        format!("@prefix gmeow: <{NS}> .\ngmeow:sliceDemo a gmeow:Slice .\n"),
    )
    .unwrap();
    (
        TempFixture {
            _tmp: tmp,
            root: root.clone(),
        },
        demo_dir,
    )
}

/// Score the fixture demo slice and project the below-floor advisories for one
/// axis exactly as the gate's `MeasuredBelowFloor` arm does.
fn advisory_report_for(
    root: &std::path::Path,
    demo_dir: &std::path::Path,
    axis_local: &str,
) -> (f64, gmeow_errors::Report) {
    let rubric = gmeow_slice_quality::load_repo_rubric(root).unwrap();
    let mut reports = gmeow_slice_quality::score_slices_with_rubric(
        root,
        std::slice::from_ref(&demo_dir.to_path_buf()),
        &rubric,
    );
    let report = reports.remove(0).unwrap();
    let grade = report
        .assessment
        .grades
        .iter()
        .find(|g| axis_local_name(&g.axis_iri) == axis_local)
        .expect("the fixture rubric grades the axis");
    let advisories = report.advisories_for_axis(&grade.axis_iri);
    assert!(
        !advisories.is_empty(),
        "{axis_local} scored {} yet produced no advisory to print",
        grade.score
    );
    (
        grade.score,
        axis_floor_advisory_report(&report.assessment.slice, axis_local, &advisories),
    )
}

#[test]
fn a_below_floor_prose_axis_prints_the_offending_term_iris() {
    let (fx, demo_dir) = fixture_below_two_floors();
    let (score, report) = advisory_report_for(&fx.root, &demo_dir, "axisProseQuality");
    assert!(
        gmeow_slice_quality::gate::evaluate_axis_floor(score, 1.0).is_failure(),
        "the fixture must sit below the 1.0 prose floor, measured {score}"
    );

    let messages: Vec<&str> = report.findings.iter().map(|f| f.message.as_str()).collect();
    let joined = messages.join("\n");
    assert!(
        joined.contains(&format!("{NS}DemoThing")),
        "the printed advisories must NAME the offending term IRI; got:\n{joined}"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "slice-quality.prose.definition-no-boundary"),
        "the no-boundary advisory must survive into the printed report:\n{joined}"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "slice-quality.prose.example-not-triple"),
        "the not-a-worked-triple advisory must survive into the printed report:\n{joined}"
    );

    // Ledger identity: every printed witness carries a content-addressed finding
    // IRI and its ANCHOR term. A hand-built Finding would carry neither and could
    // not be joined by a reasoner pass over the finding graph (it derives DARK).
    for finding in &report.findings {
        if finding.code.starts_with("slice-quality.prose.") {
            assert!(
                finding.finding_iri.is_some(),
                "{} was not minted through the ledger",
                finding.code
            );
            assert_eq!(
                finding.documented_terms,
                vec![format!("{NS}DemoThing")],
                "{} lost its anchor term",
                finding.code
            );
        }
    }
}

#[test]
fn a_below_floor_translation_axis_prints_each_uncovered_pair_with_its_reason() {
    let (fx, demo_dir) = fixture_below_two_floors();
    let (score, report) = advisory_report_for(&fx.root, &demo_dir, "axisTranslationCoverage");
    assert!(
        gmeow_slice_quality::gate::evaluate_axis_floor(score, 1.0).is_failure(),
        "the fixture must sit below the 1.0 translation floor, measured {score}"
    );

    let uncovered: Vec<&gmeow_errors::Finding> = report
        .findings
        .iter()
        .filter(|f| f.code == "slice-quality.translation.uncovered-literal")
        .collect();
    let joined = report
        .findings
        .iter()
        .map(|f| f.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    // Three authored prose literals × two target languages: every one of them is
    // named individually, with a reason, not folded into a bare count.
    assert_eq!(
        uncovered.len(),
        6,
        "every uncovered (term, predicate) in every target language must be named:\n{joined}"
    );
    for predicate in [
        "http://www.w3.org/2000/01/rdf-schema#label",
        "http://www.w3.org/2004/02/skos/core#definition",
        "http://www.w3.org/2004/02/skos/core#example",
    ] {
        assert!(
            uncovered
                .iter()
                .any(|f| f.message.contains(&format!("({NS}DemoThing, {predicate})"))),
            "the uncovered pair for {predicate} must be named:\n{joined}"
        );
    }
    assert!(
        uncovered
            .iter()
            .all(|f| f.message.contains("no catalog entry for this literal")),
        "each uncovered literal must carry its REASON:\n{joined}"
    );

    // Ledger identity again, plus the witness ANTECEDENT edge: the uncovered
    // literal's msgctxt is interned as its own node and the advisory derives from
    // it, so the DAG walks from the failing axis down to the exact catalog key.
    for finding in &uncovered {
        assert!(finding.finding_iri.is_some(), "not minted via the ledger");
        assert_eq!(finding.documented_terms, vec![format!("{NS}DemoThing")]);
        assert_eq!(
            finding.antecedents.len(),
            1,
            "the uncovered literal's msgctxt must be an ANTECEDENT node"
        );
    }
    let witnesses: Vec<&gmeow_errors::Finding> = report
        .findings
        .iter()
        .filter(|f| f.code == AXIS_ADVISORY_WITNESS_CODE)
        .collect();
    assert!(
        witnesses.iter().any(|f| f
            .message
            .contains("http://www.w3.org/2004/02/skos/core#definition")),
        "the msgctxt witness node must be interned and printed:\n{joined}"
    );
}

#[test]
fn the_per_literal_list_is_capped_but_never_silently_truncated() {
    // The cap is a rendering bound; the remainder must be STATED. Build a slice
    // with more uncovered literals than the cap and assert the explicit tail.
    let tmp = tempfile::Builder::new()
        .prefix("gmeow-axis-diag-cap-")
        .tempdir()
        .expect("create temp dir");
    let root = tmp.path().to_path_buf();
    let fx = TempFixture {
        _tmp: tmp,
        root: root.clone(),
    };
    let rubric_dir = root.join("slices/core/slice-quality-rubric");
    let demo_dir = root.join("slices/demo/demo");
    std::fs::create_dir_all(&rubric_dir).unwrap();
    std::fs::create_dir_all(&demo_dir).unwrap();
    std::fs::write(
        rubric_dir.join("module.ttl"),
        format!(
            r#"@prefix gmeow: <{NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisTranslationCoverage a gmeow:QualityAxis ;
    gmeow:axisProducer "translation_axis" ;
    gmeow:axisDimension gmeow:dimTranslation ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrTranslation .
gmeow:thrTranslation a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
"#
        ),
    )
    .unwrap();
    std::fs::write(
        rubric_dir.join("manifest.ttl"),
        format!("@prefix gmeow: <{NS}> .\ngmeow:sliceRubric a gmeow:Slice .\n"),
    )
    .unwrap();
    let mut module = format!(
        "@prefix gmeow: <{NS}> .\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n"
    );
    // 2 prose literals per term × 15 terms = 30 uncovered literals per language,
    // comfortably over the cap.
    for i in 0..15 {
        module.push_str(&format!(
                "gmeow:DemoThing{i} a owl:Class ;\n    rdfs:isDefinedBy gmeow:sliceDemo ;\n    rdfs:label \"Demo Thing {i}\"@x-gmeow-english ;\n    skos:definition \"A demo thing number {i} that exists.\"@x-gmeow-english .\n"
            ));
    }
    std::fs::write(demo_dir.join("module.ttl"), module).unwrap();
    std::fs::write(
        demo_dir.join("manifest.ttl"),
        format!("@prefix gmeow: <{NS}> .\ngmeow:sliceDemo a gmeow:Slice .\n"),
    )
    .unwrap();

    let (_score, report) = advisory_report_for(&fx.root, &demo_dir, "axisTranslationCoverage");
    let listed = report
        .findings
        .iter()
        .filter(|f| f.code == "slice-quality.translation.uncovered-literal")
        .count();
    assert_eq!(
        listed,
        2 * gmeow_slice_quality::axes::UNCOVERED_LITERAL_CAP,
        "each language lists exactly the cap"
    );
    let tail: Vec<&gmeow_errors::Finding> = report
        .findings
        .iter()
        .filter(|f| f.code == "slice-quality.translation.uncovered-literal-truncated")
        .collect();
    assert_eq!(tail.len(), 2, "one explicit tail per language");
    for finding in tail {
        assert!(
            finding.message.contains(&format!(
                "… and {} more",
                30 - gmeow_slice_quality::axes::UNCOVERED_LITERAL_CAP
            )),
            "the truncated remainder must be stated exactly: {}",
            finding.message
        );
    }
}
