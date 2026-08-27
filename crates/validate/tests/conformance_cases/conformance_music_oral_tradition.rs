// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from
//! `slices/extensions/music/tests/test_music_oral_tradition.py`.
//!
//! Oral tradition & performance lineage guards (Principles 4, 5, 9, 10, 16). The
//! Python originals queried `load_merged_graph(include_imports=False)` — the merged
//! ontology, into which the Raga-Yaman fixtures are folded (they live in
//! `slices/extensions/music/module.ttl`, not a separate fixtures load). The native
//! twin therefore uses `GraphStore::ontology()`, the imports-false merged store.
//!
//! Highlights:
//! * `no_shape_requires_notated_expression` ports the Python blank-node SHACL-list
//!   walk (`_shapes_requiring_notated` + `_path_sequence_contains` +
//!   `_has_positive_min_count`) using the bnode-aware `*_h` helpers: it walks
//!   `sh:NodeShape → sh:property/sh:node`, `sh:path` RDF-list sequences
//!   (`rdf:first`/`rdf:rest`/`rdf:nil`), `sh:hasValue`, `sh:qualifiedValueShape`,
//!   and `sh:minCount`. The merged-ontology graph carries no notated-requiring
//!   shape targeting `MusicalWork`/`Work`, so the walk yields no violations — the
//!   oral-tradition guarantee — exactly as the original.
//! * The two competency queries (`music-oral-works.rq` with COUNT/GROUP BY/HAVING/
//!   `FILTER NOT EXISTS`, and `music-gharana-memberships.rq` with the
//!   `gmeow:displayable true` gate) run over the same store.

use crate::conformance_support::*;
use purrdf::TermValue;
use purrdf::slice::rdf_query::{Object, Subject};
use std::collections::HashSet;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const SHACL: &str = "http://www.w3.org/ns/shacl#";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn sh(local: &str) -> String {
    format!("{SHACL}{local}")
}

// ── triple-membership twins ───────────────────────────────────────────────────

/// Twin of `test_oral_tradition_work_fixture_exists`.
#[gmeow_test_batch_macros::batch_test]
fn oral_tradition_work_fixture_exists() {
    let g = GraphStore::ontology();
    let work = gm("fixtureOralRagaYamanWork");
    assert!(g.has(Some(&work), Some(RDF_TYPE), Some(&gm("MusicalWork"))));
    assert!(g.has(
        Some(&work),
        Some(&gm("hasDeterminacy")),
        Some(&gm("determinacyVague")),
    ));
}

/// Twin of `test_oral_tradition_expressions_have_no_notated_member`: every
/// Expression of the oral work carries an oral / performed / improvised realization
/// mode (never notated).
#[gmeow_test_batch_macros::batch_test]
fn oral_tradition_expressions_have_no_notated_member() {
    let g = GraphStore::ontology();
    let modes: HashSet<String> = [
        gm("realizationModeOral"),
        gm("realizationModePerformed"),
        gm("realizationModeImprovised"),
    ]
    .into_iter()
    .collect();
    for term in [
        "fixtureRagaYamanOralExpression",
        "fixtureRagaYamanPerformed1960",
        "fixtureRagaYamanImprovised1975",
        "fixtureRagaYamanPerformed1980",
    ] {
        let expr = gm(term);
        assert!(
            g.has(Some(&expr), Some(RDF_TYPE), Some(&gm("Expression"))),
            "{term} should be an Expression"
        );
        let expr_modes = g.objects(&expr, &gm("realizationMode"));
        assert!(!expr_modes.is_empty(), "{term} has no realization mode");
        for m in &expr_modes {
            assert!(
                modes.contains(m),
                "{term} has unexpected realization mode {m}"
            );
        }
    }
}

/// Twin of `test_performance_lineage_derivation_chain`: the performances form a
/// `wasDerivedFrom` descent chain.
#[gmeow_test_batch_macros::batch_test]
fn performance_lineage_derivation_chain() {
    let g = GraphStore::ontology();
    let derived = gm("wasDerivedFrom");
    assert!(g.has(
        Some(&gm("fixtureRagaYamanPerformed1960")),
        Some(&derived),
        Some(&gm("fixtureRagaYamanOralExpression")),
    ));
    assert!(g.has(
        Some(&gm("fixtureRagaYamanImprovised1975")),
        Some(&derived),
        Some(&gm("fixtureRagaYamanPerformed1960")),
    ));
    assert!(g.has(
        Some(&gm("fixtureRagaYamanPerformed1980")),
        Some(&derived),
        Some(&gm("fixtureRagaYamanImprovised1975")),
    ));
}

/// Twin of `test_tune_family_is_versionset`: the tune family is a `VersionSet`;
/// memberships are `VersionMembership` relators.
#[gmeow_test_batch_macros::batch_test]
fn tune_family_is_versionset() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gm("fixtureRagaYamanKiranaSet")),
        Some(RDF_TYPE),
        Some(&gm("VersionSet")),
    ));
    for term in [
        "fixtureRagaYamanKiranaMembership1960",
        "fixtureRagaYamanKiranaMembership1975",
        "fixtureRagaYamanKiranaMembership1980",
        "fixtureRagaYamanContestedMembership",
    ] {
        assert!(
            g.has(
                Some(&gm(term)),
                Some(RDF_TYPE),
                Some(&gm("VersionMembership"))
            ),
            "{term} should be a VersionMembership"
        );
    }
}

/// Twin of `test_versionset_reused_unchanged`: no tune-family-specific terms were
/// added to the versions slice (`versions.ttl` defines neither `TuneFamily` nor
/// `TuneFamilyMembership`).
#[gmeow_test_batch_macros::batch_test]
fn versionset_reused_unchanged() {
    let g = GraphStore::ontology();
    let versions_defined_by = "https://blackcatinformatics.ca/gmeow/slices/versions";
    for term in ["TuneFamily", "TuneFamilyMembership"] {
        assert!(
            !g.has(
                Some(&gm(term)),
                Some(RDFS_IS_DEFINED_BY),
                Some(versions_defined_by),
            ),
            "versions.ttl should not define {term}"
        );
    }
}

/// Twin of `test_contested_membership_is_suppressed_not_deleted`: the contested
/// membership is `displayable false` and retained in the graph.
#[gmeow_test_batch_macros::batch_test]
fn contested_membership_is_suppressed_not_deleted() {
    let g = GraphStore::ontology();
    let membership = gm("fixtureRagaYamanContestedMembership");
    assert!(g.has(
        Some(&membership),
        Some(RDF_TYPE),
        Some(&gm("VersionMembership"))
    ));
    assert!(g.has_literal(&membership, &gm("displayable"), "false", XSD_BOOLEAN));
}

/// Twin of `test_transmission_event_and_roles`: the transmission event uses
/// `eventTypeTransmission` and transmitter/learner participation roles.
#[gmeow_test_batch_macros::batch_test]
fn transmission_event_and_roles() {
    let g = GraphStore::ontology();
    let event = gm("fixtureKiranaTransmissionEvent");
    assert!(g.has(Some(&event), Some(RDF_TYPE), Some(&gm("Event"))));
    assert!(g.has(
        Some(&event),
        Some(&gm("eventType")),
        Some(&gm("eventTypeTransmission")),
    ));
    for term in [
        "fixtureKiranaTransmitterParticipation",
        "fixtureKiranaLearnerParticipation",
    ] {
        let part = gm(term);
        assert!(g.has(Some(&part), Some(RDF_TYPE), Some(&gm("Participation"))));
        assert!(g.has(Some(&part), Some(&gm("participationEvent")), Some(&event)));
    }
}

// ── blank-node SHACL-list traversal ───────────────────────────────────────────

/// Concepts that, if REQUIRED by a shape targeting `MusicalWork`/`Work`, violate the
/// oral-tradition guarantee. Mirrors Python `_NOTATED_CONCEPTS`.
fn notated_concepts() -> HashSet<String> {
    [gm("realizationModeNotated"), gm("ScoreEdition")]
        .into_iter()
        .collect()
}

/// True iff `object` is a named node in `concepts`.
fn object_in(object: &Object, concepts: &HashSet<String>) -> bool {
    matches!(object, Object::Named(iri) if concepts.contains(iri))
}

/// Parse an integer literal object (`int(min_count)` in Python), or `None`.
fn literal_int(object: &Object) -> Option<i64> {
    match object {
        Object::Literal { value, .. } => value.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Port of `_has_positive_min_count`: the property shape has `sh:minCount > 0`.
fn has_positive_min_count(g: &GraphStore, property_shape: &Subject) -> bool {
    g.value_h(property_shape, &sh("minCount"))
        .and_then(|o| literal_int(&o))
        .is_some_and(|n| n > 0)
}

/// Port of `_path_sequence_contains`: a SHACL path (possibly an RDF-list sequence)
/// contains one of `concepts`.
fn path_sequence_contains(g: &GraphStore, path: &Object, concepts: &HashSet<String>) -> bool {
    if object_in(path, concepts) {
        return true;
    }
    // A literal path never carries a sub-list.
    let Some(path_subject) = GraphStore::object_as_subject(path) else {
        return false;
    };
    for step in g.objects_h(&path_subject, RDF_FIRST) {
        if object_in(&step, concepts) {
            return true;
        }
        if path_sequence_contains(g, &step, concepts) {
            return true;
        }
    }
    for rest in g.objects_h(&path_subject, RDF_REST) {
        let is_nil = matches!(&rest, Object::Named(iri) if iri == RDF_NIL);
        if !is_nil && path_sequence_contains(g, &rest, concepts) {
            return true;
        }
    }
    false
}

/// Port of `_shapes_requiring_notated`: the `(nodeShape, propertyShape, reason)`
/// tuples that target `MusicalWork`/`Work` and require a notated Expression.
fn shapes_requiring_notated(g: &GraphStore) -> Vec<(Subject, Subject, String)> {
    let concepts = notated_concepts();
    let work_targets: HashSet<String> = [gm("MusicalWork"), gm("Work")].into_iter().collect();
    let mut violations: Vec<(Subject, Subject, String)> = Vec::new();

    for node_shape in g.subjects_of_type_h(&sh("NodeShape")) {
        let targets = g.objects_h(&node_shape, &sh("targetClass"));
        let hits_work = targets
            .iter()
            .any(|o| matches!(o, Object::Named(iri) if work_targets.contains(iri)));
        if !hits_work {
            continue;
        }

        let mut visited: HashSet<Subject> = HashSet::new();
        let mut to_visit: Vec<Subject> = Vec::new();
        for edge in [sh("property"), sh("node")] {
            for o in g.objects_h(&node_shape, &edge) {
                if let Some(s) = GraphStore::object_as_subject(&o) {
                    to_visit.push(s);
                }
            }
        }

        while let Some(current) = to_visit.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());

            let is_property_shape = g
                .objects_h(&current, RDF_TYPE)
                .iter()
                .any(|o| matches!(o, Object::Named(iri) if iri == &sh("PropertyShape")))
                || !g.objects_h(&current, &sh("path")).is_empty();

            if is_property_shape {
                if let Some(path) = g.value_h(&current, &sh("path")) {
                    // Simple path requiring a notated concept.
                    if object_in(&path, &concepts)
                        && g.value_h(&current, &sh("minCount"))
                            .and_then(|mc| literal_int(&mc))
                            .is_some_and(|n| n > 0)
                    {
                        violations.push((
                            node_shape.clone(),
                            current.clone(),
                            format!("sh:minCount on notated path {path:?}"),
                        ));
                    }
                    // Path sequence containing a notated concept with minCount > 0.
                    if path_sequence_contains(g, &path, &concepts)
                        && has_positive_min_count(g, &current)
                    {
                        violations.push((
                            node_shape.clone(),
                            current.clone(),
                            format!("path sequence requires a notated concept: {path:?}"),
                        ));
                    }
                }

                // sh:hasValue a notated concept.
                if g.objects_h(&current, &sh("hasValue"))
                    .iter()
                    .any(|o| object_in(o, &concepts))
                {
                    violations.push((
                        node_shape.clone(),
                        current.clone(),
                        "sh:hasValue requires a notated concept".to_owned(),
                    ));
                }

                // sh:qualifiedValueShape → traverse.
                for qvs in g.objects_h(&current, &sh("qualifiedValueShape")) {
                    if let Some(s) = GraphStore::object_as_subject(&qvs) {
                        to_visit.push(s);
                    }
                }
            }

            // Recurse into nested node/property shapes.
            for edge in [sh("node"), sh("property")] {
                for o in g.objects_h(&current, &edge) {
                    if let Some(s) = GraphStore::object_as_subject(&o) {
                        to_visit.push(s);
                    }
                }
            }
        }
    }

    violations
}

/// Twin of `test_no_shape_requires_notated_expression`: no SHACL shape targeting
/// `MusicalWork`/`Work` requires a notated Expression.
#[gmeow_test_batch_macros::batch_test]
fn no_shape_requires_notated_expression() {
    let g = GraphStore::ontology();
    let violations = shapes_requiring_notated(&g);
    assert!(
        violations.is_empty(),
        "SHACL shapes violate the oral-tradition guarantee: {violations:?}"
    );
}

/// Positive control for the notated-shape walker. The real merged ontology carries
/// NO `sh:NodeShape` whose `sh:targetClass` is `MusicalWork`/`Work`, so
/// `no_shape_requires_notated_expression` above walks an empty target set and its
/// `is_empty()` assertion is VACUOUSLY TRUE — it would pass even if the walker were
/// broken. This synthetic fixture supplies exactly the shapes the walker is meant to
/// catch, so `shapes_requiring_notated` firing here proves the walker discriminates,
/// which is what makes the negative test's emptiness over the real ontology meaningful.
///
/// Three property shapes, one per detection arm of `shapes_requiring_notated`:
///   1. `gmeow:shapeSimple` — `sh:path gmeow:realizationModeNotated` with `sh:minCount 1`
///      (the simple-path arm, and `path_sequence_contains`'s `object_in` short-circuit).
///   2. `gmeow:shapeSeq` — `sh:path ( gmeow:someStep gmeow:realizationModeNotated )` with
///      `sh:minCount 1` (the RDF-list sequence arm, exercising `path_sequence_contains`).
///   3. `gmeow:shapeHasValue` — `sh:hasValue gmeow:ScoreEdition` (the `sh:hasValue` arm).
const NOTATED_VIOLATION_FIXTURE_TTL: &str = "\
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

gmeow:shapeMusicalWork a sh:NodeShape ;
    sh:targetClass gmeow:MusicalWork ;
    sh:property gmeow:shapeSimple ;
    sh:property gmeow:shapeSeq ;
    sh:property gmeow:shapeHasValue .

gmeow:shapeSimple a sh:PropertyShape ;
    sh:path gmeow:realizationModeNotated ;
    sh:minCount 1 .

gmeow:shapeSeq a sh:PropertyShape ;
    sh:path ( gmeow:someStep gmeow:realizationModeNotated ) ;
    sh:minCount 1 .

gmeow:shapeHasValue a sh:PropertyShape ;
    sh:path gmeow:notatedIn ;
    sh:hasValue gmeow:ScoreEdition .
";

/// Positive control that closes the vacuous-truth gap in
/// `no_shape_requires_notated_expression`: over a synthetic graph that DOES carry
/// notated-requiring shapes targeting `gmeow:MusicalWork`, the walker must return a
/// non-empty violation set covering all three of its detection arms.
#[gmeow_test_batch_macros::batch_test]
fn notated_shape_walker_fires_on_violations() {
    let g = GraphStore::parse_ttl(NOTATED_VIOLATION_FIXTURE_TTL);
    let violations = shapes_requiring_notated(&g);
    assert!(
        !violations.is_empty(),
        "walker must fire on a genuine notated-requiring shape"
    );

    // Pin the count by the DISTINCT violating property shapes, not the raw tuple
    // count (a single shape can trip two arms) and not blank-node ordering (the
    // fixture uses named property shapes, so identity is stable).
    let violating_shapes: HashSet<Subject> =
        violations.iter().map(|(_, ps, _)| ps.clone()).collect();
    assert_eq!(
        violating_shapes.len(),
        3,
        "expected one violating property shape per detection arm, got {violating_shapes:?}"
    );
    for local in ["shapeSimple", "shapeSeq", "shapeHasValue"] {
        assert!(
            violating_shapes.contains(&Subject::Named(gm(local))),
            "arm {local} did not fire: {violating_shapes:?}"
        );
    }
}

// ── competency-query twins ────────────────────────────────────────────────────

/// The `xsd:string`/`xsd:integer` lexical form of a solution term, or `None`.
fn literal_lexical(term: &Option<TermValue>) -> Option<&str> {
    match term {
        Some(TermValue::Literal { lexical_form, .. }) => Some(lexical_form.as_str()),
        _ => None,
    }
}

/// The IRI of a solution term, or `None`.
fn iri_of(term: &Option<TermValue>) -> Option<&str> {
    match term {
        Some(TermValue::Iri(iri)) => Some(iri.as_str()),
        _ => None,
    }
}

/// Twin of `test_competency_query_oral_works` (CQ6): the oral Raga-Yaman work with
/// at least three performances. Exercises COUNT(DISTINCT)/GROUP BY/HAVING/
/// `FILTER NOT EXISTS` (registered `music-oral/oral-works`, `Feature::FilterNotExists`).
#[gmeow_test_batch_macros::batch_test]
fn competency_query_oral_works() {
    let g = GraphStore::ontology();
    let (_, rows) = g.select(&[], &read_query("music-oral-works.rq"));
    assert!(
        !rows.is_empty(),
        "Expected at least one oral-tradition work result"
    );
    let oral_work = gm("fixtureOralRagaYamanWork");
    let row = rows
        .iter()
        .find(|r| iri_of(r.first().unwrap_or(&None)) == Some(oral_work.as_str()))
        .unwrap_or_else(|| panic!("oral Raga-Yaman work not in results: {rows:?}"));
    let count = literal_lexical(row.get(1).unwrap_or(&None))
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or_else(|| panic!("performanceCount not an integer literal: {row:?}"));
    assert!(count >= 3, "expected >= 3 performances, got {count}");
}

/// Twin of `test_competency_query_gharana_memberships` (CQ7): exactly three
/// Kirana-gharana memberships, with the suppressed (displayable-false) membership
/// excluded. The `gmeow:displayable true` gate is the suppression mechanism.
#[gmeow_test_batch_macros::batch_test]
fn competency_query_gharana_memberships() {
    let g = GraphStore::ontology();
    let (_, rows) = g.select(&[], &read_query("music-gharana-memberships.rq"));
    assert_eq!(
        rows.len(),
        3,
        "Expected 3 displayed memberships, got {}: {rows:?}",
        rows.len()
    );

    let memberships: HashSet<&str> = rows
        .iter()
        .filter_map(|r| iri_of(r.first().unwrap_or(&None)))
        .collect();
    assert!(
        !memberships.contains(gm("fixtureRagaYamanContestedMembership").as_str()),
        "contested membership must be excluded: {memberships:?}"
    );

    let performances: HashSet<&str> = rows
        .iter()
        .filter_map(|r| iri_of(r.get(1).unwrap_or(&None)))
        .collect();
    for expected in [
        "fixtureRagaYamanPerformed1960",
        "fixtureRagaYamanImprovised1975",
        "fixtureRagaYamanPerformed1980",
    ] {
        assert!(
            performances.contains(gm(expected).as_str()),
            "expected performance {expected} in results: {performances:?}"
        );
    }
}
