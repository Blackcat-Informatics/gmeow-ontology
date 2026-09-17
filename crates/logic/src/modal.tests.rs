// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[derive(Clone)]
struct Fact {
    graph: String,
    subject: String,
    predicate: String,
    object: String,
}

impl ModalFact for Fact {
    fn graph(&self) -> &str {
        &self.graph
    }

    fn subject(&self) -> &str {
        &self.subject
    }

    fn predicate(&self) -> &str {
        &self.predicate
    }

    fn object(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Borrowed(&self.object)
    }
}

fn fact(graph: &str, subject: &str, predicate: &str, object: &str) -> Fact {
    Fact {
        graph: graph.to_owned(),
        subject: subject.to_owned(),
        predicate: predicate.to_owned(),
        object: object.to_owned(),
    }
}

fn modal_frame_at(base: &str, op: &str, relation: &str, atom_worlds: &[&str]) -> Vec<Fact> {
    let frame = format!("{base}/frame");
    let mut facts = vec![
        fact(
            &frame,
            &format!("{base}/F"),
            &format!("https://blackcatinformatics.ca/logic/{op}"),
            &format!("{base}/B"),
        ),
        fact(&frame, &format!("{base}/F"), OVER_ACCESSIBILITY, relation),
        fact(
            &frame,
            &format!("{base}/F"),
            MODAL_EVAL_WORLD,
            &format!("{base}/w0"),
        ),
        fact(
            &frame,
            &format!("{base}/B"),
            ATOM_SUBJECT,
            &format!("{base}/a"),
        ),
        fact(
            &frame,
            &format!("{base}/B"),
            ATOM_PREDICATE,
            &format!("{base}/knows"),
        ),
        fact(
            &frame,
            &format!("{base}/B"),
            ATOM_OBJECT,
            &format!("{base}/b"),
        ),
        fact(
            &frame,
            &format!("{base}/w0"),
            relation,
            &format!("{base}/w1"),
        ),
        fact(
            &frame,
            &format!("{base}/w0"),
            relation,
            &format!("{base}/w2"),
        ),
    ];
    for world in atom_worlds {
        facts.push(fact(
            &format!("{base}/{world}"),
            &format!("{base}/a"),
            &format!("{base}/knows"),
            &format!("{base}/b"),
        ));
    }
    facts
}

fn modal_frame(op: &str, relation: &str, atom_worlds: &[&str]) -> Vec<Fact> {
    modal_frame_at("https://example.org/modal", op, relation, atom_worlds)
}

fn without_access_edges(mut facts: Vec<Fact>, relation: &str) -> Vec<Fact> {
    facts.retain(|fact| {
        fact.subject != "https://example.org/modal/w0" || fact.predicate != relation
    });
    facts
}

fn verdict_with<'a>(verdicts: &'a [ModalVerdict], predicate: &str) -> &'a ModalVerdict {
    verdicts
        .iter()
        .find(|verdict| verdict.predicate == predicate)
        .expect("expected modal verdict")
}

#[test]
fn contextual_formula_ownership_cannot_cross_the_asserting_source_graph() {
    let mut facts = modal_frame("necessarily", TYPED_ACCESSIBILITY[0], &["w1", "w2"]);
    let request_rows = |graph| {
        [
            fact(
                graph,
                "urn:request",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "https://blackcatinformatics.ca/logic/ContextualEvaluationRequest",
            ),
            fact(
                graph,
                "urn:request",
                "https://blackcatinformatics.ca/logic/queryFormula",
                "https://example.org/modal/F",
            ),
        ]
    };
    facts.extend(request_rows("urn:foreign-source"));
    let verdicts = evaluate(&facts).expect("foreign query cannot own this flat frame");
    assert_eq!(
        verdict_with(&verdicts, MODAL_NECESSITY_HOLDS).graph,
        "https://example.org/modal/frame"
    );
    facts.extend(request_rows("https://example.org/modal/frame"));
    let error = evaluate(&facts).expect_err("one source cannot select both evaluation routes");
    assert!(error.message().contains("selected by both"));
}

#[test]
fn all_six_typed_relations_drive_both_modal_operators() {
    for relation in TYPED_ACCESSIBILITY {
        let box_verdicts = evaluate(&modal_frame("necessarily", relation, &["w1", "w2"]))
            .expect("typed necessity evaluation");
        assert_eq!(
            verdict_with(&box_verdicts, MODAL_NECESSITY_HOLDS).object,
            "https://example.org/modal/B",
            "necessity must use {relation}"
        );

        let diamond_verdicts = evaluate(&modal_frame("possibly", relation, &["w1"]))
            .expect("typed possibility evaluation");
        assert_eq!(
            verdict_with(&diamond_verdicts, MODAL_POSSIBILITY_HOLDS).object,
            "https://example.org/modal/B",
            "possibility must use {relation}"
        );
    }
}

#[test]
fn box_holds_when_every_accessible_world_has_the_atom() {
    let verdicts = evaluate(&modal_frame(
        "necessarily",
        "https://blackcatinformatics.ca/logic/epistemicallyPossible",
        &["w1", "w2"],
    ))
    .expect("modal evaluation");
    assert!(verdicts.iter().any(|verdict| {
        verdict.predicate == MODAL_NECESSITY_HOLDS
            && verdict.graph == "https://example.org/modal/frame"
            && verdict.object == "https://example.org/modal/B"
    }));
}

#[test]
fn box_failure_emits_a_counterexample_world() {
    let verdicts = evaluate(&modal_frame(
        "necessarily",
        "https://blackcatinformatics.ca/logic/epistemicallyPossible",
        &["w1"],
    ))
    .expect("modal evaluation");
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.predicate == MODAL_NECESSITY_FAILS)
    );
    let counterexample = verdict_with(&verdicts, MODAL_COUNTEREXAMPLE_WORLD);
    assert_eq!(counterexample.object, "https://example.org/modal/w2");
    assert_eq!(counterexample.premises.len(), 9);
    assert_eq!(counterexample.source_quad_ids.len(), 9);
    assert!(
        !counterexample
            .evaluation
            .positive_premises()
            .iter()
            .any(|p| p.context == "https://example.org/modal/w2"
                && p.predicate == "https://example.org/modal/knows")
    );
}

#[test]
fn deontic_empty_accessible_set_is_undetermined() {
    let facts = without_access_edges(
        modal_frame("necessarily", DEONTICALLY_IDEAL, &[]),
        DEONTICALLY_IDEAL,
    );
    let verdicts = evaluate(&facts).expect("modal evaluation");
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.predicate == MODAL_NECESSITY_UNDETERMINED)
    );
}

#[test]
fn non_deontic_empty_accessible_set_is_vacuously_true() {
    let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
    let facts = without_access_edges(modal_frame("necessarily", relation, &[]), relation);
    let verdicts = evaluate(&facts).expect("modal evaluation");
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.predicate == MODAL_NECESSITY_HOLDS)
    );
}

#[test]
fn diamond_fails_when_no_accessible_world_has_the_atom() {
    let verdicts = evaluate(&modal_frame(
        "possibly",
        "https://blackcatinformatics.ca/logic/epistemicallyPossible",
        &[],
    ))
    .expect("modal evaluation");
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.predicate == MODAL_POSSIBILITY_FAILS)
    );
}

#[test]
fn verdict_identity_carries_the_exact_rule_and_ordered_reifier_recipe() {
    let verdicts = evaluate(&modal_frame(
        "necessarily",
        "https://blackcatinformatics.ca/logic/epistemicallyPossible",
        &["w1"],
    ))
    .expect("modal evaluation");
    let verdict = verdict_with(&verdicts, MODAL_COUNTEREXAMPLE_WORLD);
    assert_eq!(verdict.rule_iri, MODAL_RULE_IRI);
    assert_eq!(
        verdict.source_quad_ids,
        verdict
            .evaluation
            .positive_premises()
            .iter()
            .map(ModalPremise::occurrence_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(verdict.derivation_id, verdict.evaluation.derivation_id());
    assert_eq!(verdict.graph, "https://example.org/modal/frame");
    assert_eq!(verdict.subject, "https://example.org/modal/F");
}

#[test]
fn malformed_frame_hard_fails_on_bare_accessible_from() {
    let err = evaluate(&modal_frame("necessarily", ACCESSIBLE_FROM, &[])).unwrap_err();
    assert!(err.message().contains("prose-only"), "got: {err}");
}

#[test]
fn malformed_frame_hard_fails_on_claim_modal_force_relation() {
    let err = evaluate(&modal_frame(
        "necessarily",
        "https://blackcatinformatics.ca/gmeow/modalForceNecessary",
        &[],
    ))
    .unwrap_err();
    assert!(err.message().contains("modal force"), "got: {err}");
}

#[test]
fn malformed_frame_hard_fails_on_missing_or_duplicate_slots() {
    let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
    let base = modal_frame("necessarily", relation, &["w1", "w2"]);

    for (missing_predicate, expected) in [
        (OVER_ACCESSIBILITY, "overAccessibility"),
        (MODAL_EVAL_WORLD, "modalEvalWorld"),
        (ATOM_SUBJECT, ATOM_SUBJECT),
        (ATOM_PREDICATE, ATOM_PREDICATE),
        (ATOM_OBJECT, ATOM_OBJECT),
    ] {
        let mut facts = base.clone();
        facts.retain(|fact| fact.predicate != missing_predicate);
        let err = evaluate(&facts).unwrap_err();
        assert!(err.message().contains(expected), "got: {err}");
    }

    let mut duplicate_body = base.clone();
    duplicate_body.push(fact(
        "https://example.org/modal/frame",
        "https://example.org/modal/F",
        NECESSARILY,
        "https://example.org/modal/B2",
    ));
    let err = evaluate(&duplicate_body).unwrap_err();
    assert!(err.message().contains("2 body"), "got: {err}");

    let mut duplicate_relation = base;
    duplicate_relation.push(fact(
        "https://example.org/modal/frame",
        "https://example.org/modal/F",
        OVER_ACCESSIBILITY,
        "https://blackcatinformatics.ca/logic/doxasticallyAccessible",
    ));
    let err = evaluate(&duplicate_relation).unwrap_err();
    assert!(err.message().contains("found 2"), "got: {err}");
}

#[test]
fn malformed_frame_hard_fails_without_exactly_one_operator() {
    let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
    let mut no_operator = modal_frame("necessarily", relation, &["w1", "w2"]);
    no_operator.retain(|fact| fact.predicate != NECESSARILY);
    let err = evaluate(&no_operator).unwrap_err();
    assert!(err.message().contains("no logic:necessarily"), "got: {err}");

    let mut both = modal_frame("necessarily", relation, &["w1", "w2"]);
    both.push(fact(
        "https://example.org/modal/frame",
        "https://example.org/modal/F",
        POSSIBLY,
        "https://example.org/modal/B",
    ));
    let err = evaluate(&both).unwrap_err();
    assert!(
        err.message().contains("both logic:necessarily"),
        "got: {err}"
    );
}

#[test]
fn malformed_frame_hard_fails_on_non_iri_ground_atom_binding() {
    let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
    let mut facts = modal_frame("necessarily", relation, &["w1", "w2"]);
    facts.retain(|fact| fact.predicate != ATOM_OBJECT);
    facts.push(fact(
        "https://example.org/modal/frame",
        "https://example.org/modal/B",
        ATOM_OBJECT,
        "\"not-an-iri\"",
    ));
    let err = evaluate(&facts).unwrap_err();
    assert!(err.message().contains("must be an IRI"), "got: {err}");
}

#[test]
fn unrelated_non_iri_typed_edges_are_outside_modal_frame_validation() {
    let sharpens = "https://blackcatinformatics.ca/gmeow/sharpens";
    let unrelated = [
        fact(
            "https://example.org/domain",
            "https://example.org/domain/source",
            sharpens,
            "\"literal target\"",
        ),
        fact(
            "https://example.org/domain",
            "_:blank-source",
            sharpens,
            "<<(<https://example.org/s> <https://example.org/p> <https://example.org/o>)>>",
        ),
    ];
    assert!(
        evaluate(&unrelated)
            .expect("unrelated domain facts are not a modal frame")
            .is_empty()
    );

    let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
    let mut framed = modal_frame("necessarily", relation, &["w1", "w2"]);
    framed.extend(unrelated);
    let verdicts = evaluate(&framed).expect("unrelated sharpens rows do not poison a frame");
    assert!(
        verdicts
            .iter()
            .any(|verdict| verdict.predicate == MODAL_NECESSITY_HOLDS)
    );
}

#[test]
fn malformed_endpoint_on_an_active_frame_edge_aborts_atomically() {
    let relation = "https://blackcatinformatics.ca/gmeow/sharpens";
    let mut facts = without_access_edges(modal_frame("necessarily", relation, &[]), relation);
    facts.push(fact(
        "https://example.org/modal/frame",
        "https://example.org/modal/w0",
        relation,
        "\"not-a-world-iri\"",
    ));
    let err = evaluate(&facts).unwrap_err();
    assert!(
        err.message()
            .contains("typed accessibility edge target world must be an IRI"),
        "got: {err}"
    );
}

#[test]
fn one_malformed_frame_aborts_the_complete_evaluation() {
    let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
    let mut facts = modal_frame_at(
        "https://example.org/valid-modal",
        "necessarily",
        relation,
        &["w1", "w2"],
    );
    let mut malformed = modal_frame_at(
        "https://example.org/malformed-modal",
        "possibly",
        relation,
        &["w1"],
    );
    malformed.retain(|fact| fact.predicate != ATOM_PREDICATE);
    facts.extend(malformed);
    assert!(evaluate(&facts).is_err());
}

#[test]
fn malformed_frame_hard_fails_on_nested_modal_body() {
    let mut facts = modal_frame(
        "necessarily",
        "https://blackcatinformatics.ca/logic/epistemicallyPossible",
        &["w1"],
    );
    facts.push(fact(
        "https://example.org/modal/frame",
        "https://example.org/modal/B",
        NECESSARILY,
        "https://example.org/modal/C",
    ));
    let err = evaluate(&facts).unwrap_err();
    assert!(err.message().contains("nested modal"), "got: {err}");
}
#[test]
fn equal_formula_and_world_iris_do_not_join_distinct_asserting_contexts() {
    let relation = TYPED_ACCESSIBILITY[0];
    let mut facts = modal_frame("necessarily", relation, &["w1"]);
    let mut other = facts.clone();
    other.retain(|fact| {
        fact.graph.ends_with("/frame")
            && !(fact.predicate == relation && fact.object.ends_with("/w2"))
    });
    for fact in &mut other {
        fact.graph = "urn:context:other".to_owned();
    }
    facts.extend(other);
    let verdicts = evaluate(&facts).unwrap();
    assert!(
        verdicts
            .iter()
            .any(|v| v.graph.ends_with("/frame") && v.predicate == MODAL_NECESSITY_FAILS)
    );
    assert!(
        verdicts
            .iter()
            .any(|v| v.graph == "urn:context:other" && v.predicate == MODAL_NECESSITY_HOLDS)
    );
    let identities: BTreeSet<_> = verdicts.iter().map(|v| &v.derivation_id).collect();
    assert_eq!(identities.len(), verdicts.len());
    facts.reverse();
    assert_eq!(evaluate(&facts).unwrap(), verdicts);
}

#[test]
fn split_declarations_and_foreign_body_slots_never_complete_a_frame() {
    for predicate in [
        NECESSARILY,
        OVER_ACCESSIBILITY,
        MODAL_EVAL_WORLD,
        ATOM_SUBJECT,
        ATOM_PREDICATE,
        ATOM_OBJECT,
    ] {
        let mut facts = modal_frame("necessarily", TYPED_ACCESSIBILITY[0], &["w1", "w2"]);
        for fact in &mut facts {
            if fact.predicate == predicate {
                fact.graph = "urn:foreign".to_owned();
            }
        }
        assert!(evaluate(&facts).is_err(), "split {predicate} must fail");
    }
}

#[test]
fn foreign_active_looking_malformed_edge_is_not_a_frame_edge() {
    let relation = TYPED_ACCESSIBILITY[0];
    let mut facts = modal_frame("necessarily", relation, &["w1", "w2"]);
    facts.push(fact(
        "urn:foreign",
        "https://example.org/modal/w0",
        relation,
        "\"not a world\"",
    ));
    assert!(evaluate(&facts).is_ok());
    facts.last_mut().unwrap().graph = "https://example.org/modal/frame".to_owned();
    assert!(evaluate(&facts).is_err());
}

#[test]
fn default_and_blank_asserting_contexts_remain_separate() {
    let mut facts = modal_frame("necessarily", TYPED_ACCESSIBILITY[0], &["w1"]);
    let mut other = facts.clone();
    for fact in &mut facts {
        if fact.graph.ends_with("/frame") {
            fact.graph.clear();
        }
    }
    other.retain(|fact| fact.graph.ends_with("/frame") && !fact.object.ends_with("/w2"));
    for fact in &mut other {
        fact.graph = "_:context".to_owned();
    }
    facts.extend(other);
    let verdicts = evaluate(&facts).unwrap();
    assert!(
        verdicts
            .iter()
            .any(|v| v.graph.is_empty() && v.predicate == MODAL_NECESSITY_FAILS)
    );
    assert!(
        verdicts
            .iter()
            .any(|v| v.graph == "_:context" && v.predicate == MODAL_NECESSITY_HOLDS)
    );
}

#[test]
fn only_explicitly_transferred_declarations_can_complete_the_target_context() {
    let mut facts = modal_frame("necessarily", TYPED_ACCESSIBILITY[0], &["w1", "w2"]);
    let mut target = facts
        .iter()
        .find(|fact| fact.predicate == NECESSARILY)
        .unwrap()
        .clone();
    target.graph = "urn:bridge:target".to_owned();
    facts.push(target);
    assert!(
        evaluate(&facts).is_err(),
        "matching nodes do not authorize a bridge"
    );
    // This kernel consumes admitted derived rows. The caller owns the typed
    // bridge rule and retains its provenance when it publishes these rows.
    let transferred: Vec<_> = facts
        .iter()
        .filter(|fact| fact.graph.ends_with("/frame"))
        .cloned()
        .map(|mut fact| {
            fact.graph = "urn:bridge:target".to_owned();
            fact
        })
        .collect();
    facts.extend(transferred);
    assert!(
        evaluate(&facts)
            .unwrap()
            .iter()
            .any(|v| v.graph == "urn:bridge:target" && v.predicate == MODAL_NECESSITY_HOLDS)
    );
}
