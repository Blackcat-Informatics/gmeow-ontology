// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single list encoding: lowering, minting, defects, and the frontend/projection
//! round trip for named and anonymous owners.

use super::*;
use crate::frontend::{Diagnostic, Severity, parse_logic_dataset, parse_logic_str};
use crate::ir::LogicProgram;
use crate::projections::rdf;

const PREFIXES: &str = "\
@prefix ex:    <https://example.org/lists/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";

fn ex(local: &str) -> String {
    format!("https://example.org/lists/{local}")
}

fn logic(local: &str) -> String {
    format!("{LOGIC_NAMESPACE}{local}")
}

fn compile(ttl: &str) -> (LogicProgram, Vec<Diagnostic>) {
    parse_logic_str(&format!("{PREFIXES}{ttl}"), None).expect("parse ok")
}

fn objects_of<'a>(
    program: &'a LogicProgram,
    subject: &str,
    predicate: &str,
) -> Vec<&'a AtomicTerm> {
    program
        .axioms
        .iter()
        .filter(|axiom| axiom.subject == subject && axiom.predicate == predicate)
        .map(|axiom| &axiom.obj)
        .collect()
}

/// The single object of `subject predicate ?o`, which must be an IRI.
fn only_iri(program: &LogicProgram, subject: &str, predicate: &str) -> String {
    let objects = objects_of(program, subject, predicate);
    assert_eq!(
        objects.len(),
        1,
        "<{subject}> <{predicate}> must have exactly one object: {objects:?}"
    );
    objects[0]
        .as_iri()
        .unwrap_or_else(|| panic!("<{subject}> <{predicate}> object is not an IRI"))
        .to_owned()
}

/// Walk an IR list from `head`, asserting each cell carries exactly one first/rest.
fn ir_list(program: &LogicProgram, head: &str) -> Vec<AtomicTerm> {
    let mut members = Vec::new();
    let mut cursor = head.to_owned();
    let mut seen = BTreeSet::new();
    while cursor != RDF_NIL {
        assert!(seen.insert(cursor.clone()), "IR list at <{head}> is cyclic");
        let first = objects_of(program, &cursor, RDF_FIRST);
        let rest = objects_of(program, &cursor, RDF_REST);
        assert_eq!(
            first.len(),
            1,
            "cell <{cursor}> needs exactly one rdf:first"
        );
        assert_eq!(rest.len(), 1, "cell <{cursor}> needs exactly one rdf:rest");
        members.push(first[0].clone());
        cursor = rest[0].as_iri().expect("rdf:rest is a resource").to_owned();
    }
    members
}

fn iris(locals: &[&str]) -> Vec<AtomicTerm> {
    locals
        .iter()
        .map(|local| AtomicTerm::Iri(ex(local)))
        .collect()
}

fn no_blank_terms(program: &LogicProgram) {
    for axiom in &program.axioms {
        assert!(
            !matches!(axiom.obj, AtomicTerm::Blank(_)),
            "no IR axiom may reference a dangling blank node: {axiom:?}"
        );
        assert!(
            crate::ir::atomic::is_absolute_iri(&axiom.subject),
            "no IR axiom may have a blank subject: {axiom:?}"
        );
    }
}

/// The four shapes the issue measured, in one source.
const ALL_SHAPES: &str = "
ex:Named logic:oneOf ( ex:b ex:a ex:b ) .
ex:Anon logic:equivalentClass [ a logic:Class ; logic:oneOf ( ex:a ex:b ) ] .
ex:Either logic:equivalentClass [ a logic:Class ; logic:unionOf ( ex:Left ex:Right ) ] .
[ a logic:AllDisjointClasses ; logic:members ( ex:Left ex:Right ex:Middle ) ] .
";

#[test]
fn every_list_shape_reaches_the_ir_with_complete_cells() {
    let (program, diagnostics) = compile(ALL_SHAPES);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    no_blank_terms(&program);

    // (a) A named enumeration keeps its IRI and owns one list; set semantics sort+dedup.
    let head = only_iri(&program, &ex("Named"), &logic("oneOf"));
    assert_eq!(ir_list(&program, &head), iris(&["a", "b"]));
    assert_eq!(head, cell_iri(&list_base(&iris(&["a", "b"])), 0));

    // (b) An anonymous enumeration is a content-addressed logic:Enumeration node.
    let anon = only_iri(&program, &ex("Anon"), &logic("equivalentClass"));
    assert!(anon.starts_with(&logic("enumeration/")), "{anon}");
    let anon_head = only_iri(&program, &anon, &logic("oneOf"));
    assert_eq!(ir_list(&program, &anon_head), iris(&["a", "b"]));
    // Identical member sets share one content-addressed list.
    assert_eq!(anon_head, head);

    // (c) An anonymous union under equivalentClass no longer vanishes.
    let union = only_iri(&program, &ex("Either"), &logic("equivalentClass"));
    assert!(union.starts_with(CLASS_EXPRESSION_PREFIX), "{union}");
    let union_head = only_iri(&program, &union, &logic("unionOf"));
    assert_eq!(ir_list(&program, &union_head), iris(&["Left", "Right"]));
    assert_eq!(
        objects_of(&program, &union, RDF_TYPE),
        vec![&AtomicTerm::Iri(logic("Class"))]
    );

    // (d) An anonymous n-ary disjointness axiom is a content-addressed resource.
    let axiom = program
        .axioms
        .iter()
        .find(|axiom| axiom.predicate == logic("members"))
        .expect("members edge")
        .subject
        .clone();
    assert!(axiom.starts_with(NARY_AXIOM_PREFIX), "{axiom}");
    assert_eq!(
        objects_of(&program, &axiom, RDF_TYPE),
        vec![&AtomicTerm::Iri(logic("AllDisjointClasses"))]
    );
    let members_head = only_iri(&program, &axiom, &logic("members"));
    assert_eq!(
        ir_list(&program, &members_head),
        iris(&["Left", "Middle", "Right"])
    );

    // No flat per-member constructor edge survives anywhere.
    for axiom in &program.axioms {
        if constructor(&axiom.predicate).is_some() {
            let head = axiom
                .obj
                .as_iri()
                .expect("constructor object is a list head");
            assert!(head.contains("/cell/"), "flat constructor edge: {axiom:?}");
        }
    }
}

#[test]
fn every_list_shape_round_trips_through_the_canonical_projection() {
    let (program, _) = compile(ALL_SHAPES);
    let canonical = rdf::project_canonical_rdf12_dataset(&program).expect("canonical");
    let (restored, diagnostics) = parse_logic_dataset(&canonical.dataset, None).expect("reparse");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    crate::adapter::assert_ir_isomorphic(&program, &restored).expect("IR round trip");
    // The cells are direct triples of the canonical projection, not compact formulas.
    let text = rdf::project_canonical_rdf12(&program).unwrap().content;
    assert!(
        text.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#first"),
        "{text}"
    );
    assert!(
        !text.contains("_:"),
        "no blank node may reach the canonical carrier:\n{text}"
    );
}

#[test]
fn owl_dl_spells_lists_with_the_ir_cells() {
    let (program, _) = compile(ALL_SHAPES);
    let union = only_iri(&program, &ex("Either"), &logic("equivalentClass"));
    let union_head = only_iri(&program, &union, &logic("unionOf"));
    let anon = only_iri(&program, &ex("Anon"), &logic("equivalentClass"));
    let anon_head = only_iri(&program, &anon, &logic("oneOf"));
    let mut loss = crate::loss_ledger::LossLedger::new();
    let dl = rdf::project_owl_dl_dataset(&program, &mut loss).unwrap();
    let has = |s: &str, p: &str, o: &str| {
        dl.dataset.owned_quads().any(|quad| {
            quad.subject == purrdf::RdfTerm::iri(s)
                && quad.predicate == p
                && quad.object == purrdf::RdfTerm::iri(o)
        })
    };
    assert!(has(
        &union,
        "http://www.w3.org/2002/07/owl#unionOf",
        &union_head
    ));
    assert!(has(&union_head, RDF_FIRST, &ex("Left")));
    assert!(has(
        &anon,
        "http://www.w3.org/2002/07/owl#oneOf",
        &anon_head
    ));
    assert!(has(
        &ex("Either"),
        "http://www.w3.org/2002/07/owl#equivalentClass",
        &union
    ));
}

#[test]
fn owl_el_drops_anonymous_class_expressions_without_dangling_anchors() {
    let (program, _) = compile(ALL_SHAPES);
    let union = only_iri(&program, &ex("Either"), &logic("equivalentClass"));
    let mut loss = crate::loss_ledger::LossLedger::new();
    let el = rdf::project_owl_el(&program, &mut loss).unwrap();
    assert!(!el.content.contains(&union), "{}", el.content);
    assert!(!el.content.contains("/cell/"), "{}", el.content);
}

#[test]
fn nested_anonymous_members_resolve_to_their_lifted_identity() {
    let (program, diagnostics) = compile(
        "ex:Either logic:subClassOf [ logic:unionOf (
            [ a logic:Restriction ; logic:onProperty ex:p ; logic:someValuesFrom ex:C ]
            [ logic:intersectionOf ( ex:D [ logic:complementOf ex:E ] ) ] ) ] .",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    no_blank_terms(&program);
    let union = only_iri(&program, &ex("Either"), &logic("subClassOf"));
    let members = ir_list(&program, &only_iri(&program, &union, &logic("unionOf")));
    assert_eq!(members.len(), 2);
    let restriction = members
        .iter()
        .filter_map(AtomicTerm::as_iri)
        .find(|iri| iri.starts_with(&logic("restriction/")))
        .expect("restriction member resolves to its skolem");
    assert_eq!(
        only_iri(&program, restriction, &logic("onProperty")),
        ex("p")
    );
    let intersection = members
        .iter()
        .filter_map(AtomicTerm::as_iri)
        .find(|iri| iri.starts_with(CLASS_EXPRESSION_PREFIX))
        .expect("intersection member resolves to its skolem");
    let parts = ir_list(
        &program,
        &only_iri(&program, intersection, &logic("intersectionOf")),
    );
    let complement = parts
        .iter()
        .filter_map(AtomicTerm::as_iri)
        .find(|iri| iri.starts_with(CLASS_EXPRESSION_PREFIX))
        .expect("complement member");
    assert_eq!(
        only_iri(&program, complement, &logic("complementOf")),
        ex("E")
    );
}

#[test]
fn identical_anonymous_expressions_share_one_identity() {
    let (program, diagnostics) = compile(
        "ex:p logic:domain [ a logic:Class ; logic:unionOf ( ex:A ex:B ) ] .
         ex:q logic:domain [ a logic:Class ; logic:unionOf ( ex:B ex:A ) ] .",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    no_blank_terms(&program);
    assert_eq!(
        only_iri(&program, &ex("p"), &logic("domain")),
        only_iri(&program, &ex("q"), &logic("domain"))
    );
}

#[test]
fn property_chains_keep_their_authored_order() {
    let (program, diagnostics) =
        compile("ex:r logic:propertyChainAxiom ( ex:second ex:first ex:second ) .");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let head = only_iri(&program, &ex("r"), &logic("propertyChainAxiom"));
    assert_eq!(
        ir_list(&program, &head),
        iris(&["second", "first", "second"])
    );
}

#[test]
fn literal_enumerations_are_admitted_and_kept_typed() {
    let (program, diagnostics) = compile("ex:Level logic:oneOf ( \"high\" \"low\" ) .");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let head = only_iri(&program, &ex("Level"), &logic("oneOf"));
    let members = ir_list(&program, &head);
    assert!(members.iter().all(AtomicTerm::is_literal), "{members:?}");
}

#[test]
fn authored_iri_cells_are_kept_verbatim() {
    let (program, diagnostics) = compile(
        "ex:r logic:propertyChainAxiom ex:c0 .
         ex:c0 rdf:first ex:p ; rdf:rest ex:c1 .
         ex:c1 rdf:first ex:q ; rdf:rest rdf:nil .",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(
        only_iri(&program, &ex("r"), &logic("propertyChainAxiom")),
        ex("c0")
    );
    assert_eq!(ir_list(&program, &ex("c0")), iris(&["p", "q"]));
}

/// A malformed list on a named owner is an error and drops the whole statement.
fn assert_named_refusal(ttl: &str, code: &str, predicate: &str) {
    let (program, diagnostics) = compile(ttl);
    let refusal: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == code)
        .collect();
    assert_eq!(refusal.len(), 1, "expected one {code}: {diagnostics:?}");
    assert_eq!(refusal[0].severity, Severity::Error, "{refusal:?}");
    assert_eq!(refusal[0].subject.as_deref(), Some(ex("X").as_str()));
    assert!(
        objects_of(&program, &ex("X"), &logic(predicate)).is_empty(),
        "a refused list must not leave a dangling constructor edge"
    );
    assert!(
        !program
            .axioms
            .iter()
            .any(|axiom| matches!(axiom.predicate.as_str(), RDF_FIRST | RDF_REST)),
        "a refused list must not leave partial cells"
    );
    no_blank_terms(&program);
}

#[test]
fn named_owner_malformed_lists_are_errors() {
    // Not nil-terminated.
    assert_named_refusal(
        "ex:X logic:oneOf _:l . _:l rdf:first ex:a .",
        "MALFORMED_ENUMERATION",
        "oneOf",
    );
    // Cyclic.
    assert_named_refusal(
        "ex:X logic:unionOf _:l . _:l rdf:first ex:a ; rdf:rest _:m . \
         _:m rdf:first ex:b ; rdf:rest _:l .",
        "MALFORMED_LIST",
        "unionOf",
    );
    // Branching member.
    assert_named_refusal(
        "ex:X logic:unionOf _:l . _:l rdf:first ex:a , ex:b ; rdf:rest rdf:nil .",
        "MALFORMED_LIST",
        "unionOf",
    );
    // Branching tail.
    assert_named_refusal(
        "ex:X logic:members _:l . _:l rdf:first ex:a ; rdf:rest rdf:nil , _:m . \
         _:m rdf:first ex:b ; rdf:rest rdf:nil .",
        "MALFORMED_LIST",
        "members",
    );
    // A hole: a cell without rdf:first.
    assert_named_refusal(
        "ex:X logic:intersectionOf _:l . _:l rdf:rest rdf:nil .",
        "MALFORMED_LIST",
        "intersectionOf",
    );
    // A literal member in a class list.
    assert_named_refusal(
        "ex:X logic:unionOf ( ex:A \"B\" ) .",
        "MALFORMED_LIST",
        "unionOf",
    );
    // A literal cell.
    assert_named_refusal(
        "ex:X logic:hasKey \"not a list\" .",
        "MALFORMED_LIST",
        "hasKey",
    );
    // An empty list.
    assert_named_refusal("ex:X logic:unionOf () .", "MALFORMED_LIST", "unionOf");
    // A resource that is not a list at all.
    assert_named_refusal(
        "ex:X logic:propertyChainAxiom ex:notAList .",
        "MALFORMED_LIST",
        "propertyChainAxiom",
    );
    // rdf:nil carrying list fields.
    assert_named_refusal(
        "ex:X logic:oneOf ( ex:a ) . rdf:nil rdf:first ex:b .",
        "MALFORMED_ENUMERATION",
        "oneOf",
    );
    // A blank member that is no lifted class expression.
    assert_named_refusal(
        "ex:X logic:unionOf ( ex:A [ ex:note \"opaque\" ] ) .",
        "UNSUPPORTED_NESTED_CLASS_EXPRESSION",
        "unionOf",
    );
}

#[test]
fn anonymous_owner_malformed_lists_are_disclosed_and_not_referenced() {
    let (program, diagnostics) = compile(
        "ex:Y logic:equivalentClass [ logic:unionOf _:l ] . _:l rdf:first ex:a .
         ex:Z logic:equivalentClass [ logic:oneOf _:m ] . _:m rdf:first ex:b .",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MALFORMED_LIST"),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MALFORMED_ENUMERATION"),
        "{diagnostics:?}"
    );
    assert!(objects_of(&program, &ex("Y"), &logic("equivalentClass")).is_empty());
    assert!(objects_of(&program, &ex("Z"), &logic("equivalentClass")).is_empty());
    no_blank_terms(&program);
}

#[test]
fn set_lists_are_minted_from_sorted_members_and_sequences_are_not() {
    let unsorted = iris(&["b", "a"]);
    let mut sorted = unsorted.clone();
    sorted.sort();
    assert_ne!(list_base(&unsorted), list_base(&sorted));
    assert_eq!(cell_iri("urn:base", 7), "urn:base/cell/0007");
    let set = constructor(&logic("unionOf")).unwrap();
    let sequence = constructor(&logic("propertyChainAxiom")).unwrap();
    assert_eq!(set.order, ListOrder::Set);
    assert_eq!(sequence.order, ListOrder::Sequence);
    assert_eq!(
        constructor(&logic("oneOf")).unwrap().members,
        ListMembers::ResourcesOrLiterals
    );
    assert!(constructor(&logic("withRestrictions")).is_none());
}

#[test]
fn every_projection_and_common_logic_dialect_carries_the_lists() {
    let (program, _) = compile(ALL_SHAPES);
    let artifacts = crate::projections::compile_program(&program, |_| Default::default())
        .expect("every projection passes its overclaim gate");
    assert!(
        artifacts.datalog.contains("first("),
        "the Datalog projection keeps the list cells:\n{}",
        artifacts.datalog
    );
    crate::cl_roundtrip::assert_all_dialects_isomorphic(&program)
        .expect("CLIF / CGIF / XCL round-trip the list encoding");
}

#[test]
fn a_self_containing_anonymous_expression_is_disclosed_once() {
    let (program, diagnostics) =
        compile("_:u logic:unionOf ( _:u ex:B ) . ex:X logic:equivalentClass _:u .");
    let cyclic: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "MALFORMED_LIST")
        .collect();
    assert_eq!(cyclic.len(), 1, "{diagnostics:?}");
    assert!(cyclic[0].message.contains("cyclic"), "{cyclic:?}");
    assert!(objects_of(&program, &ex("X"), &logic("equivalentClass")).is_empty());
    no_blank_terms(&program);
}
