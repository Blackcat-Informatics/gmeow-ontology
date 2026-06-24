// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_creative_works.py (#867)
//!
//! Migrated tests build inline Turtle graphs, convert to N-Triples, and validate
//! against the whole shapes corpus via `validate()`.
//!
//! The Python helper functions `_add_work`, `_add_expression`, `_add_manifestation`,
//! and `_add_item` are inlined as Turtle string constants or helper functions below.
//!
//! Retained in Python (not migrated):
//!   - `test_creative_work_is_category`: loads merged graph, cross-slice subject in documents/
//!   - `test_wemi_tiers_subclass_information_object`: uses transitive_objects() graph traversal
//!   - `test_document_subclasses_work`: cross-slice subjects in documents/
//!   - `test_media_etc_subclasses_manifestation`: cross-slice subjects in documents/
//!   - `test_contribution_degree_value_vocab`: cross-slice subjects in citations/
//!   - `test_creation_event_types_exist`: cross-slice subjects in events/
//!   - `test_literary_work_subclasses_work`: cross-slice subject in documents/
//!   - `test_serial_work_subclasses_work`: cross-slice subject in documents/
//!   - `test_content_segment_subclasses_information_object`: cross-slice subject in documents/
//!   - `test_has_segment_is_subproperty_of_has_part`: cross-slice subject in documents/
//!   - `test_segment_of_is_subproperty_of_part_of`: cross-slice subject in documents/
//!   - `test_segment_type_is_functional`: cross-slice subject in documents/
//!   - `test_segment_index_is_functional`: cross-slice subject in documents/
//!   - `test_content_segment_type_value_vocab`: cross-slice subjects in documents/

mod conformance_support;
use conformance_support::*;

// ── Shared prefix block ───────────────────────────────────────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// ── Inline helper: populate a reference frame ─────────────────────────────────
//
// Mirrors `_add_expression` in Python: the reference frame sub-graph required
// by the SHACL shape for gmeow:Expression.

const REFERENCE_FRAME_TTL: &str = "\
ex:englishFrame a gmeow:ReferenceFrame .
ex:englishFrame rdfs:label \"English\" .
ex:englishFrame gmeow:frameRealm gmeow:frameRealmNarrative .
ex:englishFrame gmeow:hasAxis ex:axisLang .
ex:englishFrame gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:englishFrame gmeow:frameKind gmeow:frameKindNarrative .
ex:englishFrame gmeow:requiresHost false .
ex:englishFrame gmeow:determinacyModel gmeow:determinacyCrisp .
gmeow:frameRealmNarrative a gmeow:FrameRealm .
ex:axisLang a gmeow:Axis .
gmeow:frameKindNarrative a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
";

// ── Tests migrated from tests/test_creative_works.py ─────────────────────────

/// `test_spine_shacl_passes` — a fully-populated WEMI spine passes SHACL.
#[test]
fn spine_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
ex:expression a gmeow:Expression .
ex:expression rdfs:label \"Test Expression\" .
ex:expression gmeow:realizes ex:work .
ex:expression gmeow:hasReferenceFrame ex:englishFrame .
{REFERENCE_FRAME_TTL}\
ex:manifestation a gmeow:Manifestation .
ex:manifestation rdfs:label \"Test Manifestation\" .
ex:manifestation gmeow:embodies ex:expression .
ex:item a gmeow:Item .
ex:item rdfs:label \"Test Item\" .
ex:item gmeow:exemplifies ex:manifestation .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "fully-populated WEMI spine must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_expression_without_work_fails_shacl` — an Expression that realizes no
/// Work violates SHACL.
#[test]
fn expression_without_work_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:expression a gmeow:Expression .
ex:expression rdfs:label \"Orphan Expression\" .
ex:expression gmeow:hasReferenceFrame ex:englishFrame .
{REFERENCE_FRAME_TTL}\
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(!ok(&report), "Expression without realizes must fail SHACL");
    assert!(
        violations(&report)
            .iter()
            .any(|v| v.contains("Expression must realize")),
        "expected 'Expression must realize' violation; got: {:?}",
        violations(&report)
    );
}

/// `test_manifestation_without_expression_fails_shacl` — a Manifestation that
/// embodies no Expression violates SHACL.
#[test]
fn manifestation_without_expression_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:manifestation a gmeow:Manifestation .
ex:manifestation rdfs:label \"Orphan Manifestation\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "Manifestation without embodies must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|v| v.contains("Manifestation must embody")),
        "expected 'Manifestation must embody' violation; got: {:?}",
        violations(&report)
    );
}

/// `test_item_without_manifestation_fails_shacl` — an Item without an
/// `exemplifies` relation fails SHACL validation.
#[test]
fn item_without_manifestation_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:item a gmeow:Item .
ex:item rdfs:label \"Orphan Item\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(!ok(&report), "Item without exemplifies must fail SHACL");
    assert!(
        violations(&report)
            .iter()
            .any(|v| v.contains("Item must exemplify")),
        "expected 'Item must exemplify' violation; got: {:?}",
        violations(&report)
    );
}

/// `test_contribution_shacl_passes` — a well-formed Contribution relator passes
/// SHACL.
#[test]
fn contribution_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:contribution a gmeow:Contribution .
ex:contribution gmeow:contributor ex:alice .
ex:contribution gmeow:contributionTarget ex:work .
ex:contribution gmeow:contributionRole gmeow:roleAuthor .
ex:alice a gmeow:Agent .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed Contribution must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_contribution_missing_role_fails_shacl` — a Contribution missing a
/// contributionRole fails SHACL.
#[test]
fn contribution_missing_role_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:contribution a gmeow:Contribution .
ex:contribution gmeow:contributor ex:alice .
ex:contribution gmeow:contributionTarget ex:work .
ex:alice a gmeow:Agent .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "Contribution missing contributionRole must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|v| v.contains("Contribution must specify exactly one role")),
        "expected 'Contribution must specify exactly one role' violation; got: {:?}",
        violations(&report)
    );
}

/// `test_content_segment_shacl_passes` — a well-formed ContentSegment passes
/// SHACL.
#[test]
fn content_segment_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:chapter1 a gmeow:ContentSegment .
ex:chapter1 rdfs:label \"Chapter 1\" .
ex:chapter1 gmeow:segmentOf ex:book .
ex:book a gmeow:LiteraryWork .
ex:book rdfs:label \"Test Book\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed ContentSegment must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_content_segment_without_container_fails_shacl` — a ContentSegment
/// with no segmentOf violates SHACL.
#[test]
fn content_segment_without_container_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:chapter1 a gmeow:ContentSegment .
ex:chapter1 rdfs:label \"Orphan Chapter\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "ContentSegment without segmentOf must fail SHACL"
    );
    assert!(
        violations(&report)
            .iter()
            .any(|v| v.contains("ContentSegment must be part of")),
        "expected 'ContentSegment must be part of' violation; got: {:?}",
        violations(&report)
    );
}
