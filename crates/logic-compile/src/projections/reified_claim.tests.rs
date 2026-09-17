// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn polarity_is_the_total_map_from_the_three_put_classes() {
    assert_eq!(
        AssertionPolarity::of(PutClass::CompleteOver),
        AssertionPolarity::AssertBase
    );
    assert_eq!(
        AssertionPolarity::of(PutClass::ValidationOnly),
        AssertionPolarity::ReifyClaim
    );
    assert_eq!(
        AssertionPolarity::of(PutClass::Unsupported),
        AssertionPolarity::Withhold
    );
}

#[test]
fn full_style_renders_a_type_object_claim_with_provenance() {
    let claim = ReifiedClaim {
        cell_label: "cell".to_owned(),
        subject: "?s".to_owned(),
        predicate: RDF_TYPE.to_owned(),
        object: ClaimObject::Iri("<https://blackcatinformatics.ca/gmeow/ModelArtifact>".to_owned()),
        annotations: vec![ClaimAnnotation {
            label: "mapann".to_owned(),
            property: GM_MAPPED_FROM.to_owned(),
            value: "<http://www.w3.org/ns/mls#Model>".to_owned(),
        }],
        generated_by: Some("<https://blackcatinformatics.ca/gmeow/import/ml-schema>".to_owned()),
    };
    let lines = reified_claim_head(&claim, IriStyle::Full);
    let block = lines.join("\n");
    assert!(block.contains("_:cell <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/StatementMetadata> ."));
    assert!(block.contains("<https://blackcatinformatics.ca/gmeow/qSubject> ?s ."));
    assert!(block.contains("<https://blackcatinformatics.ca/gmeow/qObject> <https://blackcatinformatics.ca/gmeow/ModelArtifact> ."));
    assert!(block.contains("<https://blackcatinformatics.ca/gmeow/mappedFrom> ."));
    assert!(block.contains("<https://blackcatinformatics.ca/gmeow/wasGeneratedBy> <https://blackcatinformatics.ca/gmeow/import/ml-schema> ."));
}

#[test]
fn curie_style_renders_a_literal_object_claim() {
    let claim = ReifiedClaim {
        cell_label: "cell0".to_owned(),
        subject: "?s".to_owned(),
        predicate: "https://blackcatinformatics.ca/gmeow/label".to_owned(),
        object: ClaimObject::Literal("?o".to_owned()),
        annotations: vec![ClaimAnnotation {
            label: "mapann0".to_owned(),
            property: GM_MAPPED_FROM.to_owned(),
            value: "skos:prefLabel".to_owned(),
        }],
        generated_by: None,
    };
    let block = reified_claim_head(&claim, IriStyle::Curie).join("\n");
    assert!(block.contains("_:cell0 rdf:type gmeow:StatementMetadata ."));
    assert!(block.contains("gmeow:qObjectLiteral ?o ."));
    assert!(
        !block.contains("wasGeneratedBy"),
        "no provenance edge when None"
    );
}
