// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! G4 (AC3) — prove that the `slice_hash` and engine-descriptor axes fold into
//! [`SessionIdentity::descriptor_hash`] **independently**, not merely jointly with the
//! program axis.
//!
//! The integration suite (`tests/reasoning_session_restore_rejection.rs`) can only vary
//! these two axes *through* the public `bind` inputs, where both are derived — `slice_hash`
//! frames `source_iri` (which also feeds `program_hash`) and the engine descriptor is fixed
//! per build — so from the public surface they cannot be split from the program axis. Here,
//! in-crate, we reach the crate-private fold ([`digest`]) directly and re-fold the eight axes
//! with EXACTLY ONE of `slice_hash` / `engine_descriptor_hash` perturbed while every other
//! axis (including `program_hash`) is held at its baseline value. A bug that dropped either
//! axis from `bind`'s fold would (a) fail the positive control — which pins our re-fold to
//! `bind`'s own output, so an omitted axis makes the two disagree — and (b) fail the
//! perturbation asserts.
use super::*;

/// Re-fold the eight identity axes with the SAME domain tag and field order as
/// [`SessionIdentity::bind`], substituting `slice` and `engine` for the slice-hash and
/// engine-descriptor axes while every other axis is taken from `id`. Feeding `id`'s own
/// `slice_hash`/`engine_descriptor_hash` must reproduce `id.descriptor_hash` exactly.
fn refold(id: &SessionIdentity, slice: &str, engine: &str) -> String {
    digest(
        b"gmeow-logic-session-identity-v1",
        &[
            &id.data_generation.generation,
            &id.data_generation.source_contract,
            &id.program_hash,
            slice,
            &id.contract_hash,
            engine,
            &id.annotation_identity,
            &id.fragment,
        ],
    )
}

fn baseline() -> SessionIdentity {
    let data_generation = WorldSourceIdentity::new(
        "urn:blake3:0000000000000000000000000000000000000000000000000000000000000000",
        SESSION_SOURCE_CONTRACT,
    );
    let program = LogicProgram::new(vec![], vec![], vec![], None);
    let contract = ReasoningContract::new();
    let annotation = AnnotationContract::exact();
    SessionIdentity::bind(
        data_generation,
        &program,
        &contract,
        &annotation,
        "urn:example:fragment/binary-datalog",
    )
}

#[test]
fn positive_control_refold_reproduces_bind_descriptor_hash() {
    // Re-folding all eight axes at their baseline values must exactly reproduce `bind`'s
    // own `descriptor_hash`. This pins the re-fold to `bind`: if `bind` ever stopped
    // folding `slice_hash` or `engine_descriptor_hash`, this equality would break.
    let id = baseline();
    assert_eq!(
        refold(&id, &id.slice_hash, &id.engine_descriptor_hash),
        id.descriptor_hash,
        "the eight-axis re-fold must reproduce bind's descriptor_hash exactly",
    );
}

#[test]
fn slice_hash_folds_independently_of_the_program_axis() {
    // Change ONLY the slice-hash axis (program_hash and every other axis held fixed):
    // the descriptor must change, proving the slice axis is load-bearing on its own and
    // not merely coupled to the program axis.
    let id = baseline();
    let drifted = refold(
        &id,
        "urn:example:some-other-slice-provenance",
        &id.engine_descriptor_hash,
    );
    assert_ne!(
        drifted, id.descriptor_hash,
        "a slice-hash-only drift must change descriptor_hash",
    );
    // A checkpoint pinned to the slice-drifted identity is rejected under the baseline.
    assert!(
        id.assert_matches(&drifted).is_err(),
        "restore must reject a slice-hash-only identity drift",
    );
}

#[test]
fn engine_descriptor_folds_independently_of_the_program_axis() {
    // Change ONLY the engine-descriptor axis (program_hash and every other axis held
    // fixed): the descriptor must change, proving the engine axis is load-bearing on its
    // own — the axis the public surface cannot vary (it is fixed per build).
    let id = baseline();
    let drifted = refold(
        &id,
        &id.slice_hash,
        "urn:example:some-other-engine-descriptor",
    );
    assert_ne!(
        drifted, id.descriptor_hash,
        "an engine-descriptor-only drift must change descriptor_hash",
    );
    assert!(
        id.assert_matches(&drifted).is_err(),
        "restore must reject an engine-descriptor-only identity drift",
    );
}
