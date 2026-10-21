// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Validate independent case data with authenticated quality and math projections.
//! No test compiles an authored module or produces a corpus fixture.

use gmeow_logic_compile::ir::ShapeTarget;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

const QUALITY: &str = "slices/core/quality/module.ttl";
const MATH: &str = "slices/grounding/math/module.ttl";

/// Combine the actual quality projection with the imported result owners it consumes.
#[test]
fn quality_results_preserve_dimensions_estimators_and_categorical_verdicts() {
    let quality = super::module(QUALITY);
    let mut text = quality.procedural_shacl.clone();
    assert!(text.contains("AggregateQualityResultsConstraintProceduralConstraintShape"));
    for class in [
        "QuantitativeQualityAssessment",
        "AggregateQualityAssessment",
    ] {
        assert!(quality.shapes.iter().any(|shape| {
            matches!(&shape.target, ShapeTarget::Class(iri)
                if iri == &format!("https://blackcatinformatics.ca/gmeow/{class}"))
        }));
    }
    for product in &quality.shapes {
        text.push_str(&product.shacl);
    }
    for class in ["Quantity", "Estimate"] {
        let product = super::module(MATH)
            .shapes
            .iter()
            .find(|shape| {
                matches!(&shape.target, ShapeTarget::Class(iri)
                    if iri == &format!("https://blackcatinformatics.ca/math/{class}"))
            })
            .expect("producer retained the imported mathematical result contract");
        text.push_str(&product.shacl);
    }
    let shapes = parse_shapes(&text, None).expect("actual quality and math projections");
    let prefixes = "@prefix g: <https://blackcatinformatics.ca/gmeow/> .
        @prefix m: <https://blackcatinformatics.ca/math/> .
        @prefix ex: <urn:quality:> .
        ex:dataset a g:Entity . ex:dimension a m:Dimension .
        ex:quantity a m:Quantity ; m:hasDimension ex:dimension ; m:quantityValue 0.985 .
        ex:procedure a m:Estimator .";
    let quantitative = "ex:a a g:QuantitativeQualityAssessment ;
        g:observationResult ex:quantity .";
    let aggregate = "ex:a a g:AggregateQualityAssessment ; g:observationResult ex:estimate .
        ex:estimate a m:Estimate ; m:estimateValue ex:quantity ;
        m:estimator ex:procedure ; m:estimatedParameter ex:parameter .";
    let cases = [
        ("dimensioned magnitude", quantitative.to_owned(), true),
        ("missing magnitude", "ex:a a g:QuantitativeQualityAssessment .".into(), false),
        ("categorical result on quantitative assessment", quantitative.replace("ex:quantity", "ex:category"), false),
        ("mixed quantitative results", quantitative.replace("ex:quantity", "ex:quantity, ex:category"), false),
        ("complete aggregate", aggregate.into(), true),
        ("estimand target", aggregate.replace("m:estimatedParameter", "m:estimatesEstimand"), true),
        ("missing aggregate result", "ex:a a g:AggregateQualityAssessment .".into(), false),
        ("wrong aggregate result type", aggregate.replace("a m:Estimate", "a g:Entity"), false),
        ("missing estimator", aggregate.replace("m:estimator ex:procedure ;", ""), false),
        ("wrong estimator type", aggregate.replace("m:estimator ex:procedure", "m:estimator ex:wrong"), false),
        ("malformed extra estimator", aggregate.replace("m:estimator ex:procedure", "m:estimator ex:procedure, ex:wrong"), false),
        ("missing value", aggregate.replace("m:estimateValue ex:quantity ;", ""), false),
        ("wrong value type", aggregate.replace("m:estimateValue ex:quantity", "m:estimateValue ex:category"), false),
        ("malformed extra value", aggregate.replace("m:estimateValue ex:quantity", "m:estimateValue ex:quantity, ex:category"), false),
        ("missing target", aggregate.replace("m:estimatedParameter ex:parameter", ""), false),
        ("malformed extra result", format!("{aggregate} ex:a g:observationResult ex:bad . ex:bad a m:Estimate ."), false),
        ("good neighbor cannot supply missing value", format!("{aggregate} ex:other a g:AggregateQualityAssessment ; g:observationResult ex:empty . ex:empty a m:Estimate ; m:estimator ex:procedure ; m:estimatedParameter ex:parameter ."), false),
        ("base categorical verdict", "ex:a a g:QualityAssessment ; g:assessedEntity ex:dataset ; g:observationResult ex:category .".into(), true),
        ("base multidimensional multivalued report", "ex:a a g:QualityAssessment ; g:assessedEntity ex:dataset ; g:qualityDimension ex:firstDimension, ex:secondDimension ; g:observationResult ex:firstVerdict, ex:secondVerdict .".into(), true),
        ("coexisting assessments", format!("{} {aggregate}", quantitative.replace("ex:a ", "ex:direct ")), true),
    ];
    for (name, body, valid) in cases {
        let input = format!("{prefixes} {body}");
        let data = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None)
            .expect("independent quality case data");
        let report = validate_dataset(&data, &shapes).expect("validate actual projections");
        assert_eq!(report.results.is_empty(), valid, "{name}: {report:?}");
    }
    for (name, body) in [
        ("quantity dimension", quantitative),
        ("aggregate value dimension", aggregate),
    ] {
        let input = format!(
            "{} {body}",
            prefixes.replace("m:hasDimension ex:dimension ;", "")
        );
        let data = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None).unwrap();
        let report = validate_dataset(&data, &shapes).unwrap();
        assert!(
            report
                .results
                .iter()
                .any(|finding| finding.focus_node.to_string().contains("quantity")),
            "{name}: {report:?}"
        );
    }
}
