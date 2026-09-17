// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Empty-class findings cite only their exact native assertion role and world.

use gmeow_logic::explain::{Explanation, ExplanationStep};
use gmeow_logic::reason::dl::EmptyClassAssertion;

use super::derived_quads_for_witness;

const SUBJECT: &str = "urn:verdict:witness";
const WORLD: &str = "urn:verdict:world";
const NOTHING: &str = "https://blackcatinformatics.ca/logic/Nothing";

fn explanation(predicate: &str, object: &str, world: &str, citation: &str) -> Explanation {
    Explanation {
        target_derivation_id: "urn:verdict:derivation".to_owned(),
        target_quad_reifier: "urn:verdict:reifier".to_owned(),
        world_iri: world.to_owned(),
        step_skeleton: vec![ExplanationStep {
            modal_evaluation: None,
            derivation_id: "urn:verdict:derivation".to_owned(),
            rule_iri: "urn:verdict:law".to_owned(),
            quad_reifier: "urn:verdict:reifier".to_owned(),
            subject_iri: SUBJECT.to_owned(),
            predicate_iri: predicate.to_owned(),
            obj_n3: object.to_owned(),
            graph_iri: world.to_owned(),
            term_iris: Vec::new(),
            source_step_ids: Vec::new(),
            is_asserted: false,
            depth: 0,
        }],
        cited_iris: [citation.to_owned()].into_iter().collect(),
    }
}

#[test]
fn finding_derivation_excludes_foreign_worlds_data_predicates_and_the_other_class_role() {
    for (role, predicates, other) in [
        (
            EmptyClassAssertion::Membership,
            [
                "https://blackcatinformatics.ca/logic/instanceOf",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            ],
            "https://blackcatinformatics.ca/logic/subClassOf",
        ),
        (
            EmptyClassAssertion::Subsumption,
            [
                "https://blackcatinformatics.ca/logic/subClassOf",
                "http://www.w3.org/2000/01/rdf-schema#subClassOf",
            ],
            "https://blackcatinformatics.ca/logic/instanceOf",
        ),
    ] {
        for predicate in predicates {
            for object in [NOTHING, "http://www.w3.org/2002/07/owl#Nothing"] {
                let object = format!("<{object}>");
                let mut wrong_step_world = explanation(predicate, &object, WORLD, "urn:wrong-step");
                wrong_step_world.step_skeleton[0].graph_iri = "urn:verdict:foreign".to_owned();
                let unrelated = [
                    explanation(predicate, &object, "urn:verdict:foreign", "urn:foreign"),
                    explanation("urn:verdict:mentions", &object, WORLD, "urn:data"),
                    explanation(other, &object, WORLD, "urn:other-role"),
                    explanation(predicate, &format!("\"{NOTHING}\""), WORLD, "urn:literal"),
                    wrong_step_world,
                ];
                assert_eq!(
                    derived_quads_for_witness(&unrelated, SUBJECT, WORLD, role),
                    None
                );
                let mut selected = unrelated.to_vec();
                selected.push(explanation(predicate, &object, WORLD, "urn:exact-source"));
                assert_eq!(
                    derived_quads_for_witness(&selected, SUBJECT, WORLD, role),
                    Some(vec!["urn:exact-source".to_owned()]),
                    "findings must cite only the selected assertion's evidence"
                );
            }
        }
    }
}
