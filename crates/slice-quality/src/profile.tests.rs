// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent small GMEOW corpora; no repository corpus or producer is admitted.

use super::*;
use crate::{BundleStandards, score_external_slice_from_files};
use purrdf::{DatasetView, GraphMatch};

const PREFIXES: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex: <https://consumer.example/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
"#;

fn model() -> String {
    format!(
        r#"{PREFIXES}
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:tierMaximal a gmeow:QualityTier ; gmeow:tierRank 1 .
gmeow:axisInformation a gmeow:QualityAxis ; gmeow:axisProducer "information_axis" ;
    gmeow:axisDimension ex:information ; gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold ex:floor .
gmeow:axisProjection a gmeow:QualityAxis ; gmeow:axisProducer "shape_migration_axis" ;
    gmeow:axisDimension ex:projection ; gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold ex:floor .
gmeow:axisGmn1Coverage a gmeow:QualityAxis ; gmeow:axisProducer "gmn1_coverage_axis" ;
    gmeow:axisDimension ex:gmn ; gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold ex:floor .
ex:floor a gmeow:AxisThreshold ; gmeow:thresholdTier gmeow:tierMaximal ; gmeow:thresholdFloor 1.0 .
ex:full a gmeow:QualityMeasurementProfile ; gmeow:qualityProfileKey "full" ;
    gmeow:qualityProfileTargetTier gmeow:tierMaximal ;
    gmeow:qualityProfileDefaultApplicability gmeow:qualityMeasured .
ex:external a gmeow:QualityMeasurementProfile ; gmeow:qualityProfileKey "external-sacred" ;
    gmeow:qualityProfileTargetTier gmeow:tierMaximal ;
    gmeow:qualityProfileDefaultApplicability gmeow:qualityMeasured ;
    gmeow:qualityProfileApplicability [ a gmeow:QualityAxisApplicability ;
        gmeow:qualityApplicabilityAxis gmeow:axisGmn1Coverage ;
        gmeow:qualityApplicabilityStatus gmeow:qualityNotApplicable ;
        gmeow:qualityApplicabilityReason "This corpus does not adopt the GMN dictionary." ] .
"#
    )
}

fn standards(source: &str, key: &str) -> gmeow_errors::Result<BundleStandards> {
    let dataset = crate::dataset_from_documents(&[("independent-rubric.ttl", source.as_bytes())])?;
    BundleStandards::from_dataset(
        dataset,
        format!("blake3:{}", blake3::hash(source.as_bytes()).to_hex()),
        key,
    )
}

fn files() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        (
            "manifest.ttl".into(),
            format!("{PREFIXES} ex:corpus a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .")
                .into_bytes(),
        ),
        (
            "module.ttl".into(),
            format!("{PREFIXES} ex:Entry a logic:Class ; rdfs:isDefinedBy ex:corpus .")
                .into_bytes(),
        ),
        (
            "docs.md".into(),
            b"# Consumer corpus\n\nA standalone logical vocabulary.\n".to_vec(),
        ),
    ])
}

#[test]
fn selected_profiles_admit_capabilities_before_measurement_without_fallback() {
    let model = model();
    let error = standards(&model, "full")
        .err()
        .expect("the full profile requires its dictionary");
    assert!(error.to_string().contains("dictionary"), "{error}");
    let external = standards(&model, "external-sacred").unwrap();
    let report = score_external_slice_from_files(&external, &files()).unwrap();
    let profile = report.profile.as_ref().unwrap();
    assert!(profile.complete);
    assert_eq!(
        (
            profile.total_axes,
            profile.measured_axes,
            profile.not_applicable_axes,
            profile.unsupported_axes
        ),
        (3, 2, 1, 0)
    );
    assert!(matches!(
        profile.outcomes[&g("axisInformation")],
        AxisOutcome::Failed { .. }
    ));
    assert!(matches!(
        profile.outcomes[&g("axisProjection")],
        AxisOutcome::Passed { .. }
    ));
    assert!(
        matches!(&profile.outcomes[&g("axisGmn1Coverage")], AxisOutcome::NotApplicable { reason } if reason.contains("does not adopt"))
    );
    assert!(
        standards(&model, "unknown")
            .err()
            .unwrap()
            .to_string()
            .contains("unknown quality profile")
    );
}

#[test]
fn excluded_and_unsupported_axes_have_distinct_honest_denominators() {
    let source = model().replace("gmeow:qualityNotApplicable", "gmeow:qualityUnsupported");
    let standard = standards(&source, "external-sacred").unwrap();
    let report = score_external_slice_from_files(&standard, &files()).unwrap();
    let profile = report.profile.as_ref().unwrap();
    assert!(!profile.complete);
    assert!(
        !crate::lint::lint_report(&report, None, None).passed,
        "an incomplete operation must not pass an absent tier bar"
    );
    assert!(report.render_text().contains("partial roll-up"));
    assert_eq!(
        (
            profile.measured_axes,
            profile.not_applicable_axes,
            profile.unsupported_axes
        ),
        (2, 0, 1)
    );
    assert!(matches!(
        profile.outcomes[&g("axisGmn1Coverage")],
        AxisOutcome::Unsupported { .. }
    ));
    assert!(
        report
            .to_report()
            .findings
            .iter()
            .any(|finding| finding.code == "slice-quality.profile-axis"
                && finding.severity == gmeow_errors::Severity::Error)
    );
}

#[test]
fn profile_changes_do_not_change_shared_measurements_or_hide_input_identity() {
    let standard = standards(&model(), "external-sacred").unwrap();
    let input = files();
    let first = score_external_slice_from_files(&standard, &input).unwrap();
    let again = score_external_slice_from_files(&standard, &input).unwrap();
    assert_eq!(
        serde_json::to_value(&first.profile).unwrap(),
        serde_json::to_value(&again.profile).unwrap()
    );
    let mut changed = input.clone();
    changed.insert(
        "unmeasured-note.txt".into(),
        b"An additional supplied input.".to_vec(),
    );
    let changed = score_external_slice_from_files(&standard, &changed).unwrap();
    assert_ne!(
        first.profile.as_ref().unwrap().corpus_identity,
        changed.profile.as_ref().unwrap().corpus_identity
    );
    for grade in &first.assessment.grades {
        assert_eq!(
            grade.score,
            changed
                .assessment
                .grades
                .iter()
                .find(|other| other.axis_iri == grade.axis_iri)
                .unwrap()
                .score
        );
    }
    let alternative_model = model().replace(
        "This corpus does not adopt the GMN dictionary.",
        "The selected corpus uses another notation contract.",
    );
    let alternative = standards(&alternative_model, "external-sacred").unwrap();
    let alternative = score_external_slice_from_files(&alternative, &input).unwrap();
    assert_ne!(
        first.profile.as_ref().unwrap().profile_identity,
        alternative.profile.as_ref().unwrap().profile_identity
    );
    assert_ne!(
        first.profile.as_ref().unwrap().rubric_identity,
        alternative.profile.as_ref().unwrap().rubric_identity
    );
    assert_eq!(
        first.assessment.rollup.iri,
        alternative.assessment.rollup.iri
    );
    for grade in &first.assessment.grades {
        assert_eq!(
            grade.score,
            alternative
                .assessment
                .grades
                .iter()
                .find(|other| other.axis_iri == grade.axis_iri)
                .unwrap()
                .score
        );
    }
}

#[test]
fn malformed_or_ambiguous_profile_contracts_fail_instead_of_shrinking_the_roster() {
    let source = model();
    for (broken, expected) in [
        (
            source.replace(
                "gmeow:qualityProfileKey \"external-sacred\"",
                "gmeow:qualityProfileKey \"external-sacred\", \"other\"",
            ),
            "exactly one",
        ),
        (
            source.replace(
                "gmeow:qualityApplicabilityAxis gmeow:axisGmn1Coverage",
                "gmeow:qualityApplicabilityAxis ex:unknown",
            ),
            "unknown axis",
        ),
        (
            source.replace(
                "gmeow:qualityApplicabilityStatus gmeow:qualityNotApplicable",
                "gmeow:qualityApplicabilityStatus ex:unknown",
            ),
            "invalid exclusion status",
        ),
        (
            source.replace("This corpus does not adopt the GMN dictionary.", ""),
            "non-empty",
        ),
        (
            source.replace(
                "\"This corpus does not adopt the GMN dictionary.\"",
                "\"This corpus does not adopt the GMN dictionary.\"@en",
            ),
            "non-empty literal",
        ),
        (
            format!(
                "{source} ex:external gmeow:qualityProfileApplicability [ a gmeow:QualityAxisApplicability ; gmeow:qualityApplicabilityAxis gmeow:axisGmn1Coverage ; gmeow:qualityApplicabilityStatus gmeow:qualityUnsupported ; gmeow:qualityApplicabilityReason \"Another decision for the same axis.\" ] ."
            ),
            "repeats applicability",
        ),
        (
            source.replace(
                "gmeow:qualityProfileTargetTier gmeow:tierMaximal",
                "gmeow:qualityProfileTargetTier ex:unknown",
            ),
            "unknown target tier",
        ),
        (
            format!("{source} ex:external gmeow:qualityProfileFallback true ."),
            "unsupported quality profile field",
        ),
        (
            format!(
                "{source} ex:duplicate a gmeow:QualityMeasurementProfile ; gmeow:qualityProfileKey \"external-sacred\" ."
            ),
            "ambiguous",
        ),
    ] {
        let error = standards(&broken, "external-sacred")
            .err()
            .expect("invalid profile must fail");
        assert!(error.to_string().contains(expected), "{expected}: {error}");
    }
}

#[test]
fn selected_documentation_catalog_failures_are_errors_and_not_grades() {
    let source = format!(
        r#"{}
ex:documentationAxis a gmeow:QualityAxis ; gmeow:axisProducer "DocMaturity" ;
    gmeow:axisDimension ex:documentation ; gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold ex:floor .
"#,
        model()
    );
    let standard = standards(&source, "external-sacred").unwrap();
    let mut input = files();
    score_external_slice_from_files(&standard, &input)
        .expect("the native single-slice catalog admits the independent valid corpus");
    input.insert(
        "mappings/broken.ttl".into(),
        b"not a Turtle document".to_vec(),
    );
    let error = score_external_slice_from_files(&standard, &input)
        .err()
        .expect("a broken selected catalog must fail instead of scoring zero");
    assert!(error.to_string().contains("required catalog"), "{error}");
}

#[test]
fn excluding_every_axis_cannot_manufacture_a_passing_rollup() {
    let mut source = model();
    for axis in ["axisInformation", "axisProjection"] {
        source.push_str(&format!(
            r#"
ex:external gmeow:qualityProfileApplicability [ a gmeow:QualityAxisApplicability ;
    gmeow:qualityApplicabilityAxis gmeow:{axis} ;
    gmeow:qualityApplicabilityStatus gmeow:qualityNotApplicable ;
    gmeow:qualityApplicabilityReason "Outside this deliberately empty profile." ] .
"#
        ));
    }
    let error = standards(&source, "external-sacred")
        .err()
        .expect("an empty measured denominator is not a meaningful operation");
    assert!(error.to_string().contains("no measurable axes"), "{error}");
}

#[test]
fn declared_imports_include_owned_structure_and_native_reifiers_without_asserting_quotes() {
    let source = format!(
        r#"{}
ex:dep a logic:Ontology ; logic:imports ex:nested .
ex:nested a logic:Module .
ex:Nested a logic:Class ; logic:inModule ex:nested .
ex:constraint a logic:Constraint ; rdfs:isDefinedBy ex:dep ; logic:integrity ex:formula .
ex:formula a logic:Formula ; logic:not ex:child .
ex:child a logic:Formula ; logic:relation ex:Unrelated ; logic:argument ex:variable, ex:otherVariable .
ex:args rdf:first ex:Unrelated ; rdf:rest ex:tail .
ex:tail rdf:first ex:object ; rdf:rest rdf:nil .
ex:variable logic:termIndex 0 ; logic:termVariable "x" .
ex:otherVariable logic:termIndex 1 ; logic:termVariable "y" .
ex:Dependency a logic:Class ; rdfs:isDefinedBy ex:dep ; logic:oneOf ex:args ; ex:part [ ex:detail "تفصيل"@ar--rtl ] ; rdfs:seeAlso ex:Unrelated .
ex:r rdf:reifies <<( ex:Dependency ex:unasserted ex:object )>> ; ex:note "retained annotation" ; ex:metadata [ ex:noteDetail "retained structure" ] .
ex:other a logic:Ontology .
ex:Unrelated a logic:Class ; rdfs:isDefinedBy ex:other ; ex:mustNotLeak true .
"#,
        model()
    );
    let standard = standards(&source, "external-sacred").unwrap();
    let mut input = files();
    input
        .get_mut("module.ttl")
        .unwrap()
        .extend_from_slice(b"\nex:corpus logic:imports ex:dep .\n");
    let (dataset, imports) = prepare_inputs(&input, &standard.source).unwrap();
    assert_eq!(
        imports,
        vec![
            "https://consumer.example/dep",
            "https://consumer.example/nested"
        ]
    );
    let dataset = dataset.as_ref();
    for name in [
        "formula",
        "child",
        "args",
        "tail",
        "variable",
        "otherVariable",
    ] {
        let node = dataset
            .term_id_by_iri(&format!("https://consumer.example/{name}"))
            .expect("named imported formula structure is retained");
        assert!(
            dataset
                .quads_for_pattern(Some(node), None, None, GraphMatch::Any)
                .next()
                .is_some()
        );
    }
    let nested = dataset
        .term_id_by_iri("https://consumer.example/Nested")
        .expect("the transitive import's term is present");
    assert!(
        dataset
            .quads_for_pattern(Some(nested), None, None, GraphMatch::Any)
            .next()
            .is_some()
    );
    let unrelated = dataset
        .term_id_by_iri("https://consumer.example/Unrelated")
        .expect("the named reference survives");
    assert!(
        dataset
            .quads_for_pattern(Some(unrelated), None, None, GraphMatch::Any)
            .next()
            .is_none()
    );
    let quoted = dataset
        .term_id_by_iri("https://consumer.example/unasserted")
        .unwrap();
    assert!(
        dataset
            .quads_for_pattern(None, Some(quoted), None, GraphMatch::Any)
            .next()
            .is_none()
    );
    let view = gmeow_logic_compile::ingest::DslView::new(dataset);
    let metadata_detail = dataset
        .term_id_by_iri("https://consumer.example/noteDetail")
        .expect("native annotation structure survives import selection");
    assert!(
        dataset
            .quads_for_pattern(None, Some(metadata_detail), None, GraphMatch::Any)
            .next()
            .is_some()
    );
    let reifiers: Vec<_> = view.reified_statements().collect();
    assert_eq!(reifiers.len(), 1);
    assert_eq!(
        reifiers[0]
            .annotation_literal("https://consumer.example/note")
            .unwrap(),
        Some("retained annotation")
    );
    let detail = dataset
        .term_id_by_iri("https://consumer.example/detail")
        .unwrap();
    assert!(
        dataset
            .quads_for_pattern(None, Some(detail), None, GraphMatch::Any)
            .any(|quad| matches!(
                dataset.resolve(quad.o),
                TermRef::Literal {
                    lexical: "تفصيل",
                    language: Some("ar"),
                    direction: Some(purrdf::RdfTextDirection::Rtl),
                    ..
                }
            ))
    );
}

#[test]
fn a_declared_import_must_exist_as_a_real_supplied_owner() {
    let mut input = files();
    input
        .get_mut("module.ttl")
        .unwrap()
        .extend_from_slice(b"\nex:corpus logic:imports ex:missing .\n");
    let source = format!(
        "{} ex:missing rdfs:label \"a reference is not an ontology\" .",
        model()
    );
    let standard = standards(&source, "external-sacred").unwrap();
    let error = score_external_slice_from_files(&standard, &input)
        .err()
        .expect("missing import must fail");
    assert!(
        error.to_string().contains("absent from the bundle"),
        "{error}"
    );
    input.insert("imports/explicit.ttl".into(), format!("{PREFIXES} ex:missing a logic:Ontology . ex:Imported a logic:Class ; rdfs:isDefinedBy ex:missing .").into_bytes());
    let report = score_external_slice_from_files(&standard, &input).unwrap();
    assert_eq!(
        report.profile.unwrap().declared_imports,
        vec!["https://consumer.example/missing"]
    );
}

#[test]
fn profile_results_and_reasons_survive_consumer_report_formats() {
    let standard = standards(&model(), "external-sacred").unwrap();
    let report = score_external_slice_from_files(&standard, &files()).unwrap();
    let structured = report.to_report();
    let json = gmeow_errors::render::to_json(&structured).unwrap();
    let sarif = gmeow_errors::render::to_sarif(&structured).unwrap();
    let text = report.render_text();
    for output in [&json, &sarif, &text] {
        assert!(
            output.contains("does not adopt"),
            "excluded-axis reason must survive"
        );
        assert!(
            output.contains("axisInformation"),
            "measured-axis identity must survive"
        );
    }
    assert_eq!(structured.metadata["quality_profile"]["measured_axes"], 2);
    assert_eq!(structured.metadata["quality_profile"]["total_axes"], 3);
    let sarif: serde_json::Value = serde_json::from_str(&sarif).unwrap();
    assert_eq!(
        sarif["runs"][0]["properties"]["quality_profile"],
        structured.metadata["quality_profile"]
    );
    assert!(text.contains("not applicable"));
}
