// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Engine capability proof: purrdf's SHACL engine resolves the transitive asserted
//! `rdfs:subClassOf` closure over the RAW data graph — with NO `rdf:type`
//! pre-materialization — through BOTH of the mechanisms gmeow now relies on:
//!
//!   1. Natively, for `sh:targetClass` focus selection and `sh:class` value-node membership.
//!   2. For the `sh:sparql`/`sh:SPARQLTarget` bodies the engine does NOT class-close, via the
//!      `a/<rdfs:subClassOf>*` PROPERTY PATH the constraint projector (and the legacy shape
//!      bodies) now emit for every body-position `rdf:type` atom. The engine evaluates that
//!      path, so a subclass-only-typed node is matched exactly as a whole-dataset closure
//!      would have matched it — for positive selection AND inside `FILTER NOT EXISTS`.
//!
//! Together these subsume gmeow's retired `materialize_subclass_type_closure` pass: the
//! validator now runs SHACL over the raw dataset directly. Each instance below is typed ONLY
//! as a deep subclass (`ex:Sub ⊑ ex:Mid ⊑ ex:Super`, and `ex:x a ex:Sub`), never directly as
//! `ex:Super`, so a naive "asserted type only" engine would give the opposite verdict.

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
    let shapes = parse_shapes(SHAPES, None).expect("parse shapes");
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
    let shapes = parse_shapes(SHAPES, None).expect("parse shapes");
    let report = validate_dataset(&dataset(), &shapes).expect("validate");

    assert!(
        report.conforms,
        "ex:x (typed only ex:Sub) must satisfy sh:class ex:Super via the engine's subclass \
         closure, so the shape must conform, got {:?}",
        report.results
    );
}

#[test]
fn sparql_target_subclass_path_selects_a_deep_subclass_instance() {
    // The projected `sh:SPARQLTarget` idiom the constraint projector now emits: focus selection
    // through `?this a/<rdfs:subClassOf>* C`. ex:x is typed only ex:Sub, yet must be selected as a
    // focus of the ex:Super-targeting shape (so its missing ex:name is flagged) — proving the
    // engine walks the property path over the RAW data graph with no rdf:type pre-materialization.
    // Falsifiable: with a plain `?this a C` (the pre-refactor form) the engine would NOT select a
    // subclass-only-typed node and the report would conform.
    const SHAPES: &str = r#"
@prefix ex: <http://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:SuperSparqlShape a sh:NodeShape ;
    sh:target [
        a sh:SPARQLTarget ;
        sh:select """SELECT ?this WHERE { ?this a/<http://www.w3.org/2000/01/rdf-schema#subClassOf>* <http://example.org/Super> . }""" ;
    ] ;
    sh:property [ sh:path ex:name ; sh:minCount 1 ] .
"#;
    let shapes = parse_shapes(SHAPES, None).expect("parse shapes");
    let report = validate_dataset(&dataset(), &shapes).expect("validate");

    assert!(
        !report.conforms,
        "the a/<subClassOf>* SPARQLTarget must select the deep-subclass instance ex:x over the raw \
         data graph, so its missing ex:name must be flagged"
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
fn sparql_constraint_subclass_path_closes_a_negated_membership() {
    // The projected `FILTER NOT EXISTS` idiom over a value node reached from the focus: ex:y is
    // related to ex:x (typed only ex:Sub ⊑* ex:Super). A constraint that fires when the related
    // value is NOT a (subclass) instance of ex:Super must NOT fire here — proving `a/<subClassOf>*`
    // closes the chain INSIDE FILTER NOT EXISTS over raw data (the negated-atom equivalence the
    // deleted pre-pass provided). Falsifiable: with a plain `?v a C`, ex:x would read as not-a-Super
    // and the constraint would fire (report would not conform).
    const SHAPES: &str = r#"
@prefix ex: <http://example.org/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RelatedSparqlShape a sh:NodeShape ;
    sh:targetNode ex:y ;
    sh:sparql [
        a sh:SPARQLConstraint ;
        sh:message "a related value is not a (subclass) instance of ex:Super" ;
        sh:select """SELECT $this WHERE { $this <http://example.org/related> ?v . FILTER NOT EXISTS { ?v a/<http://www.w3.org/2000/01/rdf-schema#subClassOf>* <http://example.org/Super> . } }""" ;
    ] .
"#;
    let shapes = parse_shapes(SHAPES, None).expect("parse shapes");
    let report = validate_dataset(&dataset(), &shapes).expect("validate");

    assert!(
        report.conforms,
        "ex:x (typed only ex:Sub) must satisfy the a/<subClassOf>* membership inside FILTER NOT \
         EXISTS, so the negated constraint must NOT fire, got {:?}",
        report.results
    );
}
