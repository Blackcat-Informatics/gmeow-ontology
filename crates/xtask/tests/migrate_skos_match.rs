// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration coverage for the `xtask migrate-skos-match` engine (issue #1200 Task 8):
//! the deterministic align\* → native RDF-1.2 `skos:*Match` rewrite with its per-file
//! refuse-to-write self-check (field round-trip + SSSOM byte-identity). The fixtures use
//! verbatim real cell shapes (an ordinary `accounts` slice with a `gmeow:setComment`
//! prose block and several cells; a full grounding cell with every `logic:` field).

use gmeow_logic_compile::migrate_skos_match::migrate_turtle_source;

/// (a) A representative ordinary `equivalences.ttl`: a header comment, prefixes, a
/// `gmeow:MappingSet` with a `gmeow:setComment` prose block, and two `skos:*Match` cells
/// (one multi-line, one single-line — the real `eqProperties092` layout).
const SLICE_SOURCE: &str = r#"# Term equivalences for the accounts slice (authored in-slice).
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

gmeow:mapsetAccounts a gmeow:MappingSet ;
    gmeow:sssomFile "gmeow-classes.sssom.tsv" ;
    gmeow:setId "https://blackcatinformatics.ca/gmeow/mappings/accounts" ;
    gmeow:license "https://creativecommons.org/licenses/by/4.0/" ;
    gmeow:setComment "A long prose block. It must survive byte for byte, periods and all." .

gmeow:eqClasses013 a gmeow:TermEquivalence ;
    gmeow:alignSubject gmeow:OnlineAccount ;
    gmeow:alignPredicate skos:exactMatch ;
    gmeow:alignObject foaf:OnlineAccount ;
    gmeow:justification semapv:ManualMappingCuration ;
    gmeow:confidence 1.0 ;
    gmeow:sssomFile "gmeow-classes.sssom.tsv" .

gmeow:eqProperties092 a gmeow:TermEquivalence ;
    gmeow:alignSubject gmeow:accountServiceHomepage ; gmeow:alignPredicate skos:closeMatch ; gmeow:alignObject foaf:accountServiceHomepage ;
    gmeow:justification semapv:ManualMappingCuration ; gmeow:confidence 0.95 ; gmeow:comment "FOAF's OnlineAccount service-homepage idiom" ; gmeow:sssomFile "gmeow-properties.sssom.tsv" .
"#;

/// Golden for (a): only the two cell statements change; the header, prefixes, and the
/// `gmeow:MappingSet` prose block are byte-for-byte identical.
const SLICE_GOLDEN: &str = r#"# Term equivalences for the accounts slice (authored in-slice).
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

gmeow:mapsetAccounts a gmeow:MappingSet ;
    gmeow:sssomFile "gmeow-classes.sssom.tsv" ;
    gmeow:setId "https://blackcatinformatics.ca/gmeow/mappings/accounts" ;
    gmeow:license "https://creativecommons.org/licenses/by/4.0/" ;
    gmeow:setComment "A long prose block. It must survive byte for byte, periods and all." .

gmeow:OnlineAccount skos:exactMatch foaf:OnlineAccount {|
    gmeow:sssomFile "gmeow-classes.sssom.tsv" ;
    gmeow:justification semapv:ManualMappingCuration ;
    gmeow:confidence 1.0
|} .

gmeow:accountServiceHomepage skos:closeMatch foaf:accountServiceHomepage {|
    gmeow:sssomFile "gmeow-properties.sssom.tsv" ;
    gmeow:justification semapv:ManualMappingCuration ;
    gmeow:confidence 0.95 ;
    gmeow:comment "FOAF's OnlineAccount service-homepage idiom"
|} .
"#;

/// (b) A full grounding cell (verbatim `foundation-bridges.ttl` shape) carrying every
/// `logic:` field: endpoints, morphism class/kind, preservation, and the grounding type.
const GROUNDING_SOURCE: &str = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix dul: <http://www.ontologydesignpatterns.org/ont/dul/DUL.owl#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .

gmeow:mapsetLogicDul a gmeow:MappingSet ;
    gmeow:sssomFile "gmeow-logic-dul.sssom.tsv" ;
    gmeow:setId "https://blackcatinformatics.ca/gmeow/mappings/logic-dul" ;
    gmeow:license "https://creativecommons.org/licenses/by/4.0/" .

gmeow:eqLogicDulIndividual a gmeow:TermEquivalence, logic:GroundingCorrespondence ;
    gmeow:alignSubject logic:Individual ; gmeow:alignPredicate skos:closeMatch ;
    gmeow:alignObject dul:Entity ;
    logic:sourceEndpoint logic:Individual ; logic:targetEndpoint dul:Entity ;
    gmeow:justification semapv:ManualMappingCuration ; gmeow:confidence 0.85 ;
    gmeow:sssomFile "gmeow-logic-dul.sssom.tsv" ;
    gmeow:comment "Both are top-level entity categories, but their partitions and identity commitments differ." ;
    logic:morphismClass logic:BridgeView ; logic:morphismKind logic:CommitmentShiftingBridge ;
    logic:preservationKind logic:ValidationOnly .
"#;

/// Golden for (b): the grounding type is emitted first, then the SSSOM fields, then the
/// `logic:` endpoint/class/kind/preservation fields, in the fixed order.
const GROUNDING_GOLDEN: &str = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix dul: <http://www.ontologydesignpatterns.org/ont/dul/DUL.owl#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .

gmeow:mapsetLogicDul a gmeow:MappingSet ;
    gmeow:sssomFile "gmeow-logic-dul.sssom.tsv" ;
    gmeow:setId "https://blackcatinformatics.ca/gmeow/mappings/logic-dul" ;
    gmeow:license "https://creativecommons.org/licenses/by/4.0/" .

logic:Individual skos:closeMatch dul:Entity {|
    a logic:GroundingCorrespondence ;
    gmeow:sssomFile "gmeow-logic-dul.sssom.tsv" ;
    gmeow:justification semapv:ManualMappingCuration ;
    gmeow:confidence 0.85 ;
    gmeow:comment "Both are top-level entity categories, but their partitions and identity commitments differ." ;
    logic:sourceEndpoint logic:Individual ;
    logic:targetEndpoint dul:Entity ;
    logic:morphismClass logic:BridgeView ;
    logic:morphismKind logic:CommitmentShiftingBridge ;
    logic:preservationKind logic:ValidationOnly
|} .
"#;

#[test]
fn ordinary_slice_migrates_cells_and_preserves_prose_byte_for_byte() {
    let result = migrate_turtle_source(SLICE_SOURCE).expect("self-check passes → migration ok");
    assert_eq!(result.cells_migrated, 2);
    // Golden: the migrated output is exactly this (proves the cells became native form AND
    // that every other byte survived; success itself proves the field round-trip and the
    // SSSOM byte-identity gate both passed).
    assert_eq!(result.rewritten, SLICE_GOLDEN);
    // The `gmeow:setComment` prose block and the prefixes are present unchanged.
    assert!(result.rewritten.contains(
        "gmeow:setComment \"A long prose block. It must survive byte for byte, periods and all.\" ."
    ));
    assert!(
        result
            .rewritten
            .contains("@prefix foaf: <http://xmlns.com/foaf/0.1/> .")
    );
}

#[test]
fn migrated_output_is_a_fixed_point_no_align_cells_remain() {
    let once = migrate_turtle_source(SLICE_SOURCE).expect("first pass ok");
    // Re-running over the native form finds no align* cells and leaves the file untouched.
    let twice = migrate_turtle_source(&once.rewritten).expect("second pass ok");
    assert_eq!(twice.cells_migrated, 0);
    assert_eq!(twice.rewritten, once.rewritten);
    assert!(!twice.rewritten.contains("gmeow:alignPredicate"));
}

#[test]
fn grounding_cell_round_trips_with_all_logic_fields() {
    let result = migrate_turtle_source(GROUNDING_SOURCE).expect("grounding self-check passes → ok");
    assert_eq!(result.cells_migrated, 1);
    assert_eq!(result.rewritten, GROUNDING_GOLDEN);
    assert!(
        result
            .rewritten
            .contains("a logic:GroundingCorrespondence ;")
    );
    assert!(
        result
            .rewritten
            .contains("logic:preservationKind logic:ValidationOnly")
    );
}

#[test]
fn non_skos_predicate_cell_aborts_the_file() {
    // An `owl:equivalentClass` legacy cell is not consumable by the native reader (only
    // the five `skos:*Match` predicates are), so its file self-check fails and the tool
    // ABORTS rather than silently drop or mis-record the alignment fact. This proves the
    // refuse-to-write gate has teeth.
    let source = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .

gmeow:eqClasses013 a gmeow:TermEquivalence ;
    gmeow:alignSubject gmeow:OnlineAccount ;
    gmeow:alignPredicate owl:equivalentClass ;
    gmeow:alignObject foaf:OnlineAccount ;
    gmeow:justification semapv:ManualMappingCuration ;
    gmeow:confidence 1.0 ;
    gmeow:sssomFile "gmeow-classes.sssom.tsv" .
"#;
    let error = migrate_turtle_source(source).expect_err("non-skos predicate must abort");
    assert!(
        error.message().contains("field round-trip"),
        "abort should name the failing gate: {}",
        error.message()
    );
}

#[test]
fn file_without_cells_is_returned_unchanged() {
    let source = r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:mapsetOnly a gmeow:MappingSet ;
    gmeow:sssomFile "gmeow-x.sssom.tsv" .
"#;
    let result = migrate_turtle_source(source).expect("no-cell file is a clean no-op");
    assert_eq!(result.cells_migrated, 0);
    assert_eq!(result.rewritten, source);
}
