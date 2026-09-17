// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// Tiny caller-owned modal source records; no repository corpus is produced.
fn source(owner: &str, subject: &str, predicate: &str, object: &str) -> RefutationPremise {
    RefutationPremise {
        subject: TermValue::iri(subject),
        predicate: predicate.into(),
        object: TermValue::iri(object),
        graph: Some(TermValue::iri(owner)),
    }
}

fn selected() -> BTreeMap<String, Arc<[RefutationPremise]>> {
    let rows = [
        ("urn:formula", NECESSARILY, "urn:body"),
        (
            "urn:formula",
            OVER_ACCESSIBILITY,
            super::super::DEONTICALLY_IDEAL,
        ),
        ("urn:formula", MODAL_EVAL_WORLD, "urn:evaluation"),
        ("urn:body", ATOM_SUBJECT, "urn:individual"),
        ("urn:body", ATOM_PREDICATE, "urn:property"),
        ("urn:body", ATOM_OBJECT, "urn:value"),
        (
            "urn:evaluation",
            super::super::DEONTICALLY_IDEAL,
            "urn:empty",
        ),
    ]
    .into_iter()
    .map(|(s, p, o)| source("urn:owner", s, p, o))
    .collect::<Vec<_>>();
    BTreeMap::from([("urn:owner".into(), rows.into())])
}

#[test]
fn original_empty_endpoint_has_its_own_native_graph_and_no_domain_selection() {
    let program = NativeModalProgram::prepare(&selected()).unwrap();
    assert_eq!(
        program.required_worlds().get("urn:empty"),
        Some(&LogicalGraph::Named(TermValue::iri("urn:empty")))
    );
    assert_eq!(
        program.required_worlds().get("urn:evaluation"),
        Some(&LogicalGraph::Named(TermValue::iri("urn:evaluation")))
    );
    assert!(!program.required_worlds().contains_key("urn:owner"));
    let effects = program.effects();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].owner, "urn:owner");
    assert_eq!(effects[0].read_worlds.len(), effects[0].effect.reads.len());
    assert!(
        effects[0]
            .read_worlds
            .iter()
            .any(|world| world == "urn:empty")
    );
    let verdicts = evaluate_frame(
        &program.frames[0],
        vec![ModalWorldEvidence {
            world: "urn:empty".into(),
            atom_present: false,
        }],
    )
    .unwrap();
    assert_eq!(verdicts[0].predicate, MODAL_NECESSITY_FAILS);
    assert_eq!(verdicts[1].predicate, MODAL_COUNTEREXAMPLE_WORLD);
    assert!(verdicts.iter().all(|verdict| verdict.graph == "urn:owner"));
}

#[test]
fn malformed_original_frame_and_selected_edge_refuse_before_execution() {
    let mut sources = selected();
    let mut rows = sources["urn:owner"].to_vec();
    rows.push(source("urn:owner", "urn:formula", POSSIBLY, "urn:body"));
    sources.insert("urn:owner".into(), rows.into());
    assert!(NativeModalProgram::prepare(&sources).is_err());
    let mut sources = selected();
    let mut rows = sources["urn:owner"].to_vec();
    rows.last_mut().unwrap().object = TermValue::blank("not-an-accessibility-world");
    sources.insert("urn:owner".into(), rows.into());
    assert!(NativeModalProgram::prepare(&sources).is_err());
}

#[test]
fn malformed_frame_diagnostic_retains_the_original_subject() {
    let mut sources = selected();
    let mut rows = sources["urn:owner"].to_vec();
    for row in &mut rows {
        if row.subject.as_iri() == Some("urn:formula") {
            row.subject = TermValue::blank("malformed-formula");
        }
    }
    sources.insert("urn:owner".into(), rows.into());
    let error = match NativeModalProgram::prepare(&sources) {
        Ok(_) => panic!("a blank frame subject cannot be admitted as an IRI"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("malformed-formula"), "{error}");
}

#[test]
fn modal_support_preserves_foreign_world_and_exact_statement_identity() {
    let program = NativeModalProgram::prepare(&selected()).unwrap();
    let verdict = evaluate_frame(
        &program.frames[0],
        vec![ModalWorldEvidence {
            world: "urn:empty".into(),
            atom_present: true,
        }],
    )
    .unwrap()
    .remove(0);
    let expected = expected_premises(&verdict.evaluation);
    assert_eq!(expected.last().unwrap().0, "urn:empty");
    let support: Vec<_> = expected
        .iter()
        .enumerate()
        .map(|(index, (world, _))| NativeModalSupport {
            world: (*world).into(),
            proof: NativeProofId([index as u8; 32]),
        })
        .collect();
    let head = WitnessStatement {
        subject: TermValue::iri(&verdict.subject),
        predicate: verdict.predicate.clone(),
        object: TermValue::iri(&verdict.object),
    };
    let mut evidence = NativeModalEvidence {
        input_contract: [9; 32],
        evaluation: verdict.evaluation,
        supports: support,
    };
    evidence.validate_structure("urn:owner", &head).unwrap();
    assert!(evidence.validate_structure("urn:empty", &head).is_err());
    evidence.supports.last_mut().unwrap().world = "urn:owner".into();
    assert!(evidence.validate_structure("urn:owner", &head).is_err());
    evidence.supports.last_mut().unwrap().world = "urn:empty".into();
    assert!(
        evidence
            .validate_supports(&[9; 32], &BTreeMap::new())
            .is_err()
    );
    assert!(
        evidence
            .validate_supports(&[8; 32], &BTreeMap::new())
            .is_err()
    );
}
