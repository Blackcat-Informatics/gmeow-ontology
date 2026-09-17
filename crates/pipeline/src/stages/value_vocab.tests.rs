// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::stages::export::FoldView;
use gmeow_ns::gmeow_json_schema_namespaces;

/// A synthetic ontology store exercising the functional-vs-multivalued split:
///
/// * `gmeow:FrameKind` is an open value vocabulary (`logic:AbstractIndividualType`)
///   with two `gmeow:` members;
/// * `gmeow:frameKind` is FUNCTIONAL — a `logic:PropertyCharacteristicAssertion`
///   carrier characterizes it `logic:functionalProperty`;
/// * `gmeow:frameTag` ranges over the SAME vocabulary but carries NO functional
///   assertion, so it is (correctly) multivalued.
const SYNTH_STORE: &str = r#"
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix owl:   <http://www.w3.org/2002/07/owl#> .
        @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .

        gmeow:FrameKind a logic:AbstractIndividualType .
        gmeow:cartesian a gmeow:FrameKind .
        gmeow:grid      a gmeow:FrameKind .

        gmeow:frameKind a owl:ObjectProperty ;
            rdfs:range gmeow:FrameKind .
        gmeow:frameTag a owl:ObjectProperty ;
            rdfs:range gmeow:FrameKind .

        logic:frameKindFunctionality
            a owl:NamedIndividual , logic:PropertyCharacteristicAssertion ;
            logic:characterizes gmeow:frameKind ;
            logic:characteristicSort logic:functionalProperty ;
            logic:formalizes gmeow:frameKind .
    "#;

/// A node-reference-only property schema (`purrdf`'s shape for an object property
/// whose range class has no NodeShape — every value vocabulary).
fn node_ref() -> Value {
    json!({
        "type": "object",
        "properties": { "@id": { "type": "string" } },
        "required": ["@id"]
    })
}

/// The multivalued (`anyOf:[node-ref, {array of node-ref}]`) property schema
/// `purrdf` emits for a class whose node shape lacks a single-valued cap.
fn multivalued_node_ref() -> Value {
    json!({
        "anyOf": [
            node_ref(),
            { "type": "array", "items": node_ref() }
        ]
    })
}

fn parse_store() -> std::sync::Arc<RdfDataset> {
    parse_dataset(SYNTH_STORE.as_bytes(), "text/turtle", None).expect("parse synthetic store")
}

/// A functional enum-ranged property narrows to a SCALAR `$ref` (no array branch)
/// even on a class whose node shape lacked the cap — the widened
/// `anyOf:[ref, array]` input is dropped — while a NON-functional property ranging
/// over the same vocabulary keeps its `anyOf:[ref, array]` multivalued form.
#[test]
fn functional_enum_property_narrows_to_scalar_ref() {
    let store = parse_store();
    let view = FoldView::new(&store);
    let ns = gmeow_json_schema_namespaces();

    // Both fields arrive multivalued (the un-capped class node shape). The
    // functional/non-functional split, not the input cardinality, decides output.
    let mut schema = json!({
        "$defs": {
            "NarrativeReferenceFrame": {
                "type": "object",
                "properties": {
                    "@id": { "type": "string" },
                    "gmeow:frameKind": multivalued_node_ref(),
                    "gmeow:frameTag": multivalued_node_ref()
                }
            }
        }
    });

    let vocabs = enrich_value_vocab_enums(&mut schema, &ns, &view);
    assert!(
        vocabs.iter().any(|v| v.enum_key == "FrameKindEnum"),
        "value vocabulary derived"
    );

    let props = &schema["$defs"]["NarrativeReferenceFrame"]["properties"];

    // Functional: scalar single-`$ref`, array branch DROPPED.
    assert_eq!(
        props["gmeow:frameKind"],
        json!({ "$ref": "#/$defs/FrameKindEnum" }),
        "functional property must narrow to a scalar $ref"
    );

    // Non-functional: the multivalued `anyOf:[ref, {array of ref}]` is preserved.
    assert_eq!(
        props["gmeow:frameTag"],
        json!({
            "anyOf": [
                { "$ref": "#/$defs/FrameKindEnum" },
                { "type": "array", "items": { "$ref": "#/$defs/FrameKindEnum" } }
            ]
        }),
        "non-functional property must stay multivalued"
    );
}

/// `functional_property_iris` reads the carrier and ONLY the carrier: it reports the
/// property named by a `logic:functionalProperty` assertion and nothing else.
#[test]
fn functional_property_iris_reads_only_the_carrier() {
    let store = parse_store();
    let view = FoldView::new(&store);
    let functional = functional_property_iris(&view);

    assert!(
        functional.contains("https://blackcatinformatics.ca/gmeow/frameKind"),
        "carrier-characterized functional property is reported"
    );
    assert!(
        !functional.contains("https://blackcatinformatics.ca/gmeow/frameTag"),
        "a property with no functional carrier is NOT reported"
    );
    assert_eq!(functional.len(), 1, "exactly the one carried property");
}

/// A functional property whose field arrives ALREADY scalar stays scalar (the fix is
/// idempotent), and repointing still lands on the enum `$def`.
#[test]
fn functional_scalar_input_stays_scalar() {
    let store = parse_store();
    let view = FoldView::new(&store);
    let ns = gmeow_json_schema_namespaces();

    let mut schema = json!({
        "$defs": {
            "ReferenceFrame": {
                "type": "object",
                "properties": {
                    "@id": { "type": "string" },
                    "gmeow:frameKind": node_ref()
                }
            }
        }
    });

    enrich_value_vocab_enums(&mut schema, &ns, &view);

    assert_eq!(
        schema["$defs"]["ReferenceFrame"]["properties"]["gmeow:frameKind"],
        json!({ "$ref": "#/$defs/FrameKindEnum" }),
    );
}
