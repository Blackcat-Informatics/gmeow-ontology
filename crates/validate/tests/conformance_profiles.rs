// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_profiles.py
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)`, converts to N-Triples, and validates
//! against the whole shapes corpus.
//!
//! All three source tests used a plain `Graph()` (not `_graph()`), so they
//! all use `validate(&nt)` — no ontology merge required.
//!
//! Retained in Python (not migrated):
//!   - TBox structural assertions: already migrated to
//!     `slices/core/profiles/tests/structural.ttl` as declarative
//!     `gmeow:StructuralAssertion` cells (docstring note in source).

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

/// Turtle prefix block shared by all profiles tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
";

#[rstest]
#[case::profile_shape_passes_for_wellformed_profile(
    Case::inline(format!(
        "{PREFIXES}\
ex:myProfile a gmeow:Profile .
ex:myProfile rdfs:label \"My profile\" .
ex:myProfile skos:definition \"A test profile.\" .
ex:myProfile gmeow:profileDescriptor gmeow:hasProfile .
"
    ))
)]
#[case::profile_open_value_guard_violates_on_orphan(
    Case::inline(format!(
        "{PREFIXES}\
gmeow:profileReferenceFrame a gmeow:Profile .
gmeow:profileReferenceFrame rdfs:label \"Reference Frame Profile\" .
gmeow:profileReferenceFrame skos:definition \"Closed descriptor schema for reference frames.\" .
gmeow:profileReferenceFrame gmeow:profileDescriptor gmeow:frameRealm .
gmeow:profileReferenceFrame gmeow:profileOpenValue gmeow:FrameRealm .
ex:orphanRealm a gmeow:FrameRealm .
"
    ))
    .violations(&["Open value individuals must be referenced by at least one profile descriptor"])
)]
// W2 falsifying regression: an UNWIRED owned open value still fires the guard.
// The REAL `gmeow:sliceQualityRubric` profile (merged in by `.shape_union()`) narrows
// its `gmeow:profileOpenValue` to `gmeow:SliceQualityDimension`; the 10 minted
// dimensions are all referenced by an axis's `gmeow:axisDimension`, so they pass. An
// ORPHAN `gmeow:SliceQualityDimension` referenced by NO descriptor must still trip
// `gmeow:ProfileOpenValueUseConstraintProceduralConstraintShape`
// (`SPARQLConstraintComponent`). The constraint is now `logic:severity "Violation"`, so
// the orphan is a hard `sh:Violation`; `.violations(...)` both witnesses the guard and
// guards the promotion — a revert to `"Warning"` would stop the violation and red this.
#[case::slice_quality_rubric_open_value_guard_violates_on_orphan_dimension(
    Case::inline(format!(
        "{PREFIXES}\
ex:orphanDim a gmeow:SliceQualityDimension .
ex:orphanDim rdfs:label \"orphan dimension\"@x-gmeow-english .
ex:orphanDim skos:definition \"A SliceQualityDimension owned by the rubric's open value but referenced by no axis descriptor — the extensibility-by-construction guard must flag it.\"@x-gmeow-english .
ex:orphanDim gmeow:graphBoxRole gmeow:boxABox .
"
    ))
    .shape_union()
    .violations(&["Open value individuals must be referenced by at least one profile descriptor"])
)]
// A Profile with `gmeow:profileAppliesTo` set to a plain literal (not a class IRI)
// must fail SHACL with a violation mentioning one of profileAppliesTo / ProfileShape
// / class (case-sensitive disjunction → `any_violation`).
#[case::profile_shape_fails_for_invalid_profile_applies_to(
    Case::inline(format!(
        "{PREFIXES}\
ex:myProfile a gmeow:Profile .
ex:myProfile rdfs:label \"Bad profile\" .
ex:myProfile skos:definition \"profileAppliesTo must be a class.\" .
ex:myProfile gmeow:profileDescriptor gmeow:hasProfile .
ex:myProfile gmeow:profileAppliesTo \"not-a-class\" .
"
    ))
    .fails()
    .any_violation(&["profileAppliesTo", "ProfileShape", "class"])
)]
fn profiles(#[case] case: Case) {
    case.run();
}
