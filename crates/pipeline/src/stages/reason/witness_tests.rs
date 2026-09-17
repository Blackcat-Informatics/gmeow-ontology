// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW native witness publication controls; all inputs are tiny synthetic cases.

use super::*;

fn existential() -> ReasonArtifacts {
    reason_artifacts(
        br#"
<urn:R> <http://www.w3.org/2002/07/owl#onProperty> <urn:p> <urn:w> .
<urn:R> <http://www.w3.org/2002/07/owl#someValuesFrom> <urn:D> <urn:w> .
<urn:x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:R> <urn:w> .
"#,
    )
    .unwrap()
}

#[test]
fn subject_only_witness_projects_its_actual_type_head_without_an_incoming_edge() {
    let result = reason_artifacts(br#"
<http://www.w3.org/2002/07/owl#Thing> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://www.w3.org/2002/07/owl#Nothing> <urn:domain:w> .
"#).unwrap();
    assert!(
        !result
            .result
            .native_execution()
            .unwrap()
            .witness_derivations
            .is_empty()
    );
    let witness = result
        .result
        .native_execution()
        .unwrap()
        .witness_derivations
        .iter()
        .find(|witness| {
            witness
                .heads
                .iter()
                .all(|head| head.statement.subject.as_iri() == Some(witness.witness.as_str()))
        })
        .expect("the native Thing-empty witness is subject-only");
    let projections =
        resolve_witness_projections(std::slice::from_ref(witness), &result.result).unwrap();
    assert_eq!(projections.len(), witness.heads.len());
    assert!(projections.iter().all(
        |head| head.subject.as_iri() == Some(witness.witness.as_str())
            && head.object.as_iri() != Some(witness.witness.as_str())
    ));
    let diagnostics = result
        .dataset
        .project_named_graph(crate::stages::carrier::GRAPH_DIAGNOSTICS);
    assert!(diagnostics.owned_quads().any(|row| row.predicate
        == "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject"
        && row.object == RdfTerm::iri(&witness.witness)));
}

#[test]
fn witness_diagnostics_preserve_the_complete_native_scope_head_and_premise_receipt() {
    let result = existential();
    let diagnostics = result
        .dataset
        .project_named_graph(crate::stages::carrier::GRAPH_DIAGNOSTICS);
    let receipts: Vec<_> = diagnostics
        .owned_quads()
        .filter_map(|row| {
            if row.predicate != "http://www.w3.org/ns/prov#value" {
                return None;
            }
            let RdfTerm::Literal(literal) = row.object else {
                return None;
            };
            if !literal.lexical_form.starts_with("gmeow-witness-v1:") {
                return None;
            }
            Some(gmeow_logic::reason::WitnessDerivation::from_wire(&literal.lexical_form).unwrap())
        })
        .collect();
    assert_eq!(
        receipts.len(),
        result
            .result
            .native_execution()
            .unwrap()
            .witness_derivations
            .len()
    );
    let mut source_witnesses = 0;
    let mut domain_witnesses = 0;
    for witness in &result
        .result
        .native_execution()
        .unwrap()
        .witness_derivations
    {
        assert!(receipts.contains(witness));
        if witness
            .rule_iri
            .starts_with("urn:gmeow:rule:nonempty-object-domain:v1:")
        {
            domain_witnesses += 1;
            assert_eq!(witness.heads.len(), 1);
            assert!(witness.heads[0].premises.is_empty());
            assert_eq!(
                witness.heads[0].statement.predicate,
                "https://blackcatinformatics.ca/logic/instanceOf"
            );
            assert_eq!(
                witness.heads[0].statement.object.as_iri(),
                Some("https://blackcatinformatics.ca/logic/Thing")
            );
        } else {
            source_witnesses += 1;
            assert!(
                witness.heads.len() >= 2,
                "both the property and filler-class head survive"
            );
            assert!(
                witness
                    .heads
                    .iter()
                    .any(|head| head.statement.predicate == "urn:p")
            );
            assert!(
                witness
                    .heads
                    .iter()
                    .any(|head| head.statement.object.as_iri() == Some("urn:D"))
            );
        }
        assert_eq!(witness.scope.world, "urn:w");
        let projection =
            resolve_witness_projections(std::slice::from_ref(witness), &result.result).unwrap();
        assert_eq!(projection.len(), witness.heads.len());
        for head in projection {
            assert!(
                diagnostics.owned_quads().any(|row| {
                    row.subject == RdfTerm::iri(&head.r_head)
                        && row.predicate
                            == "https://blackcatinformatics.ca/logic/derivationIdentifier"
                        && row.object
                            == RdfTerm::Literal(RdfLiteral::typed(
                                &head.derivation_id,
                                "http://www.w3.org/2001/XMLSchema#string",
                            ))
                }),
                "each witness head publishes its exact native derivation under logic:derivationIdentifier"
            );
            assert!(
                diagnostics
                    .owned_quads()
                    .any(|row| row.subject == RdfTerm::iri(&head.r_head)
                        && row.predicate == "https://blackcatinformatics.ca/gmeow/inWorld"
                        && row.object == RdfTerm::iri("urn:w"))
            );
        }
    }
    assert_eq!(
        source_witnesses, 1,
        "the source property/filler obligation remains nonvacuous"
    );
    assert_eq!(
        domain_witnesses, 1,
        "only the explicitly submitted urn:w theory has an intrinsic domain"
    );
}

#[test]
fn witness_publication_refuses_wrong_result_missing_heads_and_tampered_positions() {
    let result = existential();
    let other =
        reason_artifacts(br#"<urn:unrelated> <urn:p> <urn:value> <urn:foreign> ."#).unwrap();
    assert!(
        resolve_witness_projections(
            &result
                .result
                .native_execution()
                .unwrap()
                .witness_derivations,
            &other.result
        )
        .is_err()
    );
    for mutation in 0..3 {
        let mut witnesses = result
            .result
            .native_execution()
            .unwrap()
            .witness_derivations
            .clone();
        match mutation {
            0 => witnesses[0].heads.clear(),
            1 => witnesses[0].heads[0].positions.clear(),
            _ => witnesses[0].scope.world = "urn:foreign".to_owned(),
        }
        assert!(
            resolve_witness_projections(&witnesses, &result.result).is_err(),
            "mutation {mutation}"
        );
    }
}
