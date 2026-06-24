// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_profiles.py (#867)
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

/// Turtle prefix block shared by all profiles tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
";

/// `test_profile_shape_passes_for_wellformed_profile` — a well-formed Profile
/// individual with label, definition, and profileDescriptor passes SHACL.
#[test]
fn profile_shape_passes_for_wellformed_profile() {
    let ttl = format!(
        "{PREFIXES}\
ex:myProfile a gmeow:Profile .
ex:myProfile rdfs:label \"My profile\" .
ex:myProfile skos:definition \"A test profile.\" .
ex:myProfile gmeow:profileDescriptor gmeow:hasProfile .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed Profile must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_profile_shape_fails_for_invalid_profile_applies_to` — a Profile with
/// `gmeow:profileAppliesTo` set to a plain literal (not a class IRI) must fail
/// SHACL with a violation mentioning profileAppliesTo, ProfileShape, or class.
#[test]
fn profile_shape_fails_for_invalid_profile_applies_to() {
    let ttl = format!(
        "{PREFIXES}\
ex:myProfile a gmeow:Profile .
ex:myProfile rdfs:label \"Bad profile\" .
ex:myProfile skos:definition \"profileAppliesTo must be a class.\" .
ex:myProfile gmeow:profileDescriptor gmeow:hasProfile .
ex:myProfile gmeow:profileAppliesTo \"not-a-class\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "Profile with literal profileAppliesTo must fail SHACL"
    );
    let errs = violations(&report);
    assert!(
        errs.iter().any(|e| e.contains("profileAppliesTo")
            || e.contains("ProfileShape")
            || e.contains("class")),
        "violation must mention profileAppliesTo, ProfileShape, or class; got: {errs:?}"
    );
}

/// `test_profile_open_value_guard_warns_on_orphan` — a Profile with
/// `gmeow:profileOpenValue` pointing to a class, combined with an instance of
/// that class that is not referenced by any profile descriptor, should pass
/// (warning only) with a warning about open value individuals not being
/// referenced by a profile descriptor.
#[test]
fn profile_open_value_guard_warns_on_orphan() {
    let ttl = format!(
        "{PREFIXES}\
gmeow:profileReferenceFrame a gmeow:Profile .
gmeow:profileReferenceFrame rdfs:label \"Reference Frame Profile\" .
gmeow:profileReferenceFrame skos:definition \"Closed descriptor schema for reference frames.\" .
gmeow:profileReferenceFrame gmeow:profileDescriptor gmeow:frameRealm .
gmeow:profileReferenceFrame gmeow:profileOpenValue gmeow:FrameRealm .
ex:orphanRealm a gmeow:FrameRealm .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "orphan open-value individual should produce warning only, not violation; violations: {:?}",
        violations(&report)
    );
    let warns = warnings(&report);
    assert!(
        warns.iter().any(|w| w.contains(
            "Open value individuals must be referenced by at least one profile descriptor"
        )),
        "expected open-value warning; got: {warns:?}"
    );
}
