// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic::reason::InferredAxiom;
use gmeow_logic::result::{
    CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
    ReasoningResult, ResultPayload, ResultProvenance,
};
use gmeow_ns::LOGIC_NS;
use purrdf::TermValue;

const WORLD: &str = "https://example.org/term-panel/world";
const CAT: &str = "https://blackcatinformatics.ca/gmeow/Cat";
const ANIMAL: &str = "https://blackcatinformatics.ca/gmeow/Animal";
const MAMMAL: &str = "https://blackcatinformatics.ca/gmeow/Mammal";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// A tiny conclusion projection with explicit source premises; never a corpus build.
fn native_upstream(axioms: Vec<InferredAxiom>) -> BTreeMap<String, StageProduct> {
    let result = ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::Incomplete,
        PreservationClaim::exact(),
        InformationState::Undetermined,
        ResultProvenance::native("term-panel-control", WORLD),
        ResultPayload::Inferred(axioms),
    );
    let reason = crate::stages::docs_render::reasoning_tests::product(result);
    BTreeMap::from([("stage-reason".to_owned(), reason)])
}

fn derived(object: TermValue) -> InferredAxiom {
    InferredAxiom {
        subject: CAT.to_owned(),
        predicate: SUBCLASS.to_owned(),
        object,
        world: WORLD.to_owned(),
        is_edb: false,
        rule_name: Some("el:subClassOf-transitive".to_owned()),
        premises: vec![(CAT.to_owned(), SUBCLASS.to_owned(), format!("<{MAMMAL}>"))],
        modal_evaluation: None,
    }
}

/// Every native head and premise joins once, retaining the actual source world.
#[test]
fn native_term_panels_from_reason_preserve_multiple_heads_sources_and_worlds() {
    let mut edb = derived(TermValue::iri("urn:asserted-only"));
    edb.is_edb = true;
    edb.rule_name = None;
    let upstream = native_upstream(vec![
        derived(TermValue::iri(ANIMAL)),
        derived(TermValue::iri("urn:companion")),
        edb,
    ]);
    let terms = [
        CAT,
        ANIMAL,
        MAMMAL,
        "urn:companion",
        "urn:unrelated",
        "urn:asserted-only",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let panels = term_entailments_from_upstream(&upstream, &terms).unwrap();
    assert_eq!(panels[CAT].len(), 2);
    assert_eq!(panels[MAMMAL].len(), 2);
    assert_eq!(panels[ANIMAL].len(), 1);
    assert_eq!(panels["urn:companion"].len(), 1);
    assert!(!panels.contains_key("urn:unrelated"));
    assert!(!panels.contains_key("urn:asserted-only"));
    for panel in &panels[CAT] {
        assert!(panel.conclusion.contains(WORLD));
        assert!(panel.premises[0].contains(WORLD));
        assert!(panel.premises[0].contains(MAMMAL));
        assert!(!panel.rule.is_empty());
    }
}

/// Modal identities are owned by native evaluation, with each premise world retained.
#[test]
fn modal_term_panels_use_native_evidence_without_a_derivation_namespace_assumption() {
    use gmeow_logic::modal::{ModalEvaluation, ModalFrontier, ModalOp, ModalWorldEvidence};
    let evidence = ModalEvaluation {
        context: WORLD.to_owned(),
        formula: CAT.to_owned(),
        operator: ModalOp::Box,
        body: ANIMAL.to_owned(),
        evaluation_world: "urn:world:here".to_owned(),
        accessibility_relation: format!("{LOGIC_NS}epistemicallyPossible"),
        atom_subject: "urn:alice".to_owned(),
        atom_predicate: "urn:knows".to_owned(),
        atom_object: "urn:bob".to_owned(),
        frontier: ModalFrontier::CompletedFinitePredecessor {
            worlds: vec![ModalWorldEvidence {
                world: "urn:world:there".to_owned(),
                atom_present: true,
            }],
        },
        conclusion_predicate: format!("{LOGIC_NS}modalNecessityHolds"),
        conclusion_object: ANIMAL.to_owned(),
    };
    assert!(
        evidence
            .derivation_id()
            .starts_with("https://blackcatinformatics.ca/logic/evidence/")
    );
    let axiom = InferredAxiom {
        subject: evidence.formula.clone(),
        predicate: evidence.conclusion_predicate.clone(),
        object: TermValue::iri(&evidence.conclusion_object),
        world: evidence.context.clone(),
        is_edb: false,
        rule_name: Some(format!("{LOGIC_NS}rule/modal-evaluation")),
        premises: evidence
            .positive_premises()
            .into_iter()
            .map(|p| (p.subject, p.predicate, p.object))
            .collect(),
        modal_evaluation: Some(Box::new(evidence)),
    };
    let terms = [CAT.to_owned(), ANIMAL.to_owned()].into_iter().collect();
    let panels = term_entailments_from_upstream(&native_upstream(vec![axiom]), &terms).unwrap();
    assert_eq!(panels[CAT].len(), 1);
    assert!(panels[CAT][0].conclusion.contains(WORLD));
    assert!(
        panels[CAT][0]
            .premises
            .iter()
            .any(|p| p.contains("urn:world:there") && p.contains("urn:knows"))
    );
}

/// Quoted conclusions keep nested term joins and directional literal identity natively.
#[test]
fn native_term_panels_preserve_rdf12_quoted_and_directional_values() {
    let value = TermValue::Triple {
        s: TermValue::iri("urn:nested:subject").into(),
        p: TermValue::iri("urn:nested:predicate").into(),
        o: TermValue::Literal {
            lexical_form: "مرحبا".to_owned(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".to_owned(),
            language: Some("ar".to_owned()),
            direction: Some(purrdf::RdfTextDirection::Rtl),
        }
        .into(),
    };
    let terms = [
        "urn:nested:subject".to_owned(),
        "urn:nested:predicate".to_owned(),
    ]
    .into_iter()
    .collect();
    let panels =
        term_entailments_from_upstream(&native_upstream(vec![derived(value)]), &terms).unwrap();
    assert_eq!(panels["urn:nested:predicate"].len(), 1);
    let conclusion = &panels["urn:nested:subject"][0].conclusion;
    assert!(conclusion.contains("مرحبا"));
    assert!(conclusion.contains("ar--rtl"));
}

/// A serialized explanation cannot substitute for a missing authenticated native handle.
#[test]
fn term_panels_refuse_missing_native_source_even_when_explanation_bytes_exist() {
    let terms = [CAT.to_owned()].into_iter().collect();
    assert!(term_entailments_from_upstream(&BTreeMap::new(), &terms).is_err());
    let native = native_upstream(vec![derived(TermValue::iri(ANIMAL))]);
    let wrong_owner = StageProduct::from_bundle(
        "stage-snapshot",
        std::sync::Arc::clone(native["stage-reason"].bundle()),
    )
    .expect("the stage carrier digests");
    assert!(
        term_entailments_from_upstream(
            &BTreeMap::from([("stage-reason".to_owned(), wrong_owner)]),
            &terms
        )
        .is_err(),
        "a different retained owner must never be probed as a fallback"
    );
    let serialized = StageProduct::from_artifacts(
        "stage-reason",
        BTreeMap::from([(
            crate::stages::reason::EXPLANATIONS_PATH.to_owned(),
            b"outward artifact only".to_vec(),
        )]),
    )
    .expect("the stage carrier digests");
    assert!(
        term_entailments_from_upstream(
            &BTreeMap::from([("stage-reason".to_owned(), serialized)]),
            &terms
        )
        .is_err()
    );
}

/// Production documentation retains reason before the snapshot can exist.
#[test]
fn term_panels_follow_the_actual_documentation_stage_dependencies() {
    use crate::node::Stage;
    use crate::stages::{docs_render::DocsRenderStage, reason::ReasonStage};

    let docs = DocsRenderStage::new();
    let reason = ReasonStage::new();
    let snapshot = SnapshotStage::new();
    assert!(docs.carrier_consumes().iter().any(|id| id == reason.id()));
    assert!(!docs.consumes().iter().any(|id| id == snapshot.id()));
    assert!(snapshot.consumes().iter().any(|id| id == docs.id()));

    let product = native_upstream(vec![derived(TermValue::iri(ANIMAL))])
        .remove(reason.id())
        .unwrap();
    let upstream = BTreeMap::from([(reason.id().to_owned(), product)]);
    let terms = [CAT.to_owned()].into_iter().collect();
    assert_eq!(
        term_entailments_from_upstream(&upstream, &terms).unwrap()[CAT].len(),
        1
    );
    let released = upstream
        .into_iter()
        .map(|(id, product)| (id, product.into_carrier_released().unwrap()))
        .collect();
    assert!(term_entailments_from_upstream(&released, &terms).is_err());
}

/// Missing native firing identity is a hard refusal, never an empty display rule.
#[test]
fn native_term_panels_refuse_an_inferred_row_without_its_rule() {
    let mut axiom = derived(TermValue::iri(ANIMAL));
    axiom.rule_name = None;
    let terms = [CAT.to_owned()].into_iter().collect();
    let error = term_entailments_from_upstream(&native_upstream(vec![axiom]), &terms).unwrap_err();
    assert!(error.message().contains("no firing rule"));
}
