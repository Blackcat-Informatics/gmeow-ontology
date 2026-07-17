// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC3 — an identity mismatch on ANY of the seven bound axes REJECTS a checkpoint restore,
//! and a byte-tampered checkpoint is rejected as corrupt; restore never silently coerces a
//! mismatched checkpoint into a rebuilt state.
//!
//! ## Axis independence
//!
//! Five axes are driven by mutating the restore inputs: data-generation (a different EDB),
//! ReasoningContract, annotation algebra, and the program (rules) — the last of which
//! isolates `program_hash` from `slice_hash` by holding `source_iri` fixed while the rules
//! change. Two axes cannot be varied fully independently from the public surface:
//!
//! * **slice vs program** — `slice_hash` frames `source_iri`, but `source_iri` also feeds
//!   `LogicProgram::canonical_key` (hence `program_hash`); a `source_iri` change therefore
//!   moves BOTH. The slice axis is asserted with `source_iri` varied (program+slice jointly)
//!   and this coupling is noted, rather than faking independence.
//! * **engine descriptor** — bound from `EngineContract::current()`, which is fixed per
//!   build; it cannot be varied at runtime from the public API. The **incremental-fragment**
//!   axis IS exercised directly (a checkpoint minted via [`SessionIdentity::bind`] with a
//!   different fragment string), proving `descriptor_hash` folds a non-input axis.

use gmeow_logic::runtime::{
    Checkpoint, IntegrityFault, OperationOutcome, ReasoningSession, SessionIdentity,
};
use gmeow_logic_compile::ir::{LogicProgram, ReasoningContract};

mod session_common;
use session_common::*;

const SLICE_X: &str = "urn:example:slice-x";
const SLICE_Y: &str = "urn:example:slice-y";

/// A program with a pinned `source_iri`, so `slice_hash` can be held fixed while `rules`
/// vary (isolating the program axis) or varied on its own (the joint slice change).
fn with_source(mut program: LogicProgram, source: &str) -> LogicProgram {
    program.source_iri = Some(source.to_owned());
    program
}

fn contract_with_policy() -> ReasoningContract {
    let mut contract = ReasoningContract::new();
    contract
        .resource_policies
        .insert("https://example.org/policy/audited".to_owned());
    contract
}

/// Assert `restore` refuses with an identity mismatch (never `Ok`, never a coerced state).
fn assert_identity_mismatch(
    cp: &Checkpoint,
    edb: &purrdf::RdfDataset,
    program: &LogicProgram,
    contract: &ReasoningContract,
    annotation: &gmeow_logic::annotation::AnnotationContract,
    axis: &str,
) {
    match ReasoningSession::restore(cp, edb, program, contract, annotation) {
        Err(OperationOutcome::Invalid {
            fault: IntegrityFault::IdentityMismatch { .. },
        }) => {}
        Ok(_) => panic!("restore accepted a checkpoint that differs on the {axis} axis"),
        other => panic!("expected Invalid{{IdentityMismatch}} on the {axis} axis, got {other:?}"),
    }
}

#[test]
fn ac3_data_generation_axis_rejects() {
    let program = projection_program();
    let (contract, annotation) = baseline_contracts();
    let edb_a = edge_dataset(&[("a", "b"), ("b", "c")]);
    let session = ReasoningSession::open(&edb_a, &program, &contract, &annotation).expect("open A");
    let cp = session.checkpoint();

    // A different authorized EDB → a different data-generation address.
    let edb_b = edge_dataset(&[("a", "b"), ("b", "z")]);
    assert_identity_mismatch(
        &cp,
        &edb_b,
        &program,
        &contract,
        &annotation,
        "data-generation",
    );
}

#[test]
fn ac3_program_axis_rejects_independent_of_slice() {
    // Same slice (`source_iri`), different rules → `program_hash` differs, `slice_hash`
    // holds: the program axis is load-bearing on its own.
    let program_a = with_source(projection_program(), SLICE_X);
    let program_b = with_source(transitive_program(), SLICE_X);
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let session = ReasoningSession::open(&edb, &program_a, &contract, &annotation).expect("open A");
    let cp = session.checkpoint();

    assert_identity_mismatch(&cp, &edb, &program_b, &contract, &annotation, "program");
}

#[test]
fn ac3_slice_axis_rejects_jointly_with_program() {
    // Only `source_iri` differs — but it feeds `canonical_key`, so BOTH `slice_hash` and
    // `program_hash` move (noted in the module docs: the pair cannot be split).
    let program_a = with_source(projection_program(), SLICE_X);
    let program_b = with_source(projection_program(), SLICE_Y);
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let session = ReasoningSession::open(&edb, &program_a, &contract, &annotation).expect("open A");
    let cp = session.checkpoint();

    assert_identity_mismatch(&cp, &edb, &program_b, &contract, &annotation, "slice");
}

#[test]
fn ac3_contract_axis_rejects() {
    let program = projection_program();
    let annotation = gmeow_logic::annotation::AnnotationContract::exact();
    let contract_a = ReasoningContract::new();
    let contract_b = contract_with_policy();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let session = ReasoningSession::open(&edb, &program, &contract_a, &annotation).expect("open A");
    let cp = session.checkpoint();

    assert_identity_mismatch(&cp, &edb, &program, &contract_b, &annotation, "contract");
}

#[test]
fn ac3_annotation_algebra_axis_rejects() {
    let program = projection_program();
    let contract = ReasoningContract::new();
    let annotation_a = gmeow_logic::annotation::AnnotationContract::exact();
    let annotation_b =
        gmeow_logic::annotation::AnnotationContract::exact().with_max_fixpoint_rounds(7);
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let session = ReasoningSession::open(&edb, &program, &contract, &annotation_a).expect("open A");
    let cp = session.checkpoint();

    assert_identity_mismatch(
        &cp,
        &edb,
        &program,
        &contract,
        &annotation_b,
        "annotation-algebra",
    );
}

#[test]
fn ac3_incremental_fragment_axis_rejects() {
    // Mint a checkpoint whose identity was bound with a DIFFERENT fragment string; the
    // rebuilt identity uses the real certified-fragment name, so `descriptor_hash` (which
    // folds the fragment axis) differs → restore refuses. This proves a non-input axis is
    // genuinely folded.
    let program = projection_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let session = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open");

    let real = session.identity().clone();
    let mutated = SessionIdentity::bind(
        real.data_generation.clone(),
        &program,
        &contract,
        &annotation,
        "urn:example:some-other-fragment",
    );
    assert_ne!(
        mutated.descriptor_hash, real.descriptor_hash,
        "the fragment axis folds into descriptor_hash"
    );
    let cp = Checkpoint::new(
        mutated,
        real.data_generation.generation.clone(),
        session.head().to_owned(),
    );

    assert_identity_mismatch(
        &cp,
        &edb,
        &program,
        &contract,
        &annotation,
        "incremental-fragment",
    );
}

#[test]
fn ac3_byte_tampered_checkpoint_is_corrupt() {
    // Serialize a valid checkpoint, flip its journal_head field, deserialize, and restore:
    // the recomputed content address no longer matches → CorruptCheckpoint.
    let program = projection_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let session = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open");
    let cp = session.checkpoint();

    let mut value: serde_json::Value = serde_json::to_value(&cp).expect("serialize checkpoint");
    value["journal_head"] = serde_json::Value::String(
        "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
    );
    let tampered: Checkpoint =
        serde_json::from_value(value).expect("deserialize tampered checkpoint");

    match ReasoningSession::restore(&tampered, &edb, &program, &contract, &annotation) {
        Err(OperationOutcome::Invalid {
            fault: IntegrityFault::CorruptCheckpoint { .. },
        }) => {}
        other => panic!("expected Invalid{{CorruptCheckpoint}}, got {other:?}"),
    }
}

#[test]
fn ac3_positive_control_untampered_checkpoint_restores_to_base() {
    let program = transitive_program();
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    let session = ReasoningSession::open(&edb, &program, &contract, &annotation).expect("open");
    let base_closure = session_derived(&session, &idb_reach());
    let cp = session.checkpoint();

    let restored = ReasoningSession::restore(&cp, &edb, &program, &contract, &annotation)
        .expect("untampered checkpoint restores");
    assert_eq!(
        session_derived(&restored, &idb_reach()),
        base_closure,
        "the positive control restores to the pre-checkpoint (base) closure"
    );
}
