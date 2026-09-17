// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from slices/core/inference/tests/test_inference.py
//! (whole file; the Python file is deleted).
//!
//! The 17 asserted-TBox guards run over the merged ontology (`GraphStore::ontology()`,
//! the native twin of `load_merged_graph(include_imports=False)`). The two SHACL
//! guards read producer observations over the exact slice module and generated
//! constraint-shape union. The six-example inventory is retained; syntax-only
//! parser conformance belongs to PurRDF.

use crate::conformance_support::*;
use crate::inference_observations;
use std::collections::BTreeSet;
use std::fs;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const SLICE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/inference";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_ISDEFINEDBY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_SYMMETRIC: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
const OWL_IRREFLEXIVE: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

/// The allowed logic: master metaclasses (exactly one per class — the invariant).
const LOGIC_MASTERS: &[&str] = &[
    "Kind",
    "Category",
    "Relator",
    "Mode",
    "QualityValue",
    "AbstractIndividualType",
    "Phase",
    "Role",
    "SubKind",
    "RoleMixin",
    "PhaseMixin",
    "Mixin",
    "Event",
    "Situation",
    "Disposition",
];

const CLASSES: &[&str] = &[
    "InferenceProcess",
    "InferenceCommitment",
    "Analogy",
    "Correspondence",
    "InferenceMode",
    "InferenceTenure",
    "Argument",
    "ArgumentEvaluation",
    "Attack",
    "AttackKind",
    "AttackTarget",
    "AcceptanceStatus",
    "PremiseUse",
    "InferenceApplication",
    "Support",
    "SupportSource",
];

fn g(local: &str) -> String {
    format!("{G}{local}")
}
fn lg(local: &str) -> String {
    format!("{LOGIC}{local}")
}

// ── Exactly-one logic-master invariant + slice definedness ────────────────────

#[gmeow_test_batch_macros::batch_test]
fn every_class_has_exactly_one_logic_metaclass() {
    let store = GraphStore::ontology();
    let masters: BTreeSet<String> = LOGIC_MASTERS.iter().map(|m| lg(m)).collect();
    for cls in CLASSES {
        let meta: Vec<String> = store
            .objects(&g(cls), RDF_TYPE)
            .into_iter()
            .filter(|t| masters.contains(t))
            .collect();
        assert_eq!(
            meta.len(),
            1,
            "{cls} must carry exactly one logic master metaclass, got {meta:?}"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn all_terms_defined_by_inference_slice() {
    let store = GraphStore::ontology();
    for cls in CLASSES {
        assert!(
            store.has(Some(&g(cls)), Some(RDFS_ISDEFINEDBY), Some(SLICE_IRI)),
            "{cls} is not rdfs:isDefinedBy the inference slice"
        );
    }
}

// ── The endurant/occurrent split ──────────────────────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn inference_process_is_eventtype_under_mental_process() {
    let s = GraphStore::ontology();
    let ip = g("InferenceProcess");
    assert!(s.has(Some(&ip), Some(RDF_TYPE), Some(&lg("Event"))));
    assert!(s.has(Some(&ip), Some(RDFS_SUBCLASSOF), Some(&g("MentalProcess"))));
    assert!(
        !s.has(Some(&ip), Some(RDFS_SUBCLASSOF), Some(&lg("Relator"))),
        "InferenceProcess must not also be a Relator (rejected double-typing)"
    );
}

#[gmeow_test_batch_macros::batch_test]
fn inference_commitment_is_relator_kind() {
    let s = GraphStore::ontology();
    let ic = g("InferenceCommitment");
    assert!(s.has(Some(&ic), Some(RDF_TYPE), Some(&lg("Kind"))));
    assert!(s.has(Some(&ic), Some(RDFS_SUBCLASSOF), Some(&lg("Relator"))));
    assert!(
        !s.has(Some(&ic), Some(RDFS_SUBCLASSOF), Some(&g("MentalProcess"))),
        "InferenceCommitment must stay off the occurrent side"
    );
}

#[gmeow_test_batch_macros::batch_test]
fn relator_classes_carry_relator_only_via_subclassof() {
    let s = GraphStore::ontology();
    for cls in ["Analogy", "Correspondence"] {
        assert!(s.has(Some(&g(cls)), Some(RDF_TYPE), Some(&lg("Kind"))));
        assert!(s.has(Some(&g(cls)), Some(RDFS_SUBCLASSOF), Some(&lg("Relator"))));
        assert!(
            !s.has(Some(&g(cls)), Some(RDF_TYPE), Some(&lg("Relator"))),
            "{cls} must be a Relator by subClassOf, not by direct typing"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn inference_tenure_is_situation_under_timescoped() {
    let s = GraphStore::ontology();
    let it = g("InferenceTenure");
    assert!(s.has(Some(&it), Some(RDF_TYPE), Some(&lg("Situation"))));
    assert!(s.has(
        Some(&it),
        Some(RDFS_SUBCLASSOF),
        Some(&g("TimeScopedRelation"))
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn value_vocabs_are_abstract_individual_types() {
    let s = GraphStore::ontology();
    for cls in ["InferenceMode", "AttackKind", "AcceptanceStatus"] {
        assert!(s.has(
            Some(&g(cls)),
            Some(RDF_TYPE),
            Some(&lg("AbstractIndividualType"))
        ));
        assert!(s.has(
            Some(&g(cls)),
            Some(RDFS_SUBCLASSOF),
            Some(&lg("QualityValue"))
        ));
    }
}

// ── Value individuals ─────────────────────────────────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn mode_individuals_typed() {
    let s = GraphStore::ontology();
    for mode in [
        "modeDeduction",
        "modeInduction",
        "modeAbduction",
        "modeAnalogical",
    ] {
        assert!(s.has(Some(&g(mode)), Some(RDF_TYPE), Some(&g("InferenceMode"))));
    }
}

#[gmeow_test_batch_macros::batch_test]
fn attack_kind_individuals_typed() {
    let s = GraphStore::ontology();
    for kind in ["attackUndermine", "attackUndercut", "attackRebut"] {
        assert!(s.has(Some(&g(kind)), Some(RDF_TYPE), Some(&g("AttackKind"))));
    }
}

#[gmeow_test_batch_macros::batch_test]
fn acceptance_status_individuals_typed() {
    let s = GraphStore::ontology();
    for status in ["acceptanceIn", "acceptanceOut", "acceptanceUndecided"] {
        assert!(s.has(
            Some(&g(status)),
            Some(RDF_TYPE),
            Some(&g("AcceptanceStatus"))
        ));
    }
}

// ── Property domains / ranges / characteristics ───────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn flat_spine_properties_domain_claim() {
    let s = GraphStore::ontology();
    for prop in ["inferredFrom", "inferenceMode"] {
        assert!(s.has(
            Some(&g(prop)),
            Some(RDFS_DOMAIN),
            Some(&g("StandpointClaim"))
        ));
    }
}

#[gmeow_test_batch_macros::batch_test]
fn reified_slots_domain_commitment() {
    let s = GraphStore::ontology();
    for prop in ["premise", "conclusion", "inferenceModeOf", "warrant"] {
        assert!(s.has(
            Some(&g(prop)),
            Some(RDFS_DOMAIN),
            Some(&g("InferenceCommitment"))
        ));
    }
}

#[gmeow_test_batch_macros::batch_test]
fn bridge_links_process_to_commitment() {
    let s = GraphStore::ontology();
    assert!(s.has(
        Some(&g("hasInferenceCommitment")),
        Some(RDFS_DOMAIN),
        Some(&g("InferenceProcess"))
    ));
    assert!(s.has(
        Some(&g("hasInferenceCommitment")),
        Some(RDFS_RANGE),
        Some(&g("InferenceCommitment"))
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn functional_properties() {
    let s = GraphStore::ontology();
    for prop in [
        "conclusion",
        "inferenceModeOf",
        "correspondingSource",
        "correspondingTarget",
        "tenureOf",
    ] {
        assert!(
            s.is_functional_carrier(&g(prop)),
            "{prop} must carry a logic: functionalProperty characteristic"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn competes_with_is_symmetric_claim_to_claim() {
    let s = GraphStore::ontology();
    let cw = g("competesWith");
    assert!(s.has(Some(&cw), Some(RDF_TYPE), Some(OWL_SYMMETRIC)));
    assert!(s.has(Some(&cw), Some(RDFS_DOMAIN), Some(&g("StandpointClaim"))));
    assert!(s.has(Some(&cw), Some(RDFS_RANGE), Some(&g("StandpointClaim"))));
    // Irreflexivity is enforced in SHACL, NOT as an OWL axiom (DL-clean).
    assert!(!s.has(Some(&cw), Some(RDF_TYPE), Some(OWL_IRREFLEXIVE)));
}

#[gmeow_test_batch_macros::batch_test]
fn conclusion_ranges_over_standpoint_claim() {
    let s = GraphStore::ontology();
    assert!(s.has(
        Some(&g("conclusion")),
        Some(RDFS_RANGE),
        Some(&g("StandpointClaim"))
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn solver_layer_scores_are_decimal() {
    let s = GraphStore::ontology();
    for prop in ["explanatoryScore", "systematicity"] {
        assert!(s.has(Some(&g(prop)), Some(RDFS_RANGE), Some(XSD_DECIMAL)));
    }
}

#[gmeow_test_batch_macros::batch_test]
fn wellformed_commitment_conforms() {
    let report = inference_observations::report("wellformed");
    assert!(
        report.violations().is_empty(),
        "well-formed commitment should conform; violations: {:?}",
        report.violations()
    );
}

#[gmeow_test_batch_macros::batch_test]
fn malformed_commitment_is_flagged() {
    let report = inference_observations::report("malformed");
    assert!(
        !report.violations().is_empty(),
        "malformed commitment should be flagged"
    );
    let blob = report.violations().join(" ");
    for needle in [
        // The premise≠conclusion and no-self-attack checks now project from the logic:
        // RelatumDistinctness constraints ("… must be distinct"), replacing the legacy sh:sparql
        // "attack itself" prose; the argument-component self-attack rides the RoleCompositionExclusion
        // family ("… as one of its own components").
        "must be distinct",
        "irreflexive",
        "own component",
    ] {
        assert!(
            blob.contains(needle),
            "expected {needle:?} in violations; got: {blob}"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn all_six_worked_examples_remain_registered() {
    let dir = repo_root().join("slices/core/inference/examples");
    let mut names: BTreeSet<String> = BTreeSet::new();
    for entry in fs::read_dir(&dir).expect("examples dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            names.insert(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(
        names.contains("argumentation.ttl"),
        "argumentation.ttl missing"
    );
    assert_eq!(names.len(), 6, "expected 6 worked examples; got {names:?}");
}
