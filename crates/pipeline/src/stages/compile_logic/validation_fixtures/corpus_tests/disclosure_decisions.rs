// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Consume the authenticated producer-selected disclosure projection. Tests never
//! compile source modules or generate shapes; only the small case data is authored here.

use gmeow_logic_compile::ir::{ConstraintComponent, ShapeTarget};
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

fn assert_case(product: &super::ShapeProduct, name: &str, body: &str, expected: bool) {
    let shapes = parse_shapes(&product.shacl, None).expect("producer-authored disclosure shape");
    let input = format!(
        "@prefix g: <{NS}> . @prefix ex: <urn:disclosure:> .
         ex:reader a g:Agent . ex:otherReader a g:Agent .
         ex:released a g:DisclosureOutcome . ex:hidden a g:DisclosureOutcome .
         ex:wrong a g:InformationObject . {body}"
    );
    let data = purrdf::parse_dataset(input.as_bytes(), "text/turtle", None)
        .expect("independent disclosure boundary data");
    let report = validate_dataset(&data, &shapes).expect("validate retained projection");
    assert_eq!(report.results.is_empty(), expected, "{name}: {report:?}");
}

#[test]
fn disclosure_decisions_require_one_typed_reader_and_outcome_locally() {
    let product = super::module("slices/core/observations/module.ttl")
        .shapes
        .iter()
        .find(|shape| {
            matches!(&shape.target, ShapeTarget::Class(class) if class == &format!("{NS}DisclosureDecision"))
        })
        .expect("producer selected the actual DisclosureDecision projection");
    for (path, class) in [
        ("disclosureReader", "Agent"),
        ("observationResult", "DisclosureOutcome"),
    ] {
        let properties: Vec<_> = product
            .properties
            .iter()
            .filter(|property| property.path == format!("{NS}{path}"))
            .collect();
        assert!(
            properties
                .iter()
                .any(|property| property.min_count == Some(1)),
            "{path} must be required"
        );
        assert!(
            properties.iter().any(|property| {
                property.components.iter().any(|component| {
            matches!(component, ConstraintComponent::Class(iri) if iri == &format!("{NS}{class}"))
        })
            }),
            "{path} must retain its universal value class"
        );
        assert!(properties.iter().any(|property| property.components.iter().any(|component| {
            matches!(component, ConstraintComponent::QualifiedValueShape { shape, max: Some(1), .. }
                if shape.iter().any(|inner| matches!(inner, ConstraintComponent::Class(iri) if iri == &format!("{NS}{class}"))))
        })), "{path} must retain its local qualified maximum");
    }
    for (name, body, expected) in [
        (
            "complete",
            "ex:d a g:DisclosureDecision ; g:disclosureReader ex:reader ; g:observationResult ex:released .",
            true,
        ),
        (
            "missing reader",
            "ex:d a g:DisclosureDecision ; g:observationResult ex:released .",
            false,
        ),
        (
            "missing outcome",
            "ex:d a g:DisclosureDecision ; g:disclosureReader ex:reader .",
            false,
        ),
        (
            "two readers",
            "ex:d a g:DisclosureDecision ; g:disclosureReader ex:reader, ex:otherReader ; g:observationResult ex:released .",
            false,
        ),
        (
            "two outcomes",
            "ex:d a g:DisclosureDecision ; g:disclosureReader ex:reader ; g:observationResult ex:released, ex:hidden .",
            false,
        ),
        (
            "wrong reader class",
            "ex:d a g:DisclosureDecision ; g:disclosureReader ex:wrong ; g:observationResult ex:released .",
            false,
        ),
        (
            "wrong outcome class",
            "ex:d a g:DisclosureDecision ; g:disclosureReader ex:reader ; g:observationResult ex:wrong .",
            false,
        ),
        (
            "ordinary observation remains multivalued",
            "ex:d a g:Observation ; g:observationResult ex:released, ex:hidden .",
            true,
        ),
        (
            "separate reader decisions coexist",
            "ex:d a g:DisclosureDecision ; g:disclosureReader ex:reader ; g:observationResult ex:released . ex:other a g:DisclosureDecision ; g:disclosureReader ex:otherReader ; g:observationResult ex:hidden .",
            true,
        ),
    ] {
        assert_case(product, name, body, expected);
    }
}
