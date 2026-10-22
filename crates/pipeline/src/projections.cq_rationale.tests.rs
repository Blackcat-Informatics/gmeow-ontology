// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored in-memory documentation through its RDF exit and the actual public
//! retag owner. No bundle, fixture selector or producer is used.

use super::*;

#[test]
fn cq_rationale_public_exit_uses_only_supplied_tag_map_and_preserves_direction() {
    let model = gmeow_docs::model::DocsModel {
        competencies: vec![gmeow_docs::model::DocCompetency {
            iri: "urn:authored:cq".to_owned(),
            rationales: vec![
                purrdf::RdfLiteral {
                    lexical_form: "left".to_owned(),
                    datatype: None,
                    language: Some("x-gmeow-english".to_owned()),
                    direction: Some(purrdf::RdfTextDirection::Ltr),
                },
                purrdf::RdfLiteral {
                    lexical_form: "right".to_owned(),
                    datatype: None,
                    language: Some("x-gmeow-english".to_owned()),
                    direction: Some(purrdf::RdfTextDirection::Rtl),
                },
                purrdf::RdfLiteral::language_tagged("justification", "fr"),
                purrdf::RdfLiteral::simple("unspecified"),
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let source = gmeow_docs::rdf::to_gmeow_rdf(&model, &BTreeMap::new());
    let dataset = purrdf::parse_dataset(source.as_bytes(), "application/n-quads", None).unwrap();
    let original: Vec<_> = dataset
        .owned_quads()
        .filter(|quad| quad.predicate == format!("{GM}cqRationale"))
        .collect();
    let mut unchanged = original.clone();
    retag_quads(&mut unchanged, &TagMap::new());
    assert_eq!(unchanged, original);
    let mut public = original;
    retag_quads(
        &mut public,
        &TagMap::from([("x-gmeow-english".to_owned(), "en".to_owned())]),
    );
    let mut found = BTreeMap::new();
    for quad in public {
        let RdfTerm::Literal(literal) = quad.object else {
            panic!("rationale must remain literal")
        };
        found.insert(literal.lexical_form, (literal.language, literal.direction));
    }
    assert_eq!(
        found["left"],
        (Some("en".to_owned()), Some(purrdf::RdfTextDirection::Ltr))
    );
    assert_eq!(
        found["right"],
        (Some("en".to_owned()), Some(purrdf::RdfTextDirection::Rtl))
    );
    assert_eq!(found["justification"], (Some("fr".to_owned()), None));
    assert_eq!(found["unspecified"], (None, None));
}
