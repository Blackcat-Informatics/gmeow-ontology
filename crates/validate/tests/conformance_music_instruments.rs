// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from
//! slices/extensions/music/tests/test_music_instruments.py (whole file; the
//! Python file is deleted).
//!
//! The 8 asserted-TBox guards run over the slice `module.ttl`
//! (`GraphStore::parse_ttl_file`); the 5 SHACL guards build the inline instance
//! graphs the Python assembled via `g.add(...)` and validate them against the
//! whole shapes corpus (`Case::inline`), reproducing the honest in-graph ABox
//! completion of the referenced `tuningSystem12EDO` / modification individuals.
//!
//! source -> dest map:
//!   test_configuration_properties_exist                              -> configuration_properties_functional
//!   test_participation_instrument_item_ranges_over_entity           -> participation_instrument_item_ranges_over_entity
//!   test_instrument_type_seeds_exist                                 -> instrument_type_seeds_exist
//!   test_instrument_type_hs_numbers                                  -> instrument_type_hs_numbers
//!   test_instrument_type_mimo_matches                                -> instrument_type_mimo_matches
//!   test_instrument_modification_seeds_exist                         -> instrument_modification_seeds_exist
//!   test_playing_technique_seeds_exist                               -> playing_technique_seeds_exist
//!   test_configuration_fixtures_exist                                -> configuration_fixtures_exist
//!   test_instrument_configuration_valid_with_type_passes_shacl       -> case::configuration_valid_with_type_passes
//!   test_instrument_configuration_valid_with_item_passes_shacl       -> case::configuration_valid_with_item_passes
//!   test_instrument_configuration_missing_target_fails_shacl         -> case::configuration_missing_target_fails
//!   test_instrument_configuration_two_intervals_fails_shacl          -> case::configuration_two_intervals_fails
//!   test_instrument_configuration_compound_modification_passes_shacl -> case::configuration_compound_modification_passes

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const SKOS_EXACTMATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const MIMO: &str = "http://www.mimo-db.eu/InstrumentsKeywords/";
const MUSIC_MODULE: &str = "slices/extensions/music/module.ttl";

fn g(local: &str) -> String {
    format!("{G}{local}")
}
fn module() -> &'static GraphStore {
    static STORE: std::sync::OnceLock<GraphStore> = std::sync::OnceLock::new();
    STORE.get_or_init(|| GraphStore::parse_ttl_file(&repo_root().join(MUSIC_MODULE)))
}

// ── Asserted-TBox guards (slice module) ───────────────────────────────────────

#[test]
fn configuration_properties_functional() {
    // Functionality is now carried by the canonical `logic:` characteristic records
    // (in the logic slice), not by a local `owl:FunctionalProperty` marker on the music
    // module — so this asserts over the merged ontology, not `module()`.
    let s = GraphStore::ontology();
    for prop in [
        "configurationOf",
        "configurationInstrumentType",
        "configurationTuningFrame",
        "configurationInterval",
    ] {
        assert!(
            s.is_functional_carrier(&g(prop)),
            "{prop} should carry a logic: functionalProperty characteristic"
        );
    }
    // Modification is deliberately non-functional to allow compound modifications.
    assert!(
        !s.is_functional_carrier(&g("configurationModification")),
        "configurationModification must not be functional"
    );
}

#[test]
fn participation_instrument_item_ranges_over_entity() {
    let s = module();
    let prop = g("participationInstrumentItem");
    assert!(s.has(Some(&prop), Some(RDFS_RANGE), Some(&g("Entity"))));
    assert!(!s.has(Some(&prop), Some(RDFS_RANGE), Some(&g("Item"))));
}

#[test]
fn instrument_type_seeds_exist() {
    let s = module();
    for term in [
        "instrumentTypePiano",
        "instrumentTypeViolin",
        "instrumentTypeDoubleBass",
        "instrumentTypeDrumKit",
        "instrumentTypeElectricGuitar",
        "instrumentTypeVoice",
        "instrumentTypeSitar",
        "instrumentTypeTabla",
        "instrumentTypeModularSynth",
        "instrumentTypeTurntables",
        "instrumentTypeAdaptedGuitar",
        "instrumentTypeGamelan",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("InstrumentType"))),
            "{term} should be an InstrumentType"
        );
    }
}

#[test]
fn instrument_type_hs_numbers() {
    let s = module();
    for (term, hs) in [
        ("instrumentTypePiano", "314.122-4-8"),
        ("instrumentTypeElectricGuitar", "321.322-6"),
    ] {
        assert!(
            s.has_literal(&g(term), &g("hsNumber"), hs, XSD_STRING),
            "{term} should have hsNumber {hs}"
        );
    }
}

#[test]
fn instrument_type_mimo_matches() {
    let s = module();
    for (term, mimo_id) in [
        ("instrumentTypePiano", "2299"),
        ("instrumentTypeElectricGuitar", "3236"),
        ("instrumentTypeSitar", "3456"),
        ("instrumentTypeTabla", "2899"),
        ("instrumentTypeGamelan", "2805"),
    ] {
        let expected = format!("{MIMO}{mimo_id}");
        assert!(
            s.has(Some(&g(term)), Some(SKOS_EXACTMATCH), Some(&expected)),
            "{term} should exactMatch {expected}"
        );
    }
}

#[test]
fn instrument_modification_seeds_exist() {
    let s = module();
    for term in [
        "instrumentModificationPrepared",
        "instrumentModificationScordatura",
        "instrumentModificationCapo",
        "instrumentModificationMute",
        "instrumentModificationElectrified",
        "instrumentModificationExtendedRange",
    ] {
        assert!(
            s.has(
                Some(&g(term)),
                Some(RDF_TYPE),
                Some(&g("InstrumentModification"))
            ),
            "{term} should be an InstrumentModification"
        );
    }
}

#[test]
fn playing_technique_seeds_exist() {
    let s = module();
    for term in [
        "playingTechniqueArco",
        "playingTechniquePizzicato",
        "playingTechniqueColLegno",
        "playingTechniquePreparedPiano",
        "playingTechniqueMultiphonics",
        "playingTechniqueTapping",
        "playingTechniqueSlap",
        "playingTechniqueGrowl",
        "playingTechniqueKonnakol",
        "playingTechniqueBentNote",
        "playingTechniqueHarmonics",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("PlayingTechnique"))),
            "{term} should be a PlayingTechnique"
        );
    }
}

#[test]
fn configuration_fixtures_exist() {
    let s = module();
    for fixture in [
        "fixturePreparedPianoConfiguration",
        "fixtureDropDGuitarConfiguration",
        "fixture1959LesPaulConfiguration",
    ] {
        assert!(s.has(
            Some(&g(fixture)),
            Some(RDF_TYPE),
            Some(&g("InstrumentConfiguration"))
        ));
    }
    assert!(s.has(
        Some(&g("fixturePreparedPianoConfiguration")),
        Some(&g("configurationModification")),
        Some(&g("instrumentModificationPrepared"))
    ));
    assert!(s.has(
        Some(&g("fixtureDropDGuitarConfiguration")),
        Some(&g("configurationModification")),
        Some(&g("instrumentModificationScordatura"))
    ));
    assert!(s.has(
        Some(&g("fixtureDropDGuitarConfiguration")),
        Some(&g("configurationInterval")),
        Some(&g("pitchIntervalMajorSecondDown"))
    ));
    assert!(s.has(
        Some(&g("fixture1959LesPaulConfiguration")),
        Some(&g("configurationOf")),
        Some(&g("fixture1959LesPaul"))
    ));
}

// ── SHACL guards (whole shapes corpus, inline fixtures) ───────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test-music-instruments/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

/// The canonical `tuningSystem12EDO` frame descriptors restated in-graph (honest
/// ABox completion, values verbatim from module.ttl) so the generated
/// `TuningSystemShape` range shape is satisfied without loading the merged
/// ontology.
const EDO12: &str = "\
gmeow:tuningSystem12EDO a gmeow:TuningSystem .
gmeow:tuningSystem12EDO gmeow:tuningKind gmeow:tuningSystemKindEqualDivision .
gmeow:tuningSystem12EDO gmeow:frameRealm gmeow:frameRealmMusicalPitch .
gmeow:tuningSystem12EDO gmeow:frameKind gmeow:frameKindScalar .
gmeow:tuningSystem12EDO gmeow:requiresHost \"false\"^^xsd:boolean .
";

#[rstest]
#[case::configuration_valid_with_type_passes(Case::inline(format!(
    "{PREFIXES}\
ex:configValid a gmeow:InstrumentConfiguration .
ex:configValid gmeow:configurationInstrumentType gmeow:instrumentTypePiano .
ex:configValid gmeow:configurationModification gmeow:instrumentModificationPrepared .
ex:configValid gmeow:configurationTuningFrame gmeow:tuningSystem12EDO .
gmeow:instrumentModificationPrepared a gmeow:InstrumentModification .
{EDO12}"
)))]
#[case::configuration_valid_with_item_passes(Case::inline(format!(
    "{PREFIXES}\
ex:configValidItem a gmeow:InstrumentConfiguration .
ex:configValidItem gmeow:configurationOf ex:myInstrument .
ex:configValidItem gmeow:configurationModification gmeow:instrumentModificationCapo .
ex:configValidItem gmeow:configurationTuningFrame gmeow:tuningSystem12EDO .
ex:configValidItem gmeow:configurationInterval gmeow:pitchIntervalMajorSecondDown .
gmeow:instrumentModificationCapo a gmeow:InstrumentModification .
{EDO12}"
)))]
#[case::configuration_missing_target_fails(Case::inline(format!(
    "{PREFIXES}\
ex:configBadTarget a gmeow:InstrumentConfiguration .
ex:configBadTarget gmeow:configurationModification gmeow:instrumentModificationPrepared .
"
)).fails().violations(&["exactly one of: a specific instrument item"]))]
#[case::configuration_two_intervals_fails(Case::inline(format!(
    "{PREFIXES}\
ex:configBadInterval a gmeow:InstrumentConfiguration .
ex:configBadInterval gmeow:configurationInstrumentType gmeow:instrumentTypeElectricGuitar .
ex:configBadInterval gmeow:configurationModification gmeow:instrumentModificationScordatura .
ex:configBadInterval gmeow:configurationInterval gmeow:pitchIntervalMajorSecondDown .
ex:configBadInterval gmeow:configurationInterval gmeow:pitchIntervalPerfectFifth .
"
)).fails().violations(&["At most one interval"]))]
#[case::configuration_compound_modification_passes(Case::inline(format!(
    "{PREFIXES}\
ex:configCompound a gmeow:InstrumentConfiguration .
ex:configCompound gmeow:configurationInstrumentType gmeow:instrumentTypeElectricGuitar .
ex:configCompound gmeow:configurationModification gmeow:instrumentModificationMute .
ex:configCompound gmeow:configurationModification gmeow:instrumentModificationElectrified .
ex:configCompound gmeow:configurationTuningFrame gmeow:tuningSystem12EDO .
gmeow:instrumentModificationMute a gmeow:InstrumentModification .
gmeow:instrumentModificationElectrified a gmeow:InstrumentModification .
{EDO12}"
)))]
fn instrument_shacl(#[case] case: Case) {
    case.run();
}
