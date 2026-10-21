// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only consumers of authenticated risk and mathematics validation products.

use gmeow_logic_compile::ir::ShapeTarget;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

/// Check weights and probabilities against their actual production projection owners.
#[test]
fn risk_weights_and_probabilities_keep_distinct_contracts() {
    let risk = super::module("slices/extensions/risk/module.ttl");
    let math = super::module("slices/grounding/math/module.ttl");
    let mut text = risk.procedural_shacl.clone();
    for name in [
        "LinkStrengthQuantityConstraint",
        "RiskLikelihoodResultsConstraint",
        "RiskProbabilityEventContextConstraint",
    ] {
        assert!(text.contains(&format!("{name}ProceduralConstraintShape")));
    }
    for product in &risk.shapes {
        text.push_str(&product.shacl);
    }
    for class in [
        "Quantity",
        "RationalValue",
        "ProbabilityValue",
        "ProbabilityEvent",
        "ProbabilitySpace",
        "ProbabilityMeasure",
        "Measure",
    ] {
        let product = math
            .shapes
            .iter()
            .find(|shape| {
                matches!(&shape.target, ShapeTarget::Class(iri)
                if iri == &format!("https://blackcatinformatics.ca/math/{class}"))
            })
            .expect("producer retained imported mathematical validation");
        text.push_str(&product.shacl);
    }
    assert!(
        math.procedural_shacl
            .contains("RationalProbabilityRangeConstraintProceduralConstraintShape")
    );
    text.push_str(&math.procedural_shacl);
    let observations = super::module("slices/core/observations/module.ttl");
    let observation = observations
        .shapes
        .iter()
        .find(|shape| {
            matches!(&shape.target, ShapeTarget::Class(iri)
            if iri == "https://blackcatinformatics.ca/gmeow/Observation")
        })
        .expect("producer retained the actual inherited observation contract");
    text.push_str(&observation.shacl);
    text.push_str(&observations.procedural_shacl);
    let places = super::module("slices/core/places/module.ttl");
    assert!(
        places
            .shapes
            .iter()
            .any(|shape| matches!(&shape.target, ShapeTarget::Class(iri)
        if iri == "https://blackcatinformatics.ca/gmeow/ReferenceFrame"))
    );
    for product in &places.shapes {
        if matches!(&product.target, ShapeTarget::Class(iri)
            if matches!(iri.as_str(), "https://blackcatinformatics.ca/gmeow/ReferenceFrame" | "https://blackcatinformatics.ca/gmeow/Axis"))
        {
            text.push_str(&product.shacl);
        }
    }
    // The owner emits Axis typing and axis-count agreement as procedural laws.
    assert!(
        places
            .procedural_shacl
            .contains("FrameDimensionCountConstraintProceduralConstraintShape")
    );
    assert!(
        places
            .procedural_shacl
            .contains("FrameAxisTypeConstraintProceduralConstraintShape")
    );
    text.push_str(&places.procedural_shacl);
    let shapes = parse_shapes(&text, None).expect("actual risk, math and frame projections");
    let common = r#"
        @prefix g: <https://blackcatinformatics.ca/gmeow/> .
        @prefix m: <https://blackcatinformatics.ca/math/> .
        @prefix l: <https://blackcatinformatics.ca/logic/> .
        @prefix ex: <urn:risk-grounding:> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        # Small independent schema shortcuts model the production Tier-1 boundary.
        # No test loads or reconstructs the live ontology subclass closure.
        g:RiskLikelihoodAssessment rdfs:subClassOf g:Observation .
        m:ProbabilityValue rdfs:subClassOf m:Quantity .
        m:ProbabilityMeasure rdfs:subClassOf m:Measure .
        m:ProbabilitySpace rdfs:subClassOf m:MeasurableSpace .
        ex:SpecialWeight rdfs:subClassOf m:Quantity .
        g:CausalLink rdfs:subClassOf g:RiskFactor, g:Entity .
        g:Hazard rdfs:subClassOf g:RiskFactor, g:Entity .
        ex:reader a g:Entity .
        ex:factor a g:Entity . ex:antecedent a g:EventType . ex:consequent a g:EventType .
        g:causallyPromotes a g:CausalModality .
        m:dimensionless a m:Dimension . ex:model a l:ProbabilityModel .
        g:frameRealmMathematical a g:FrameRealm . g:frameKindScalar a g:FrameKind .
        g:determinacyCrisp a g:Determinacy . g:axisScalar a g:Axis .
        ex:scale a g:ReferenceFrame ; g:frameRealm g:frameRealmMathematical ;
            g:frameKind g:frameKindScalar ; g:determinacyModel g:determinacyCrisp ;
            g:hasAxis g:axisScalar ; g:dimensionCount "1"^^xsd:nonNegativeInteger ; g:requiresHost false .
        ex:space a m:ProbabilitySpace ; m:sampleSpace ex:sample ;
            m:eventSigmaAlgebra ex:algebra ; m:probabilityMeasure ex:measure .
        ex:sample a m:SampleSpace . ex:algebra a m:SigmaAlgebra . ex:measure a m:ProbabilityMeasure ; m:totalMass 1 ; m:measureOn ex:space .
    "#;
    let link = "ex:link a g:CausalLink ; g:linkAntecedent ex:antecedent ; g:linkConsequent ex:consequent ; g:causalModality g:causallyPromotes .";
    let weight = "ex:link g:linkStrength ex:weight . ex:weight a m:Quantity ; m:quantityValue 2.5 ; m:hasDimension m:dimensionless ; g:hasReferenceFrame ex:scale .";
    let probability = "ex:a a g:RiskLikelihoodAssessment ; g:vantage ex:reader ; g:observedAt \"2026-01-01T00:00:00Z\"^^xsd:dateTime ; g:observedFeature ex:factor ; g:observationResult ex:p . ex:p a m:ProbabilityValue ; m:quantityValue 0.2 ; m:hasDimension m:dimensionless ; l:probabilityModel ex:model . ex:event a m:ProbabilityEvent ; m:probabilityOfEvent ex:p ; m:eventInSpace ex:space .";
    let rational_probability = probability
        .replace(
            "a m:ProbabilityValue",
            "a m:ProbabilityValue, m:RationalValue",
        )
        .replace(
            "m:quantityValue 0.2 ;",
            "m:isExact true ; m:numerator 1 ; m:denominator 3 ;",
        );
    let rational_weight = weight
        .replace("a m:Quantity", "a m:Quantity, m:RationalValue")
        .replace(
            "m:quantityValue 2.5 ;",
            "m:isExact true ; m:numerator 4 ; m:denominator 3 ;",
        );
    let mut cases = vec![
        ("unweighted", link.to_owned(), true),
        ("weight above one", format!("{link} {weight}"), true),
        ("negative weight", format!("{link} {}", weight.replace("2.5", "-3.0")), true),
        ("literal weight", format!("{link} ex:link g:linkStrength 0.7 ."), false),
        ("weight missing frame", format!("{link} {}", weight.replace("; g:hasReferenceFrame ex:scale", "")), false),
        ("weight wrong frame", format!("{link} {}", weight.replace("g:hasReferenceFrame ex:scale", "g:hasReferenceFrame ex:bad")), false),
        ("weight extra malformed frame", format!("{link} {}", weight.replace("g:hasReferenceFrame ex:scale", "g:hasReferenceFrame ex:scale, ex:bad")), false),
        ("weight missing magnitude", format!("{link} {}", weight.replace("m:quantityValue 2.5 ;", "")), false),
        ("weight nonfinite magnitude", format!("{link} {}", weight.replace("2.5", "\"INF\"^^xsd:double")), false),
        ("weight malformed extra magnitude", format!("{link} {}", weight.replace("m:quantityValue 2.5", "m:quantityValue 2.5, \"bad\"")), false),
        ("weight missing dimension", format!("{link} {}", weight.replace("m:hasDimension m:dimensionless ;", "")), false),
        ("probability", probability.to_owned(), true),
        ("inherited observation vantage type", probability.replace("g:vantage ex:reader", "g:vantage ex:bad"), false),
        ("inherited observation time datatype", probability.replace("\"2026-01-01T00:00:00Z\"^^xsd:dateTime", "\"invalid\""), false),
        ("inherited observation time maximum", format!("{probability} ex:a g:observedAt \"2026-01-02T00:00:00Z\"^^xsd:dateTime ."), false),
        ("quantity subclass weight", format!("{link} {}", weight.replace("a m:Quantity", "a ex:SpecialWeight")), true),
        ("probability zero", probability.replace("0.2", "0.0"), true),
        ("probability one", probability.replace("0.2", "1.0"), true),
        ("missing result", "ex:a a g:RiskLikelihoodAssessment ; g:observedFeature ex:factor .".into(), false),
        ("missing feature", probability.replace("g:observedFeature ex:factor ;", ""), false),
        ("literal result", probability.replace("g:observationResult ex:p", "g:observationResult 0.2"), false),
        ("frame cannot substitute for model", probability.replace("l:probabilityModel ex:model", "g:hasReferenceFrame ex:scale"), false),
        ("wrong model", probability.replace("l:probabilityModel ex:model", "l:probabilityModel ex:bad"), false),
        ("malformed extra model", probability.replace("l:probabilityModel ex:model", "l:probabilityModel ex:model, ex:bad"), false),
        ("missing event", probability.replace("m:probabilityOfEvent ex:p ;", ""), false),
        ("wrong event", probability.replace("ex:event a m:ProbabilityEvent", "ex:event a g:Entity"), false),
        ("missing space", probability.replace("; m:eventInSpace ex:space", ""), false),
        ("wrong space", probability.replace("m:eventInSpace ex:space", "m:eventInSpace ex:bad"), false),
        ("malformed extra space", probability.replace("m:eventInSpace ex:space", "m:eventInSpace ex:space, ex:bad"), false),
        ("incomplete space", format!("{} ex:incomplete a m:ProbabilitySpace .", probability.replace("m:eventInSpace ex:space", "m:eventInSpace ex:incomplete")), false),
        ("missing magnitude", probability.replace("m:quantityValue 0.2 ;", ""), false),
        ("nonfinite probability", probability.replace("0.2", "\"NaN\"^^xsd:double"), false),
        ("probability above one", probability.replace("0.2", "1.2"), false),
        ("negative probability", probability.replace("0.2", "-0.2"), false),
        ("missing dimension", probability.replace("m:hasDimension m:dimensionless ;", ""), false),
        ("coexistence", format!("{link} {weight} {probability}"), true),
        ("categorical severity", "ex:hazard a g:Hazard ; g:hazardBearer ex:factor ; g:manifestedAsType ex:consequent ; g:hazardSeverity ex:severity . ex:severity a g:SeverityLevel .".into(), true),
        ("neighbor cannot supply model", format!("{probability} ex:other a g:RiskLikelihoodAssessment ; g:vantage ex:reader ; g:observedFeature ex:factor ; g:observationResult ex:unframed . ex:unframed a m:ProbabilityValue ; m:quantityValue 0.3 ; m:hasDimension m:dimensionless . ex:otherEvent a m:ProbabilityEvent ; m:probabilityOfEvent ex:unframed ; m:eventInSpace ex:space ."), false),
        ("one result cannot conceal another", format!("{probability} ex:a g:observationResult ex:missing . ex:missing a m:ProbabilityValue ; m:quantityValue 0.4 ; m:hasDimension m:dimensionless ; l:probabilityModel ex:model ."), false),
        ("two modeled results", format!("{probability} {}", probability.replace("ex:a ", "ex:other ").replace("ex:p", "ex:q").replace("ex:event", "ex:otherEvent")), true),
        ("confidence cannot substitute for model", probability.replace("l:probabilityModel ex:model", "l:confidence 0.9 ; g:hasReferenceFrame ex:scale"), false),
        ("confidence remains metadata", format!("{probability} ex:p l:confidence 0.9 ."), true),
    ];
    cases.extend([
        ("exact huge valid probability", rational_probability.replace("m:numerator 1 ; m:denominator 3", "m:numerator 340282366920938463463374607431768211456 ; m:denominator 340282366920938463463374607431768211457"), true),
        ("exact huge out-of-range probability", rational_probability.replace("m:numerator 1 ; m:denominator 3", "m:numerator 340282366920938463463374607431768211457 ; m:denominator 340282366920938463463374607431768211456"), false),
        ("exact probability one third", rational_probability.clone(), true),
        ("exact negative denominator", rational_probability.replace("m:numerator 1 ; m:denominator 3", "m:numerator -1 ; m:denominator -3"), true),
        ("exact mixed denominator sign", rational_probability.replace("m:denominator 3", "m:denominator -3"), false),
        ("exact probability zero", rational_probability.replace("m:numerator 1", "m:numerator 0"), true),
        ("exact probability above one", rational_probability.replace("m:numerator 1 ; m:denominator 3", "m:numerator 3 ; m:denominator 2"), false),
        ("exact negative probability", rational_probability.replace("m:numerator 1", "m:numerator -1"), false),
        ("exact zero denominator", rational_probability.replace("m:denominator 3", "m:denominator 0"), false),
        ("exact missing numerator", rational_probability.replace("m:numerator 1 ;", ""), false),
        ("exact missing denominator", rational_probability.replace("m:denominator 3 ;", ""), false),
        ("exact mistyped numerator", rational_probability.replace("m:numerator 1", "m:numerator \"1\""), false),
        ("exact mistyped denominator", rational_probability.replace("m:denominator 3", "m:denominator \"3\""), false),
        ("exact missing rational type", rational_probability.replace("a m:ProbabilityValue, m:RationalValue", "a m:ProbabilityValue"), false),
        ("exact missing flag", rational_probability.replace("m:isExact true ;", ""), false),
        ("exact true and false flags", rational_probability.replace("m:isExact true", "m:isExact true, false"), false),
        ("exact true and mistyped flags", rational_probability.replace("m:isExact true", "m:isExact true, \"bad\""), false),
        ("exact probability plus decimal", format!("{rational_probability} ex:p m:quantityValue 0.9 ."), false),
        ("exact weight plus decimal", format!("{link} {rational_weight} ex:weight m:quantityValue 0.9 ."), false),
        ("exact weight mixed flags", format!("{link} {}", rational_weight.replace("m:isExact true", "m:isExact true, false")), false),
        ("exact false flag", rational_probability.replace("m:isExact true", "m:isExact false"), false),
        ("exact weight outside unit interval", format!("{link} {rational_weight}"), true),
        ("exact negative weight", format!("{link} {}", rational_weight.replace("m:numerator 4", "m:numerator -4")), true),
        ("exact incomplete weight", format!("{link} {}", rational_weight.replace("m:denominator 3 ;", "")), false),
        ("exact mistyped weight", format!("{link} {}", rational_weight.replace("m:numerator 4", "m:numerator \"4\"")), false),
    ]);
    let frame = r#"ex:testFrame a g:ReferenceFrame ;
        g:frameRealm g:frameRealmMathematical ; g:frameKind g:frameKindScalar ;
        g:determinacyModel g:determinacyCrisp ; g:hasAxis g:axisScalar ;
        g:dimensionCount "1"^^xsd:nonNegativeInteger ; g:requiresHost false ."#;
    let framed_weight = weight.replace(
        "g:hasReferenceFrame ex:scale",
        "g:hasReferenceFrame ex:testFrame",
    );
    for (name, profile, valid) in [
        ("complete imported frame", frame.to_owned(), true),
        (
            "missing frame determinacy",
            frame.replace("g:determinacyModel g:determinacyCrisp ;", ""),
            false,
        ),
        (
            "missing frame realm",
            frame.replace("g:frameRealm g:frameRealmMathematical ;", ""),
            false,
        ),
        (
            "missing frame kind",
            frame.replace("g:frameKind g:frameKindScalar ;", ""),
            false,
        ),
        (
            "missing frame axis",
            frame.replace("g:hasAxis g:axisScalar ;", ""),
            false,
        ),
        (
            "missing frame dimension count",
            frame.replace("g:dimensionCount \"1\"^^xsd:nonNegativeInteger ;", ""),
            false,
        ),
        (
            "missing frame host policy",
            frame.replace("; g:requiresHost false", ""),
            false,
        ),
        (
            "frame axis-count mismatch",
            frame.replace(
                "\"1\"^^xsd:nonNegativeInteger",
                "\"2\"^^xsd:nonNegativeInteger",
            ),
            false,
        ),
        (
            "frame wrong axis type",
            frame.replace("g:hasAxis g:axisScalar", "g:hasAxis ex:notAnAxis"),
            false,
        ),
        (
            "frame wrong determinacy type",
            frame.replace(
                "g:determinacyModel g:determinacyCrisp",
                "g:determinacyModel ex:notDeterminacy",
            ),
            false,
        ),
    ] {
        cases.push((name, format!("{link} {framed_weight} {profile}"), valid));
    }
    let measure =
        "ex:testMeasure a m:ProbabilityMeasure ; m:totalMass 1 ; m:measureOn ex:otherSpace .";
    let measure_probability =
        probability.replace("m:eventInSpace ex:space", "m:eventInSpace ex:otherSpace");
    let measure_space = "ex:otherSpace a m:ProbabilitySpace ; m:sampleSpace ex:sample ; m:eventSigmaAlgebra ex:algebra ; m:probabilityMeasure ex:testMeasure .";
    for (name, profile, valid) in [
        ("complete probability measure", measure.to_owned(), true),
        (
            "missing probability measure mass",
            measure.replace("m:totalMass 1 ;", ""),
            false,
        ),
        (
            "wrong probability measure mass",
            measure.replace("m:totalMass 1", "m:totalMass 0"),
            false,
        ),
        (
            "missing measure space",
            measure.replace("; m:measureOn ex:otherSpace", ""),
            false,
        ),
        (
            "mistyped measure space",
            measure.replace("m:measureOn ex:otherSpace", "m:measureOn ex:factor"),
            false,
        ),
    ] {
        cases.push((
            name,
            format!("{measure_probability} {measure_space} {profile}"),
            valid,
        ));
    }
    for (name, body, valid) in cases {
        let input = format!("{common} {body}");
        let data = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None)
            .expect("independent boundary data");
        let report = validate_dataset(&data, &shapes).expect("validate retained products");
        assert_eq!(report.results.is_empty(), valid, "{name}: {report:?}");
    }
}
