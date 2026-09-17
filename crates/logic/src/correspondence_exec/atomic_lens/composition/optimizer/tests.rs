// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::correspondence_exec::CarrierAtoms;
use crate::correspondence_exec::atomic_lens::tests::{GRAPH, SOURCE, SUBJECT, VIEW, edit, source};

const FINAL: &str = "urn:gmeow:example:clinicalMagnitude";
const TERMINAL: &str = "urn:gmeow:example:optimized-terminal";

fn stage(source: &str, target: &str, inverse: bool) -> AtomicPropertyLens {
    AtomicPropertyLens::new(source, target, inverse, ViewLimits::default()).unwrap()
}

fn hash(value: &str) -> String {
    blake3::hash(value.as_bytes()).to_hex().to_string()
}

fn scope() -> AtomicOptimizationScope {
    AtomicOptimizationScope::new([
        &hash("program"),
        &hash("source-theory"),
        &hash("view-theory"),
        &hash("context"),
        &hash("effects"),
        &hash("complements"),
        &hash("preservation"),
        &hash("reasoning-contract"),
    ])
    .unwrap()
}

fn composition() -> AtomicComposition {
    AtomicComposition::new(vec![
        stage(SOURCE, VIEW, false),
        stage(VIEW, FINAL, true),
        stage(FINAL, TERMINAL, true),
    ])
    .unwrap()
}

#[test]
fn deterministic_saturation_extracts_and_rechecks_a_fused_plan() {
    let composition = composition();
    let first = composition
        .optimize(scope(), AtomicOptimizerLimits::default())
        .unwrap();
    let second = composition
        .optimize(scope(), AtomicOptimizerLimits::default())
        .unwrap();
    assert_eq!(first.certificate(), second.certificate());
    assert!(matches!(
        first.certificate().selected,
        AtomicPhysicalPlan::Fused { .. }
    ));
    assert!(
        first
            .certificate()
            .applications
            .iter()
            .any(|application| { application.rule == AtomicRewriteRule::Reassociation })
    );
    verify_atomic_certificate(first.certificate(), &composition, &scope()).unwrap();
}

#[test]
fn certificate_is_bound_to_context_program_and_extracted_term() {
    let composition = composition();
    let plan = composition
        .optimize(scope(), AtomicOptimizerLimits::default())
        .unwrap();
    let mut wrong_scope = scope();
    wrong_scope.context_digest = hash("another-context");
    assert!(verify_atomic_certificate(plan.certificate(), &composition, &wrong_scope).is_err());

    let mut forged = plan.certificate().clone();
    let AtomicPhysicalPlan::Fused { stage } = &mut forged.selected else {
        panic!("fixture should fuse");
    };
    stage.inverse = !stage.inverse;
    assert!(verify_atomic_certificate(&forged, &composition, &scope()).is_err());
}

#[test]
fn deterministic_exhaustion_retains_the_original_plan() {
    let limits = AtomicOptimizerLimits {
        max_eclasses: 1,
        max_enodes: 1,
        max_rule_applications: 1,
    };
    let composition = composition();
    let plan = composition.optimize(scope(), limits).unwrap();
    assert!(plan.certificate().search.exhausted);
    assert_eq!(plan.certificate().selected, AtomicPhysicalPlan::Original);
    let state = plan.acquire(source()).unwrap();
    assert_eq!(state.receipt().mode, AtomicExecutionMode::Original);
}

#[test]
fn fused_get_put_and_cancellation_preserve_the_complete_carrier() {
    let composition = composition();
    let original = composition.acquire(source()).unwrap();
    let plan = composition
        .optimize(scope(), AtomicOptimizerLimits::default())
        .unwrap();
    let state = plan.acquire(source()).unwrap();
    assert_eq!(state.receipt().mode, AtomicExecutionMode::Fused);
    assert_eq!(state.receipt().logical_stages.len(), 3);
    assert!(CarrierAtoms::read(&state.view()) == CarrierAtoms::read(original.get().dataset()));

    let edited = edit(SUBJECT, TERMINAL, Some("urn:gmeow:example:optimized-value"));
    let expected = original
        .put_shared_scopes(Arc::clone(&edited))
        .unwrap()
        .carrier()
        .materialize()
        .unwrap();
    let updated = state.put_shared_scopes(edited).unwrap();
    let actual = updated.carrier().materialize().unwrap();
    assert!(CarrierAtoms::read(&actual) == CarrierAtoms::read(&expected));
    assert_eq!(actual.reifier_quads().count(), 1);
    assert_eq!(actual.annotation_quads().count(), 2);
    assert_eq!(actual.named_graphs().count(), 2);

    let (cancelled, receipt) = updated.cancel_unchanged_get_put().unwrap();
    assert!(CarrierAtoms::read(&cancelled.materialize().unwrap()) == CarrierAtoms::read(&actual));
    assert_eq!(receipt.fallback, None);
    assert_eq!(
        receipt.applied_runtime_rule,
        Some(AtomicRewriteRule::WitnessedGetPutCancellation)
    );
}

#[test]
fn two_inversions_cannot_fuse_away_a_literal_subject_failure() {
    let composition =
        AtomicComposition::new(vec![stage(SOURCE, VIEW, true), stage(VIEW, FINAL, true)]).unwrap();
    let plan = composition
        .optimize(scope(), AtomicOptimizerLimits::default())
        .unwrap();
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let subject = builder.intern_iri(SUBJECT);
    let predicate = builder.intern_iri(SOURCE);
    let value = builder.intern_literal(purrdf::RdfLiteral::simple("literal endpoint"));
    let graph = builder.intern_iri(GRAPH);
    builder.push_quad(subject, predicate, value, Some(graph));
    let error = plan.acquire(builder.freeze().unwrap()).unwrap_err();
    assert!(format!("{error:#}").contains("literal"));
}

#[test]
fn conservative_limit_miss_runs_the_original_and_preserves_its_failure() {
    let mut limited = stage(VIEW, FINAL, false);
    limited.limits.max_rows = 0;
    let composition = AtomicComposition::new(vec![stage(SOURCE, VIEW, false), limited]).unwrap();
    let plan = composition
        .optimize(scope(), AtomicOptimizerLimits::default())
        .unwrap();
    let error = plan.acquire(source()).unwrap_err();
    let text = format!("{error:#}");
    assert!(text.contains("stage 1") && text.contains("limit"), "{text}");
}
