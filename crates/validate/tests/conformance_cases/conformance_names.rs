// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_names.py
//!
//! Structural + DL-safety guards for the names building block, ported to native
//! `#[test]` fns over `GraphStore::ontology()` (the native twin of the merged
//! `load_merged_graph(include_imports=False)` graph) plus the fixture-only SHACL
//! harness (`validate`, the twin of Python `run_shacl`).
//!
//! Python-fn → Rust-fn mapping:
//!   - `test_place_naming_is_defined_class` → [`place_naming_is_defined_class`]
//!     (the canonical blank-node walk: equivalentClass → intersectionOf list →
//!     blank `owl:Restriction` members).
//!   - `test_seeded_pronoun_sets_have_five_forms` → [`seeded_pronoun_sets_have_five_forms`]
//!   - `test_pronoun_name_only_value_exists` → [`pronoun_name_only_value_exists`]
//!   - `test_audience_and_standpoint_are_distinct` → [`audience_and_standpoint_are_distinct`]
//!   - `test_appellation_umbrella_and_structural_subclasses` → [`appellation_umbrella_and_structural_subclasses`]
//!   - `test_has_title_subproperty_of_hasappellation` → [`has_title_subproperty_of_hasappellation`]
//!   - `test_has_software_name_subproperty_of_hasappellation` → [`has_software_name_subproperty_of_hasappellation`]
//!   - `test_contested_name_usage_coexists` → [`contested_name_usage_coexists`]

use crate::conformance_support::*;
use purrdf::slice::rdf_query::{Object, Subject};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";

const EX_NAMES: &str = "https://blackcatinformatics.ca/gmeow/examples/names/";

fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// The maximal source-cited anchor inventory of stably-declinable English pronoun
/// sets; each MUST carry all five functional forms.
const DECLINABLE_PRONOUN_ANCHORS: [&str; 21] = [
    "pronounSheHer",
    "pronounHeHim",
    "pronounTheyThem",
    "pronounItIts",
    "pronounXeXem",
    "pronounZeHir",
    "pronounEyEm",
    "pronounEEm",
    "pronounZeZir",
    "pronounFaeFaer",
    "pronounAeAer",
    "pronounVeVer",
    "pronounViVir",
    "pronounPerPer",
    "pronounNeNem",
    "pronounThonThon",
    "pronounCoCos",
    "pronounHuHum",
    "pronounKiKin",
    "pronounZheZher",
    "pronounOneOne",
];

/// Non-specifying values — they assert a stance, not a declension, so they carry
/// no five forms by design.
const NON_SPECIFYING_PRONOUNS: [&str; 3] = ["pronounAny", "pronounAsk", "pronounNameOnly"];

// ── The canonical blank-node case ─────────────────────────────────────────────

/// Twin of `test_place_naming_is_defined_class`.
///
/// PlaceNaming reuses the NameUsage relator as a DEFINED class
/// (≡ NameUsage ⊓ ∃usageNamed.Place) — the first `owl:equivalentClass` defined
/// class in the ontology. The chain walked is:
/// `value_h(PlaceNaming, owl:equivalentClass)` → Blank → `object_as_subject`
/// → `value_h(_, owl:intersectionOf)` → Blank → `object_as_subject`
/// → `rdf_list_h` → for each member `object_as_subject` → `restriction_matches`.
#[gmeow_test_batch_macros::batch_test]
fn place_naming_is_defined_class() {
    let g = GraphStore::ontology();
    let pn = gmeow("PlaceNaming");
    assert!(g.has(Some(&pn), Some(RDF_TYPE), Some(OWL_CLASS)));
    assert!(g.has(Some(&pn), Some(RDFS_SUBCLASS_OF), Some(&gmeow("NameUsage"))));

    let pn_subject = Subject::Named(pn);
    let mut found = false;
    // Iterate every equivalentClass body (Python iterated `graph.objects(...)`).
    for eq in g.objects_h(&pn_subject, OWL_EQUIVALENT_CLASS) {
        let Some(eq_subject) = GraphStore::object_as_subject(&eq) else {
            continue;
        };
        let Some(inter) = g.value_h(&eq_subject, OWL_INTERSECTION_OF) else {
            continue;
        };
        let Some(inter_head) = GraphStore::object_as_subject(&inter) else {
            continue;
        };
        let members = g.rdf_list_h(&inter_head);
        let has_nameusage = members
            .iter()
            .any(|m| matches!(m, Object::Named(iri) if *iri == gmeow("NameUsage")));
        let has_place_restriction = members
            .iter()
            .filter_map(GraphStore::object_as_subject)
            .any(|member| {
                g.restriction_matches(
                    &member,
                    &gmeow("usageNamed"),
                    OWL_SOME_VALUES_FROM,
                    &gmeow("Place"),
                )
            });
        if has_nameusage && has_place_restriction {
            found = true;
        }
    }
    assert!(
        found,
        "PlaceNaming ≡ NameUsage ⊓ ∃usageNamed.Place must be defined"
    );
}

// ── PronounSet five-forms coverage ────────────────────────────────────────────

/// Twin of `test_seeded_pronoun_sets_have_five_forms`.
#[gmeow_test_batch_macros::batch_test]
fn seeded_pronoun_sets_have_five_forms() {
    let g = GraphStore::ontology();
    let forms = [
        "pronounSubject",
        "pronounObject",
        "pronounPossessiveDeterminer",
        "pronounPossessive",
        "pronounReflexive",
    ];
    let pronoun_set = gmeow("PronounSet");
    for anchor in DECLINABLE_PRONOUN_ANCHORS {
        let node = gmeow(anchor);
        assert!(
            g.has(Some(&node), Some(RDF_TYPE), Some(&pronoun_set)),
            "{anchor} is not a PronounSet"
        );
        assert!(
            g.has(Some(&node), Some(RDFS_LABEL), None),
            "{anchor} lacks a label"
        );
        for form in forms {
            assert!(
                g.has(Some(&node), Some(&gmeow(form)), None),
                "{anchor} is missing {form}"
            );
        }
    }
}

/// Twin of `test_pronoun_name_only_value_exists`.
#[gmeow_test_batch_macros::batch_test]
fn pronoun_name_only_value_exists() {
    let g = GraphStore::ontology();
    let name_only = gmeow("pronounNameOnly");
    let pronoun_set = gmeow("PronounSet");
    assert!(g.has(Some(&name_only), Some(RDF_TYPE), Some(&pronoun_set)));
    assert!(g.has(Some(&name_only), Some(RDFS_LABEL), None));
    // Distinct individuals — not collapsed onto pronounAny / pronounAsk.
    assert_ne!(name_only, gmeow("pronounAny"));
    assert_ne!(name_only, gmeow("pronounAsk"));
    // No declined forms (each asserts the ABSENCE of a pronoun set).
    for value in NON_SPECIFYING_PRONOUNS {
        let node = gmeow(value);
        assert!(g.has(Some(&node), Some(RDF_TYPE), Some(&pronoun_set)));
        assert!(
            !g.has(Some(&node), Some(&gmeow("pronounSubject")), None),
            "{value} must carry no pronounSubject form"
        );
    }
}

// ── Standpoint coexistence + audience/standpoint distinction ──────────────────

/// Twin of `test_audience_and_standpoint_are_distinct`.
#[gmeow_test_batch_macros::batch_test]
fn audience_and_standpoint_are_distinct() {
    let g = GraphStore::ontology();
    let audience = gmeow("usageAudience");
    let according_to = gmeow("accordingTo");
    assert!(!g.has(
        Some(&audience),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&according_to)
    ));
    assert!(!g.has(
        Some(&according_to),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&audience)
    ));
    assert!(!g.has(
        Some(&audience),
        Some(OWL_EQUIVALENT_PROPERTY),
        Some(&according_to)
    ));
}

/// Twin of `test_contested_name_usage_coexists`.
///
/// Two standpoint-indexed NameUsage claims on the same person load, SHACL-pass
/// (fixture-only `validate`, the twin of Python `run_shacl`), and are BOTH
/// retained — neither is the ground truth.
#[gmeow_test_batch_macros::batch_test]
fn contested_name_usage_coexists() {
    let path = repo_root().join("tests/fixtures/coverage/names-contested.ttl");
    let nt = ttl_file_to_nt(&path);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "names-contested fixture must SHACL-pass; violations: {:?}",
        violations(&report)
    );

    let g = GraphStore::parse_ttl_file(&path);
    let person = format!("{EX_NAMES}person");
    let names = g.objects(&person, &gmeow("hasName"));
    assert!(
        names.contains(&format!("{EX_NAMES}chosenName")),
        "chosenName must be retained; got: {names:?}"
    );
    assert!(
        names.contains(&format!("{EX_NAMES}legalName")),
        "legalName must be retained; got: {names:?}"
    );
}

// ── Appellation umbrella + hasAppellation specializations ─────────────────────

/// Twin of `test_appellation_umbrella_and_structural_subclasses`.
#[gmeow_test_batch_macros::batch_test]
fn appellation_umbrella_and_structural_subclasses() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gmeow("Appellation")),
        Some(RDFS_SUBCLASS_OF),
        Some(&gmeow("InformationObject"))
    ));
    for sub in [
        "PersonName",
        "Filename",
        "PlaceName",
        "OrganizationName",
        "CreativeWorkTitle",
        "AgreementName",
        "SoftwareName",
    ] {
        assert!(
            g.has(
                Some(&gmeow(sub)),
                Some(RDFS_SUBCLASS_OF),
                Some(&gmeow("Appellation"))
            ),
            "{sub} must be a subclass of Appellation"
        );
    }
}

/// Twin of `test_has_title_subproperty_of_hasappellation`.
#[gmeow_test_batch_macros::batch_test]
fn has_title_subproperty_of_hasappellation() {
    let g = GraphStore::ontology();
    let ht = gmeow("hasTitle");
    assert!(g.has(Some(&ht), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)));
    assert!(g.has(
        Some(&ht),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&gmeow("hasAppellation"))
    ));
    assert!(g.has(Some(&ht), Some(RDFS_DOMAIN), Some(&gmeow("CreativeWork"))));
    assert!(g.has(
        Some(&ht),
        Some(RDFS_RANGE),
        Some(&gmeow("CreativeWorkTitle"))
    ));
}

/// Twin of `test_has_software_name_subproperty_of_hasappellation`.
#[gmeow_test_batch_macros::batch_test]
fn has_software_name_subproperty_of_hasappellation() {
    let g = GraphStore::ontology();
    let hsn = gmeow("hasSoftwareName");
    assert!(g.has(Some(&hsn), Some(RDF_TYPE), Some(OWL_OBJECT_PROPERTY)));
    assert!(g.has(
        Some(&hsn),
        Some(RDFS_SUB_PROPERTY_OF),
        Some(&gmeow("hasAppellation"))
    ));
    // Domain is inherited from hasAppellation (Entity), not restricted to
    // SoftwareProject, so both projects and products can bear software names.
    assert!(!g.has(
        Some(&hsn),
        Some(RDFS_DOMAIN),
        Some(&gmeow("SoftwareProject"))
    ));
    assert!(g.has(Some(&hsn), Some(RDFS_RANGE), Some(&gmeow("SoftwareName"))));
}
