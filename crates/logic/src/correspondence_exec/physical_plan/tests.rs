// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use gmeow_logic_compile::ir::{
    CorrespondenceRelation, MorphismClass, MorphismKind, PreservationKind, TransactionProgramIr,
};
use purrdf::{BlankScope, RdfDatasetBuilder};

use super::*;

const FIRST: &str = "urn:gmeow:composition:first";
const SECOND: &str = "urn:gmeow:composition:second";
const COMPOSITE: &str = "urn:gmeow:composition:result";
const DECLARATION: &str = "urn:gmeow:composition:declaration";
const P: &str = "urn:gmeow:path:p";
const Q: &str = "urn:gmeow:path:q";
const STANDPOINT: &str = "urn:gmeow:standpoint:clinical";

fn correspondence(iri: &str, get: &str, source: &str, target: &str) -> Correspondence {
    Correspondence::new(
        iri,
        CorrespondenceRelation::RelatedMatch,
        MorphismClass::BridgeView,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        Some(get.to_owned()),
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        Some(STANDPOINT.to_owned()),
        Some(PreservationKind::SoundUnder),
    )
    .unwrap()
    .with_endpoints(source, target)
    .unwrap()
}

fn program() -> CorrespondenceProgram {
    let first_get = "urn:gmeow:composition:first/get";
    let second_get = "urn:gmeow:composition:second/get";
    let composite_get = "urn:gmeow:composition:result/get";
    let composition = CorrespondenceComposition::new(
        DECLARATION.to_owned(),
        FIRST.to_owned(),
        SECOND.to_owned(),
        COMPOSITE.to_owned(),
    )
    .unwrap();
    CorrespondenceProgram::new(
        vec![
            correspondence(FIRST, first_get, "urn:type:S", "urn:type:M"),
            correspondence(SECOND, second_get, "urn:type:M", "urn:type:T"),
            correspondence(COMPOSITE, composite_get, "urn:type:S", "urn:type:T"),
        ],
        PreservationKind::SoundUnder,
    )
    .with_compositions(vec![composition])
    .with_leg_programs(vec![
        TransactionProgramIr {
            iri: first_get.to_owned(),
            body: LegPath::Step(P.to_owned()),
        },
        TransactionProgramIr {
            iri: second_get.to_owned(),
            body: LegPath::Step(Q.to_owned()),
        },
        TransactionProgramIr {
            iri: composite_get.to_owned(),
            body: LegPath::Seq(vec![
                LegPath::Step(P.to_owned()),
                LegPath::Step(Q.to_owned()),
            ]),
        },
    ])
}

fn dataset() -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let source = builder.intern_iri("urn:data:source");
    let p = builder.intern_iri(P);
    let q = builder.intern_iri(Q);
    let middle_one = builder.intern_blank("middle-one", BlankScope(7));
    let middle_two = builder.intern_iri("urn:data:middle-two");
    let target_one = builder.intern_iri("urn:data:target-one");
    let target_two = builder.intern_iri("urn:data:target-two");
    builder.push_quad(source, p, middle_one, None);
    builder.push_quad(source, p, middle_two, None);
    builder.push_quad(middle_one, q, target_one, None);
    builder.push_quad(middle_two, q, target_one, None);
    builder.push_quad(middle_two, q, target_two, None);

    // Unselected carrier structure must remain part of the input identity even
    // though the default-graph relation query does not mutate or return it.
    let graph = builder.intern_iri("urn:data:standpoint-graph");
    let empty = builder.intern_iri("urn:data:empty-graph");
    builder.declare_named_graph(empty);
    let provenance = builder.intern_iri("http://www.w3.org/ns/prov#wasDerivedFrom");
    let origin = builder.intern_iri("urn:data:record");
    builder.push_quad(source, provenance, origin, Some(graph));
    let statement = builder.intern_triple(source, p, middle_two);
    let reifier = builder.intern_iri("urn:data:claim");
    builder.push_reifier_in_graph(reifier, statement, Some(graph));
    builder.push_annotation_in_graph(reifier, provenance, origin, Some(graph));
    builder.freeze().unwrap()
}

#[test]
fn fused_witness_join_matches_original_without_intermediate_relations() {
    let program = program();
    let fused =
        PreparedCompositionProgram::prepare(&program, CompositionPlanLimits::default()).unwrap();
    assert!(matches!(
        fused.report().certificates[0].selected,
        CompositionPhysicalPlan::FusedWitnessJoin
    ));
    let original =
        PreparedCompositionProgram::prepare(&program, CompositionPlanLimits { max_path_nodes: 0 })
            .unwrap();
    assert!(matches!(
        original.report().certificates[0].selected,
        CompositionPhysicalPlan::Original {
            reason: OriginalPlanReason::SearchLimit
        }
    ));

    let dataset = dataset();
    let fused_result = fused.execute(DECLARATION, &dataset).unwrap();
    let original_result = original.execute(DECLARATION, &dataset).unwrap();
    assert_eq!(fused_result.rows, original_result.rows);
    assert_eq!(fused_result.rows.len(), 3);
    assert_eq!(fused_result.receipt.materialized_intermediate_rows, 0);
    assert_eq!(original_result.receipt.materialized_intermediate_rows, 5);
    assert_eq!(fused_result.receipt.logical_stages.len(), 3);
    assert!(
        fused_result
            .receipt
            .logical_stages
            .iter()
            .all(|stage| stage.checks.as_slice() == STAGE_CHECKS)
    );
}

#[test]
fn certificate_recheck_rejects_context_and_program_rebinding() {
    let program = program();
    let prepared =
        PreparedCompositionProgram::prepare(&program, CompositionPlanLimits::default()).unwrap();
    verify_composition_report(prepared.report(), &program).unwrap();
    let certificate = &prepared.report().certificates[0];
    verify_composition_certificate(certificate, &program).unwrap();

    let mut forged = certificate.clone();
    forged.scope.context_digest = digest_text("forged", "context");
    assert!(verify_composition_certificate(&forged, &program).is_err());

    let mut rebound = program.clone();
    rebound.leg_programs[0].body = LegPath::Inverse(Box::new(LegPath::Step(P.to_owned())));
    assert!(verify_composition_certificate(certificate, &rebound).is_err());

    let mut incomplete = prepared.report().clone();
    incomplete.certificates.clear();
    assert!(verify_composition_report(&incomplete, &program).is_err());

    let mut rebound_report = prepared.report().clone();
    rebound_report.program_digest = digest_text("forged", "program");
    assert!(verify_composition_report(&rebound_report, &program).is_err());
}

#[test]
fn a_declared_composite_with_a_different_body_is_rejected() {
    let mut program = program();
    let result_leg = program
        .leg_programs
        .iter_mut()
        .find(|leg| leg.iri == "urn:gmeow:composition:result/get")
        .unwrap();
    result_leg.body = LegPath::Step("urn:gmeow:path:unrelated".to_owned());
    let error =
        match PreparedCompositionProgram::prepare(&program, CompositionPlanLimits::default()) {
            Ok(_) => panic!("false composite body must be rejected"),
            Err(error) => error,
        };
    assert!(
        error
            .message()
            .contains("not the normalized sequential composition")
    );
}

#[test]
fn a_standpoint_change_requires_an_explicit_context_transport() {
    let mut program = program();
    program
        .correspondences
        .iter_mut()
        .find(|value| value.iri == SECOND)
        .unwrap()
        .according_to = Some("urn:gmeow:standpoint:other".to_owned());
    let error =
        match PreparedCompositionProgram::prepare(&program, CompositionPlanLimits::default()) {
            Ok(_) => panic!("implicit standpoint change must be rejected"),
            Err(error) => error,
        };
    assert!(error.message().contains("context transport"));
}
