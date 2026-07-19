// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC6 (governance) — the "remains semver-governed" guarantee as an executable drift pin.
//!
//! The public session surface is `#[non_exhaustive]` (a compile-time additive guarantee)
//! plus a descriptor-hash drift pin: any change to the engine descriptor — or to the
//! content-addressed [`SessionIdentity`] built from a FIXED input — moves a golden BLAKE3
//! hex, forcing a DELIBERATE version bump and checkpoint re-bless rather than a silent
//! semantic drift that leaves existing checkpoints spuriously valid.

use gmeow_logic::runtime::{EngineContract, ReasoningSession};

mod session_common;
use session_common::*;

/// Golden engine-descriptor hash. A drift here is a deliberate engine version bump.
/// Re-blessed for the broader chase-termination-class ladder (joint / super-weak /
/// model-summarizing acyclicity certifiers + the authored-existential-rule surface),
/// which extends the forward reasoning/certificate contract folded into this descriptor.
/// Re-blessed again for the stage-2 certifier hardening: the partial-order `combine` meet
/// (incomparable JA∥SWA meet to their glb), the budgeted MSA critical-instance fixpoint
/// (Exhausted → conservative refuse), the fail-fast authored-rule reader, and the
/// certifier perf rewrites — all move the `physical/chase.rs` / `reason/dl.rs` content
/// digest folded into this descriptor. Re-blessed once more for the round-3 certifier
/// hardening: the reordered soundness differential, the atomic authored-rule reader
/// (non-resource ref / duplicate slot hard-fails), and the MSA critical-instance size cap
/// all move the `physical/chase.rs` / `reason/dl.rs` content digest. Re-blessed again for
/// the public bilinear-form distance API (`bilinear_sqdist` / `compare_sqdist` /
/// `BilinearFormError`) exposed on the runtime engine surface (`physical/builtin_eval.rs`,
/// `physical/mod.rs`) so external crates drive the moded bilinear builtin through the
/// governed family — both files are folded into the runtime engine-source content digest.
const GOLDEN_ENGINE_DESCRIPTOR_HASH: &str =
    "8ff063da4d6c3bf23e1203431ab7f03d526cfdeabf616342d8a33119d3181b35";

/// Golden `SessionIdentity.descriptor_hash` over the fixed input below. A drift here is a
/// deliberate session-identity contract bump (it also moves whenever the engine, program,
/// contract, or annotation framing changes — the full seven-axis fold).
const GOLDEN_SESSION_DESCRIPTOR_HASH: &str =
    "8d180b1df2f2663b1325e054a41feebb1a95bdbf8ff90376907795e13f003849";

#[test]
fn semver_engine_descriptor_hash_is_pinned() {
    let actual = EngineContract::current().descriptor_hash;
    assert_eq!(
        actual.len(),
        64,
        "descriptor hash is a 64-hex BLAKE3 address"
    );
    assert_eq!(
        actual, GOLDEN_ENGINE_DESCRIPTOR_HASH,
        "the engine descriptor drifted — bump the version and re-bless checkpoints"
    );
}

#[test]
fn semver_fixed_session_identity_descriptor_hash_is_pinned() {
    // A fixed, deterministic input: a fixed EDB, program, contract, and annotation. The
    // minted data-generation and all seven identity axes are pure functions of these, so
    // the folded descriptor_hash is a stable golden.
    let (contract, annotation) = baseline_contracts();
    let edb = edge_dataset(&[("a", "b")]);
    let session =
        ReasoningSession::open(&edb, &projection_program(), &contract, &annotation).expect("open");
    let actual = &session.identity().descriptor_hash;
    assert_eq!(
        actual.len(),
        64,
        "descriptor hash is a 64-hex BLAKE3 address"
    );
    assert_eq!(
        actual, GOLDEN_SESSION_DESCRIPTOR_HASH,
        "the fixed-input session identity drifted — a deliberate contract bump is required"
    );
}
