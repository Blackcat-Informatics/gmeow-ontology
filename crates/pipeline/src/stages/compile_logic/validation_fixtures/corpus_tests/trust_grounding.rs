// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only consumers of producer-selected trust and mathematical contracts.

use gmeow_logic_compile::ir::ShapeTarget;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

#[test]
fn introducer_weights_preserve_the_protocol_scale_and_categorical_trust() {
    let trust = super::module("slices/core/trust/module.ttl");
    let math = super::module("slices/grounding/math/module.ttl");
    let mut text = trust.procedural_shacl.clone();
    assert!(text.contains("IntroducerWeightRangeConstraintProceduralConstraintShape"));
    for product in &trust.shapes {
        text.push_str(&product.shacl);
    }
    for class in ["Quantity", "ProbabilityValue"] {
        let product = math
            .shapes
            .iter()
            .find(|shape| {
                matches!(&shape.target, ShapeTarget::Class(iri)
                if iri == &format!("https://blackcatinformatics.ca/math/{class}"))
            })
            .expect("producer retained the canonical mathematical owner");
        text.push_str(&product.shacl);
    }
    text.push_str(&math.procedural_shacl);
    let places = super::module("slices/core/places/module.ttl");
    assert!(places.shapes.iter().any(|shape| matches!(&shape.target,
        ShapeTarget::Class(iri) if iri == "https://blackcatinformatics.ca/gmeow/ReferenceFrame")));
    // Retain the complete actual frame/axis products and their procedural owner checks.
    // Axis has no independent required fields; its type is checked from the frame role.
    for product in &places.shapes {
        text.push_str(&product.shacl);
    }
    assert!(
        places
            .procedural_shacl
            .contains("FrameDimensionCountConstraint")
    );
    assert!(places.procedural_shacl.contains("FrameAxisTypeConstraint"));
    text.push_str(&places.procedural_shacl);
    let shapes = parse_shapes(&text, None).expect("actual trust and mathematical projections");
    let common = r#"
        @prefix g: <https://blackcatinformatics.ca/gmeow/> .
        @prefix m: <https://blackcatinformatics.ca/math/> .
        @prefix l: <https://blackcatinformatics.ca/logic/> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix ex: <urn:trust-grounding:> .
        g:IntroducerWeight rdfs:subClassOf m:Quantity .
        m:dimensionless a m:Dimension .
        g:openPgpIntroducerAmountFrame a g:ReferenceFrame ;
            g:determinacyModel g:determinacyCrisp ; g:dimensionCount "1"^^xsd:nonNegativeInteger ;
            g:frameKind g:frameKindScalar ; g:frameRealm g:frameRealmMathematical ;
            g:hasAxis g:axisScalar ; g:requiresHost false .
        ex:otherFrame a g:ReferenceFrame ;
            g:determinacyModel g:determinacyCrisp ; g:dimensionCount "1"^^xsd:nonNegativeInteger ;
            g:frameKind g:frameKindScalar ; g:frameRealm g:frameRealmMathematical ;
            g:hasAxis g:axisScalar ; g:requiresHost false .
        g:determinacyCrisp a g:Determinacy . g:frameKindScalar a g:FrameKind .
        g:frameRealmMathematical a g:FrameRealm . g:axisScalar a g:Axis .
        ex:otherDimension a m:Dimension .
        ex:alice a g:Agent . ex:bob a g:Agent .
    "#;
    let assertion = "ex:trust a g:TrustAssertion ; g:trustor ex:alice ; g:trustee ex:bob ; g:trustLevel \"full\" ; g:introducerDepth 1 .";
    let weight = "ex:trust g:introducerAmount ex:w . ex:w a g:IntroducerWeight ; m:quantityValue 120 ; m:hasDimension m:dimensionless ; g:hasReferenceFrame g:openPgpIntroducerAmountFrame .";
    for (name, body, valid) in [
        ("optional amount", assertion.to_owned(), true),
        (
            "complete trust weight",
            format!("{assertion} {weight}"),
            true,
        ),
        (
            "partial trust weight",
            format!("{assertion} {}", weight.replace("120", "60")),
            true,
        ),
        (
            "zero protocol octet",
            format!("{assertion} {}", weight.replace("120", "0")),
            true,
        ),
        (
            "maximum protocol octet",
            format!("{assertion} {}", weight.replace("120", "255")),
            true,
        ),
        (
            "negative amount",
            format!("{assertion} {}", weight.replace("120", "-1")),
            false,
        ),
        (
            "oversized amount",
            format!("{assertion} {}", weight.replace("120", "256")),
            false,
        ),
        (
            "noninteger amount",
            format!("{assertion} {}", weight.replace("120", "0.5")),
            false,
        ),
        (
            "bare scalar amount",
            format!("{assertion} ex:trust g:introducerAmount 60 ."),
            false,
        ),
        (
            "untyped amount",
            format!(
                "{assertion} {}",
                weight.replace("a g:IntroducerWeight ;", "a m:Quantity ;")
            ),
            false,
        ),
        (
            "missing value",
            format!(
                "{assertion} {}",
                weight.replace("m:quantityValue 120 ;", "")
            ),
            false,
        ),
        (
            "malformed value",
            format!("{assertion} {}", weight.replace("120", "\"bad\"")),
            false,
        ),
        (
            "malformed extra value",
            format!("{assertion} {}", weight.replace("120", "120, \"bad\"")),
            false,
        ),
        (
            "two values",
            format!("{assertion} {}", weight.replace("120", "120, 60")),
            false,
        ),
        (
            "missing dimension",
            format!(
                "{assertion} {}",
                weight.replace("m:hasDimension m:dimensionless ;", "")
            ),
            false,
        ),
        (
            "wrong dimension",
            format!(
                "{assertion} {}",
                weight.replace(
                    "m:hasDimension m:dimensionless",
                    "m:hasDimension ex:otherDimension"
                )
            ),
            false,
        ),
        (
            "missing frame",
            format!(
                "{assertion} {}",
                weight.replace("g:hasReferenceFrame g:openPgpIntroducerAmountFrame", "")
            ),
            false,
        ),
        (
            "different scale",
            format!(
                "{assertion} {}",
                weight.replace(
                    "g:hasReferenceFrame g:openPgpIntroducerAmountFrame",
                    "g:hasReferenceFrame ex:otherFrame"
                )
            ),
            false,
        ),
        (
            "extra scale",
            format!(
                "{assertion} {}",
                weight.replace(
                    "g:hasReferenceFrame g:openPgpIntroducerAmountFrame",
                    "g:hasReferenceFrame g:openPgpIntroducerAmountFrame, ex:otherFrame"
                )
            ),
            false,
        ),
        (
            "depth stays integer",
            format!(
                "{} {weight}",
                assertion.replace("g:introducerDepth 1", "g:introducerDepth ex:w")
            ),
            false,
        ),
        (
            "confidence is separate metadata",
            format!("{assertion} {weight} ex:trust l:confidence 0.9 ."),
            true,
        ),
        (
            "amount is not a probability",
            format!("{assertion} {weight} ex:w a m:ProbabilityValue ."),
            false,
        ),
        (
            "zero weight is not a probability object",
            format!(
                "{assertion} {} ex:w a m:ProbabilityValue .",
                weight.replace("120", "0")
            ),
            false,
        ),
        (
            "unit weight is not a probability object",
            format!(
                "{assertion} {} ex:w a m:ProbabilityValue .",
                weight.replace("120", "1")
            ),
            false,
        ),
        (
            "same number on separate weight and probability objects",
            format!(
                "{assertion} {} ex:p a m:ProbabilityValue ; m:quantityValue 1 ; m:hasDimension m:dimensionless ; g:hasReferenceFrame ex:otherFrame .",
                weight.replace("120", "1")
            ),
            true,
        ),
        (
            "neighbor cannot supply magnitude",
            format!(
                "{assertion} {} ex:neighbor a g:IntroducerWeight ; m:quantityValue 120 ; m:hasDimension m:dimensionless ; g:hasReferenceFrame g:openPgpIntroducerAmountFrame .",
                weight.replace("m:quantityValue 120 ;", "")
            ),
            false,
        ),
    ] {
        let data =
            purrdf::parse_dataset(format!("{common} {body}").as_bytes(), "text/turtle", None)
                .unwrap();
        let report =
            validate_dataset(&data, &shapes).expect("validate retained production contracts");
        assert_eq!(report.results.is_empty(), valid, "{name}: {report:?}");
    }
    for (name, from, to) in [
        (
            "missing determinacy",
            "g:determinacyModel g:determinacyCrisp ;",
            "",
        ),
        (
            "missing dimension count",
            "g:dimensionCount \"1\"^^xsd:nonNegativeInteger ;",
            "",
        ),
        ("missing frame kind", "g:frameKind g:frameKindScalar ;", ""),
        (
            "missing realm",
            "g:frameRealm g:frameRealmMathematical ;",
            "",
        ),
        ("missing axis", "g:hasAxis g:axisScalar ;", ""),
        ("missing host policy", "g:requiresHost false", ""),
        (
            "wrong axis count",
            "g:dimensionCount \"1\"^^xsd:nonNegativeInteger",
            "g:dimensionCount \"2\"^^xsd:nonNegativeInteger",
        ),
        (
            "wrong axis type",
            "g:axisScalar a g:Axis",
            "g:axisScalar a g:Entity",
        ),
        (
            "wrong determinacy type",
            "g:determinacyCrisp a g:Determinacy",
            "g:determinacyCrisp a g:Entity",
        ),
        (
            "wrong kind type",
            "g:frameKindScalar a g:FrameKind",
            "g:frameKindScalar a g:Entity",
        ),
        (
            "wrong realm type",
            "g:frameRealmMathematical a g:FrameRealm",
            "g:frameRealmMathematical a g:Entity",
        ),
    ] {
        let input = format!("{} {assertion} {weight}", common.replace(from, to));
        let data = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None).unwrap();
        let report = validate_dataset(&data, &shapes).unwrap();
        assert!(!report.results.is_empty(), "{name}: {report:?}");
    }
}
