// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_myth.py
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)`, converts to N-Triples, and
//! validates against the whole shapes corpus.
//!
//! `_add_narrative_frame(g, frame, axis)` expanded inline for every call site:
//!
//! ```text
//!   <frame>  a gmeow:NarrativeReferenceFrame ;
//!            gmeow:frameRealm gmeow:frameRealmNarrative ;
//!            gmeow:hasAxis <axis> ;
//!            gmeow:dimensionCount "1"^^xsd:nonNegativeInteger ;
//!            gmeow:frameKind gmeow:frameKindNarrative ;
//!            gmeow:requiresHost false ;
//!            gmeow:determinacyModel gmeow:determinacyCrisp .
//! ```
//!
//! The `_graph()`-based TBox-membership + bnode tests are migrated below as
//! `#[test]` fns over `GraphStore::ontology()` (the native twin of the merged
//! `load_merged_graph(include_imports=False)` graph):
//!   - `test_social_object_is_category` → [`social_object_is_category`]
//!   - `test_myth_properties_exist` → [`myth_properties_exist`]
//!   - `test_has_myth_telling_domain_range` → [`has_myth_telling_domain_range`]
//!   - `test_myth_frame_is_functional` → [`myth_frame_is_functional`]
//!   - `test_propagates_from_is_derived_from_subproperty` → [`propagates_from_is_derived_from_subproperty`]
//!   - `test_recurring_risk_exists` → [`recurring_risk_exists`]
//!   - `test_affected_consumer_surface_exists` → [`affected_consumer_surface_exists`]
//!   - `test_myth_el_restriction_on_has_myth_telling` → [`myth_el_restriction_on_has_myth_telling`]
//!   - `test_no_truth_axiom_on_myth` → [`no_truth_axiom_on_myth`]

mod conformance_support;
use conformance_support::*;
use purrdf::slice::rdf_query::Subject;
use rstest::rstest;

// ── IRI constants for the TBox-membership assertions ──────────────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn logic(local: &str) -> String {
    format!("{LOGIC}{local}")
}

// ── Helpers for the inline Turtle snippets ────────────────────────────────────

/// Turtle prefix block shared by all myth tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

/// Inline expansion of `_add_narrative_frame(g, frame, axis)`.
///
/// Emits the triples the Python helper adds for a minimal NarrativeReferenceFrame.
fn narrative_frame_ttl(frame: &str, axis: &str) -> String {
    format!(
        "\
{frame} a gmeow:NarrativeReferenceFrame .
{frame} gmeow:frameRealm gmeow:frameRealmNarrative .
{frame} gmeow:hasAxis {axis} .
{frame} gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
{frame} gmeow:frameKind gmeow:frameKindNarrative .
{frame} gmeow:requiresHost false .
{frame} gmeow:determinacyModel gmeow:determinacyCrisp .
"
    )
}

// ── Tests migrated from tests/test_myth.py ────────────────────────────────────

#[rstest]
#[case::myth_shacl_passes(
    Case::inline(format!(
        "{PREFIXES}\
{frame}\
ex:urbanLegend a gmeow:Myth .
ex:urbanLegend gmeow:mythFrame ex:urbanLegendFrame .
ex:urbanLegend gmeow:hasMythTelling ex:articleTelling .
ex:urbanLegend gmeow:recurringRisk true .
ex:urbanLegend gmeow:affectedConsumerSurface gmeow:consumerPublicSite .
ex:articleTelling a gmeow:CreativeWork .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
gmeow:consumerPublicSite a gmeow:ProjectionContext .
",
        frame = narrative_frame_ttl("ex:urbanLegendFrame", "ex:axisPlot"),
    ))
)]
#[case::myth_missing_frame_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:urbanLegend a gmeow:Myth .
ex:urbanLegend gmeow:hasMythTelling ex:articleTelling .
ex:articleTelling a gmeow:CreativeWork .
"
    ))
        .fails()
        .violations(&["exactly one reference frame (gmeow:mythFrame)"])
)]
#[case::myth_propagation_shacl_passes(
    Case::inline(format!(
        "{PREFIXES}\
{frame}\
ex:urbanLegend a gmeow:Myth .
ex:urbanLegend gmeow:mythFrame ex:urbanLegendFrame .
ex:articleTelling a gmeow:CreativeWork .
ex:socialPostTelling a gmeow:CreativeWork .
ex:socialPostTelling gmeow:propagatesFrom ex:articleTelling .
ex:urbanLegend gmeow:hasMythTelling ex:articleTelling .
ex:urbanLegend gmeow:hasMythTelling ex:socialPostTelling .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisPlot a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
",
        frame = narrative_frame_ttl("ex:urbanLegendFrame", "ex:axisPlot"),
    ))
)]
fn myth(#[case] case: Case) {
    case.run();
}

// ── Migrated `_graph()`-based TBox + bnode tests ──────────────────────────────

/// Twin of `test_social_object_is_category`.
#[test]
fn social_object_is_category() {
    let g = GraphStore::ontology();
    let so = gmeow("SocialObject");
    assert!(g.has(Some(&so), Some(RDF_TYPE), Some(OWL_CLASS)));
    assert!(g.has(Some(&so), Some(RDF_TYPE), Some(&logic("Category"))));
    assert!(g.has(Some(&so), Some(RDFS_SUBCLASS_OF), Some(&gmeow("Entity"))));
    assert!(g.has(Some(&so), Some(RDFS_SUBCLASS_OF), Some(&logic("Object"))));
}

/// Twin of `test_myth_properties_exist`.
#[test]
fn myth_properties_exist() {
    let g = GraphStore::ontology();
    for prop in ["hasMythTelling", "mythFrame", "propagatesFrom"] {
        assert!(
            g.has(
                Some(&gmeow(prop)),
                Some(RDF_TYPE),
                Some(OWL_OBJECT_PROPERTY)
            ),
            "{prop} must be an owl:ObjectProperty"
        );
    }
}

/// Twin of `test_has_myth_telling_domain_range`.
#[test]
fn has_myth_telling_domain_range() {
    let g = GraphStore::ontology();
    let prop = gmeow("hasMythTelling");
    assert!(g.has(Some(&prop), Some(RDFS_DOMAIN), Some(&gmeow("Myth"))));
    assert!(g.has(Some(&prop), Some(RDFS_RANGE), Some(&gmeow("CreativeWork"))));
}

/// Twin of `test_myth_frame_is_functional`.
#[test]
fn myth_frame_is_functional() {
    let g = GraphStore::ontology();
    let prop = gmeow("mythFrame");
    assert!(
        g.is_functional_carrier(&prop),
        "gmeow:mythFrame must carry a logic: functionalProperty characteristic"
    );
    assert!(g.has(Some(&prop), Some(RDFS_DOMAIN), Some(&gmeow("Myth"))));
    assert!(g.has(
        Some(&prop),
        Some(RDFS_RANGE),
        Some(&gmeow("NarrativeReferenceFrame"))
    ));
}

/// Twin of `test_propagates_from_is_derived_from_subproperty`.
#[test]
fn propagates_from_is_derived_from_subproperty() {
    let g = GraphStore::ontology();
    let prop = gmeow("propagatesFrom");
    assert!(g.has(
        Some(&prop),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&gmeow("wasDerivedFrom"))
    ));
    assert!(g.has(Some(&prop), Some(RDFS_DOMAIN), Some(&gmeow("CreativeWork"))));
    assert!(g.has(Some(&prop), Some(RDFS_RANGE), Some(&gmeow("CreativeWork"))));
}

/// Twin of `test_recurring_risk_exists`.
#[test]
fn recurring_risk_exists() {
    let g = GraphStore::ontology();
    let prop = gmeow("recurringRisk");
    assert!(g.has(Some(&prop), Some(RDF_TYPE), Some(OWL_DATATYPE_PROPERTY)));
    assert!(g.has(Some(&prop), Some(RDFS_DOMAIN), Some(&gmeow("Myth"))));
    assert!(g.has(Some(&prop), Some(RDFS_RANGE), Some(XSD_BOOLEAN)));
}

/// Twin of `test_affected_consumer_surface_exists`.
#[test]
fn affected_consumer_surface_exists() {
    let g = GraphStore::ontology();
    let prop = gmeow("affectedConsumerSurface");
    assert!(g.has(Some(&prop), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)));
    assert!(g.has(Some(&prop), Some(RDFS_DOMAIN), Some(&gmeow("Myth"))));
    assert!(g.has(
        Some(&prop),
        Some(RDFS_RANGE),
        Some(&gmeow("ProjectionContext"))
    ));
}

/// Twin of `test_myth_el_restriction_on_has_myth_telling`.
///
/// Walks the blank `owl:Restriction` nodes that are `rdfs:subClassOf` of
/// `gmeow:Myth` and asserts one matches `∃ hasMythTelling . CreativeWork`.
#[test]
fn myth_el_restriction_on_has_myth_telling() {
    let g = GraphStore::ontology();
    let myth = Subject::Named(gmeow("Myth"));
    let found = g
        .objects_h(&myth, RDFS_SUBCLASS_OF)
        .iter()
        .filter_map(GraphStore::object_as_subject)
        .any(|restriction| {
            g.restriction_matches(
                &restriction,
                &gmeow("hasMythTelling"),
                OWL_SOME_VALUES_FROM,
                &gmeow("CreativeWork"),
            )
        });
    assert!(
        found,
        "Myth must have an EL someValuesFrom restriction on hasMythTelling"
    );
}

/// Twin of `test_no_truth_axiom_on_myth` — no truth-verdict property may declare
/// `rdfs:domain gmeow:Myth`.
#[test]
fn no_truth_axiom_on_myth() {
    let g = GraphStore::ontology();
    for forbidden in ["isTrue", "isFalse", "isDeceptive"] {
        assert!(
            !g.has(
                Some(&gmeow(forbidden)),
                Some(RDFS_DOMAIN),
                Some(&gmeow("Myth"))
            ),
            "no {forbidden} property may target gmeow:Myth"
        );
    }
}
