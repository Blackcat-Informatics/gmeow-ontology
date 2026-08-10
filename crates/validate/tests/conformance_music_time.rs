// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from slices/extensions/music/tests/test_music_time.py
//! (whole file; the Python file is deleted).
//!
//! The 11 asserted-TBox guards run over the slice `module.ttl`
//! (`GraphStore::parse_ttl_file`); the 10 SHACL guards build the inline instance
//! graphs the Python assembled via `g.add(...)` and validate them against the
//! whole shapes corpus (`Case::inline`), reproducing the honest in-graph ABox
//! completion of the referenced canonical `musicalTimeFrameCommon` individual.
//! Two guards (the start-denominator and meter-carrier cardinality bounds) exercise
//! a PROJECTED SHACL shape that carries no `sh:message` and is deliberately excluded
//! from that whole-shapes fixture corpus (`generated/shapes/validation-shapes.ttl`;
//! see its doc comment on `collect_generated_shapes`), so those two use
//! `Case::shape_union` (the live production shape union) and assert by path +
//! constraint component instead of message text.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTYOF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
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
fn musical_time_frame_is_reference_frame() {
    assert!(module().has(
        Some(&g("MusicalTimeFrame")),
        Some(RDFS_SUBCLASSOF),
        Some(&g("ReferenceFrame"))
    ));
}

#[test]
fn tempo_map_is_time_mapping() {
    assert!(module().has(
        Some(&g("TempoMap")),
        Some(RDFS_SUBCLASSOF),
        Some(&g("TimeMapping"))
    ));
}

#[test]
fn has_musical_time_frame_subproperty() {
    assert!(module().has(
        Some(&g("hasMusicalTimeFrame")),
        Some(RDFS_SUBPROPERTYOF),
        Some(&g("hasReferenceFrame"))
    ));
}

#[test]
fn ontology_properties_multi_source_functionality() {
    // Functionality is now carried by the canonical `logic:` characteristic records
    // (which live in the logic slice), not by a local `owl:FunctionalProperty` marker
    // on the music module — so this asserts over the merged ontology, not `module()`.
    let s = GraphStore::ontology();
    let constitutive = [
        "timeMappingKind",
        "tempoMapSegmentOf",
        "segmentSpan",
        "segmentTempoMapKind",
        "segmentMapRatioNumerator",
        "segmentMapRatioDenominator",
        "metricStructureOf",
        "metricGroupOrder",
        "meterCarrier",
        "assignedMeter",
        "assignmentSpan",
        "modulationFromFrame",
        "modulationToFrame",
        "grooveKind",
        "grooveGridUnit",
        "timeStartNumerator",
        "timeStartDenominator",
        "timeDurationNumerator",
        "timeDurationDenominator",
        "mapRatioNumerator",
        "mapRatioDenominator",
        "tempoRatioExpression",
        "groupLengthNumerator",
        "groupLengthDenominator",
        "pivotSourceValue",
        "pivotTargetValue",
    ];
    for prop in constitutive {
        assert!(
            s.is_functional_carrier(&g(prop)),
            "{prop} should carry a logic: functionalProperty characteristic"
        );
    }
    for prop in ["tempoRatioApprox", "groupAccentWeight"] {
        assert!(
            !s.is_functional_carrier(&g(prop)),
            "{prop} should NOT carry a logic: functionalProperty characteristic"
        );
    }
}

#[test]
fn common_musical_time_frame_exists() {
    assert!(module().has(
        Some(&g("musicalTimeFrameCommon")),
        Some(RDF_TYPE),
        Some(&g("MusicalTimeFrame"))
    ));
}

#[test]
fn tempo_map_common_exists() {
    assert!(module().has(
        Some(&g("tempoMapCommon")),
        Some(RDF_TYPE),
        Some(&g("TempoMap"))
    ));
}

#[test]
fn meter_sequence_fixtures_exist() {
    let s = module();
    for term in [
        "metricStructure58",
        "metricStructure78",
        "metricStructure44",
    ] {
        assert!(s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("MetricStructure"))));
    }
}

#[test]
fn polymeter_assignments_exist() {
    let s = module();
    for term in ["meterAssignmentGuitar78", "meterAssignmentDrums44"] {
        assert!(s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("MeterAssignment"))));
    }
}

#[test]
fn polymeter_pattern_exists() {
    assert!(module().has(
        Some(&g("polymeterPattern")),
        Some(RDF_TYPE),
        Some(&g("Entity"))
    ));
}

#[test]
fn tuplet_fixtures_exist() {
    let s = module();
    for term in ["timeMappingTuplet32", "timeMappingTuplet54"] {
        assert!(s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("TimeMapping"))));
    }
}

#[test]
fn sqrt2_canon_fixture_exists() {
    let s = module();
    let tm = g("timeMappingSqrt2Canon");
    assert!(s.has(Some(&tm), Some(RDF_TYPE), Some(&g("TimeMapping"))));
    assert!(s.has_literal(&tm, &g("tempoRatioExpression"), "sqrt(2)/2", XSD_STRING));
}

// ── SHACL guards (whole shapes corpus, inline fixtures) ───────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test-music-time/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

/// The canonical `musicalTimeFrameCommon` individual restated in-graph (honest
/// ABox completion, values verbatim from module.ttl) so the generated `sh:class`
/// range shape is satisfied without loading the merged ontology.
const FRAME_COMMON: &str = "\
gmeow:musicalTimeFrameCommon a gmeow:MusicalTimeFrame .
gmeow:musicalTimeFrameCommon gmeow:frameRealm gmeow:frameRealmMusicalTime .
gmeow:musicalTimeFrameCommon gmeow:frameKind gmeow:frameKindScalar .
gmeow:musicalTimeFrameCommon gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
gmeow:musicalTimeFrameCommon gmeow:hasAxis gmeow:axisTime .
gmeow:musicalTimeFrameCommon gmeow:requiresHost \"false\"^^xsd:boolean .
";

#[rstest]
#[case::musical_time_span_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:spanValid a gmeow:MusicalTimeSpan .
ex:spanValid gmeow:hasMusicalTimeFrame gmeow:musicalTimeFrameCommon .
ex:spanValid gmeow:timeStartNumerator \"0\"^^xsd:integer .
ex:spanValid gmeow:timeStartDenominator \"1\"^^xsd:integer .
ex:spanValid gmeow:timeDurationNumerator \"5\"^^xsd:integer .
ex:spanValid gmeow:timeDurationDenominator \"8\"^^xsd:integer .
{FRAME_COMMON}"
)))]
#[case::musical_time_span_missing_frame_fails(Case::inline(format!(
    "{PREFIXES}\
ex:spanNoFrame a gmeow:MusicalTimeSpan .
ex:spanNoFrame gmeow:timeStartNumerator \"0\"^^xsd:integer .
ex:spanNoFrame gmeow:timeStartDenominator \"1\"^^xsd:integer .
ex:spanNoFrame gmeow:timeDurationNumerator \"5\"^^xsd:integer .
ex:spanNoFrame gmeow:timeDurationDenominator \"8\"^^xsd:integer .
"
// The generated frame-requirement shape (crates/pipeline/src/stages/frame_shapes.rs,
// driven by gmeow:requiresFrame/gmeow:frameCardinality on MusicalTimeSpan) emits a
// generic "must carry exactly one reference frame (<property>)" message for every
// framed class — not a MusicalTimeSpan-specific "MusicalTimeFrame" phrasing; that
// exact wording is pinned by its own dedicated test
// (crates/pipeline/src/stages/frame_shapes.rs, the CharacterArc case).
)).fails().violations(&["must carry exactly one reference frame (gmeow:hasMusicalTimeFrame)"]))]
#[case::musical_time_span_zero_denominator_fails(Case::inline(format!(
    "{PREFIXES}\
ex:spanZeroDenom a gmeow:MusicalTimeSpan .
ex:spanZeroDenom gmeow:hasMusicalTimeFrame gmeow:musicalTimeFrameCommon .
ex:spanZeroDenom gmeow:timeStartNumerator \"0\"^^xsd:integer .
ex:spanZeroDenom gmeow:timeStartDenominator \"0\"^^xsd:integer .
ex:spanZeroDenom gmeow:timeDurationNumerator \"5\"^^xsd:integer .
ex:spanZeroDenom gmeow:timeDurationDenominator \"8\"^^xsd:integer .
{FRAME_COMMON}"
))
// `MusicalTimeSpan`'s positive-start-denominator bound is now projected SHACL
// derived from the EL-safe `logic:Restriction`/`logic:ValueRangeConstraint` axioms
// in module.ttl (`generated/shapes/validation-shapes.ttl`), which — like every
// OWL-restriction-derived cardinality/range shape — carries no `sh:message` (the
// prose-message convention was retired with the shapes-to-logic migration; see
// docs/MIGRATING-SHAPES-TO-LOGIC.md). `whole_shapes()` deliberately excludes that
// file from this fixture corpus (an open-world someValuesFrom reading would
// over-flag ABox-incomplete fixtures elsewhere in this suite), so exercising this
// PROJECTED bound requires the live production shape union instead, and the
// message-less result is asserted by path + constraint component rather than by
// text (see `Case::fails_on_path`'s doc comment).
.shape_union()
.fails()
.fails_on_path(
    "https://blackcatinformatics.ca/gmeow/timeStartDenominator",
    "MinExclusiveConstraintComponent",
))]
#[case::time_mapping_rational_passes(Case::inline(format!(
    "{PREFIXES}\
ex:tupletValid a gmeow:TimeMapping .
ex:tupletValid gmeow:timeMappingKind gmeow:timeMappingKindTuplet .
ex:tupletValid gmeow:mapsFrame gmeow:musicalTimeFrameCommon .
ex:tupletValid gmeow:mapsToFrame gmeow:musicalTimeFrameCommon .
ex:tupletValid gmeow:mapRatioNumerator \"3\"^^xsd:integer .
ex:tupletValid gmeow:mapRatioDenominator \"2\"^^xsd:integer .
{FRAME_COMMON}"
)))]
#[case::time_mapping_irrational_passes(Case::inline(format!(
    "{PREFIXES}\
ex:canonValid a gmeow:TimeMapping .
ex:canonValid gmeow:timeMappingKind gmeow:timeMappingKindTempoCanon .
ex:canonValid gmeow:mapsFrame gmeow:musicalTimeFrameVoiceGuitar .
ex:canonValid gmeow:mapsToFrame gmeow:musicalTimeFrameVoiceDrums .
ex:canonValid gmeow:tempoRatioExpression \"sqrt(2)/2\"^^xsd:string .
ex:canonValid gmeow:tempoRatioApprox \"0.70710678\"^^xsd:decimal .
gmeow:musicalTimeFrameVoiceGuitar a gmeow:MusicalTimeFrame .
gmeow:musicalTimeFrameVoiceGuitar gmeow:frameRealm gmeow:frameRealmMusicalTime .
gmeow:musicalTimeFrameVoiceGuitar gmeow:frameKind gmeow:frameKindScalar .
gmeow:musicalTimeFrameVoiceGuitar gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
gmeow:musicalTimeFrameVoiceGuitar gmeow:hasAxis gmeow:axisTime .
gmeow:musicalTimeFrameVoiceGuitar gmeow:requiresHost \"false\"^^xsd:boolean .
gmeow:musicalTimeFrameVoiceDrums a gmeow:MusicalTimeFrame .
gmeow:musicalTimeFrameVoiceDrums gmeow:frameRealm gmeow:frameRealmMusicalTime .
gmeow:musicalTimeFrameVoiceDrums gmeow:frameKind gmeow:frameKindScalar .
gmeow:musicalTimeFrameVoiceDrums gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
gmeow:musicalTimeFrameVoiceDrums gmeow:hasAxis gmeow:axisTime .
gmeow:musicalTimeFrameVoiceDrums gmeow:requiresHost \"false\"^^xsd:boolean .
"
)))]
#[case::time_mapping_both_encodings_fails(Case::inline(format!(
    "{PREFIXES}\
ex:tupletBoth a gmeow:TimeMapping .
ex:tupletBoth gmeow:timeMappingKind gmeow:timeMappingKindTuplet .
ex:tupletBoth gmeow:mapsFrame gmeow:musicalTimeFrameCommon .
ex:tupletBoth gmeow:mapsToFrame gmeow:musicalTimeFrameCommon .
ex:tupletBoth gmeow:mapRatioNumerator \"3\"^^xsd:integer .
ex:tupletBoth gmeow:mapRatioDenominator \"2\"^^xsd:integer .
ex:tupletBoth gmeow:tempoRatioExpression \"sqrt(2)/2\"^^xsd:string .
ex:tupletBoth gmeow:tempoRatioApprox \"0.70710678\"^^xsd:decimal .
{FRAME_COMMON}"
)).fails().violations(&["exactly one encoding"]))]
#[case::tempo_map_segment_backed_passes(Case::inline(format!(
    "{PREFIXES}\
ex:tempoMapSegmented a gmeow:TempoMap .
ex:tempoMapSegmented gmeow:timeMappingKind gmeow:timeMappingKindTempoMap .
ex:tempoMapSegmented gmeow:mapsFrame gmeow:musicalTimeFrameCommon .
ex:tempoMapSegmented gmeow:mapsToFrame gmeow:temporalFrameTAI .
ex:tempoMapSegmented gmeow:hasTempoMapSegment ex:tempoMapSegmentOne .
ex:tempoMapSegmentOne a gmeow:TempoMapSegment .
ex:tempoMapSegmentOne gmeow:tempoMapSegmentOf ex:tempoMapSegmented .
ex:tempoMapSegmentOne gmeow:segmentSpan gmeow:musicalTimeSpanWholeSection .
ex:tempoMapSegmentOne gmeow:segmentTempoMapKind gmeow:tempoMapKindConstant .
ex:tempoMapSegmentOne gmeow:segmentMapRatioNumerator \"1\"^^xsd:integer .
ex:tempoMapSegmentOne gmeow:segmentMapRatioDenominator \"2\"^^xsd:integer .
{FRAME_COMMON}"
)))]
#[case::non_tempo_map_segment_backed_fails(Case::inline(format!(
    "{PREFIXES}\
ex:tupletWithSegment a gmeow:TimeMapping .
ex:tupletWithSegment gmeow:timeMappingKind gmeow:timeMappingKindTuplet .
ex:tupletWithSegment gmeow:mapsFrame gmeow:musicalTimeFrameCommon .
ex:tupletWithSegment gmeow:mapsToFrame gmeow:musicalTimeFrameCommon .
ex:tupletWithSegment gmeow:hasTempoMapSegment ex:tupletSegmentOne .
ex:tupletSegmentOne a gmeow:TempoMapSegment .
ex:tupletSegmentOne gmeow:tempoMapSegmentOf ex:tupletWithSegment .
ex:tupletSegmentOne gmeow:segmentSpan gmeow:musicalTimeSpanWholeSection .
ex:tupletSegmentOne gmeow:segmentTempoMapKind gmeow:tempoMapKindConstant .
ex:tupletSegmentOne gmeow:segmentMapRatioNumerator \"1\"^^xsd:integer .
ex:tupletSegmentOne gmeow:segmentMapRatioDenominator \"2\"^^xsd:integer .
{FRAME_COMMON}"
)).fails().violations(&["exactly one encoding"]))]
#[case::meter_assignment_missing_carrier_fails(Case::inline(format!(
    "{PREFIXES}\
ex:meterBad a gmeow:MeterAssignment .
ex:meterBad gmeow:assignedMeter gmeow:metricStructure44 .
ex:meterBad gmeow:assignmentSpan gmeow:musicalTimeSpanWholeSection .
"
))
// `MeterAssignment`'s exactly-one-carrier bound is the same kind of projected,
// message-less SHACL as the start-denominator bound above (derived from the
// `owl:Restriction`/`owl:someValuesFrom gmeow:Entity` + qualified-cardinality
// axioms on `gmeow:meterCarrier` in module.ttl, surfaced only via
// `generated/shapes/validation-shapes.ttl`) — same fix, same rationale.
.shape_union()
.fails()
.fails_on_path(
    "https://blackcatinformatics.ca/gmeow/meterCarrier",
    "MinCountConstraintComponent",
))]
#[case::metric_modulation_pivot_format_fails(Case::inline(format!(
    "{PREFIXES}\
ex:modulationBad a gmeow:MetricModulation .
ex:modulationBad gmeow:modulationFromFrame gmeow:musicalTimeFrameCommon .
ex:modulationBad gmeow:modulationToFrame gmeow:musicalTimeFrameVoiceGuitar .
ex:modulationBad gmeow:pivotSourceValue \"3/8\"^^xsd:string .
ex:modulationBad gmeow:pivotTargetValue \"bad\"^^xsd:string .
"
)).fails().violations(&["rational string"]))]
fn music_time_shacl(#[case] case: Case) {
    case.run();
}
