// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The cross-lingual grounding-CONFLICT path of the SINGLE canonical assembly:
//! when two terms are asserted equivalent (a native alignment cell) yet carry
//! DIFFERENT translations for the same annotation predicate + language, the packet
//! records the disagreement on the affected language cell (via
//! `gmeow:groundingConflict` and `gmeow:groundingConflictWith`) — a cross-lingual
//! grounding defect surfaced for review, never silently reconciled.
//!
//! This drives the REAL [`assemble_packet`] over a bespoke temp slice whose fr
//! catalog genuinely disagrees across an equivalence, proving the disagreement
//! detection is a LIVE production path (the shipped corpus simply has no such
//! disagreement today, which is the legitimate absence case).
//!
//! Fixture: two internal `logic:` terms defined by one slice and asserted equivalent
//! via an in-slice native alignment cell, whose `fr.po` catalog gives them
//! DIFFERENT French `rdfs:label` values ("Vérité" vs "Véracité").

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_slice_brief::{BriefInputs, GroundingAttribute, assemble_packet};

const TRUTH_A: &str = "https://blackcatinformatics.ca/logic/GmeowTruthA";
const TRUTH_B: &str = "https://blackcatinformatics.ca/logic/GmeowTruthB";

/// The bespoke manifest declaring the slice identity (NOT part of the slice graph).
const MANIFEST_TTL: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

<https://blackcatinformatics.ca/gmeow/slices/fixture> a gmeow:Slice .
"#;

/// Two internal `logic:` terms, both defined by the fixture slice and each carrying
/// an English `rdfs:label` coat (so the fr/zh JOIN iterates `rdfs:label`).
const MODULE_TTL: &str = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .

logic:GmeowTruthA a owl:Class ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/fixture> ;
    rdfs:label "Truth"@en ;
    skos:definition "The truth value, spelling A." .

logic:GmeowTruthB a owl:Class ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/fixture> ;
    rdfs:label "Truth"@en ;
    skos:definition "The truth value, spelling B." .
"#;

/// The in-slice alignment linkage: an internal native alignment cell asserting the
/// two terms are equivalent — the antecedent the disagreement check reads.
const EQUIV_TTL: &str = r#"
@prefix gmeow:  <https://blackcatinformatics.ca/gmeow/> .
@prefix skos:   <http://www.w3.org/2004/02/skos/core#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .
@prefix logic:  <https://blackcatinformatics.ca/logic/> .

logic:GmeowTruthA skos:exactMatch logic:GmeowTruthB {|
    gmeow:justification semapv:ManualMappingCuration ;
    gmeow:confidence 1.0 ;
    gmeow:sssomFile "fixture.sssom.tsv"
|} .
"#;

/// The fr catalog: the two EQUIVALENT terms disagree on their French `rdfs:label`.
const FR_PO: &str = r#"# GMEOW translation catalog (slice: fixture, language: fr).
msgid ""
msgstr ""
"Project-Id-Version: gmeow\n"
"Language: fr\n"
"MIME-Version: 1.0\n"
"Content-Type: text/plain; charset=UTF-8\n"
"Content-Transfer-Encoding: 8bit\n"

msgctxt "https://blackcatinformatics.ca/logic/GmeowTruthA|rdfs:label"
msgid "Truth"
msgstr "Vérité"

msgctxt "https://blackcatinformatics.ca/logic/GmeowTruthB|rdfs:label"
msgid "Truth"
msgstr "Véracité"
"#;

/// Materialize the bespoke slice under `dir`: manifest + module + the mapping and the
/// disagreeing fr catalog the cross-lingual JOIN discovers.
fn write_slice(dir: &Path) {
    std::fs::write(dir.join("manifest.ttl"), MANIFEST_TTL).expect("write manifest.ttl");
    std::fs::write(dir.join("module.ttl"), MODULE_TTL).expect("write module.ttl");
    let mappings = dir.join("mappings");
    std::fs::create_dir_all(&mappings).expect("create mappings dir");
    std::fs::write(mappings.join("equivalences.ttl"), EQUIV_TTL).expect("write equivalences.ttl");
    let i18n = dir.join("i18n");
    std::fs::create_dir_all(&i18n).expect("create i18n dir");
    std::fs::write(i18n.join("fr.po"), FR_PO).expect("write fr.po");
}

/// Two equivalent terms whose fr `rdfs:label` translations DISAGREE make the packet
/// mark BOTH terms' fr label cells as a conflict, each naming the other as the
/// disagreeing counterpart, and materialize the disagreement in the canonical turtle.
#[test]
fn disagreeing_translations_across_an_equivalence_emit_a_grounding_conflict() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_slice(temp.path());
    let tiers: BTreeMap<String, i64> = BTreeMap::new();

    let packet = assemble_packet(&BriefInputs {
        slice_dir: temp.path(),
        axis: None,
        batch: None,
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    })
    .expect("assemble the fixture packet");

    // Both terms' fr `rdfs:label` cells are present AND flagged as a conflict, each
    // naming the OTHER equivalent term as the disagreeing counterpart.
    let cell_a = packet
        .grounding
        .iter()
        .find(|c| {
            c.term == TRUTH_A
                && c.attribute == GroundingAttribute::Fr
                && c.predicate.as_deref() == Some("rdfs:label")
        })
        .expect("a groundingFr rdfs:label cell for GmeowTruthA");
    assert!(cell_a.present, "the fr label for GmeowTruthA is present");
    assert_eq!(cell_a.value.as_deref(), Some("Vérité"));
    assert!(
        cell_a.conflict,
        "GmeowTruthA's fr label disagrees with its equivalent, so conflict must be true"
    );
    assert_eq!(
        cell_a.conflict_with.as_deref(),
        Some(TRUTH_B),
        "the conflicting counterpart is the equivalent term GmeowTruthB"
    );

    let cell_b = packet
        .grounding
        .iter()
        .find(|c| {
            c.term == TRUTH_B
                && c.attribute == GroundingAttribute::Fr
                && c.predicate.as_deref() == Some("rdfs:label")
        })
        .expect("a groundingFr rdfs:label cell for GmeowTruthB");
    assert!(
        cell_b.conflict,
        "GmeowTruthB's fr label is also in conflict"
    );
    assert_eq!(
        cell_b.conflict_with.as_deref(),
        Some(TRUTH_A),
        "the conflict is symmetric — GmeowTruthB names GmeowTruthA"
    );

    // The disagreement is MATERIALIZED in the canonical turtle (a shipped fact, not a
    // struct-only flag): the sparse encoding carries gmeow:groundingConflict and the
    // gmeow:groundingConflictWith counterpart.
    let turtle = packet.to_turtle();
    assert!(
        turtle.contains("gmeow:groundingConflict true"),
        "the sparse turtle materializes the groundingConflict flag:\n{turtle}"
    );
    assert!(
        turtle.contains("gmeow:groundingConflictWith logic:GmeowTruthB"),
        "the sparse turtle names GmeowTruthB as the disagreeing counterpart:\n{turtle}"
    );
    assert!(
        turtle.contains("gmeow:groundingConflictWith logic:GmeowTruthA"),
        "the sparse turtle names GmeowTruthA as the disagreeing counterpart:\n{turtle}"
    );

    // A control: with no disagreement there would be no such fact. Prove the flag is
    // driven by the catalog disagreement, not always-on, by counting exactly the two
    // conflict cells the fixture's single equivalence produces.
    let conflicts = packet.grounding.iter().filter(|c| c.conflict).count();
    assert_eq!(
        conflicts, 2,
        "exactly the two fr-label cells of the disagreeing equivalence are conflicts"
    );
}
