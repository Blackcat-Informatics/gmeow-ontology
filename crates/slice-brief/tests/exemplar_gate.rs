// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The SHACL per-term conformance gate on exemplar tiering: exemplars
//! must be same-slice coats **that passed the validation gate**, not merely coats with
//! a full source-only completeness count. These tests drive the SINGLE canonical shared
//! tiering [`gmeow_slice_brief::exemplar_tiers`] over a bespoke temp slice against a
//! bespoke shape set, so the gate — not a proxy — decides eligibility.
//!
//! Fixture: three `ex:Widget` terms defined by one slice.
//! * `ex:Alpha` — a FULL coat (all six coat predicates) and a conforming `ex:size` (an
//!   `xsd:integer`), so it passes the gate.
//! * `ex:Bravo` — a FULL coat too, but `ex:size` is a string, which VIOLATES the shape's
//!   `sh:datatype xsd:integer`. Despite the full coat it must be rank 0 (ineligible).
//! * `ex:Charlie` — a SPARSER conforming coat (label + definition only), so it is
//!   eligible but ranks below `ex:Alpha`.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_slice_brief::{BriefInputs, ShapeUnion, assemble_packet, exemplar_tiers};

const ALPHA: &str = "https://example.org/Alpha";
const BRAVO: &str = "https://example.org/Bravo";
const CHARLIE: &str = "https://example.org/Charlie";

/// A bespoke NodeShape requiring every `ex:Widget`'s `ex:size` to be an `xsd:integer`.
/// `ex:Bravo`'s string value violates it; the others conform.
const SHAPES_TTL: &str = r#"
@prefix sh:   <http://www.w3.org/ns/shacl#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:   <https://example.org/> .

ex:WidgetShape a sh:NodeShape ;
    sh:targetClass ex:Widget ;
    sh:property [
        sh:path ex:size ;
        sh:datatype xsd:integer ;
        sh:minCount 1 ;
    ] .
"#;

/// The bespoke slice module: three widgets, all defined by `ex:TheSlice`. `ex:Alpha`
/// and `ex:Bravo` carry a full six-predicate coat; `ex:Charlie` carries only two coat
/// predicates. `ex:Bravo`'s `ex:size` is a bare string (an `xsd:string`), which the
/// shape rejects.
const MODULE_TTL: &str = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:Alpha a ex:Widget ;
    rdfs:isDefinedBy ex:TheSlice ;
    ex:size 5 ;
    rdfs:label "Alpha" ;
    skos:definition "the alpha widget" ;
    skos:example "an alpha example" ;
    gmeow:useWhen "use alpha when" ;
    gmeow:avoidWhen "avoid alpha when" ;
    gmeow:howToUse "how to use alpha" .

ex:Bravo a ex:Widget ;
    rdfs:isDefinedBy ex:TheSlice ;
    ex:size "big" ;
    rdfs:label "Bravo" ;
    skos:definition "the bravo widget" ;
    skos:example "a bravo example" ;
    gmeow:useWhen "use bravo when" ;
    gmeow:avoidWhen "avoid bravo when" ;
    gmeow:howToUse "how to use bravo" .

ex:Charlie a ex:Widget ;
    rdfs:isDefinedBy ex:TheSlice ;
    ex:size 7 ;
    rdfs:label "Charlie" ;
    skos:definition "the charlie widget" .
"#;

/// The bespoke manifest declaring the slice identity (NOT part of the slice graph).
const MANIFEST_TTL: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/> .

ex:TheSlice a gmeow:Slice .
"#;

/// Materialize the bespoke slice under `dir` (manifest.ttl + module.ttl).
fn write_slice(dir: &Path) {
    std::fs::write(dir.join("manifest.ttl"), MANIFEST_TTL).expect("write manifest.ttl");
    std::fs::write(dir.join("module.ttl"), MODULE_TTL).expect("write module.ttl");
}

/// Parse the bespoke shape set into the shared [`ShapeUnion`] type the gate consumes.
fn shapes() -> ShapeUnion {
    purrdf::shapes::engine::parse_shapes(SHAPES_TTL).expect("parse bespoke shapes")
}

/// The gate excludes a full-coat term that VIOLATES a shape (rank 0), and ranks the
/// surviving conforming terms by coat completeness — proving eligibility is the SHACL
/// verdict, not the source-only completeness count.
#[test]
fn shacl_violation_excludes_a_full_coat_term_and_completeness_orders_the_rest() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_slice(temp.path());
    let shapes = shapes();

    let tiers: BTreeMap<String, i64> =
        exemplar_tiers(temp.path(), &shapes).expect("gate the bespoke slice");

    // ex:Bravo has a FULL coat (completeness 6) but violates sh:datatype → rank 0.
    assert_eq!(
        tiers.get(BRAVO).copied(),
        Some(0),
        "a full-coat term with a SHACL violation must be rank 0 (ineligible), got {:?}",
        tiers.get(BRAVO)
    );
    // ex:Alpha conforms with a full coat → rank 6.
    assert_eq!(
        tiers.get(ALPHA).copied(),
        Some(6),
        "a conforming full coat must rank at its completeness count (6), got {:?}",
        tiers.get(ALPHA)
    );
    // ex:Charlie conforms with a sparse coat (label + definition) → rank 2.
    assert_eq!(
        tiers.get(CHARLIE).copied(),
        Some(2),
        "a conforming sparse coat must rank at its completeness count (2), got {:?}",
        tiers.get(CHARLIE)
    );
    // The fuller conforming coat strictly outranks the sparser conforming coat.
    assert!(
        tiers[ALPHA] > tiers[CHARLIE],
        "a fuller conforming coat must outrank a sparser conforming coat"
    );
}

/// End-to-end selection: `assemble_packet` fed the gated tiers surfaces ONLY the
/// conforming terms, ordered `(rank desc, IRI asc)`, and NEVER the violating full-coat
/// term — even though `ex:Bravo`'s coat is as complete as `ex:Alpha`'s.
#[test]
fn assemble_selects_conforming_exemplars_and_never_the_violating_one() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_slice(temp.path());
    let shapes = shapes();

    let tiers = exemplar_tiers(temp.path(), &shapes).expect("gate the bespoke slice");
    let packet = assemble_packet(&BriefInputs {
        slice_dir: temp.path(),
        axis: None,
        batch: None,
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    })
    .expect("assemble the bespoke packet");

    assert_eq!(
        packet.exemplars,
        vec![ALPHA.to_string(), CHARLIE.to_string()],
        "only the conforming terms are exemplars, ordered by (rank desc, IRI asc)"
    );
    assert!(
        !packet.exemplars.contains(&BRAVO.to_string()),
        "the full-coat but SHACL-violating term must never be surfaced as an exemplar"
    );
    // Two eligible against a target of three: the shortfall is recorded, never faked.
    assert_eq!(
        packet.exemplar_shortfall, 1,
        "with two eligible exemplars and a target of three, the shortfall is 1"
    );
}
