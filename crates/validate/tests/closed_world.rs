// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Closed-world fidelity proof (#700 Task 6, Deliverable 2).
//!
//! The headline acceptance criterion: the SHACL-derived JSON Schema is a
//! CLOSED-WORLD projection — a required property (`sh:minCount 1`) REJECTS an
//! incomplete instance node that the old open-world LinkML schema accepted.
//!
//! We build a one-class shapes graph (`gmeow:Thing` requiring `gmeow:req`),
//! compile it to a JSON Schema with [`gmeow_shacl::json_schema::compile`], and
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

use gmeow_shacl::{engine, json_schema};
use gmeow_validate::instance::{validate_instance, InstanceFormat};

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
    let compiled = json_schema::compile(&shapes);
    compiled.schema_json.into_bytes()
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
    // NB: a bare-node root fails the top-level `anyOf` (neither the @graph
    // envelope branch nor the Node branch holds), so the `jsonschema` crate
    // collapses the sub-error into one `anyOf` message rather than surfacing the
    // missing `gmeow:req`. The property-naming guarantee is asserted in the
    // @graph-envelope variant below, where the per-node sub-error surfaces.
    assert!(
        violations.iter().any(|m| m.contains("anyOf")),
        "the incomplete bare node must fail the root anyOf, got {violations:?}"
    );
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
