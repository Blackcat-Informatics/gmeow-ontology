// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{ConstraintProvenance, ShaclNodeKind, ShapeTarget, ShapeValue};

const NS: &str = "https://blackcatinformatics.ca/gmeow/";

fn g(local: &str) -> String {
    format!("{NS}{local}")
}

/// A one-property `Class(K)` shape.
fn class_shape(local: &str, pc: PropertyConstraintIr) -> ValidationShapeIr {
    ValidationShapeIr::new(
        format!("{}-shape", g(local)),
        ShapeTarget::Class(g(local)),
        vec![pc],
        None,
    )
    .unwrap()
}

/// A `Class(K)` shape carrying only node-level components.
fn class_node_shape(local: &str, nodes: Vec<ConstraintComponent>) -> ValidationShapeIr {
    ValidationShapeIr::new(
        format!("{}-shape", g(local)),
        ShapeTarget::Class(g(local)),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(nodes)
    .unwrap()
}

#[test]
fn certify_cardinality_min_max() {
    let pc = PropertyConstraintIr::new(
        g("hasPart"),
        Some(1),
        Some(3),
        Some(ConstraintProvenance::OwlRestriction),
        vec![],
    )
    .unwrap();
    let s = class_shape("Widget", pc);
    certify(&s).expect("cardinality min/max must certify");
}

#[test]
fn certify_exact_cardinality() {
    let pc = PropertyConstraintIr::new(
        g("hasSerial"),
        Some(1),
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![],
    )
    .unwrap();
    let s = class_shape("Gadget", pc);
    // Exact cardinality lowers to owl:cardinality; the round-trip must still hold.
    assert!(lift(&s).axioms_ttl.contains("owl:cardinality"));
    certify(&s).expect("exact cardinality must certify");
}

#[test]
fn certify_functional() {
    let pc = PropertyConstraintIr::new(
        g("hasOwner"),
        None,
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![],
    )
    .unwrap();
    let s = ValidationShapeIr::new(
        format!("{}-domain-shape", g("hasOwner")),
        ShapeTarget::SubjectsOf(g("hasOwner")),
        vec![pc],
        None,
    )
    .unwrap();
    let ttl = lift(&s).axioms_ttl;
    assert!(
        ttl.contains("logic:PropertyCharacteristicAssertion")
            && ttl.contains("logic:characteristicSort logic:functionalProperty"),
        "functional lift emits the canonical logic: carrier record, not owl:FunctionalProperty: {ttl}"
    );
    assert!(
        !ttl.contains("owl:FunctionalProperty"),
        "the deprecated owl:FunctionalProperty marker is no longer lifted: {ttl}"
    );
    certify(&s).expect("functional property must certify");
}

#[test]
fn certify_inverse_functional() {
    let pc = PropertyConstraintIr::new(
        g("hasSsn"),
        None,
        Some(1),
        Some(ConstraintProvenance::OwlRestriction),
        vec![],
    )
    .unwrap()
    .inverted();
    let s = ValidationShapeIr::new(
        format!("{}-range-shape", g("hasSsn")),
        ShapeTarget::ObjectsOf(g("hasSsn")),
        vec![pc],
        None,
    )
    .unwrap();
    let ttl = lift(&s).axioms_ttl;
    assert!(
        ttl.contains("logic:PropertyCharacteristicAssertion")
            && ttl.contains("logic:characteristicSort logic:inverseFunctionalProperty"),
        "inverse-functional lift emits the canonical logic: carrier record, not \
             owl:InverseFunctionalProperty: {ttl}"
    );
    assert!(
        !ttl.contains("owl:InverseFunctionalProperty"),
        "the deprecated owl:InverseFunctionalProperty marker is no longer lifted: {ttl}"
    );
    certify(&s).expect("inverse-functional property must certify");
}

#[test]
fn certify_qualified_value_shape() {
    let pc = PropertyConstraintIr::new(
        g("hasWheel"),
        None,
        None,
        None,
        vec![ConstraintComponent::QualifiedValueShape {
            shape: vec![ConstraintComponent::Class(g("Wheel"))],
            min: Some(1),
            max: None,
        }],
    )
    .unwrap();
    let s = class_shape("Car", pc);
    let prop = lift(&s);
    assert!(prop.axioms_ttl.contains("owl:onClass"));
    assert!(prop.axioms_ttl.contains("owl:minQualifiedCardinality"));
    certify(&s).expect("qualified value shape must certify");
}

#[test]
fn certify_some_values_from_class() {
    // `someValuesFrom C` under-approximates to a bare `sh:class C` component.
    let pc = PropertyConstraintIr::new(
        g("hasEngine"),
        None,
        None,
        None,
        vec![ConstraintComponent::Class(g("Engine"))],
    )
    .unwrap();
    let s = class_shape("Vehicle", pc);
    assert!(lift(&s).axioms_ttl.contains("owl:allValuesFrom"));
    certify(&s).expect("class-membership component must certify");
}

#[test]
fn certify_has_value_iri() {
    let pc = PropertyConstraintIr::new(
        g("hasStatus"),
        None,
        None,
        None,
        vec![ConstraintComponent::HasValue(ShapeValue::Iri(g("Active")))],
    )
    .unwrap();
    let s = class_shape("Account", pc);
    assert!(lift(&s).axioms_ttl.contains("owl:hasValue"));
    certify(&s).expect("hasValue IRI must certify");
}

#[test]
fn certify_in_value_set() {
    let nodes = vec![ConstraintComponent::In(vec![
        ShapeValue::Iri(g("Red")),
        ShapeValue::Iri(g("Green")),
    ])];
    let s = class_node_shape("Signal", nodes);
    assert!(lift(&s).axioms_ttl.contains("owl:oneOf"));
    certify(&s).expect("oneOf value set must certify");
}

#[test]
fn certify_datatype() {
    let pc = PropertyConstraintIr::new(
        g("hasLabel"),
        None,
        None,
        None,
        vec![ConstraintComponent::Datatype(format!("{XSD}string"))],
    )
    .unwrap();
    let s = class_shape("Node", pc);
    certify(&s).expect("datatype component must certify");
}

#[test]
fn certify_node_kind() {
    for nk in [ShaclNodeKind::BlankNodeOrIri, ShaclNodeKind::Literal] {
        let pc = PropertyConstraintIr::new(
            g("hasRef"),
            None,
            None,
            None,
            vec![ConstraintComponent::NodeKindShacl(nk)],
        )
        .unwrap();
        let s = class_shape("Ref", pc);
        certify(&s).unwrap_or_else(|e| panic!("node-kind {nk:?} must certify: {e}"));
    }
}

#[test]
fn certify_disjoint_not() {
    let nodes = vec![ConstraintComponent::Not(Box::new(
        ConstraintComponent::Class(g("Liquid")),
    ))];
    let s = class_node_shape("Solid", nodes);
    assert!(lift(&s).axioms_ttl.contains("owl:disjointWith"));
    certify(&s).expect("disjointness must certify");
}

#[test]
fn certify_domain_with_closure_optin() {
    let nodes = vec![ConstraintComponent::Class(g("Person"))];
    let s = ValidationShapeIr::new(
        format!("{}-domain-shape", g("knows")),
        ShapeTarget::SubjectsOf(g("knows")),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(nodes)
    .unwrap();
    let prop = lift(&s);
    assert!(prop.axioms_ttl.contains("rdfs:domain"));
    assert!(prop.axioms_ttl.contains("logic:ClosedWorldClosure"));
    certify(&s).expect("domain with closure opt-in must certify");
}

#[test]
fn certify_range_with_closure_optin() {
    let nodes = vec![ConstraintComponent::Datatype(format!("{XSD}dateTime"))];
    let s = ValidationShapeIr::new(
        format!("{}-range-shape", g("bornOn")),
        ShapeTarget::ObjectsOf(g("bornOn")),
        vec![],
        None,
    )
    .unwrap()
    .with_node_components(nodes)
    .unwrap();
    let prop = lift(&s);
    assert!(prop.axioms_ttl.contains("rdfs:range"));
    assert!(prop.axioms_ttl.contains("logic:ClosedWorldClosure"));
    certify(&s).expect("range with closure opt-in must certify");
}

#[test]
fn pattern_is_residue_never_emitted_and_core_still_certifies() {
    // A faceted-datatype property shape: a `Datatype` base with a lossy `Pattern` facet.
    let pc = PropertyConstraintIr::new(
        g("hasCode"),
        None,
        None,
        None,
        vec![
            ConstraintComponent::Datatype(format!("{XSD}string")),
            ConstraintComponent::Pattern {
                regex: "^[A-Z]{3}$".into(),
                flags: None,
            },
        ],
    )
    .unwrap();
    let s = class_shape("Product", pc);
    let prop = lift(&s);
    // The lossy pattern is carried in residue, NOT emitted as any sh:pattern-equivalent axiom.
    assert!(
        !prop.axioms_ttl.contains("pattern") && !prop.axioms_ttl.contains("Pattern"),
        "no pattern-equivalent axiom may be emitted: {}",
        prop.axioms_ttl
    );
    assert!(
        prop.residue.iter().any(|r| r.contains("regex-dialect")),
        "the pattern must be recorded as residue: {:?}",
        prop.residue
    );
    // The base datatype IS emitted, and the core (pattern excluded) round-trips.
    assert!(prop.axioms_ttl.contains("owl:onDatatype"));
    certify(&s).expect("the shape-expressible core (pattern excluded) must certify");
}

#[test]
fn length_facets_round_trip() {
    let pc = PropertyConstraintIr::new(
        g("hasName"),
        None,
        None,
        None,
        vec![
            ConstraintComponent::Datatype(format!("{XSD}string")),
            ConstraintComponent::MinLength(2),
            ConstraintComponent::MaxLength(64),
        ],
    )
    .unwrap();
    let s = class_shape("Named", pc);
    let prop = lift(&s);
    assert!(prop.axioms_ttl.contains("xsd:minLength"));
    assert!(prop.axioms_ttl.contains("xsd:maxLength"));
    certify(&s).expect("length facets must round-trip");
}

#[test]
fn numeric_range_has_no_owl_antecedent_and_is_residue() {
    // A SHACL-faithful component OWL cannot state: recorded as residue, never emitted, and the
    // (now-empty) core certifies vacuously.
    let pc = PropertyConstraintIr::new(
        g("hasWeight"),
        None,
        None,
        None,
        vec![ConstraintComponent::NumericRange {
            min: Some(0.0),
            max: Some(100.0),
            min_inclusive: true,
            max_inclusive: true,
        }],
    )
    .unwrap();
    let s = class_shape("Parcel", pc);
    let prop = lift(&s);
    assert!(
        prop.residue
            .iter()
            .any(|r| r.contains("no faithful OWL/RDFS")),
        "numeric range must be residue: {:?}",
        prop.residue
    );
    certify(&s).expect("a fully-residue shape certifies vacuously");
}

#[test]
fn lift_is_deterministic() {
    // A shape mixing several families → the two lifts must be byte-identical.
    let pc1 = PropertyConstraintIr::new(
        g("hasPart"),
        Some(1),
        Some(3),
        Some(ConstraintProvenance::OwlRestriction),
        vec![],
    )
    .unwrap();
    let pc2 = PropertyConstraintIr::new(
        g("hasColour"),
        None,
        None,
        None,
        vec![ConstraintComponent::HasValue(ShapeValue::Iri(g("Blue")))],
    )
    .unwrap();
    let s = ValidationShapeIr::new(
        format!("{}-shape", g("Toy")),
        ShapeTarget::Class(g("Toy")),
        vec![pc1, pc2],
        None,
    )
    .unwrap()
    .with_node_components(vec![ConstraintComponent::Not(Box::new(
        ConstraintComponent::Class(g("Tool")),
    ))])
    .unwrap();
    assert_eq!(lift(&s).axioms_ttl, lift(&s).axioms_ttl);
    certify(&s).expect("the mixed shape's core must certify");
}

#[test]
fn lift_writes_no_files_returns_owned_string() {
    // Guard: the lift surface is emit-only (Principle 4). It returns an owned String; it holds no
    // filesystem-write path. Exercising it must not touch `slices/**` — asserted here by the
    // absence of any I/O in `lift`/`analyze` and by the value-returning signature.
    let pc = PropertyConstraintIr::new(
        g("hasThing"),
        Some(1),
        None,
        Some(ConstraintProvenance::OwlRestriction),
        vec![],
    )
    .unwrap();
    let s = class_shape("Holder", pc);
    let prop = lift(&s);
    // The proposal is in-memory text; a `slices/` path never appears in it.
    assert!(!prop.axioms_ttl.contains("slices/"));
    assert!(prop.axioms_ttl.starts_with("@prefix"));
}
