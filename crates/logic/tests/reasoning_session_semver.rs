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
/// The native.5 contract admits native derived inputs to the structural DL reader,
/// retains shared datasets and complete literal identity, and publishes the values
/// supporting a functional clash. Native value comparisons share one parsed
/// interpretation across DL and refutation, with world-scoped identity laws and
/// explicit unknown datatype mappings. Inferred objects remain native terms through
/// closure export; contextual modal evidence has an explicit binary field even
/// when absent. Rule bindings, incremental grounding and witness frontiers retain
/// native values; dense join slots hold local term IDs. Earlier checkpoints must
/// fail admission. Signed SCC planning also frames implicit structural builtin
/// reads under the selected operator interpretation. Producer effects now separate
/// typed read/write patterns and preserve constant-marker roles; predicate
/// completion requires every possible writer, including dynamic schema outputs.
/// Source-sensitive schedules bind a complete native value-flow summary to an
/// immutable input borrow. Shared value-flow layouts and termination admission
/// survive input-shape changes; stale or unbound inputs cannot execute the plan.
/// Ordinary joins, schema slots and witness layouts now share template ownership
/// across changing strata. Exact rule keys authorize each grouping, and no second
/// joint-stratum cache retains a released template.
/// Guarded schema laws share native datatype interpretations, refuse undefined
/// comparisons and feed their witnessed contradictions into authored rule execution.
/// Key agreement now waits for complete canonical composite-key definitions and
/// retains actual component/value evidence; malformed records withhold coverage.
/// Native maximum witnesses include their explicit qualifier, literal value
/// identity and pairwise resource inequalities in shared authored-rule execution.
/// Datatype definitions now require completed structural producers and compile into
/// world-local shared expression DAGs with retained conjunctive facets and premises.
/// Datatype refutation and coverage now share these plans and one source analysis.
/// Capacity is lazy; existential requests remain separate from universal membership,
/// and inherited contradictions retain actual native source paths.
/// Typed datatype contradictions now feed authored consumers through completed
/// definitions and positive value joins. Primitive capacities come from PurRDF;
/// the complete fragment registry is exposed without a source-corpus parse.
/// Refutation and annotation evidence now use complete native transport; the
/// shared scoped-term codec participates in both engine contract inventories.
/// Planner v15 streams native chase joins and stops conjunctive head probes at a
/// complete witness; an exhausted probe cannot establish absence or invent a
/// witness. The shared traversal participates in both engine source inventories.
/// Chase heads publish directly into the caller's candidate buffer; standalone
/// first-source and joint total-provenance winners retain their existing contracts.
/// Native refusals and shared law loaders now preserve typed diagnostic ownership.
/// Planner v16 streams native object-witness families with world/frontier identity,
/// bounded complete presence probes, and input-scoped termination admission. Exact
/// finite selector domains share native value-flow analysis; compact multiplicity
/// permits only position proofs, and borrowed selector cells avoid payload copies.
/// Planner v17 shares deterministic resource equality and local self consequences
/// with authored consumers, preserving world, native values and schema premises.
/// Equality guards reuse cardinality syntax and never arbitrarily merge a larger bound.
/// Canonical composition declarations now enter the shared program identity; the
/// engine source inventory binds their typed IR and ordered reference keys.
/// Finite indexed presentation maps and pushouts now share canonical symbol
/// transport and preserve exact evidence publications. Their independent square
/// and factorization checker, together with the native correspondence executors,
/// participates in the descriptor; earlier checkpoints cannot attest this code.
/// Authored merges now reuse original compiler-owned Formula roots and native
/// source evidence. The source grammar, shared ownership lookup and finite
/// admission boundary participate in this identity: unsupported scope, sorts
/// and default-world modal translations cannot receive a preservation witness.
/// Presentation lowering now belongs to the compiler's typed declaration catalog.
/// Complete native metadata, references and refusal identities participate in
/// program identity; execution reuses those records and their original source.
/// The shared native term codec and actual compiler readers are bound here, so
/// a pre-migration checkpoint cannot restore this execution contract.
/// Direct, cached and hydrated rule execution now share source admission: no
/// aggregate, negative head or contextual operator is silently erased. Source
/// failures retain their diagnostic without a second full-reasoner probe. The
/// operator preparation and complete session protocol source now participate
/// in the engine identity, including classification, replay and restoration.
/// Planner v19 retains typed complete-group reductions, strict producer completion,
/// native PurRDF value folds and explicit aggregate annotation/profile admission.
/// Selected empty worlds remain execution scopes; older checkpoints cannot attest
/// these semantics or the newly bound reduce and world-store source components.
/// Native builtin bindings now borrow typed RDF values. Ordinary and joint
/// numeric gaps stop before round commit; this behavior and its input decoder
/// participate in both the forward and backward engine source identities.
/// Planner v20 carries canonical typed RDF numeric instructions, shared scalar
/// preparation and complete input-variable dependencies in the numeric termination
/// proof. Public scalar materialization retains its joint admission and provenance.
/// Source-selected correspondence axis rules now share bounded native analyses,
/// retain qualitative evidence and execute with exact source/standpoint ownership.
/// The source roster includes that admission and execution code. Earlier identities
/// cannot attest these numerical claims, assumptions or evidence transport.
/// Conditional value-flow support now keeps predicate-producing source roles exact,
/// preventing unrelated rows from inventing a writer for protected source grammar.
/// The sparse operator vocabulary and its input-bound cache identity invalidate every
/// schedule, session and checkpoint minted under the earlier column-only abstraction.
/// The correspondence compiler now retains executable leg programs through its native
/// graph projection and inverse. That selected compiler dependency is part of the engine
/// descriptor, so native.5 refuses checkpoints minted before the lossless contract.
const GOLDEN_ENGINE_DESCRIPTOR_HASH: &str =
    "340fcf40049fa71504b31e32b356ce155fdb194c9c5ef8e1a50d8fa6a2ebcdaf";

/// Golden `SessionIdentity.descriptor_hash` over the fixed input below.
///
/// The identity binds seven axes: the authorized data generation, program, slice
/// provenance, reasoning contract, engine descriptor, annotation contract, and
/// certified fragment. The source contract is framed with the data-generation value,
/// so those seven axes contribute eight fields. Any change must move this pin and
/// refuse restoration of a stale checkpoint.
const GOLDEN_SESSION_DESCRIPTOR_HASH: &str =
    "b48265ccf0db1d21c32161ec5f1cd674ab065203bc358bf283fd5254b7c70048";

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
    let edb = edge_arc(&[("a", "b")]);
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
