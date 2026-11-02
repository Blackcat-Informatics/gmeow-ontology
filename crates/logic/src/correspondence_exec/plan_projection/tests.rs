// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{BlankScope, RdfDatasetBuilder, RdfLiteral};

use super::*;
use purrdf_lex::json::record::{ToJson, from_slice};

const PLAN: &str = "urn:gmeow:plan:test";
const CYCLE: &str = "urn:gmeow:plan:test/cycle";
const PREPARE: &str = "urn:gmeow:action:prepare";
const GUARD: &str = "urn:gmeow:guard:fit";
const ADMINISTER: &str = "urn:gmeow:action:administer";
const DEFER: &str = "urn:gmeow:action:defer";
const OBSERVE: &str = "urn:gmeow:action:observe";
const TRACKED: &str = "urn:gmeow:state:platelets";

fn plan_dataset() -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let scope = BlankScope(42);
    let rdf_type = builder.intern_iri(RDF_TYPE).unwrap();
    let rdf_first = builder.intern_iri(RDF_FIRST).unwrap();
    let rdf_rest = builder.intern_iri(RDF_REST).unwrap();
    let rdf_nil = builder.intern_iri(RDF_NIL).unwrap();
    let plan = builder.intern_iri(PLAN).unwrap();
    let cycle = builder.intern_iri(CYCLE).unwrap();
    let plan_class = builder.intern_iri(&logic("Plan")).unwrap();
    let body_pred = builder.intern_iri(&logic("body")).unwrap();
    let loop_class = builder.intern_iri(&logic("Loop")).unwrap();
    let loop_count_pred = builder.intern_iri(&logic("loopCount")).unwrap();
    let loop_period_pred = builder.intern_iri(&logic("loopPeriod")).unwrap();
    let loop_body_pred = builder.intern_iri(&logic("loopBody")).unwrap();
    let serial_pred = builder.intern_iri(&logic("serial")).unwrap();
    let loop_node = builder.intern_blank("loop", scope).unwrap();
    let loop_list = builder.intern_blank("loop-list", scope).unwrap();
    let count = builder
        .intern_literal(RdfLiteral::typed(
            "2",
            "http://www.w3.org/2001/XMLSchema#integer",
        ))
        .unwrap();
    let period = builder
        .intern_literal(RdfLiteral::typed(
            "P3W",
            "http://www.w3.org/2001/XMLSchema#duration",
        ))
        .unwrap();
    builder.push_quad(plan, rdf_type, plan_class, None).unwrap();
    builder.push_quad(plan, body_pred, loop_node, None).unwrap();
    builder
        .push_quad(loop_node, rdf_type, loop_class, None)
        .unwrap();
    builder
        .push_quad(loop_node, loop_count_pred, count, None)
        .unwrap();
    builder
        .push_quad(loop_node, loop_period_pred, period, None)
        .unwrap();
    builder
        .push_quad(loop_node, loop_body_pred, loop_list, None)
        .unwrap();
    builder
        .push_quad(loop_list, rdf_first, cycle, None)
        .unwrap();
    builder
        .push_quad(loop_list, rdf_rest, rdf_nil, None)
        .unwrap();
    builder
        .push_quad(cycle, rdf_type, plan_class, None)
        .unwrap();

    let serial = builder.intern_blank("serial-1", scope).unwrap();
    let serial_2 = builder.intern_blank("serial-2", scope).unwrap();
    let serial_3 = builder.intern_blank("serial-3", scope).unwrap();
    let prepare = builder.intern_iri(PREPARE).unwrap();
    let guard = builder.intern_iri(GUARD).unwrap();
    let observe = builder.intern_iri(OBSERVE).unwrap();
    builder.push_quad(cycle, serial_pred, serial, None).unwrap();
    builder.push_quad(serial, rdf_first, prepare, None).unwrap();
    builder.push_quad(serial, rdf_rest, serial_2, None).unwrap();
    builder.push_quad(serial_2, rdf_first, guard, None).unwrap();
    builder
        .push_quad(serial_2, rdf_rest, serial_3, None)
        .unwrap();
    builder
        .push_quad(serial_3, rdf_first, observe, None)
        .unwrap();
    builder
        .push_quad(serial_3, rdf_rest, rdf_nil, None)
        .unwrap();

    let action_class = builder.intern_iri(&logic("ActionSchema")).unwrap();
    let resource_pred = builder.intern_iri(&logic("resource")).unwrap();
    let nurse = builder.intern_iri("urn:gmeow:resource:nurse").unwrap();
    for action in [prepare, observe] {
        builder
            .push_quad(action, rdf_type, action_class, None)
            .unwrap();
    }
    builder
        .push_quad(prepare, resource_pred, nurse, None)
        .unwrap();

    let guard_class = builder.intern_iri(&logic("GuardedBranch")).unwrap();
    let guard_pred = builder.intern_iri(&logic("guard")).unwrap();
    let then_pred = builder.intern_iri(&logic("then")).unwrap();
    let else_pred = builder.intern_iri(&logic("else")).unwrap();
    let and_pred = builder.intern_iri(&logic("and")).unwrap();
    let condition = builder.intern_blank("condition", scope).unwrap();
    let condition_list = builder.intern_blank("condition-list", scope).unwrap();
    let tracked = builder.intern_iri(TRACKED).unwrap();
    let administer = builder.intern_iri(ADMINISTER).unwrap();
    let defer = builder.intern_iri(DEFER).unwrap();
    builder
        .push_quad(guard, rdf_type, guard_class, None)
        .unwrap();
    builder
        .push_quad(guard, guard_pred, condition, None)
        .unwrap();
    builder
        .push_quad(guard, then_pred, administer, None)
        .unwrap();
    builder.push_quad(guard, else_pred, defer, None).unwrap();
    builder
        .push_quad(condition, and_pred, condition_list, None)
        .unwrap();
    builder
        .push_quad(condition_list, rdf_first, tracked, None)
        .unwrap();
    builder
        .push_quad(condition_list, rdf_rest, rdf_nil, None)
        .unwrap();
    builder
        .push_quad(administer, rdf_type, action_class, None)
        .unwrap();
    builder
        .push_quad(defer, rdf_type, action_class, None)
        .unwrap();

    let tracked_class = builder.intern_iri(&gmeow("TrackedState")).unwrap();
    let currency_pred = builder.intern_iri(&logic("currency")).unwrap();
    let currency = builder
        .intern_literal(RdfLiteral::typed(
            "PT12H",
            "http://www.w3.org/2001/XMLSchema#duration",
        ))
        .unwrap();
    builder
        .push_quad(tracked, rdf_type, tracked_class, None)
        .unwrap();
    builder
        .push_quad(tracked, currency_pred, currency, None)
        .unwrap();
    builder.freeze().unwrap()
}

fn observed_dataset(schemas: &[&str], missing_last_schema: bool) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let instantiates_plan = builder.intern_iri(&logic("instantiatesPlan")).unwrap();
    let instantiates_schema = builder.intern_iri(&logic("instantiatesSchema")).unwrap();
    let plan = builder.intern_iri(PLAN).unwrap();
    for (index, schema) in schemas.iter().enumerate() {
        let occurrence = builder
            .intern_iri(&format!("urn:gmeow:observed:{index}"))
            .unwrap();
        builder
            .push_quad(occurrence, instantiates_plan, plan, None)
            .unwrap();
        if !missing_last_schema || index + 1 != schemas.len() {
            let schema = builder.intern_iri(schema).unwrap();
            builder
                .push_quad(occurrence, instantiates_schema, schema, None)
                .unwrap();
        }
    }
    builder.freeze().unwrap()
}

/// Preserve the guarded plan skeleton while refusing malformed list inputs and certificates.
#[test]
fn fixed_loop_projects_to_a_prescriptive_guarded_skeleton() {
    let source = plan_dataset();
    let projection = project_plan(&source, PLAN, None, PlanProjectionLimits::default()).unwrap();
    let mut bytes = purrdf_lex::json::write_pretty(&projection.to_json()).into_bytes();
    bytes.push(b'\n');
    let restored: PlanProjection = from_slice(&bytes).unwrap();
    assert_eq!(restored, projection);
    verify_plan_projection_certificate(&restored).unwrap();
    let projection = restored;
    assert_eq!(projection.loop_count, 2);
    assert!(projection.loop_variable.is_none());
    assert_eq!(projection.steps.len(), 10);
    assert_eq!(projection.freshness.len(), 1);
    assert_eq!(projection.guards[0].condition.tracked_states, vec![TRACKED]);
    assert!(projection.edges.iter().any(|edge| {
        edge.kind == PlannedEdgeKind::NextIteration
            && edge.from.contains("cycle/0")
            && edge.to.contains("cycle/1")
    }));
    assert!(
        projection
            .loss_evidence
            .iter()
            .any(|loss| loss.contains("does not assert that any step occurred"))
    );
    assert!(!projection.complement.source_facts.is_empty());
    verify_plan_projection(&projection, &source, None, PlanProjectionLimits::default()).unwrap();

    let mut forged = projection;
    forged.certificate.source_digest = "00".repeat(32);
    assert!(
        verify_plan_projection(&forged, &source, None, PlanProjectionLimits::default()).is_err()
    );

    // Malformed source edges must never become a partial prescriptive list.
    let facts: Vec<_> = purrdf::native_quads::flat_rdf_quads(&source)
        .map(Fact::from_quad)
        .collect();
    let head = facts
        .iter()
        .find(|fact| fact.predicate == RDF_FIRST && iri_value(&fact.object) == Some(CYCLE))
        .unwrap()
        .subject
        .clone();
    let literal = RdfTerm::Literal(RdfLiteral::typed(
        "invalid list resource",
        "http://www.w3.org/2001/XMLSchema#string",
    ));
    for (subject, kind, prefix) in [
        (
            head.as_str(),
            purrdf_core::ListErrorKind::MultipleFirst,
            vec![],
        ),
        (
            RDF_NIL,
            purrdf_core::ListErrorKind::NonEmptyNil,
            vec![CYCLE.to_owned()],
        ),
    ] {
        let mut malformed = facts.clone();
        malformed.push(Fact {
            subject: subject.to_owned(),
            predicate: RDF_FIRST.to_owned(),
            object: literal.clone(),
            graph: None,
        });
        let error = Facts::select(&malformed, PLAN, None)
            .unwrap()
            .list(&head, 32)
            .unwrap_err();
        let cause = error
            .downcast_ref::<purrdf_core::ListError<String>>()
            .unwrap();
        assert_eq!(cause.kind, kind);
        assert_eq!(cause.node, subject);
        assert_eq!(cause.members, prefix);
    }
    for predicate in [RDF_FIRST, RDF_REST] {
        let mut malformed = facts.clone();
        malformed
            .iter_mut()
            .find(|fact| fact.subject == head && fact.predicate == predicate)
            .unwrap()
            .object = literal.clone();
        let error = Facts::select(&malformed, PLAN, None)
            .unwrap()
            .list(&head, 32)
            .unwrap_err();
        assert!(error.message().contains("requires a resource"));
        assert!(error.message().contains(predicate));
    }
}

#[test]
fn bounded_unrolling_refuses_a_smaller_caller_contract() {
    let source = plan_dataset();
    let error = project_plan(
        &source,
        PLAN,
        None,
        PlanProjectionLimits {
            max_unroll: 1,
            ..PlanProjectionLimits::default()
        },
    )
    .unwrap_err();
    assert!(error.message().contains("unrolling contract"));
}

#[test]
fn recovery_requires_real_plan_and_schema_witnesses() {
    let source = plan_dataset();
    let projection = project_plan(&source, PLAN, None, PlanProjectionLimits::default()).unwrap();
    let observed = observed_dataset(&[PREPARE, ADMINISTER, OBSERVE], false);
    let recovery = recover_planned_schema_skeleton(&projection, &observed).unwrap();
    let mut bytes = purrdf_lex::json::write_pretty(&recovery.to_json()).into_bytes();
    bytes.push(b'\n');
    let restored: PlanRecovery = from_slice(&bytes).unwrap();
    assert_eq!(restored, recovery);
    verify_plan_recovery_receipt(&restored, &projection).unwrap();
    let recovery = restored;
    assert_eq!(
        recovery.verdict,
        PlanRecoveryVerdict::PlannedSkeletonRecovered
    );
    assert_eq!(recovery.occurrences.len(), 3);
    assert_eq!(recovery.unobserved_planned_schemas, vec![DEFER]);
    verify_plan_recovery(&recovery, &projection, &observed).unwrap();
    let mut forged = recovery.clone();
    forged.recovered_schemas.push(DEFER.to_owned());
    assert!(verify_plan_recovery(&forged, &projection, &observed).is_err());

    let incomplete = observed_dataset(&[PREPARE], true);
    assert!(recover_planned_schema_skeleton(&projection, &incomplete).is_err());

    let off_plan = observed_dataset(&[PREPARE, "urn:gmeow:action:unplanned"], false);
    let recovery = recover_planned_schema_skeleton(&projection, &off_plan).unwrap();
    assert_eq!(
        recovery.verdict,
        PlanRecoveryVerdict::PlannedSkeletonRecovered
    );
    assert_eq!(recovery.off_plan_occurrences.len(), 1);
}
