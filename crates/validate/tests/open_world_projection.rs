// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Open-world fidelity proof for the SHACL → JSON-Schema projection.
//!
//! The closed-world JSON Schema (`purrdf::shapes::json_schema::compile`) is an
//! ADDITIVE projection: a `sh:targetClass` NodeShape constrains what it names and
//! leaves everything else open, closing the class ONLY on an explicit
//! `sh:closed true`. Two soundness properties that a naive emitter gets wrong are
//! pinned here against the real compiler:
//!
//! 1. A disjointness-only shape (`sh:not [ sh:class … ]`, no `sh:property`,
//!    no `sh:closed`) must stay OPEN — an instance of the target class carrying
//!    real properties is SHACL-conformant and MUST NOT be rejected. This is the
//!    organic `Agent-shape` shape class derived into the validation-shapes union.
//! 2. `sh:not [ sh:class X ]` must compile to a NEGATED `@type`-membership test
//!    for `X`, valid even when `X` is otherwise unmodeled (no `$def`). A naive
//!    emitter lowers it to `not { <generic node object> }`, which matches every
//!    object and so fails the `not` for every node.
//!
//! Every test drives the SAME production surface the corpus sweep uses
//! (the focused projection contract): shapes are compiled with `json_schema::compile`, data
//! graphs are parsed with the native codec and projected to JSON-LD with
//! `instance::project_graph`, and the projection is validated with
//! `gmeow_validate::instance::validate_instance`. The final test is a Principle-17
//! soundness+completeness oracle: the projection MUST agree with native SHACL
//! (`engine::validate_dataset`) on discriminating instances.

use gmeow_validate::instance::{InstanceFormat, validate_instance};
use purrdf::parse_dataset;
use purrdf::shapes::{engine, instance, json_schema};

/// The gmeow namespace table used by both the schema compiler and the JSON-LD
/// projector — identical to the corpus sweep's, so CURIE compaction matches
/// production exactly.
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

/// Compile a shapes Turtle document into `(schema_bytes, parsed_schema)`.
fn compile_schema(shapes_ttl: &str) -> (Vec<u8>, serde_json::Value) {
    let shapes = engine::parse_shapes(shapes_ttl, None).expect("parse shapes");
    let compiled = json_schema::compile(&shapes, &gmeow_namespaces());
    let parsed: serde_json::Value =
        serde_json::from_str(&compiled.schema_json).expect("compiled schema is valid JSON");
    (compiled.schema_json.into_bytes(), parsed)
}

/// Parse a data-graph Turtle document with the native codec and project it to
/// JSON-LD instance bytes — the exact production path the corpus sweep runs.
fn project_instance(data_ttl: &str) -> Vec<u8> {
    let store = parse_dataset(data_ttl.as_bytes(), "text/turtle", None).expect("parse data graph");
    let value = instance::project_graph(&store, &gmeow_namespaces());
    serde_json::to_vec(&value).expect("serialize projected instance")
}

/// Whether `data_ttl` conforms to `shapes_ttl` per the native SHACL engine.
fn shacl_conforms(shapes_ttl: &str, data_ttl: &str) -> bool {
    let shapes = engine::parse_shapes(shapes_ttl, None).expect("parse shapes");
    let store = parse_dataset(data_ttl.as_bytes(), "text/turtle", None).expect("parse data graph");
    engine::validate_dataset(store.as_ref(), &shapes)
        .expect("validate_dataset over a frozen dataset is infallible")
        .conforms
}

/// Whether the JSON-Schema projection of `shapes_ttl` accepts `data_ttl`.
fn projection_accepts(shapes_ttl: &str, data_ttl: &str) -> Vec<String> {
    let (schema, _) = compile_schema(shapes_ttl);
    let instance = project_instance(data_ttl);
    validate_instance(&instance, InstanceFormat::Json, &schema)
        .expect("validate_instance must not hard-error")
}

/// The `$defs/<local>` body of a compiled schema (panics if absent).
fn class_def<'a>(schema: &'a serde_json::Value, local: &str) -> &'a serde_json::Value {
    schema
        .get("$defs")
        .and_then(|d| d.get(local))
        .unwrap_or_else(|| panic!("compiled schema is missing $defs/{local}"))
}

/// A disjointness-only NodeShape targeting `gmeow:Agent`: two `sh:not [ sh:class … ]`
/// constraints, NO `sh:property`, NO `sh:closed`. This is the derived `Agent-shape`
/// shape class that surfaced the projection unsoundness.
const DISJOINTNESS_ONLY: &str = r#"
@prefix sh:    <http://www.w3.org/ns/shacl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:AgentShape a sh:NodeShape ;
    sh:targetClass gmeow:Agent ;
    sh:not [ sh:class gmeow:SocialObject ] ;
    sh:not [ sh:class gmeow:InformationObject ] .
"#;

#[test]
fn disjointness_only_targetclass_stays_open() {
    // An Agent carrying a real property (not typed any disjoint class) is
    // SHACL-conformant; the additive projection MUST accept it.
    let data = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:ada a gmeow:Agent ;
    gmeow:knowsThat ex:earthOrbitsSun .
"#;
    let violations = projection_accepts(DISJOINTNESS_ONLY, data);
    assert!(
        violations.is_empty(),
        "a disjointness-only sh:targetClass shape must stay OPEN — an Agent carrying \
         a real property must be accepted, got {violations:?}"
    );

    // Structural co-assert: the class body carries NO `additionalProperties:false`.
    let (_, schema) = compile_schema(DISJOINTNESS_ONLY);
    let agent = class_def(&schema, "Agent");
    assert!(
        agent.get("additionalProperties").is_none(),
        "an unclosed shape must not emit additionalProperties on #/$defs/Agent, got {agent}"
    );
}

#[test]
fn sh_closed_true_still_closes() {
    // A PURE `sh:closed true` shape (NO `sh:not`, NO `sh:property`) MUST close the
    // class to only the structural keys — this pins the "open UNLESS sh:closed"
    // contract in both directions, so the open-world fix cannot silently disable
    // legitimate closure. Isolating closure from `sh:not` keeps the rejection signal
    // attributable to closure ALONE, not to a disjointness constraint (so this test
    // stays a true closure regression even if `sh:not` were to regress separately).
    let shapes = r#"
@prefix sh:    <http://www.w3.org/ns/shacl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:AgentShape a sh:NodeShape ;
    sh:targetClass gmeow:Agent ;
    sh:closed true .
"#;
    // A node carrying an undeclared property is REJECTED by closure alone.
    let dirty = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:ada a gmeow:Agent ;
    gmeow:knowsThat ex:earthOrbitsSun .
"#;
    let violations = projection_accepts(shapes, dirty);
    assert!(
        !violations.is_empty(),
        "sh:closed true must close the class — an undeclared property must be rejected"
    );

    // A clean node carrying ONLY the structural keys (`@type`) is still ACCEPTED —
    // closure rejects undeclared properties, it does not reject conformant nodes.
    let clean = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:eve a gmeow:Agent .
"#;
    let violations = projection_accepts(shapes, clean);
    assert!(
        violations.is_empty(),
        "sh:closed true must still ACCEPT a node carrying only structural keys, got {violations:?}"
    );

    let (_, schema) = compile_schema(shapes);
    let agent = class_def(&schema, "Agent");
    assert_eq!(
        agent.get("additionalProperties"),
        Some(&serde_json::Value::Bool(false)),
        "sh:closed true must emit additionalProperties:false on #/$defs/Agent, got {agent}"
    );
}

#[test]
fn sh_not_class_over_unmodeled_x_negates_by_type() {
    // `gmeow:UnmodeledThing` has NO NodeShape, so it never receives a `$def`.
    // The `sh:not [ sh:class gmeow:UnmodeledThing ]` must still compile to a
    // negated @type-membership test, not `not { <any object> }`.
    let shapes = r#"
@prefix sh:    <http://www.w3.org/ns/shacl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:AgentShape a sh:NodeShape ;
    sh:targetClass gmeow:Agent ;
    sh:not [ sh:class gmeow:UnmodeledThing ] .
"#;

    // A node typed the UNMODELED class must be REJECTED (it IS an UnmodeledThing),
    // and — Principle-17 oracle — the projection must AGREE with native SHACL rather
    // than merely match a hard-coded expectation.
    let is_unmodeled = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:x a gmeow:Agent, gmeow:UnmodeledThing .
"#;
    let shacl = shacl_conforms(shapes, is_unmodeled);
    assert!(
        !shacl,
        "native SHACL premise: a node typed the sh:not[sh:class X] class must be non-conformant"
    );
    let accepted = projection_accepts(shapes, is_unmodeled).is_empty();
    assert_eq!(
        accepted, shacl,
        "sh:not[sh:class X] over unmodeled X: projection must AGREE with native SHACL for a \
         node typed X (SHACL conforms={shacl}, projection accepts={accepted}) — X has no $def, \
         so the `not` must be a negated @type test, not a match-any-object inversion"
    );

    // A node NOT typed the unmodeled class must be ACCEPTED — proving the `not`
    // discriminates on @type rather than matching every object — and again agree
    // with native SHACL.
    let not_unmodeled = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:y a gmeow:Agent .
"#;
    let shacl = shacl_conforms(shapes, not_unmodeled);
    assert!(
        shacl,
        "native SHACL premise: a node NOT typed the sh:not[sh:class X] class must be conformant"
    );
    let accepted = projection_accepts(shapes, not_unmodeled).is_empty();
    assert_eq!(
        accepted, shacl,
        "sh:not[sh:class X] over unmodeled X: projection must AGREE with native SHACL for a \
         node not typed X (SHACL conforms={shacl}, projection accepts={accepted})"
    );
}

#[test]
fn projection_agrees_with_native_shacl_on_disjointness_only() {
    // Principle-17 soundness+completeness oracle: for the disjointness-only shape,
    // the JSON-Schema projection must AGREE with native SHACL on both a conformant
    // and a non-conformant instance — accept exactly what SHACL accepts.
    let conformant = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:ada a gmeow:Agent ;
    gmeow:knowsThat ex:earthOrbitsSun .
"#;
    // Typed BOTH gmeow:Agent and a disjoint class → violates sh:not[sh:class …].
    let non_conformant = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:bad a gmeow:Agent, gmeow:SocialObject .
"#;

    for (label, data, expect_conform) in [
        ("conformant", conformant, true),
        ("non-conformant", non_conformant, false),
    ] {
        let shacl = shacl_conforms(DISJOINTNESS_ONLY, data);
        assert_eq!(
            shacl, expect_conform,
            "native SHACL disagreed with the fixture premise for the {label} instance"
        );
        let accepted = projection_accepts(DISJOINTNESS_ONLY, data).is_empty();
        assert_eq!(
            accepted, shacl,
            "the JSON-Schema projection must agree with native SHACL on the {label} \
             instance (SHACL conforms={shacl}, projection accepts={accepted})"
        );
    }
}
