// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from slices/core/inference/tests/test_inference.py
//! (whole file; the Python file is deleted).
//!
//! The 17 asserted-TBox guards run over the merged ontology (`GraphStore::ontology()`,
//! the native twin of `load_merged_graph(include_imports=False)`). The two SHACL
//! guards validate the slice module + an inline instance against the *slice*
//! `shapes.ttl` (via `parse_shapes` + `validate_dataset`) exactly as the Python
//! `run_shacl(..., shapes_path=_SHAPES)` did. `test_all_examples_parse` →
//! `all_examples_parse`.

use crate::conformance_support::*;
use purrdf::parse_dataset;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};
use purrdf::shapes::report::ValidationReport;
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

const INFERENCE_MODULE: &str = "slices/core/inference/module.ttl";
const INFERENCE_SHAPES: &str = "slices/core/inference/shapes.ttl";

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

// ── SHACL guards against the SLICE shapes ─────────────────────────────────────

const PRELUDE: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <http://example.org/inf/> .
ex:methodReason a gmeow:ObservationMethod .
";

const WELLFORMED: &str = "\
ex:p1 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:concl a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:commit a gmeow:InferenceCommitment ;
    gmeow:premise ex:p1 ;
    gmeow:conclusion ex:concl ;
    gmeow:inferenceModeOf gmeow:modeDeduction .
ex:h1 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason ;
    gmeow:competesWith ex:h2 .
ex:h2 a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
";

const MALFORMED: &str = "\
ex:claimX a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:badCommit a gmeow:InferenceCommitment ;
    gmeow:premise ex:claimX ;
    gmeow:conclusion ex:claimX .
ex:selfRival a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason ;
    gmeow:competesWith ex:selfRival .
ex:selfAttack a gmeow:Attack ;
    gmeow:attackSource ex:claimX ;
    gmeow:attackTarget ex:claimX ;
    gmeow:attackKind gmeow:attackRebut .
ex:claimY a gmeow:StandpointClaim ; gmeow:observationMethod ex:methodReason .
ex:selfArg a gmeow:Argument ; gmeow:argumentConclusion ex:claimY .
ex:componentSelfAttack a gmeow:Attack ;
    gmeow:attackSource ex:selfArg ;
    gmeow:attackTarget ex:claimY ;
    gmeow:attackKind gmeow:attackRebut .
";

/// Validate `slice module + instance` against the *slice* `shapes.ttl` — the twin
/// of `run_shacl(_data(instance), shapes_path=_SHAPES)`.
fn validate_against_slice_shapes(instance_ttl: &str) -> ValidationReport {
    // The real post-migration enforcement surface: the residual hand-authored slice `shapes.ttl`
    // PLUS the projected FOL constraint shapes. The premise≠conclusion cross-node check migrated
    // out of the hand-authored slice shape into a `logic:` RelatumDistinctness axiom projected to
    // `generated/shapes/constraint-shapes.ttl` (design/LOGIC-VALIDATION.md), so the slice-local
    // check must fold that projection in to still exercise it. Turtle concatenation is well-formed
    // (duplicate `@prefix` lines are legal); the FOL constraints fire only on malformed data.
    let mut shapes_ttl =
        fs::read_to_string(repo_root().join(INFERENCE_SHAPES)).expect("inference shapes");
    shapes_ttl.push('\n');
    shapes_ttl.push_str(&authenticated_corpus_text("validate-constraint-shapes.ttl"));
    let shapes = parse_shapes(&shapes_ttl).expect("inference shapes parse");
    let module_nt = ttl_file_to_nt(&repo_root().join(INFERENCE_MODULE));
    let instance_nt = ttl_str_to_nt(&format!("{PRELUDE}{instance_ttl}"));
    let data_nt = format!("{module_nt}\n{instance_nt}");
    let dataset = parse_dataset(data_nt.as_bytes(), "application/n-triples", None)
        .expect("data N-Triples parse");
    validate_dataset(&dataset, &shapes).expect("slice SHACL validation")
}

#[gmeow_test_batch_macros::batch_test]
fn wellformed_commitment_conforms() {
    let report = validate_against_slice_shapes(WELLFORMED);
    assert!(
        ok(&report),
        "well-formed commitment should conform; violations: {:?}",
        violations(&report)
    );
}

#[gmeow_test_batch_macros::batch_test]
fn malformed_commitment_is_flagged() {
    let report = validate_against_slice_shapes(MALFORMED);
    assert!(!ok(&report), "malformed commitment should be flagged");
    let blob = violations(&report).join(" ");
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
fn all_examples_parse() {
    let dir = repo_root().join("slices/core/inference/examples");
    let mut names: BTreeSet<String> = BTreeSet::new();
    for entry in fs::read_dir(&dir).expect("examples dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            names.insert(path.file_name().unwrap().to_string_lossy().into_owned());
            // Parse it — parse_ttl_file panics on malformed Turtle.
            let _ = GraphStore::parse_ttl_file(&path);
        }
    }
    assert!(
        names.contains("argumentation.ttl"),
        "argumentation.ttl missing"
    );
    assert_eq!(names.len(), 6, "expected 6 worked examples; got {names:?}");
}
