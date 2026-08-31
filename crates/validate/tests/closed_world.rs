// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Closed-world fidelity proof.
//!
//! The headline acceptance criterion: the SHACL-derived JSON Schema is a
//! CLOSED-WORLD projection — a required property (`sh:minCount 1`) REJECTS an
//! incomplete instance node that the old open-world LinkML schema accepted.
//!
//! We build a one-class shapes graph (`gmeow:Thing` requiring `gmeow:req`),
//! compile it to a JSON Schema with [`purrdf::shapes::json_schema::compile`], and
//! validate two instance nodes of the SAME `@type` through
//! [`gmeow_validate::instance::validate_instance`]:
//!
//! * a COMPLETE node (carries `gmeow:req`) → accepted (no violations);
//! * an INCOMPLETE node (missing `gmeow:req`) → rejected, with a violation that
//!   names the missing property.
//!
//! The `@type` discrimination is what makes this closed-world: the `Node` schema
//! sees `@type: "gmeow:Thing"` and therefore enforces `#/$defs/Thing`'s
//! `required` list, which an open-world schema would not.

use gmeow_validate::instance::{InstanceFormat, validate_instance};
use purrdf::shapes::{engine, json_schema};

/// A shapes graph with one class `gmeow:Thing` requiring `gmeow:req`.
const SHAPES_TTL: &str = r#"
@prefix sh:    <http://www.w3.org/ns/shacl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .

gmeow:ThingShape a sh:NodeShape ;
    sh:targetClass gmeow:Thing ;
    sh:property [
        sh:path     gmeow:req ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:datatype xsd:string
    ] .
"#;

/// Compile [`SHAPES_TTL`] into the closed-world JSON Schema bytes.
fn schema_bytes() -> Vec<u8> {
    let shapes = engine::parse_shapes(SHAPES_TTL).expect("parse shapes");
    let compiled = json_schema::compile(&shapes, &gmeow_namespaces());
    compiled.schema_json.into_bytes()
}

fn gmeow_namespaces() -> json_schema::Namespaces {
    json_schema::Namespaces::new(
        "gmeow",
        &[
            (
                "gmeow".to_owned(),
                "https://blackcatinformatics.ca/gmeow/".to_owned(),
            ),
            (
                "logic".to_owned(),
                "https://blackcatinformatics.ca/logic/".to_owned(),
            ),
        ],
    )
    .expect("gmeow namespaces")
}

#[test]
fn required_accepts_a_complete_instance() {
    let schema = schema_bytes();
    // A complete gmeow:Thing carrying its required property.
    let instance = br#"{"@id":"gmeow:t1","@type":"gmeow:Thing","gmeow:req":"present"}"#;
    let violations = validate_instance(instance, InstanceFormat::Json, &schema)
        .expect("validate_instance must not hard-error");
    assert!(
        violations.is_empty(),
        "a complete instance must be accepted, got {violations:?}"
    );
}

#[test]
fn required_rejects_an_incomplete_instance_the_old_open_world_schema_accepted() {
    let schema = schema_bytes();
    // SAME @type, but the required gmeow:req is missing. An open-world schema
    // accepted this; the closed-world projection MUST reject it.
    let instance = br#"{"@id":"gmeow:t2","@type":"gmeow:Thing"}"#;
    let violations = validate_instance(instance, InstanceFormat::Json, &schema)
        .expect("validate_instance must not hard-error");
    assert!(
        !violations.is_empty(),
        "an incomplete instance of a modeled class must be rejected (the old \
         open-world schema accepted it)"
    );
    // We deliberately DON'T assert on the `jsonschema` crate's message wording
    // here: a bare-node root collapses its sub-error into the top-level union, so
    // the missing `gmeow:req` is not surfaced, and matching on "anyOf" would be
    // brittle across dependency upgrades. The non-empty rejection above is the
    // contract; the property-NAMING guarantee is asserted in the @graph-envelope
    // variant below, where the per-node sub-error surfaces `gmeow:req` structurally.
}

#[test]
fn required_rejects_incomplete_node_inside_graph_envelope() {
    // The same proof through the @graph envelope the projector emits, so the
    // closed-world guarantee holds for whole-document instances too.
    let schema = schema_bytes();
    let complete =
        br#"{"@graph":[{"@id":"gmeow:t1","@type":"gmeow:Thing","gmeow:req":"present"}]}"#;
    let incomplete = br#"{"@graph":[{"@id":"gmeow:t2","@type":"gmeow:Thing"}]}"#;

    let ok = validate_instance(complete, InstanceFormat::Json, &schema).expect("validate");
    assert!(ok.is_empty(), "complete @graph node accepted, got {ok:?}");

    let bad = validate_instance(incomplete, InstanceFormat::Json, &schema).expect("validate");
    assert!(
        !bad.is_empty() && bad.iter().any(|m| m.contains("gmeow:req")),
        "incomplete @graph node must be rejected naming gmeow:req, got {bad:?}"
    );
}
