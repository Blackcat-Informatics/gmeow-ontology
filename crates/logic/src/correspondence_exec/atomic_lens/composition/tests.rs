// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::correspondence_exec::CarrierAtoms;
use crate::correspondence_exec::atomic_lens::tests::{
    EMPTY_GRAPH, GRAPH, SOURCE, SUBJECT, VIEW, edit, source,
};
use gmeow_logic_compile::ir::DischargeVerdict;
use purrdf::{RdfDatasetBuilder, ViewLimits};

const FINAL: &str = "urn:gmeow:example:clinicalMagnitude";
const TERMINAL: &str = "urn:gmeow:example:terminalMagnitude";

fn stage(source: &str, target: &str, inverse: bool) -> AtomicPropertyLens {
    AtomicPropertyLens::new(source, target, inverse, ViewLimits::default()).unwrap()
}

fn program() -> AtomicComposition {
    AtomicComposition::new(vec![stage(SOURCE, VIEW, false), stage(VIEW, FINAL, false)]).unwrap()
}

#[test]
fn composed_laws_recover_rich_prior_state_and_independent_edits() {
    let state = program().acquire(source()).unwrap();
    let first = edit(SUBJECT, FINAL, Some("urn:gmeow:example:independent-first"));
    let second = edit(
        "urn:gmeow:example:another-observation",
        FINAL,
        Some("urn:gmeow:example:independent-second"),
    );
    for outcome in [
        state.check_get_put("composed-acquisition").unwrap(),
        state
            .check_put_get("composed-first-edit", Arc::clone(&first))
            .unwrap(),
        state
            .check_put_get("composed-second-edit", Arc::clone(&second))
            .unwrap(),
        state
            .check_put_put("composed-two-edits", first, second)
            .unwrap(),
        state
            .check_section("all-complements", AtomicInitialState::EmptyWithComplement)
            .unwrap(),
        state
            .check_put_get("composed-deletion", edit(SUBJECT, FINAL, None))
            .unwrap(),
    ] {
        assert_eq!(
            outcome.verdict,
            DischargeVerdict::ObligationDischarged,
            "{outcome:?}"
        );
    }
}

#[test]
fn update_reuses_the_checked_intermediate_and_executes_each_original_stage() {
    let state = program().acquire(source()).unwrap();
    assert_eq!(
        state
            .execution()
            .iter()
            .map(|entry| entry.stage)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let updated = state
        .put_shared_scopes(edit(SUBJECT, FINAL, Some("urn:gmeow:example:edited")))
        .unwrap();
    assert_eq!(
        updated.execution(),
        &[
            AtomicStageExecution {
                stage: 1,
                operation: AtomicOperation::Put,
                reused_intermediate: true
            },
            AtomicStageExecution {
                stage: 0,
                operation: AtomicOperation::Put,
                reused_intermediate: false
            },
        ]
    );
    assert!(Arc::ptr_eq(
        updated.stages[1].source_focus.as_ref().unwrap(),
        &updated.stages[0].augmented.view
    ));
    assert!(
        updated
            .stages
            .iter()
            .all(|stage| stage.carrier.stats().work.materializations == 0)
    );
    assert_eq!(updated.carrier().reifier_quads().count(), 1);
    assert_eq!(updated.carrier().annotation_quads().count(), 2);
    assert_eq!(updated.carrier().named_graphs().count(), 2);
    assert_eq!(
        updated.stages[1]
            .augmented
            .complement
            .residual
            .base()
            .rdf_row_count(),
        0
    );
}

#[test]
fn reuse_agrees_with_explicit_sequential_carrier_materialization() {
    let original = source();
    let first = stage(SOURCE, VIEW, false)
        .acquire(Arc::clone(&original))
        .unwrap();
    let second = stage(VIEW, FINAL, true)
        .acquire(Arc::clone(first.get().dataset()))
        .unwrap();
    let edited = edit("urn:gmeow:example:new-value", FINAL, Some(SUBJECT));
    let intermediate = second
        .put_shared_scopes(Arc::clone(&edited))
        .unwrap()
        .carrier()
        .materialize()
        .unwrap();
    let expected = first
        .put_shared_scopes(intermediate)
        .unwrap()
        .carrier()
        .materialize()
        .unwrap();
    let composed =
        AtomicComposition::new(vec![stage(SOURCE, VIEW, false), stage(VIEW, FINAL, true)]).unwrap();
    let actual = composed
        .acquire(original)
        .unwrap()
        .put_shared_scopes(edited)
        .unwrap()
        .carrier()
        .materialize()
        .unwrap();
    assert!(CarrierAtoms::read(&actual) == CarrierAtoms::read(&expected));
}

#[test]
fn augmented_recovery_does_not_retain_discarded_intermediate_payloads() {
    let state = program().acquire(source()).unwrap();
    let intermediate = Arc::downgrade(&state.stages[0].augmented.view);
    let expected = state.carrier().materialize().unwrap();
    let augmented = state.get();
    drop(state);
    assert!(
        intermediate.upgrade().is_none(),
        "only graph catalogue data belongs in this intermediate complement"
    );
    let restored = augmented
        .restore(AtomicInitialState::EmptyWithComplement)
        .unwrap();
    let actual = restored.carrier().materialize().unwrap();
    assert!(CarrierAtoms::read(&actual) == CarrierAtoms::read(&expected));
    assert_eq!(
        restored
            .execution()
            .iter()
            .map(|entry| entry.stage)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    assert!(
        restored
            .execution()
            .iter()
            .all(|entry| entry.operation == AtomicOperation::Restore)
    );
}

#[test]
fn identity_steps_and_association_preserve_the_executable_sequence() {
    let a = AtomicComposition::new(vec![stage(SOURCE, VIEW, false)]).unwrap();
    let b = AtomicComposition::new(vec![stage(VIEW, FINAL, true)]).unwrap();
    let c = AtomicComposition::new(vec![stage(FINAL, TERMINAL, true)]).unwrap();
    let left = a
        .then(&b)
        .unwrap()
        .then(&c)
        .unwrap()
        .acquire(source())
        .unwrap();
    let right = a
        .then(&b.then(&c).unwrap())
        .unwrap()
        .acquire(source())
        .unwrap();
    assert!(
        Arc::ptr_eq(
            left.stages[1].augmented.complement.residual.base(),
            left.stages[2].augmented.complement.residual.base(),
        ),
        "intermediate stages must share one native graph catalogue"
    );
    assert_eq!(left.execution(), right.execution());
    assert!(CarrierAtoms::read(left.get().dataset()) == CarrierAtoms::read(right.get().dataset()));
    let edited = edit(
        SUBJECT,
        TERMINAL,
        Some("urn:gmeow:example:association-edit"),
    );
    let left = left.put_shared_scopes(Arc::clone(&edited)).unwrap();
    let right = right.put_shared_scopes(edited).unwrap();
    assert!(
        CarrierAtoms::read(&left.carrier().materialize().unwrap())
            == CarrierAtoms::read(&right.carrier().materialize().unwrap())
    );
    let identity = AtomicComposition::new(vec![stage(VIEW, VIEW, false)]).unwrap();
    let with_identity = a.then(&identity).unwrap().acquire(source()).unwrap();
    assert!(
        CarrierAtoms::read(a.acquire(source()).unwrap().get().dataset())
            == CarrierAtoms::read(with_identity.get().dataset())
    );
    assert_eq!(
        with_identity.execution().len(),
        2,
        "the identity stage's checks still execute"
    );
}

fn literal_carrier(predicate: &str) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri(SUBJECT);
    let property = builder.intern_iri(predicate);
    let value = builder.intern_literal(purrdf::RdfLiteral::simple("literal endpoint"));
    let graph = builder.intern_iri(GRAPH);
    let empty = builder.intern_iri(EMPTY_GRAPH);
    builder.declare_named_graph(empty);
    builder.push_quad(subject, property, value, Some(graph));
    builder.freeze().unwrap()
}

#[test]
fn cancelling_inversions_cannot_erase_intermediate_admission_failures() {
    let sequence =
        AtomicComposition::new(vec![stage(SOURCE, VIEW, true), stage(VIEW, FINAL, true)]).unwrap();
    // The apparent fused identity accepts the value, but the actual stage 0 does not.
    assert!(
        stage(SOURCE, FINAL, false)
            .acquire(literal_carrier(SOURCE))
            .is_ok()
    );
    let error = sequence.acquire(literal_carrier(SOURCE)).unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("stage 0") && error.contains("literal"),
        "{error}"
    );
    let state = sequence.acquire(source()).unwrap();
    let error = state.put_shared_scopes(literal_carrier(FINAL)).unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("stage 1") && error.contains("literal"),
        "{error}"
    );
    assert_eq!(
        state
            .check_get_put("original-survives-failed-put")
            .unwrap()
            .verdict,
        DischargeVerdict::ObligationDischarged
    );
}

#[test]
fn original_retention_limits_are_checked_at_their_own_stage() {
    let mut limited = stage(VIEW, FINAL, false);
    limited.limits.max_rows = 0;
    let sequence = AtomicComposition::new(vec![stage(SOURCE, VIEW, false), limited]).unwrap();
    let error = format!("{:#}", sequence.acquire(source()).unwrap_err());
    assert!(
        error.contains("stage 1") && error.contains("limit"),
        "{error}"
    );
}

#[test]
fn mismatched_types_and_missing_or_reordered_complements_fail_closed() {
    assert!(AtomicComposition::new(Vec::new()).is_err());
    assert!(
        AtomicComposition::new(vec![
            stage(SOURCE, VIEW, false),
            stage(FINAL, TERMINAL, false)
        ])
        .is_err()
    );
    let state = program().acquire(source()).unwrap();
    let mut missing = state.get();
    missing.complements.pop();
    assert!(
        missing
            .restore(AtomicInitialState::EmptyWithComplement)
            .is_err()
    );
    let mut reordered = state.get();
    reordered.complements.swap(0, 1);
    assert!(
        reordered
            .restore(AtomicInitialState::EmptyWithComplement)
            .is_err()
    );
    let mut rebound = state.get();
    rebound.program = Arc::new(
        AtomicComposition::new(vec![stage(SOURCE, VIEW, true), stage(VIEW, FINAL, false)]).unwrap(),
    );
    assert!(
        rebound
            .restore(AtomicInitialState::EmptyWithComplement)
            .is_err()
    );
}

#[test]
fn reuse_witness_rejects_another_publication_and_rich_source_residue() {
    let updated = program()
        .acquire(source())
        .unwrap()
        .put_shared_scopes(edit(SUBJECT, FINAL, Some("urn:gmeow:example:value")))
        .unwrap();
    assert!(
        CompleteFocus::check(&updated.stages[0]).is_err(),
        "rich source metadata is not a complete focus"
    );
    assert!(CompleteFocus::check(&updated.stages[1]).is_ok());
    let mut forged = updated.stages[1].clone();
    forged.source_focus = Some(edit(
        SUBJECT,
        VIEW,
        Some("urn:gmeow:example:different-publication"),
    ));
    assert!(CompleteFocus::check(&forged).is_err());
    let mut missing_graph = updated.stages[1].clone();
    missing_graph.source_focus = Some(RdfDatasetBuilder::new().freeze().unwrap());
    assert!(CompleteFocus::check(&missing_graph).is_err());
}

#[test]
fn stage_attribution_keeps_the_original_diagnostic_and_source_provenance() {
    let cause = gmeow_errors::Diag::from(std::io::Error::other("complement source unavailable"))
        .with_context("read selected complement")
        .with_focus("urn:gmeow:example:source-quality")
        .with_derived_from_quads(["urn:gmeow:example:source-assertion".to_owned()]);
    let code = cause.code();
    let grade = cause.grade();
    let origin = cause.emitted_at();
    let result = stage_error(2, cause);
    assert!(result.is::<std::io::Error>());
    assert_eq!(result.code(), code);
    assert_eq!(result.grade(), grade);
    assert_eq!(result.emitted_at(), origin);
    assert_eq!(result.inner().context.len(), 2);
    assert_eq!(result.inner().context[0].label, "read selected complement");
    assert_eq!(
        result.inner().context[1].label,
        "atomic composition stage 2"
    );
    assert_eq!(
        result.inner().derived_from_quads,
        ["urn:gmeow:example:source-assertion"]
    );
    assert_eq!(
        result.inner().source_ctx.focus.as_ref().unwrap().0,
        "urn:gmeow:example:source-quality"
    );
}
