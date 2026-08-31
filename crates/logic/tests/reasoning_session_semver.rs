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

/// Golden digest of [`EngineContract::current`].
///
/// The descriptor frames the native engine identity, backward source hash, forward
/// reasoning-contract hash, and ordered profile/decidability manifest. Any byte-level
/// change to that public runtime contract moves this pin so existing checkpoints cannot
/// claim compatibility without an explicit version decision.
/// The current value deliberately includes the source bytes after process-history comments
/// were replaced by the executable invariants those comments described and after the
/// gmeow-logic public rustdoc intra-doc links were corrected in place.
const GOLDEN_ENGINE_DESCRIPTOR_HASH: &str =
    "4e50404342bcee7729ce99f3b13e32244776545b495e9755b6b3785cbcf6c457";

/// Golden `SessionIdentity.descriptor_hash` over the fixed input below.
///
/// The identity binds seven axes: the authorized data generation, program, slice
/// provenance, reasoning contract, engine descriptor, annotation contract, and
/// certified fragment. The source contract is framed with the data-generation value,
/// so those seven axes contribute eight fields. Any change must move this pin and
/// refuse restoration of a stale checkpoint.
const GOLDEN_SESSION_DESCRIPTOR_HASH: &str =
    "4287e07f56d4cdd3e2123c9b70d963ee1693e1832142af177dbacaf186a7fd64";

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
