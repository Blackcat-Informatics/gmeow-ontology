// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Annotation-lift tests for the canonical `logic:` front-end ([`parse_logic_str`]).
//!
//! These exercise the RDFS/SKOS annotation surface — the carrier-tagged
//! `NodeKind::Annotation` lift, the foreign-subject skip, and the non-carrier-tag skip
//! — through the `logic:` authoring path.

use crate::frontend::parse_logic_str;
use crate::ir::{LogicProgram, NodeKind};

const PREFIXES: &str = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gufo:  <http://purl.org/nemo/gufo#> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix gm:    <https://blackcatinformatics.ca/gmeow/> .
";

fn logic_prog(ttl: &str) -> LogicProgram {
    parse_logic_str(&format!("{PREFIXES}{ttl}"), None)
        .expect("parse ok")
        .0
}

// ── RDFS/SKOS annotation lift (R1/R2) ────────────────────────────────────────

const ANNOTATED_TERM: &str = r#"
gm:Widget rdfs:label "Widget"@x-gmeow-english ;
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
        let want_load_bearing =
            a.predicate.ends_with("#comment") || a.predicate.ends_with("core#definition");
        assert_eq!(
            a.load_bearing, want_load_bearing,
            "load_bearing bit for predicate {}",
            a.predicate
        );
        assert!(a.obj_is_literal, "annotation object is a literal");
    }
}

#[test]
fn annotation_lift_ignores_foreign_subject_labels() {
    // A foreign alignment-target / example subject (schema.org) carries its own @en label:
    // that is the external vocabulary's metadata, NOT GMEOW's annotation surface. It must be
    // neither lifted nor carrier-checked (no diagnostic).
    let (prog, diags) = parse_logic_str(
        &format!(
            "{PREFIXES}@prefix schema: <https://schema.org/> .\n\
             schema:Thing rdfs:label \"Thing\"@en ; skos:definition \"An external thing.\"@en ."
        ),
        None,
    )
    .expect("parse ok");
    assert!(
        prog.axioms
            .iter()
            .all(|a| a.node_kind != NodeKind::Annotation),
        "a foreign subject's label must NOT be lifted as a GMEOW annotation"
    );
    assert!(
        !diags
            .iter()
            .any(|d| d.code == "NON_CARRIER_ANNOTATION_LANG"),
        "a foreign @en label is not a carrier-discipline violation"
    );
}

#[test]
fn annotation_lift_skips_non_carrier_tagged_annotations() {
    // Only the internal @x-gmeow-english carrier surface is lifted. A non-carrier tag (an @en
    // example/demonstration label — the compile-logic corpus carries example/test subjects too)
    // is NOT the carrier surface: skipped, never lifted and never a hard error here. The
    // authoritative fail-closed carrier-discipline guard is the structural lint (validate-gts),
    // scoped to the shipped core-term graphs. (R2/AC2)
    let (prog, diags) = parse_logic_str(
        &format!("{PREFIXES}\ngm:Bad rdfs:label \"Widget\"@en ."),
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
        !diags
            .iter()
            .any(|d| d.code == "NON_CARRIER_ANNOTATION_LANG"),
        "a non-carrier tag is skipped, not a hard lift error (the lint is the guard)"
    );
}
