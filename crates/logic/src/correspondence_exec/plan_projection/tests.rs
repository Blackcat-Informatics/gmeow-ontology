// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{BlankScope, RdfDatasetBuilder, RdfLiteral};

use super::*;

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
    let rdf_type = builder.intern_iri(RDF_TYPE);
    let rdf_first = builder.intern_iri(RDF_FIRST);
    let rdf_rest = builder.intern_iri(RDF_REST);
    let rdf_nil = builder.intern_iri(RDF_NIL);
    let plan = builder.intern_iri(PLAN);
    let cycle = builder.intern_iri(CYCLE);
    let plan_class = builder.intern_iri(&logic("Plan"));
    let body_pred = builder.intern_iri(&logic("body"));
    let loop_class = builder.intern_iri(&logic("Loop"));
    let loop_count_pred = builder.intern_iri(&logic("loopCount"));
    let loop_period_pred = builder.intern_iri(&logic("loopPeriod"));
    let loop_body_pred = builder.intern_iri(&logic("loopBody"));
    let serial_pred = builder.intern_iri(&logic("serial"));
    let loop_node = builder.intern_blank("loop", scope);
    let loop_list = builder.intern_blank("loop-list", scope);
    let count = builder.intern_literal(RdfLiteral::typed(
        "2",
        "http://www.w3.org/2001/XMLSchema#integer",
    ));
    let period = builder.intern_literal(RdfLiteral::typed(
        "P3W",
        "http://www.w3.org/2001/XMLSchema#duration",
    ));
    builder.push_quad(plan, rdf_type, plan_class, None);
    builder.push_quad(plan, body_pred, loop_node, None);
    builder.push_quad(loop_node, rdf_type, loop_class, None);
    builder.push_quad(loop_node, loop_count_pred, count, None);
    builder.push_quad(loop_node, loop_period_pred, period, None);
    builder.push_quad(loop_node, loop_body_pred, loop_list, None);
    builder.push_quad(loop_list, rdf_first, cycle, None);
    builder.push_quad(loop_list, rdf_rest, rdf_nil, None);
    builder.push_quad(cycle, rdf_type, plan_class, None);

    let serial = builder.intern_blank("serial-1", scope);
    let serial_2 = builder.intern_blank("serial-2", scope);
    let serial_3 = builder.intern_blank("serial-3", scope);
    let prepare = builder.intern_iri(PREPARE);
    let guard = builder.intern_iri(GUARD);
    let observe = builder.intern_iri(OBSERVE);
    builder.push_quad(cycle, serial_pred, serial, None);
    builder.push_quad(serial, rdf_first, prepare, None);
    builder.push_quad(serial, rdf_rest, serial_2, None);
    builder.push_quad(serial_2, rdf_first, guard, None);
    builder.push_quad(serial_2, rdf_rest, serial_3, None);
    builder.push_quad(serial_3, rdf_first, observe, None);
    builder.push_quad(serial_3, rdf_rest, rdf_nil, None);

    let action_class = builder.intern_iri(&logic("ActionSchema"));
    let resource_pred = builder.intern_iri(&logic("resource"));
    let nurse = builder.intern_iri("urn:gmeow:resource:nurse");
    for action in [prepare, observe] {
        builder.push_quad(action, rdf_type, action_class, None);
    }
    builder.push_quad(prepare, resource_pred, nurse, None);

    let guard_class = builder.intern_iri(&logic("GuardedBranch"));
    let guard_pred = builder.intern_iri(&logic("guard"));
    let then_pred = builder.intern_iri(&logic("then"));
    let else_pred = builder.intern_iri(&logic("else"));
    let and_pred = builder.intern_iri(&logic("and"));
    let condition = builder.intern_blank("condition", scope);
    let condition_list = builder.intern_blank("condition-list", scope);
    let tracked = builder.intern_iri(TRACKED);
    let administer = builder.intern_iri(ADMINISTER);
    let defer = builder.intern_iri(DEFER);
    builder.push_quad(guard, rdf_type, guard_class, None);
    builder.push_quad(guard, guard_pred, condition, None);
    builder.push_quad(guard, then_pred, administer, None);
    builder.push_quad(guard, else_pred, defer, None);
    builder.push_quad(condition, and_pred, condition_list, None);
    builder.push_quad(condition_list, rdf_first, tracked, None);
    builder.push_quad(condition_list, rdf_rest, rdf_nil, None);
    builder.push_quad(administer, rdf_type, action_class, None);
    builder.push_quad(defer, rdf_type, action_class, None);

    let tracked_class = builder.intern_iri(&gmeow("TrackedState"));
    let currency_pred = builder.intern_iri(&logic("currency"));
    let currency = builder.intern_literal(RdfLiteral::typed(
        "PT12H",
        "http://www.w3.org/2001/XMLSchema#duration",
    ));
    builder.push_quad(tracked, rdf_type, tracked_class, None);
    builder.push_quad(tracked, currency_pred, currency, None);
    builder.freeze().unwrap()
}

fn observed_dataset(schemas: &[&str], missing_last_schema: bool) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let instantiates_plan = builder.intern_iri(&logic("instantiatesPlan"));
    let instantiates_schema = builder.intern_iri(&logic("instantiatesSchema"));
    let plan = builder.intern_iri(PLAN);
    for (index, schema) in schemas.iter().enumerate() {
        let occurrence = builder.intern_iri(&format!("urn:gmeow:observed:{index}"));
        builder.push_quad(occurrence, instantiates_plan, plan, None);
        if !missing_last_schema || index + 1 != schemas.len() {
            let schema = builder.intern_iri(*schema);
            builder.push_quad(occurrence, instantiates_schema, schema, None);
        }
    }
    builder.freeze().unwrap()
}

#[test]
fn fixed_loop_projects_to_a_prescriptive_guarded_skeleton() {
    let source = plan_dataset();
    let projection = project_plan(&source, PLAN, None, PlanProjectionLimits::default()).unwrap();
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
