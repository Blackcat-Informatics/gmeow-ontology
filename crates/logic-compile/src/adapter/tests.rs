// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the OWL/gUFO adapter, driven by Turtle source strings.
//!
//! These are the authoritative adapter tests; they supersede the Python
//! `tests/test_logic_adapter.py`, which was retired.

use super::*;
use crate::frontend::parse_logic_str;

const PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gufo:  <http://purl.org/nemo/gufo#> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
";

fn adapt(ttl: &str) -> (LogicProgram, Vec<Diagnostic>) {
    adapt_legacy_str(&format!("{PREFIXES}{ttl}"), None).expect("adapt ok")
}

fn logic_prog(ttl: &str) -> LogicProgram {
    parse_logic_str(&format!("{PREFIXES}{ttl}"), None)
        .expect("parse ok")
        .0
}

// ── Error paths ──────────────────────────────────────────────────────────────

#[test]
fn adapt_empty_graph_raises() {
    let err = adapt_legacy_str(PREFIXES, None).unwrap_err();
    assert!(err.0.contains("empty"));
}

// ── RDFS/SKOS annotation lift (issue #1200 R1/R2) ────────────────────────────

const ANNOTATED_TERM: &str = r#"
ex:Widget rdfs:label "Widget"@x-gmeow-english ;
    rdfs:comment "A widget, canonically."@x-gmeow-english ;
    skos:definition "The canonical widget concept."@x-gmeow-english ;
    skos:prefLabel "widget"@x-gmeow-english ;
    skos:altLabel "gadget"@x-gmeow-english ;
    skos:scopeNote "Use for gizmos, not doohickeys."@x-gmeow-english .
"#;

#[test]
fn annotation_lift_produces_six_first_class_annotation_axioms() {
    let prog = logic_prog(ANNOTATED_TERM);
    let anns: Vec<_> = prog
        .axioms
        .iter()
        .filter(|a| a.node_kind == NodeKind::Annotation)
        .collect();
    assert_eq!(anns.len(), 6, "all six annotation predicates lift");
    // The prose annotations (skos:definition, rdfs:comment) are load-bearing; the display
    // labels are droppable hints.
    for a in &anns {
        let want_load_bearing = a.predicate.ends_with("#comment")
            || a.predicate.ends_with("core#definition");
        assert_eq!(
            a.load_bearing, want_load_bearing,
            "load_bearing bit for predicate {}",
            a.predicate
        );
        assert!(a.obj_is_literal, "annotation object is a literal");
    }
}

#[test]
fn annotation_lift_owl_and_logic_twins_isomorphic() {
    // The annotation surface is authored in identical syntax on both the owl/rdfs adapter
    // path and the logic: frontend path, so the two must normalize to identical annotation
    // axioms — the isomorphism gate that keeps the two lift sites from drifting.
    let owl = adapt(ANNOTATED_TERM).0;
    let logic = logic_prog(ANNOTATED_TERM);
    assert_ir_isomorphic(&owl, &logic).expect("annotation twins must be IR-isomorphic");
}

#[test]
fn annotation_lift_fails_closed_on_non_carrier_tag() {
    // A non-carrier language tag is a discipline violation: a blocking diagnostic is emitted
    // and NO annotation axiom is produced (never a silent retag). (R2/AC2)
    let (prog, diags) = parse_logic_str(
        &format!("{PREFIXES}\nex:Bad rdfs:label \"Widget\"@en ."),
        None,
    )
    .expect("parse ok");
    assert!(
        prog.axioms
            .iter()
            .all(|a| a.node_kind != NodeKind::Annotation),
        "a non-carrier-tagged annotation must NOT be lifted"
    );
    assert!(
        diags.iter().any(|d| d.code == "NON_CARRIER_ANNOTATION_LANG"
            && d.severity == Severity::Error),
        "a blocking NON_CARRIER_ANNOTATION_LANG diagnostic is emitted"
    );
}

#[test]
fn adapt_nonexistent_file_raises() {
    let err = adapt_legacy_path(Path::new("/no/such/adapter-xyz.ttl"), None).unwrap_err();
    assert!(err.0.contains("does not exist"));
}

#[test]
fn adapt_invalid_turtle_raises() {
    let err = adapt_legacy_str("not turtle <<<", None).unwrap_err();
    assert!(err.0.contains("Failed to parse"));
}

// ── gUFO stereotype → logic: sort ────────────────────────────────────────────

#[test]
fn adapt_gufo_kind_to_logic_kind() {
    let (prog, _) = adapt("ex:Person a gufo:Kind .");
    assert_eq!(prog.axioms.len(), 1);
    let ax = &prog.axioms[0];
    assert!(ax.subject.ends_with("/Person"));
    assert_eq!(ax.predicate, RDF_TYPE);
    assert_eq!(ax.obj, logic("Kind"));
}

#[test]
fn adapt_gufo_event_and_situation_type() {
    let (prog, _) = adapt(
        "ex:Birth a gufo:EventType .
         ex:Snapshot a gufo:SituationType .",
    );
    assert!(prog.axioms.iter().any(|a| a.obj == logic("Event")));
    assert!(prog.axioms.iter().any(|a| a.obj == logic("Situation")));
}

#[test]
fn adapt_all_gufo_sortals() {
    let (prog, diags) = adapt(
        "ex:K a gufo:Kind . ex:Sk a gufo:SubKind . ex:Ph a gufo:Phase .
         ex:Ro a gufo:Role . ex:Ca a gufo:Category . ex:Mi a gufo:Mixin .
         ex:Rm a gufo:RoleMixin . ex:Pm a gufo:PhaseMixin . ex:Re a gufo:Relator .",
    );
    assert!(diags.is_empty());
    assert_eq!(prog.axioms.len(), 9);
    for sort in [
        "Kind",
        "SubKind",
        "Phase",
        "Role",
        "Category",
        "Mixin",
        "RoleMixin",
        "PhaseMixin",
        "Relator",
    ] {
        assert!(
            prog.axioms.iter().any(|a| a.obj == logic(sort)),
            "missing sort {sort}"
        );
    }
}

#[test]
fn adapt_blank_node_gufo_sort_emits_diagnostic() {
    let (prog, diags) = adapt("[] a gufo:Kind .");
    assert!(prog.axioms.is_empty());
    assert!(diags.iter().any(|d| d.code == "BLANK_NODE_GUFO_SORT"));
}

// ── OWL structural predicates ────────────────────────────────────────────────

#[test]
fn adapt_rdfs_subclass_of() {
    let (prog, _) = adapt("ex:Employee rdfs:subClassOf ex:Person .");
    let ax = &prog.axioms[0];
    assert_eq!(ax.predicate, logic("subClassOf"));
    assert!(ax.subject.ends_with("/Employee"));
    assert!(ax.obj.ends_with("/Person"));
}

#[test]
fn adapt_owl_equivalent_disjoint_inverse() {
    let (prog, _) = adapt(
        "ex:A owl:equivalentClass ex:B .
         ex:A owl:disjointWith ex:C .
         ex:p owl:inverseOf ex:q .",
    );
    assert!(
        prog.axioms
            .iter()
            .any(|a| a.predicate == logic("equivalentClass"))
    );
    assert!(
        prog.axioms
            .iter()
            .any(|a| a.predicate == logic("disjointWith"))
    );
    assert!(
        prog.axioms
            .iter()
            .any(|a| a.predicate == logic("inverseOf"))
    );
}

#[test]
fn adapt_rdfs_domain_and_range() {
    let (prog, _) = adapt(
        "ex:p rdfs:domain ex:A .
         ex:p rdfs:range ex:B .",
    );
    assert!(prog.axioms.iter().any(|a| a.predicate == logic("domain")));
    assert!(prog.axioms.iter().any(|a| a.predicate == logic("range")));
}

// ── OWL property characteristics ─────────────────────────────────────────────

#[test]
fn adapt_owl_characteristics() {
    let (prog, _) = adapt(
        "ex:p a owl:TransitiveProperty .
         ex:q a owl:SymmetricProperty .
         ex:r a owl:FunctionalProperty .
         ex:s a owl:InverseFunctionalProperty .
         ex:t a owl:ReflexiveProperty .
         ex:u a owl:AsymmetricProperty .
         ex:v a owl:IrreflexiveProperty .",
    );
    for c in [
        "transitiveProperty",
        "symmetricProperty",
        "functionalProperty",
        "inverseFunctionalProperty",
        "reflexiveProperty",
        "asymmetricProperty",
        "irreflexiveProperty",
    ] {
        assert!(
            prog.axioms.iter().any(|a| a.obj == logic(c)),
            "missing characteristic {c}"
        );
    }
}

// ── OWL restrictions (class-expression lift) ─────────────────────────────────

/// The skolem restriction node an authored class points at (`C logic:subClassOf R`).
fn restriction_node_of(prog: &LogicProgram, class_suffix: &str) -> String {
    prog.axioms
        .iter()
        .find(|a| a.predicate == logic("subClassOf") && a.subject.ends_with(class_suffix))
        .map(|a| a.obj.clone())
        .expect("subClassOf → restriction anchor")
}

#[test]
fn blank_node_restriction_lifts_to_skolem_axioms() {
    let (prog, diags) = adapt(
        "ex:Bird rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasBeak ; owl:someValuesFrom ex:Beak ] .",
    );
    // No fail-soft skip: the restriction lifts to first-class logic: axioms.
    assert!(
        !diags.iter().any(|d| d.code == "UNMAPPED_OWL_CONSTRUCT"),
        "restriction must not be dropped: {diags:?}"
    );
    let r = restriction_node_of(&prog, "/Bird");
    assert!(
        r.starts_with(&logic("restriction/")),
        "anchor must be a deterministic skolem IRI, got {r}"
    );
    let has = |s: &str, p: &str, o: &str| {
        prog.axioms
            .iter()
            .any(|a| a.subject == s && a.predicate == p && a.obj == o)
    };
    assert!(has(&r, RDF_TYPE, &logic("Restriction")));
    assert!(has(
        &r,
        &logic("onProperty"),
        "https://example.org/test/hasBeak"
    ));
    assert!(has(
        &r,
        &logic("someValuesFrom"),
        "https://example.org/test/Beak"
    ));
}

#[test]
fn restriction_missing_on_property_emits_diagnostic() {
    let (prog, diags) = adapt(
        "ex:Bird rdfs:subClassOf [ a owl:Restriction ;
            owl:someValuesFrom ex:Beak ] .",
    );
    assert!(diags.iter().any(|d| d.code == "MALFORMED_RESTRICTION"));
    assert!(
        !prog
            .axioms
            .iter()
            .any(|a| a.predicate == logic("someValuesFrom")),
        "a restriction with no onProperty must not lift a constraint"
    );
}

#[test]
#[should_panic(expected = "onProperty values")]
fn restriction_with_two_on_properties_hard_fails() {
    // Two onProperty values on one restriction is a wiring contradiction, not a
    // disclosable malformedness — pick-first would silently drop a slot, so the lift
    // must hard-fail rather than continue.
    let _ = adapt(
        "ex:Bird rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasBeak ; owl:onProperty ex:hasWing ;
            owl:someValuesFrom ex:Beak ] .",
    );
}

#[test]
fn two_classes_share_one_restriction_node() {
    // Identical restriction on two classes → ONE skolem node (structure sharing);
    // the content key excludes the subject class.
    let (prog, _) = adapt(
        "ex:Bird rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasBeak ; owl:someValuesFrom ex:Beak ] .
         ex:Duck rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasBeak ; owl:someValuesFrom ex:Beak ] .",
    );
    let bird_r = restriction_node_of(&prog, "/Bird");
    let duck_r = restriction_node_of(&prog, "/Duck");
    assert_eq!(bird_r, duck_r, "identical restrictions must share one node");
    // The shared node's defining axioms appear exactly once (dedup).
    let type_count = prog
        .axioms
        .iter()
        .filter(|a| a.subject == bird_r && a.predicate == RDF_TYPE)
        .count();
    assert_eq!(type_count, 1, "shared restriction internals must dedup");
}

// ── Round-trip: owl:Restriction ≡ logic:Restriction ──────────────────────────

#[test]
fn roundtrip_owl_somevaluesfrom_equals_logic() {
    let prog_logic = logic_prog(
        "ex:Bird logic:subClassOf [ a logic:Restriction ;
            logic:onProperty ex:hasBeak ; logic:someValuesFrom ex:Beak ] .",
    );
    let prog_owl = adapt(
        "ex:Bird rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasBeak ; owl:someValuesFrom ex:Beak ] .",
    )
    .0;
    // Same skolem IRI on both surfaces (the content key agrees byte-for-byte).
    assert_eq!(
        restriction_node_of(&prog_logic, "/Bird"),
        restriction_node_of(&prog_owl, "/Bird")
    );
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_owl_hasvalue_iri_equals_logic() {
    let prog_logic = logic_prog(
        "ex:RedThing logic:subClassOf [ a logic:Restriction ;
            logic:onProperty ex:hasColour ; logic:hasValue ex:Red ] .",
    );
    let prog_owl = adapt(
        "ex:RedThing rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasColour ; owl:hasValue ex:Red ] .",
    )
    .0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_owl_hasvalue_literal_equals_logic() {
    let prog_logic = logic_prog(
        "ex:Adult logic:subClassOf [ a logic:Restriction ;
            logic:onProperty ex:minAge ; logic:hasValue 18 ] .",
    );
    let prog_owl = adapt(
        "ex:Adult rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:minAge ; owl:hasValue 18 ] .",
    )
    .0;
    // The literal filler round-trips (obj_is_literal preserved on both surfaces).
    assert!(
        prog_owl
            .axioms
            .iter()
            .any(|a| a.predicate == logic("hasValue") && a.obj_is_literal)
    );
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_owl_allvaluesfrom_equals_logic() {
    let prog_logic = logic_prog(
        "ex:VegDish logic:subClassOf [ a logic:Restriction ;
            logic:onProperty ex:hasIngredient ; logic:allValuesFrom ex:Vegetable ] .",
    );
    let prog_owl = adapt(
        "ex:VegDish rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasIngredient ; owl:allValuesFrom ex:Vegetable ] .",
    )
    .0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_owl_min_cardinality_equals_logic() {
    let prog_logic = logic_prog(
        "ex:Parent logic:subClassOf [ a logic:Restriction ;
            logic:onProperty ex:hasChild ; logic:minCardinality 1 ] .",
    );
    let prog_owl = adapt(
        "ex:Parent rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasChild ; owl:minCardinality 1 ] .",
    )
    .0;
    assert!(
        prog_owl
            .axioms
            .iter()
            .any(|a| a.predicate == logic("minCardinality") && a.obj == "1" && a.obj_is_literal)
    );
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_owl_qualified_cardinality_equals_logic() {
    // A qualified cardinality node carries both the count and its onClass filler; both
    // lift as constraints on the same skolem node and both feed the content key.
    let prog_logic = logic_prog(
        "ex:Hand logic:subClassOf [ a logic:Restriction ;
            logic:onProperty ex:hasPart ; logic:qualifiedCardinality 5 ;
            logic:onClass ex:Finger ] .",
    );
    let prog_owl = adapt(
        "ex:Hand rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasPart ; owl:qualifiedCardinality 5 ;
            owl:onClass ex:Finger ] .",
    )
    .0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
    // Distinct qualified cardinality on the same property but a different onClass mints a
    // DIFFERENT skolem node (onClass participates in the content key).
    let other = adapt(
        "ex:Hand rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:hasPart ; owl:qualifiedCardinality 5 ;
            owl:onClass ex:Thumb ] .",
    )
    .0;
    assert_ne!(
        restriction_node_of(&prog_owl, "/Hand"),
        restriction_node_of(&other, "/Hand")
    );
}

#[test]
fn nested_class_expression_filler_is_disclosed_not_lifted() {
    // someValuesFrom of an anonymous class expression (a nested union) has no stable
    // filler identity: the restriction must be DISCLOSED (surfaced), never lifted with a
    // non-deterministic blank label.
    let (prog, diags) = adapt(
        "ex:Weird rdfs:subClassOf [ a owl:Restriction ;
            owl:onProperty ex:rel ;
            owl:someValuesFrom [ a owl:Class ; owl:unionOf ( ex:A ex:B ) ] ] .",
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == "UNSUPPORTED_NESTED_RESTRICTION"),
        "a nested-filler restriction must be disclosed: {diags:?}"
    );
    assert!(
        !prog
            .axioms
            .iter()
            .any(|a| a.predicate == logic("someValuesFrom")),
        "no blank-labelled filler may leak into the IR"
    );
}

// ── OWL enumerations (owl:oneOf) ─────────────────────────────────────────────

#[test]
fn roundtrip_owl_oneof_enumeration_equals_logic() {
    // An anonymous owl:oneOf enumeration and its logic: twin normalize to the same
    // content-addressed logic:enumeration node with individual logic:oneOf axioms.
    let prog_logic = logic_prog(
        "ex:Season logic:equivalentClass [ a logic:Enumeration ;
            logic:oneOf ( ex:Spring ex:Summer ex:Autumn ex:Winter ) ] .",
    );
    let prog_owl = adapt(
        "ex:Season owl:equivalentClass [ a owl:Class ;
            owl:oneOf ( ex:Spring ex:Summer ex:Autumn ex:Winter ) ] .",
    )
    .0;
    let e = prog_owl
        .axioms
        .iter()
        .find(|a| a.predicate == logic("equivalentClass") && a.subject.ends_with("/Season"))
        .map(|a| a.obj.clone())
        .expect("equivalentClass → enumeration anchor");
    assert!(
        e.starts_with(&logic("enumeration/")),
        "anchor must be a deterministic skolem enumeration IRI, got {e}"
    );
    assert_eq!(
        prog_owl
            .axioms
            .iter()
            .filter(|a| a.subject == e && a.predicate == logic("oneOf"))
            .count(),
        4,
        "four members lift as individual oneOf axioms"
    );
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn enumeration_with_broken_list_emits_diagnostic() {
    // A oneOf list that is not nil-terminated (a cell missing rdf:rest) is corrupt.
    // The lift must disclose it and skip the enumeration rather than silently lift a
    // truncated member set.  Hand-authored raw list cells (the ( … ) collection syntax
    // only ever emits well-formed lists).
    let (prog, diags) = adapt(
        "ex:Season owl:equivalentClass [ a owl:Class ; owl:oneOf _:l0 ] .
         _:l0 rdf:first ex:Spring ; rdf:rest _:l1 .
         _:l1 rdf:first ex:Summer .",
    );
    assert!(
        diags.iter().any(|d| d.code == "MALFORMED_ENUMERATION"),
        "a corrupt oneOf list must surface a MALFORMED_ENUMERATION diagnostic"
    );
    assert!(
        !prog.axioms.iter().any(|a| a.predicate == logic("oneOf")),
        "a corrupt enumeration must not lift any oneOf member"
    );
    assert!(
        !prog
            .axioms
            .iter()
            .any(|a| a.obj.starts_with(&logic("enumeration/"))),
        "a corrupt enumeration must not lift a skolem enumeration node"
    );
}

// ── OWL datatype restrictions (owl:withRestrictions dataranges) ──────────────

const XSD: &str = "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n";

#[test]
fn roundtrip_owl_withrestrictions_equals_logic() {
    // An owl:withRestrictions datarange and its logic: twin normalize to the same
    // content-addressed logic:datarange node with a logic:onDatatype base and one axiom
    // per xsd: facet.
    let prog_logic = logic_prog(&format!(
        "{XSD}ex:PositiveScore logic:equivalentClass [ a rdfs:Datatype ;
            logic:onDatatype xsd:decimal ;
            logic:withRestrictions ( [ xsd:minInclusive \"0.0\"^^xsd:decimal ]
                                     [ xsd:maxInclusive \"1.0\"^^xsd:decimal ] ) ] ."
    ));
    let prog_owl = adapt(&format!(
        "{XSD}ex:PositiveScore owl:equivalentClass [ a rdfs:Datatype ;
            owl:onDatatype xsd:decimal ;
            owl:withRestrictions ( [ xsd:minInclusive \"0.0\"^^xsd:decimal ]
                                   [ xsd:maxInclusive \"1.0\"^^xsd:decimal ] ) ] ."
    ))
    .0;
    let d = prog_owl
        .axioms
        .iter()
        .find(|a| a.predicate == logic("equivalentClass") && a.subject.ends_with("/PositiveScore"))
        .map(|a| a.obj.clone())
        .expect("equivalentClass → datarange anchor");
    assert!(
        d.starts_with(&logic("datarange/")),
        "anchor must be a deterministic skolem datarange IRI, got {d}"
    );
    // The base datatype rides on logic:onDatatype (an IRI object).
    assert!(prog_owl.axioms.iter().any(|a| a.subject == d
        && a.predicate == logic("onDatatype")
        && a.obj.ends_with("XMLSchema#decimal")
        && !a.obj_is_literal));
    // Each facet rides on its full xsd: IRI as a literal-valued axiom.
    let facet_count = prog_owl
        .axioms
        .iter()
        .filter(|a| {
            a.subject == d
                && a.predicate.starts_with("http://www.w3.org/2001/XMLSchema#")
                && a.obj_is_literal
        })
        .count();
    assert_eq!(facet_count, 2, "two facets lift as literal-valued axioms");
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn datarange_missing_ondatatype_emits_diagnostic() {
    // A withRestrictions datarange with no owl:onDatatype base is malformed: the lift must
    // disclose it and skip rather than mint a base-less node.
    let (prog, diags) = adapt(&format!(
        "{XSD}ex:Bad owl:equivalentClass [ a rdfs:Datatype ;
            owl:withRestrictions ( [ xsd:minInclusive \"0.0\"^^xsd:decimal ] ) ] ."
    ));
    assert!(
        diags.iter().any(|d| d.code == "MALFORMED_DATARANGE"),
        "a datarange with no onDatatype must surface a MALFORMED_DATARANGE diagnostic: {diags:?}"
    );
    assert!(
        !prog
            .axioms
            .iter()
            .any(|a| a.obj.starts_with(&logic("datarange/"))),
        "a malformed datarange must not lift a skolem datarange node"
    );
}

// ── IR isomorphism gate ──────────────────────────────────────────────────────

#[test]
fn isomorphic_identical_programs() {
    let a = adapt("ex:Person a gufo:Kind .").0;
    let b = adapt("ex:Person a gufo:Kind .").0;
    assert!(assert_ir_isomorphic(&a, &b).is_ok());
}

#[test]
fn isomorphic_divergent_programs_raise() {
    let a = adapt("ex:Person a gufo:Kind .").0;
    let b = adapt("ex:Person a gufo:Role .").0;
    let err = assert_ir_isomorphic(&a, &b).unwrap_err();
    assert!(err.0.contains("isomorphism gate FAILED"));
    assert!(err.0.contains("A has, B lacks (axiom)"));
    assert!(err.0.contains("B has, A lacks (axiom)"));
}

#[test]
fn isomorphic_order_independent() {
    let a = adapt("ex:P a gufo:Kind . ex:Q a gufo:Role .").0;
    let b = adapt("ex:Q a gufo:Role . ex:P a gufo:Kind .").0;
    assert!(assert_ir_isomorphic(&a, &b).is_ok());
}

// ── Round-trip: logic: ≡ owl/gufo ────────────────────────────────────────────

#[test]
fn roundtrip_gufo_kind_equals_logic_kind() {
    let prog_logic = logic_prog("ex:Person a logic:Kind .");
    let prog_gufo = adapt("ex:Person a gufo:Kind .").0;
    assert_eq!(prog_logic.axioms.len(), 1);
    assert_eq!(prog_gufo.axioms.len(), 1);
    assert!(assert_ir_isomorphic(&prog_logic, &prog_gufo).is_ok());
}

#[test]
fn roundtrip_owl_subclassof_equals_logic_subclassof() {
    let prog_logic = logic_prog("ex:Employee logic:subClassOf ex:Person .");
    let prog_owl = adapt("ex:Employee rdfs:subClassOf ex:Person .").0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_owl_transitive_property() {
    let prog_logic = logic_prog("ex:p a logic:transitiveProperty .");
    let prog_owl = adapt("ex:p a owl:TransitiveProperty .").0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_ok());
}

#[test]
fn roundtrip_divergent_pair_raises() {
    let prog_logic = logic_prog("ex:Person a logic:Kind .");
    let prog_owl = adapt("ex:Person a gufo:Role .").0;
    assert!(assert_ir_isomorphic(&prog_logic, &prog_owl).is_err());
}

// ── source_iri ───────────────────────────────────────────────────────────────

#[test]
fn adapt_source_iri_stored() {
    let (prog, _) = adapt_legacy_str(
        &format!("{PREFIXES}ex:P a gufo:Kind ."),
        Some("urn:src".to_owned()),
    )
    .unwrap();
    assert_eq!(prog.source_iri.as_deref(), Some("urn:src"));
}
