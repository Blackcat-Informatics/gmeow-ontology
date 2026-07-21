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
/// all move the `physical/chase.rs` / `reason/dl.rs` content digest. Re-blessed for the
/// functional-characteristic carrier migration: the foundation chase now derives
/// `functionalProperty(?P,?P)` from the canonical `logic:PropertyCharacteristicAssertion`
/// carrier (a new Datalog rule alongside the `owl:FunctionalProperty` marker rule), and
/// `reason/dl.rs` unions the carrier into the functional-clash reader — both move the folded
/// program/`reason/dl.rs` content digest. Re-blessed for the key carrier migration: `reason/dl.rs`
/// now reads keys from the canonical `logic:KeyAssertion` carrier (`logic:keyClass` +
/// `logic:keyProperty`) unioned into the key-agreement clash reader and coverage inventory
/// alongside the `owl:hasKey` list, so the datatype/single-property key survives removal of the
/// `owl:hasKey` slice declaration — moving the `reason/dl.rs` content digest folded here.
/// Re-blessed once more for the stage-2 `cargo fmt` pass over the branch-modified reasoning
/// core: reformatting `reason/dl.rs` / `physical/chase.rs` (behaviour-preserving — `reason-verify`
/// stays green) moves the raw-source content digest folded into this descriptor. Re-blessed again
/// for the merge of origin/main PR 1385: the public bilinear-form distance API (`bilinear_sqdist` /
/// `compare_sqdist` / `BilinearFormError`) on the runtime engine surface (`physical/builtin_eval.rs`,
/// `physical/mod.rs`) also folds into the runtime engine-source content digest, so the golden below
/// is the merged value (both this branch's fmt and PR 1385's bilinear API move the digest).
/// Re-blessed for the fragment-certified refutation kernel: `reason/refute.rs` is registered
/// as a new load-bearing `NATIVE_CONTRACT_COMPONENTS` engine component (the unified beyond-Horn
/// decider the `reason/dl.rs` decide path now invokes), so the native contract hash folded into
/// this descriptor moves. The kernel is inert on this input (it registers no family sub-decider
/// yet), so no reasoning verdict changes — only the source-content digest.
/// Re-blessed for Family 5 (the FIRST real refutation sub-decider): `reason/refute/datatype.rs`
/// is a new engine source module registered in `SUB_DECIDERS`, and `reason/dl.rs` now consults it
/// for datatype value-space coverage (promoting the facet / cardinality / oneOf families exactly
/// when the subsolver decides). Both move the native-contract source-content digest folded into
/// this descriptor. This DOES change reasoning verdicts on the datatype value-space fragment
/// (previously-withheld W3C-divergence cases are now soundly decided), but NOT on this fixed
/// datatype-free input, so the fixed-input session verdict is unchanged. (Value below
/// is the post-`cargo fmt` state of the Family 5 branch — the behaviour-preserving format
/// pass over `reason/refute.rs` folds into the raw source-content digest.)
/// Re-blessed for Family 2/6a/7 (the counting / arithmetic-feasibility sub-decider):
/// `reason/refute/counting.rs` is a new engine source module registered in `SUB_DECIDERS`,
/// and `reason/dl.rs` now consults it for cardinality / inverse-functional / `owl:hasSelf`
/// coverage (promoting those families and narrowing their class-definition / refutation-shape
/// withholds exactly when the sub-decider decides). Both move the native-contract
/// source-content digest folded into this descriptor. This DOES change reasoning verdicts on
/// the counting fragment (previously-withheld W3C-divergence cardinality / IFP / hasSelf cases
/// are now soundly decided), but NOT on this fixed edge-only input, so the fixed-input session
/// verdict is unchanged.
/// Re-blessed for Family 1/3/6b (+ entangled Family 4) — the bounded case-split /
/// complement / union-disjoint / malformed-list sub-decider: `reason/refute/casesplit.rs`
/// is a new engine source module registered in `SUB_DECIDERS`, and `reason/refute.rs`
/// (its `mod` + registry entry) and `reason/dl.rs` (the coverage coordination — the
/// refutation-shape withholds for complement / union / oneOf / malformed list are now
/// narrowed by `!casesplit::decides`) both change. `reason/refute.rs` and `reason/dl.rs`
/// are folded into the native contract hash, so the engine-descriptor digest moves. This
/// DOES change reasoning verdicts on the case-split fragment (previously-withheld
/// W3C-divergence complement / union-disjoint / disjointUnion / malformed-list cases are
/// now soundly decided), but NOT on this fixed edge-only input, so the fixed-input session
/// verdict is unchanged.
/// Re-blessed for Task 6b — the kernel's decidability surface as first-class content:
/// `reason/refute.rs` gains the shipped registry API (`RefutationPattern`,
/// `decided_fragments`, `retained_boundaries`) and a live production consumer
/// (`production_boundary_findings`), the blanket `#![allow(dead_code)]` is removed, and
/// `reason/dl.rs` folds a family-scoped kernel withhold into a new
/// `DlVerdict::boundary_findings`. All fold into the native contract source-content
/// digest, so the engine descriptor moves. The kernel stays inert on this fixed
/// edge-only input (its steady state is `NoDeciderEngaged`, which emits nothing), so no
/// reasoning verdict changes — only the source-content digest.
/// Re-blessed once more when the coverage-gate determinism/refusal `#[cfg(test)]` tests were
/// added to `reason/refute.rs`: `native_contract_hash()` `include_str!`s the whole file, so its
/// byte content moves the engine descriptor; no reasoning verdict changes.
/// Re-blessed once more for the G12 refutation-kernel helper consolidation: `resource_key`,
/// `world_key`, `is_rational_tower`, and `parse_rational` moved from
/// `reason/refute/{casesplit,counting,datatype}.rs` into a single canonical definition in
/// `reason/refute.rs` (folded via `include_str!` into `native_contract_hash()`), so the raw
/// source-content digest moves; this is a pure refactor and no reasoning verdict changes.
const GOLDEN_ENGINE_DESCRIPTOR_HASH: &str =
    "9438ab472fe2bf0e72ab39cc8a1c8a3a87d2ad18769c9dde54557ab3a5ad6563";

/// Golden `SessionIdentity.descriptor_hash` over the fixed input below. A drift here is a
/// deliberate session-identity contract bump (it also moves whenever the engine, program,
/// contract, or annotation framing changes — the full seven-axis fold).
/// Re-blessed for the fragment-certified refutation kernel component registration (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded axes, so
/// the fixed-input session identity moves with it even though the reasoning verdict is unchanged.
/// Re-blessed again for Family 5 (the datatype value-space sub-decider) for the same reason: the
/// native contract hash is one of the seven folded axes and moves with the new engine source, while
/// the fixed datatype-free input's reasoning verdict is unchanged. (Post-`cargo fmt` value,
/// tracking the engine-descriptor golden above.)
/// Re-blessed again for Family 2/6a/7 (the counting / arithmetic-feasibility sub-decider) for the
/// same reason: the native contract hash is one of the seven folded axes and moves with the new
/// engine source module, while the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed once more for Family 1/3/6b (+ entangled Family 4) — the case-split / complement /
/// union-disjoint / malformed-list sub-decider — for the same reason: the native contract hash
/// (folding the changed `reason/refute.rs` + `reason/dl.rs`) is one of the seven folded axes and
/// moves with the new engine source, while the fixed edge-only input's reasoning verdict is unchanged.
/// Re-blessed for Task 6b for the same reason as the engine-descriptor golden above:
/// the native contract hash is one of the seven folded identity axes and moves with the
/// changed `reason/refute.rs` + `reason/dl.rs` engine source, while the fixed edge-only
/// input's reasoning verdict is unchanged.
/// Re-blessed once more when the coverage-gate determinism/refusal `#[cfg(test)]` tests were
/// added to `reason/refute.rs`: `native_contract_hash()` `include_str!`s the whole file, so its
/// byte content (folded into the session identity axis) moves; the fixed edge-only input's
/// reasoning verdict is unchanged and the engine descriptor hash is untouched.
/// Re-blessed once more for the G12 refutation-kernel helper consolidation (see the
/// engine-descriptor golden above): the native contract hash is one of the seven folded
/// identity axes and moves with the changed `reason/refute.rs` engine source, while the
/// fixed edge-only input's reasoning verdict is unchanged.
const GOLDEN_SESSION_DESCRIPTOR_HASH: &str =
    "e22a0bbf9591833595b8c9be98c8fb4ab06f0685bb473e9e1f90cf1db1ac4f3d";

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
