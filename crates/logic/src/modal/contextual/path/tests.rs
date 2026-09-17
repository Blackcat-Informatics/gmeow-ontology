// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::frontend::{FINITE_AT_OR_AFTER, FINITE_NEXT};

fn source(statuses: &[&str]) -> std::sync::Arc<RdfDataset> {
    let mut text = String::from(
        "@prefix l: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix g: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n<urn:world> {\n",
    );
    for (index, status) in statuses.iter().enumerate() {
        text.push_str(&format!("<urn:state:{index}> a l:State .\n"));
        if index > 0 {
            text.push_str(&format!(
                "<urn:state:{index}> l:temporallySucceeds <urn:state:{}> .\n",
                index - 1
            ));
        }
        if !status.is_empty() {
            text.push_str(&format!(
                "<urn:claim:{index}> rdf:reifies <<( <urn:state:{index}> l:situationObtains <urn:situation> )>> ; g:accordingTo <urn:observer> ; g:standpointSupportStatus g:{status} .\n",
            ));
        }
    }
    text.push('}');
    purrdf::parse_dataset(text.as_bytes(), "application/trig", None).unwrap()
}

fn selection(count: usize, finalized: bool) -> PathSelection {
    PathSelection {
        path: "urn:path".into(),
        world: "urn:world".into(),
        standpoint: "urn:observer".into(),
        states: (0..count).map(|i| format!("urn:state:{i}")).collect(),
        finalized,
        evidence_closed: true,
    }
}

fn formula(operator: &str) -> Formula {
    formula_at(operator, "urn:state:0")
}

fn formula_at(operator: &str, anchor: &str) -> Formula {
    let variable = Term::Var("position".into());
    let atom = Formula::Atom {
        relation: Term::Iri(format!("{LOGIC_NAMESPACE}situationObtains")),
        args: vec![
            variable.clone(),
            variable.clone(),
            Term::Iri("urn:situation".into()),
        ],
    };
    let guard = Formula::Atom {
        relation: Term::Iri(
            if operator == "next" {
                FINITE_NEXT
            } else {
                FINITE_AT_OR_AFTER
            }
            .into(),
        ),
        args: vec![Term::Iri(anchor.into()), variable],
    };
    if operator == "globally" {
        Formula::Forall {
            vars: vec!["position".into()],
            body: Box::new(Formula::Implies(Box::new(guard), Box::new(atom))),
        }
    } else {
        Formula::Exists {
            vars: vec!["position".into()],
            body: Box::new(Formula::And(vec![guard, atom])),
        }
    }
}

fn assess(
    dataset: &RdfDataset,
    selected: &PathSelection,
    formula: &Formula,
    budget: Option<u64>,
) -> gmeow_errors::Result<ContextualAssessment> {
    evaluate_path(
        dataset,
        selected,
        PathQuery {
            request: "urn:request",
            formula_identity: "urn:goal",
            anchor: "urn:state:0",
            formula,
        },
        budget,
        None,
    )
}

#[test]
fn path_binding_reads_each_actual_state_and_retains_the_path_basis() {
    let dataset = source(&["supportOpposed", "supportSupported"]);
    let selected = selection(2, true);
    for (operator, expected) in [
        ("next", InformationState::Supported),
        ("eventually", InformationState::Supported),
        ("globally", InformationState::Opposed),
    ] {
        let result = assess(&dataset, &selected, &formula(operator), None).unwrap();
        assert_eq!(result.result.information, expected, "{operator}");
        assert_eq!(result.result.evaluation, EvaluationStatus::Completed);
        assert_eq!(result.temporal_prefixes.len(), 1);
        let TemporalBasis::Path(basis) = &result.temporal_prefixes[0] else {
            panic!("no journal may be fabricated");
        };
        assert_eq!(basis.path, selected.path);
        assert_eq!(basis.world, selected.world);
        assert_eq!(basis.standpoint, selected.standpoint);
        assert!(basis.finalized);
        assert!(
            result
                .inferences
                .iter()
                .flat_map(|proof| &proof.antecedents)
                .any(|identity| identity == "urn:claim:1")
        );
    }
}

#[test]
fn path_unknown_conflict_and_open_tail_remain_independent() {
    let missing = source(&[""]);
    let mut selected = selection(1, false);
    let next = formula("next");
    let open = assess(&missing, &selected, &next, None).unwrap();
    assert_eq!(open.result.information, InformationState::Undetermined);
    selected.finalized = true;
    let closed = assess(&missing, &selected, &next, None).unwrap();
    assert_eq!(closed.result.information, InformationState::Opposed);
    assert_ne!(
        open.result.provenance.contract_hash,
        closed.result.provenance.contract_hash
    );
    let no_claim = assess(&missing, &selected, &formula("eventually"), None).unwrap();
    assert_eq!(no_claim.result.information, InformationState::Neither);
    let both = source(&["supportBoth"]);
    let conflict = assess(&both, &selected, &formula("globally"), None).unwrap();
    assert_eq!(conflict.result.information, InformationState::Both);
    assert!(conflict.result.provenance.proof.is_some());
    assert!(conflict.result.provenance.counterproof.is_some());
}

#[test]
fn path_order_and_membership_fail_closed() {
    let dataset = source(&["supportSupported", "supportSupported", "supportSupported"]);
    for states in [
        vec![],
        vec!["urn:state:0", "urn:state:0"],
        vec!["urn:state:1", "urn:state:0"],
        vec!["urn:state:0", "urn:state:2"],
    ] {
        let mut selected = selection(3, true);
        selected.states = states.into_iter().map(str::to_owned).collect();
        assert!(assess(&dataset, &selected, &formula("eventually"), None).is_err());
    }
}

#[test]
fn path_standpoint_isolation_and_budget_do_not_manufacture_truth() {
    let dataset = source(&["supportSupported", "supportSupported"]);
    let mut selected = selection(2, true);
    let bounded = assess(&dataset, &selected, &formula("globally"), Some(0)).unwrap();
    assert_eq!(bounded.result.evaluation, EvaluationStatus::BudgetExhausted);
    assert!(bounded.result.provenance.proof.is_none());
    selected.standpoint = "urn:another-observer".into();
    let other = assess(&dataset, &selected, &formula("globally"), None).unwrap();
    assert_eq!(other.result.information, InformationState::Neither);
    assert!(other.result.provenance.proof.is_none());
    assert!(other.result.provenance.counterproof.is_none());
}

#[test]
fn prepared_path_shares_evidence_across_goals_and_keeps_each_execution_independent() {
    let dataset = source(&["supportOpposed", "supportSupported"]);
    let selected = selection(2, true);
    let path = PreparedPath::admit(&dataset, &selected).unwrap();
    for (operator, expected) in [
        ("eventually", InformationState::Supported),
        ("globally", InformationState::Opposed),
    ] {
        let formula = formula(operator);
        let query = path
            .prepare(PathQuery {
                request: "urn:request",
                formula_identity: "urn:goal",
                anchor: "urn:state:0",
                formula: &formula,
            })
            .unwrap();
        let complete = query.evaluate(None, None).unwrap();
        assert_eq!(complete.result.information, expected);
        let interrupted = query.evaluate(Some(0), None).unwrap();
        assert_eq!(
            interrupted.result.evaluation,
            EvaluationStatus::BudgetExhausted
        );
        assert!(interrupted.result.provenance.proof.is_none());
        assert!(interrupted.result.provenance.counterproof.is_none());
        let repeated = query.evaluate(None, None).unwrap();
        assert_eq!(complete.result, repeated.result);
        assert_eq!(
            complete.result,
            assess(&dataset, &selected, &formula, None).unwrap().result,
        );
        assert!(
            complete
                .inferences
                .iter()
                .flat_map(|proof| &proof.antecedents)
                .any(|identity| identity
                    == if operator == "eventually" {
                        "urn:claim:1"
                    } else {
                        "urn:claim:0"
                    })
        );
    }
}

#[test]
fn prepared_path_never_reuses_another_observations_evidence() {
    let supported = source(&["supportSupported"]);
    let opposed = source(&["supportOpposed"]);
    let selected = selection(1, true);
    let formula = formula("eventually");
    let mut preparations = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for (dataset, expected) in [
        (&supported, InformationState::Supported),
        (&opposed, InformationState::Opposed),
    ] {
        let path = PreparedPath::admit(dataset, &selected).unwrap();
        let query = path
            .prepare(PathQuery {
                request: "urn:request",
                formula_identity: "urn:goal",
                anchor: "urn:state:0",
                formula: &formula,
            })
            .unwrap();
        assert!(preparations.insert(query.identity().to_owned()));
        let assessment = query.evaluate(None, None).unwrap();
        assert_eq!(assessment.result.information, expected);
        assert!(identities.insert(assessment.result.provenance.contract_hash));
    }
}

fn atomic_at(anchor: &str) -> Formula {
    Formula::Atom {
        relation: Term::Iri(format!("{LOGIC_NAMESPACE}situationObtains")),
        args: vec![
            Term::Iri(anchor.into()),
            Term::Iri(anchor.into()),
            Term::Iri("urn:situation".into()),
        ],
    }
}

#[test]
fn explicit_atomic_and_temporal_anchors_keep_the_full_observation_and_provenance() {
    let dataset = source(&["supportOpposed", "supportSupported"]);
    let selected = selection(2, true);
    let path = PreparedPath::admit(&dataset, &selected).unwrap();
    let mut observation = None;
    for (anchor, expected, witness) in [
        ("urn:state:0", InformationState::Opposed, "urn:claim:0"),
        ("urn:state:1", InformationState::Supported, "urn:claim:1"),
    ] {
        let formula = atomic_at(anchor);
        let prepared = path
            .prepare(PathQuery {
                request: "urn:atomic-request",
                formula_identity: "urn:atomic-goal",
                anchor,
                formula: &formula,
            })
            .unwrap();
        let assessment = prepared.evaluate(None, None).unwrap();
        assert_eq!(assessment.result.information, expected);
        assert_eq!(
            assessment.result.provenance.context.attributed.as_deref(),
            Some(anchor)
        );
        assert_eq!(
            assessment.result.provenance.context.path.as_deref(),
            Some(selected.path.as_str())
        );
        assert_eq!(assessment.result.provenance.context.world, selected.world);
        assert_eq!(
            assessment.result.provenance.context.standpoint.as_deref(),
            Some(selected.standpoint.as_str())
        );
        let proof = if expected == InformationState::Supported {
            assessment.result.provenance.proof.as_ref()
        } else {
            assessment.result.provenance.counterproof.as_ref()
        }
        .expect("the selected state's evidence is retained");
        assert!(proof.cited_iris.contains(witness));
        assert_eq!(assessment.temporal_prefixes.len(), 1);
        let TemporalBasis::Path(basis) = &assessment.temporal_prefixes[0] else {
            panic!("an anchored state query cannot fabricate a journal");
        };
        if let Some(expected) = &observation {
            assert_eq!(
                basis, expected,
                "an anchor does not rebuild or narrow the admitted path"
            );
        } else {
            observation = Some(basis.clone());
        }
        let native = crate::result_rdf::project_contextual_assessment_dataset(&assessment).unwrap();
        let restored =
            crate::result_rdf::parse_reasoning_dataset(&native, GraphMatch::Default).unwrap();
        assert_eq!(restored.provenance, assessment.result.provenance);
    }
    for (anchor, expected) in [
        ("urn:state:0", InformationState::Supported),
        ("urn:state:1", InformationState::Opposed),
    ] {
        let formula = formula_at("next", anchor);
        let prepared = path
            .prepare(PathQuery {
                request: "urn:temporal-request",
                formula_identity: "urn:temporal-goal",
                anchor,
                formula: &formula,
            })
            .unwrap();
        let assessment = prepared.evaluate(None, None).unwrap();
        assert_eq!(assessment.result.information, expected);
        assert_eq!(
            assessment.result.provenance.context.attributed.as_deref(),
            Some(anchor)
        );
        let TemporalBasis::Path(basis) = &assessment.temporal_prefixes[0] else {
            panic!("temporal suffix retains a path observation");
        };
        assert_eq!(Some(basis), observation.as_ref());
    }
}

#[test]
fn preparation_rejects_foreign_and_malformed_anchors_before_execution() {
    let dataset = source(&["supportSupported", "supportSupported", "supportSupported"]);
    let selected = selection(2, true);
    let path = PreparedPath::admit(&dataset, &selected).unwrap();
    let formula = atomic_at("urn:state:0");
    for anchor in ["urn:state:2", "urn:foreign", "relative", ""] {
        let refused = path.prepare(PathQuery {
            request: "urn:request",
            formula_identity: "urn:goal",
            anchor,
            formula: &formula,
        });
        assert!(
            refused.is_err(),
            "anchor must belong to the selected path: {anchor}"
        );
    }
}

#[test]
fn prepared_identity_binds_selectors_anchor_and_observation_separately_from_run_budget() {
    let dataset = source(&["supportSupported", "supportSupported"]);
    let selected = selection(2, true);
    let path = PreparedPath::admit(&dataset, &selected).unwrap();
    // This exact formula explicitly selects the first state. Changing the outer
    // query anchor must still change its prepared and execution identity.
    let formula = atomic_at("urn:state:0");
    let mut preparations = BTreeSet::new();
    let mut receipts = BTreeSet::new();
    for (request, formula_identity, anchor) in [
        ("urn:request", "urn:goal", "urn:state:0"),
        ("urn:other-request", "urn:goal", "urn:state:0"),
        ("urn:request", "urn:other-goal", "urn:state:0"),
        ("urn:request", "urn:goal", "urn:state:1"),
    ] {
        let prepared = path
            .prepare(PathQuery {
                request,
                formula_identity,
                anchor,
                formula: &formula,
            })
            .unwrap();
        assert!(preparations.insert(prepared.identity().to_owned()));
        let complete = prepared.evaluate(None, None).unwrap();
        assert!(receipts.insert(complete.result.provenance.contract_hash.clone()));
        let interrupted = prepared.evaluate(Some(0), None).unwrap();
        assert_eq!(
            interrupted.result.evaluation,
            EvaluationStatus::BudgetExhausted
        );
        assert_ne!(
            complete.result.provenance.contract_hash,
            interrupted.result.provenance.contract_hash
        );
        assert!(interrupted.result.provenance.proof.is_none());
        assert!(interrupted.result.provenance.counterproof.is_none());
        let repeated = prepared.evaluate(None, None).unwrap();
        assert_eq!(complete.result, repeated.result);
        assert_eq!(complete.inferences, repeated.inferences);
        assert_eq!(complete.temporal_prefixes, repeated.temporal_prefixes);
    }
    let alternate_formula = Formula::Not(Box::new(formula.clone()));
    let alternate = path
        .prepare(PathQuery {
            request: "urn:request",
            formula_identity: "urn:goal",
            anchor: "urn:state:0",
            formula: &alternate_formula,
        })
        .unwrap();
    assert!(
        preparations.insert(alternate.identity().to_owned()),
        "typed formula content is committed"
    );
    for changed in [
        PathSelection {
            path: "urn:another-path".into(),
            ..selected.clone()
        },
        PathSelection {
            standpoint: "urn:another-observer".into(),
            ..selected.clone()
        },
        PathSelection {
            finalized: false,
            ..selected.clone()
        },
        PathSelection {
            evidence_closed: false,
            ..selected.clone()
        },
        PathSelection {
            states: vec!["urn:state:0".into()],
            ..selected.clone()
        },
    ] {
        let changed_path = PreparedPath::admit(&dataset, &changed).unwrap();
        let prepared = changed_path
            .prepare(PathQuery {
                request: "urn:request",
                formula_identity: "urn:goal",
                anchor: "urn:state:0",
                formula: &formula,
            })
            .unwrap();
        assert!(
            preparations.insert(prepared.identity().to_owned()),
            "the full observation selector is committed"
        );
        let assessment = prepared.evaluate(None, None).unwrap();
        assert!(receipts.insert(assessment.result.provenance.contract_hash));
    }
}
