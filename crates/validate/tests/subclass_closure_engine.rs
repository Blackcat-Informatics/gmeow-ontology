// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Engine capability proof: purrdf's SHACL engine resolves the transitive asserted
//! `rdfs:subClassOf` closure INTERNALLY, for BOTH `sh:targetClass` focus selection and
//! `sh:class` value-node membership — over the RAW data graph, with NO `rdf:type`
//! pre-materialization.
//!
//! This is the load-bearing fact behind gmeow's SHACL story: because the engine closes
//! `sh:class`/`sh:targetClass`, gmeow's own subclass-closure pass
//! (`materialize_subclass_type_closure`) only has to cover the OTHER constraint mechanisms
//! (`sh:sparql`/`sh:SPARQLTarget` bodies matching `?this a Class`), which the engine does NOT
//! close. Both facts are asserted here so the boundary is a tested contract, not a comment.
//!
//! Each instance below is typed ONLY as a deep subclass (`ex:Sub ⊑ ex:Mid ⊑ ex:Super`, and
//! `ex:x a ex:Sub`), never directly as `ex:Super`, so a naive "asserted type only" engine
//! would give the opposite verdict.

use purrdf::shapes::engine::{parse_shapes, validate_dataset};

/// `ex:x` is typed only as the deepest subclass; the two-hop `subClassOf` chain to `ex:Super`
/// is present in the data graph (the engine reads asserted edges, never runs a reasoner).
const DATA: &str = r#"
@prefix ex:   <http://example.org/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

ex:Sub  rdfs:subClassOf ex:Mid .
ex:Mid  rdfs:subClassOf ex:Super .

ex:x a ex:Sub .
ex:y ex:related ex:x .
"#;

fn dataset() -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(DATA.as_bytes(), "text/turtle", None).expect("parse data graph")
}

#[test]
fn sh_target_class_selects_a_deep_subclass_instance() {
    // A shape targeting ex:Super that REQUIRES ex:name. ex:x (typed only ex:Sub) carries no
    // ex:name, so it can only be flagged if the engine selected it as a focus node by closing
    // sh:targetClass over the asserted subClassOf chain.
    const SHAPES: &str = r#"
@prefix ex: <http://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:SuperShape a sh:NodeShape ;
    sh:targetClass ex:Super ;
    sh:property [ sh:path ex:name ; sh:minCount 1 ] .
"#;
    let shapes = parse_shapes(SHAPES).expect("parse shapes");
    let report = validate_dataset(&dataset(), &shapes).expect("validate");

    assert!(
        !report.conforms,
        "a deep-subclass instance must be selected as a focus of the ex:Super-targeting shape \
         (engine sh:targetClass subclass closure), so its missing ex:name must be flagged"
    );
    assert!(
        report
            .results
            .iter()
            .any(|r| format!("{:?}", r.focus_node).contains("http://example.org/x")),
        "the flagged focus node must be ex:x, got {:?}",
        report.results
    );
}

#[test]
fn sh_class_accepts_a_deep_subclass_value() {
    // A shape whose property value must be a SHACL instance of ex:Super. ex:y ex:related ex:x,
    // and ex:x is typed only ex:Sub — so conformance can hold only if the engine closes
    // sh:class over the asserted subClassOf chain.
    const SHAPES: &str = r#"
@prefix ex: <http://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RelatedShape a sh:NodeShape ;
    sh:targetNode ex:y ;
    sh:property [ sh:path ex:related ; sh:class ex:Super ] .
"#;
    let shapes = parse_shapes(SHAPES).expect("parse shapes");
    let report = validate_dataset(&dataset(), &shapes).expect("validate");

    assert!(
        report.conforms,
        "ex:x (typed only ex:Sub) must satisfy sh:class ex:Super via the engine's subclass \
         closure, so the shape must conform, got {:?}",
        report.results
    );
}
